import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type McpProjectOptionDto,
  type ProjectDto,
  type ReadoptMcpTargetResultDto,
  type Tool,
} from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { mcpProjectOptionsQueryOptions } from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { invalidateProjectScope } from "@/lib/projects-api";
import { toolMetadata } from "@/lib/tool-metadata";

import { ProjectAssignmentsSection } from "../assignment-card";
import { OptionTag, ProjectOptionRow } from "../option-row";
import { projectBlocked } from "./shared";

export function ProjectMcpAssignments({
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
  const {
    openPreview,
    requestPreview,
    previewMutation,
    applyMutation,
    readoptMutation,
    closePreview,
  } = useSyncPreviewFlow<ReadoptMcpTargetResultDto>({
    artifactKind: "mcp",
    directApply,
    preview: () =>
      commands.previewMcpSync({
        tool,
        projectId: project.id,
        excludeFromGit,
      }),
    apply: ({ previewId, tool: previewTool }) =>
      commands.applyMcpPreview({
        previewId,
        tool: previewTool,
        projectId: project.id,
      }),
    readopt: (previewTool) =>
      commands.readoptMcpTarget({
        tool: previewTool,
        projectId: project.id,
      }),
    invalidate,
    messages: {
      previewFailed: "生成项目 MCP 预览失败。",
      applyFailed: "应用项目 MCP 预览失败。",
      readoptFailed: "重新接管项目 MCP 目标失败。",
      empty: "该项目只有全局继承 MCP，不需要创建项目配置文件。",
      // 保留项目详情页原有的持久化预览反馈文案；具体资源数量仍可在预览中审阅。
      applied: () => "项目原生配置已通过持久化预览应用并完成写后验证。",
    },
    onReadopted: (result) => {
      onMessage(
        `已以当前内容重新接管（刷新 ${result.updatedItemCount} 个、清理 ${result.removedItemCount} 个条目基线）；请再次点击同步按钮完成写入。`,
      );
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
    onSuccess: async () => {
      await invalidate();
      if (directApply) {
        requestPreview(tool, true);
        return;
      }
      onMessage("MCP 项目追加意图已更新；原生配置尚未写入。");
    },
  });
  const blocked = projectBlocked(project, tool);
  const previewPending = previewMutation.isPending || applyMutation.isPending;

  return (
    <>
      <ProjectAssignmentsSection
        title="MCP"
        description="全局项持续继承且只读；项目只能追加其他中央 MCP。"
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
            ? `${toolMetadata(tool).label} MCP 直接应用`
            : `${toolMetadata(tool).label} MCP 同步预览`
        }
        onPreview={() => requestPreview(tool, true)}
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
      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? tool}
        artifactKind="mcp"
        applying={applyMutation.isPending}
        readopting={readoptMutation.isPending}
        onClose={() => {
          if (!applyMutation.isPending && !readoptMutation.isPending) {
            closePreview();
          }
        }}
        onApply={() => {
          if (openPreview) {
            applyMutation.mutate({
              previewId: openPreview.plan.previewId,
              tool: openPreview.tool,
            });
          }
        }}
        onReadopt={() => {
          if (openPreview) readoptMutation.mutate(openPreview.tool);
        }}
      />
    </>
  );
}
