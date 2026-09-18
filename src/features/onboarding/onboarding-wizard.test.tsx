import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type PreviewPlan,
  type PromptImportPreviewDto,
  type ProviderImportPreviewDto,
} from "@/bindings/commands";
import { OnboardingWizard } from "@/features/onboarding/onboarding-wizard";
import { renderWithProviders } from "@/test/render";
import {
  makePreviewPlan,
  makePromptProfile,
  makeProviderImportCandidate,
  makeProviderImportPreview,
  makeProviderProfile,
  makeTarget,
} from "@/test/fixtures";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const candidateId = "00000000-0000-4000-8000-000000000723";

const importPreview: ProviderImportPreviewDto = makeProviderImportPreview({
  previewId: "00000000-0000-4000-8000-000000000721",
  tool: "claude",
  targetPath: "/isolated/home/.claude/settings.json",
  candidates: [
    makeProviderImportCandidate({
      candidateId,
      suggestedName: "已发现 Claude 渠道",
      apiBaseUrl: "https://fixture.example.com",
      defaultModel: "fixture-model",
    }),
  ],
});

const noProviderImport: ProviderImportPreviewDto = makeProviderImportPreview({
  previewId: null,
  candidates: [],
});

const codexOAuthImportPreview: ProviderImportPreviewDto =
  makeProviderImportPreview({
    previewId: "00000000-0000-4000-8000-000000000726",
    tool: "codex",
    targetPath: "/isolated/home/.codex/config.toml",
    candidates: [
      makeProviderImportCandidate({
        candidateId: "00000000-0000-4000-8000-000000000727",
        suggestedName: "Codex 官方账号登录",
        authKind: "official_login",
        apiBaseUrl: "",
        apiKeyConfigured: false,
        defaultModel: "gpt-5.5",
        redactedProjection: { model: "gpt-5.5" },
      }),
    ],
  });

const promptImportPreview: PromptImportPreviewDto = {
  previewId: "00000000-0000-4000-8000-000000000725",
  tool: "claude",
  targetPath: "/isolated/home/.claude/CLAUDE.md",
  suggestedName: "已发现 Claude 提示词",
  body: "# fixture prompt",
};

const piImportPreview: ProviderImportPreviewDto = makeProviderImportPreview({
  ...importPreview,
  previewId: "00000000-0000-4000-8000-000000000729",
  tool: "pi",
  targetPath: "/isolated/home/.pi/agent/models.json",
  candidates: [
    makeProviderImportCandidate({
      candidateId: "00000000-0000-4000-8000-000000000730",
      providerId: "cc",
      suggestedName: "已发现 Pi 渠道",
    }),
  ],
});

