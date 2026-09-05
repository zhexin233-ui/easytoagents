/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import {
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
  type HookDto,
  type PreviewPlan,
  type Tool,
} from "@/bindings/commands";
import { HooksPage } from "@/features/hooks/hooks-page";
import { toolMetadata } from "@/lib/tool-metadata";

vi.mock("@/bindings/commands", () => ({
  commands: {
    listHooks: vi.fn(),
    getHook: vi.fn(),
    createHook: vi.fn(),
    updateHook: vi.fn(),
    setHookEnabled: vi.fn(),
    deleteHook: vi.fn(),
    setGlobalHookAssignment: vi.fn(),
    setProjectHookAssignment: vi.fn(),
    listHookProjects: vi.fn(),
    listHookProjectOptions: vi.fn(),
    listGlobalHookTargetStatuses: vi.fn(),
    getAppSettings: vi.fn(),
    previewHookSync: vi.fn(),
    applyHookPreview: vi.fn(),
    readoptHookTarget: vi.fn(),
    discoverHookImport: vi.fn(),
    confirmHookImport: vi.fn(),
  },
}));

const hook: HookDto = {
  id: "00000000-0000-4000-8000-000000000701",
  name: "block-rm",
  event: "PreToolUse",
  matcher: "Bash",
  command: "bash .claude/hooks/block-rm.sh",
  timeoutSeconds: 30,
  enabled: true,
  globalTools: [],
  rowVersion: 1,
};

const hookTargetPreview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000799",
  scope: "global",
  projectId: null,
  dbVersion: 4,
  warningCodes: [],
  targets: [
    {
      targetId: "00000000-0000-4000-8000-000000000798",
      descriptor: {
        tool: "claude",
        artifactKind: "hook",
        scope: "global",
        projectRoot: null,
        path: "/isolated/home/.claude/settings.json",
        format: "json",
        managedSelectorRoots: ["hooks"],
        sensitiveSelectors: [],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "reject",
      },
      ownership: { kind: "selectors", paths: [["hooks"]] },
      changeKind: "update",
      status: "in_sync",
      currentFullHash: "a".repeat(64),
      currentManagedHash: "b".repeat(64),
      desiredManagedHash: "c".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: { before: {}, after: { hooks: {} } },
      warningCodes: [],
      baselineMismatchedItems: [],
      readoptAvailable: false,
      errorCode: null,
      git: null,
      excludeFromGit: false,
    },
  ],
};

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <HooksPage />
    </QueryClientProvider>,
  );
}

async function statusCard(tool: Tool) {
  const section = screen
    .getByRole("heading", { name: "全局目标状态" })
    .closest("section");
  if (!section) throw new Error("未找到全局目标状态");
  const card = (
    await within(section).findByText(toolMetadata(tool).label)
  ).closest("article");
  if (!card) throw new Error("未找到工具状态卡");
  return card;
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(commands.listHooks).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.listGlobalHookTargetStatuses).mockResolvedValue({
    status: "ok",
    data: [
      {
        tool: "claude",
        projectId: null,
        targetPath: "/isolated/home/.claude/settings.json",
        status: "missing",
        diagnosticCode: null,
      },
      {
        tool: "codex",
        projectId: null,
        targetPath: "/isolated/home/.codex/hooks.json",
        status: "missing",
        diagnosticCode: null,
      },
      {
        tool: "cursor",
        projectId: null,
        targetPath: "/isolated/home/.cursor/hooks.json",
        status: "missing",
        diagnosticCode: null,
      },
      {
        tool: "zcode",
        projectId: null,
        targetPath: "/isolated/home/.zcode/cli/config.json",
        status: "missing",
        diagnosticCode: null,
      },
    ],
  });
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: {
      applyMode: "preview_confirm",
      enabledTools: ["claude", "codex", "cursor", "zcode"],
    },
  });
  vi.mocked(commands.previewHookSync).mockResolvedValue({
    status: "ok",
    data: hookTargetPreview,
  });
  vi.mocked(commands.applyHookPreview).mockResolvedValue({
    status: "ok",
    data: {
      runId: hookTargetPreview.previewId,
      status: "succeeded",
      appliedTargets: 1,
      snapshotCount: 1,
    },
  });
});

afterEach(cleanup);

