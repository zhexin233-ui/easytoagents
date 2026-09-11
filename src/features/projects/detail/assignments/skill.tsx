import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type ProjectDto,
  type SkillProjectOptionDto,
  type Tool,
} from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { invalidateProjectScope } from "@/lib/projects-api";
import { skillProjectOptionsQueryOptions } from "@/lib/skills-api";
import { toolMetadata } from "@/lib/tool-metadata";

import { ProjectAssignmentsSection } from "./project-assignments-section";
import { OptionTag, ProjectOptionRow } from "../option-row";
import { projectBlocked } from "./shared";

export function ProjectSkillAssignments({
  project,
  tool,
  directApply,
  onMessage,
}: {
  project: ProjectDto;
  tool: Tool;
  directApply: boolean;
  onMessage: (message: string) => void;
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
  const {
    openPreview,
    requestPreview,
    previewMutation,
    applyMutation,
    closePreview,
  } = useSyncPreviewFlow({
    artifactKind: "skill",
    directApply,
    preview: () =>
      commands.previewSkillSync({
        tool,
        projectId: project.id,
        excludeFromGit,
      }),
    apply: ({ previewId, tool: previewTool }) =>
      commands.applySkillPreview({
        previewId,
        tool: previewTool,
        projectId: project.id,
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
    onSuccess: async () => {
      await invalidate();
      if (directApply) {
        requestPreview(tool, true);
        return;
      }
      onMessage("Skill 项目追加意图已更新；项目链接尚未写入。");
    },
  });
  const blocked = projectBlocked(project, tool);
  const previewPending = previewMutation.isPending || applyMutation.isPending;

  return (
    <>
      <ProjectAssignmentsSection
        title="Skills"
        description="项目项始终是指向中央库的符号链接；全局项不可在项目中禁用。"
        blocked={blocked}
        directApply={directApply}
        error={profileErrorText(optionsQuery.error ?? assignmentMutation.error)}
        pending={optionsQuery.isPending}
        empty={optionsQuery.data?.length === 0}
        excludeFromGit={excludeFromGit}
        onExcludeFromGit={setExcludeFromGit}
        previewPending={previewPending}
        previewLabel={
          directApply
            ? `${toolMetadata(tool).label} Skills 直接应用`
            : `${toolMetadata(tool).label} Skills 同步预览`
        }
        onPreview={() => requestPreview(tool, true)}
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
      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? tool}
        artifactKind="skill"
        applying={applyMutation.isPending}
        onClose={() => {
          if (!applyMutation.isPending) closePreview();
        }}
        onApply={() => {
          if (openPreview) {
            applyMutation.mutate({
              previewId: openPreview.plan.previewId,
              tool: openPreview.tool,
            });
          }
        }}
      />
    </>
  );
}
