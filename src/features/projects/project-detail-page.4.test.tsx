import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  commands,
  project,
  hookOptions,
  hookPreview,
  renderPage,
  setupMocks,
} from "./project-detail-page.test-helpers";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

describe("ProjectDetailPage", () => {
  beforeEach(setupMocks);
  it("Hook 项目追加按事件分组展示，继承项只读", async () => {
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Hook" }),
    );
    expect(
      await screen.findByRole("article", {
        name: "项目 工具调用前（PreToolUse）分组",
      }),
    ).toBeInTheDocument();
    // 等待选项查询完成后断言（分组骨架在数据到达前也会渲染）。
    await screen.findByText(/全局继承（只读）：/);
    const group = screen.getByRole("article", {
      name: "项目 工具调用前（PreToolUse）分组",
    });
    expect(within(group).getByText("project-hook")).toBeInTheDocument();
    expect(screen.getByText(/全局继承（只读）：/)).toHaveTextContent(
      "inherited-hook",
    );
    // available-hook 未加入任何分组；会话开始分组为空提示。
    expect(within(group).queryByText("available-hook")).not.toBeInTheDocument();
    const sessionGroup = screen.getByRole("article", {
      name: "项目 会话开始（SessionStart）分组",
    });
    expect(
      within(sessionGroup).getByText("该分组暂无项目追加。"),
    ).toBeInTheDocument();
  });

  it("Hook 项目预览 Apply 使用 Hook 命令", async () => {
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Hook" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude Hooks 同步预览" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyHookPreview).toHaveBeenCalledWith({
        previewId: hookPreview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });

  it("从事件分组往项目追加中央 Hook，按分组事件分配", async () => {
    vi.mocked(commands.setProjectHookAssignment).mockResolvedValue({
      status: "ok",
      data: {
        id: hookOptions[2]!.hookId,
        name: "available-hook",
        event: "Stop",
        matcher: null,
        command: "true",
        timeoutSeconds: null,
        enabled: true,
        scriptName: null,
        globalAssignments: [],
        rowVersion: 6,
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Hook" }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: "往项目 会话开始 分组添加 Hook",
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "添加到项目 会话开始（SessionStart）",
    });
    fireEvent.click(
      within(dialog).getByRole("button", {
        name: "添加 available-hook 到项目 会话开始",
      }),
    );
    await waitFor(() => {
      expect(commands.setProjectHookAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        hookId: hookOptions[2]!.hookId,
        event: "SessionStart",
        assigned: true,
        hookRowVersion: 6,
        projectRowVersion: project.rowVersion,
      });
    });
  });

  it("分组内移除项目追加时使用分配行上的生效事件", async () => {
    vi.mocked(commands.setProjectHookAssignment).mockResolvedValue({
      status: "ok",
      data: {
        id: hookOptions[1]!.hookId,
        name: "project-hook",
        event: "SessionStart",
        matcher: null,
        command: "true",
        timeoutSeconds: null,
        enabled: true,
        scriptName: null,
        globalAssignments: [],
        rowVersion: 4,
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Hook" }),
    );
    await screen.findByText("project-hook");
    const group = screen.getByRole("article", {
      name: "项目 工具调用前（PreToolUse）分组",
    });
    fireEvent.click(
      within(group).getByRole("button", {
        name: "从项目 工具调用前 分组移除 project-hook",
      }),
    );
    await waitFor(() => {
      expect(commands.setProjectHookAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        hookId: hookOptions[1]!.hookId,
        event: "PreToolUse",
        assigned: false,
        hookRowVersion: 4,
        projectRowVersion: project.rowVersion,
      });
    });
  });
});
