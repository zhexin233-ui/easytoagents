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
  type PromptOverrideState,
  type PromptProfileDto,
  type Tool,
} from "@/bindings/commands";
import { PromptsPage } from "@/features/prompts/prompts-page";
import { centralListLayoutStorageKeys } from "@/components/use-persisted-central-list-layout";

vi.mock("@/bindings/commands", () => ({
  commands: {
    listPromptProfiles: vi.fn(),
    getToolProfileStatus: vi.fn(),
    createPromptProfile: vi.fn(),
    updatePromptProfile: vi.fn(),
    setGlobalPromptAssignment: vi.fn(),
    deletePromptProfile: vi.fn(),
    discoverPromptImport: vi.fn(),
    confirmPromptImport: vi.fn(),
    previewPromptSync: vi.fn(),
    applyProfilePreview: vi.fn(),
    getAppSettings: vi.fn(),
  },
}));

const promptProfile: PromptProfileDto = {
  id: "00000000-0000-4000-8000-000000000402",
  name: "默认提示词",
  body: "# 原始规则",
  globalTools: [],
  importedFromPath: null,
  rowVersion: 3,
};

const promptPreview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000499",
  scope: "global",
  projectId: null,
  dbVersion: 4,
  warningCodes: [],
  targets: [
    {
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
    },
  ],
};

function renderPromptsPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <PromptsPage />
    </QueryClientProvider>,
  );
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function fillPromptForm(dialog: HTMLElement) {
  fireEvent.change(within(dialog).getByLabelText("名称"), {
    target: { value: "新草稿" },
  });
  fireEvent.change(within(dialog).getByLabelText("Markdown 正文"), {
    target: { value: "# 草稿规则" },
  });
}

function toolStatus(
  tool: Tool,
  overrides: Partial<{ promptOverride: PromptOverrideState }> = {},
) {
  return {
    tool,
    availability: "installed" as const,
    installationVersion: "2.1.217",
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
    data: { applyMode: "preview_confirm" },
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
  cleanup();
  vi.clearAllMocks();
});

