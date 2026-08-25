import type { SyncStatus } from "@/bindings/commands";

export const globalTargetStatusLabels = {
  missing: "○ 待初始化",
} satisfies Partial<Record<SyncStatus, string>>;

const globalPreviewBlockingStatuses = new Set<SyncStatus>([
  "failed",
  "policy_blocked",
  "untrusted",
]);

export function isGlobalTargetPreviewBlocked(status: SyncStatus) {
  return globalPreviewBlockingStatuses.has(status);
}

export function globalTargetStatusDescription(
  status: SyncStatus,
  diagnosticCode: string | null,
) {
  if (status === "missing") {
    return "尚未写入受管目标；生成预览会在确认后初始化。";
  }
  if (status === "policy_blocked") {
    if (diagnosticCode === "CLAUDE_POLICY_BLOCKED") {
      return "Claude 管理策略禁止该类自定义目标。";
    }
    if (diagnosticCode === "CLAUDE_POLICY_UNKNOWN") {
      return "Claude 管理策略证据未知，当前按阻止处理。";
    }
    return "当前工具策略阻止修改该目标。";
  }
  if (status === "failed") {
    return "目标能力检测失败，需先修复工具可用性。";
  }
  if (status === "untrusted") {
    return "目标未受信任，当前不能预览。";
  }
  return null;
}
