import type { SyncStatus } from "@/bindings/commands";
import type { SyncStatusBadgeTone } from "@/components/sync-status-badge";

interface GlobalTargetStatusPresentation {
  label?: string;
  description: string | null;
  tone?: SyncStatusBadgeTone;
  previewBlocked: boolean;
}

const globalPreviewBlockingStatuses = new Set<SyncStatus>([
  "failed",
  "policy_blocked",
  "untrusted",
]);

export function globalTargetStatusPresentation(
  status: SyncStatus,
  diagnosticCode: string | null,
): GlobalTargetStatusPresentation {
  const previewBlocked = globalPreviewBlockingStatuses.has(status);
  if (status === "missing") {
    return {
      label: "○ 待初始化",
      description: "尚未写入受管目标；生成预览会在确认后初始化。",
      tone: "warning",
      previewBlocked,
    };
  }
  if (status === "policy_blocked") {
    if (diagnosticCode === "CLAUDE_POLICY_UNKNOWN") {
      return {
        label: "△ 策略状态待确认",
        description:
          "无法确认 Claude 管理策略是否允许该类自定义目标，当前已安全阻止预览。",
        tone: "warning",
        previewBlocked,
      };
    }
    return {
      label: "⛔ 策略阻止",
      description:
        diagnosticCode === "CLAUDE_POLICY_BLOCKED"
          ? "Claude 管理策略禁止该类自定义目标。"
          : "当前工具策略阻止修改该目标。",
      tone: "blocked",
      previewBlocked,
    };
  }
  if (status === "failed") {
    return {
      description: "目标能力检测失败，需先修复工具可用性。",
      previewBlocked,
    };
  }
  if (status === "untrusted") {
    return {
      description: "目标未受信任，当前不能预览。",
      previewBlocked,
    };
  }
  return { description: null, previewBlocked };
}
