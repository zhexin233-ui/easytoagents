import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands, type SyncStatus } from "@/bindings/commands";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";
import {
  nativeImport,
  preview,
  renderPage,
  setupMocks,
} from "./skills-page.test-helpers";
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});
beforeEach(setupMocks);
describe("全局 Skills 检测与复制导入", () => {
  it("接管候选与复制候选分组独立选择，direct 模式也必须先审阅预览", async () => {
    const data = nativeImport("claude");
    data.candidates = data.candidates.map((candidate) =>
      candidate.candidateId === "existing"
        ? {
            ...candidate,
            takeoverEligible: true,
            takeoverEntryType: "external_symlink",
            reason: "中央库已有完全一致内容；可显式预览接管当前工具入口",
          }
        : candidate,
    );
    vi.mocked(commands.discoverSkillImport).mockResolvedValueOnce({
      status: "ok",
      data,
    });
    vi.mocked(commands.getAppSettings).mockResolvedValueOnce({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      }),
    );
    const importDialog = await screen.findByRole("dialog", {
      name: "导入 Claude 全局 Skills",
    });
    expect(
      await within(importDialog).findByRole("heading", {
        name: "复制到中央库",
      }),
    ).toBeVisible();
    expect(
      within(importDialog).getByRole("heading", { name: "接管正式目录" }),
    ).toBeVisible();
    expect(
      within(importDialog).getByRole("checkbox", { name: "导入 new-skill" }),
    ).toBeEnabled();
    const takeover = within(importDialog).getByRole("checkbox", {
      name: "接管 fixture-skill",
    });
    expect(takeover).toBeEnabled();
    expect(
      within(importDialog).getByRole("button", {
        name: "预览接管所选项（0）",
      }),
    ).toBeDisabled();
    fireEvent.click(takeover);
    fireEvent.click(
      within(importDialog).getByRole("button", {
        name: "预览接管所选项（1）",
      }),
    );
    await waitFor(() =>
      expect(commands.prepareSkillTakeover).toHaveBeenCalledExactlyOnceWith({
        previewId: data.previewId,
        candidateIds: ["existing"],
      }),
    );
    expect(commands.confirmSkillImport).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
    const previewDialog = await screen.findByRole("dialog", {
      name: "确认原生配置变更",
    });
    const status = await screen.findByText(
      "已为 1 项 Skill 准备接管；请审阅持久化预览后显式应用。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(
        "已为 1 项 Skill 准备接管；请审阅持久化预览后显式应用。",
      ),
    ).toHaveLength(1);
    expect(
      within(previewDialog).getByRole("button", { name: "应用这份预览" }),
    ).toBeEnabled();
    fireEvent.click(
      within(previewDialog).getByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledExactlyOnceWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: null,
      }),
    );
  });
  it("复制已完成但刷新失败时说明实际结果且不重新消费已确认令牌", async () => {
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "检测并导入 Claude 全局 Skills",
      }),
    );
    fireEvent.click(
      await screen.findByRole("checkbox", { name: "导入 new-skill" }),
    );
    vi.mocked(commands.listSkills).mockResolvedValueOnce({
      status: "error",
      error: {
        code: "DATABASE_ERROR",
        message: "中央列表暂不可读",
        recoverable: true,
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "复制所选项（1）" }));
    const dialog = screen.getByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "已复制到中央库，但列表刷新失败：DATABASE_ERROR：中央列表暂不可读",
    );
    expect(
      within(dialog).getByRole("button", { name: "复制所选项（1）" }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: "重新检测" }),
    ).toBeEnabled();
    fireEvent.submit(within(dialog).getByRole("form"));
    expect(commands.confirmSkillImport).toHaveBeenCalledTimes(1);
  });
});

