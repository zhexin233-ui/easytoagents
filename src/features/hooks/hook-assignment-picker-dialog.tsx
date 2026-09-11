import { useState } from "react";
import { useMutation } from "@tanstack/react-query";

import {
  commands,
  type HookDto,
  type HookEvent,
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
import { hooksKeys } from "@/lib/hooks-api";
import { useQueryClient } from "@tanstack/react-query";

interface HookAssignmentPickerDialogProps {
  tool: Tool;
  event: HookEvent;
  eventLabel: string;
  hooks: HookDto[];
  onClose: () => void;
  onAssigned: (message: string) => void;
}

/// 从中央库选择 Hook 加入指定工具的事件分组。
/// 同一 (tool, hook) 只有一个生效事件：选择已分配到其他事件的 Hook
/// 会把该工具上的生效事件切换为当前分组事件（行内说明）。
export function HookAssignmentPickerDialog(
  props: HookAssignmentPickerDialogProps,
) {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const assign = useMutation({
    mutationFn: async ({ hook }: { hook: HookDto }) => {
      const current = hook.globalAssignments.find(
        (assignment) => assignment.tool === props.tool,
      );
      return unwrapResult(
        await commands.setGlobalHookAssignment({
          tool: props.tool,
          hookId: hook.id,
          event: props.event,
          assigned: current?.event !== props.event,
          rowVersion: hook.rowVersion,
        }),
      );
    },
    onSuccess: async (_result, { hook }) => {
      await queryClient.invalidateQueries({ queryKey: hooksKeys.all });
      props.onAssigned(
        `${hook.name} 已加入 ${props.eventLabel} 分组（${props.event}）。`,
      );
    },
    onError: (mutationError) => {
      setError(profileErrorText(mutationError) ?? "分配失败。");
    },
  });
  const close = () => {
    if (!assign.isPending) props.onClose();
  };
  const { dialogRef } = useDialogFocus(true, close);

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy="hook-picker-title"
        describedBy="hook-picker-description"
        className="max-h-[90vh] max-w-2xl"
      >
        <DialogHeader>
          <h2 id="hook-picker-title" className="text-xl font-semibold">
            添加到 {props.eventLabel}（{props.event}）
          </h2>
          <Button
            variant="outline"
            disabled={assign.isPending}
            onClick={close}
            aria-label="关闭选择器"
          >
            关闭
          </Button>
        </DialogHeader>
        <p
          id="hook-picker-description"
          className="text-muted-foreground mt-3 text-sm"
        >
          从中央库选择要加入该事件分组的
          Hook；分配只更新中央意图，原生写入仍需预览后 Apply。
        </p>
        {error ? (
          <p role="alert" className="text-destructive mt-4 text-sm">
            {error}
          </p>
        ) : null}
        <div className="mt-4 space-y-2">
          {props.hooks.length === 0 ? (
            <p className="text-muted-foreground text-sm">
              中央库尚无 Hook，请先在上方中央列表创建或导入。
            </p>
          ) : null}
          {props.hooks.map((hook) => {
            const current = hook.globalAssignments.find(
              (assignment) => assignment.tool === props.tool,
            );
            const inThisGroup = current?.event === props.event;
            return (
              <div
                key={hook.id}
                className="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm"
              >
                <div className="min-w-0">
                  <p className="truncate font-medium" title={hook.name}>
                    {hook.name}
                    {!hook.enabled ? "（已停用）" : ""}
                  </p>
                  <p className="text-muted-foreground mt-1 text-xs">
                    {hook.event} ·{" "}
                    {current
                      ? inThisGroup
                        ? "已在该分组"
                        : `当前生效事件 ${current.event}，添加后将切换`
                      : "未分配到该工具"}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant={inThisGroup ? "outline" : "default"}
                  disabled={inThisGroup || assign.isPending}
                  aria-label={`添加 ${hook.name} 到 ${props.eventLabel}`}
                  onClick={() => assign.mutate({ hook })}
                >
                  {inThisGroup ? "已在该分组" : "添加"}
                </Button>
              </div>
            );
          })}
        </div>
      </DialogContent>
    </DialogOverlay>
  );
}
