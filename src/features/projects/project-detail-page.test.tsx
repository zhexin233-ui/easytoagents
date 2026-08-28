/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
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
  type SkillProjectOptionDto,
} from "@/bindings/commands";
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
      targetPath: "/isolated/projects/detail/.agents/skills",
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

describe("ProjectDetailPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "ok",
      data: project,
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

  it("默认只展示 MCP，并可在 MCP 与 Skill 管理视图间双向切换", async () => {
    renderPage();

    const group = await screen.findByRole("group", {
      name: "项目资源管理视图",
    });
    const mcpButton = within(group).getByRole("button", {
      name: "管理项目 MCP",
    });
    const skillButton = within(group).getByRole("button", {
      name: "管理项目 Skill",
    });
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveAttribute("aria-pressed", "false");
    expect(await screen.findAllByRole("heading", { name: "MCP" })).toHaveLength(
      2,
    );
    expect(
      screen.queryByRole("heading", { name: "Skills" }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(2),
    );
    expect(commands.listSkillProjectOptions).not.toHaveBeenCalled();

    fireEvent.click(skillButton);
    expect(mcpButton).toHaveAttribute("aria-pressed", "false");
    expect(skillButton).toHaveAttribute("aria-pressed", "true");
    expect(
      await screen.findAllByRole("heading", { name: "Skills" }),
    ).toHaveLength(2);
    expect(
      screen.queryByRole("heading", { name: "MCP" }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(2),
    );

    fireEvent.click(mcpButton);
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveAttribute("aria-pressed", "false");
    expect(await screen.findAllByRole("heading", { name: "MCP" })).toHaveLength(
      2,
    );
    expect(
      screen.queryByRole("heading", { name: "Skills" }),
    ).not.toBeInTheDocument();
  });

  it("切换资源视图会重置尚未提交的 Git exclude 选择", async () => {
    renderPage();
    const claudeSection = (
      await screen.findByRole("heading", { name: "Claude 项目追加" })
    ).closest("section");
    if (!claudeSection) throw new Error("未找到 Claude 项目管理列");

    const mcpExclude = within(claudeSection).getByRole("checkbox", {
      name: /若目标是应用新建且未跟踪/,
    });
    fireEvent.click(mcpExclude);
    expect(mcpExclude).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    const skillExclude = within(claudeSection).getByRole("checkbox", {
      name: /若目标是应用新建且未跟踪/,
    });
    expect(skillExclude).not.toBeChecked();
    fireEvent.click(skillExclude);
    expect(skillExclude).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 MCP" }));
    expect(
      within(claudeSection).getByRole("checkbox", {
        name: /若目标是应用新建且未跟踪/,
      }),
    ).not.toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    expect(
      within(claudeSection).getByRole("checkbox", {
        name: /若目标是应用新建且未跟踪/,
      }),
    ).not.toBeChecked();
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
    const inherited = await screen.findByLabelText("全局 MCP MCP 项目追加");
    const available = screen.getByLabelText("项目 MCP MCP 项目追加");
    expect(inherited).toBeChecked();
    expect(inherited).toBeDisabled();
    expect(available).not.toBeChecked();

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
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(4),
    );

    vi.mocked(commands.setProjectMcpAssignment).mockClear();
    fireEvent.click(screen.getByLabelText("项目 MCP MCP 项目追加"));
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

    const inherited = await screen.findByLabelText("全局 Skill Skill 项目追加");
    const available = screen.getByLabelText("项目 Skill Skill 项目追加");
    expect(inherited).toBeChecked();
    expect(inherited).toBeDisabled();
    expect(available).not.toBeChecked();

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
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(4),
    );

    vi.mocked(commands.setProjectSkillAssignment).mockClear();
    fireEvent.click(screen.getByLabelText("项目 Skill Skill 项目追加"));
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
      .getByRole("heading", { name: "Claude 项目追加" })
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
