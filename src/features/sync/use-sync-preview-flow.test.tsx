import { renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import type { PropsWithChildren } from "react";

import {
  type ApplyResult,
  type PreviewPlan,
  type Result,
  type SyncScopeDto,
} from "@/bindings/commands";
import {
  stableSyncScopes,
  useSyncPreviewFlow,
} from "@/features/sync/use-sync-preview-flow";
import { NotifyProvider } from "@/components/notify";
import { ProfileRpcError } from "@/lib/profile-api";
import { makePreviewPlan } from "@/test/fixtures/preview-plan";

function ok<T>(data: T): Result<T, never> {
  return { status: "ok", data };
}

function applyResult(runId: string): ApplyResult {
  return {
    runId,
    status: "succeeded",
    appliedTargets: 1,
    snapshotCount: 1,
  };
}

function wrapper({ children }: PropsWithChildren) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return (
    <QueryClientProvider client={queryClient}>
      <NotifyProvider>{children}</NotifyProvider>
    </QueryClientProvider>
  );
}

describe("useSyncPreviewFlow scope 执行器", () => {
  it("混合 global/project scope 稳定去重并将全局排在项目 scope 之前", () => {
    const scopes: SyncScopeDto[] = stableSyncScopes([
      { artifactKind: "mcp", tool: "codex", projectId: "project-b" },
      { artifactKind: "mcp", tool: "claude", projectId: "project-a" },
      { artifactKind: "mcp", tool: "codex", projectId: null },
      { artifactKind: "mcp", tool: "codex", projectId: "project-a" },
      { artifactKind: "mcp", tool: "codex", projectId: "project-a" },
      { artifactKind: "mcp", tool: "codex", projectId: null },
    ]);

    expect(scopes).toEqual([
      { artifactKind: "mcp", tool: "claude", projectId: "project-a" },
      { artifactKind: "mcp", tool: "codex", projectId: null },
      { artifactKind: "mcp", tool: "codex", projectId: "project-a" },
      { artifactKind: "mcp", tool: "codex", projectId: "project-b" },
    ]);
  });

  it("逐 scope 等待完成，project-only 失败不会阻断其它 scope", async () => {
    const events: string[] = [];
    const preview = vi.fn(
      (tool: SyncScopeDto["tool"], projectId?: string | null) => {
        events.push(`preview:${tool}:${projectId ?? "global"}`);
        if (tool === "codex" && projectId === "project-only") {
          return Promise.reject(new Error("project-only preview failed"));
        }
        return Promise.resolve(
          ok<PreviewPlan>(
            makePreviewPlan({
              previewId: `${tool}-${projectId ?? "global"}`,
            }),
          ),
        );
      },
    );
    const apply = vi.fn(
      ({
        previewId,
        tool,
        projectId,
      }: {
        previewId: string;
        tool: SyncScopeDto["tool"];
        projectId?: string | null;
      }) => {
        events.push(`apply:${tool}:${projectId ?? "global"}`);
        return Promise.resolve(ok(applyResult(previewId)));
      },
    );
    const invalidate = vi.fn(() => {
      events.push("invalidate");
      return Promise.resolve();
    });

    const { result } = renderHook(
      () =>
        useSyncPreviewFlow({
          artifactKind: "mcp",
          preview,
          apply,
          invalidate,
          messages: {
            previewFailed: "预览失败",
            applyFailed: "应用失败",
            applied: () => "已应用",
          },
        }),
      { wrapper },
    );

    const run = await result.current.requestPreview([
      { artifactKind: "mcp", tool: "codex", projectId: "project-only" },
      { artifactKind: "mcp", tool: "claude", projectId: null },
      { artifactKind: "mcp", tool: "claude", projectId: null },
      { artifactKind: "mcp", tool: "codex", projectId: "project-ok" },
    ]);

    expect(run.status).toBe("partial_failure");
    expect(run.succeeded).toEqual([
      { artifactKind: "mcp", tool: "claude", projectId: null },
      { artifactKind: "mcp", tool: "codex", projectId: "project-ok" },
    ]);
    expect(run.failures).toHaveLength(1);
    expect(run.failures[0]?.scope).toEqual({
      artifactKind: "mcp",
      tool: "codex",
      projectId: "project-only",
    });
    expect(events).toEqual([
      "preview:claude:global",
      "apply:claude:global",
      "preview:codex:project-ok",
      "apply:codex:project-ok",
      "preview:codex:project-only",
      "invalidate",
    ]);
    expect(invalidate).toHaveBeenCalledTimes(1);
  });

  it("空 scope 与空计划都是可等待的 no-op", async () => {
    const preview = vi.fn(() =>
      Promise.resolve(ok<PreviewPlan>(makePreviewPlan({ targets: [] }))),
    );
    const apply = vi.fn(() => Promise.resolve(ok(applyResult("unused"))));
    const invalidate = vi.fn(() => Promise.resolve());

    const { result } = renderHook(
      () =>
        useSyncPreviewFlow({
          artifactKind: "mcp",
          preview,
          apply,
          invalidate,
          messages: {
            previewFailed: "预览失败",
            applyFailed: "应用失败",
            applied: () => "已应用",
          },
        }),
      { wrapper },
    );

    await expect(result.current.requestPreview([])).resolves.toMatchObject({
      status: "success",
      scopes: [],
      appliedTargets: 0,
      snapshotCount: 0,
    });
    expect(preview).not.toHaveBeenCalled();
    expect(apply).not.toHaveBeenCalled();
    expect(invalidate).not.toHaveBeenCalled();

    await expect(
      result.current.requestPreview({
        artifactKind: "mcp",
        tool: "claude",
        projectId: null,
      }),
    ).resolves.toMatchObject({ status: "success", appliedTargets: 0 });
    expect(preview).toHaveBeenCalledTimes(1);
    expect(apply).not.toHaveBeenCalled();
  });

  it("STALE_PREVIEW 只重建当前 scope 一次", async () => {
    const events: string[] = [];
    let applyCount = 0;
    const preview = vi.fn(() => {
      events.push("preview");
      return Promise.resolve(
        ok<PreviewPlan>(
          makePreviewPlan({ previewId: `preview-${events.length}` }),
        ),
      );
    });
    const apply = vi.fn(({ previewId }: { previewId: string }) => {
      events.push(`apply:${previewId}`);
      applyCount += 1;
      if (applyCount === 1) {
        return Promise.reject(
          new ProfileRpcError({
            code: "STALE_PREVIEW",
            message: "预览已过期",
            recoverable: true,
          }),
        );
      }
      return Promise.resolve(ok(applyResult(previewId)));
    });

    const { result } = renderHook(
      () =>
        useSyncPreviewFlow({
          artifactKind: "mcp",
          preview,
          apply,
          invalidate: () => Promise.resolve(),
          messages: {
            previewFailed: "预览失败",
            applyFailed: "应用失败",
            applied: () => "已应用",
          },
        }),
      { wrapper },
    );

    const run = await result.current.requestPreview({
      artifactKind: "mcp",
      tool: "claude",
      projectId: null,
    });

    expect(run.status).toBe("success");
    expect(preview).toHaveBeenCalledTimes(2);
    expect(apply).toHaveBeenCalledTimes(2);
    expect(events).toEqual([
      "preview",
      "apply:preview-1",
      "preview",
      "apply:preview-3",
    ]);
  });
});
