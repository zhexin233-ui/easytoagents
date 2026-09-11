import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings/commands";
import {
  deferred,
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
  it("本地目录导入默认收起，按钮打开弹窗且未选目录时禁止提交", async () => {
    renderPage();
    expect(
      screen.queryByRole("dialog", { name: "从本地目录导入" }),
    ).not.toBeInTheDocument();
    expect(commands.importSkill).not.toHaveBeenCalled();
    const trigger = screen.getByRole("button", { name: "从本地目录导入" });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "从本地目录导入" });
    expect(within(dialog).getByLabelText("已选择目录")).toHaveDisplayValue("");
    expect(within(dialog).getByPlaceholderText("尚未选择")).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    ).toBeDisabled();
    expect(commands.importSkill).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
    fireEvent.click(trigger);
    const reopened = screen.getByRole("dialog", { name: "从本地目录导入" });
    expect(within(reopened).getByPlaceholderText("尚未选择")).toBeVisible();
    fireEvent.keyDown(reopened, { key: "Escape" });
    await waitFor(() => expect(reopened).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
    expect(commands.importSkill).not.toHaveBeenCalled();
  });
  it("通过目录选择器显式选择来源并只调用生成的导入 command", async () => {
    vi.mocked(open).mockResolvedValue("/isolated/source/new-skill");
    vi.mocked(commands.importSkill).mockResolvedValue({
      status: "ok",
      data: skill,
    });
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: "从本地目录导入" }));
    const dialog = await screen.findByRole("dialog", {
      name: "从本地目录导入",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择目录" }));
    expect(
      await within(dialog).findByDisplayValue("/isolated/source/new-skill"),
    ).toBeVisible();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    );
    await waitFor(() =>
      expect(commands.importSkill).toHaveBeenCalledWith({
        sourcePath: "/isolated/source/new-skill",
      }),
    );
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    const status = screen.getByText(
      "Skill 已复制到应用私有中央库；来源目录未修改，原生目标也尚未写入。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(
        "Skill 已复制到应用私有中央库；来源目录未修改，原生目标也尚未写入。",
      ),
    ).toHaveLength(1);
  });
  it("本地目录导入失败时保留弹窗、已选目录与重试入口", async () => {
    vi.mocked(open).mockResolvedValue("/isolated/source/broken");
    vi.mocked(commands.importSkill).mockResolvedValue({
      status: "error",
      error: {
        code: "PARSE_ERROR",
        message: "SKILL.md frontmatter 不合法",
        recoverable: true,
      },
    });
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: "从本地目录导入" }));
    const dialog = await screen.findByRole("dialog", {
      name: "从本地目录导入",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择目录" }));
    await within(dialog).findByDisplayValue("/isolated/source/broken");
    fireEvent.click(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    );
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "PARSE_ERROR：SKILL.md frontmatter 不合法",
    );
    expect(
      within(dialog).getByDisplayValue("/isolated/source/broken"),
    ).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    ).toBeEnabled();
    expect(commands.listSkills).toHaveBeenCalledTimes(1);
  });
  it("目录选择器失败时在弹窗内展示结构化错误且不调用导入", async () => {
    vi.mocked(open).mockRejectedValue(new Error("无法访问所选目录"));
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: "从本地目录导入" }));
    const dialog = await screen.findByRole("dialog", {
      name: "从本地目录导入",
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择目录" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "选择目录失败：无法访问所选目录",
    );
    expect(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    ).toBeDisabled();
    expect(commands.importSkill).not.toHaveBeenCalled();
  });
  it("GitHub 导入默认收起，精确提交链接且下载期间阻止关闭和双提交", async () => {
    const pending =
      deferred<Awaited<ReturnType<typeof commands.importGithubSkill>>>();
    vi.mocked(commands.importGithubSkill).mockReturnValue(pending.promise);
    renderPage();
    const trigger = screen.getByRole("button", { name: "从 GitHub 导入" });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "从 GitHub 导入" });
    const input = within(dialog).getByLabelText("GitHub Skill 目录链接");
    fireEvent.change(input, {
      target: {
        value:
          "  https://github.com/vercel-labs/skills/tree/main/skills/find-skills  ",
      },
    });
    const submit = within(dialog).getByRole("button", {
      name: "复制到中央库",
    });
    fireEvent.click(submit);
    await waitFor(() =>
      expect(commands.importGithubSkill).toHaveBeenCalledWith({
        url: "https://github.com/vercel-labs/skills/tree/main/skills/find-skills",
      }),
    );
    expect(
      await within(dialog).findByText("正在从 GitHub 下载并安全导入…"),
    ).toHaveAttribute("role", "status");
    expect(
      within(dialog).getByRole("button", { name: "正在下载并导入…" }),
    ).toBeDisabled();
    fireEvent.submit(within(dialog).getByRole("form"));
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(commands.importGithubSkill).toHaveBeenCalledTimes(1);
    expect(dialog).toBeVisible();
    await act(async () => {
      pending.resolve({ status: "ok", data: skill });
      await pending.promise;
    });
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(
      await screen.findByText(
        "GitHub Skill 已复制到应用私有中央库；未执行脚本，也未自动分配或同步。",
      ),
    ).toHaveAttribute("role", "status");
    expect(trigger).toHaveFocus();
    expect(commands.setGlobalSkillAssignment).not.toHaveBeenCalled();
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });
  it("GitHub 导入失败允许修改链接重试", async () => {
    vi.mocked(commands.importGithubSkill).mockResolvedValueOnce({
      status: "error",
      error: {
        code: "NOT_FOUND",
        message: "未找到 GitHub Skill 目录",
        recoverable: true,
      },
    });
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: "从 GitHub 导入" }));
    const dialog = screen.getByRole("dialog", { name: "从 GitHub 导入" });
    const input = within(dialog).getByLabelText("GitHub Skill 目录链接");
    fireEvent.change(input, {
      target: { value: "https://github.com/owner/repo/tree/main/missing" },
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    );
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "NOT_FOUND：未找到 GitHub Skill 目录",
    );
    fireEvent.change(input, {
      target: { value: "https://github.com/owner/repo/tree/main/fixed" },
    });
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    ).toBeEnabled();
  });
  it("GitHub 复制成功但列表刷新失败时锁住旧提交并说明实际结果", async () => {
    vi.mocked(commands.importGithubSkill).mockResolvedValue({
      status: "ok",
      data: skill,
    });
    renderPage();
    await screen.findByText(skill.description);
    vi.mocked(commands.listSkills).mockResolvedValueOnce({
      status: "error",
      error: {
        code: "DATABASE_ERROR",
        message: "中央列表暂不可读",
        recoverable: true,
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "从 GitHub 导入" }));
    const dialog = screen.getByRole("dialog", { name: "从 GitHub 导入" });
    fireEvent.change(within(dialog).getByLabelText("GitHub Skill 目录链接"), {
      target: { value: "https://github.com/owner/repo/tree/main/skill" },
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    );
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Skill 已复制到中央库，但列表刷新失败：DATABASE_ERROR：中央列表暂不可读",
    );
    expect(
      within(dialog).getByLabelText("GitHub Skill 目录链接"),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: "复制到中央库" }),
    ).toBeDisabled();
    fireEvent.submit(within(dialog).getByRole("form"));
    expect(commands.importGithubSkill).toHaveBeenCalledTimes(1);
  });
  it("独立展示内容预览和删除冲突，不泄露 frontmatter 元数据", async () => {
    const rpcError = {
      code: "CONFLICT" as const,
      message: "中央 Skill 已变化",
      recoverable: true,
      action: "review_conflict" as const,
    };
    vi.mocked(commands.previewSkillContent).mockResolvedValue({
      status: "error",
      error: rpcError,
    });
    vi.mocked(commands.deleteSkill).mockResolvedValue({
      status: "error",
      error: rpcError,
    });
    renderPage();
    expect(await screen.findByText(skill.description)).toBeVisible();
    expect(
      screen.queryByText("phase6-private-frontmatter-marker"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "内容预览" }));
    const contentAlert = await screen.findByText(
      "内容预览失败：CONFLICT：中央 Skill 已变化",
    );
    expect(contentAlert).toHaveAttribute("role", "alert");
    expect(contentAlert).toHaveAttribute("aria-atomic", "true");
    expect(
      screen.getAllByText("内容预览失败：CONFLICT：中央 Skill 已变化"),
    ).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "移出中央库" }));
    const deleteAlert = await screen.findByText(
      "移出中央库失败：CONFLICT：中央 Skill 已变化",
    );
    expect(deleteAlert).toHaveAttribute("role", "alert");
    expect(deleteAlert).toHaveAttribute("aria-atomic", "true");
    expect(
      screen.getAllByText("移出中央库失败：CONFLICT：中央 Skill 已变化"),
    ).toHaveLength(1);
    expect(
      screen.getByText("内容预览失败：CONFLICT：中央 Skill 已变化"),
    ).toHaveAttribute("role", "alert");
  });
  it("内容预览和移出中央库进行中时保留图标按钮状态语义", async () => {
    const contentPending =
      deferred<Awaited<ReturnType<typeof commands.previewSkillContent>>>();
    vi.mocked(commands.previewSkillContent).mockReturnValue(
      contentPending.promise,
    );
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "内容预览" }));
    const readingButton = await screen.findByRole("button", {
      name: "正在读取…",
    });
    expect(readingButton).toBeDisabled();
    expect(readingButton).toHaveAttribute("title", "正在读取…");
    await act(async () => {
      contentPending.resolve({
        status: "ok",
        data: {
          id: skill.id,
          name: skill.name,
          skillMd: "# 内容",
          files: ["SKILL.md"],
          contentHash: skill.contentHash,
          rowVersion: skill.rowVersion,
        },
      });
      await contentPending.promise;
    });
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    const removePending =
      deferred<Awaited<ReturnType<typeof commands.deleteSkill>>>();
    vi.mocked(commands.deleteSkill).mockReturnValue(removePending.promise);
    fireEvent.click(await screen.findByRole("button", { name: "移出中央库" }));
    const removingButton = await screen.findByRole("button", {
      name: "正在移出…",
    });
    expect(removingButton).toBeDisabled();
    expect(removingButton).toHaveAttribute("title", "正在移出…");
    await act(async () => {
      removePending.resolve({
        status: "ok",
        data: { id: skill.id, deleted: true },
      });
      await removePending.promise;
    });
    const status = await screen.findByText(
      "Skill 已安全移出中央库，来源目录保持不变。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText("Skill 已安全移出中央库，来源目录保持不变。"),
    ).toHaveLength(1);
  });
  it("关闭内容预览后把焦点恢复到触发按钮", async () => {
    vi.mocked(commands.previewSkillContent).mockResolvedValue({
      status: "ok",
      data: {
        id: skill.id,
        name: skill.name,
        skillMd:
          "---\nname: fixture-skill\ndescription: 隔离测试 Skill\n---\n\n测试正文",
        files: ["SKILL.md"],
        contentHash: skill.contentHash,
        rowVersion: skill.rowVersion,
      },
    });
    renderPage();
    const trigger = await screen.findByRole("button", { name: "内容预览" });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog", {
      name: skill.name,
    });
    const closeButton = screen.getByRole("button", { name: "关闭" });
    expect(closeButton).toHaveFocus();
    expect(fireEvent.keyDown(dialog, { key: "Tab" })).toBe(false);
    expect(closeButton).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });
  it.each([
    "CENTRAL_SKILL_MISSING",
    "CENTRAL_SKILL_TYPE_CHANGED",
    "CENTRAL_SKILL_PATH_CHANGED",
    "CENTRAL_SKILL_INVALID",
  ] as const)("其他中央诊断 %s 不显示同步更改", async (diagnosticCode) => {
    vi.mocked(commands.listSkills).mockResolvedValue({
      status: "ok",
      data: [
        {
          ...skill,
          status: "invalid",
          diagnosticCode,
        },
      ],
    });
    renderPage();
    expect(await screen.findByText(diagnosticCode)).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "同步更改" }),
    ).not.toBeInTheDocument();
  });
});
