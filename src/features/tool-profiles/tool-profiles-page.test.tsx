/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试需要直接核验 mock。 */
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type PreviewPlan,
  type ProviderProfileDto,
} from "@/bindings/commands";
import { ToolProfilesPage } from "@/features/tool-profiles/tool-profiles-page";

vi.mock("@/bindings/commands", () => ({
  commands: {
    listProviderProfiles: vi.fn(),
    getToolProfileStatus: vi.fn(),
    createProviderProfile: vi.fn(),
    updateProviderProfile: vi.fn(),
    copyProviderProfile: vi.fn(),
    setActiveProviderProfile: vi.fn(),
    deleteProviderProfile: vi.fn(),
    discoverProviderImport: vi.fn(),
    confirmProviderImport: vi.fn(),
    previewProviderSync: vi.fn(),
    applyProfilePreview: vi.fn(),
    getAppSettings: vi.fn(),
    updateAppSettings: vi.fn(),
  },
}));

const provider: ProviderProfileDto = {
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
  },
  isActive: false,
  rowVersion: 2,
};

const codexOAuthProvider: ProviderProfileDto = {
  id: "00000000-0000-4000-8000-000000000501",
  tool: "codex",
  name: "Codex OAuth 登录",
  apiBaseUrl: "https://api.openai.com/v1",
  apiKeyConfigured: false,
  defaultModel: "gpt-5.5",
  options: {
    credentialEnvKey: null,
    extraEnv: {},
    providerId: "openai",
    wireApi: null,
  },
  isActive: true,
  rowVersion: 4,
};

const preview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000499",
  scope: "global",
  projectId: null,
  dbVersion: 4,
  warningCodes: [],
  targets: [
    {
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
    },
  ],
};

function renderPage(tool: ProviderProfileDto["tool"] = "claude") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ToolProfilesPage tool={tool} />
    </QueryClientProvider>,
  );
}

