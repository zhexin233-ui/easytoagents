import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type ExternalChangePlanDto } from "@/bindings/commands";
import { ExternalChangeActions } from "@/features/sync/external-change-actions";
import { errResult, okResult } from "@/test/commands-mock";
import { renderWithProviders } from "@/test/render";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

function externalPlan(
  overrides: Partial<ExternalChangePlanDto> = {},
): ExternalChangePlanDto {
  return {
    previewId: "external-plan-1",
    artifactKind: "agent",
    tool: "claude",
    projectId: null,
    status: "external_owned_change",
    targetPaths: ["/isolated/home/.claude/agents/reviewer.md"],
    observedFullHashes: ["a".repeat(64)],
    observedManagedHashes: ["b".repeat(64)],
    rowVersions: [],
    redactedDiff: {},
    canAdoptNative: true,
    adoptBlockedReason: null,
    canOverwriteCentral: true,
    overwriteBlockedReason: null,
    ...overrides,
  };
}

describe("ExternalChangeActions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("Apply 返回 stale 时只重新签发一次计划并消费新证据", async () => {
    const plan = externalPlan();
    vi.mocked(commands.prepareExternalChangePlan).mockResolvedValue(
      okResult(plan),
    );
    vi.mocked(commands.applyExternalChangePlan)
      .mockResolvedValueOnce(
        errResult({
          code: "STALE_PREVIEW",
          message: "观察已过期",
          recoverable: true,
        }),
      )
      .mockResolvedValueOnce(
        okResult({
          runId: plan.previewId,
          status: "succeeded",
          appliedTargets: 1,
          snapshotCount: 0,
        }),
      );

    renderWithProviders(
      <ExternalChangeActions
        artifactKind="agent"
        tool="claude"
        status="external_owned_change"
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "采纳原生更改" }),
    );

    await waitFor(() =>
      expect(commands.applyExternalChangePlan).toHaveBeenCalledTimes(2),
    );
    expect(commands.prepareExternalChangePlan).toHaveBeenCalledTimes(3);
    expect(commands.applyExternalChangePlan).toHaveBeenNthCalledWith(1, {
      previewId: plan.previewId,
      artifactKind: "agent",
      tool: "claude",
      projectId: null,
      action: "adopt_native",
    });
    expect(commands.applyExternalChangePlan).toHaveBeenNthCalledWith(2, {
      previewId: plan.previewId,
      artifactKind: "agent",
      tool: "claude",
      projectId: null,
      action: "adopt_native",
    });
  });
});
