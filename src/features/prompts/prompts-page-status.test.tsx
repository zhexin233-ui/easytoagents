import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type PreviewPlan,
  type PromptOverrideState,
  type PromptProfileDto,
  type Tool,
} from "@/bindings/commands";
import { PromptsPage } from "@/features/prompts/prompts-page";
import { centralListLayoutStorageKeys } from "@/components/use-persisted-central-list-layout";
import { renderWithProviders } from "@/test/render";
import { makePromptProfile } from "@/test/fixtures/dtos";
import { makePreviewPlan, makeTarget } from "@/test/fixtures/preview-plan";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const promptProfile: PromptProfileDto = makePromptProfile({
  id: "00000000-0000-4000-8000-000000000402",
  name: "默认提示词",
  body: "# 原始规则",
  globalTools: [],
  importedFromPath: null,
  rowVersion: 3,
});

const promptPreview: PreviewPlan = makePreviewPlan({
  previewId: "00000000-0000-4000-8000-000000000499",
  dbVersion: 4,
  targets: [
    makeTarget({
      targetId: "00000000-0000-4000-8000-000000000497",
      descriptor: {
        tool: "claude",
        artifactKind: "prompt",
        scope: "global",
        projectRoot: null,
        path: "/isolated/home/.claude/CLAUDE.md",
        format: "markdown",
        managedSelectorRoots: ["$document"],
        sensitiveSelectors: [],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "reject",
      },
      ownership: { kind: "whole_document" },
      changeKind: "update",
      status: "in_sync",
      currentFullHash: "a".repeat(64),
      currentManagedHash: "b".repeat(64),
      desiredManagedHash: "c".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: {
        before: "# 原始规则",
        after: "# 新规则",
      },
      warningCodes: [],
      baselineMismatchedItems: [],
      readoptAvailable: false,
      errorCode: null,
      git: null,
      excludeFromGit: false,
    }),
  ],
});

function renderPromptsPage() {
  return renderWithProviders(<PromptsPage />);
}

function promptSection(): HTMLElement {
  const section = screen
    .getByRole("heading", { name: "中央列表" })
    .closest("section");
  if (!section) {
    throw new Error("未找到 中央列表 区域");
  }
  return section;
}

function toolStatus(
  tool: Tool,
  overrides: Partial<{ promptOverride: PromptOverrideState }> = {},
) {
  return {
    tool,
    availability: "installed" as const,
    installationVersion: "2.1.217",
    installationProbeDiagnostic: null,
    providerCapability: { state: "supported" as const, diagnosticCode: null },
    promptCapability: { state: "supported" as const, diagnosticCode: null },
    providerTargetPath:
      tool === "claude"
        ? "/isolated/home/.claude/settings.json"
        : "/isolated/home/.codex/config.toml",
    promptTargetPath:
      tool === "claude"
        ? "/isolated/home/.claude/CLAUDE.md"
        : "/isolated/home/.codex/AGENTS.md",
    promptOverride: overrides.promptOverride ?? "not_applicable",
    providerPolicy: "allowed" as const,
    newSessionNotice: "新会话生效",
    bearerTokenWarning: null,
  };
}