function sectionByHeading(name: string): HTMLElement {
  const section = screen.getByRole("heading", { name }).closest("section");
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

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: { applyMode: "preview_confirm" },
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

afterEach(() => {
  cleanup();
});

describe("ToolProfilesPage", () => {
  it.each(["claude", "codex"] as const)(
    "%s 渠道 默认隐藏表单，新增和编辑可关闭清理且焦点不离开弹窗",
    async (tool) => {
      const kind = "渠道" as const;
      vi.mocked(commands.listProviderProfiles).mockResolvedValue({
        status: "ok",
        data: [{ ...provider, tool }],
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
      expect(dialog).toHaveAccessibleDescription(/保存只更新中央/);
      expect(within(dialog).getByLabelText("名称")).toHaveValue("");
      const close = within(dialog).getByRole("button", { name: "关闭" });
      const submit = within(dialog).getByRole("button", {
        name: `创建${kind}`,
      });
      expect(close).toHaveFocus();
      fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
      expect(submit).toHaveFocus();
      fireEvent.keyDown(submit, { key: "Tab" });
      expect(close).toHaveFocus();
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
      fireEvent.click(within(dialog).getByRole("button", { name: "关闭" }));
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
      return { status: "ok", data: provider };
    });
    renderPage();
    const section = sectionByHeading("渠道");
    await within(section).findByText(/尚无.*档案/);
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
    for (const name of ["关闭", "取消", "正在保存…"]) {
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

  it("创建渠道时使用遮罩密钥输入并只调用生成 command", async () => {
    vi.mocked(commands.createProviderProfile).mockResolvedValue({
      status: "ok",
      data: { ...provider, isActive: true },
    });
    renderPage();
    const section = sectionByHeading("渠道");
    await within(section).findByText(/尚无渠道档案/);
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [{ ...provider, name: "新渠道", isActive: true }],
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
          credentialEnvKey: "ANTHROPIC_API_KEY",
          extraEnv: {},
          wireApi: null,
        },
        activate: true,
      }),
    );
    expect(
      await within(section).findByText(
        "中央渠道档案已保存，原生配置尚未修改。",
      ),
    ).toBeVisible();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(within(section).getByRole("listitem")).toHaveTextContent(
      "新渠道 · 当前生效",
    );
    expect(commands.listProviderProfiles).toHaveBeenCalledTimes(2);
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("编辑已存在渠道时默认保留遮罩密钥", async () => {
    const providerWithExtraEnv: ProviderProfileDto = {
      ...provider,
      options: {
        ...provider.options,
        extraEnv: {
          ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-opus",
          ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-sonnet",
        },
      },
    };
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [providerWithExtraEnv],
    });
    vi.mocked(commands.updateProviderProfile).mockResolvedValue({
      status: "ok",
      data: { ...provider, name: "已重命名" },
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
          credentialEnvKey: "ANTHROPIC_API_KEY",
          extraEnv: {
            ANTHROPIC_DEFAULT_OPUS_MODEL: "claude-opus",
            ANTHROPIC_DEFAULT_SONNET_MODEL: "claude-sonnet",
          },
          wireApi: null,
        },
        rowVersion: provider.rowVersion,
      }),
    );
  });

  it("Codex OAuth 渠道显示登录来源并允许无密钥编辑", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [codexOAuthProvider],
    });
    vi.mocked(commands.updateProviderProfile).mockResolvedValue({
      status: "ok",
      data: { ...codexOAuthProvider, name: "Codex 官方登录" },
    });

    renderPage("codex");

    const section = sectionByHeading("渠道");
    expect(
      await within(section).findByText("gpt-5.5 · 使用 Codex OAuth 登录"),
    ).toBeVisible();
    expect(
      within(section).getByRole("button", { name: "复制到 Claude" }),
    ).toBeDisabled();

    fireEvent.click(within(section).getByRole("button", { name: "编辑" }));
    expect(
      screen.getByRole("dialog", { name: "编辑 Codex 渠道" }),
    ).toBeVisible();
    const keyInput = within(section).getByLabelText("API Key（默认遮罩）");
    expect(keyInput).toBeDisabled();
    expect(keyInput).toHaveAttribute(
      "placeholder",
      "留空以继续使用 Codex OAuth 登录",
    );
    fireEvent.change(within(section).getByLabelText("名称"), {
      target: { value: "Codex 官方登录" },
    });
    fireEvent.click(within(section).getByRole("button", { name: "保存编辑" }));

    await waitFor(() =>
      expect(commands.updateProviderProfile).toHaveBeenCalledWith({
        id: codexOAuthProvider.id,
        name: "Codex 官方登录",
        apiBaseUrl: codexOAuthProvider.apiBaseUrl,
        apiKey: { action: "keep" },
        defaultModel: codexOAuthProvider.defaultModel,
        options: { credentialEnvKey: null, extraEnv: {}, wireApi: null },
        rowVersion: codexOAuthProvider.rowVersion,
      }),
    );
  });

  it("Codex OAuth 导入预览显示登录凭据来源", async () => {
    vi.mocked(commands.discoverProviderImport).mockResolvedValue({
      status: "ok",
      data: {
        previewId: "00000000-0000-4000-8000-000000000502",
        tool: "codex",
        targetPath: "/isolated/home/.codex/config.toml",
        suggestedName: "Codex OAuth 登录",
        apiBaseUrl: "https://api.openai.com/v1",
        apiKeyConfigured: false,
        defaultModel: "gpt-5.5",
        redactedProjection: { model: "gpt-5.5" },
      },
    });

    renderPage("codex");

    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "检测已有配置" }),
    );

    expect(
      await within(section).findByText("gpt-5.5 · 使用 Codex OAuth 登录"),
    ).toBeVisible();
  });

  it("切换档案后打开统一预览并消费持久化 preview", async () => {
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [provider],
    });
    vi.mocked(commands.previewProviderSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, warningCodes: ["FIXTURE_PLAN_WARNING"] },
    });
    renderPage();
    const section = sectionByHeading("渠道");
    fireEvent.click(
      await within(section).findByRole("button", { name: "切换并预览" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(commands.setActiveProviderProfile).toHaveBeenCalledWith("claude", {
      id: provider.id,
      rowVersion: provider.rowVersion,
    });
    expect(screen.getByText("FIXTURE_PLAN_WARNING")).toBeVisible();
    expect(
      screen.getByText("/isolated/home/.claude/settings.json"),
    ).toBeVisible();
    expect(screen.getAllByText(/\[REDACTED\]/).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
        projectId: null,
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
    renderPage();
    const section = sectionByHeading("渠道");
    const activateButton = await within(section).findByRole("button", {
      name: "切换并预览",
    });
    listProviderProfiles.mockClear();

    fireEvent.click(activateButton);

    expect(await within(section).findByRole("alert")).toHaveTextContent(
      "POLICY_BLOCKED：宿主策略禁止生成预览",
    );
    await waitFor(() => expect(listProviderProfiles).toHaveBeenCalledTimes(1));
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

  it("直接应用模式下无冲突渠道预览跳过对话框立即 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    renderPage();
    const section = sectionByHeading("渠道");

    fireEvent.click(
      await within(section).findByRole("button", {
        name: "直接应用渠道同步",
      }),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
        projectId: null,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(
      await screen.findByText("已应用 1 个目标，可从快照恢复。"),
    ).toBeVisible();
  });

  it("直接应用模式下冲突渠道预览回退为人工确认且 Apply 禁用", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
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
        name: "直接应用渠道同步",
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "应用这份预览" })).toBeDisabled();
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("直接应用模式下切换生效渠道自动同步并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    vi.mocked(commands.listProviderProfiles).mockResolvedValue({
      status: "ok",
      data: [provider],
    });
    renderPage();
    const section = sectionByHeading("渠道");

    fireEvent.click(
      await within(section).findByRole("button", { name: "切换并直接应用" }),
    );
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        artifactKind: "provider",
        projectId: null,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
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
      await within(section).findByRole("button", { name: "预览渠道同步" }),
    );

    expect(await within(section).findByRole("alert")).toHaveTextContent(
      "尚无生效渠道档案，也没有可清理的受管基线；请先检测已有配置或创建并激活渠道。",
    );
  });

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
