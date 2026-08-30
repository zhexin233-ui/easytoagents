/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type McpProjectOptionDto,
  type PreviewPlan,
  type ProjectDto,
  type PromptProfileDto,
  type SkillProjectOptionDto,
} from "@/bindings/commands";
import claudeIconUrl from "@/assets/brand/claude-icon-square.svg";
import codexIconUrl from "@/assets/brand/codex-icon-light.png";
import { ProjectDetailPage } from "@/features/projects/project-detail-page";

vi.mock("@/bindings/commands", () => ({
  commands: {
    getProject: vi.fn(),
    listMcpProjectOptions: vi.fn(),
    listSkillProjectOptions: vi.fn(),
    setProjectMcpAssignment: vi.fn(),
    setProjectSkillAssignment: vi.fn(),
    previewMcpSync: vi.fn(),
    previewSkillSync: vi.fn(),
    applyMcpPreview: vi.fn(),
    applySkillPreview: vi.fn(),
    readoptMcpTarget: vi.fn(),
    getAppSettings: vi.fn(),
    updateAppSettings: vi.fn(),
    getPromptProjectAssignment: vi.fn(),
    setPromptProjectAssignment: vi.fn(),
    listPromptProfiles: vi.fn(),
    previewPromptSync: vi.fn(),
    applyProfilePreview: vi.fn(),
  },
}));

const project: ProjectDto = {
  id: "00000000-0000-4000-8000-000000000711",
  displayName: "详情项目",
  rootPath: "/isolated/projects/detail",
  pathStatus: "valid",
  gitStatus: "repository",
  codexTrustStatus: "trusted",
  claudePolicyStatus: "allowed",
  targets: [
    {
      tool: "claude",
      artifactKind: "mcp",
      targetPath: "/isolated/projects/detail/.mcp.json",
      capability: "supported",
      policy: "allowed",
      trust: "not_required",
      status: "missing",
      diagnosticCode: null,
    },
    {
      tool: "claude",
      artifactKind: "skill",
      targetPath: "/isolated/projects/detail/.claude/skills",
      capability: "supported",
      policy: "allowed",
      trust: "not_required",
      status: "missing",
      diagnosticCode: null,
    },
    {
      tool: "codex",
      artifactKind: "mcp",
      targetPath: "/isolated/projects/detail/.codex/config.toml",
      capability: "supported",
      policy: "allowed",
      trust: "trusted",
      status: "missing",
      diagnosticCode: null,
    },
    {
      tool: "codex",
      artifactKind: "skill",
      targetPath: "/isolated/projects/detail/.codex/skills",
      capability: "supported",
      policy: "allowed",
      trust: "trusted",
      status: "missing",
      diagnosticCode: null,
    },
  ],
  lastScannedAt: "2026-08-24T10:00:00Z",
  rowVersion: 7,
};

const mcpOptions: McpProjectOptionDto[] = [
  {
    mcpId: "00000000-0000-4000-8000-000000000712",
    name: "全局 MCP",
    enabled: true,
    state: "inherited",
    selectable: false,
    rowVersion: 2,
  },
  {
    mcpId: "00000000-0000-4000-8000-000000000713",
    name: "项目 MCP",
    enabled: true,
    state: "available",
    selectable: true,
    rowVersion: 4,
  },
];

const skillOptions: SkillProjectOptionDto[] = [
  {
    skillId: "00000000-0000-4000-8000-000000000716",
    name: "全局 Skill",
    status: "ready",
    state: "inherited",
    selectable: false,
    rowVersion: 3,
  },
  {
    skillId: "00000000-0000-4000-8000-000000000717",
    name: "项目 Skill",
    status: "ready",
    state: "available",
    selectable: true,
    rowVersion: 6,
  },
];

