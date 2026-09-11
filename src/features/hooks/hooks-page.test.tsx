import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type HookDto,
  type PreviewPlan,
  type Tool,
} from "@/bindings/commands";
import { HooksPage } from "@/features/hooks/hooks-page";
import { toolMetadata } from "@/lib/tool-metadata";
import { renderWithProviders } from "@/test/render";
import { makeHook } from "@/test/fixtures/dtos";
import { makePreviewPlan, makeTarget } from "@/test/fixtures/preview-plan";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const hook: HookDto = makeHook({
  id: "00000000-0000-4000-8000-000000000701",
  name: "block-rm",
  event: "PreToolUse",
  matcher: "Bash",
  command: "bash .claude/hooks/block-rm.sh",
  timeoutSeconds: 30,
  enabled: true,
  scriptName: null,
  globalAssignments: [],
  rowVersion: 1,
});

const hookTargetPreview: PreviewPlan = makePreviewPlan({
  previewId: "00000000-0000-4000-8000-000000000799",
  dbVersion: 4,
  targets: [
    makeTarget({
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
    }),
  ],
});

function renderPage() {
  return renderWithProviders(<HooksPage />);
}

/// 工具事件分组区当前激活工具的状态卡；需要其他工具时先切换页签。
async function statusCard(tool: Tool = "claude") {
  if (tool !== "claude") {
    fireEvent.click(
      await screen.findByRole("button", {
        name: `查看 ${toolMetadata(tool).label} Hooks`,
      }),
    );
  }
  const section = screen
    .getByRole("heading", { name: "工具事件分组" })
    .closest("section");
  if (!section) throw new Error("未找到工具事件分组");
  const targetPath =
    tool === "claude"
      ? "/isolated/home/.claude/settings.json"
      : tool === "codex"
        ? "/isolated/home/.codex/hooks.json"
        : tool === "cursor"
          ? "/isolated/home/.cursor/hooks.json"
          : "/isolated/home/.zcode/cli/config.json";
  const card = (await within(section).findByText(targetPath)).closest(
    "article",
  );
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

afterEach(() => vi.clearAllMocks());

describe("HooksPage 中央列表", () => {
  it("关闭 Claude 后工具视图夹逼到第一个启用工具，不再展示 Claude 状态与入口", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["codex", "cursor", "zcode"],
      },
    });
    renderPage();

    const section = (
      await screen.findByRole("heading", { name: "工具事件分组" })
    ).closest("section");
    if (!section) throw new Error("未找到工具事件分组");
    // 默认选中值仍是 claude，但渲染期按启用工具夹逼到 codex。
    expect(
      await within(section).findByText("/isolated/home/.codex/hooks.json"),
    ).toBeVisible();
    expect(
      within(section).queryByText("/isolated/home/.claude/settings.json"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "查看 Claude Hooks" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "查看 Codex Hooks" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

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
    await screen.findByText("工具事件分组");
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
        scriptSourcePath: null,
      });
    });
    expect(commands.previewHookSync).not.toHaveBeenCalled();
  });

  it("删除 Hook 只更新中央意图并提示单独预览", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [
        makeHook({
          ...hook,
          globalAssignments: [{ tool: "claude", event: "PreToolUse" as const }],
        }),
      ],
    });
    vi.mocked(commands.deleteHook).mockResolvedValue({
      status: "ok",
      data: { id: hook.id, deleted: true },
    });
    renderPage();
    await screen.findByRole("heading", { name: "block-rm" });
    const deleteButtons = screen.getAllByRole("button", { name: "删除" });
    if (!deleteButtons[0]) throw new Error("未找到删除按钮");
    fireEvent.click(deleteButtons[0]);
    await waitFor(() => expect(commands.deleteHook).toHaveBeenCalled());
    await waitFor(() => {
      expect(commands.previewHookSync).not.toHaveBeenCalled();
    });
  });
});

describe("HooksPage 事件分组分配", () => {
  it("从事件分组添加中央 Hook，按分组事件调用分配", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [hook],
    });
    vi.mocked(commands.setGlobalHookAssignment).mockResolvedValue({
      status: "ok",
      data: {
        ...hook,
        globalAssignments: [{ tool: "claude", event: "PreToolUse" }],
      },
    });
    renderPage();
    await screen.findByRole("heading", { name: "block-rm" });
    fireEvent.click(
      await screen.findByRole("button", {
        name: "往 工具调用前 分组添加 Hook",
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "添加到 工具调用前（PreToolUse）",
    });
    fireEvent.click(
      within(dialog).getByRole("button", {
        name: "添加 block-rm 到 工具调用前",
      }),
    );
    await waitFor(() => {
      expect(commands.setGlobalHookAssignment).toHaveBeenCalledWith({
        tool: "claude",
        hookId: hook.id,
        event: "PreToolUse",
        assigned: true,
        rowVersion: hook.rowVersion,
      });
    });
  });

  it("同一 Hook 可在其他工具分配为不同事件", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [
        makeHook({
          ...hook,
          globalAssignments: [{ tool: "claude", event: "PreToolUse" as const }],
        }),
      ],
    });
    vi.mocked(commands.setGlobalHookAssignment).mockResolvedValue({
      status: "ok",
      data: hook,
    });
    renderPage();
    await screen.findByRole("heading", { name: "block-rm" });
    // 切换到 Codex 工具页签，把同一 Hook 添加到「会话开始」分组。
    fireEvent.click(
      await screen.findByRole("button", { name: "查看 Codex Hooks" }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: "往 会话开始 分组添加 Hook",
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "添加到 会话开始（SessionStart）",
    });
    fireEvent.click(
      within(dialog).getByRole("button", {
        name: "添加 block-rm 到 会话开始",
      }),
    );
    await waitFor(() => {
      expect(commands.setGlobalHookAssignment).toHaveBeenCalledWith({
        tool: "codex",
        hookId: hook.id,
        event: "SessionStart",
        assigned: true,
        rowVersion: hook.rowVersion,
      });
    });
  });

  it("已分配 Hook 出现在对应事件分组并可移除", async () => {
    vi.mocked(commands.listHooks).mockResolvedValue({
      status: "ok",
      data: [
        makeHook({
          ...hook,
          globalAssignments: [{ tool: "claude", event: "PreToolUse" as const }],
        }),
      ],
    });
    vi.mocked(commands.setGlobalHookAssignment).mockResolvedValue({
      status: "ok",
      data: hook,
    });
    renderPage();
    await screen.findByRole("heading", { name: "block-rm" });
    const group = screen.getByRole("article", {
      name: "工具调用前（PreToolUse）分组",
    });
    expect(within(group).getByText("block-rm")).toBeInTheDocument();
    fireEvent.click(
      within(group).getByRole("button", {
        name: "从 工具调用前 分组移除 block-rm",
      }),
    );
    await waitFor(() => {
      expect(commands.setGlobalHookAssignment).toHaveBeenCalledWith({
        tool: "claude",
        hookId: hook.id,
        event: "PreToolUse",
        assigned: false,
        rowVersion: hook.rowVersion,
      });
    });
  });

  it("工具不支持的事件分组不展示（Cursor 无提示词提交）", async () => {
    renderPage();
    await screen.findByRole("button", { name: "查看 Cursor Hooks" });
    fireEvent.click(screen.getByRole("button", { name: "查看 Cursor Hooks" }));
    expect(
      await screen.findByRole("article", {
        name: "工具调用前（PreToolUse）分组",
      }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("article", {
        name: "提示词提交（UserPromptSubmit）分组",
      }),
    ).not.toBeInTheDocument();
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
