import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type ProjectDto,
  type SkillProjectOptionDto,
  type Tool,
} from "@/bindings/commands";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { invalidateProjectScope } from "@/lib/projects-api";
import { skillProjectOptionsQueryOptions } from "@/lib/skills-api";

import { ProjectAssignmentsSection } from "./project-assignments-section";
import { OptionTag, ProjectOptionRow } from "../option-row";
import { projectBlocked } from "./shared";

export function ProjectSkillAssignments({
  project,
  tool,
}: {
  project: ProjectDto;
  tool: Tool;
}) {
  const queryClient = useQueryClient();
  const optionsQuery = useQuery(
    skillProjectOptionsQueryOptions(project.id, tool),
  );
  const [excludeFromGit, setExcludeFromGit] = useState(false);
  const invalidate = async () => {
    await invalidateProjectScope(queryClient, [
      "project",
      "skill",
      "mcp",
      "hook",
    ]);
  };
  const { requestPreview } = useSyncPreviewFlow({
    artifactKind: "skill",
    preview: (previewTool, projectId) =>
      commands.previewSkillSync({
        tool: previewTool,
        projectId: projectId ?? project.id,
        excludeFromGit,
      }),
    apply: ({ previewId, tool: previewTool, projectId }) =>
      commands.applySkillPreview({
        previewId,
        tool: previewTool,
        projectId: projectId ?? project.id,
      }),
    invalidate,
    messages: {
      previewFailed: "生成项目 Skill 预览失败。",
      applyFailed: "应用项目 Skill 预览失败。",
      empty: "该项目只有全局继承 Skills，不需要创建项目链接目录。",
      applied: () => "项目原生配置已通过持久化预览应用并完成写后验证。",
    },
  });
  const assignmentMutation = useMutation({
    mutationFn: async ({
      option,
      assigned,
    }: {
      option: SkillProjectOptionDto;
      assigned: boolean;
    }) =>
      unwrapResult(
        await commands.setProjectSkillAssignment({
          projectId: project.id,
          tool,
          skillId: option.skillId,
          assigned,
          skillRowVersion: option.rowVersion,
          projectRowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async (result) => {
      await invalidate();
      await requestPreview(result.affectedSyncScopes ?? []);
    },
  });
  const blocked = projectBlocked(project, tool);

  return (
    <ProjectAssignmentsSection
      title="Skills"
      description="全局 Skills 只读继承；项目只能追加其他中央 Skills。"
      blocked={blocked}
      error={profileErrorText(optionsQuery.error ?? assignmentMutation.error)}
      pending={optionsQuery.isPending}
      empty={optionsQuery.data?.length === 0}
      excludeFromGit={excludeFromGit}
      onExcludeFromGit={setExcludeFromGit}
    >
      {optionsQuery.data?.map((option) => (
        <ProjectOptionRow
          key={option.skillId}
          name={option.name}
          state={option.state}
          actionLabel={`${option.name} Skill 项目追加`}
          actionDisabled={
            option.state === "inherited" ||
            (option.state === "available" && !option.selectable) ||
            assignmentMutation.isPending
          }
          onToggle={() =>
            assignmentMutation.mutate({
              option,
              assigned: option.state === "available",
            })
          }
        >
          {option.status !== "ready" ? (
            <OptionTag tone="warning">{option.status}</OptionTag>
          ) : null}
        </ProjectOptionRow>
      ))}
    </ProjectAssignmentsSection>
  );
}