const preview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000714",
  scope: "project",
  projectId: project.id,
  dbVersion: 7,
  warningCodes: ["GIT_TRACKED"],
  targets: [
    {
      targetId: "00000000-0000-4000-8000-000000000715",
      descriptor: {
        tool: "claude",
        artifactKind: "mcp",
        scope: "project",
        projectRoot: project.rootPath,
        path: "/isolated/projects/detail/.mcp.json",
        format: "json",
        managedSelectorRoots: ["mcpServers"],
        sensitiveSelectors: ["mcpServers/*/headers"],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "reject",
      },
      ownership: { kind: "selectors", paths: [["mcpServers", "项目 MCP"]] },
      changeKind: "add",
      status: "missing",
      currentFullHash: null,
      currentManagedHash: null,
      desiredManagedHash: "a".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: { after: { mcpServers: { "项目 MCP": {} } } },
      warningCodes: ["GIT_TRACKED"],
      baselineMismatchedItems: [],
      readoptAvailable: false,
      errorCode: null,
      git: {
        isRepository: true,
        tracked: true,
        ignored: false,
        ignoredByLocalExclude: false,
      },
      excludeFromGit: false,
    },
  ],
};

const skillPreview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000718",
  scope: "project",
  projectId: project.id,
  dbVersion: 8,
  warningCodes: [],
  targets: [
    {
      targetId: "00000000-0000-4000-8000-000000000719",
      descriptor: {
        tool: "claude",
        artifactKind: "skill",
        scope: "project",
        projectRoot: project.rootPath,
        path: "/isolated/projects/detail/.claude/skills",
        format: "symlink_directory",
        managedSelectorRoots: ["$children"],
        sensitiveSelectors: [],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "managed_children_only",
      },
      ownership: { kind: "symlink_names", paths: ["项目 Skill"] },
      changeKind: "add",
      status: "missing",
      currentFullHash: null,
      currentManagedHash: null,
      desiredManagedHash: "b".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: {
        before: null,
        after: { "项目 Skill": { targetType: "symlink" } },
      },
      warningCodes: [],
      baselineMismatchedItems: [],
      readoptAvailable: false,
      errorCode: null,
      git: {
        isRepository: true,
        tracked: false,
        ignored: false,
        ignoredByLocalExclude: false,
      },
      excludeFromGit: true,
    },
  ],
};

const promptProfileFixture: PromptProfileDto = {
  id: "00000000-0000-4000-8000-000000000731",
  tool: "claude",
  name: "项目提示词",
  body: "# 项目规则",
  isActive: false,
  importedFromPath: null,
  rowVersion: 2,
};

const promptPreview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000732",
  scope: "project",
  projectId: project.id,
  dbVersion: 9,
  warningCodes: [],
  targets: [
    {
      targetId: "00000000-0000-4000-8000-000000000733",
      descriptor: {
        tool: "claude",
        artifactKind: "prompt",
        scope: "project",
        projectRoot: project.rootPath,
        path: "/isolated/projects/detail/CLAUDE.md",
        format: "markdown",
        managedSelectorRoots: ["$document"],
        sensitiveSelectors: [],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "reject",
      },
      ownership: { kind: "whole_document" },
      changeKind: "add",
      status: "missing",
      currentFullHash: null,
      currentManagedHash: null,
      desiredManagedHash: "d".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: { before: null, after: "# 项目规则" },
      warningCodes: [],
      baselineMismatchedItems: [],
      readoptAvailable: false,
      errorCode: null,
      git: {
        isRepository: true,
        tracked: false,
        ignored: false,
        ignoredByLocalExclude: false,
      },
      excludeFromGit: false,
    },
  ],
};

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const rendered = render(
    <MemoryRouter initialEntries={[`/projects/${project.id}`]}>
      <QueryClientProvider client={client}>
        <Routes>
          <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
  return { ...rendered, client };
}

function createDeferred<T>() {
  let promiseResolve: ((value: T) => void) | undefined;
  const promise = new Promise<T>((resolve) => {
    promiseResolve = resolve;
  });
  return {
    promise,
    resolve(value: T) {
      if (!promiseResolve) throw new Error("延迟 Promise 尚未初始化");
      promiseResolve(value);
    },
  };
}

