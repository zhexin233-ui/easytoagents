import { vi } from "vitest";
import { renderWithProviders } from "@/test/render";
import { Routes, Route } from "react-router-dom";
import claudeIconUrl from "@/assets/brand/claude-icon-square.svg";
import codexIconUrl from "@/assets/brand/codex-icon-light.png";
import cursorIconUrl from "@/assets/brand/cursor-icon.svg";
import {
  commands,
  type HookProjectOptionDto,
  type McpProjectOptionDto,
  type PreviewPlan,
  type ProjectDto,
  type ProjectNativeResourceDto,
  type SkillProjectOptionDto,
} from "@/bindings/commands";
import { ProjectDetailPage } from "@/features/projects/detail/page";
import { makeProject } from "@/test/fixtures/dtos";
import { makePreviewPlan, makeTarget } from "@/test/fixtures/preview-plan";

export { commands };
export { claudeIconUrl, codexIconUrl, cursorIconUrl };

export const project: ProjectDto = makeProject({
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
      tool: "cursor",
      artifactKind: "mcp",
      targetPath: "/isolated/projects/detail/.cursor/mcp.json",
      capability: "supported",
      policy: "allowed",
      trust: "not_required",
      status: "missing",
      diagnosticCode: null,
    },
    {
      tool: "cursor",
      artifactKind: "skill",
      targetPath: "/isolated/projects/detail/.cursor/skills",
      capability: "supported",
      policy: "allowed",
      trust: "not_required",
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
  nativeResources: {
    active: 0,
    disabled: 0,
    missing: 0,
    conflict: 0,
  },
  lastScannedAt: "2026-08-24T10:00:00Z",
  rowVersion: 7,
});

export const mcpOptions: McpProjectOptionDto[] = [
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

export const skillOptions: SkillProjectOptionDto[] = [
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

export const preview: PreviewPlan = makePreviewPlan({
  previewId: "00000000-0000-4000-8000-000000000714",
  scope: "project",
  projectId: project.id,
  dbVersion: 7,
  warningCodes: ["GIT_TRACKED"],
  targets: [
    makeTarget({
      targetId: "00000000-0000-4000-8000-000000000715",
      descriptor: {
        tool: "claude",
        artifactKind: "mcp",
        scope: "project",
        projectRoot: project.rootPath,
        path: "/isolated/projects/detail/.mcp.json",
        allowedRoot: null,
        mcpContainer: null,
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
    }),
  ],
});

export const skillPreview: PreviewPlan = makePreviewPlan({
  previewId: "00000000-0000-4000-8000-000000000718",
  scope: "project",
  projectId: project.id,
  dbVersion: 8,
  warningCodes: [],
  targets: [
    makeTarget({
      targetId: "00000000-0000-4000-8000-000000000719",
      descriptor: {
        tool: "claude",
        artifactKind: "skill",
        scope: "project",
        projectRoot: project.rootPath,
        path: "/isolated/projects/detail/.claude/skills",
        allowedRoot: null,
        mcpContainer: null,
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
    }),
  ],
});

export const nativeResource: ProjectNativeResourceDto = {
  id: "00000000-0000-4000-8000-000000000741",
  projectId: project.id,
  tool: "claude",
  artifactKind: "mcp",
  displayName: "native-stdio",
  targetPath: "/isolated/projects/detail/.mcp.json",
  entryType: "mcp_entry",
  state: "active",
  rowVersion: 2,
  canDisable: true,
  canRestore: false,
  diagnosticCodes: [],
  safeSummary: { kind: "mcp" },
  disabledAt: null,
};

export const hookOptions: HookProjectOptionDto[] = [
  {
    hookId: "00000000-0000-4000-8000-000000000760",
    name: "inherited-hook",
    event: "PreToolUse",
    enabled: true,
    state: "inherited",
    selectable: false,
    assignedEvent: null,
    rowVersion: 3,
  },
  {
    hookId: "00000000-0000-4000-8000-000000000761",
    name: "project-hook",
    event: "SessionStart",
    enabled: true,
    state: "selected",
    selectable: true,
    assignedEvent: "PreToolUse",
    rowVersion: 4,
  },
  {
    hookId: "00000000-0000-4000-8000-000000000762",
    name: "available-hook",
    event: "Stop",
    enabled: true,
    state: "available",
    selectable: true,
    assignedEvent: null,
    rowVersion: 6,
  },
];

export const hookPreview: PreviewPlan = makePreviewPlan({
  ...preview,
  previewId: "00000000-0000-4000-8000-000000000763",
  targets: [
    {
      ...preview.targets[0]!,
      descriptor: {
        ...preview.targets[0]!.descriptor,
        artifactKind: "hook",
        path: "/isolated/projects/detail/.claude/settings.json",
        managedSelectorRoots: ["hooks"],
      },
      ownership: { kind: "selectors", paths: [["hooks"]] },
    },
  ],
});

export const nativePreview: PreviewPlan = makePreviewPlan({
  ...preview,
  previewId: "00000000-0000-4000-8000-000000000742",
  warningCodes: ["PROJECT_NATIVE_RESOURCE_REQUIRES_CONFIRMATION"],
  targets: preview.targets.map((target) => ({
    ...target,
    changeKind: "delete",
    redactedDiff: { after: {} },
  })),
});

export function renderPage() {
  const result = renderWithProviders(
    <Routes>
      <Route path="/projects/:projectId" element={<ProjectDetailPage />} />
      <Route path="/projects" element={<p>项目列表路由已渲染</p>} />
    </Routes>,
    {
      initialEntries: [`/projects/${project.id}`],
    },
  );
  return { ...result, client: result.queryClient };
}

export function createDeferred<T>() {
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

export function setupMocks() {
  vi.clearAllMocks();
  vi.mocked(commands.getProject).mockResolvedValue({
    status: "ok",
    data: project,
  });
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: {
      applyMode: "preview_confirm",
      enabledTools: ["claude", "codex", "cursor"],
    },
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
  vi.mocked(commands.listHookProjectOptions).mockImplementation((input) =>
    Promise.resolve({
      status: "ok",
      data: input.tool === "claude" ? hookOptions : [],
    }),
  );
  vi.mocked(commands.previewHookSync).mockResolvedValue({
    status: "ok",
    data: hookPreview,
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
  vi.mocked(commands.applyHookPreview).mockResolvedValue({
    status: "ok",
    data: {
      runId: "run-hook-1",
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
  vi.mocked(commands.getInterruptedRun).mockResolvedValue({
    status: "ok",
    data: null,
  });
  vi.mocked(commands.listProjectNativeResources).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.previewProjectNativeResourceAction).mockResolvedValue({
    status: "ok",
    data: nativePreview,
  });
  vi.mocked(commands.applyProjectNativeResourcePreview).mockResolvedValue({
    status: "ok",
    data: {
      runId: "run-native-1",
      status: "succeeded",
      appliedTargets: 1,
      snapshotCount: 1,
    },
  });
}
