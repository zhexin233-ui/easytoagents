import type {
  HookDto,
  McpServerDto,
  OfficialLoginStatusDto,
  ProjectDto,
  ProviderImportCandidateDto,
  ProviderImportPreviewDto,
  ProviderProfileDto,
  PromptProfileDto,
  SkillDto,
  SyncScopeDto,
} from "@/bindings/commands";

/**
 * 为中央 mutation 结果附加后端签发的精确同步范围。
 *
 * 生成绑定在后端字段落地前仍可能没有该可选属性；通过 fixture helper
 * 保持测试可以同时覆盖迁移前后的绑定，而页面只读取返回 DTO 的该字段。
 */
export function withAffectedSyncScopes<T extends object>(
  value: T,
  scopes: readonly SyncScopeDto[],
): T & { affectedSyncScopes: SyncScopeDto[] } {
  return { ...value, affectedSyncScopes: [...scopes] };
}

export const globalSyncScope = (
  artifactKind: SyncScopeDto["artifactKind"],
  tool: SyncScopeDto["tool"],
): SyncScopeDto => ({ artifactKind, tool, projectId: null });

export const makeSkill = (o: Partial<SkillDto> = {}): SkillDto => ({
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
  ...o,
});
export const makeHook = (o: Partial<HookDto> = {}): HookDto => ({
  id: "hook-1",
  name: "Example Hook",
  event: "SessionStart",
  matcher: null,
  command: "echo ok",
  timeoutSeconds: null,
  enabled: true,
  scriptName: null,
  globalAssignments: [],
  rowVersion: 1,
  ...o,
});
export const makeMcpServer = (o: Partial<McpServerDto> = {}): McpServerDto => ({
  id: "00000000-0000-4000-8000-000000000501",
  name: "fixture-mcp",
  transport: "stdio",
  command: "npx",
  args: ["-y", "fixture"],
  url: null,
  headerNames: [],
  envNames: ["MCP_TOKEN"],
  redactedExtra: { nested: { apiToken: "[REDACTED]" } },
  enabled: true,
  globalTools: [],
  rowVersion: 2,
  ...o,
});
export const makePromptProfile = (
  o: Partial<PromptProfileDto> = {},
): PromptProfileDto => ({
  id: "prompt-1",
  name: "Example Prompt",
  body: "Be helpful.",
  globalTools: [],
  importedFromPath: null,
  rowVersion: 1,
  ...o,
});
export const makeProviderProfile = (
  o: Partial<ProviderProfileDto> = {},
): ProviderProfileDto => ({
  id: "provider-1",
  tool: "claude",
  name: "Example Provider",
  apiBaseUrl: "https://example.test",
  apiKeyConfigured: false,
  defaultModel: "model",
  options: {
    authKind: "api_key",
    credentialEnvKey: null,
    extraEnv: {},
    providerId: null,
    wireApi: null,
    zcodeKind: null,
    opencodeNpm: null,
    opencodeApi: null,
  },
  pi: null,
  isActive: false,
  rowVersion: 1,
  ...o,
});
export const makeProviderImportCandidate = (
  o: Partial<ProviderImportCandidateDto> = {},
): ProviderImportCandidateDto => ({
  candidateId: "00000000-0000-4000-8000-000000000702",
  providerId: "fixture",
  suggestedName: "已发现 Claude 渠道",
  status: "importable",
  reason: null,
  authKind: "api_key",
  defaultProvider: true,
  apiBaseUrl: "https://fixture.example.com",
  apiKeyConfigured: true,
  defaultModel: "fixture-model",
  apiFormat: null,
  modelCount: 0,
  redactedProjection: { env: "[REDACTED]" },
  skippedEnvKeys: [],
  ...o,
});
export const makeProviderImportPreview = (
  o: Partial<ProviderImportPreviewDto> = {},
): ProviderImportPreviewDto => ({
  previewId: "00000000-0000-4000-8000-000000000701",
  tool: "claude",
  targetPath: "/isolated/home/.claude/settings.json",
  candidates: [makeProviderImportCandidate()],
  message: null,
  ...o,
});
export const makeOfficialLoginStatus = (
  o: Partial<OfficialLoginStatusDto> = {},
): OfficialLoginStatusDto => ({
  tool: "claude",
  supported: true,
  phase: "idle",
  loggedIn: false,
  authMethod: null,
  account: null,
  diagnostic: null,
  loginUrl: null,
  manualCommand: "claude auth login",
  ...o,
});
export const makeProject = (o: Partial<ProjectDto> = {}): ProjectDto => ({
  id: "project-1",
  displayName: "Example Project",
  rootPath: "/tmp/project",
  pathStatus: "valid",
  gitStatus: "not_repository",
  codexTrustStatus: "trusted",
  claudePolicyStatus: "allowed",
  targets: [],
  nativeResources: { active: 0, disabled: 0, missing: 0, conflict: 0 },
  lastScannedAt: null,
  rowVersion: 1,
  ...o,
});
