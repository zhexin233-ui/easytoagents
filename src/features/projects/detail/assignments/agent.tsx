import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type AgentProjectOptionDto,
  type ProjectDto,
  type ReadoptAgentTargetResultDto,
  type Tool,
} from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { agentProjectOptionsQueryOptions } from "@/lib/agents-api";
import { profileErrorText, unwrapResult } from "@/lib/rpc";
import { invalidateProjectScope } from "@/lib/projects-api";
import { toolMetadata } from "@/lib/tool-metadata";

import { OptionTag, ProjectOptionRow } from "../option-row";
import { ProjectAssignmentsSection } from "./project-assignments-section";
import { projectBlocked } from "./shared";

interface ProjectAgentAssignmentsProps {
  project: ProjectDto;
  tool: Tool;
  directApply: boolean;
  onMessage: (message: string) => void;
}

/** 项目级 Agents 追加管理；全局分配项在列表中保持只读继承。 */
export function ProjectAgentAssignments({
  project,
  tool,
  directApply,
  onMessage,
}: ProjectAgentAssignmentsProps) {
  const queryClient = useQueryClient();
  const optionsQuery = useQuery(
    agentProjectOptionsQueryOptions(project.id, tool),
  );
  const [excludeFromGit, setExcludeFromGit] = useState(false);
  const {
    openPreview,
    requestPreview,
    previewMutation,
    applyMutation,
    readoptMutation,
    closePreview,
  } = useSyncPreviewFlow<ReadoptAgentTargetResultDto>({
    artifactKind: "agent",
    directApply,
    preview: () =>
      commands.previewAgentSync({
        tool,
        projectId: project.id,
        excludeFromGit,
      }),
    apply: ({ previewId, tool: previewTool }) =>
      commands.applyAgentPreview({
        previewId,
        tool: previewTool,
        projectId: project.id,
      }),
    readopt: (previewTool, targetPath) => {
      if (!targetPath) {
        throw new Error("重新接管 Agent 目标缺少文件路径。");
      }
      return commands.readoptAgentTarget({
        tool: previewTool,
        projectId: project.id,
        targetPath,
      });
    },
    invalidate: async () => {
      await invalidateProjectScope(queryClient, ["project", "agent"]);
    },
    messages: {
      previewFailed: "生成项目 Agents 预览失败。",
      applyFailed: "应用项目 Agents 预览失败。",
      empty: "该项目没有需要写入的项目级 Agent 文件。",
      applied: () => "项目 Agents 已通过持久化预览应用并完成写后验证。",
      readoptFailed: "重新接管项目 Agents 目标失败。",
    },
    onReadopted: () => {
      onMessage(
        "已以当前内容重新接管项目 Agent 目标；请再次点击同步按钮完成写入。",
      );
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
    onSuccess: async () => {
      await invalidateProjectScope(queryClient, ["project", "agent"]);
      if (directApply) {
        requestPreview(tool, true);
        return;
      }
      onMessage(
        "项目 Agent 追加意图已更新；原生 Agent 文件尚未写入。请预览并确认应用。",
      );
    },
  });
  const blocked = projectBlocked(project, tool);
  const options = optionsQuery.data ?? [];
  const previewPending = previewMutation.isPending || applyMutation.isPending;

  return (
    <>
      <ProjectAssignmentsSection
        title="Agents"
        description="全局 Agent 只读继承；项目可以追加其他中央 Agents。"
        blocked={blocked}
        directApply={directApply}
        error={profileErrorText(optionsQuery.error ?? assignmentMutation.error)}
        pending={optionsQuery.isPending}
        empty={options.length === 0}
        excludeFromGit={excludeFromGit}
        onExcludeFromGit={setExcludeFromGit}
        previewPending={previewPending}
        previewLabel={
          directApply
            ? `${toolMetadata(tool).label} Agents 直接应用`
            : `${toolMetadata(tool).label} Agents 同步预览`
        }
        onPreview={() => requestPreview(tool, true)}
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
      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? tool}
        artifactKind="agent"
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
        onReadopt={(targetPath) => {
          if (openPreview) {
            readoptMutation.mutate({
              tool: openPreview.tool,
              targetPath,
            });
          }
        }}
      />
    </>
  );
}