beforeEach(() => {
  window.localStorage.clear();
  vi.mocked(commands.getToolProfileStatus).mockImplementation((tool) =>
    Promise.resolve({ status: "ok", data: toolStatus(tool) }),
  );
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: { applyMode: "preview_confirm", enabledTools: ["claude", "codex"] },
  });
  vi.mocked(commands.listPromptProfiles).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.setGlobalPromptAssignment).mockResolvedValue({
    status: "ok",
    data: promptProfile,
  });
  vi.mocked(commands.previewPromptSync).mockResolvedValue({
    status: "ok",
    data: promptPreview,
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("PromptsPage", () => {
  it("直接应用模式下删除已分配提示词自动同步清理并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    const assignedProfile: PromptProfileDto = makePromptProfile({
      ...promptProfile,
      globalTools: ["claude"],
    });
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [assignedProfile],
    });
    vi.mocked(commands.deletePromptProfile).mockResolvedValue({
      status: "ok",
      data: { id: assignedProfile.id, deleted: true },
    });
    vi.mocked(commands.applyProfilePreview).mockResolvedValue({
      status: "ok",
      data: {
        runId: promptPreview.previewId,
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
    const confirmSpy = vi.spyOn(globalThis, "confirm").mockReturnValue(true);
    renderPromptsPage();

    fireEvent.click(await screen.findByRole("button", { name: "删除" }));
    await waitFor(() =>
      expect(commands.previewPromptSync).toHaveBeenCalledWith("claude"),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: promptPreview.previewId,
        tool: "claude",
        artifactKind: "prompt",
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(
      await screen.findByText("已应用 1 个目标，可从快照恢复。"),
    ).toBeVisible();
    confirmSpy.mockRestore();
  });

  it("每工具至多一份生效：已启用图标呈选中态，启用新档案走替换语义", async () => {
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [
        makePromptProfile({
          ...promptProfile,
          name: "生效档案",
          globalTools: ["claude"],
        }),
        {
          ...promptProfile,
          id: "00000000-0000-4000-8000-000000000403",
          name: "备用档案",
          globalTools: [],
        },
      ],
    });
    renderPromptsPage();
    const section = promptSection();

    const enabledGroup = await within(section).findByRole("group", {
      name: "生效档案 全局启用",
    });
    expect(
      within(enabledGroup).getByRole("button", { name: "Claude 全局已分配" }),
    ).toHaveAttribute("aria-pressed", "true");
    // 再次点击已启用的图标即停用该工具生效。
    fireEvent.click(
      within(enabledGroup).getByRole("button", { name: "Claude 全局已分配" }),
    );
    await waitFor(() =>
      expect(commands.setGlobalPromptAssignment).toHaveBeenCalledWith({
        tool: "claude",
        promptProfileId: promptProfile.id,
        assigned: false,
        rowVersion: promptProfile.rowVersion,
      }),
    );

    const standbyGroup = within(section).getByRole("group", {
      name: "备用档案 全局启用",
    });
    fireEvent.click(
      within(standbyGroup).getByRole("button", { name: "Claude 全局未分配" }),
    );
    await waitFor(() =>
      expect(commands.setGlobalPromptAssignment).toHaveBeenCalledWith({
        tool: "claude",
        promptProfileId: "00000000-0000-4000-8000-000000000403",
        assigned: true,
        rowVersion: promptProfile.rowVersion,
      }),
    );
  });

  it("以可访问状态展示加载、空列表与每工具遮蔽证据", async () => {
    vi.mocked(commands.listPromptProfiles).mockReturnValue(
      new Promise<never>(() => {}),
    );
    vi.mocked(commands.getToolProfileStatus).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok" as const,
        data: toolStatus(tool, { promptOverride: "unknown" }),
      }),
    );
    renderPromptsPage();

    expect(await screen.findByText("正在加载提示词档案…")).toHaveAttribute(
      "role",
      "status",
    );
    const notices = await screen.findAllByText(/新会话生效/);
    expect(notices.length).toBe(2);
    const warnings =
      await screen.findAllByText(/无法安全确认 Codex 指令遮蔽状态/);
    expect(warnings.length).toBe(2);
  });

  it("中央列表布局独立持久化并在重新挂载后恢复，非法值回退为单列", () => {
    localStorage.setItem(centralListLayoutStorageKeys.mcp, "grid");

    const firstRender = renderPromptsPage();
    expect(screen.getByRole("button", { name: "单列显示" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    fireEvent.click(screen.getByRole("button", { name: "三列网格显示" }));
    expect(localStorage.getItem(centralListLayoutStorageKeys.prompts)).toBe(
      "grid",
    );
    expect(localStorage.getItem(centralListLayoutStorageKeys.mcp)).toBe("grid");
    firstRender.unmount();

    const secondRender = renderPromptsPage();
    expect(
      screen.getByRole("button", { name: "三列网格显示" }),
    ).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "单列显示" }));
    expect(localStorage.getItem(centralListLayoutStorageKeys.prompts)).toBe(
      "list",
    );
    secondRender.unmount();

    localStorage.setItem(
      centralListLayoutStorageKeys.prompts,
      "invalid-layout",
    );
    renderPromptsPage();
    expect(screen.getByRole("button", { name: "单列显示" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
});