const syncPreview: PreviewPlan = makePreviewPlan({
  previewId: "00000000-0000-4000-8000-000000000722",
  dbVersion: 1,
  targets: [
    makeTarget({
      targetId: "00000000-0000-4000-8000-000000000723",
      descriptor: {
        tool: "claude",
        artifactKind: "provider",
        scope: "global",
        projectRoot: null,
        path: "/isolated/home/.claude/settings.json",
        allowedRoot: null,
        mcpContainer: null,
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
      errorCode: null,
      git: null,
      excludeFromGit: false,
    }),
  ],
});

const promptSyncPreview: PreviewPlan = makePreviewPlan({
  ...syncPreview,
  previewId: "00000000-0000-4000-8000-000000000726",
  targets: [
    makeTarget({
      targetId: "00000000-0000-4000-8000-000000000727",
      descriptor: {
        ...syncPreview.targets[0]!.descriptor,
        artifactKind: "prompt",
        path: "/isolated/home/.claude/CLAUDE.md",
      },
    }),
  ],
});

function renderWizard(onClose = vi.fn()) {
  return {
    onClose,
    ...renderWithProviders(<OnboardingWizard open onClose={onClose} />),
  };
}

describe("OnboardingWizard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "codex"] },
    });
    vi.mocked(commands.getToolProfileStatus).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: {
          tool,
          availability: "installed",
          installationVersion: tool === "claude" ? "2.1.217" : "0.114.0",
          installationProbeDiagnostic: null,
          providerCapability: {
            state: "supported" as const,
            diagnosticCode: null,
          },
          promptCapability: {
            state: "supported" as const,
            diagnosticCode: null,
          },
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
        data: tool === "claude" ? importPreview : noProviderImport,
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
      data: { tool: "claude", importedCount: 1 },
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

  it("按检测、选择、预览、应用推进，跳过 Codex 时保持其非受管", async () => {
    renderWizard();
    expect(
      await screen.findByText(/已检测到版本 2\.1\.217，可接管的原生配置。/),
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
      items: [{ candidateId, name: "已发现 Claude 渠道" }],
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
        data: tool === "codex" ? codexOAuthImportPreview : noProviderImport,
      }),
    );

    renderWizard();

    expect(
      await screen.findByText("gpt-5.5 · 官方账号登录（不接管凭据）"),
    ).toBeInTheDocument();
    const providerChoices = screen.getAllByLabelText("导入并接管 Provider");
    const codexProviderChoice = providerChoices[1];
    if (!codexProviderChoice) throw new Error("缺少 Codex Provider 选项");
    expect(codexProviderChoice).toBeEnabled();
  });

  it("暂停按钮保留选择并调用关闭回调", async () => {
    const { onClose } = renderWizard();
    await screen.findByText(/已检测到版本 2\.1\.217，可接管的原生配置。/);
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

    await screen.findByText(/已检测到版本 2\.1\.217，可接管的原生配置。/);
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
    await screen.findByText(/已检测到版本 2\.1\.217，可接管的原生配置。/);
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
          installationProbeDiagnostic: null,
          providerCapability: {
            state: "supported" as const,
            diagnosticCode: null,
          },
          promptCapability: {
            state: "supported" as const,
            diagnosticCode: null,
          },
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
        : Promise.resolve({ status: "ok", data: noProviderImport }),
    );

    renderWizard();

    expect(await screen.findByText(/未检测到安装，请跳过。/)).toBeVisible();
    expect(screen.getAllByLabelText("导入并接管 Provider")[0]).toBeDisabled();
    expect(
      screen.getAllByText("未检测到安装，无法读取或应用原生目标。")[0],
    ).toBeVisible();
    fireEvent.click(screen.getByLabelText("跳过 Claude，保持非受管"));
    fireEvent.click(screen.getByLabelText("跳过 Codex，保持非受管"));
    expect(
      screen.getByRole("button", { name: "确认选择并生成预览" }),
    ).toBeEnabled();
  });

  it("中断后忽略已在中央 active 档案中的旧选择且不生成其预览", async () => {
    localStorage.setItem(
      "easytoagents.onboarding.selections.v1",
      JSON.stringify({
        claude: { provider: true, prompt: false, skip: false },
        codex: { provider: false, prompt: false, skip: true },
      }),
    );
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: noProviderImport,
    });
    vi.mocked(commands.listProviderProfiles).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data:
          tool === "claude"
            ? [
                makeProviderProfile({
                  id: "00000000-0000-4000-8000-000000000724",
                  tool: "claude",
                  name: "已发现 Claude 渠道",
                  apiBaseUrl: "https://fixture.example.com",
                  apiKeyConfigured: true,
                  defaultModel: "fixture-model",
                  isActive: true,
                }),
              ]
            : [],
      }),
    );

    renderWizard();
    expect(
      await screen.findByText("已有中央档案；其余项目可选择跳过。"),
    ).toBeInTheDocument();
    const claudeCard = screen.getByText("Claude").closest("fieldset");
    if (!claudeCard) throw new Error("缺少 Claude 工具卡片");
    expect(
      within(claudeCard).queryByLabelText("导入并接管 Provider"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "确认选择并生成预览" }),
    ).toBeDisabled();

    fireEvent.click(screen.getByLabelText("跳过 Claude，保持非受管"));
    fireEvent.click(screen.getByRole("button", { name: "确认选择并生成预览" }));

    expect(await screen.findByText("向导已完成")).toBeInTheDocument();
    expect(commands.confirmProviderImport).not.toHaveBeenCalled();
    expect(commands.previewProviderSync).not.toHaveBeenCalled();
  });

  it("部分接管时隐藏已接管 Provider，并只为待处理提示词生成预览", async () => {
    vi.mocked(commands.discoverPromptImport).mockResolvedValue({
      status: "ok",
      data: promptImportPreview,
    });
    vi.mocked(commands.listProviderProfiles).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data:
          tool === "claude"
            ? [makeProviderProfile({ tool: "claude", isActive: true })]
            : [],
      }),
    );
    vi.mocked(commands.confirmPromptImport).mockResolvedValue({
      status: "ok",
      data: makePromptProfile({ globalTools: ["claude"] }),
    });
    vi.mocked(commands.previewPromptSync).mockResolvedValue({
      status: "ok",
      data: promptSyncPreview,
    });

    renderWizard();

    const claudeCard = await waitFor(() => {
      const card = screen.getByText("Claude").closest("fieldset");
      if (!card) throw new Error("缺少 Claude 工具卡片");
      return card;
    });
    expect(
      within(claudeCard).queryByLabelText("导入并接管 Provider"),
    ).not.toBeInTheDocument();
    const promptChoice =
      within(claudeCard).getByLabelText("无损导入并接管全局提示词");
    expect(promptChoice).toBeEnabled();
    fireEvent.click(promptChoice);
    fireEvent.click(screen.getByLabelText("跳过 Codex，保持非受管"));
    fireEvent.click(screen.getByRole("button", { name: "确认选择并生成预览" }));

    expect(await screen.findByText("Claude · 全局提示词")).toBeInTheDocument();
    expect(commands.confirmProviderImport).not.toHaveBeenCalled();
    expect(commands.previewProviderSync).not.toHaveBeenCalled();
    expect(commands.confirmPromptImport).toHaveBeenCalledWith({
      previewId: promptImportPreview.previewId,
      name: promptImportPreview.suggestedName,
    });
    expect(commands.previewPromptSync).toHaveBeenCalledWith("claude");
  });

  it("Provider 与全局提示词均已接管时隐藏整张工具卡片", async () => {
    vi.mocked(commands.discoverPromptImport).mockResolvedValue({
      status: "ok",
      data: promptImportPreview,
    });
    vi.mocked(commands.listProviderProfiles).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data:
          tool === "claude"
            ? [makeProviderProfile({ tool: "claude", isActive: true })]
            : [],
      }),
    );
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [makePromptProfile({ globalTools: ["claude"] })],
    });

    renderWizard();

    await screen.findByLabelText("跳过 Codex，保持非受管");
    expect(
      screen.queryByText("Claude", { selector: "legend" }),
    ).not.toBeInTheDocument();
    expect(screen.getByLabelText("跳过 Codex，保持非受管")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("跳过 Codex，保持非受管"));
    fireEvent.click(screen.getByRole("button", { name: "确认选择并生成预览" }));

    await waitFor(() =>
      expect(commands.completeOnboarding).toHaveBeenCalledOnce(),
    );
    expect(commands.confirmProviderImport).not.toHaveBeenCalled();
    expect(commands.confirmPromptImport).not.toHaveBeenCalled();
    expect(commands.previewProviderSync).not.toHaveBeenCalled();
    expect(commands.previewPromptSync).not.toHaveBeenCalled();
  });

  it("启用 Pi 时首次接管检测纳入 Pi Provider", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        enabledTools: ["claude", "codex", "cursor", "zcode", "opencode", "pi"],
      },
    });
    vi.mocked(commands.discoverProviderImport).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: tool === "pi" ? piImportPreview : noProviderImport,
      }),
    );

    renderWizard();

    const piCard = await waitFor(() => {
      const card = screen.getByText("Pi").closest("fieldset");
      if (!card) throw new Error("缺少 Pi 工具卡片");
      return card;
    });
    expect(within(piCard).getByText("发现 Provider")).toBeInTheDocument();
    const piProviderChoice =
      within(piCard).getByLabelText("导入并接管 Provider");
    fireEvent.click(piProviderChoice);
    for (const tool of ["Claude", "Codex", "Cursor", "ZCode", "OpenCode"]) {
      fireEvent.click(screen.getByLabelText(`跳过 ${tool}，保持非受管`));
    }
    fireEvent.click(screen.getByRole("button", { name: "确认选择并生成预览" }));

    expect(await screen.findByText("Pi · Provider")).toBeInTheDocument();
    expect(commands.confirmProviderImport).toHaveBeenCalledWith({
      previewId: piImportPreview.previewId,
      items: [
        {
          candidateId: "00000000-0000-4000-8000-000000000730",
          name: "已发现 Pi 渠道",
        },
      ],
    });
    expect(commands.previewProviderSync).toHaveBeenCalledWith("pi");
  });

  it("启用工具变化触发新检测时忽略晚到的旧结果", async () => {
    const originalStatus = vi
      .mocked(commands.getToolProfileStatus)
      .getMockImplementation();
    if (!originalStatus) throw new Error("缺少工具状态 mock");
    let releaseFirstDetection!: () => void;
    const firstDetectionBlocked = new Promise<void>((resolve) => {
      releaseFirstDetection = resolve;
    });
    let statusCalls = 0;
    let secondDetectionStarted = false;
    vi.mocked(commands.getToolProfileStatus).mockImplementation((tool) => {
      statusCalls += 1;
      const result = originalStatus(tool);
      if (tool === "claude" && statusCalls === 1) {
        return firstDetectionBlocked.then(() => result);
      }
      return result;
    });
    vi.mocked(commands.discoverProviderImport).mockImplementation((tool) =>
      Promise.resolve({
        status: "ok",
        data: secondDetectionStarted
          ? noProviderImport
          : tool === "claude"
            ? importPreview
            : noProviderImport,
      }),
    );

    const { queryClient } = renderWizard();
    await waitFor(() =>
      expect(commands.getToolProfileStatus).toHaveBeenCalledWith("claude"),
    );
    secondDetectionStarted = true;
    queryClient.setQueryData(["settings"], {
      enabledTools: ["claude"],
    });

    expect(await screen.findByText("未发现可导入配置。")).toBeInTheDocument();
    releaseFirstDetection();
    await waitFor(() =>
      expect(screen.queryByText("发现 Provider")).not.toBeInTheDocument(),
    );
  });

  it("新检测在途时不再显示上一轮的选择证据", async () => {
    let secondDetectionStarted = false;
    let releaseSecondDetection!: () => void;
    const secondDetectionBlocked = new Promise<void>((resolve) => {
      releaseSecondDetection = resolve;
    });
    vi.mocked(commands.discoverProviderImport).mockImplementation((tool) => {
      if (secondDetectionStarted) {
        return secondDetectionBlocked.then(() => ({
          status: "ok" as const,
          data: noProviderImport,
        }));
      }
      return Promise.resolve({
        status: "ok" as const,
        data: tool === "claude" ? importPreview : noProviderImport,
      });
    });

    const { queryClient } = renderWizard();
    expect(await screen.findByText("发现 Provider")).toBeInTheDocument();

    secondDetectionStarted = true;
    queryClient.setQueryData(["settings"], {
      enabledTools: ["claude"],
    });
    expect(
      await screen.findByText("正在只读检测各工具的 Provider 与全局提示词…"),
    ).toBeVisible();
    expect(screen.queryByText("发现 Provider")).not.toBeInTheDocument();

    releaseSecondDetection();
    expect(await screen.findByText("未发现可导入配置。")).toBeInTheDocument();
  });

  it("无可导入 Provider 且无 active 档案时显示复选框禁用原因", async () => {
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: noProviderImport,
    });

    renderWizard();

    expect(
      (await screen.findAllByText("未发现可导入的 Provider。"))[0],
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
        name: promptImportPreview.suggestedName,
        body: promptImportPreview.body,
        globalTools: ["claude"],
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
    await screen.findByText(/已检测到版本 2\.1\.217，可接管的原生配置。/);
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
      "向导操作未完成提示词应用失败 请检查权限并从恢复点恢复后重试。",
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
