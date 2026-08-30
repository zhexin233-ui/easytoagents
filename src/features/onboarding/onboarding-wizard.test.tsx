/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type PreviewPlan,
  type PromptImportPreviewDto,
  type ProviderImportPreviewDto,
} from "@/bindings/commands";
import { OnboardingWizard } from "@/features/onboarding/onboarding-wizard";

vi.mock("@/bindings/commands", () => ({
  commands: {
    getToolProfileStatus: vi.fn(),
    discoverProviderImport: vi.fn(),
    discoverPromptImport: vi.fn(),
    listProviderProfiles: vi.fn(),
    listPromptProfiles: vi.fn(),
    confirmProviderImport: vi.fn(),
    confirmPromptImport: vi.fn(),
    previewProviderSync: vi.fn(),
    previewPromptSync: vi.fn(),
    applyProfilePreview: vi.fn(),
    completeOnboarding: vi.fn(),
  },
}));

const importPreview: ProviderImportPreviewDto = {
  previewId: "00000000-0000-4000-8000-000000000721",
  tool: "claude",
  targetPath: "/isolated/home/.claude/settings.json",
  suggestedName: "已发现 Claude 渠道",
  apiBaseUrl: "https://fixture.example.com",
  apiKeyConfigured: true,
  defaultModel: "fixture-model",
  redactedProjection: { env: "[REDACTED]" },
};

const codexOAuthImportPreview: ProviderImportPreviewDto = {
  previewId: "00000000-0000-4000-8000-000000000726",
  tool: "codex",
  targetPath: "/isolated/home/.codex/config.toml",
  suggestedName: "Codex OAuth 登录",
  apiBaseUrl: "https://api.openai.com/v1",
  apiKeyConfigured: false,
  defaultModel: "gpt-5.5",
  redactedProjection: { model: "gpt-5.5" },
};

const promptImportPreview: PromptImportPreviewDto = {
  previewId: "00000000-0000-4000-8000-000000000725",
  tool: "claude",
  targetPath: "/isolated/home/.claude/CLAUDE.md",
  suggestedName: "已发现 Claude 提示词",
  body: "# fixture prompt",
};

const syncPreview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000722",
  scope: "global",
  projectId: null,
  dbVersion: 1,
  warningCodes: [],
  targets: [
    {
      targetId: "00000000-0000-4000-8000-000000000723",
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
      redactedDiff: { before: "[REDACTED]", after: "[REDACTED]" },
      warningCodes: [],
      baselineMismatchedItems: [],
      readoptAvailable: false,
      errorCode: null,
      git: null,
      excludeFromGit: false,
    },
  ],
};

const promptSyncPreview: PreviewPlan = {
  ...syncPreview,
  previewId: "00000000-0000-4000-8000-000000000726",
  targets: syncPreview.targets.map((target) => ({
    ...target,
    targetId: "00000000-0000-4000-8000-000000000727",
    descriptor: {
      ...target.descriptor,
      artifactKind: "prompt",
      path: "/isolated/home/.claude/CLAUDE.md",
    },
  })),
};

function renderWizard(onClose = vi.fn()) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return {
    onClose,
    ...render(
      <QueryClientProvider client={client}>
        <OnboardingWizard open onClose={onClose} />
      </QueryClientProvider>,
    ),
  };
}

