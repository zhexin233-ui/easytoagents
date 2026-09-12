import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings/commands";
import { makeSkill } from "@/test/fixtures";
import {
  deferred,
  preview,
  renderPage,
  setupMocks,
  skill,
} from "./skills-page.test-helpers";
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});
beforeEach(setupMocks);
describe("SkillsPage", () => {
  it("仅内容变更诊断在列表和网格显示同步更改，打开确认框不发 RPC", async () => {
    const drifted = makeSkill({
      ...skill,
      status: "invalid",
      diagnosticCode: "CENTRAL_SKILL_CONTENT_CHANGED",
    });
    vi.mocked(commands.listSkills).mockResolvedValue({
      status: "ok",
      data: [drifted],
    });
    renderPage();
    const section = screen
      .getByRole("heading", { name: "中央列表" })
      .closest("section");
    if (!section) throw new Error("未找到 Skills 中央列表");
    const listTrigger = await within(section).findByRole("button", {
      name: "同步更改",
    });
    expect(listTrigger).toBeVisible();
    expect(listTrigger).toHaveAttribute("title", "同步更改");
    expect(listTrigger).toHaveClass("size-8", "p-0");
    expect(listTrigger.querySelector("svg")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(listTrigger).not.toHaveTextContent("同步更改");
    listTrigger.focus();
    fireEvent.click(listTrigger);
    const dialog = screen.getByRole("dialog", { name: "同步更改" });
    expect(dialog).toHaveTextContent(
      "是否将当前中央文件采纳为权威内容？这只会更新应用内记录，不会改写工具目录中的符号链接。",
    );
    expect(within(dialog).getByRole("button", { name: "是" })).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "取消" })).toBeVisible();
    expect(commands.adoptSkillContent).not.toHaveBeenCalled();
    expect(commands.previewSkillContent).not.toHaveBeenCalled();
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
    expect(dialog).not.toHaveTextContent("# Skill");
    expect(dialog).not.toHaveTextContent("phase6-private-content-marker");
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(listTrigger).toHaveFocus();
    expect(commands.adoptSkillContent).not.toHaveBeenCalled();
    fireEvent.click(listTrigger);
    const reopened = screen.getByRole("dialog", { name: "同步更改" });
    fireEvent.keyDown(reopened, { key: "Escape" });
    await waitFor(() => expect(reopened).not.toBeInTheDocument());
    expect(listTrigger).toHaveFocus();
    expect(commands.adoptSkillContent).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "三列网格显示" }));
    const gridTrigger = within(section).getByRole("button", {
      name: "同步更改",
    });
    expect(gridTrigger).toBeVisible();
    expect(gridTrigger).toHaveAttribute("title", "同步更改");
    expect(gridTrigger).toHaveClass("size-8", "p-0");
    expect(gridTrigger.querySelector("svg")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(gridTrigger).not.toHaveTextContent("同步更改");
  });
  it("确认同步更改只提交打开时的 id 与 rowVersion，成功后诊断消失且不预览 Apply", async () => {
    const drifted = makeSkill({
      ...skill,
      status: "invalid",
      diagnosticCode: "CENTRAL_SKILL_CONTENT_CHANGED",
      description: "漂移前的说明",
      rowVersion: 3,
    });
    const adopted = makeSkill({
      ...drifted,
      status: "ready",
      diagnosticCode: null,
      description: "采纳后的描述",
      contentHash: "c".repeat(64),
      rowVersion: 4,
    });
    vi.mocked(commands.listSkills).mockResolvedValue({
      status: "ok",
      data: [drifted],
    });
    vi.mocked(commands.adoptSkillContent).mockImplementation(() => {
      vi.mocked(commands.listSkills).mockResolvedValue({
        status: "ok",
        data: [adopted],
      });
      return Promise.resolve({ status: "ok", data: adopted });
    });
    renderPage();
    const trigger = await screen.findByRole("button", { name: "同步更改" });
    trigger.focus();
    fireEvent.click(trigger);
    expect(commands.adoptSkillContent).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "是" }));
    await waitFor(() =>
      expect(commands.adoptSkillContent).toHaveBeenCalledExactlyOnceWith({
        id: drifted.id,
        rowVersion: drifted.rowVersion,
      }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "同步更改" }),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByText("CENTRAL_SKILL_CONTENT_CHANGED"),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "同步更改" })).toBeNull();
    const status = screen.getByText(
      "已采纳当前中央文件为权威内容；工具目录中的符号链接未被改写。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(
        "已采纳当前中央文件为权威内容；工具目录中的符号链接未被改写。",
      ),
    ).toHaveLength(1);
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
    expect(commands.previewSkillContent).not.toHaveBeenCalled();
  });
  it("同步更改失败时通知错误，不调用预览或 Apply", async () => {
    const drifted = makeSkill({
      ...skill,
      status: "invalid",
      diagnosticCode: "CENTRAL_SKILL_CONTENT_CHANGED",
    });
    vi.mocked(commands.listSkills).mockResolvedValue({
      status: "ok",
      data: [drifted],
    });
    vi.mocked(commands.adoptSkillContent).mockResolvedValue({
      status: "error",
      error: {
        code: "CONFLICT",
        message: "Skill 已被其他操作修改",
        recoverable: true,
        action: "review_conflict",
      },
    });
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "同步更改" }));
    fireEvent.click(screen.getByRole("button", { name: "是" }));
    const alert = await screen.findByText(
      "同步更改失败：CONFLICT：Skill 已被其他操作修改",
    );
    expect(alert).toHaveAttribute("role", "alert");
    expect(alert).toHaveAttribute("aria-atomic", "true");
    expect(
      screen.getAllByText("同步更改失败：CONFLICT：Skill 已被其他操作修改"),
    ).toHaveLength(1);
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "同步更改" }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByText("CENTRAL_SKILL_CONTENT_CHANGED")).toBeVisible();
    expect(screen.getByRole("button", { name: "同步更改" })).toBeVisible();
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });
  it("同步更改提交期间锁定关闭与重复提交", async () => {
    const drifted = makeSkill({
      ...skill,
      status: "invalid",
      diagnosticCode: "CENTRAL_SKILL_CONTENT_CHANGED",
    });
    const pending =
      deferred<Awaited<ReturnType<typeof commands.adoptSkillContent>>>();
    vi.mocked(commands.listSkills).mockResolvedValue({
      status: "ok",
      data: [drifted],
    });
    vi.mocked(commands.adoptSkillContent).mockReturnValue(pending.promise);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "同步更改" }));
    const dialog = screen.getByRole("dialog", { name: "同步更改" });
    fireEvent.click(within(dialog).getByRole("button", { name: "是" }));
    expect(
      await within(dialog).findByRole("button", { name: "正在采纳…" }),
    ).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "取消" })).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    fireEvent.keyDown(dialog, { key: "Escape" });
    fireEvent.click(within(dialog).getByRole("button", { name: "正在采纳…" }));
    expect(dialog).toBeInTheDocument();
    expect(commands.adoptSkillContent).toHaveBeenCalledTimes(1);
    await act(async () => {
      pending.resolve({
        status: "ok",
        data: { ...drifted, status: "ready", diagnosticCode: null },
      });
      await pending.promise;
    });
  });
  it("不再呈现或查询项目追加入口", async () => {
    renderPage();
    expect(
      await screen.findByRole("heading", { name: "全局目标状态" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "项目追加与全局继承" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByLabelText("项目")).not.toBeInTheDocument();
    expect(commands.listSkillProjects).not.toHaveBeenCalled();
    expect(commands.listSkillProjectOptions).not.toHaveBeenCalled();
    expect(commands.setProjectSkillAssignment).not.toHaveBeenCalled();
  });
  it("使用持久化 previewId Apply Skills 链接计划", async () => {
    renderPage();
    const section = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const card = section
      ? (await within(section).findByText("Claude")).closest("article")
      : null;
    if (!card) throw new Error("未找到 Claude Skills 状态卡");
    expect(within(card).getByText("○ 待初始化")).toHaveClass("bg-amber-50");
    expect(
      within(card).getByText("尚未写入受管目标；生成预览会在确认后初始化。"),
    ).toBeVisible();
    const previewButton = within(card).getByRole("button", {
      name: "预览全局同步",
    });
    expect(previewButton).toBeEnabled();
    fireEvent.click(previewButton);
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: null,
      }),
    );
    const status = await screen.findByText(/已应用 1 个 Skills 目标/);
    expect(status).toHaveAttribute("role", "status");
    expect(screen.getAllByText(/已应用 1 个 Skills 目标/)).toHaveLength(1);
  });
  it("直接应用模式下全局目标状态卡隐藏手动同步按钮", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    renderPage();
    const section = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const card = section
      ? (await within(section).findByText("Claude")).closest("article")
      : null;
    if (!card) throw new Error("未找到 Claude Skills 状态卡");
    expect(
      within(card).getByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      }),
    ).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "直接应用全局同步" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "预览全局同步" }),
    ).not.toBeInTheDocument();
    expect(
      await within(card).findByText(
        "尚未写入受管目标；分配条目后会自动初始化。",
      ),
    ).toBeVisible();
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });
  it("直接应用模式下分配自动同步的预览与 Apply 失败通知按队列堆叠", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    vi.mocked(commands.listSkills).mockResolvedValue({
      status: "ok",
      data: [skill],
    });
    vi.mocked(commands.setGlobalSkillAssignment).mockResolvedValue({
      status: "ok",
      data: skill,
    });
    vi.mocked(commands.previewSkillSync)
      .mockResolvedValueOnce({
        status: "error",
        error: {
          code: "DATABASE_ERROR",
          message: "Skills 预览暂不可用",
          recoverable: true,
        },
      })
      .mockResolvedValue({ status: "ok", data: preview });
    vi.mocked(commands.applySkillPreview).mockResolvedValue({
      status: "error",
      error: {
        code: "ATOMIC_WRITE_FAILED",
        message: "Skills 应用失败",
        recoverable: true,
      },
    });
    renderPage();
    expect(
      await screen.findByText(
        "全局分配只更新中央配置；直接应用模式下会自动同步写入工具目录。",
      ),
    ).toBeVisible();
    fireEvent.click(
      await screen.findByRole("button", { name: "Codex 全局未分配" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "DATABASE_ERROR：Skills 预览暂不可用",
    );
    expect(
      screen.getAllByText("DATABASE_ERROR：Skills 预览暂不可用"),
    ).toHaveLength(1);
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude 全局已分配" }),
    );
    await waitFor(() =>
      expect(
        screen.getByText("ATOMIC_WRITE_FAILED：Skills 应用失败"),
      ).toHaveAttribute("role", "alert"),
    );
    expect(
      screen.getAllByText("ATOMIC_WRITE_FAILED：Skills 应用失败"),
    ).toHaveLength(1);
  });
  it("全局空目标预览只提示无需写入，不展示可 Apply 的对话框", async () => {
    vi.mocked(commands.previewSkillSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, targets: [] },
    });
    renderPage();
    const section = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const card = section
      ? (await within(section).findByText("Claude")).closest("article")
      : null;
    if (!card) throw new Error("未找到 Claude Skills 状态卡");
    fireEvent.click(within(card).getByRole("button", { name: "预览全局同步" }));
    const status = await screen.findByText(
      "当前工具没有需要同步的全局 Skill。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText("当前工具没有需要同步的全局 Skill。"),
    ).toHaveLength(1);
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });
  it("被关闭的工具从平台图标列与状态卡中消失", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      },
    });
    renderPage();
    expect(
      await screen.findByRole("button", { name: "Claude 全局已分配" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Cursor 全局未分配" }),
    ).not.toBeInTheDocument();
    const statusSection = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    if (!statusSection) throw new Error("未找到全局目标状态");
    expect(await within(statusSection).findByText("Claude")).toBeVisible();
    expect(within(statusSection).queryByText("Cursor")).not.toBeInTheDocument();
  });
  it.each([
    ["Claude", "claude", false, []],
    ["Codex", "codex", true, ["claude", "codex"]],
    ["Cursor", "cursor", true, ["claude", "cursor"]],
  ] as const)(
    "%s 全局分配更新中央配置并刷新列表与目标，不隐式预览或 Apply",
    async (toolLabel, tool, assigned, updatedTools) => {
      const updatedSkill = makeSkill({
        ...skill,
        globalTools: [...updatedTools],
        rowVersion: skill.rowVersion + 1,
      });
      vi.mocked(commands.listSkills)
        .mockResolvedValueOnce({ status: "ok", data: [skill] })
        .mockResolvedValue({ status: "ok", data: [updatedSkill] });
      vi.mocked(commands.setGlobalSkillAssignment).mockResolvedValue({
        status: "ok",
        data: updatedSkill,
      });
      vi.mocked(commands.listGlobalSkillTargetStatuses)
        .mockResolvedValueOnce({
          status: "ok",
          data: [
            {
              tool: "claude",
              projectId: null,
              targetPath: "/isolated/home/.claude/skills",
              status: "missing",
              diagnosticCode: null,
            },
            {
              tool: "codex",
              projectId: null,
              targetPath: "/isolated/home/.codex/skills",
              status: "missing",
              diagnosticCode: null,
            },
          ],
        })
        .mockResolvedValue({
          status: "ok",
          data: [
            {
              tool,
              projectId: null,
              targetPath:
                tool === "claude"
                  ? "/isolated/home/.claude/skills"
                  : "/isolated/home/.codex/skills",
              status: "missing",
              diagnosticCode: assigned
                ? "SKILL_TARGET_INITIAL_SYNC_PENDING"
                : null,
            },
          ],
        });
      renderPage();
      expect(
        await screen.findByText(
          "全局分配只更新中央配置，不会写入工具目录；请在下方预览全局同步并确认应用。",
        ),
      ).toBeVisible();
      fireEvent.click(
        screen.getByRole("button", {
          name: `${toolLabel} 全局${assigned ? "未分配" : "已分配"}`,
        }),
      );
      await waitFor(() =>
        expect(commands.setGlobalSkillAssignment).toHaveBeenCalledWith({
          tool,
          skillId: skill.id,
          assigned,
          rowVersion: skill.rowVersion,
        }),
      );
      const status = await screen.findByText(
        "全局分配已更新；这只改变中央配置，分配或取消分配不会自动写入工具目录。请预览全局同步并确认应用。",
      );
      expect(status).toHaveAttribute("role", "status");
      expect(
        screen.getAllByText(
          "全局分配已更新；这只改变中央配置，分配或取消分配不会自动写入工具目录。请预览全局同步并确认应用。",
        ),
      ).toHaveLength(1);
      const updatedButton = await screen.findByRole("button", {
        name: `${toolLabel} 全局${assigned ? "已分配" : "未分配"}`,
      });
      expect(updatedButton).toBeVisible();
      expect(updatedButton).toHaveAttribute(
        "aria-pressed",
        assigned ? "true" : "false",
      );
      expect(updatedButton).toHaveAttribute(
        "title",
        `${toolLabel} 全局${assigned ? "已分配" : "未分配"}`,
      );
      expect(updatedButton.querySelector("img")).not.toBeNull();
      expect(updatedButton.querySelector("svg")).toBeNull();
      await waitFor(() => {
        expect(commands.listSkills).toHaveBeenCalledTimes(2);
        expect(commands.listGlobalSkillTargetStatuses).toHaveBeenCalledTimes(2);
      });
      expect(commands.previewSkillSync).not.toHaveBeenCalled();
      expect(commands.applySkillPreview).not.toHaveBeenCalled();
    },
  );
});
