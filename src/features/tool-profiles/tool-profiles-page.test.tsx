import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { Suspense } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type ExternalChangePlanDto,
  type PreviewPlan,
  type ProviderProfileDto,
} from "@/bindings/commands";
import { TOOL_PROFILE_ROUTES } from "@/app/tool-profile-routes";
import { Link, Route, Routes } from "react-router-dom";
import { ToolProfilesPage } from "@/features/tool-profiles/tool-profiles-page";
import { renderWithProviders } from "@/test/render";
import {
  globalSyncScope,
  makeOfficialLoginStatus,
  makeProviderImportCandidate,
  makeProviderImportPreview,
  makeProviderProfile,
  withAffectedSyncScopes,
} from "@/test/fixtures/dtos";
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
    authKind: "api_key",
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

const codexOAuthProvider: ProviderProfileDto = makeProviderProfile({
  id: "00000000-0000-4000-8000-000000000501",
  tool: "codex",
  name: "Codex OAuth 登录",
  apiBaseUrl: "",
  apiKeyConfigured: false,
  defaultModel: "gpt-5.5",
  options: {
    authKind: "official_login",
    credentialEnvKey: null,
    extraEnv: {},
    providerId: "openai",
    wireApi: null,
    zcodeKind: null,
    opencodeNpm: null,
    opencodeApi: null,
  },
  isActive: true,
  rowVersion: 4,
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
      redactedDiff: {
        before: { env: { ANTHROPIC_API_KEY: "[REDACTED]" } },
        after: { env: { ANTHROPIC_API_KEY: "[REDACTED]" } },
      },
      warningCodes: [],
      baselineMismatchedItems: [],
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

function sectionByHeading(name: string): HTMLElement {
  const section = screen.getByRole("heading", { name }).closest("section");
  if (!section) {
    throw new Error(`未找到 ${name} 区域`);
  }
  return section;
}

/// 懒加载页面挂载是异步的，切换页签的场景用这个变体等待区域出现。
async function sectionByHeadingAsync(name: string): Promise<HTMLElement> {
  const heading = await screen.findByRole("heading", { name });
  const section = heading.closest("section");
  if (!section) {
    throw new Error(`未找到 ${name} 区域`);
  }
  return section;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function fillProfileForm(dialog: HTMLElement) {
  fireEvent.change(within(dialog).getByLabelText("名称"), {
    target: { value: "新草稿" },
  });
  fireEvent.change(within(dialog).getByLabelText("API 地址"), {
    target: { value: "https://draft.example.com/v1" },
  });
  fireEvent.change(within(dialog).getByLabelText("API Key（默认遮罩）"), {
    target: { value: "draft-secret" },
  });
  fireEvent.change(within(dialog).getByLabelText("默认模型"), {
    target: { value: "draft-model" },
  });
}

/// 与真实 router.tsx 共用同一份工具路由配置（含 key），验证切换页签重置。
/// AppShell 在真实路由里也以 Suspense 包裹懒加载页面。
function ToolRouteHarness() {
  return (
    <>
      <nav aria-label="工具切换">
        {TOOL_PROFILE_ROUTES.map(({ path }) => (
          <Link key={path} to={`/${path}`}>
            {path}
          </Link>
        ))}
      </nav>
      <Suspense fallback={<p role="status">正在加载页面…</p>}>
        <Routes>
          {TOOL_PROFILE_ROUTES.map(({ path, element }) => (
            <Route key={path} path={`/${path}`} element={element} />
          ))}
        </Routes>
      </Suspense>
    </>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: { enabledTools: ["claude", "codex"] },
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
  vi.mocked(commands.listGlobalProfileTargetStatuses).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.discoverProviderImport).mockResolvedValue({
    status: "ok",
    data: makeProviderImportPreview({ previewId: null, candidates: [] }),
  });
  vi.mocked(commands.getOfficialLoginStatus).mockImplementation((tool) =>
    Promise.resolve({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool,
        manualCommand: tool === "codex" ? "codex login" : "claude auth login",
      }),
    }),
  );
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
  it("Provider 外部变化状态卡提供基于计划的采纳动作", async () => {
    const targetPath = "/isolated/home/.claude/settings.json";
    const plan: ExternalChangePlanDto = {
      previewId: "00000000-0000-4000-8000-000000000477",
      artifactKind: "provider",
      tool: "claude",
      projectId: null,
      status: "external_owned_change",
      targetPaths: [targetPath],
      observedFullHashes: ["a".repeat(64)],
      observedManagedHashes: ["b".repeat(64)],
      rowVersions: [],
      redactedDiff: { before: {}, after: {} },
      canAdoptNative: true,
      adoptBlockedReason: null,
      canOverwriteCentral: true,
      overwriteBlockedReason: null,
    };
    vi.mocked(commands.listGlobalProfileTargetStatuses).mockResolvedValue({
      status: "ok",
      data: [
        {
          artifactKind: "provider",
          tool: "claude",
          targetPath,
          status: "external_owned_change",
          diagnosticCode: null,
        },
      ],
    });
    vi.mocked(commands.prepareExternalChangePlan).mockResolvedValue({
      status: "ok",
      data: plan,
    });
    vi.mocked(commands.applyExternalChangePlan).mockResolvedValue({
      status: "ok",
      data: {
        runId: plan.previewId,
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 0,
      },
    });

    renderPage();

    const action = await screen.findByRole("button", {
      name: "采纳原生更改",
    });
    fireEvent.click(action);

    await waitFor(() => {
      expect(commands.applyExternalChangePlan).toHaveBeenCalledWith({
        previewId: plan.previewId,
        artifactKind: "provider",
        tool: "claude",
        projectId: null,
        action: "adopt_native",
      });
    });
  });

  it("Cursor 渠道渲染状态区但不渲染 Provider 面板，且不读取或写入 Provider", async () => {
    renderPage("cursor");

    // 状态区正常渲染（提示词能力已开放，页面不再整页 fail closed）。
    expect(await screen.findByText("已检测到 Cursor 2.1.217")).toBeVisible();
    // Provider 面板与其表单、导入入口均不存在。
    expect(
      screen.queryByRole("button", { name: "新增渠道" }),
    ).not.toBeInTheDocument();
    // 走 react-router 的 Link，而不是写死 hash 地址；MemoryRouter 下 href 为路由路径。
    expect(screen.getByRole("link", { name: "管理提示词" })).toHaveAttribute(
      "href",
      "/prompts",
    );
    expect(commands.listProviderProfiles).not.toHaveBeenCalled();
    expect(commands.createProviderProfile).not.toHaveBeenCalled();
    expect(commands.previewProviderSync).not.toHaveBeenCalled();
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it.each(["claude", "codex"] as const)(
    "%s 渠道 默认隐藏表单，新增和编辑可关闭清理且焦点不离开弹窗",
    async (tool) => {
      const kind = "渠道" as const;
      vi.mocked(commands.listProviderProfiles).mockResolvedValue({
        status: "ok",
        data: [makeProviderProfile({ ...provider, tool })],
      });
      renderPage(tool);
      const section = sectionByHeading("渠道");
      const toolName = tool === "claude" ? "Claude" : "Codex";
      const edit = await within(section).findByRole("button", { name: "编辑" });
      const trigger = within(section).getByRole("button", {
        name: `新增${kind}`,
      });
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(screen.queryByLabelText("名称")).not.toBeInTheDocument();

      edit.focus();
      fireEvent.click(edit);
      let dialog = screen.getByRole("dialog", {
        name: `编辑 ${toolName} ${kind}`,
      });
      expect(within(dialog).getByLabelText("名称")).toHaveValue(provider.name);
      fireEvent.change(within(dialog).getByLabelText("名称"), {
        target: { value: "未保存的编辑" },
      });
      fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(edit).toHaveFocus();

      trigger.focus();
      fireEvent.click(trigger);
      dialog = screen.getByRole("dialog", { name: `新增 ${toolName} ${kind}` });
      expect(dialog).toHaveAttribute("aria-modal", "true");
      expect(dialog).toHaveAccessibleDescription(/当前生效档案会自动同步/);
      expect(within(dialog).getByLabelText("名称")).toHaveValue("");
      const submit = within(dialog).getByRole("button", {
        name: `创建${kind}`,
      });
      const firstField = within(dialog).getByRole("radio", {
        name: "API Key（第三方 / 自定义接入）",
      });
      expect(firstField).toHaveFocus();
      fireEvent.keyDown(firstField, { key: "Tab", shiftKey: true });
      expect(submit).toHaveFocus();
      fireEvent.keyDown(submit, { key: "Tab" });
      expect(firstField).toHaveFocus();
      fillProfileForm(dialog);
      fireEvent.keyDown(dialog, { key: "Escape" });
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(trigger).toHaveFocus();

      fireEvent.click(trigger);
      dialog = screen.getByRole("dialog", { name: `新增 ${toolName} ${kind}` });
      expect(within(dialog).getByLabelText("名称")).toHaveValue("");
      expect(within(dialog).getByLabelText("API Key（默认遮罩）")).toHaveValue(
        "",
      );
      fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(trigger).toHaveFocus();
      expect(commands.createProviderProfile).not.toHaveBeenCalled();
      expect(commands.updateProviderProfile).not.toHaveBeenCalled();
    },
  );

  it("渠道 保存失败在弹窗内保留输入，关闭重开不保留错误", async () => {
    vi.mocked(commands.createProviderProfile).mockResolvedValue({
      status: "error",
      error: {
        code: "INVALID_INPUT",
        message: "档案输入无效",
        recoverable: true,
        action: "rescan",
      },
    });
    renderPage();
    const trigger = screen.getByRole("button", { name: "新增渠道" });
    fireEvent.click(trigger);
    let dialog = screen.getByRole("dialog", { name: "新增 Claude 渠道" });
    fillProfileForm(dialog);
    fireEvent.submit(within(dialog).getByRole("form"));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "INVALID_INPUT：档案输入无效",
    );
    expect(within(dialog).getByLabelText("名称")).toHaveValue("新草稿");
    expect(within(dialog).getByLabelText("API Key（默认遮罩）")).toHaveValue(
      "draft-secret",
    );
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新增 Claude 渠道" });
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("名称")).toHaveValue("");
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("渠道 env 校验错误保留草稿，取消后重新新增会清除校验状态", async () => {
    renderPage();
    const trigger = screen.getByRole("button", { name: "新增渠道" });
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "新增 Claude 渠道" });
    fillProfileForm(dialog);
    fireEvent.change(
      within(dialog).getByLabelText("额外 env（每行 KEY=VALUE）"),
      {
        target: { value: "缺少等号" },
      },
    );
    fireEvent.submit(within(dialog).getByRole("form"));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "额外 env 必须按每行 KEY=VALUE 填写。",
    );
    expect(within(dialog).getByLabelText("名称")).toHaveValue("新草稿");
    expect(commands.createProviderProfile).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    fireEvent.click(trigger);
    const nextDialog = screen.getByRole("dialog", { name: "新增 Claude 渠道" });
    expect(within(nextDialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("");
    expect(
      within(nextDialog).getByLabelText("额外 env（每行 KEY=VALUE）"),
    ).toHaveValue("");
  });

  it("渠道 保存和刷新期间阻止重复提交与关闭，完成后不影响新草稿", async () => {
    const pending = deferred<void>();
    const refresh = deferred<void>();
    vi.mocked(commands.createProviderProfile).mockImplementation(async () => {
      await pending.promise;
      return {
        status: "ok",
        data: withAffectedSyncScopes(provider, []),
      };
    });
    renderPage();
    const section = sectionByHeading("渠道");
    await within(section).findByText(/尚无渠道/);
    vi.mocked(commands.listProviderProfiles).mockImplementationOnce(
      async () => {
        await refresh.promise;
        return { status: "ok", data: [provider] };
      },
    );
    const trigger = within(section).getByRole("button", {
      name: "新增渠道",
    });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", {
      name: "新增 Claude 渠道",
    });
    fillProfileForm(dialog);
    const form = within(dialog).getByRole("form");
    act(() => {
      fireEvent.submit(form);
      fireEvent.submit(form);
      fireEvent.click(trigger);
      fireEvent.keyDown(dialog, { key: "Escape" });
    });
    expect(await within(dialog).findByRole("status")).toHaveTextContent(
      "正在保存",
    );
    for (const name of ["取消", "正在保存…"]) {
      const button = within(dialog).getByRole("button", { name });
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    fireEvent.keyDown(dialog, { key: "Escape" });
    fireEvent.submit(form);
    const createCommand = commands.createProviderProfile;
    const listCommand = commands.listProviderProfiles;
    expect(createCommand).toHaveBeenCalledTimes(1);
    expect(dialog).toBeVisible();

    await act(async () => {
      pending.resolve();
      await pending.promise;
    });
    await waitFor(() => expect(listCommand).toHaveBeenCalledTimes(2));
    fireEvent.click(trigger);
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(dialog).toBeVisible();
    expect(within(dialog).getByLabelText("名称")).toHaveValue("新草稿");
    await act(async () => {
      refresh.resolve();
      await refresh.promise;
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(trigger).toHaveFocus();
    fireEvent.click(trigger);
    const nextDialog = screen.getByRole("dialog", {
      name: "新增 Claude 渠道",
    });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("");
    fireEvent.change(within(nextDialog).getByLabelText("名称"), {
      target: { value: "下一份草稿" },
    });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("下一份草稿");
    expect(createCommand).toHaveBeenCalledTimes(1);
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("创建并激活渠道时使用遮罩密钥输入并按返回范围同步", async () => {
    vi.mocked(commands.createProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes({ ...provider, isActive: true }, [
        globalSyncScope("provider", "claude"),
      ]),
    });
    renderPage();
    const section = sectionByHeading("渠道");
    await within(section).findByText(/尚无渠道/);
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [
        makeProviderProfile({ ...provider, name: "新渠道", isActive: true }),
      ],
    });
    fireEvent.click(within(section).getByRole("button", { name: "新增渠道" }));
    const keyInput = within(section).getByLabelText("API Key（默认遮罩）");
    expect(keyInput).toHaveAttribute("type", "password");

    fireEvent.change(within(section).getByLabelText("名称"), {
      target: { value: "新渠道" },
    });
    fireEvent.change(within(section).getByLabelText("API 地址"), {
      target: { value: "https://new.example.com/v1" },
    });
    fireEvent.change(keyInput, { target: { value: "fixture-ui-secret" } });
    fireEvent.change(within(section).getByLabelText("默认模型"), {
      target: { value: "claude-new" },
    });
    fireEvent.click(within(section).getByRole("button", { name: "创建渠道" }));

    await waitFor(() =>
      expect(commands.createProviderProfile).toHaveBeenCalledWith({
        tool: "claude",
        name: "新渠道",
        apiBaseUrl: "https://new.example.com/v1",
        apiKey: "fixture-ui-secret",
        defaultModel: "claude-new",
        options: {
          authKind: "api_key",
          credentialEnvKey: "ANTHROPIC_API_KEY",
          extraEnv: {},
          wireApi: null,
          zcodeKind: null,
          opencodeNpm: null,
          opencodeApi: null,
        },
        activate: true,
      }),
    );
    await waitFor(() =>
      expect(commands.previewProviderSync).toHaveBeenCalledWith("claude"),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
      }),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(within(section).getByRole("listitem")).toHaveTextContent("新渠道");
    expect(within(section).getByText("当前生效")).toHaveClass(
      "rounded-full",
      "border-emerald-200",
      "bg-emerald-50",
    );
    expect(commands.listProviderProfiles).toHaveBeenCalledTimes(3);
    expect(
      await screen.findByText("已应用 1 个目标，可从快照恢复。"),
    ).toBeVisible();
  });

  it("编辑当前生效渠道时默认保留遮罩密钥并按返回范围同步", async () => {
    const providerWithExtraEnv: ProviderProfileDto = makeProviderProfile({
      ...provider,
      isActive: true,
      options: {
        ...provider.options,
        extraEnv: {
          ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-opus",
          ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-sonnet",
        },
      },
    });
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [providerWithExtraEnv],
    });
    vi.mocked(commands.updateProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes(
        { ...provider, name: "已重命名", isActive: true },
        [globalSyncScope("provider", "claude")],
      ),
    });
    renderPage();
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "编辑" }),
    );
    expect(
      screen.getByRole("dialog", { name: "编辑 Claude 渠道" }),
    ).toBeVisible();
    const keyInput = within(section).getByLabelText("API Key（默认遮罩）");
    expect(keyInput).toHaveValue("");
    expect(keyInput).toHaveAttribute("placeholder", "留空以保留现有密钥");
    fireEvent.change(within(section).getByLabelText("名称"), {
      target: { value: "已重命名" },
    });
    fireEvent.click(within(section).getByRole("button", { name: "保存编辑" }));
    await waitFor(() =>
      expect(commands.updateProviderProfile).toHaveBeenCalledWith({
        id: provider.id,
        name: "已重命名",
        apiBaseUrl: provider.apiBaseUrl,
        apiKey: { action: "keep" },
        defaultModel: provider.defaultModel,
        options: {
          authKind: "api_key",
          credentialEnvKey: "ANTHROPIC_API_KEY",
          extraEnv: {
            ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-opus",
            ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-sonnet",
          },
          wireApi: null,
          zcodeKind: null,
          opencodeNpm: null,
          opencodeApi: null,
        },
        rowVersion: provider.rowVersion,
      }),
    );
    await waitFor(() =>
      expect(commands.previewProviderSync).toHaveBeenCalledWith("claude"),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
      }),
    );
  });

  it("官方账号登录渠道显示登录来源，编辑时隐藏接入字段并固定认证方式", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [codexOAuthProvider],
    });
    vi.mocked(commands.updateProviderProfile).mockResolvedValue({
      status: "ok",
      data: { ...codexOAuthProvider, name: "Codex 官方登录" },
    });
    vi.mocked(commands.getOfficialLoginStatus).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "codex",
        loggedIn: true,
        authMethod: "chatgpt",
        manualCommand: "codex login",
      }),
    });

    renderPage("codex");

    const section = sectionByHeading("渠道");
    expect(
      await within(section).findByText("gpt-5.5 · 官方账号登录"),
    ).toBeVisible();
    expect(
      within(section).getByRole("button", { name: "复制到 Claude" }),
    ).toBeDisabled();

    fireEvent.click(within(section).getByRole("button", { name: "编辑" }));
    const dialog = screen.getByRole("dialog", { name: "编辑 Codex 渠道" });
    expect(dialog).toBeVisible();
    // 官方渠道没有接入地址、密钥与 wire_api 字段；认证方式只读。
    expect(
      within(dialog).queryByLabelText("API Key（默认遮罩）"),
    ).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("API 地址")).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("wire_api")).not.toBeInTheDocument();
    const officialRadio = within(dialog).getByRole("radio", {
      name: "官方账号登录（OAuth）",
    });
    expect(officialRadio).toBeChecked();
    expect(officialRadio).toBeDisabled();
    expect(
      await within(dialog).findByText("已登录官方账号（chatgpt）"),
    ).toBeVisible();
    expect(commands.getOfficialLoginStatus).toHaveBeenCalledWith("codex");

    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "Codex 官方登录" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存编辑" }));

    await waitFor(() =>
      expect(commands.updateProviderProfile).toHaveBeenCalledWith({
        id: codexOAuthProvider.id,
        name: "Codex 官方登录",
        apiBaseUrl: "",
        apiKey: { action: "keep" },
        defaultModel: codexOAuthProvider.defaultModel,
        options: {
          authKind: "official_login",
          credentialEnvKey: null,
          extraEnv: {},
          wireApi: null,
          zcodeKind: null,
          opencodeNpm: null,
          opencodeApi: null,
        },
        rowVersion: codexOAuthProvider.rowVersion,
      }),
    );
  });

  it("新增官方账号登录渠道可触发、取消官方 CLI 登录，并以官方类型创建档案", async () => {
    const running = makeOfficialLoginStatus({
      tool: "claude",
      phase: "running",
      loggedIn: null,
    });
    vi.mocked(commands.startOfficialLogin).mockResolvedValue({
      status: "ok",
      data: running,
    });
    vi.mocked(commands.cancelOfficialLogin).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "claude",
        phase: "cancelled",
        diagnostic: "Opening browser...",
      }),
    });
    vi.mocked(commands.createProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes(
        makeProviderProfile({
          name: "Claude 官方账号",
          apiBaseUrl: "",
          defaultModel: "",
          options: {
            authKind: "official_login",
            credentialEnvKey: null,
            extraEnv: {},
            providerId: null,
            wireApi: null,
            zcodeKind: null,
            opencodeNpm: null,
            opencodeApi: null,
          },
          isActive: true,
        }),
        [],
      ),
    });
    renderPage();
    const section = sectionByHeading("渠道");
    await within(section).findByText(/尚无渠道/);
    fireEvent.click(within(section).getByRole("button", { name: "新增渠道" }));
    const dialog = screen.getByRole("dialog", { name: "新增 Claude 渠道" });
    // 默认 API Key 方式：接入字段可见，未查询登录状态。
    expect(within(dialog).getByLabelText("API 地址")).toBeVisible();
    expect(commands.getOfficialLoginStatus).not.toHaveBeenCalled();

    fireEvent.click(
      within(dialog).getByRole("radio", { name: "官方账号登录（OAuth）" }),
    );
    expect(within(dialog).queryByLabelText("API 地址")).not.toBeInTheDocument();
    expect(
      within(dialog).queryByLabelText("API Key（默认遮罩）"),
    ).not.toBeInTheDocument();
    expect(
      within(dialog).queryByLabelText("认证 env key"),
    ).not.toBeInTheDocument();
    // Claude 官方渠道仍可维护额外 env。
    expect(
      within(dialog).getByLabelText("额外 env（每行 KEY=VALUE）"),
    ).toBeVisible();
    expect(
      await within(dialog).findByText("当前未登录官方账号。"),
    ).toBeVisible();
    expect(within(dialog).getByText("claude auth login")).toBeVisible();

    fireEvent.click(
      within(dialog).getByRole("button", { name: "登录官方账号" }),
    );
    await waitFor(() =>
      expect(commands.startOfficialLogin).toHaveBeenCalledWith("claude"),
    );
    expect(
      await within(dialog).findByText("正在等待浏览器完成官方账号授权…"),
    ).toBeVisible();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消登录" }));
    await waitFor(() =>
      expect(commands.cancelOfficialLogin).toHaveBeenCalledWith("claude"),
    );
    expect(await within(dialog).findByText("登录已取消。")).toBeVisible();
    expect(within(dialog).getByText("Opening browser...")).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "登录官方账号" }),
    ).toBeEnabled();

    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "Claude 官方账号" },
    });
    fireEvent.change(
      within(dialog).getByLabelText("额外 env（每行 KEY=VALUE）"),
      { target: { value: "CLAUDE_CODE_MAX_OUTPUT_TOKENS=32000" } },
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "创建渠道" }));
    await waitFor(() =>
      expect(commands.createProviderProfile).toHaveBeenCalledWith({
        tool: "claude",
        name: "Claude 官方账号",
        apiBaseUrl: "",
        apiKey: "",
        defaultModel: "",
        options: {
          authKind: "official_login",
          credentialEnvKey: null,
          extraEnv: { CLAUDE_CODE_MAX_OUTPUT_TOKENS: "32000" },
          wireApi: null,
          zcodeKind: null,
          opencodeNpm: null,
          opencodeApi: null,
        },
        activate: true,
      }),
    );
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("官方 CLI 缺少登录子命令时禁用登录按钮并给出手动命令", async () => {
    vi.mocked(commands.getOfficialLoginStatus).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "codex",
        supported: false,
        loggedIn: null,
        manualCommand: "codex login",
        diagnostic: "当前 Codex CLI 版本没有 login status 子命令，请升级后重试",
      }),
    });
    renderPage("codex");
    const section = sectionByHeading("渠道");
    await within(section).findByText(/尚无渠道/);
    fireEvent.click(within(section).getByRole("button", { name: "新增渠道" }));
    const dialog = screen.getByRole("dialog", { name: "新增 Codex 渠道" });
    fireEvent.click(
      within(dialog).getByRole("radio", { name: "官方账号登录（OAuth）" }),
    );
    expect(
      await within(dialog).findByText(/未安装或不提供登录子命令/),
    ).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "登录官方账号" }),
    ).toBeDisabled();
    expect(within(dialog).queryByLabelText("wire_api")).not.toBeInTheDocument();
    expect(commands.startOfficialLogin).not.toHaveBeenCalled();
  });

  it("导入预览列出未纳入管理的疑似凭据 env 键", async () => {
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: makeProviderImportPreview({
        candidates: [
          makeProviderImportCandidate({
            defaultModel: "",
            skippedEnvKeys: ["ANTHROPIC_CUSTOM_HEADERS", "SOME_FLAG"],
          }),
        ],
      }),
    });
    renderPage();
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "检测已有配置" }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText(/工具默认模型 · 密钥已遮罩保存/),
    ).toBeVisible();
    expect(
      within(dialog).getByText(
        /不纳入管理并保持原样：ANTHROPIC_CUSTOM_HEADERS、SOME_FLAG/,
      ),
    ).toBeVisible();
  });

  it("检测没有可导入渠道时显示反馈", async () => {
    renderPage();
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "检测已有配置" }),
    );
    expect(
      await screen.findByText("未检测到可导入的已有渠道配置。"),
    ).toBeVisible();
  });

  it("已有中央渠道时说明无需再次接管", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [provider],
    });
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: makeProviderImportPreview({
        previewId: null,
        candidates: [
          makeProviderImportCandidate({
            status: "already_managed",
            suggestedName: "Example Provider",
          }),
        ],
      }),
    });
    renderPage();
    const section = sectionByHeading("渠道");
    await within(section).findByText(provider.name);
    fireEvent.click(
      within(section).getByRole("button", { name: "检测已有配置" }),
    );
    expect(
      await screen.findByText("已有中央渠道档案都不需要再次接管。"),
    ).toBeVisible();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("OpenCode 空检测结果解释默认模型与自定义渠道的关联", async () => {
    renderPage("opencode");
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "检测已有配置" }),
    );
    expect(
      await screen.findByText(/默认模型（model）引用了自定义 provider/),
    ).toBeVisible();
    expect(commands.discoverProviderImport).toHaveBeenCalledWith("opencode");
  });

  it("Codex 官方账号导入预览显示登录凭据来源", async () => {
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: makeProviderImportPreview({
        previewId: "00000000-0000-4000-8000-000000000502",
        tool: "codex",
        targetPath: "/isolated/home/.codex/config.toml",
        candidates: [
          makeProviderImportCandidate({
            suggestedName: "Codex 官方账号登录",
            authKind: "official_login",
            apiBaseUrl: "",
            apiKeyConfigured: false,
            defaultModel: "gpt-5.5",
            redactedProjection: { model: "gpt-5.5" },
          }),
        ],
      }),
    });

    renderPage("codex");

    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "检测已有配置" }),
    );

    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText(/gpt-5\.5 · 官方账号登录（不接管凭据）/),
    ).toBeVisible();
  });

  it("切换档案后按返回范围自动同步并消费持久化 preview", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [provider],
    });
    vi.mocked(commands.previewProviderSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, warningCodes: ["FIXTURE_PLAN_WARNING"] },
    });
    vi.mocked(commands.setActiveProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes(provider, [
        globalSyncScope("provider", "claude"),
      ]),
    });
    renderPage();
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "切换并同步" }),
    );
    await waitFor(() =>
      expect(commands.setActiveProviderProfile).toHaveBeenCalledWith("claude", {
        id: provider.id,
        rowVersion: provider.rowVersion,
      }),
    );
    await waitFor(() =>
      expect(commands.previewProviderSync).toHaveBeenCalledWith("claude"),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
      }),
    );
  });

  it("切换已提交但预览失败时仍刷新渠道查询", async () => {
    const listProviderProfiles = vi.mocked(commands.listProviderProfiles);
    listProviderProfiles.mockResolvedValue({
      status: "ok",
      data: [provider],
    });
    vi.mocked(commands.previewProviderSync).mockResolvedValue({
      status: "error",
      error: {
        code: "POLICY_BLOCKED",
        message: "宿主策略禁止生成预览",
        recoverable: true,
        action: "rescan",
      },
    });
    vi.mocked(commands.setActiveProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes(provider, [
        globalSyncScope("provider", "claude"),
      ]),
    });
    renderPage();
    const section = sectionByHeading("渠道");
    const activateButton = await within(section).findByRole("button", {
      name: "切换并同步",
    });
    listProviderProfiles.mockClear();

    fireEvent.click(activateButton);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "POLICY_BLOCKED：宿主策略禁止生成预览",
    );
    expect(
      screen.getAllByText(/POLICY_BLOCKED：宿主策略禁止生成预览/),
    ).toHaveLength(1);
    await waitFor(() => expect(listProviderProfiles).toHaveBeenCalledTimes(2));
  });

  it("分别显示 RPC 错误码与错误状态", async () => {
    vi.mocked(commands.createProviderProfile).mockResolvedValue({
      status: "error",
      error: {
        code: "INVALID_INPUT",
        message: "输入内容无效",
        recoverable: true,
        action: "rescan",
      },
    });
    renderPage();
    const section = sectionByHeading("渠道");
    fireEvent.click(within(section).getByRole("button", { name: "新增渠道" }));
    fireEvent.change(within(section).getByLabelText("名称"), {
      target: { value: "错误档案" },
    });
    fireEvent.change(within(section).getByLabelText("API 地址"), {
      target: { value: "https://invalid.example.com" },
    });
    fireEvent.change(within(section).getByLabelText("API Key（默认遮罩）"), {
      target: { value: "fixture-error-secret" },
    });
    fireEvent.change(within(section).getByLabelText("默认模型"), {
      target: { value: "fixture-model" },
    });
    fireEvent.click(within(section).getByRole("button", { name: "创建渠道" }));
    expect(await within(section).findByRole("alert")).toHaveTextContent(
      "INVALID_INPUT：输入内容无效",
    );
  });

  it("手动同步直接消费持久化预览并 Apply", async () => {
    renderPage();
    const section = sectionByHeading("渠道");

    fireEvent.click(
      await within(section).findByRole("button", {
        name: "同步当前配置",
      }),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
      }),
    );
    expect(
      await screen.findByText("已应用 1 个目标，可从快照恢复。"),
    ).toBeVisible();
  });

  it("手动同步遇到真实冲突时反馈错误且不写入", async () => {
    const baseTarget = preview.targets[0];
    if (!baseTarget) throw new Error("预览 fixture 缺少目标");
    vi.mocked(commands.previewProviderSync).mockResolvedValue({
      status: "ok",
      data: {
        ...preview,
        targets: [
          {
            ...baseTarget,
            changeKind: "conflict",
            status: "external_owned_change",
          },
        ],
      },
    });
    renderPage();
    const section = sectionByHeading("渠道");

    fireEvent.click(
      await within(section).findByRole("button", {
        name: "同步当前配置",
      }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "应用渠道预览失败。",
    );
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("切换生效渠道消费返回范围并自动同步 Apply", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [provider],
    });
    vi.mocked(commands.setActiveProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes(provider, [
        globalSyncScope("provider", "claude"),
      ]),
    });
    renderPage();
    const section = sectionByHeading("渠道");

    fireEvent.click(
      await within(section).findByRole("button", { name: "切换并同步" }),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
      }),
    );
  });

  it("删除当前生效渠道按返回范围自动清理并 Apply", async () => {
    const activeProvider = { ...provider, isActive: true };
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [activeProvider],
    });
    vi.mocked(commands.deleteProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes({ id: activeProvider.id, deleted: true }, [
        globalSyncScope("provider", "claude"),
      ]),
    });
    const confirmSpy = vi.spyOn(globalThis, "confirm").mockReturnValue(true);
    renderPage();
    const section = sectionByHeading("渠道");

    fireEvent.click(
      await within(section).findByRole("button", { name: "删除" }),
    );
    await waitFor(() =>
      expect(commands.deleteProviderProfile).toHaveBeenCalledWith({
        id: activeProvider.id,
        rowVersion: activeProvider.rowVersion,
      }),
    );
    await waitFor(() =>
      expect(commands.previewProviderSync).toHaveBeenCalledWith("claude"),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
      }),
    );
    expect(
      await screen.findByText("已应用 1 个目标，可从快照恢复。"),
    ).toBeVisible();
    confirmSpy.mockRestore();
  });

  it("无生效渠道且无受管基线时把预览 NOT_FOUND 显示为可操作空状态", async () => {
    vi.mocked(commands.previewProviderSync).mockResolvedValue({
      status: "error",
      error: {
        code: "NOT_FOUND",
        message: "未找到目标资源",
        details: { resource: "activeProviderProfile", path: "claude" },
        recoverable: true,
        action: "rescan",
      },
    });
    renderPage();
    const section = sectionByHeading("渠道");

    fireEvent.click(
      await within(section).findByRole("button", { name: "同步当前配置" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "尚无生效渠道档案，也没有可清理的受管基线；请先检测已有配置或创建并激活渠道。",
    );
    expect(
      screen.getAllByText(
        /尚无生效渠道档案，也没有可清理的受管基线；请先检测已有配置或创建并激活渠道。/,
      ),
    ).toHaveLength(1);
  });

  it("工具档案路由注册 Pi 并可从共享页签导航到详情页", async () => {
    expect(TOOL_PROFILE_ROUTES.map(({ path }) => path)).toContain("pi");
    renderWithProviders(<ToolRouteHarness />, { route: "/claude" });

    fireEvent.click(screen.getByRole("link", { name: "pi" }));

    expect(
      await screen.findByRole("heading", { name: "Pi", level: 1 }),
    ).toBeVisible();
  });

  it("切换工具页签后导入预览不残留", async () => {
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: makeProviderImportPreview(),
    });
    renderWithProviders(<ToolRouteHarness />, { route: "/claude" });

    const claudeSection = await sectionByHeadingAsync("渠道");
    fireEvent.click(
      await within(claudeSection).findByRole("button", {
        name: "检测已有配置",
      }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("导入 Claude 已有渠道")).toBeVisible();

    fireEvent.click(screen.getByRole("link", { name: "zcode" }));
    await screen.findByRole("heading", { name: "ZCode", level: 1 });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    // 切换页签不触发检测；确认导入也只发生在发起检测的工具上。
    expect(commands.discoverProviderImport).toHaveBeenCalledTimes(1);
    expect(commands.confirmProviderImport).not.toHaveBeenCalled();
  });

  it("切换工具页签后检测错误提示不残留", async () => {
    vi.mocked(commands.discoverProviderImport).mockRejectedValue(
      new Error("检测失败"),
    );
    renderWithProviders(<ToolRouteHarness />, { route: "/claude" });

    const claudeSection = await sectionByHeadingAsync("渠道");
    fireEvent.click(
      await within(claudeSection).findByRole("button", {
        name: "检测已有配置",
      }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("检测失败");

    fireEvent.click(screen.getByRole("link", { name: "codex" }));
    await screen.findByRole("heading", { name: "Codex", level: 1 });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("切换工具页签后渠道表单内容重置", async () => {
    renderWithProviders(<ToolRouteHarness />, { route: "/claude" });
    const claudeSection = await sectionByHeadingAsync("渠道");
    fireEvent.click(
      await within(claudeSection).findByRole("button", { name: "新增渠道" }),
    );
    const dialog = screen.getByRole("dialog", { name: "新增 Claude 渠道" });
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "跨页签草稿" },
    });

    fireEvent.click(screen.getByRole("link", { name: "zcode" }));
    await screen.findByRole("heading", { name: "ZCode", level: 1 });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    const zcodeSection = await sectionByHeadingAsync("渠道");
    fireEvent.click(
      within(zcodeSection).getByRole("button", { name: "新增渠道" }),
    );
    const zcodeDialog = screen.getByRole("dialog", { name: "新增 ZCode 渠道" });
    expect(within(zcodeDialog).getByLabelText("名称")).toHaveValue("");
  });

  it("Pi 多 provider 检测列出全部候选并批量导入", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [],
    });
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: makeProviderImportPreview({
        tool: "pi",
        targetPath: "/isolated/home/.pi/agent/models.json",
        candidates: [
          makeProviderImportCandidate({
            candidateId: "00000000-0000-4000-8000-000000000801",
            providerId: "cc",
            suggestedName: "cc",
            defaultProvider: true,
            apiFormat: "openai-completions",
            modelCount: 1,
            defaultModel: "deepseek/v4.1-flash",
          }),
          makeProviderImportCandidate({
            candidateId: "00000000-0000-4000-8000-000000000802",
            providerId: "gemini",
            suggestedName: "gemini",
            defaultProvider: false,
            apiFormat: "openai-completions",
            modelCount: 1,
            defaultModel: "gemini-3.8-flash-high",
          }),
        ],
      }),
    });
    vi.mocked(commands.confirmProviderImport).mockResolvedValue({
      status: "ok",
      data: { tool: "pi", importedCount: 2 },
    });

    renderPage("pi");
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "检测已有配置" }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("导入 cc")).toBeChecked();
    expect(within(dialog).getByLabelText("导入 gemini")).not.toBeChecked();
    expect(within(dialog).getByText("默认渠道")).toBeVisible();
    expect(
      within(dialog).getAllByText(/API 格式 openai-completions · 1 个模型/),
    ).toHaveLength(2);

    fireEvent.click(within(dialog).getByLabelText("导入 gemini"));
    // 每个可导入候选都有自己的名称输入；第二个属于 gemini。
    const nameInputs = within(dialog).getAllByLabelText("导入名称");
    fireEvent.change(nameInputs[1]!, {
      target: { value: "Gemini 渠道" },
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "确认导入 2 个渠道" }),
    );

    await waitFor(() =>
      expect(commands.confirmProviderImport).toHaveBeenCalledWith({
        previewId: "00000000-0000-4000-8000-000000000701",
        items: [
          { candidateId: "00000000-0000-4000-8000-000000000801", name: "cc" },
          {
            candidateId: "00000000-0000-4000-8000-000000000802",
            name: "Gemini 渠道",
          },
        ],
      }),
    );
  });

  it("已纳入管理与配置无效的候选不可勾选并显示原因", async () => {
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: makeProviderImportPreview({
        tool: "pi",
        targetPath: "/isolated/home/.pi/agent/models.json",
        previewId: "00000000-0000-4000-8000-000000000811",
        candidates: [
          makeProviderImportCandidate({
            candidateId: "00000000-0000-4000-8000-000000000812",
            providerId: "cc",
            suggestedName: "cc",
            status: "already_managed",
          }),
          makeProviderImportCandidate({
            candidateId: "00000000-0000-4000-8000-000000000813",
            providerId: "broken",
            suggestedName: "broken",
            status: "invalid",
            reason: "PI_PROVIDER_FIELDS_INVALID",
          }),
          makeProviderImportCandidate({
            candidateId: "00000000-0000-4000-8000-000000000814",
            providerId: "gemini",
            suggestedName: "gemini",
          }),
        ],
      }),
    });

    renderPage("pi");
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "检测已有配置" }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("导入 cc")).toBeDisabled();
    expect(within(dialog).getByLabelText("导入 broken")).toBeDisabled();
    expect(within(dialog).getByLabelText("导入 gemini")).toBeEnabled();
    expect(within(dialog).getByText(/已纳入管理/)).toBeVisible();
    expect(
      within(dialog).getByText(/配置无效 · 缺少接入地址或 API Key/),
    ).toBeVisible();
  });

  it("Pi 渠道卡片只读展示 API 格式与模型摘要", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [
        makeProviderProfile({
          tool: "pi",
          name: "cc",
          defaultModel: "deepseek/v4.1-flash",
          pi: {
            apiFormat: "openai-completions",
            models: [
              { id: "deepseek/v4.1-flash", name: null },
              { id: "second-model", name: "Second" },
            ],
          },
        }),
      ],
    });

    renderPage("pi");
    const section = sectionByHeading("渠道");
    expect(
      await within(section).findByText(
        "API 格式 openai-completions · 2 个模型",
      ),
    ).toBeVisible();
  });

  it("Pi 多 provider 参与档案编辑按后端返回范围立即同步", async () => {
    const piProvider = makeProviderProfile({
      ...provider,
      tool: "pi",
      name: "cc",
      isActive: false,
      pi: {
        apiFormat: "openai-completions",
        models: [{ id: "deepseek/v4.1-flash", name: null }],
      },
    });
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [piProvider],
    });
    vi.mocked(commands.updateProviderProfile).mockResolvedValue({
      status: "ok",
      data: withAffectedSyncScopes(piProvider, [
        globalSyncScope("provider", "pi"),
      ]),
    });

    renderPage("pi");
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "编辑" }),
    );
    const dialog = screen.getByRole("dialog", { name: "编辑 Pi 渠道" });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存编辑" }));

    await waitFor(() =>
      expect(commands.previewProviderSync).toHaveBeenCalledWith("pi"),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "pi",
        artifactKind: "provider",
      }),
    );
  });
});