describe("OnboardingWizard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(commands.getToolProfileStatus).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: {
          tool,
          availability: "installed",
          installationVersion: tool === "claude" ? "2.1.217" : "0.114.0",
          providerTargetPath:
            tool === "claude"
              ? "/isolated/home/.claude/settings.json"
              : "/isolated/home/.codex/config.toml",
          promptTargetPath:
            tool === "claude"
              ? "/isolated/home/.claude/CLAUDE.md"
              : "/isolated/home/.codex/AGENTS.md",
          promptOverride: "not_applicable",
          providerPolicy: "allowed",
          newSessionNotice: "新会话生效",
          bearerTokenWarning: tool === "codex" ? "明文令牌警告" : null,
        },
      }),
    );
    vi.mocked(commands.discoverProviderImport).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: tool === "claude" ? importPreview : null,
      }),
    );
    vi.mocked(commands.discoverPromptImport).mockResolvedValue({
      status: "ok",
      data: null,
    });
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [],
    });
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [],
    });
    vi.mocked(commands.completeOnboarding).mockResolvedValue({
      status: "ok",
      data: { completed: true },
    });
    vi.mocked(commands.confirmProviderImport).mockResolvedValue({
      status: "ok",
      data: {
        id: "00000000-0000-4000-8000-000000000724",
        tool: "claude",
        name: importPreview.suggestedName,
        apiBaseUrl: importPreview.apiBaseUrl,
        apiKeyConfigured: true,
        defaultModel: importPreview.defaultModel,
        options: {
          credentialEnvKey: "ANTHROPIC_API_KEY",
          extraEnv: {},
          providerId: null,
          wireApi: null,
        },
        isActive: true,
        rowVersion: 1,
      },
    });
    vi.mocked(commands.previewProviderSync).mockResolvedValue({
      status: "ok",
      data: syncPreview,
    });
    vi.mocked(commands.applyProfilePreview).mockResolvedValue({
      status: "ok",
      data: {
        runId: "run-1",
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
  });

  afterEach(cleanup);

  it("按检测、选择、预览、应用推进，跳过 Codex 时保持其非受管", async () => {
    renderWizard();
    expect(
      await screen.findByText(/已安全检测到版本 2\.1\.217，可接管的原生配置。/),
    ).toBeInTheDocument();
    const providerChoices = screen.getAllByLabelText("导入并接管 Provider");
    const claudeProviderChoice = providerChoices[0];
    if (!claudeProviderChoice) throw new Error("缺少 Claude Provider 选项");
    fireEvent.click(claudeProviderChoice);
    fireEvent.click(screen.getByLabelText("跳过 Codex，保持非受管"));
    fireEvent.click(screen.getByRole("button", { name: "确认选择并生成预览" }));

    expect(await screen.findByText("Claude · Provider")).toBeInTheDocument();
    expect(commands.confirmProviderImport).toHaveBeenCalledWith({
      previewId: importPreview.previewId,
      name: importPreview.suggestedName,
    });
    expect(commands.previewProviderSync).toHaveBeenCalledWith("claude");
    expect(commands.previewProviderSync).not.toHaveBeenCalledWith("codex");

    fireEvent.click(screen.getByRole("button", { name: "应用全部预览" }));
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: syncPreview.previewId,
        tool: "claude",
        artifactKind: "provider",
      }),
    );
    expect(await screen.findByText("向导已完成")).toBeInTheDocument();
  });

  it("Codex OAuth Provider 预览显示登录凭据来源", async () => {
    vi.mocked(commands.discoverProviderImport).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: tool === "codex" ? codexOAuthImportPreview : null,
      }),
    );

    renderWizard();

    expect(
      await screen.findByText("gpt-5.5 · 使用 Codex OAuth 登录"),
    ).toBeInTheDocument();
    const providerChoices = screen.getAllByLabelText("导入并接管 Provider");
    const codexProviderChoice = providerChoices[1];
    if (!codexProviderChoice) throw new Error("缺少 Codex Provider 选项");
    expect(codexProviderChoice).toBeEnabled();
  });

  it("暂停按钮保留选择并调用关闭回调", async () => {
    const { onClose } = renderWizard();
    await screen.findByText(/已安全检测到版本 2\.1\.217，可接管的原生配置。/);
    fireEvent.click(screen.getByRole("button", { name: "暂停向导" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("已持久化跳过时仍可直接选择可导入 Provider 并自动取消跳过", async () => {
    localStorage.setItem(
      "easytoagents.onboarding.selections.v1",
      JSON.stringify({
        claude: { provider: false, prompt: false, skip: true },
        codex: { provider: false, prompt: false, skip: true },
      }),
    );
    renderWizard();

    await screen.findByText(/已安全检测到版本 2\.1\.217，可接管的原生配置。/);
    const providerChoice = screen.getAllByLabelText("导入并接管 Provider")[0];
    if (!providerChoice) throw new Error("缺少 Claude Provider 选项");
    expect(providerChoice).toBeEnabled();
    expect(screen.getByLabelText("跳过 Claude，保持非受管")).toBeChecked();

    fireEvent.click(providerChoice);

    expect(providerChoice).toBeChecked();
    expect(screen.getByLabelText("跳过 Claude，保持非受管")).not.toBeChecked();
  });

  it("要求每个工具明确选择，并持久化全跳过完成状态", async () => {
    renderWizard();
    await screen.findByText(/已安全检测到版本 2\.1\.217，可接管的原生配置。/);
    const prepare = screen.getByRole("button", {
      name: "确认选择并生成预览",
    });
    expect(prepare).toBeDisabled();
    fireEvent.click(screen.getByLabelText("跳过 Claude，保持非受管"));
    fireEvent.click(screen.getByLabelText("跳过 Codex，保持非受管"));
    expect(prepare).toBeEnabled();
    fireEvent.click(prepare);

    await waitFor(() =>
      expect(commands.completeOnboarding).toHaveBeenCalledOnce(),
    );
    expect(commands.confirmProviderImport).not.toHaveBeenCalled();
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
    expect(await screen.findByText("向导已完成")).toBeInTheDocument();
  });

  it("将未安装工具显示为独立受阻状态且只允许显式跳过", async () => {
    vi.mocked(commands.getToolProfileStatus).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: {
          tool,
          availability: tool === "claude" ? "unavailable" : "installed",
          installationVersion: tool === "claude" ? null : "0.114.0",
          providerTargetPath:
            tool === "claude"
              ? "/isolated/home/.claude/settings.json"
              : "/isolated/home/.codex/config.toml",
          promptTargetPath:
            tool === "claude"
              ? "/isolated/home/.claude/CLAUDE.md"
              : "/isolated/home/.codex/AGENTS.md",
          promptOverride: "not_applicable",
          providerPolicy: "allowed",
          newSessionNotice: "新会话生效",
          bearerTokenWarning: null,
        },
      }),
    );
    vi.mocked(commands.discoverProviderImport).mockImplementation((tool) =>
      tool === "claude"
        ? Promise.resolve({
            status: "error",
            error: {
              code: "NOT_FOUND",
              message: "未找到目标资源",
              details: { resource: "toolInstallation", path: "claude" },
              recoverable: true,
              action: "rescan",
            },
          })
        : Promise.resolve({ status: "ok", data: null }),
    );

    renderWizard();

    expect(
      await screen.findByText(/未检测到工具安装；原生目标不会被读取或应用/),
    ).toBeVisible();
    expect(screen.getAllByLabelText("导入并接管 Provider")[0]).toBeDisabled();
    expect(
      screen.getAllByText("未检测到工具安装，无法读取或应用原生目标。")[0],
    ).toBeVisible();
    fireEvent.click(screen.getByLabelText("跳过 Claude，保持非受管"));
    fireEvent.click(screen.getByLabelText("跳过 Codex，保持非受管"));
    expect(
      screen.getByRole("button", { name: "确认选择并生成预览" }),
    ).toBeEnabled();
  });

  it("中断后从中央 active 档案重新生成持久化预览而不重复导入", async () => {
    localStorage.setItem(
      "easytoagents.onboarding.selections.v1",
      JSON.stringify({
        claude: { provider: true, prompt: false, skip: false },
        codex: { provider: false, prompt: false, skip: true },
      }),
    );
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: null,
    });
    vi.mocked(commands.listProviderProfiles).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data:
          tool === "claude"
            ? [
                {
                  id: "00000000-0000-4000-8000-000000000724",
                  tool: "claude",
                  name: importPreview.suggestedName,
                  apiBaseUrl: importPreview.apiBaseUrl,
                  apiKeyConfigured: true,
                  defaultModel: importPreview.defaultModel,
                  options: {
                    credentialEnvKey: "ANTHROPIC_API_KEY",
                    extraEnv: {},
                    providerId: null,
                    wireApi: null,
                  },
                  isActive: true,
                  rowVersion: 1,
                },
              ]
            : [],
      }),
    );

    renderWizard();
    expect(
      await screen.findByText("已存在中央档案，可继续生成新的持久化同步预览。"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认选择并生成预览" }));

    expect(await screen.findByText("Claude · Provider")).toBeInTheDocument();
    expect(commands.confirmProviderImport).not.toHaveBeenCalled();
    expect(commands.previewProviderSync).toHaveBeenCalledWith("claude");
  });

  it("无可导入 Provider 且无 active 档案时显示复选框禁用原因", async () => {
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: null,
    });

    renderWizard();

    expect(
      (
        await screen.findAllByText(
          "未发现可导入 Provider，也没有生效的中央 Provider 档案。",
        )
      )[0],
    ).toBeVisible();
    expect(screen.getAllByLabelText("导入并接管 Provider")[0]).toBeDisabled();
  });

  it("多份预览部分成功后重试时只消费剩余预览", async () => {
    vi.mocked(commands.discoverPromptImport).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: tool === "claude" ? promptImportPreview : null,
      }),
    );
    vi.mocked(commands.confirmPromptImport).mockResolvedValue({
      status: "ok",
      data: {
        id: "00000000-0000-4000-8000-000000000728",
        tool: "claude",
        name: promptImportPreview.suggestedName,
        body: promptImportPreview.body,
        isActive: true,
        importedFromPath: promptImportPreview.targetPath,
        rowVersion: 1,
      },
    });
    vi.mocked(commands.previewPromptSync).mockResolvedValue({
      status: "ok",
      data: promptSyncPreview,
    });
    vi.mocked(commands.applyProfilePreview)
      .mockResolvedValueOnce({
        status: "ok",
        data: {
          runId: "provider-run",
          status: "succeeded",
          appliedTargets: 1,
          snapshotCount: 1,
        },
      })
      .mockResolvedValueOnce({
        status: "error",
        error: {
          code: "ATOMIC_WRITE_FAILED",
          message: "提示词应用失败",
          recoverable: true,
          action: "rescan",
        },
      })
      .mockResolvedValueOnce({
        status: "ok",
        data: {
          runId: "prompt-run",
          status: "succeeded",
          appliedTargets: 1,
          snapshotCount: 1,
        },
      });

    renderWizard();
    await screen.findByText(/已安全检测到版本 2\.1\.217，可接管的原生配置。/);
    const providerChoice = screen.getAllByLabelText("导入并接管 Provider")[0];
    const promptChoice =
      screen.getAllByLabelText("无损导入并接管全局提示词")[0];
    if (!providerChoice || !promptChoice) throw new Error("缺少 Claude 选项");
    fireEvent.click(providerChoice);
    fireEvent.click(promptChoice);
    fireEvent.click(screen.getByLabelText("跳过 Codex，保持非受管"));
    fireEvent.click(screen.getByRole("button", { name: "确认选择并生成预览" }));
    await screen.findByText("Claude · Provider");

    fireEvent.click(screen.getByRole("button", { name: "应用全部预览" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "ATOMIC_WRITE_FAILED：提示词应用失败",
    );
    expect(commands.applyProfilePreview).toHaveBeenNthCalledWith(1, {
      previewId: syncPreview.previewId,
      tool: "claude",
      artifactKind: "provider",
    });
    expect(commands.applyProfilePreview).toHaveBeenNthCalledWith(2, {
      previewId: promptSyncPreview.previewId,
      tool: "claude",
      artifactKind: "prompt",
    });
    expect(
      screen.getByRole("button", { name: "已有应用，不能返回选择" }),
    ).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "应用全部预览" }));
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledTimes(3),
    );
    expect(commands.applyProfilePreview).toHaveBeenNthCalledWith(3, {
      previewId: promptSyncPreview.previewId,
      tool: "claude",
      artifactKind: "prompt",
    });
    expect(await screen.findByText("向导已完成")).toBeInTheDocument();
  });
});
