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
  if (status === "external_non_owned_change") {
    if (diagnosticCode === "SKILL_TARGET_INITIAL_EMPTY") {
      return {
        label: "○ 空目录，待配置",
        description:
          "目标目录为空，尚未配置同步；可先导入技能到中央库，再分配并预览同步。",
        tone: "warning",
        previewBlocked,
      };
    }
    if (diagnosticCode === "SKILL_TARGET_INITIAL_UNMANAGED") {
      return {
        label: "○ 未纳入同步管理",
        description:
          "已有目录尚未纳入同步管理；可检测其中的用户技能并复制到中央库。导入不会自动接管原有安装。",
        tone: "warning",
        previewBlocked,
      };
    }
  }
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
