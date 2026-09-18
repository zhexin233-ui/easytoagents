import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type AgentProjectOptionDto,
  type ProjectDto,
  type Tool,
} from "@/bindings/commands";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { agentProjectOptionsQueryOptions } from "@/lib/agents-api";
import { profileErrorText, unwrapResult } from "@/lib/rpc";
import { invalidateProjectScope } from "@/lib/projects-api";

import { OptionTag, ProjectOptionRow } from "../option-row";
import { ProjectAssignmentsSection } from "./project-assignments-section";
import { projectBlocked } from "./shared";

interface ProjectAgentAssignmentsProps {
  project: ProjectDto;
  tool: Tool;
}

/** 项目级 Agents 追加管理；全局分配项在列表中保持只读继承。 */
export function ProjectAgentAssignments({
  project,
  tool,
}: ProjectAgentAssignmentsProps) {
  const queryClient = useQueryClient();
  const optionsQuery = useQuery(
    agentProjectOptionsQueryOptions(project.id, tool),
  );
  const [excludeFromGit, setExcludeFromGit] = useState(false);
  const { requestPreview } = useSyncPreviewFlow({
    artifactKind: "agent",
    preview: (previewTool, projectId) =>
      commands.previewAgentSync({
        tool: previewTool,
        projectId: projectId ?? project.id,
        excludeFromGit,
      }),
    apply: ({ previewId, tool: previewTool, projectId }) =>
      commands.applyAgentPreview({
        previewId,
        tool: previewTool,
        projectId: projectId ?? project.id,
      }),
    invalidate: async () => {
      await invalidateProjectScope(queryClient, ["project", "agent"]);
    },
    messages: {
      previewFailed: "生成项目 Agents 预览失败。",
      applyFailed: "应用项目 Agents 预览失败。",
      empty: "该项目没有需要写入的项目级 Agent 文件。",
      applied: () => "项目 Agents 已通过持久化预览应用并完成写后验证。",
    },
  });
  const assignmentMutation = useMutation({
    mutationFn: async ({
      option,
      assigned,
    }: {
      option: AgentProjectOptionDto;
      assigned: boolean;
    }) =>
      unwrapResult(
        await commands.setProjectAgentAssignment({
          projectId: project.id,
          tool,
          agentId: option.agentId,
          assigned,
          agentRowVersion: option.rowVersion,
          projectRowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async (result) => {
      await invalidateProjectScope(queryClient, ["project", "agent"]);
      await requestPreview(result.affectedSyncScopes ?? []);
    },
  });
  const blocked = projectBlocked(project, tool);
  const options = optionsQuery.data ?? [];

  return (
    <ProjectAssignmentsSection
      title="Agents"
      description="全局 Agent 只读继承；项目可以追加其他中央 Agents。"
      blocked={blocked}
      error={profileErrorText(optionsQuery.error ?? assignmentMutation.error)}
      pending={optionsQuery.isPending}
      empty={options.length === 0}
      excludeFromGit={excludeFromGit}
      onExcludeFromGit={setExcludeFromGit}
    >
      {options.map((option) => (
        <ProjectOptionRow
          key={option.agentId}
          name={option.name}
          state={option.state}
          actionLabel={`${option.name} Agents 项目追加`}
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
          {!option.enabled ? (
            <OptionTag tone="warning">已停用</OptionTag>
          ) : null}
        </ProjectOptionRow>
      ))}
    </ProjectAssignmentsSection>
  );
}
