/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合。 */
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type PreviewPlan, type SkillDto } from "@/bindings/commands";
import { SkillsPage } from "@/features/skills/skills-page";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/bindings/commands", () => ({
  commands: {
    listSkills: vi.fn(),
    getSkill: vi.fn(),
    importSkill: vi.fn(),
    previewSkillContent: vi.fn(),
    deleteSkill: vi.fn(),
    setGlobalSkillAssignment: vi.fn(),
    setProjectSkillAssignment: vi.fn(),
    listSkillProjects: vi.fn(),
    listSkillProjectOptions: vi.fn(),
    listGlobalSkillTargetStatuses: vi.fn(),
    previewSkillSync: vi.fn(),
    applySkillPreview: vi.fn(),
  },
}));

const skill: SkillDto = {
  id: "00000000-0000-4000-8000-000000000601",
  name: "fixture-skill",
  sourcePath: "/isolated/source/fixture-skill",
  centralPath: "/isolated/private/skills/00000000-0000-4000-8000-000000000601",
  contentHash: "a".repeat(64),
  description: "隔离测试 Skill",
  status: "ready",
  diagnosticCode: null,
  globalTools: ["claude"],
  rowVersion: 2,
};

const preview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000699",
  scope: "global",
  projectId: null,
  dbVersion: 3,
  warningCodes: [],
  targets: [
    {
      targetId: "00000000-0000-4000-8000-000000000698",
      descriptor: {
        tool: "claude",
        artifactKind: "skill",
        scope: "global",
        projectRoot: null,
        path: "/isolated/home/.claude/skills",
        format: "symlink_directory",
        managedSelectorRoots: ["$children"],
        sensitiveSelectors: [],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "managed_children_only",
      },
      ownership: { kind: "symlink_names", paths: ["fixture-skill"] },
      changeKind: "add",
      status: "missing",
      currentFullHash: null,
      currentManagedHash: null,
      desiredManagedHash: "b".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: {
        before: null,
        after: { "fixture-skill": { targetType: "symlink" } },
      },
      warningCodes: [],
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
      <SkillsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listSkills).mockResolvedValue({
    status: "ok",
    data: [skill],
  });
  vi.mocked(commands.listSkillProjects).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.listSkillProjectOptions).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.listGlobalSkillTargetStatuses).mockResolvedValue({
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
        targetPath: "/isolated/home/.agents/skills",
        status: "missing",
        diagnosticCode: null,
      },
    ],
  });
  vi.mocked(commands.previewSkillSync).mockResolvedValue({
    status: "ok",
    data: preview,
  });
  vi.mocked(commands.applySkillPreview).mockResolvedValue({
    status: "ok",
    data: {
      runId: preview.previewId,
      status: "succeeded",
      appliedTargets: 1,
      snapshotCount: 2,
    },
  });
});

afterEach(cleanup);

describe("SkillsPage", () => {
  it("分别展示列表、目标和项目的加载与空状态", () => {
    vi.mocked(commands.listSkills).mockReturnValue(
      new Promise(() => undefined),
    );
    vi.mocked(commands.listGlobalSkillTargetStatuses).mockReturnValue(
      new Promise(() => undefined),
    );
    vi.mocked(commands.listSkillProjects).mockReturnValue(
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
    expect(screen.getByText("正在读取已登记项目…")).toHaveAttribute(
      "role",
      "status",
    );
  });

  it("分别展示目标错误、项目错误和目标冲突诊断", async () => {
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
    vi.mocked(commands.listSkillProjects).mockResolvedValue({
      status: "error",
      error: rpcError,
    });
    renderPage();

    expect((await screen.findAllByRole("alert")).length).toBeGreaterThanOrEqual(
      2,
    );
    expect(screen.getAllByText("CONFLICT：隔离冲突")).toHaveLength(2);

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
    vi.mocked(commands.listSkillProjects).mockResolvedValue({
      status: "ok",
      data: [],
    });
    renderPage();
    expect(
      await screen.findByText("CENTRAL_SKILL_CONTENT_CHANGED"),
    ).toBeVisible();
    expect(screen.getByText("external_owned_change")).toBeVisible();
  });

  it("通过目录选择器显式选择来源并只调用生成的导入 command", async () => {
    vi.mocked(open).mockResolvedValue("/isolated/source/new-skill");
    vi.mocked(commands.importSkill).mockResolvedValue({
      status: "ok",
      data: skill,
    });
    renderPage();
    fireEvent.click(screen.getByRole("button", { name: "选择目录" }));
    expect(
      await screen.findByDisplayValue("/isolated/source/new-skill"),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "复制到中央库" }));
    await waitFor(() =>
      expect(commands.importSkill).toHaveBeenCalledWith({
        sourcePath: "/isolated/source/new-skill",
      }),
    );
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
    expect(
      await screen.findByText("内容预览失败：CONFLICT：中央 Skill 已变化"),
    ).toHaveAttribute("role", "alert");

    fireEvent.click(screen.getByRole("button", { name: "移出中央库" }));
    expect(
      await screen.findByText("移出中央库失败：CONFLICT：中央 Skill 已变化"),
    ).toHaveAttribute("role", "alert");
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
    expect(screen.getByRole("button", { name: "关闭" })).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });

  it("把全局继承的项目 Skill 显示为只读且不可重复选择", async () => {
    vi.mocked(commands.listSkillProjects).mockResolvedValue({
      status: "ok",
      data: [
        {
          id: "00000000-0000-4000-8000-000000000610",
          displayName: "隔离项目",
          rootPath: "/isolated/project",
          codexTrustStatus: "trusted",
          rowVersion: 4,
        },
      ],
    });
    vi.mocked(commands.listSkillProjectOptions).mockResolvedValue({
      status: "ok",
      data: [
        {
          skillId: skill.id,
          name: skill.name,
          status: "ready",
          state: "inherited",
          selectable: false,
          rowVersion: skill.rowVersion,
        },
      ],
    });
    renderPage();
    await screen.findByRole("option", { name: "隔离项目" });
    fireEvent.change(await screen.findByLabelText("项目"), {
      target: { value: "00000000-0000-4000-8000-000000000610" },
    });
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledWith({
        projectId: "00000000-0000-4000-8000-000000000610",
        tool: "claude",
      }),
    );
    const inherited = await screen.findByText(/全局继承（只读）/);
    const checkbox = inherited.closest("label")?.querySelector("input");
    expect(checkbox).toBeDisabled();
    expect(commands.setProjectSkillAssignment).not.toHaveBeenCalled();
  });

  it("Codex 项目未受信任时独立阻止项目 Skill 预览", async () => {
    vi.mocked(commands.listSkillProjects).mockResolvedValue({
      status: "ok",
      data: [
        {
          id: "00000000-0000-4000-8000-000000000611",
          displayName: "未受信任项目",
          rootPath: "/isolated/untrusted-project",
          codexTrustStatus: "untrusted",
          rowVersion: 1,
        },
      ],
    });
    renderPage();
    await screen.findByRole("option", { name: "未受信任项目" });
    fireEvent.change(await screen.findByLabelText("项目"), {
      target: { value: "00000000-0000-4000-8000-000000000611" },
    });
    fireEvent.change(screen.getByLabelText("工具"), {
      target: { value: "codex" },
    });

    expect(await screen.findByText(/Codex 项目尚未受信任/)).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByRole("button", { name: "预览项目同步" })).toBeDisabled();
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
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
    fireEvent.click(within(card).getByRole("button", { name: "预览全局同步" }));
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
  });
});
