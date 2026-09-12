import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { AppShell } from "@/app/app-shell";
import { commands } from "@/bindings/commands";
import { themeStorageKey } from "@/components/use-theme";
import { makeProject } from "@/test/fixtures/dtos";
import { renderWithProviders } from "@/test/render";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

type EnvironmentReadyHandler = (succeeded: boolean) => void;

const environmentEvents = vi.hoisted(() => {
  const handlers: Array<(succeeded: boolean) => void> = [];
  return { handlers };
});

vi.mock("@/lib/tauri-events", () => ({
  subscribeEnvironmentReady: (handler: EnvironmentReadyHandler) => {
    environmentEvents.handlers.push(handler);
    return () => {
      const index = environmentEvents.handlers.indexOf(handler);
      if (index >= 0) environmentEvents.handlers.splice(index, 1);
    };
  },
}));

const project = makeProject({
  id: "00000000-0000-4000-8000-000000000801",
  displayName: "侧栏项目",
  rootPath: "/isolated/projects/sidebar",
  pathStatus: "valid",
  gitStatus: "repository",
  codexTrustStatus: "trusted",
  claudePolicyStatus: "allowed",
  targets: [],
  nativeResources: { active: 1, disabled: 0, missing: 0, conflict: 0 },
  lastScannedAt: "2026-09-09T10:00:00Z",
  rowVersion: 7,
});

function LocationProbe() {
  const { pathname } = useLocation();
  return <output aria-label="当前路径">{pathname}</output>;
}

function renderShell(initialEntry = "/") {
  return renderWithProviders(
    <>
      <AppShell />
      <LocationProbe />
    </>,
    { initialEntries: [initialEntry] },
  );
}

