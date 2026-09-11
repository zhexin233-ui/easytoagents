import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type HookEvent,
  type HookProjectOptionDto,
  type ProjectDto,
  type Tool,
} from "@/bindings/commands";
import { ChangePreviewDialog } from "@/components/change-preview-dialog";
import { Button } from "@/components/ui/button";
import { useSyncPreviewFlow } from "@/features/sync/use-sync-preview-flow";
import { hookProjectOptionsQueryOptions } from "@/lib/hooks-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { invalidateProjectScope } from "@/lib/projects-api";
import { toolMetadata } from "@/lib/tool-metadata";
import {
  HOOK_EVENT_GROUPS,
  hookEventSupportedByTool,
} from "@/features/hooks/hook-events";
import { ProjectHookPickerDialog } from "@/features/projects/project-hook-picker-dialog";

import { ProjectAssignmentsSection } from "./project-assignments-section";
import { projectBlocked } from "./shared";

export function ProjectHookAssignments({
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
    hookProjectOptionsQueryOptions(project.id, tool),
  );
  const [excludeFromGit, setExcludeFromGit] = useState(false);
  const [openPicker, setOpenPicker] = useState<{
    event: HookEvent;
    eventLabel: string;
  } | null>(null);
  const invalidate = async () => {
    await invalidateProjectScope(queryClient, ["project", "hook"]);
  };
  const {
    openPreview,
    requestPreview,
    previewMutation,
    applyMutation,
    closePreview,
  } = useSyncPreviewFlow({
    artifactKind: "hook",
    directApply,
    preview: () =>
      commands.previewHookSync({
        tool,
        projectId: project.id,
        excludeFromGit,
      }),
    apply: ({ previewId, tool: previewTool }) =>
      commands.applyHookPreview({
        previewId,
        tool: previewTool,
        projectId: project.id,
      }),
    invalidate,
    messages: {
      previewFailed: "生成项目 Hook 预览失败。",
      applyFailed: "应用项目 Hook 预览失败。",
      empty: "该项目只有全局继承 Hook，不需要创建项目配置文件。",
      applied: () => "项目原生配置已通过持久化预览应用并完成写后验证。",
    },
  });
  const assignmentMutation = useMutation({
    mutationFn: async ({
      option,
      assigned,
    }: {
      option: HookProjectOptionDto;
      assigned: boolean;
    }) =>
      unwrapResult(
        await commands.setProjectHookAssignment({
          projectId: project.id,
          tool,
          hookId: option.hookId,
          event: option.assignedEvent ?? option.event,
          assigned,
          hookRowVersion: option.rowVersion,
          projectRowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async () => {
      await invalidate();
      if (directApply) {
        requestPreview(tool, true);
        return;
      }
      onMessage("Hook 项目追加意图已更新；原生配置尚未写入。");
    },
  });
  const blocked = projectBlocked(project, tool);
  const options = optionsQuery.data ?? [];
  const inherited = options.filter((option) => option.state === "inherited");
  const visibleEventGroups = useMemo(
    () =>
      HOOK_EVENT_GROUPS.map((group) => ({
        ...group,
        events: group.events.filter((item) =>
          hookEventSupportedByTool(tool, item.event),
        ),
      })).filter((group) => group.events.length > 0),
    [tool],
  );
  const previewPending = previewMutation.isPending || applyMutation.isPending;

  return (
    <>
      <ProjectAssignmentsSection
        title="Hooks"
        description="事件随项目追加指定，分组方式与全局 Hooks 管理一致；全局项持续继承且只读。"
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
            ? `${toolMetadata(tool).label} Hooks 直接应用`
            : `${toolMetadata(tool).label} Hooks 同步预览`
        }
        onPreview={() => requestPreview(tool, true)}
      >
        {inherited.length > 0 ? (
          <p className="text-muted-foreground text-xs">
            全局继承（只读）：
            {inherited.map((option) => option.name).join("、")}
          </p>
        ) : null}
        <div className="space-y-4">
          {visibleEventGroups.map((group) => (
            <div key={group.label}>
              <h4 className="text-xs font-semibold text-slate-500 dark:text-slate-400">
                {group.label}
              </h4>
              <div className="mt-2 space-y-2">
                {group.events.map(({ event, label }) => {
                  const assigned = options.filter(
                    (option) =>
                      option.state === "selected" &&
                      option.assignedEvent === event,
                  );
                  return (
                    <article
                      key={event}
                      className="rounded-lg border p-3 text-sm"
                      aria-label={`项目 ${label}（${event}）分组`}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <p className="text-xs font-medium">
                          {label}
                          <span className="text-muted-foreground ml-2">
                            {event}
                          </span>
                        </p>
                        <Button
                          size="sm"
                          variant="outline"
                          aria-label={`往项目 ${label} 分组添加 Hook`}
                          onClick={() =>
                            setOpenPicker({ event, eventLabel: label })
                          }
                        >
                          从中央列表添加
                        </Button>
                      </div>
                      {assigned.length === 0 ? (
                        <p className="text-muted-foreground mt-2 text-xs">
                          该分组暂无项目追加。
                        </p>
                      ) : (
                        <ul className="mt-2 space-y-2">
                          {assigned.map((option) => (
                            <li
                              key={option.hookId}
                              className="flex items-center justify-between gap-3 rounded border bg-slate-50 px-3 py-2 text-xs dark:bg-slate-900"
                            >
                              <span className="min-w-0 truncate">
                                {option.name}
                                {!option.enabled ? "（已停用）" : ""}
                              </span>
                              <Button
                                size="sm"
                                variant="outline"
                                aria-label={`从项目 ${label} 分组移除 ${option.name}`}
                                disabled={assignmentMutation.isPending}
                                onClick={() =>
                                  assignmentMutation.mutate({
                                    option,
                                    assigned: false,
                                  })
                                }
                              >
                                移除
                              </Button>
                            </li>
                          ))}
                        </ul>
                      )}
                    </article>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
        {openPicker ? (
          <ProjectHookPickerDialog
            project={project}
            tool={tool}
            event={openPicker.event}
            eventLabel={openPicker.eventLabel}
            options={options}
            onClose={() => setOpenPicker(null)}
            onAssigned={(message) => {
              setOpenPicker(null);
              onMessage(message);
              if (directApply) requestPreview(tool, true);
            }}
          />
        ) : null}
      </ProjectAssignmentsSection>
      <ChangePreviewDialog
        preview={openPreview?.plan ?? null}
        tool={openPreview?.tool ?? tool}
        artifactKind="hook"
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
