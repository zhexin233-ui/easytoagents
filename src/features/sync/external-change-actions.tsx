import { useMutation, useQuery } from "@tanstack/react-query";

import {
  commands,
  type ArtifactKind,
  type SyncStatus,
  type Tool,
} from "@/bindings/commands";
import { Button } from "@/components/ui/button";
import { useNotify } from "@/components/use-notify";
import {
  ProfileRpcError,
  profileErrorText,
  unwrapResult,
} from "@/lib/profile-api";

interface ExternalChangeActionsProps {
  artifactKind: ArtifactKind;
  tool: Tool;
  projectId?: string | null;
  status: SyncStatus;
  onInvalidate?: () => Promise<void>;
  onMatchOrImport?: (() => void) | undefined;
}

const EXTERNAL_CHANGE_STATUSES = new Set<SyncStatus>([
  "external_owned_change",
  "external_non_owned_change",
]);

function isStalePlanError(error: unknown): boolean {
  return (
    error instanceof ProfileRpcError && error.appError.code === "STALE_PREVIEW"
  );
}

/**
 * 被动扫描状态卡的非模态双向动作。
 *
 * 先请求绑定当前 observation 的 ExternalChangePlan，再消费其 previewId；
 * 绝不在组件内直接调用文件写入命令，也不弹出二次确认对话框。
 */
export function ExternalChangeActions({
  artifactKind,
  tool,
  projectId = null,
  status,
  onInvalidate,
  onMatchOrImport,
}: ExternalChangeActionsProps) {
  const { notify } = useNotify();
  const hasExternalChange = EXTERNAL_CHANGE_STATUSES.has(status);
  const planQuery = useQuery({
    queryKey: ["external-change-plan", artifactKind, tool, projectId],
    queryFn: async () =>
      unwrapResult(
        await commands.prepareExternalChangePlan({
          artifactKind,
          tool,
          projectId,
        }),
      ),
    // 计划是一次性 observation 证据；窗口 focus 只刷新状态卡，不在后台重复
    // 签发新的持久化计划。用户点击动作时仍会重新签发并消费最新计划。
    staleTime: 0,
    refetchOnWindowFocus: false,
    enabled: hasExternalChange,
  });
  const overwriteBlocked =
    planQuery.data !== undefined && !planQuery.data.canOverwriteCentral;
  const adoptBlocked =
    planQuery.data !== undefined && !planQuery.data.canAdoptNative;
  const prepareAndApply = async (
    action: "adopt_native" | "overwrite_central",
  ) => {
    const run = async () => {
      const plan = unwrapResult(
        await commands.prepareExternalChangePlan({
          artifactKind,
          tool,
          projectId,
        }),
      );
      const allowed =
        action === "adopt_native"
          ? plan.canAdoptNative
          : plan.canOverwriteCentral;
      if (!allowed) {
        throw new Error(
          (action === "adopt_native"
            ? plan.adoptBlockedReason
            : plan.overwriteBlockedReason) ??
            (action === "adopt_native"
              ? "NATIVE_ADOPTION_UNAVAILABLE"
              : "CENTRAL_OVERWRITE_UNAVAILABLE"),
        );
      }
      return unwrapResult(
        await commands.applyExternalChangePlan({
          previewId: plan.previewId,
          artifactKind,
          tool,
          projectId,
          action,
        }),
      );
    };

    try {
      return await run();
    } catch (error) {
      if (!isStalePlanError(error)) throw error;
      // Apply 前的 observation 可能在状态卡展示期间过期；只重新签发
      // 一次计划，第二次 stale 直接反馈并保留应用内重试入口。
      return run();
    }
  };
  const overwriteMutation = useMutation({
    mutationFn: () => prepareAndApply("overwrite_central"),
    onSuccess: async (result) => {
      await onInvalidate?.();
      notify({
        kind: "success",
        message:
          result.appliedTargets > 0
            ? `已以中央配置覆盖 ${result.appliedTargets} 个原生目标。`
            : "中央配置与原生目标无需写入。",
      });
    },
    onError: async (error) => {
      try {
        await onInvalidate?.();
      } finally {
        notify({
          kind: "error",
          message:
            profileErrorText(error) ?? "以中央配置覆盖原生变化失败，可重试。",
        });
      }
    },
  });

  const adoptMutation = useMutation({
    mutationFn: () => prepareAndApply("adopt_native"),
    onSuccess: async (result) => {
      await onInvalidate?.();
      notify({
        kind: "success",
        message:
          result.appliedTargets > 0
            ? `已采纳 ${result.appliedTargets} 个原生目标，中央配置已更新。`
            : "没有可采纳的原生变化。",
      });
    },
    onError: async (error) => {
      try {
        await onInvalidate?.();
      } finally {
        notify({
          kind: "error",
          message:
            profileErrorText(error) ??
            "采纳原生更改失败，可重试或改用中央配置覆盖。",
        });
      }
    },
  });

  if (!hasExternalChange) return null;

  return (
    <div className="mt-3 flex flex-wrap gap-2">
      {planQuery.isPending ? (
        <p role="status" className="text-muted-foreground basis-full text-xs">
          正在检查可执行的外部变化动作…
        </p>
      ) : null}
      {planQuery.isError ? (
        <div
          role="alert"
          className="flex basis-full flex-wrap items-center gap-2 text-xs"
        >
          <span>
            {profileErrorText(planQuery.error) ??
              "无法读取外部变化计划，动作已安全阻止。"}
          </span>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void planQuery.refetch()}
          >
            重新检测
          </Button>
        </div>
      ) : null}
      {planQuery.data && adoptBlocked ? (
        <p className="text-warning basis-full text-xs">
          原生更改暂不可直接采纳：
          <code className="ml-1">{planQuery.data.adoptBlockedReason}</code>
          {onMatchOrImport ? "，请使用应用内匹配/导入。" : "。"}
        </p>
      ) : null}
      {planQuery.data && overwriteBlocked ? (
        <p className="text-warning basis-full text-xs">
          中央覆盖已阻止：
          <code className="ml-1">
            {planQuery.data.overwriteBlockedReason ??
              "CENTRAL_OVERWRITE_UNAVAILABLE"}
          </code>
        </p>
      ) : null}
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={
          planQuery.isError ||
          overwriteBlocked ||
          overwriteMutation.isPending ||
          adoptMutation.isPending
        }
        onClick={() => overwriteMutation.mutate()}
      >
        {overwriteMutation.isPending ? "正在覆盖…" : "以中央配置覆盖"}
      </Button>
      {status === "external_owned_change" ? (
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={
            planQuery.isError ||
            adoptBlocked ||
            adoptMutation.isPending ||
            overwriteMutation.isPending
          }
          onClick={() => adoptMutation.mutate()}
        >
          {adoptMutation.isPending ? "正在采纳…" : "采纳原生更改"}
        </Button>
      ) : null}
      {onMatchOrImport ? (
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={onMatchOrImport}
        >
          在应用内匹配/导入
        </Button>
      ) : null}
    </div>
  );
}
