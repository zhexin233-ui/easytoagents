/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合。 */
import {
  act,
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

import {
  commands,
  type PreviewPlan,
  type SkillDto,
  type SkillImportPreviewDto,
  type SyncStatus,
  type Tool,
} from "@/bindings/commands";
import { centralListLayoutStorageKeys } from "@/components/use-persisted-central-list-layout";
import { SkillsPage } from "@/features/skills/skills-page";
import { globalTargetStatusPresentation } from "@/lib/global-target-status-ui";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/bindings/commands", () => ({
  commands: {
    listSkills: vi.fn(),
    getSkill: vi.fn(),
    importSkill: vi.fn(),
    discoverSkillImport: vi.fn(),
    confirmSkillImport: vi.fn(),
    previewSkillContent: vi.fn(),
    deleteSkill: vi.fn(),
    setGlobalSkillAssignment: vi.fn(),
    setProjectSkillAssignment: vi.fn(),
    listSkillProjects: vi.fn(),
    listSkillProjectOptions: vi.fn(),
    listGlobalSkillTargetStatuses: vi.fn(),
    getAppSettings: vi.fn(),
    updateAppSettings: vi.fn(),
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

function nativeImport(tool: Tool): SkillImportPreviewDto {
  const root =
    tool === "claude"
      ? "/isolated/custom-claude/skills"
      : "/isolated/custom-codex/skills";
  return {
    previewId: `native-${tool}-preview`,
    tool,
    sources:
      tool === "claude"
        ? [
            {
              kind: "claude_global",
              path: root,
              status: "ready",
              diagnosticCode: null,
              message: null,
            },
          ]
        : [
            {
              kind: "codex_agents",
              path: "/isolated/home/.agents/skills",
              status: "missing",
              diagnosticCode: "SKILL_IMPORT_SOURCE_MISSING",
              message: null,
            },
            {
              kind: "codex_compatibility",
              path: root,
              status: "ready",
              diagnosticCode: "SKILL_IMPORT_BUILTIN_EXCLUDED",
              message: "已排除内置技能集合。",
            },
          ],
    candidates: [
      {
        candidateId: "new",
        name: "new-skill",
        description: "新的用户技能",
        sourcePaths: [`${root}/new-skill`],
        status: "importable",
        reason: null,
        existingSkillId: null,
      },
      {
        candidateId: "unselected",
        name: "unselected-skill",
        description: "不应导入的未勾选项",
        sourcePaths: [`${root}/unselected-skill`],
        status: "importable",
        reason: null,
        existingSkillId: null,
      },
      {
        candidateId: "existing",
        name: "fixture-skill",
        description: "与中央记录相同",
        sourcePaths: [`${root}/fixture-skill`],
        status: "already_imported",
        reason: null,
        existingSkillId: skill.id,
      },
      {
        candidateId: "conflict",
        name: "conflict-skill",
        description: "同名不同内容",
        sourcePaths: [`${root}/conflict-skill`],
        status: "name_conflict",
        reason: "同名技能内容不同，不会覆盖或改名。",
        existingSkillId: null,
      },
      {
        candidateId: "invalid",
        name: "invalid-skill",
        description: "无效技能入口",
        sourcePaths: [`${root}/invalid-skill`],
        status: "invalid",
        reason: "来源链接不可安全读取。",
        existingSkillId: null,
      },
    ],
    message: null,
  };
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((fulfill) => {
    resolve = fulfill;
  });
  return { promise, resolve };
}

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const rendered = render(
    <QueryClientProvider client={client}>
      <SkillsPage />
    </QueryClientProvider>,
  );
  return { ...rendered, client };
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(commands.discoverSkillImport).mockImplementation((tool) =>
    Promise.resolve({ status: "ok", data: nativeImport(tool) }),
  );
  vi.mocked(commands.confirmSkillImport).mockResolvedValue({
    status: "ok",
    data: { tool: "claude", createdCount: 1 },
  });
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
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: { applyMode: "preview_confirm" },
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
      expect(within(footer).getByRole("button", { name })).toBeVisible();
    }
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
    const invalidSkill: SkillDto = {
      ...skill,
      status: "invalid",
      diagnosticCode: "SKILL_PARSE_ERROR",
      globalTools: ["claude"],
    };
    const updatedSkill: SkillDto = {
      ...invalidSkill,
      globalTools: [],
      rowVersion: invalidSkill.rowVersion + 1,
    };
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
    expect(
      screen.getByText(
        "Skill 已复制到应用私有中央库；来源目录未修改，原生目标也尚未写入。",
      ),
    ).toBeVisible();
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
    const closeButton = screen.getByRole("button", { name: "关闭" });
    expect(closeButton).toHaveFocus();
    expect(fireEvent.keyDown(dialog, { key: "Tab" })).toBe(false);
    expect(closeButton).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
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
  });

  it("直接应用模式下无冲突 Skills 预览跳过对话框立即 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    renderPage();
    const section = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const card = section
      ? (await within(section).findByText("Claude")).closest("article")
      : null;
    if (!card) throw new Error("未找到 Claude Skills 状态卡");
    const applyButton = within(card).getByRole("button", {
      name: "直接应用全局同步",
    });
    expect(applyButton).toBeEnabled();
    fireEvent.click(applyButton);
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: null,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText(/已应用 1 个 Skills 目标/)).toBeVisible();
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
    expect(
      await screen.findByText("当前工具没有需要同步的全局 Skill。"),
    ).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });

  it.each([
    ["Claude", "claude", false, []],
    ["Codex", "codex", true, ["claude", "codex"]],
  ] as const)(
    "%s 全局分配更新中央配置并刷新列表与目标，不隐式预览或 Apply",
    async (toolLabel, tool, assigned, updatedTools) => {
      const updatedSkill = {
        ...skill,
        globalTools: [...updatedTools],
        rowVersion: skill.rowVersion + 1,
      };
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
              targetPath: "/isolated/home/.agents/skills",
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
                  : "/isolated/home/.agents/skills",
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
      expect(
        await screen.findByText(
          "全局分配已更新；这只改变中央配置，分配或取消分配不会自动写入工具目录。请预览全局同步并确认应用。",
        ),
      ).toBeVisible();
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

  it("直接应用模式下分配切换自动同步并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    const updatedSkill: SkillDto = {
      ...skill,
      globalTools: ["claude", "codex"],
      rowVersion: skill.rowVersion + 1,
    };
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

  it.each(["claude", "codex"] as const)(
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
        name: `检测并导入 ${tool === "claude" ? "Claude" : "Codex"} 全局 Skills`,
      });
      expect(trigger).toBeEnabled();
      trigger.focus();
      fireEvent.click(trigger);
      const dialog = await screen.findByRole("dialog", {
        name: `导入 ${tool === "claude" ? "Claude" : "Codex"} 全局 Skills`,
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
        within(dialog).getByRole("button", { name: "确认导入所选项（0）" }),
      ).toBeDisabled();
      if (tool === "codex") {
        expect(
          within(dialog).getByText("Codex 用户目录（正式同步目标）"),
        ).toBeVisible();
        expect(
          within(dialog).getByText("Codex 兼容目录（仅导入来源）"),
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
      fireEvent.click(newSkill);
      vi.mocked(commands.listSkills).mockResolvedValue({
        status: "ok",
        data: [{ ...skill, name: "new-skill", globalTools: [] }],
      });
      fireEvent.click(
        within(dialog).getByRole("button", { name: "确认导入所选项（1）" }),
      );
      await waitFor(() => expect(dialog).not.toBeInTheDocument());
      expect(commands.confirmSkillImport).toHaveBeenCalledExactlyOnceWith({
        previewId: nativeImport(tool).previewId,
        candidateIds: ["new"],
      });
      expect(
        await screen.findByRole("heading", { name: "new-skill" }),
      ).toBeVisible();
      expect(
        screen.getByText(
          /已复制 1 项 Skill 到中央库；原有安装未变，尚未自动分配或同步/,
        ),
      ).toBeVisible();
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
      screen.getByRole("button", { name: "确认导入所选项（1）" }),
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
              kind: "codex_compatibility",
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
        within(dialog).getByRole("button", { name: "确认导入所选项（0）" }),
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
      screen.getByRole("button", { name: "确认导入所选项（0）" }),
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
      fireEvent.click(
        screen.getByRole("button", { name: "确认导入所选项（1）" }),
      );
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
      fireEvent.click(
        screen.getByRole("button", { name: "确认导入所选项（1）" }),
      );
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
      const close = screen.getByRole("button", { name: "关闭 Skills 导入" });
      const rescan = screen.getByRole("button", { name: "重新检测" });
      close.focus();
      fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
      expect(rescan).toHaveFocus();
      fireEvent.keyDown(rescan, { key: "Tab" });
      expect(close).toHaveFocus();
      const dialog = screen.getByRole("dialog");
      dialog.focus();
      fireEvent.keyDown(dialog, { key: "Tab" });
      expect(close).toHaveFocus();
      dialog.focus();
      fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
      expect(rescan).toHaveFocus();
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
    const close = screen.getByRole("button", { name: "关闭 Skills 导入" });
    const rescan = screen.getByRole("button", { name: "重新检测" });
    act(() => {
      fireEvent.submit(form);
      fireEvent.submit(form);
      fireEvent.click(close);
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
    expect(trigger).toHaveFocus();
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
    fireEvent.click(
      screen.getByRole("button", { name: "确认导入所选项（1）" }),
    );
    const dialog = screen.getByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "已复制到中央库，但列表刷新失败：DATABASE_ERROR：中央列表暂不可读",
    );
    expect(
      within(dialog).getByRole("button", { name: "确认导入所选项（1）" }),
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
          targetPath: "/isolated/home/.agents/skills",
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
