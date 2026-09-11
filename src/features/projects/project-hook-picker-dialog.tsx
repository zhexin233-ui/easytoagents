import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  commands,
  type HookEvent,
  type HookProjectOptionDto,
  type ProjectDto,
  type Tool,
} from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import {
  DialogContent,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { invalidateProjectScope } from "@/lib/projects-api";

interface ProjectHookPickerDialogProps {
  project: ProjectDto;
  tool: Tool;
  event: HookEvent;
  eventLabel: string;
  options: HookProjectOptionDto[];
  onClose: () => void;
  onAssigned: (message: string) => void;
}

/// 从中央库选择 Hook 追加到项目的指定事件分组。
/// 全局继承项不出现在列表中（只读）；同一 (项目, 工具, hook) 只有一个
/// 生效事件，重复添加会切换事件。
export function ProjectHookPickerDialog(props: ProjectHookPickerDialogProps) {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const assign = useMutation({
    mutationFn: async ({ option }: { option: HookProjectOptionDto }) =>
      unwrapResult(
        await commands.setProjectHookAssignment({
          projectId: props.project.id,
          tool: props.tool,
          hookId: option.hookId,
          event: props.event,
          assigned: true,
          hookRowVersion: option.rowVersion,
          projectRowVersion: props.project.rowVersion,
        }),
      ),
    onSuccess: async (_result, { option }) => {
      await invalidateProjectScope(queryClient, ["project", "hook"]);
      props.onAssigned(
        `${option.name} 已加入项目 ${props.eventLabel} 分组（${props.event}）。`,
      );
    },
    onError: (mutationError) => {
      setError(profileErrorText(mutationError) ?? "项目追加失败。");
    },
  });
  const close = () => {
    if (!assign.isPending) props.onClose();
  };
  const { dialogRef } = useDialogFocus(true, close);
  const available = props.options.filter(
    (option) => option.state === "available" && option.selectable,
  );

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy="project-hook-picker-title"
        describedBy="project-hook-picker-description"
        className="max-h-[90vh] max-w-2xl"
      >
        <DialogHeader>
          <h2 id="project-hook-picker-title" className="text-xl font-semibold">
            添加到项目 {props.eventLabel}（{props.event}）
          </h2>
          <Button
            variant="outline"
            disabled={assign.isPending}
            onClick={close}
            aria-label="关闭项目 Hook 选择器"
          >
            关闭
          </Button>
        </DialogHeader>
        <p
          id="project-hook-picker-description"
          className="text-muted-foreground mt-3 text-sm"
        >
          项目追加只更新中央意图；原生写入仍需生成项目预览并 Apply。
        </p>
        {error ? (
          <p role="alert" className="text-destructive mt-4 text-sm">
            {error}
          </p>
        ) : null}
        <div className="mt-4 space-y-2">
          {available.length === 0 ? (
            <p className="text-muted-foreground text-sm">
              中央库中没有可追加的 Hook（已追加与全局继承项不会重复出现）。
            </p>
          ) : null}
          {available.map((option) => (
            <div
              key={option.hookId}
              className="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm"
            >
              <div className="min-w-0">
                <p className="truncate font-medium" title={option.name}>
                  {option.name}
                  {!option.enabled ? "（已停用）" : ""}
                </p>
                <p className="text-muted-foreground mt-1 text-xs">
                  默认事件 {option.event}
                </p>
              </div>
              <Button
                size="sm"
                aria-label={`添加 ${option.name} 到项目 ${props.eventLabel}`}
                disabled={assign.isPending}
                onClick={() => assign.mutate({ option })}
              >
                添加
              </Button>
            </div>
          ))}
        </div>
      </DialogContent>
    </DialogOverlay>
  );
}