describe("PromptsPage", () => {
  it("默认隐藏表单，新增和编辑可关闭清理且焦点不离开弹窗", async () => {
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [promptProfile],
    });
    renderPromptsPage();
    const section = promptSection();
    const edit = await within(section).findByRole("button", {
      name: "编辑",
    });
    const trigger = within(section).getByRole("button", {
      name: "新增提示词",
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    edit.focus();
    fireEvent.click(edit);
    let dialog = screen.getByRole("dialog", { name: "编辑提示词" });
    expect(within(dialog).getByLabelText("名称")).toHaveValue(
      promptProfile.name,
    );
    expect(within(dialog).getByLabelText("Markdown 正文")).toHaveValue(
      promptProfile.body,
    );
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "未保存的编辑" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(edit).toHaveFocus();

    trigger.focus();
    fireEvent.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新增提示词" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleDescription(/保存只更新中央/);
    expect(within(dialog).getByLabelText("名称")).toHaveValue("");
    const close = within(dialog).getByRole("button", { name: "关闭" });
    const submit = within(dialog).getByRole("button", {
      name: "创建提示词",
    });
    expect(close).toHaveFocus();
    fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    expect(submit).toHaveFocus();
    fireEvent.keyDown(submit, { key: "Tab" });
    expect(close).toHaveFocus();
    fillPromptForm(dialog);
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    fireEvent.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新增提示词" });
    expect(within(dialog).getByLabelText("名称")).toHaveValue("");
    expect(within(dialog).getByLabelText("Markdown 正文")).toHaveValue("");
    fireEvent.click(within(dialog).getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    expect(commands.createPromptProfile).not.toHaveBeenCalled();
    expect(commands.updatePromptProfile).not.toHaveBeenCalled();
  });

  it("提示词 保存失败在弹窗内保留输入，关闭重开不保留错误", async () => {
    vi.mocked(commands.createPromptProfile).mockResolvedValue({
      status: "error",
      error: {
        code: "INVALID_INPUT",
        message: "档案输入无效",
        recoverable: true,
        action: "rescan",
      },
    });
    renderPromptsPage();
    const trigger = screen.getByRole("button", { name: "新增提示词" });
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "新增提示词" });
    fillPromptForm(dialog);
    fireEvent.submit(within(dialog).getByRole("form"));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "INVALID_INPUT：档案输入无效",
    );
    expect(within(dialog).getByLabelText("名称")).toHaveValue("新草稿");
    expect(within(dialog).getByLabelText("Markdown 正文")).toHaveValue(
      "# 草稿规则",
    );
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.click(trigger);
    const nextDialog = screen.getByRole("dialog", { name: "新增提示词" });
    expect(within(nextDialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("");
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("提示词 保存和刷新期间阻止重复提交与关闭，完成后不影响新草稿", async () => {
    const pending = deferred<void>();
    const refresh = deferred<void>();
    vi.mocked(commands.createPromptProfile).mockImplementation(async () => {
      await pending.promise;
      return { status: "ok", data: promptProfile };
    });
    renderPromptsPage();
    const section = promptSection();
    await within(section).findByText(/尚无提示词档案/);
    vi.mocked(commands.listPromptProfiles).mockImplementationOnce(async () => {
      await refresh.promise;
      return { status: "ok", data: [promptProfile] };
    });
    const trigger = within(section).getByRole("button", {
      name: "新增提示词",
    });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "新增提示词" });
    fillPromptForm(dialog);
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
    expect(commands.createPromptProfile).toHaveBeenCalledTimes(1);
    expect(dialog).toBeVisible();

    await act(async () => {
      pending.resolve();
      await pending.promise;
    });
    await waitFor(() =>
      expect(commands.listPromptProfiles).toHaveBeenCalledTimes(2),
    );
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
    const nextDialog = screen.getByRole("dialog", { name: "新增提示词" });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("");
    fireEvent.change(within(nextDialog).getByLabelText("名称"), {
      target: { value: "下一份草稿" },
    });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("下一份草稿");
    expect(commands.createPromptProfile).toHaveBeenCalledTimes(1);
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("可创建不绑定工具的提示词档案", async () => {
    vi.mocked(commands.createPromptProfile).mockResolvedValue({
      status: "ok",
      data: {
        id: "00000000-0000-4000-8000-000000000403",
        name: "代码审查",
        body: "# 审查规则",
        globalTools: [],
        importedFromPath: null,
        rowVersion: 1,
      },
    });
    renderPromptsPage();
    const section = promptSection();
    await within(section).findByText(/尚无提示词档案/);
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [{ ...promptProfile, name: "代码审查" }],
    });
    fireEvent.click(
      within(section).getByRole("button", { name: "新增提示词" }),
    );
    const createDialog = screen.getByRole("dialog", { name: "新增提示词" });
    fireEvent.change(within(createDialog).getByLabelText("名称"), {
      target: { value: "代码审查" },
    });
    fireEvent.change(within(createDialog).getByLabelText("Markdown 正文"), {
      target: { value: "# 审查规则" },
    });
    fireEvent.click(
      within(createDialog).getByRole("button", { name: "创建提示词" }),
    );
    await waitFor(() =>
      expect(commands.createPromptProfile).toHaveBeenCalledWith({
        name: "代码审查",
        body: "# 审查规则",
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    const card = within(section).getByText("代码审查").closest("article");
    expect(card).toHaveTextContent("未启用");
    expect(commands.listPromptProfiles).toHaveBeenCalledTimes(2);
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("编辑档案并按工具图标启用，应用时消费提示词 preview", async () => {
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [promptProfile],
    });
    vi.mocked(commands.updatePromptProfile).mockResolvedValue({
      status: "ok",
      data: { ...promptProfile, name: "更新后的提示词", body: "# 新规则" },
    });
    renderPromptsPage();
    const section = promptSection();

    fireEvent.click(
      await within(section).findByRole("button", { name: "编辑" }),
    );
    const editDialog = screen.getByRole("dialog", { name: "编辑提示词" });
    fireEvent.change(within(editDialog).getByLabelText("名称"), {
      target: { value: "更新后的提示词" },
    });
    fireEvent.change(within(editDialog).getByLabelText("Markdown 正文"), {
      target: { value: "# 新规则" },
    });
    fireEvent.click(
      within(editDialog).getByRole("button", { name: "保存编辑" }),
    );
    await waitFor(() =>
      expect(commands.updatePromptProfile).toHaveBeenCalledWith({
        id: promptProfile.id,
        name: "更新后的提示词",
        body: "# 新规则",
        rowVersion: promptProfile.rowVersion,
      }),
    );

    // 刷新后列表回读仍为原档案名（mock 未变更 name）。
    const group = await within(section).findByRole("group", {
      name: "默认提示词 全局启用",
    });
    const codexButton = within(group).getByRole("button", {
      name: "Codex 全局未分配",
    });
    expect(codexButton).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(codexButton);
    await waitFor(() =>
      expect(commands.setGlobalPromptAssignment).toHaveBeenCalledWith({
        tool: "codex",
        promptProfileId: promptProfile.id,
        assigned: true,
        rowVersion: promptProfile.rowVersion,
      }),
    );
    // 预览确认模式下图标只更新中央配置；预览由工具卡片的同步按钮发起。
    expect(commands.previewPromptSync).not.toHaveBeenCalled();
    expect(
      await screen.findByText(/全局启用已更新；这只改变中央配置/),
    ).toBeVisible();
    fireEvent.click(
      await screen.findByRole("button", { name: "预览 Codex 全局同步" }),
    );
    await waitFor(() =>
      expect(commands.previewPromptSync).toHaveBeenCalledWith("codex", null),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: promptPreview.previewId,
        tool: "codex",
        artifactKind: "prompt",
        projectId: null,
      }),
    );
  });

  it("每工具至多一份生效：已启用图标呈选中态，启用新档案走替换语义", async () => {
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [
        { ...promptProfile, name: "生效档案", globalTools: ["claude"] },
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