describe("Skills 首次目标状态展示", () => {
  it("缺失与已有目录目标都精确展示已分配待同步，并仍需用户点击预览", async () => {
    vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValueOnce({
      status: "ok",
      data: [
        {
          tool: "claude",
          projectId: null,
          targetPath: "/isolated/home/.claude/skills",
          status: "external_non_owned_change",
          diagnosticCode: "SKILL_TARGET_INITIAL_SYNC_PENDING",
        },
        {
          tool: "codex",
          projectId: null,
          targetPath: "/isolated/home/.codex/skills",
          status: "missing",
          diagnosticCode: "SKILL_TARGET_INITIAL_SYNC_PENDING",
        },
      ],
    });
    renderPage();

    expect(await screen.findAllByText("○ 已分配，待同步")).toHaveLength(2);
    expect(
      screen.getAllByText(
        "分配已写入中央配置，但尚未写入工具目录；点击“预览全局同步”并确认应用。现有非受管内容会保留。",
      ),
    ).toHaveLength(2);
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();

    const section = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const claudeCard = section
      ? within(section).getByText("Claude").closest("article")
      : null;
    if (!claudeCard) throw new Error("未找到 Claude Skills 状态卡");
    fireEvent.click(
      within(claudeCard).getByRole("button", { name: "预览全局同步" }),
    );
    await waitFor(() =>
      expect(commands.previewSkillSync).toHaveBeenCalledExactlyOnceWith({
        tool: "claude",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });

  it("直接应用模式下首次待同步状态引导重新分配自动同步且隐藏手动按钮", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValue({
      status: "ok",
      data: [
        {
          tool: "claude",
          projectId: null,
          targetPath: "/isolated/home/.claude/skills",
          status: "missing",
          diagnosticCode: "SKILL_TARGET_INITIAL_SYNC_PENDING",
        },
      ],
    });
    renderPage();

    expect(await screen.findByText("○ 已分配，待同步")).toBeVisible();
    expect(
      screen.getAllByText(
        "分配已写入中央配置，但尚未写入工具目录；重新切换该分配可触发自动同步。现有非受管内容会保留。",
      ),
    ).toHaveLength(1);
    expect(
      screen.queryByRole("button", { name: "直接应用全局同步" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "预览全局同步" }),
    ).not.toBeInTheDocument();
  });

  it.each([
    ["external_non_owned_change", "EXTERNAL_NON_OWNED_CHANGE", "△ 非受管变更"],
    [
      "external_owned_change",
      "CENTRAL_SKILL_CONTENT_CHANGED",
      "! 受管内容冲突",
    ],
    ["parse_error", "SKILL_PARSE_ERROR", "! 格式错误"],
    ["permission_denied", "PERMISSION_DENIED", "! 权限不足"],
    ["target_type_changed", "TARGET_TYPE_CHANGED", "! 目标类型变化"],
  ] as const)(
    "真实 %s 继续展示原有诊断，不覆盖为首次目录",
    async (status, diagnosticCode, label) => {
      vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValueOnce({
        status: "ok",
        data: [
          {
            tool: "claude",
            projectId: null,
            targetPath: "/isolated/home/.claude/skills",
            status,
            diagnosticCode,
          },
        ],
      });
      renderPage();
      expect(await screen.findByText(label)).toBeVisible();
      expect(screen.getByText(diagnosticCode)).toBeVisible();
      expect(screen.queryByText("○ 未纳入同步管理")).not.toBeInTheDocument();
      expect(screen.queryByText("○ 空目录，待配置")).not.toBeInTheDocument();
    },
  );

  it.each([
    [
      "SKILL_TARGET_INITIAL_EMPTY",
      "○ 空目录，待配置",
      "目标目录为空，尚未配置同步；可先导入技能到中央库，再分配并预览同步。",
    ],
    [
      "SKILL_TARGET_INITIAL_UNMANAGED",
      "○ 未纳入同步管理",
      "已有目录尚未纳入同步管理；可检测其中的用户技能并复制到中央库。导入不会自动接管原有安装。",
    ],
  ] as const)(
    "%s 只解释首次目录状态，不宣称已发现或同步技能",
    async (diagnosticCode, label, description) => {
      vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValueOnce({
        status: "ok",
        data: [
          {
            tool: "claude",
            projectId: null,
            targetPath: "/isolated/home/.claude/skills",
            status: "external_non_owned_change",
            diagnosticCode,
          },
        ],
      });
      renderPage();
      expect(await screen.findByText(label)).toHaveClass("bg-amber-50");
      expect(screen.getByText(description)).toBeVisible();
      expect(
        screen.getByRole("button", { name: "检测并导入 Claude 全局 Skills" }),
      ).toBeEnabled();
      expect(
        screen.getByRole("button", { name: "预览全局同步" }),
      ).toBeEnabled();
      expect(commands.discoverSkillImport).not.toHaveBeenCalled();
      expect(commands.previewSkillSync).not.toHaveBeenCalled();
    },
  );

  const otherStatuses: SyncStatus[] = [
    "in_sync",
    "external_owned_change",
    "missing",
    "parse_error",
    "permission_denied",
    "policy_blocked",
    "untrusted",
    "target_type_changed",
    "failed",
  ];
  it.each(otherStatuses)(
    "%s 与首次诊断不匹配时保留既有展示和阻断",
    (status) => {
      for (const diagnostic of [
        "SKILL_TARGET_INITIAL_EMPTY",
        "SKILL_TARGET_INITIAL_UNMANAGED",
      ]) {
        expect(globalTargetStatusPresentation(status, diagnostic)).toEqual(
          globalTargetStatusPresentation(status, null),
        );
      }
    },
  );

  const pendingMismatchedStatuses: SyncStatus[] = [
    "in_sync",
    "external_owned_change",
    "parse_error",
    "permission_denied",
    "policy_blocked",
    "untrusted",
    "target_type_changed",
    "failed",
  ];
  it.each(pendingMismatchedStatuses)(
    "%s 与待同步诊断不匹配时保留既有展示和阻断",
    (status) => {
      expect(
        globalTargetStatusPresentation(
          status,
          "SKILL_TARGET_INITIAL_SYNC_PENDING",
        ),
      ).toEqual(globalTargetStatusPresentation(status, null));
    },
  );

  it.each([null, "EXTERNAL_NON_OWNED_CHANGE", "CENTRAL_SKILL_CONTENT_CHANGED"])(
    "普通诊断 %s 不覆盖真实漂移的共享展示",
    (diagnostic) => {
      expect(
        globalTargetStatusPresentation("external_non_owned_change", diagnostic),
      ).toEqual({ description: null, previewBlocked: false });
    },
  );
});
