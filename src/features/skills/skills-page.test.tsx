import {
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings/commands";
import { centralListLayoutStorageKeys } from "@/components/use-persisted-central-list-layout";
import { makeSkill } from "@/test/fixtures";
import { renderPage, setupMocks, skill } from "./skills-page.test-helpers";
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});
beforeEach(setupMocks);
describe("SkillsPage", () => {
  it("中央列表默认单列并可切换为响应式三列", async () => {
    renderPage();
    const section = screen
      .getByRole("heading", { name: "中央列表" })
      .closest("section");
    if (!section) throw new Error("未找到 Skills 中央列表");
    const card = await within(section).findByRole("heading", {
      name: skill.name,
    });
    const article = card.closest("article");
    if (!article) throw new Error("未找到 Skills 中央卡片");
    const list = article.parentElement;
    if (!list) throw new Error("未找到 Skills 中央列表容器");
    const body = article.querySelector<HTMLElement>(
      '[data-slot="central-list-card-body"]',
    );
    const footer = article.querySelector<HTMLElement>(
      '[data-slot="central-list-card-actions"]',
    );
    if (!body || !footer) throw new Error("未找到 Skills 卡片主体或操作栏");
    const pageGrid = section.parentElement;
    if (!pageGrid) throw new Error("未找到 Skills 页面上方布局容器");
    const listButton = within(section).getByRole("button", {
      name: "单列显示",
    });
    const gridButton = within(section).getByRole("button", {
      name: "三列网格显示",
    });
    expect(listButton).toHaveAttribute("aria-pressed", "true");
    expect(gridButton).toHaveAttribute("aria-pressed", "false");
    expect(list).toHaveClass("space-y-3");
    expect(list).not.toHaveClass("grid");
    expect(article).toHaveAttribute("data-layout", "list");
    expect(within(article).getByText("原来源（只读溯源）")).toBeVisible();
    expect(within(article).getByText("中央副本")).toBeVisible();
    expect(within(article).getByText(skill.sourcePath)).toBeVisible();
    expect(within(article).getByText(skill.centralPath)).toBeVisible();
    expect(
      within(footer).queryByRole("button", { name: "内容预览" }),
    ).not.toBeInTheDocument();
    fireEvent.click(gridButton);
    expect(listButton).toHaveAttribute("aria-pressed", "false");
    expect(gridButton).toHaveAttribute("aria-pressed", "true");
    expect(list).toHaveClass(
      "grid",
      "auto-rows-fr",
      "items-stretch",
      "md:grid-cols-2",
      "lg:grid-cols-3",
    );
    expect(list).not.toHaveClass("space-y-3");
    expect(article).toHaveAttribute("data-layout", "grid");
    expect(article).toHaveClass(
      "flex",
      "h-full",
      "flex-col",
      "overflow-hidden",
    );
    expect(body).toHaveClass("flex", "flex-1", "flex-col", "p-4");
    expect(footer).toHaveClass("mt-auto", "border-t", "px-4", "py-3");
    expect(footer).toHaveAccessibleName(`${skill.name} 操作`);
    expect(
      within(article).queryByText("原来源（只读溯源）"),
    ).not.toBeInTheDocument();
    expect(within(article).queryByText("中央副本")).not.toBeInTheDocument();
    expect(
      within(article).queryByText(skill.sourcePath),
    ).not.toBeInTheDocument();
    expect(
      within(article).queryByText(skill.centralPath),
    ).not.toBeInTheDocument();
    expect(within(body).getByText(skill.description)).toHaveClass(
      "line-clamp-3",
    );
    for (const name of ["内容预览", "移出中央库"]) {
      const button = within(footer).getByRole("button", { name });
      expect(button).toBeVisible();
      expect(button).toHaveAttribute("title", name);
      expect(button).toHaveClass("size-8", "p-0");
      expect(button.querySelector("svg")).toHaveAttribute(
        "aria-hidden",
        "true",
      );
    }
    expect(
      within(footer).queryByRole("button", { name: "同步更改" }),
    ).not.toBeInTheDocument();
    expect(
      within(footer).getByLabelText(`${skill.name} 全局平台分配`),
    ).toHaveClass("ml-auto");
  });
  it("独立保存布局并在重新挂载后恢复，非法值回退为单列", () => {
    localStorage.setItem(centralListLayoutStorageKeys.mcp, "grid");
    const firstRender = renderPage();
    const listButton = screen.getByRole("button", { name: "单列显示" });
    const gridButton = screen.getByRole("button", {
      name: "三列网格显示",
    });
    expect(listButton).toHaveAttribute("aria-pressed", "true");
    expect(gridButton).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(gridButton);
    expect(localStorage.getItem(centralListLayoutStorageKeys.skills)).toBe(
      "grid",
    );
    expect(localStorage.getItem(centralListLayoutStorageKeys.mcp)).toBe("grid");
    firstRender.unmount();
    const secondRender = renderPage();
    expect(
      screen.getByRole("button", { name: "三列网格显示" }),
    ).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "单列显示" }));
    expect(localStorage.getItem(centralListLayoutStorageKeys.skills)).toBe(
      "list",
    );
    expect(localStorage.getItem(centralListLayoutStorageKeys.mcp)).toBe("grid");
    secondRender.unmount();
    const thirdRender = renderPage();
    expect(screen.getByRole("button", { name: "单列显示" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    thirdRender.unmount();
    localStorage.setItem(centralListLayoutStorageKeys.skills, "invalid-layout");
    renderPage();
    expect(screen.getByRole("button", { name: "单列显示" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
  it("无效 Skill 的图标按钮仅允许取消既有分配", async () => {
    const invalidSkill = makeSkill({
      ...skill,
      status: "invalid",
      diagnosticCode: "SKILL_PARSE_ERROR",
      globalTools: ["claude"],
    });
    const updatedSkill = makeSkill({
      ...invalidSkill,
      globalTools: [],
      rowVersion: invalidSkill.rowVersion + 1,
    });
    vi.mocked(commands.listSkills)
      .mockResolvedValueOnce({ status: "ok", data: [invalidSkill] })
      .mockResolvedValue({ status: "ok", data: [updatedSkill] });
    vi.mocked(commands.setGlobalSkillAssignment).mockResolvedValue({
      status: "ok",
      data: updatedSkill,
    });
    renderPage();
    const claudeButton = await screen.findByRole("button", {
      name: "Claude 全局已分配",
    });
    const codexButton = screen.getByRole("button", {
      name: "Codex 全局未分配",
    });
    expect(claudeButton).toBeEnabled();
    expect(claudeButton).toHaveAttribute("aria-pressed", "true");
    expect(claudeButton).toHaveAttribute("title", "Claude 全局已分配");
    const claudeIcon = claudeButton.querySelector("img");
    expect(claudeIcon?.getAttribute("src")).toMatch(
      /^(data:image\/svg\+xml|.*claude-icon-square\.svg$)/,
    );
    expect(claudeButton.querySelector("svg")).toBeNull();
    expect(claudeButton.firstElementChild).toHaveClass("opacity-100");
    expect(codexButton).toBeDisabled();
    expect(codexButton).toHaveAttribute("aria-pressed", "false");
    expect(codexButton).toHaveAttribute("title", "Codex 全局未分配");
    const codexIcon = codexButton.querySelector("img");
    expect(codexIcon?.getAttribute("src")).toMatch(
      /^(data:image\/png|.*codex-icon-light\.png$)/,
    );
    expect(codexButton.querySelector("svg")).toBeNull();
    expect(codexButton.firstElementChild).toHaveClass(
      "opacity-25",
      "grayscale",
    );
    expect(screen.queryByText("Claude 全局已分配")).not.toBeInTheDocument();
    fireEvent.click(claudeButton);
    await waitFor(() =>
      expect(commands.setGlobalSkillAssignment).toHaveBeenCalledWith({
        tool: "claude",
        skillId: invalidSkill.id,
        assigned: false,
        rowVersion: invalidSkill.rowVersion,
      }),
    );
    expect(
      await screen.findByRole("button", { name: "Claude 全局未分配" }),
    ).toBeDisabled();
  });
  it("分别展示列表与目标的加载状态", () => {
    vi.mocked(commands.listSkills).mockReturnValue(
      new Promise(() => undefined),
    );
    vi.mocked(commands.listGlobalSkillTargetStatuses).mockReturnValue(
      new Promise(() => undefined),
    );
    renderPage();
    expect(screen.getByText("正在读取 Skills…")).toHaveAttribute(
      "role",
      "status",
    );
    expect(screen.getByText("正在检查全局 Skills 目标…")).toHaveAttribute(
      "role",
      "status",
    );
  });
  it("分别展示目标错误和目标冲突诊断", async () => {
    const rpcError = {
      code: "CONFLICT" as const,
      message: "隔离冲突",
      recoverable: true,
      action: "review_conflict" as const,
    };
    vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValue({
      status: "error",
      error: rpcError,
    });
    renderPage();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "CONFLICT：隔离冲突",
    );
    cleanup();
    vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValue({
      status: "ok",
      data: [
        {
          tool: "claude",
          projectId: null,
          targetPath: "/isolated/home/.claude/skills",
          status: "external_owned_change",
          diagnosticCode: "CENTRAL_SKILL_CONTENT_CHANGED",
        },
      ],
    });
    renderPage();
    expect(
      await screen.findByText("CENTRAL_SKILL_CONTENT_CHANGED"),
    ).toBeVisible();
    expect(screen.getByText("external_owned_change")).toBeVisible();
  });
});
