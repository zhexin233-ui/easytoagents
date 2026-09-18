import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type McpProjectOptionDto,
  type ProjectDto,
  type Tool,
} from "@/bindings/commands";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { mcpProjectOptionsQueryOptions } from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { invalidateProjectScope } from "@/lib/projects-api";

import { ProjectAssignmentsSection } from "./project-assignments-section";
import { OptionTag, ProjectOptionRow } from "../option-row";
import { projectBlocked } from "./shared";

export function ProjectMcpAssignments({
  project,
  tool,
}: {
  project: ProjectDto;
  tool: Tool;
}) {
  const queryClient = useQueryClient();
  const optionsQuery = useQuery(
    mcpProjectOptionsQueryOptions(project.id, tool),
  );
  const [excludeFromGit, setExcludeFromGit] = useState(false);
  const invalidate = async () => {
    await invalidateProjectScope(queryClient, [
      "project",
      "mcp",
      "skill",
      "hook",
    ]);
  };
  const { requestPreview } = useSyncPreviewFlow({
    artifactKind: "mcp",
    preview: (previewTool, projectId) =>
      commands.previewMcpSync({
        tool: previewTool,
        projectId: projectId ?? project.id,
        excludeFromGit,
      }),
    apply: ({ previewId, tool: previewTool, projectId }) =>
      commands.applyMcpPreview({
        previewId,
        tool: previewTool,
        projectId: projectId ?? project.id,
      }),
    invalidate,
    messages: {
      previewFailed: "生成项目 MCP 预览失败。",
      applyFailed: "应用项目 MCP 预览失败。",
      empty: "该项目只有全局继承 MCP，不需要创建项目配置文件。",
      applied: () => "项目原生配置已通过持久化预览应用并完成写后验证。",
    },
  });
  const assignmentMutation = useMutation({
    mutationFn: async ({
      option,
      assigned,
    }: {
      option: McpProjectOptionDto;
      assigned: boolean;
    }) =>
      unwrapResult(
        await commands.setProjectMcpAssignment({
          projectId: project.id,
          tool,
          mcpId: option.mcpId,
          assigned,
          mcpRowVersion: option.rowVersion,
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
      title="MCP"
      description="全局项只读继承；项目只能追加其他中央 MCP。"
      blocked={blocked}
      error={profileErrorText(optionsQuery.error ?? assignmentMutation.error)}
      pending={optionsQuery.isPending}
      empty={optionsQuery.data?.length === 0}
      excludeFromGit={excludeFromGit}
      onExcludeFromGit={setExcludeFromGit}
    >
      {optionsQuery.data?.map((option) => (
        <ProjectOptionRow
          key={option.mcpId}
          name={option.name}
          state={option.state}
          actionLabel={`${option.name} MCP 项目追加`}
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