describe("ProjectDetailPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "ok",
      data: project,
    });
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "preview_confirm" },
    });
    vi.mocked(commands.listMcpProjectOptions).mockImplementation((input) =>
      Promise.resolve({
        status: "ok",
        data: input.tool === "claude" ? mcpOptions : [],
      }),
    );
    vi.mocked(commands.listSkillProjectOptions).mockImplementation((input) =>
      Promise.resolve({
        status: "ok",
        data: input.tool === "claude" ? skillOptions : [],
      }),
    );
    vi.mocked(commands.setProjectMcpAssignment).mockResolvedValue({
      status: "ok",
      data: {
        id: mcpOptions[1]?.mcpId ?? "",
        name: "项目 MCP",
        transport: "stdio",
        command: "fixture",
        args: [],
        url: null,
        headerNames: [],
        envNames: [],
        redactedExtra: {},
        enabled: true,
        globalTools: [],
        rowVersion: 5,
      },
    });
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: preview,
    });
    vi.mocked(commands.setProjectSkillAssignment).mockResolvedValue({
      status: "ok",
      data: {
        id: skillOptions[1]?.skillId ?? "",
        name: "项目 Skill",
        sourcePath: "/isolated/source/project-skill",
        centralPath: "/isolated/private/project-skill",
        contentHash: "c".repeat(64),
        description: "项目测试 Skill",
        status: "ready",
        diagnosticCode: null,
        globalTools: [],
        rowVersion: 7,
      },
    });
    vi.mocked(commands.previewSkillSync).mockResolvedValue({
      status: "ok",
      data: skillPreview,
    });
    vi.mocked(commands.applyMcpPreview).mockResolvedValue({
      status: "ok",
      data: {
        runId: "run-1",
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
    vi.mocked(commands.applySkillPreview).mockResolvedValue({
      status: "ok",
      data: {
        runId: "run-2",
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
  });

  afterEach(cleanup);

  it("直接应用模式下无冲突项目预览跳过对话框立即 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    renderPage();
    // 预览自带 GIT_TRACKED 警告；警告不阻止直接应用，与对话框行为一致。
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 直接应用" }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(
      await screen.findByText(
        "项目原生配置已通过持久化预览应用并完成写后验证。",
      ),
    ).toBeVisible();
  });

  it("初始未纳管诊断呈现中性徽章与说明而非非受管变更警告", async () => {
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "ok",
      data: {
        ...project,
        targets: project.targets.map((target) =>
          target.tool === "codex" && target.artifactKind === "mcp"
            ? {
                ...target,
                status: "external_non_owned_change" as const,
                diagnosticCode: "PROJECT_TARGET_INITIAL_UNMANAGED",
              }
            : target,
        ),
      },
    });
    renderPage();
    expect(await screen.findByText("○ 未纳管")).toBeVisible();
    expect(
      screen.getByText(
        "该目标由外部维护，本项目暂无需要写入的项目级配置；全局配置持续继承。",
      ),
    ).toBeVisible();
    expect(screen.queryByText(/非受管变更/)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/诊断：PROJECT_TARGET_INITIAL_UNMANAGED/),
    ).not.toBeInTheDocument();
  });

  it("直接应用模式下启用项目追加自动同步并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "项目 MCP MCP 项目追加" }),
    );
    await waitFor(() =>
      expect(commands.setProjectMcpAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        mcpId: mcpOptions[1]?.mcpId,
        assigned: true,
        mcpRowVersion: mcpOptions[1]?.rowVersion,
        projectRowVersion: project.rowVersion,
      }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(
      await screen.findByText(
        "项目原生配置已通过持久化预览应用并完成写后验证。",
      ),
    ).toBeVisible();
  });

  it("直接应用模式下冲突项目预览回退为人工确认且 Apply 禁用", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    const baseTarget = preview.targets[0];
    if (!baseTarget) throw new Error("预览 fixture 缺少目标");
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: {
        ...preview,
        targets: [
          {
            ...baseTarget,
            changeKind: "conflict",
            status: "external_owned_change",
          },
        ],
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 直接应用" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "应用这份预览" })).toBeDisabled();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });

  it("冲突项目预览展示不匹配条目并支持以当前内容重新接管", async () => {
    const baseTarget = preview.targets[0];
    if (!baseTarget) throw new Error("预览 fixture 缺少目标");
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: {
        ...preview,
        targets: [
          {
            ...baseTarget,
            changeKind: "conflict",
            status: "external_owned_change",
            warningCodes: ["MANAGED_ITEM_BASELINE_MISMATCH"],
            baselineMismatchedItems: ["项目 MCP"],
            readoptAvailable: true,
            errorCode: "CONFLICT",
          },
        ],
      },
    });
    vi.mocked(commands.readoptMcpTarget).mockResolvedValue({
      status: "ok",
      data: {
        targetPath: "/isolated/projects/detail/.mcp.json",
        updatedItemCount: 1,
        removedItemCount: 0,
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    expect(
      await screen.findByText("内容不一致的受管条目：项目 MCP"),
    ).toBeVisible();

    fireEvent.click(
      screen.getByRole("button", {
        name: "以当前内容重新接管 /isolated/projects/detail/.mcp.json",
      }),
    );
    await waitFor(() =>
      expect(commands.readoptMcpTarget).toHaveBeenCalledWith({
        tool: "claude",
        projectId: project.id,
      }),
    );
    expect(await screen.findByText(/请再次点击同步按钮完成写入/)).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
  });

  it("默认选择 Claude MCP，并可通过两组按钮切换四种管理组合", async () => {
    renderPage();

    const resourceGroup = await screen.findByRole("group", {
      name: "项目资源管理视图",
    });
    const mcpButton = within(resourceGroup).getByRole("button", {
      name: "管理项目 MCP",
    });
    const skillButton = within(resourceGroup).getByRole("button", {
      name: "管理项目 Skill",
    });
    const platformGroup = screen.getByRole("group", {
      name: "项目平台管理视图",
    });
    const claudeButton = within(platformGroup).getByRole("button", {
      name: "管理 Claude 项目资源",
    });
    const codexButton = within(platformGroup).getByRole("button", {
      name: "管理 Codex 项目资源",
    });
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveAttribute("aria-pressed", "false");
    expect(claudeButton).toHaveAttribute("aria-pressed", "true");
    expect(claudeButton).toHaveAttribute("title", "管理 Claude 项目资源");
    expect(codexButton).toHaveAttribute("aria-pressed", "false");
    expect(codexButton).toHaveAttribute("title", "管理 Codex 项目资源");
    expect(claudeButton.querySelector("img")).toHaveAttribute(
      "src",
      claudeIconUrl,
    );
    expect(codexButton.querySelector("img")).toHaveAttribute(
      "src",
      codexIconUrl,
    );
    expect(claudeButton.querySelector("img")).toHaveAttribute("alt", "");
    expect(claudeButton.querySelector("img")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(codexButton.querySelector("img")).toHaveAttribute("alt", "");
    expect(codexButton.querySelector("img")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(claudeButton.querySelector("svg")).toBeNull();
    expect(codexButton.querySelector("svg")).toBeNull();
    expect(claudeButton.firstElementChild).toHaveClass("opacity-100");
    expect(codexButton.firstElementChild).toHaveClass(
      "opacity-25",
      "grayscale",
    );
    expect(
      await screen.findByRole("heading", { name: "Claude MCP 项目追加" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: /Codex .*项目追加/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Skills" }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(1),
    );
    expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "claude",
    });
    expect(commands.listSkillProjectOptions).not.toHaveBeenCalled();

    fireEvent.click(skillButton);
    expect(mcpButton).toHaveAttribute("aria-pressed", "false");
    expect(skillButton).toHaveAttribute("aria-pressed", "true");
    expect(
      await screen.findByRole("heading", { name: "Claude Skill 项目追加" }),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "Skills" })).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "MCP" }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(1),
    );
    expect(commands.listSkillProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "claude",
    });

    fireEvent.click(codexButton);
    expect(claudeButton).toHaveAttribute("aria-pressed", "false");
    expect(codexButton).toHaveAttribute("aria-pressed", "true");
    expect(
      await screen.findByRole("heading", { name: "Codex Skill 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(2),
    );
    expect(commands.listSkillProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "codex",
    });

    fireEvent.click(mcpButton);
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveAttribute("aria-pressed", "false");
    expect(
      await screen.findByRole("heading", { name: "Codex MCP 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(2),
    );
    expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "codex",
    });

    fireEvent.click(claudeButton);
    expect(claudeButton).toHaveAttribute("aria-pressed", "true");
    expect(codexButton).toHaveAttribute("aria-pressed", "false");
    expect(
      await screen.findByRole("heading", { name: "Claude MCP 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(3),
    );
    expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "claude",
    });
  });

  it("切换资源或平台会重置尚未提交的 Git exclude 选择", async () => {
    renderPage();
    await screen.findByRole("heading", { name: "Claude MCP 项目追加" });

    const mcpExclude = screen.getByRole("checkbox", {
      name: /若目标是应用新建且未跟踪/,
    });
    fireEvent.click(mcpExclude);
    expect(mcpExclude).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    const skillExclude = screen.getByRole("checkbox", {
      name: /若目标是应用新建且未跟踪/,
    });
    expect(skillExclude).not.toBeChecked();
    fireEvent.click(skillExclude);
    expect(skillExclude).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 MCP" }));
    expect(
      screen.getByRole("checkbox", {
        name: /若目标是应用新建且未跟踪/,
      }),
    ).not.toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    expect(
      screen.getByRole("checkbox", {
        name: /若目标是应用新建且未跟踪/,
      }),
    ).not.toBeChecked();

    fireEvent.click(
      screen.getByRole("button", { name: "管理 Codex 项目资源" }),
    );
    const codexSection = screen
      .getByRole("heading", { name: "Codex Skill 项目追加" })
      .closest("section");
    if (!codexSection) throw new Error("未找到 Codex Skill 项目管理区");
    const codexExclude = within(codexSection).getByRole("checkbox", {
      name: /若目标是应用新建且未跟踪/,
    });
    expect(codexExclude).not.toBeChecked();
    fireEvent.click(codexExclude);
    expect(codexExclude).toBeChecked();

    fireEvent.click(
      screen.getByRole("button", { name: "管理 Claude 项目资源" }),
    );
    expect(
      screen.getByRole("checkbox", {
        name: /若目标是应用新建且未跟踪/,
      }),
    ).not.toBeChecked();
  });

  it("切换组合会清理消息与预览 mutation，并忽略旧组合的迟到结果", async () => {
    const delayedPreview =
      createDeferred<Awaited<ReturnType<typeof commands.previewMcpSync>>>();
    vi.mocked(commands.previewMcpSync)
      .mockResolvedValueOnce({
        status: "ok",
        data: { ...preview, targets: [] },
      })
      .mockReturnValueOnce(delayedPreview.promise);
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    expect(
      await screen.findByText(
        "该项目只有全局继承 MCP，不需要创建项目配置文件。",
      ),
    ).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    expect(
      screen.queryByText("该项目只有全局继承 MCP，不需要创建项目配置文件。"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "管理项目 MCP" }));
    const claudePreviewButton = await screen.findByRole("button", {
      name: "Claude MCP 同步预览",
    });
    fireEvent.click(claudePreviewButton);
    await waitFor(() => expect(claudePreviewButton).toBeDisabled());

    fireEvent.click(
      screen.getByRole("button", { name: "管理 Codex 项目资源" }),
    );
    const codexPreviewButton = await screen.findByRole("button", {
      name: "Codex MCP 同步预览",
    });
    expect(codexPreviewButton).toBeEnabled();

    await act(async () => {
      delayedPreview.resolve({ status: "ok", data: preview });
      await delayedPreview.promise;
    });
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
  });

  it("旧组合的迟到 Apply 不会关闭当前组合的新预览或写入旧消息", async () => {
    const delayedApply =
      createDeferred<Awaited<ReturnType<typeof commands.applyMcpPreview>>>();
    vi.mocked(commands.applyMcpPreview).mockReturnValueOnce(
      delayedApply.promise,
    );
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledTimes(1),
    );
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    fireEvent.click(
      screen.getByRole("button", { name: "管理 Codex 项目资源" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Codex MCP 同步预览" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();

    await act(async () => {
      delayedApply.resolve({
        status: "ok",
        data: {
          runId: "late-claude-apply",
          status: "succeeded",
          appliedTargets: 1,
          snapshotCount: 1,
        },
      });
      await delayedApply.promise;
    });
    expect(
      screen.getByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("项目原生配置已通过持久化预览应用并完成写后验证。"),
    ).not.toBeInTheDocument();
  });

  it("平台切换后 MCP 与 Skill 预览和 Apply 都使用当前 Codex 目标", async () => {
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理 Codex 项目资源" }),
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Codex MCP 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "codex",
        projectId: project.id,
        excludeFromGit: false,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "codex",
        projectId: project.id,
      }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "确认原生配置变更" }),
      ).not.toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Codex Skills 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewSkillSync).toHaveBeenCalledWith({
        tool: "codex",
        projectId: project.id,
        excludeFromGit: false,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledWith({
        previewId: skillPreview.previewId,
        tool: "codex",
        projectId: project.id,
      }),
    );
  });

  it("MCP 继承保持只读，项目追加刷新三组查询并用持久化预览 Apply", async () => {
    vi.mocked(commands.getProject)
      .mockResolvedValueOnce({ status: "ok", data: project })
      .mockResolvedValue({
        status: "ok",
        data: { ...project, rowVersion: project.rowVersion + 1 },
      });
    const { client } = renderPage();
    const invalidateQueries = vi.spyOn(client, "invalidateQueries");
    const inherited = await screen.findByRole("button", {
      name: "全局 MCP MCP 项目追加",
    });
    const available = screen.getByRole("button", {
      name: "项目 MCP MCP 项目追加",
    });
    expect(inherited).toBeDisabled();
    expect(inherited).toHaveAttribute("aria-pressed", "true");
    expect(inherited).toHaveTextContent("禁用");
    expect(available).toBeEnabled();
    expect(available).toHaveAttribute("aria-pressed", "false");
    expect(available).toHaveTextContent("启用");
    expect(screen.getByText("全局继承")).toBeVisible();
    expect(screen.getByText("只读")).toBeVisible();
    expect(screen.getByText("可追加")).toBeVisible();

    fireEvent.click(available);
    await waitFor(() =>
      expect(commands.setProjectMcpAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        mcpId: mcpOptions[1]?.mcpId,
        assigned: true,
        mcpRowVersion: 4,
        projectRowVersion: 7,
      }),
    );
    await waitFor(() => expect(commands.getProject).toHaveBeenCalledTimes(2));
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["projects"],
    });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["mcp"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["skills"] });
    await waitFor(() =>
      expect(
        client.getQueryData<ProjectDto>(["projects", "detail", project.id])
          ?.rowVersion,
      ).toBe(8),
    );
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(2),
    );

    vi.mocked(commands.setProjectMcpAssignment).mockClear();
    fireEvent.click(
      screen.getByRole("button", { name: "项目 MCP MCP 项目追加" }),
    );
    await waitFor(() =>
      expect(commands.setProjectMcpAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        mcpId: mcpOptions[1]?.mcpId,
        assigned: true,
        mcpRowVersion: 4,
        projectRowVersion: 8,
      }),
    );
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();

    fireEvent.click(
      screen.getByRole("button", { name: "Claude MCP 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: project.id,
        excludeFromGit: false,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
  });

  it("Skill 继承保持只读，项目追加支持 Git exclude、预览与显式 Apply", async () => {
    vi.mocked(commands.getProject)
      .mockResolvedValueOnce({ status: "ok", data: project })
      .mockResolvedValue({
        status: "ok",
        data: { ...project, rowVersion: project.rowVersion + 1 },
      });
    const { client } = renderPage();
    const invalidateQueries = vi.spyOn(client, "invalidateQueries");
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Skill" }),
    );

    const inherited = await screen.findByRole("button", {
      name: "全局 Skill Skill 项目追加",
    });
    const available = screen.getByRole("button", {
      name: "项目 Skill Skill 项目追加",
    });
    expect(inherited).toBeDisabled();
    expect(inherited).toHaveAttribute("aria-pressed", "true");
    expect(inherited).toHaveTextContent("禁用");
    expect(available).toBeEnabled();
    expect(available).toHaveAttribute("aria-pressed", "false");
    expect(available).toHaveTextContent("启用");
    expect(screen.getByText("全局继承")).toBeVisible();
    expect(screen.getByText("只读")).toBeVisible();
    expect(screen.getByText("可追加")).toBeVisible();

    fireEvent.click(available);
    await waitFor(() =>
      expect(commands.setProjectSkillAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        skillId: skillOptions[1]?.skillId,
        assigned: true,
        skillRowVersion: 6,
        projectRowVersion: 7,
      }),
    );
    await waitFor(() => expect(commands.getProject).toHaveBeenCalledTimes(2));
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["projects"],
    });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["mcp"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["skills"] });
    await waitFor(() =>
      expect(
        client.getQueryData<ProjectDto>(["projects", "detail", project.id])
          ?.rowVersion,
      ).toBe(8),
    );
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(2),
    );

    vi.mocked(commands.setProjectSkillAssignment).mockClear();
    fireEvent.click(
      screen.getByRole("button", { name: "项目 Skill Skill 项目追加" }),
    );
    await waitFor(() =>
      expect(commands.setProjectSkillAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        skillId: skillOptions[1]?.skillId,
        assigned: true,
        skillRowVersion: 6,
        projectRowVersion: 8,
      }),
    );
    expect(commands.applySkillPreview).not.toHaveBeenCalled();

    const claudeSection = screen
      .getByRole("heading", { name: "Claude Skill 项目追加" })
      .closest("section");
    if (!claudeSection) throw new Error("未找到 Claude 项目管理列");
    fireEvent.click(
      within(claudeSection).getByRole("checkbox", {
        name: /若目标是应用新建且未跟踪/,
      }),
    );
    fireEvent.click(
      within(claudeSection).getByRole("button", {
        name: "Claude Skills 同步预览",
      }),
    );
    await waitFor(() =>
      expect(commands.previewSkillSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: project.id,
        excludeFromGit: true,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledWith({
        previewId: skillPreview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
  });

  it("提示词分配支持硬拷贝覆盖语义、预览与显式 Apply，解除分配保留项目文件", async () => {
    vi.mocked(commands.getProject)
      .mockResolvedValueOnce({ status: "ok", data: project })
      .mockResolvedValue({
        status: "ok",
        data: { ...project, rowVersion: project.rowVersion + 1 },
      });
    vi.mocked(commands.getPromptProjectAssignment)
      .mockResolvedValueOnce({
        status: "ok",
        data: { projectId: project.id, tool: "claude", profileId: null },
      })
      .mockResolvedValue({
        status: "ok",
        data: {
          projectId: project.id,
          tool: "claude",
          profileId: promptProfileFixture.id,
        },
      });
    vi.mocked(commands.listPromptProfiles).mockResolvedValue({
      status: "ok",
      data: [promptProfileFixture],
    });
    vi.mocked(commands.setPromptProjectAssignment).mockResolvedValue({
      status: "ok",
      data: {
        projectId: project.id,
        tool: "claude",
        profileId: promptProfileFixture.id,
      },
    });
    vi.mocked(commands.previewPromptSync).mockResolvedValue({
      status: "ok",
      data: promptPreview,
    });
    const { client } = renderPage();
    const invalidateQueries = vi.spyOn(client, "invalidateQueries");
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目提示词" }),
    );

    const assignButton = await screen.findByRole("button", {
      name: "分配 项目提示词 为项目提示词",
    });
    fireEvent.click(assignButton);
    await waitFor(() =>
      expect(commands.setPromptProjectAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        promptProfileId: promptProfileFixture.id,
        projectRowVersion: 7,
      }),
    );
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["profiles"] });

    // 分配成功后由失效刷新取回已分配状态。
    await screen.findByRole("button", { name: "解除项目提示词分配" });
    const enabledPreview = screen.getByRole("button", {
      name: "Claude 提示词同步预览",
    });
    expect(enabledPreview).toBeEnabled();
    fireEvent.click(enabledPreview);
    await waitFor(() =>
      expect(commands.previewPromptSync).toHaveBeenCalledWith(
        "claude",
        project.id,
      ),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyProfilePreview).toHaveBeenCalledWith({
        previewId: promptPreview.previewId,
        tool: "claude",
        artifactKind: "prompt",
        projectId: project.id,
      }),
    );

    vi.mocked(commands.setPromptProjectAssignment).mockClear();
    vi.spyOn(globalThis, "confirm").mockReturnValue(true);
    fireEvent.click(screen.getByRole("button", { name: "解除项目提示词分配" }));
    await waitFor(() =>
      expect(commands.setPromptProjectAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        promptProfileId: null,
        projectRowVersion: 8,
      }),
    );
    vi.mocked(globalThis.confirm).mockRestore();
  });

  it("已追加与异常状态以 tag 展示且按钮显示禁用", async () => {
    vi.mocked(commands.listMcpProjectOptions).mockResolvedValue({
      status: "ok",
      data: [
        {
          mcpId: "00000000-0000-4000-8000-000000000722",
          name: "已追加 MCP",
          enabled: false,
          state: "selected",
          selectable: true,
          rowVersion: 9,
        },
      ],
    });
    vi.mocked(commands.listSkillProjectOptions).mockResolvedValue({
      status: "ok",
      data: [
        {
          skillId: "00000000-0000-4000-8000-000000000723",
          name: "异常 Skill",
          status: "invalid",
          state: "selected",
          selectable: true,
          rowVersion: 10,
        },
      ],
    });
    renderPage();

    const mcpButton = await screen.findByRole("button", {
      name: "已追加 MCP MCP 项目追加",
    });
    expect(mcpButton).toBeEnabled();
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(mcpButton).toHaveTextContent("禁用");
    expect(screen.getByText("项目追加")).toBeVisible();
    expect(screen.getByText("已停用")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    const skillButton = await screen.findByRole("button", {
      name: "异常 Skill Skill 项目追加",
    });
    expect(skillButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveTextContent("禁用");
    expect(screen.getByText("项目追加")).toBeVisible();
    expect(screen.getByText("invalid")).toBeVisible();
  });

  it("MCP 与 Skill 空目标预览只解释无需写入且不开放 Apply", async () => {
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, targets: [] },
    });
    vi.mocked(commands.previewSkillSync).mockResolvedValue({
      status: "ok",
      data: { ...skillPreview, targets: [] },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    expect(
      await screen.findByText(
        "该项目只有全局继承 MCP，不需要创建项目配置文件。",
      ),
    ).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude Skills 同步预览" }),
    );
    expect(
      await screen.findByText(
        "该项目只有全局继承 Skills，不需要创建项目链接目录。",
      ),
    ).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });

  it("在 MCP 与 Skill 视图中都保留项目目标阻断", async () => {
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "ok",
      data: { ...project, codexTrustStatus: "untrusted" },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "管理 Codex 项目资源" }),
    );
    expect(
      await screen.findByRole("button", { name: "Codex MCP 同步预览" }),
    ).toBeDisabled();
    expect(screen.getByText(/Codex 项目尚未受信任/)).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    expect(
      await screen.findByRole("button", { name: "Codex Skills 同步预览" }),
    ).toBeDisabled();
    expect(screen.getByText(/Codex 项目尚未受信任/)).toBeVisible();
    expect(commands.previewMcpSync).not.toHaveBeenCalled();
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
  });
});
