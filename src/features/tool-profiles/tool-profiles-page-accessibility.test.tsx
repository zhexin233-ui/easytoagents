import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type PreviewPlan,
  type ProviderProfileDto,
} from "@/bindings/commands";
import { ToolProfilesPage } from "@/features/tool-profiles/tool-profiles-page";
import { renderWithProviders } from "@/test/render";
import { makeProviderProfile } from "@/test/fixtures/dtos";
import { makePreviewPlan, makeTarget } from "@/test/fixtures/preview-plan";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const provider: ProviderProfileDto = makeProviderProfile({
  id: "00000000-0000-4000-8000-000000000401",
  tool: "claude",
  name: "主渠道",
  apiBaseUrl: "https://provider.example.com/v1",
  apiKeyConfigured: true,
  defaultModel: "claude-fixture",
  options: {
    credentialEnvKey: "ANTHROPIC_API_KEY",
    extraEnv: {},
    providerId: null,
    wireApi: null,
    zcodeKind: null,
    opencodeNpm: null,
    opencodeApi: null,
  },
  isActive: false,
  rowVersion: 2,
});

const preview: PreviewPlan = makePreviewPlan({
  previewId: "00000000-0000-4000-8000-000000000499",
  dbVersion: 4,
  targets: [
    makeTarget({
      targetId: "00000000-0000-4000-8000-000000000498",
      descriptor: {
        tool: "claude",
        artifactKind: "provider",
        scope: "global",
        projectRoot: null,
        path: "/isolated/home/.claude/settings.json",
        format: "json",
        managedSelectorRoots: ["env"],
        sensitiveSelectors: ["env"],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "reject",
      },
      ownership: { kind: "selectors", paths: [["env", "ANTHROPIC_API_KEY"]] },
      changeKind: "update",
      status: "in_sync",
      currentFullHash: "a".repeat(64),
      currentManagedHash: "b".repeat(64),
      desiredManagedHash: "c".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: {
        before: { env: { ANTHROPIC_API_KEY: "[REDACTED]" } },
        after: { env: { ANTHROPIC_API_KEY: "[REDACTED]" } },
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

function renderPage(tool: ProviderProfileDto["tool"] = "claude") {
  return renderWithProviders(<ToolProfilesPage tool={tool} />, {
    route: `/${tool}`,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: { applyMode: "preview_confirm", enabledTools: ["claude", "codex"] },
  });
  vi.mocked(commands.listProviderProfiles).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.getToolProfileStatus).mockResolvedValue({
    status: "ok",
    data: {
      tool: "claude",
      availability: "installed",
      installationVersion: "2.1.217",
      installationProbeDiagnostic: null,
      providerCapability: { state: "supported", diagnosticCode: null },
      promptCapability: { state: "supported", diagnosticCode: null },
      providerTargetPath: "/isolated/home/.claude/settings.json",
      promptTargetPath: "/isolated/home/.claude/CLAUDE.md",
      promptOverride: "not_applicable",
      providerPolicy: "allowed",
      newSessionNotice: "新会话生效",
      bearerTokenWarning: null,
    },
  });
  vi.mocked(commands.discoverProviderImport).mockResolvedValue({
    status: "ok",
    data: null,
  });
  vi.mocked(commands.previewProviderSync).mockResolvedValue({
    status: "ok",
    data: preview,
  });
  vi.mocked(commands.setActiveProviderProfile).mockResolvedValue({
    status: "ok",
    data: provider,
  });
  vi.mocked(commands.applyProfilePreview).mockResolvedValue({
    status: "ok",
    data: {
      runId: preview.previewId,
      status: "succeeded",
      appliedTargets: 1,
      snapshotCount: 1,
    },
  });
});

describe("ToolProfilesPage", () => {
  it("以可访问状态展示加载、空列表与未知宿主管理证据", async () => {
    vi.mocked(commands.listProviderProfiles).mockReturnValue(
      new Promise<never>(() => {}),
    );
    vi.mocked(commands.getToolProfileStatus).mockResolvedValue({
      status: "ok",
      data: {
        tool: "claude",
        availability: "unsupported",
        installationVersion: null,
        installationProbeDiagnostic: null,
        providerCapability: { state: "supported", diagnosticCode: null },
        promptCapability: { state: "supported", diagnosticCode: null },
        providerTargetPath: "/isolated/home/.claude/settings.json",
        promptTargetPath: "/isolated/home/.claude/CLAUDE.md",
        promptOverride: "not_applicable",
        providerPolicy: "unknown",
        newSessionNotice: "新会话生效",
        bearerTokenWarning: null,
      },
    });
    renderPage();

    expect(await screen.findByText("正在加载渠道档案…")).toHaveAttribute(
      "role",
      "status",
    );
    expect(
      await screen.findByText(/无法确认 Claude Provider 是否由宿主管理/),
    ).toBeVisible();
    expect(await screen.findByText(/安装探针未能安全确认版本/)).toBeVisible();
  });
});
