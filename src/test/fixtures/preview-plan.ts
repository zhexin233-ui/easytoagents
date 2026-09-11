import type {
  PreviewPlan,
  PreviewTargetPlan,
  TargetDescriptor,
} from "@/bindings/commands";

export function makeTarget(
  overrides: Partial<PreviewTargetPlan> = {},
): PreviewTargetPlan {
  const descriptor: TargetDescriptor = {
    tool: "claude",
    artifactKind: "provider",
    scope: "global",
    projectRoot: null,
    path: "/tmp/target.json",
    format: "json",
    managedSelectorRoots: [],
    sensitiveSelectors: [],
    capability: { state: "supported", diagnosticCode: null },
    policy: "allowed",
    trust: "not_required",
    promptOverride: "not_applicable",
    symlinkPolicy: "reject",
  };
  return {
    targetId: "00000000-0000-4000-8000-000000000002",
    descriptor,
    ownership: { kind: "whole_document" },
    changeKind: "add",
    status: "missing",
    currentFullHash: null,
    currentManagedHash: null,
    desiredManagedHash: "0".repeat(64),
    targetRowVersion: 1,
    rowVersions: [],
    redactedDiff: {},
    warningCodes: [],
    baselineMismatchedItems: [],
    readoptAvailable: false,
    errorCode: null,
    git: null,
    excludeFromGit: false,
    ...overrides,
  };
}
export function makePreviewPlan(
  overrides: Partial<PreviewPlan> = {},
): PreviewPlan {
  return {
    previewId: "00000000-0000-4000-8000-000000000001",
    scope: "global",
    projectId: null,
    dbVersion: 1,
    targets: [makeTarget()],
    warningCodes: [],
    ...overrides,
  };
}

/**
 * 默认的 Skill 全局同步预览，用于覆盖页面测试中重复出现的目标描述。
 * 调用方仍可通过 overrides 覆盖任意 PreviewPlan 字段。
 */
export function makeSkillPreview(
  overrides: Partial<PreviewPlan> = {},
): PreviewPlan {
  return makePreviewPlan({
    previewId: "00000000-0000-4000-8000-000000000699",
    scope: "global",
    projectId: null,
    dbVersion: 3,
    warningCodes: [],
    targets: [
      makeTarget({
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
        baselineMismatchedItems: [],
        readoptAvailable: false,
        errorCode: null,
        git: null,
        excludeFromGit: false,
      }),
    ],
    ...overrides,
  });
}

/** 默认的 MCP 全局同步预览。 */
export function makeMcpPreview(
  overrides: Partial<PreviewPlan> = {},
): PreviewPlan {
  return makePreviewPlan({
    previewId: "00000000-0000-4000-8000-000000000599",
    scope: "global",
    projectId: null,
    dbVersion: 3,
    warningCodes: [],
    targets: [
      makeTarget({
        targetId: "00000000-0000-4000-8000-000000000598",
        descriptor: {
          tool: "claude",
          artifactKind: "mcp",
          scope: "global",
          projectRoot: null,
          path: "/isolated/home/.claude.json",
          format: "json",
          managedSelectorRoots: ["mcpServers"],
          sensitiveSelectors: ["mcpServers/*/headers", "mcpServers/*/env"],
          capability: { state: "supported", diagnosticCode: null },
          policy: "allowed",
          trust: "not_required",
          promptOverride: "not_applicable",
          symlinkPolicy: "reject",
        },
        ownership: {
          kind: "selectors",
          paths: [["mcpServers", "fixture-mcp"]],
        },
        changeKind: "update",
        status: "in_sync",
        currentFullHash: "a".repeat(64),
        currentManagedHash: "b".repeat(64),
        desiredManagedHash: "c".repeat(64),
        targetRowVersion: 1,
        rowVersions: [],
        redactedDiff: {
          before: {},
          after: { mcpServers: { "fixture-mcp": { env: "[REDACTED]" } } },
        },
        warningCodes: [],
        baselineMismatchedItems: [],
        readoptAvailable: false,
        errorCode: null,
        git: null,
        excludeFromGit: false,
      }),
    ],
    ...overrides,
  });
}
