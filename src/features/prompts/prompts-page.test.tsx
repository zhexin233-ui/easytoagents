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
    setActivePromptProfile: vi.fn(),
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
  tool: "claude",
  name: "默认提示词",
  body: "# 原始规则",
  isActive: false,
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

function renderPromptsPage(tool: Tool = "claude") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const view = render(
    <QueryClientProvider client={client}>
      <PromptsPage />
    </QueryClientProvider>,
  );
  if (tool !== "claude") {
    fireEvent.click(screen.getByRole("tab", { name: "Codex" }));
  }
  return view;
}

function promptSection(): HTMLElement {
  const section = screen
    .getByRole("heading", { name: "全局提示词" })
    .closest("section");
  if (!section) {
    throw new Error("未找到 全局提示词 区域");
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

beforeEach(() => {
  window.localStorage.clear();
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
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: { applyMode: "preview_confirm" },
  });
  vi.mocked(commands.listPromptProfiles).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.setActivePromptProfile).mockResolvedValue({
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
  it.each(["claude", "codex"] as const)(
    "%s 提示词 默认隐藏表单，新增和编辑可关闭清理且焦点不离开弹窗",
    async (tool) => {
      vi.mocked(commands.listPromptProfiles).mockResolvedValue({
        status: "ok",
        data: [{ ...promptProfile, tool }],
      });
      renderPromptsPage(tool);
      const toolName = tool === "claude" ? "Claude" : "Codex";
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
      let dialog = screen.getByRole("dialog", {
        name: `编辑 ${toolName} 提示词`,
      });
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
      dialog = screen.getByRole("dialog", { name: `新增 ${toolName} 提示词` });
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
      dialog = screen.getByRole("dialog", { name: `新增 ${toolName} 提示词` });
      expect(within(dialog).getByLabelText("名称")).toHaveValue("");
      expect(within(dialog).getByLabelText("Markdown 正文")).toHaveValue("");
      fireEvent.click(within(dialog).getByRole("button", { name: "关闭" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(trigger).toHaveFocus();
      expect(commands.createPromptProfile).not.toHaveBeenCalled();
      expect(commands.updatePromptProfile).not.toHaveBeenCalled();
    },
  );

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
    const dialog = screen.getByRole("dialog", { name: "新增 Claude 提示词" });
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
    const nextDialog = screen.getByRole("dialog", {
      name: "新增 Claude 提示词",
    });
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
    const dialog = screen.getByRole("dialog", { name: "新增 Claude 提示词" });
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
    const nextDialog = screen.getByRole("dialog", {
      name: "新增 Claude 提示词",
    });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("");
    fireEvent.change(within(nextDialog).getByLabelText("名称"), {
      target: { value: "下一份草稿" },
    });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("下一份草稿");
    expect(commands.createPromptProfile).toHaveBeenCalledTimes(1);
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("可创建独立的提示词档案", async () => {
    vi.mocked(commands.createPromptProfile).mockResolvedValue({
      status: "ok",
      data: {
        id: "00000000-0000-4000-8000-000000000403",
        tool: "claude",
        name: "代码审查",
        body: "# 审查规则",
        isActive: true,
        importedFromPath: null,
        rowVersion: 1,
      },
    });
    renderPromptsPage();
    const section = promptSection();
    await within(section).findByText(/尚无提示词档案/);
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [{ ...promptProfile, name: "代码审查", isActive: true }],
    });
    fireEvent.click(
      within(section).getByRole("button", { name: "新增提示词" }),
    );
    fireEvent.change(within(section).getByLabelText("名称"), {
      target: { value: "代码审查" },
    });
    fireEvent.change(within(section).getByLabelText("Markdown 正文"), {
      target: { value: "# 审查规则" },
    });
    fireEvent.click(
      within(section).getByRole("button", { name: "创建提示词" }),
    );
    await waitFor(() =>
      expect(commands.createPromptProfile).toHaveBeenCalledWith({
        tool: "claude",
        name: "代码审查",
        body: "# 审查规则",
        activate: true,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    const activeButton = await within(section).findByRole("button", {
      name: "代码审查 当前生效",
    });
    expect(activeButton).toBeDisabled();
    expect(activeButton).toHaveAttribute("aria-pressed", "true");
    expect(activeButton.closest("article")).toHaveTextContent("当前生效");
    expect(commands.listPromptProfiles).toHaveBeenCalledTimes(2);
    expect(commands.applyProfilePreview).not.toHaveBeenCalled();
  });

  it("可编辑并切换提示词，应用时消费提示词 preview", async () => {
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
    expect(
      screen.getByRole("dialog", { name: "编辑 Claude 提示词" }),
    ).toBeVisible();
    fireEvent.change(within(section).getByLabelText("名称"), {
      target: { value: "更新后的提示词" },
    });
    fireEvent.change(within(section).getByLabelText("Markdown 正文"), {
      target: { value: "# 新规则" },
    });
    fireEvent.click(within(section).getByRole("button", { name: "保存编辑" }));
    await waitFor(() =>
      expect(commands.updatePromptProfile).toHaveBeenCalledWith({
        id: promptProfile.id,
        name: "更新后的提示词",
        body: "# 新规则",
        rowVersion: promptProfile.rowVersion,
      }),
    );

    const activate = await within(section).findByRole("button", {
      name: `将 ${promptProfile.name} 设为当前生效`,
    });
    expect(activate).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(activate);
    await waitFor(() =>
      expect(commands.setActivePromptProfile).toHaveBeenCalledWith("claude", {
        id: promptProfile.id,
        rowVersion: promptProfile.rowVersion,
      }),
    );
    expect(commands.previewPromptSync).toHaveBeenCalledWith("claude", null);
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: promptPreview.previewId,
        tool: "claude",
        artifactKind: "prompt",
        projectId: null,
      }),
    );
  });

  it("以可访问状态展示加载、空列表与未知指令遮蔽证据", async () => {
    vi.mocked(commands.listPromptProfiles).mockReturnValue(
      new Promise<never>(() => {}),
    );
    vi.mocked(commands.getToolProfileStatus).mockResolvedValue({
      status: "ok",
      data: {
        tool: "codex",
        availability: "installed",
        installationVersion: "2.1.217",
        providerTargetPath: "/isolated/home/.codex/config.toml",
        promptTargetPath: "/isolated/home/.codex/AGENTS.md",
        promptOverride: "unknown",
        providerPolicy: "allowed",
        newSessionNotice: "新会话生效",
        bearerTokenWarning: null,
      },
    });
    renderPromptsPage("codex");

    expect(await screen.findByText("正在加载提示词档案…")).toHaveAttribute(
      "role",
      "status",
    );
    expect(await screen.findByText(/新会话生效/)).toBeVisible();
    expect(
      await screen.findByText(/无法安全确认 Codex 指令遮蔽状态/),
    ).toBeVisible();
  });

  it("图标选中生效：当前生效档案选中禁用，未生效档案点击走预览链路", async () => {
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [
        { ...promptProfile, name: "生效档案", isActive: true },
        {
          ...promptProfile,
          id: "00000000-0000-4000-8000-000000000403",
          name: "备用档案",
          isActive: false,
        },
      ],
    });
    renderPromptsPage();

    const active = await screen.findByRole("button", {
      name: "生效档案 当前生效",
    });
    expect(active).toBeDisabled();
    expect(active).toHaveAttribute("aria-pressed", "true");
    const standby = screen.getByRole("button", {
      name: "将 备用档案 设为当前生效",
    });
    expect(standby).toBeEnabled();
    expect(standby).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(standby);
    await waitFor(() =>
      expect(commands.setActivePromptProfile).toHaveBeenCalledWith("claude", {
        id: "00000000-0000-4000-8000-000000000403",
        rowVersion: promptProfile.rowVersion,
      }),
    );
    expect(commands.previewPromptSync).toHaveBeenCalledWith("claude", null);
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
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