describe("AppShell 侧边栏设置入口", () => {
  beforeEach(() => {
    environmentEvents.handlers.length = 0;
    vi.mocked(commands.getEnvironmentState).mockReset();
    vi.mocked(commands.getEnvironmentState).mockResolvedValue({
      status: "ok",
      data: { probing: false, tools: [] },
    });
    vi.mocked(commands.listProjects).mockReset();
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [],
    });
    vi.mocked(commands.removeProject).mockReset();
    vi.mocked(commands.removeProject).mockResolvedValue({
      status: "ok",
      data: {
        id: project.id,
        removed: true,
        nativeConfigurationLeftUnmanaged: true,
      },
    });
    vi.mocked(commands.renameProject).mockReset();
    vi.mocked(commands.renameProject).mockResolvedValue({
      status: "ok",
      data: { ...project, displayName: "新项目名称", rowVersion: 8 },
    });
    vi.mocked(commands.getAppSettings).mockReset();
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "preview_confirm", enabledTools: ["claude", "codex"] },
    });
    localStorage.clear();
    document.documentElement.classList.remove("dark");
  });

  it("收到 environment-ready 事件后重新拉取依赖环境的查询", async () => {
    renderShell();
    await waitFor(() => expect(commands.listProjects).toHaveBeenCalledTimes(1));
    expect(environmentEvents.handlers).toHaveLength(1);
    act(() => environmentEvents.handlers[0]?.(true));
    await waitFor(() => expect(commands.listProjects).toHaveBeenCalledTimes(2));
  });

  it("按总览、提示词、MCP、Hooks、Skills、项目的顺序渲染一级导航", () => {
    renderShell();

    const navigation = screen.getByRole("navigation", { name: "一级导航" });
    expect(
      within(navigation)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual(["总览", "提示词", "MCP", "Hooks", "Skills", "项目"]);
  });

  it("设置不再是一级导航链接，而是左下角的图标按钮", () => {
    renderShell();

    expect(
      screen.queryByRole("link", { name: "设置" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute(
      "aria-haspopup",
      "dialog",
    );
  });

  it("点击后打开设置对话框，关闭后消失", async () => {
    renderShell();

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    expect(await screen.findByRole("dialog", { name: "设置" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("顶栏不再渲染外观切换，外观切换位于设置弹窗内", async () => {
    renderShell();

    expect(
      screen.queryByRole("group", { name: "外观模式" }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const group = await screen.findByRole("group", { name: "外观模式" });
    expect(group).toBeVisible();
    expect(
      screen.getByRole("button", { name: "跟随系统外观" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

  it("当前应用入口使用 accent 选中态，切换应用时只改变当前入口", () => {
    renderShell("/claude");

    const claudeLink = screen.getByRole("link", { name: /Claude/ });
    const codexLink = screen.getByRole("link", { name: /Codex/ });
    expect(claudeLink).toHaveClass("bg-accent-soft");
    expect(claudeLink).not.toHaveClass("bg-muted");
    expect(codexLink).not.toHaveClass("bg-accent-soft");

    fireEvent.click(codexLink);
    expect(codexLink).toHaveClass("bg-accent-soft");
    expect(claudeLink).not.toHaveClass("bg-accent-soft");
  });

  it("顶栏只渲染启用的工具入口", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "preview_confirm", enabledTools: ["claude"] },
    });
    renderShell();

    const navigation = screen.getByRole("navigation", { name: "工具入口" });
    expect(
      within(navigation).getByRole("link", { name: /Claude/ }),
    ).toBeVisible();
    // 加载中先按默认集合渲染，数据到达后 Codex 入口移除。
    await waitFor(() =>
      expect(
        within(navigation).queryByRole("link", { name: /Codex/ }),
      ).not.toBeInTheDocument(),
    );
    expect(within(navigation).getAllByRole("link")).toHaveLength(1);
  });

  it("在设置弹窗中切换外观会立即挂 dark class 并持久化", async () => {
    renderShell();

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "暗色模式" }));
    expect(screen.getByRole("button", { name: "暗色模式" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(localStorage.getItem(themeStorageKey)).toBe("dark");

    fireEvent.click(screen.getByRole("button", { name: "亮色模式" }));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(localStorage.getItem(themeStorageKey)).toBe("light");
  });

  it("侧栏项目行提供带项目名的移除按钮，取消确认不调用命令", async () => {
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [project],
    });
    renderShell();

    const removeButton = await screen.findByRole("button", {
      name: `移除项目 ${project.displayName}`,
    });
    expect(removeButton).toHaveAttribute(
      "title",
      `移除项目 ${project.displayName}`,
    );
    expect(removeButton).toHaveClass(
      "opacity-0",
      "group-hover:opacity-100",
      "group-focus-within:opacity-100",
      "focus-visible:opacity-100",
      "text-muted-foreground",
    );
    fireEvent.click(removeButton);

    const dialog = await screen.findByRole("dialog", {
      name: "确认移除项目",
    });
    expect(dialog).toHaveTextContent(
      `项目“${project.displayName}”的登记吗？此操作只移除登记，不删除项目目录或原生配置。`,
    );
    expect(
      screen
        .getAllByRole("button", { name: /移除项目/ })
        .every((button) => button.hasAttribute("disabled")),
    ).toBe(true);

    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(commands.removeProject).not.toHaveBeenCalled();
  });

  it("通过侧栏编辑按钮修改项目显示名称", async () => {
    const renamedProject = {
      ...project,
      displayName: "新项目名称",
      rowVersion: 8,
    };
    vi.mocked(commands.listProjects)
      .mockResolvedValueOnce({ status: "ok", data: [project] })
      .mockResolvedValue({ status: "ok", data: [renamedProject] });
    vi.mocked(commands.renameProject).mockResolvedValue({
      status: "ok",
      data: renamedProject,
    });
    renderShell();

    const editButton = await screen.findByRole("button", {
      name: `编辑项目 ${project.displayName}`,
    });
    expect(editButton).toHaveClass(
      "opacity-0",
      "group-hover:opacity-100",
      "group-focus-within:opacity-100",
      "text-muted-foreground",
    );
    fireEvent.click(editButton);

    const dialog = await screen.findByRole("dialog", {
      name: "修改项目显示名称",
    });
    const input = within(dialog).getByRole("textbox", { name: "显示名称" });
    expect(input).toHaveValue(project.displayName);
    expect(
      within(dialog).getByRole("button", { name: "保存名称" }),
    ).toBeDisabled();

    fireEvent.change(input, { target: { value: renamedProject.displayName } });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存名称" }));

    await waitFor(() =>
      expect(commands.renameProject).toHaveBeenCalledWith({
        id: project.id,
        displayName: renamedProject.displayName,
        rowVersion: project.rowVersion,
      }),
    );
    expect(
      await screen.findByRole("link", { name: renamedProject.displayName }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText("项目显示名称已修改为“新项目名称”。"),
    ).toBeInTheDocument();
  });

  it("修改项目显示名称失败时保留弹窗和输入", async () => {
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [project],
    });
    vi.mocked(commands.renameProject).mockResolvedValue({
      status: "error",
      error: {
        code: "CONFLICT",
        message: "项目版本冲突",
        details: { reason: "项目已被其他操作更新" },
        recoverable: true,
      },
    });
    renderShell();

    fireEvent.click(
      await screen.findByRole("button", {
        name: `编辑项目 ${project.displayName}`,
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "修改项目显示名称",
    });
    const input = within(dialog).getByRole("textbox", { name: "显示名称" });
    fireEvent.change(input, { target: { value: "冲突后的名称" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存名称" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "CONFLICT：项目已被其他操作更新",
    );
    expect(input).toHaveValue("冲突后的名称");
    expect(dialog).toBeInTheDocument();
  });

  it("确认后使用精确版本化 payload，刷新项目查询并从当前项目导航到列表", async () => {
    vi.mocked(commands.listProjects)
      .mockResolvedValueOnce({ status: "ok", data: [project] })
      .mockResolvedValue({ status: "ok", data: [] });
    renderShell(`/projects/${project.id}`);

    fireEvent.click(
      await screen.findByRole("button", {
        name: `移除项目 ${project.displayName}`,
      }),
    );
    fireEvent.click(
      within(
        await screen.findByRole("dialog", { name: "确认移除项目" }),
      ).getByRole("button", { name: "确认移除" }),
    );

    await waitFor(() =>
      expect(commands.removeProject).toHaveBeenCalledWith({
        id: project.id,
        rowVersion: project.rowVersion,
      }),
    );
    await waitFor(() =>
      expect(screen.getByLabelText("当前路径")).toHaveTextContent("/projects"),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("button", {
          name: `移除项目 ${project.displayName}`,
        }),
      ).not.toBeInTheDocument(),
    );
    expect(
      await screen.findByText(/项目“侧栏项目”已移除登记/),
    ).toBeInTheDocument();
  });

  it("有禁用或冲突原生资源时语义化禁用移除按钮并解释原因", async () => {
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [
        {
          ...project,
          nativeResources: {
            ...project.nativeResources,
            disabled: 1,
            conflict: 1,
          },
        },
      ],
    });
    renderShell();

    const removeButton = await screen.findByRole("button", {
      name: `移除项目 ${project.displayName}`,
    });
    expect(removeButton).toBeDisabled();
    expect(
      screen.getByText("无法移除：请先恢复已禁用或存在冲突的原生资源。"),
    ).toBeVisible();
    fireEvent.click(removeButton);
    expect(commands.removeProject).not.toHaveBeenCalled();
  });

  it("移除非当前项目时保留当前路由", async () => {
    vi.mocked(commands.listProjects)
      .mockResolvedValueOnce({ status: "ok", data: [project] })
      .mockResolvedValue({ status: "ok", data: [] });
    renderShell("/mcp");

    fireEvent.click(
      await screen.findByRole("button", {
        name: `移除项目 ${project.displayName}`,
      }),
    );
    const dialog = await screen.findByRole("dialog", { name: "确认移除项目" });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认移除" }));

    await waitFor(() =>
      expect(commands.removeProject).toHaveBeenCalledWith({
        id: project.id,
        rowVersion: project.rowVersion,
      }),
    );
    expect(screen.getByLabelText("当前路径")).toHaveTextContent("/mcp");
  });

  it("移除失败时通过可访问通知展示结构化错误并保留操作入口", async () => {
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [project],
    });
    vi.mocked(commands.removeProject).mockResolvedValue({
      status: "error",
      error: {
        code: "CONFLICT",
        message: "项目版本冲突",
        details: { reason: "项目已被其他操作更新" },
        recoverable: true,
      },
    });
    renderShell();

    fireEvent.click(
      await screen.findByRole("button", {
        name: `移除项目 ${project.displayName}`,
      }),
    );
    const dialog = await screen.findByRole("dialog", { name: "确认移除项目" });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认移除" }));

    expect(
      await screen.findByText(
        /移除项目“侧栏项目”失败：CONFLICT：项目已被其他操作更新/,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: `移除项目 ${project.displayName}` }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("dialog", { name: "确认移除项目" }),
    ).toBeInTheDocument();
  });
});
