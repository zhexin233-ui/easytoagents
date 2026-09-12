import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings/commands";
import { toolMetadata } from "@/lib/tool-metadata";
import { makeSkill } from "@/test/fixtures";
import {
  deferred,
  nativeImport,
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
  it("直接应用模式下分配切换自动同步并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    const updatedSkill = makeSkill({
      ...skill,
      globalTools: ["claude", "codex"],
      rowVersion: skill.rowVersion + 1,
    });
    vi.mocked(commands.listSkills)
      .mockResolvedValueOnce({ status: "ok", data: [skill] })
      .mockResolvedValue({ status: "ok", data: [updatedSkill] });
    vi.mocked(commands.setGlobalSkillAssignment).mockResolvedValue({
      status: "ok",
      data: updatedSkill,
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "Codex 全局未分配" }),
    );
    await waitFor(() =>
      expect(commands.previewSkillSync).toHaveBeenCalledWith({
        tool: "codex",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "codex",
        projectId: null,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText(/已应用 1 个 Skills 目标/)).toBeVisible();
  });
  it.each([
    [
      "CLAUDE_POLICY_UNKNOWN",
      "△ 策略状态待确认",
      "无法确认 Claude 管理策略是否允许该类自定义目标，当前已安全阻止预览。",
      "bg-amber-50",
    ],
    [
      "CLAUDE_POLICY_BLOCKED",
      "⛔ 策略阻止",
      "Claude 管理策略禁止该类自定义目标。",
      "bg-red-50",
    ],
  ] as const)(
    "区分全局策略诊断 %s 的提示、色调和阻断操作",
    async (diagnosticCode, label, description, toneClass) => {
      vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValue({
        status: "ok",
        data: [
          {
            tool: "claude",
            projectId: null,
            targetPath: "/isolated/home/.claude/skills",
            status: "policy_blocked",
            diagnosticCode,
          },
        ],
      });
      renderPage();
      const section = screen
        .getByRole("heading", { name: "全局目标状态" })
        .closest("section");
      const card = section
        ? (await within(section).findByText("Claude")).closest("article")
        : null;
      if (!card) throw new Error("未找到 Claude Skills 状态卡");
      expect(within(card).getByText(label)).toHaveClass(toneClass);
      expect(within(card).getByText(description)).toBeVisible();
      expect(within(card).getByText(diagnosticCode)).toBeVisible();
      const button = within(card).getByRole("button", {
        name: "预览全局同步",
      });
      expect(button).toBeDisabled();
      fireEvent.click(button);
      const importButton = within(card).getByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      });
      expect(importButton).toBeDisabled();
      fireEvent.click(importButton);
      expect(commands.discoverSkillImport).not.toHaveBeenCalled();
      expect(commands.previewSkillSync).not.toHaveBeenCalled();
    },
  );
});
describe("全局 Skills 检测与复制导入", () => {
  it("候选仍可展示但没有确认令牌时不允许选择或提交", async () => {
    vi.mocked(commands.discoverSkillImport).mockResolvedValueOnce({
      status: "ok",
      data: {
        ...nativeImport("claude"),
        previewId: null,
        message: "来源检测不完整，需要处理后重扫。",
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      }),
    );
    expect(
      await screen.findByRole("checkbox", { name: "导入 new-skill" }),
    ).toBeDisabled();
    expect(
      screen.getByText("当前检测结果不能确认导入，请处理来源诊断后重新检测。"),
    ).toBeVisible();
    expect(screen.getByText("来源检测不完整，需要处理后重扫。")).toBeVisible();
    fireEvent.submit(
      screen.getByRole("form", { name: "导入 Claude 全局 Skills" }),
    );
    expect(commands.confirmSkillImport).not.toHaveBeenCalled();
  });
  it.each(["claude", "codex", "cursor"] as const)(
    "%s 中央空列表可显式检测，仅提交勾选项并刷新 Skills，不隐式分配或同步",
    async (tool) => {
      vi.mocked(commands.listSkills).mockResolvedValue({
        status: "ok",
        data: [],
      });
      vi.mocked(commands.confirmSkillImport).mockResolvedValueOnce({
        status: "ok",
        data: { tool, createdCount: 1 },
      });
      const { client } = renderPage();
      expect(
        await screen.findByText(/尚无 Skill。可在下方全局目标卡片/),
      ).toBeVisible();
      expect(commands.discoverSkillImport).not.toHaveBeenCalled();
      const trigger = await screen.findByRole("button", {
        name: `检测并导入 ${toolMetadata(tool).label} 全局 Skills`,
      });
      expect(trigger).toBeEnabled();
      trigger.focus();
      fireEvent.click(trigger);
      const dialog = await screen.findByRole("dialog", {
        name: `导入 ${toolMetadata(tool).label} 全局 Skills`,
      });
      const newSkill = await within(dialog).findByRole("checkbox", {
        name: "导入 new-skill",
      });
      expect(commands.discoverSkillImport).toHaveBeenCalledExactlyOnceWith(
        tool,
      );
      for (const checkbox of within(dialog).getAllByRole("checkbox")) {
        expect(checkbox).not.toBeChecked();
      }
      expect(newSkill).toBeEnabled();
      expect(
        within(dialog).getByRole("checkbox", { name: "导入 fixture-skill" }),
      ).toBeDisabled();
      expect(
        within(dialog).getByRole("checkbox", { name: "导入 conflict-skill" }),
      ).toBeDisabled();
      expect(
        within(dialog).getByRole("checkbox", { name: "导入 invalid-skill" }),
      ).toBeDisabled();
      expect(within(dialog).getByText("已在中央库")).toBeVisible();
      expect(
        within(dialog).getByText("中央已有相同内容，不会新增副本或分配。"),
      ).toBeVisible();
      expect(
        within(dialog).getByText("同名技能内容不同，不会覆盖或改名。"),
      ).toBeVisible();
      expect(within(dialog).getByText("来源链接不可安全读取。")).toBeVisible();
      expect(
        within(dialog).getByRole("button", { name: "复制所选项（0）" }),
      ).toBeDisabled();
      if (tool === "codex") {
        expect(
          within(dialog).getByText("Codex 官方目录（正式同步目标）"),
        ).toBeVisible();
        expect(
          within(dialog).getByText("Codex Agents 通用目录（仅导入来源）"),
        ).toBeVisible();
        expect(within(dialog).getByText("来源目录不存在")).toBeVisible();
        expect(
          within(dialog).getByText("/isolated/custom-codex/skills"),
        ).toBeVisible();
        expect(
          within(dialog).getByText(/Codex .system 内置技能不在本次导入范围/),
        ).toBeVisible();
        expect(
          within(dialog).queryByRole("checkbox", { name: /\.system|imagegen/ }),
        ).not.toBeInTheDocument();
      }
      if (tool === "cursor") {
        expect(
          within(dialog).getByText("Cursor 官方目录（正式同步目标）"),
        ).toBeVisible();
        expect(
          within(dialog).getByText("Cursor Agents 通用目录（仅导入来源）"),
        ).toBeVisible();
        expect(
          within(dialog).getByText("/isolated/custom-cursor/skills"),
        ).toBeVisible();
      }
      fireEvent.click(newSkill);
      vi.mocked(commands.listSkills).mockResolvedValue({
        status: "ok",
        data: [{ ...skill, name: "new-skill", globalTools: [] }],
      });
      fireEvent.click(
        within(dialog).getByRole("button", { name: "复制所选项（1）" }),
      );
      await waitFor(() => expect(dialog).not.toBeInTheDocument());
      expect(commands.confirmSkillImport).toHaveBeenCalledExactlyOnceWith({
        previewId: nativeImport(tool).previewId,
        candidateIds: ["new"],
      });
      expect(
        await screen.findByRole("heading", { name: "new-skill" }),
      ).toBeVisible();
      const status = screen.getByText(
        /已复制 1 项 Skill 到中央库；原有安装未变，尚未自动分配或同步/,
      );
      expect(status).toHaveAttribute("role", "status");
      expect(
        screen.getAllByText(
          /已复制 1 项 Skill 到中央库；原有安装未变，尚未自动分配或同步/,
        ),
      ).toHaveLength(1);
      expect(
        screen.getByRole("button", { name: "Claude 全局未分配" }),
      ).toBeVisible();
      expect(trigger).toHaveFocus();
      expect(commands.listSkills).toHaveBeenCalledTimes(2);
      expect(commands.listGlobalSkillTargetStatuses).toHaveBeenCalledTimes(2);
      expect(commands.listSkillProjects).not.toHaveBeenCalled();
      expect(commands.listSkillProjectOptions).not.toHaveBeenCalled();
      expect(commands.discoverSkillImport).toHaveBeenCalledTimes(1);
      await waitFor(() =>
        expect(
          client.getQueryCache().findAll({ queryKey: ["skill-import"] }),
        ).toHaveLength(0),
      );
      expect(commands.importSkill).not.toHaveBeenCalled();
      expect(commands.setGlobalSkillAssignment).not.toHaveBeenCalled();
      expect(commands.setProjectSkillAssignment).not.toHaveBeenCalled();
      expect(commands.previewSkillSync).not.toHaveBeenCalled();
      expect(commands.applySkillPreview).not.toHaveBeenCalled();
    },
  );
  it("逐来源展示局部失败，保留另一来源的可选候选与全部重复入口", async () => {
    const data = nativeImport("codex");
    data.sources = data.sources.map((source) =>
      source.kind === "codex_agents"
        ? {
            ...source,
            status: "unavailable",
            diagnosticCode: "PERMISSION_DENIED",
            message: "无法读取此来源，另一来源已独立检测。",
          }
        : source,
    );
    data.candidates = data.candidates.map((candidate) =>
      candidate.candidateId === "new"
        ? {
            ...candidate,
            sourcePaths: [
              "/isolated/custom-codex/skills/new-skill",
              "/isolated/custom-codex/skills/alias-skill",
            ],
          }
        : candidate,
    );
    vi.mocked(commands.discoverSkillImport).mockResolvedValueOnce({
      status: "ok",
      data,
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "检测并导入 Codex 全局 Skills",
      }),
    );
    const checkbox = await screen.findByRole("checkbox", {
      name: "导入 new-skill",
    });
    expect(checkbox).toBeEnabled();
    expect(screen.getByText("来源不可用，检测未完成")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(
      screen.getByText("无法读取此来源，另一来源已独立检测。"),
    ).toBeVisible();
    expect(
      screen.getByText("/isolated/custom-codex/skills/alias-skill"),
    ).toBeVisible();
    fireEvent.click(checkbox);
    expect(
      screen.getByRole("button", { name: "复制所选项（1）" }),
    ).toBeEnabled();
    expect(commands.confirmSkillImport).not.toHaveBeenCalled();
  });
  it.each([
    [
      "missing",
      "来源目录不存在",
      "SKILL_IMPORT_SOURCE_MISSING",
      "请先检查来源目录。",
    ],
    [
      "empty",
      "没有可导入的用户技能",
      "SKILL_IMPORT_BUILTIN_EXCLUDED",
      "已排除内置技能集合。",
    ],
    [
      "unavailable",
      "来源不可用，检测未完成",
      "SKILL_IMPORT_BUDGET_EXCEEDED",
      "已达到扫描资源上限，检测结果不完整。",
    ],
  ] as const)(
    "%s 来源没有可选项时不允许确认且不宣称成功",
    async (status, label, diagnosticCode, message) => {
      vi.mocked(commands.discoverSkillImport).mockResolvedValueOnce({
        status: "ok",
        data: {
          tool: "codex",
          previewId: null,
          candidates: [],
          message: null,
          sources: [
            {
              kind: "codex_home",
              path: "/isolated/custom-codex/skills",
              status,
              diagnosticCode,
              message,
            },
          ],
        },
      });
      renderPage();
      fireEvent.click(
        await screen.findByRole("button", {
          name: "检测并导入 Codex 全局 Skills",
        }),
      );
      expect(await screen.findByText(label)).toBeVisible();
      const dialog = screen.getByRole("dialog");
      expect(within(dialog).getByText(message)).toBeVisible();
      expect(within(dialog).getByText(diagnosticCode)).toBeVisible();
      expect(within(dialog).queryAllByRole("checkbox")).toHaveLength(0);
      expect(
        within(dialog).getByRole("button", { name: "复制所选项（0）" }),
      ).toBeDisabled();
      expect(
        within(dialog).getByRole("button", { name: "重新检测" }),
      ).toBeEnabled();
      expect(
        within(dialog).queryByText(/导入成功|已复制 \d/),
      ).not.toBeInTheDocument();
      expect(commands.confirmSkillImport).not.toHaveBeenCalled();
    },
  );
  it("检测 RPC 失败保留结构化错误，只在用户重新检测时再读", async () => {
    vi.mocked(commands.discoverSkillImport).mockResolvedValueOnce({
      status: "error",
      error: {
        code: "PERMISSION_DENIED",
        message: "无法安全读取来源",
        recoverable: true,
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "PERMISSION_DENIED：无法安全读取来源",
    );
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      window.dispatchEvent(new Event("online"));
      await Promise.resolve();
    });
    expect(commands.discoverSkillImport).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole("button", { name: "复制所选项（0）" }),
    ).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "重新检测" }));
    expect(
      await screen.findByRole("checkbox", { name: "导入 new-skill" }),
    ).not.toBeChecked();
    expect(commands.discoverSkillImport).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it.each(["STALE_PREVIEW", "PREVIEW_ALREADY_CONSUMED"] as const)(
    "确认返回 %s 后禁止复用令牌，重新检测清空选择并使用新令牌",
    async (code) => {
      vi.mocked(commands.confirmSkillImport).mockResolvedValueOnce({
        status: "error",
        error: { code, message: "检测证据已过期", recoverable: true },
      });
      renderPage();
      const trigger = await screen.findByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      });
      trigger.focus();
      fireEvent.click(trigger);
      fireEvent.click(
        await screen.findByRole("checkbox", { name: "导入 new-skill" }),
      );
      fireEvent.click(screen.getByRole("button", { name: "复制所选项（1）" }));
      expect(await screen.findByRole("alert")).toHaveTextContent(
        `${code}：检测证据已过期 请重新检测后再确认。`,
      );
      expect(
        screen.getByRole("checkbox", { name: "导入 new-skill" }),
      ).toBeDisabled();
      fireEvent.submit(
        screen.getByRole("form", { name: "导入 Claude 全局 Skills" }),
      );
      expect(commands.confirmSkillImport).toHaveBeenCalledTimes(1);
      vi.mocked(commands.discoverSkillImport).mockResolvedValueOnce({
        status: "ok",
        data: { ...nativeImport("claude"), previewId: "fresh-preview" },
      });
      fireEvent.click(screen.getByRole("button", { name: "重新检测" }));
      const checkbox = await screen.findByRole("checkbox", {
        name: "导入 new-skill",
      });
      expect(checkbox).not.toBeChecked();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      fireEvent.click(checkbox);
      fireEvent.click(screen.getByRole("button", { name: "复制所选项（1）" }));
      await waitFor(() =>
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
      );
      expect(commands.confirmSkillImport).toHaveBeenLastCalledWith({
        previewId: "fresh-preview",
        candidateIds: ["new"],
      });
      expect(commands.discoverSkillImport).toHaveBeenCalledTimes(2);
      expect(trigger).toHaveFocus();
    },
  );
  it.each(["claude", "codex"] as const)(
    "关闭在途检测后打开 %s，旧响应不覆盖新弹窗并保持键盘焦点",
    async (nextTool) => {
      const pending =
        deferred<Awaited<ReturnType<typeof commands.discoverSkillImport>>>();
      vi.mocked(commands.discoverSkillImport).mockReturnValueOnce(
        pending.promise,
      );
      renderPage();
      const trigger = await screen.findByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      });
      trigger.focus();
      fireEvent.click(trigger);
      expect(
        await screen.findByText("正在检测已有全局 Skills…"),
      ).toHaveAttribute("role", "status");
      fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
      expect(trigger).toHaveFocus();
      const nextTrigger = screen.getByRole("button", {
        name: `检测并导入 ${nextTool === "claude" ? "Claude" : "Codex"} 全局 Skills`,
      });
      nextTrigger.focus();
      fireEvent.click(nextTrigger);
      expect(
        await screen.findByRole("checkbox", { name: "导入 new-skill" }),
      ).not.toBeChecked();
      await act(async () => {
        pending.resolve({
          status: "ok",
          data: {
            ...nativeImport("claude"),
            previewId: null,
            candidates: [],
            message: "不应显示的旧响应",
          },
        });
        await pending.promise;
      });
      expect(screen.queryByText("不应显示的旧响应")).not.toBeInTheDocument();
      expect(commands.discoverSkillImport).toHaveBeenCalledTimes(2);
      expect(commands.discoverSkillImport).toHaveBeenLastCalledWith(nextTool);
      const rescan = screen.getByRole("button", { name: "重新检测" });
      const dialog = screen.getByRole("dialog");
      dialog.focus();
      fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
      expect(rescan).toHaveFocus();
      fireEvent.keyDown(rescan, { key: "Tab" });
      expect(
        screen.getByRole("checkbox", { name: "导入 new-skill" }),
      ).toHaveFocus();
      fireEvent.click(screen.getByRole("checkbox", { name: "导入 new-skill" }));
      fireEvent.keyDown(dialog, { key: "Escape" });
      expect(nextTrigger).toHaveFocus();
      fireEvent.click(nextTrigger);
      expect(
        await screen.findByRole("checkbox", { name: "导入 new-skill" }),
      ).not.toBeChecked();
      fireEvent.click(screen.getByRole("button", { name: "取消" }));
      expect(nextTrigger).toHaveFocus();
      expect(commands.confirmSkillImport).not.toHaveBeenCalled();
    },
  );
  it("同步阻止重复提交、关闭和重扫，中央列表刷新阶段仍锁定且焦点不逃逸", async () => {
    const pending =
      deferred<Awaited<ReturnType<typeof commands.confirmSkillImport>>>();
    const refresh = deferred<Awaited<ReturnType<typeof commands.listSkills>>>();
    vi.mocked(commands.confirmSkillImport).mockReturnValueOnce(pending.promise);
    renderPage();
    const trigger = await screen.findByRole("button", {
      name: "检测并导入 Claude 全局 Skills",
    });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.click(
      await screen.findByRole("checkbox", { name: "导入 new-skill" }),
    );
    const form = screen.getByRole("form", { name: "导入 Claude 全局 Skills" });
    const dialog = screen.getByRole("dialog");
    const cancel = screen.getByRole("button", { name: "取消" });
    const rescan = screen.getByRole("button", { name: "重新检测" });
    act(() => {
      fireEvent.submit(form);
      fireEvent.submit(form);
      fireEvent.click(cancel);
      fireEvent.click(rescan);
      fireEvent.keyDown(dialog, { key: "Escape" });
    });
    await waitFor(() =>
      expect(commands.confirmSkillImport).toHaveBeenCalledTimes(1),
    );
    expect(dialog).toBeVisible();
    expect(dialog).toHaveFocus();
    for (const button of within(dialog).getAllByRole("button"))
      expect(button).toBeDisabled();
    expect(fireEvent.keyDown(dialog, { key: "Tab" })).toBe(false);
    expect(dialog).toHaveFocus();
    vi.mocked(commands.listSkills).mockReturnValueOnce(refresh.promise);
    await act(async () => {
      pending.resolve({
        status: "ok",
        data: { tool: "claude", createdCount: 1 },
      });
      await pending.promise;
    });
    expect(
      await screen.findByText("已复制到中央库，正在刷新列表…"),
    ).toHaveAttribute("role", "status");
    expect(
      screen.queryByText(
        /已复制 1 项 Skill 到中央库；原有安装未变，尚未自动分配或同步/,
      ),
    ).not.toBeInTheDocument();
    expect(dialog).toBeVisible();
    for (const button of within(dialog).getAllByRole("button"))
      expect(button).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "Escape" });
    fireEvent.submit(form);
    expect(fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true })).toBe(
      false,
    );
    expect(dialog).toHaveFocus();
    expect(commands.confirmSkillImport).toHaveBeenCalledTimes(1);
    expect(commands.discoverSkillImport).toHaveBeenCalledTimes(1);
    await act(async () => {
      refresh.resolve({ status: "ok", data: [skill] });
      await refresh.promise;
    });
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(
      await screen.findByText(
        /已复制 1 项 Skill 到中央库；原有安装未变，尚未自动分配或同步/,
      ),
    ).toHaveAttribute("role", "status");
    expect(trigger).toHaveFocus();
  });
});
