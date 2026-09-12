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
  DialogBody,
  DialogContent,
  DialogFooter,
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
      >
        <DialogHeader>
          <h2 id="hook-picker-title" className="text-[15px] font-semibold">
            添加到 {props.eventLabel}（{props.event}）
          </h2>
        </DialogHeader>
        <DialogBody className="space-y-4">
          {error ? (
            <p role="alert" className="text-destructive">
              {error}
            </p>
          ) : null}
          <div className="space-y-2">
            {props.hooks.length === 0 ? (
              <p className="text-muted-foreground">中央库尚无 Hook。</p>
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
        </DialogBody>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={assign.isPending}
            onClick={close}
          >
            取消
          </Button>
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>
  );
}
