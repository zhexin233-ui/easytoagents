import { createElement } from "react";
import { vi } from "vitest";

import {
  commands,
  type SkillImportPreviewDto,
  type Tool,
} from "@/bindings/commands";
import { SkillsPage } from "@/features/skills/skills-page";
import { makeSkill, makeSkillPreview } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";

export const skill = makeSkill();
export const preview = makeSkillPreview();
export function nativeImport(tool: Tool): SkillImportPreviewDto {
  const root =
    tool === "claude"
      ? "/isolated/custom-claude/skills"
      : tool === "codex"
        ? "/isolated/custom-codex/skills"
        : "/isolated/custom-cursor/skills";
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
        : tool === "codex"
          ? [
              {
                kind: "codex_home",
                path: root,
                status: "ready",
                diagnosticCode: "SKILL_IMPORT_BUILTIN_EXCLUDED",
                message: "已排除内置技能集合。",
              },
              {
                kind: "codex_agents",
                path: "/isolated/home/.agents/skills",
                status: "missing",
                diagnosticCode: "SKILL_IMPORT_SOURCE_MISSING",
                message: null,
              },
            ]
          : [
              {
                kind: "cursor_home",
                path: root,
                status: "ready",
                diagnosticCode: null,
                message: null,
              },
              {
                kind: "cursor_agents",
                path: "/isolated/home/.agents/skills",
                status: "missing",
                diagnosticCode: "SKILL_IMPORT_SOURCE_MISSING",
                message: null,
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
        takeoverEligible: false,
        takeoverEntryType: null,
      },
      {
        candidateId: "unselected",
        name: "unselected-skill",
        description: "不应导入的未勾选项",
        sourcePaths: [`${root}/unselected-skill`],
        status: "importable",
        reason: null,
        existingSkillId: null,
        takeoverEligible: false,
        takeoverEntryType: null,
      },
      {
        candidateId: "existing",
        name: "fixture-skill",
        description: "与中央记录相同",
        sourcePaths: [`${root}/fixture-skill`],
        status: "already_imported",
        reason: null,
        existingSkillId: skill.id,
        takeoverEligible: false,
        takeoverEntryType: null,
      },
      {
        candidateId: "conflict",
        name: "conflict-skill",
        description: "同名不同内容",
        sourcePaths: [`${root}/conflict-skill`],
        status: "name_conflict",
        reason: "同名技能内容不同，不会覆盖或改名。",
        existingSkillId: null,
        takeoverEligible: false,
        takeoverEntryType: null,
      },
      {
        candidateId: "invalid",
        name: "invalid-skill",
        description: "无效技能入口",
        sourcePaths: [`${root}/invalid-skill`],
        status: "invalid",
        reason: "来源链接不可安全读取。",
        existingSkillId: null,
        takeoverEligible: false,
        takeoverEntryType: null,
      },
    ],
    message: null,
  };
}
export function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((fulfill) => {
    resolve = fulfill;
  });
  return { promise, resolve };
}
export function renderPage() {
  const rendered = renderWithProviders(createElement(SkillsPage));
  return { ...rendered, client: rendered.queryClient };
}
export function setupMocks() {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(commands.discoverSkillImport).mockImplementation((tool) =>
    Promise.resolve({ status: "ok", data: nativeImport(tool) }),
  );
  vi.mocked(commands.confirmSkillImport).mockResolvedValue({
    status: "ok",
    data: { tool: "claude", createdCount: 1 },
  });
  vi.mocked(commands.prepareSkillTakeover).mockResolvedValue({
    status: "ok",
    data: {
      tool: "claude",
      assignedCount: 1,
      reusedCount: 0,
      plan: preview,
    },
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
        targetPath: "/isolated/home/.codex/skills",
        status: "missing",
        diagnosticCode: null,
      },
      {
        tool: "cursor",
        projectId: null,
        targetPath: "/isolated/home/.cursor/skills",
        status: "missing",
        diagnosticCode: null,
      },
    ],
  });
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: {
      applyMode: "preview_confirm",
      enabledTools: ["claude", "codex", "cursor"],
    },
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
}
