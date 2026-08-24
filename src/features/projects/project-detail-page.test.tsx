/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type McpProjectOptionDto,
  type PreviewPlan,
  type ProjectDto,
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

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={[`/projects/${project.id}`]}>
      <QueryClientProvider client={client}>
        <Routes>
          <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
        </Routes>
      </QueryClientProvider>
    </MemoryRouter>,
  );
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
    vi.mocked(commands.listSkillProjectOptions).mockResolvedValue({
      status: "ok",
      data: [],
    });
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
    vi.mocked(commands.applyMcpPreview).mockResolvedValue({
      status: "ok",
      data: {
        runId: "run-1",
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
  });

  afterEach(cleanup);

  it("持续显示全局继承，只允许追加其他项并用持久化预览 Apply", async () => {
    vi.mocked(commands.getProject)
      .mockResolvedValueOnce({ status: "ok", data: project })
      .mockResolvedValue({
        status: "ok",
        data: { ...project, rowVersion: project.rowVersion + 1 },
      });
    renderPage();
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
    await waitFor(() =>
      expect(
        vi.mocked(commands.listSkillProjectOptions).mock.calls.length,
      ).toBeGreaterThan(2),
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Claude MCP 同步预览" }),
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
});