describe("HooksPage 中央列表", () => {
  it("空库展示引导文案与四工具全局目标状态", async () => {
    renderPage();
    expect(await screen.findByText(/中央库尚无 Hook/)).toBeInTheDocument();
    for (const tool of ["claude", "codex", "cursor", "zcode"] as const) {
      const card = await statusCard(tool);
      expect(card).toHaveTextContent(
        tool === "claude"
          ? "/isolated/home/.claude/settings.json"
          : tool === "codex"
            ? "/isolated/home/.codex/hooks.json"
            : tool === "cursor"
              ? "/isolated/home/.cursor/hooks.json"
              : "/isolated/home/.zcode/cli/config.json",
      );
    }
  });

  it("展示中央 Hook 并提供事件/命令摘要", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [hook],
    });
    renderPage();
    expect(await screen.findByText("block-rm")).toBeInTheDocument();
    expect(screen.getByText(/PreToolUse · 已启用/)).toBeInTheDocument();
    expect(
      screen.getByText("bash .claude/hooks/block-rm.sh"),
    ).toBeInTheDocument();
    expect(screen.getByText(/匹配全部|Bash/)).toBeInTheDocument();
  });

  it("新增 Hook 提交到 createHook，且不隐式写原生配置", async () => {
    vi.mocked(commands.createHook).mockResolvedValue({
      status: "ok",
      data: hook,
    });
    renderPage();
    await screen.findByText("全局目标状态");
    fireEvent.click(screen.getByRole("button", { name: "新增 Hook" }));
    const dialog = screen.getByRole("dialog", { name: "新增 Hook" });
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "block-rm" },
    });
    fireEvent.change(within(dialog).getByLabelText(/Matcher/), {
      target: { value: "Bash" },
    });
    fireEvent.change(within(dialog).getByLabelText("命令"), {
      target: { value: "bash .claude/hooks/block-rm.sh" },
    });
    fireEvent.change(within(dialog).getByLabelText(/超时秒数/), {
      target: { value: "30" },
    });
    fireEvent.submit(
      within(dialog).getByRole("button", { name: "保存中央意图" }),
    );
    await waitFor(() => {
      expect(commands.createHook).toHaveBeenCalledWith({
        name: "block-rm",
        event: "PreToolUse",
        matcher: "Bash",
        command: "bash .claude/hooks/block-rm.sh",
        timeoutSeconds: 30,
        enabled: true,
      });
    });
    expect(commands.previewHookSync).not.toHaveBeenCalled();
  });

  it("删除 Hook 只更新中央意图并提示单独预览", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [{ ...hook, globalTools: ["claude"] }],
    });
    vi.mocked(commands.deleteHook).mockResolvedValue({
      status: "ok",
      data: { id: hook.id, deleted: true },
    });
    renderPage();
    await screen.findByText("block-rm");
    const deleteButtons = screen.getAllByRole("button", { name: "删除" });
    if (!deleteButtons[0]) throw new Error("未找到删除按钮");
    fireEvent.click(deleteButtons[0]);
    await waitFor(() => expect(commands.deleteHook).toHaveBeenCalled());
    await waitFor(() => {
      expect(commands.previewHookSync).not.toHaveBeenCalled();
    });
  });
});

describe("HooksPage 全局分配", () => {
  it("按分配状态切换平台按钮并调用 setGlobalHookAssignment", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [hook],
    });
    vi.mocked(commands.setGlobalHookAssignment).mockResolvedValue({
      status: "ok",
      data: { ...hook, globalTools: ["claude"] },
    });
    renderPage();
    await screen.findByText("block-rm");
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude 全局未分配" }),
    );
    await waitFor(() => {
      expect(commands.setGlobalHookAssignment).toHaveBeenCalledWith({
        tool: "claude",
        hookId: hook.id,
        assigned: true,
        rowVersion: hook.rowVersion,
      });
    });
  });

  it("事件不被工具支持时禁用分配按钮（Cursor 不支持 UserPromptSubmit）", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [{ ...hook, event: "UserPromptSubmit" }],
    });
    renderPage();
    await screen.findByText("block-rm");
    const cursorButton = await screen.findByRole("button", {
      name: "Cursor 全局未分配",
    });
    expect(cursorButton).toBeDisabled();
    const claudeButton = screen.getByRole("button", {
      name: "Claude 全局未分配",
    });
    expect(claudeButton).toBeEnabled();
  });
});

describe("HooksPage 全局同步", () => {
  it("生成全局预览后打开持久化预览并按参数 Apply", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [hook],
    });
    renderPage();
    const card = await statusCard("claude");
    fireEvent.click(within(card).getByRole("button", { name: "生成全局预览" }));
    await waitFor(() => {
      expect(commands.previewHookSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
        excludeFromGit: false,
      });
    });
    const dialog = await screen.findByRole("dialog", {
      name: "确认原生配置变更",
    });
    expect(dialog).toBeInTheDocument();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() => {
      expect(commands.applyHookPreview).toHaveBeenCalledWith({
        previewId: hookTargetPreview.previewId,
        tool: "claude",
        projectId: null,
      });
    });
  });

  it("中央库为空时提示先创建或导入，不生成空目标", async () => {
    vi.mocked(commands.previewHookSync).mockResolvedValueOnce({
      status: "ok",
      data: { ...hookTargetPreview, targets: [] },
    });
    renderPage();
    const card = await statusCard("claude");
    fireEvent.click(within(card).getByRole("button", { name: "生成全局预览" }));
    await waitFor(() => {
      expect(commands.previewHookSync).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(
        screen.getByText(/暂无启用且已分配到该工具的中央 Hook/),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
  });
});
