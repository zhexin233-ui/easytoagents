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
  options: { directApply?: boolean } = {},
): GlobalTargetStatusPresentation {
  const { directApply = false } = options;
  const previewBlocked = globalPreviewBlockingStatuses.has(status);
  if (
    diagnosticCode === "SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED" &&
    status === "external_owned_change"
  ) {
    return {
      label: "已有同名安装，待接管",
      description:
        "工具目录中已有同名技能，可检测并接管；内容不同需先处理差异。",
      tone: "warning",
      previewBlocked,
    };
  }
  if (
    diagnosticCode === "SKILL_TARGET_INITIAL_SYNC_PENDING" &&
    (status === "missing" || status === "external_non_owned_change")
  ) {
    return {
      label: "已分配，待同步",
      description: directApply
        ? "分配已写入，尚未写入工具目录；重新切换分配可触发自动同步。"
        : "分配已写入，尚未写入工具目录；点击“预览全局同步”并确认应用。",
      tone: "warning",
      previewBlocked,
    };
  }
  if (status === "external_non_owned_change") {
    if (diagnosticCode === "SKILL_TARGET_INITIAL_EMPTY") {
      return {
        label: "空目录，待配置",
        description: "目标目录为空；可先导入技能到中央库，再分配并预览同步。",
        tone: "warning",
        previewBlocked,
      };
    }
    if (diagnosticCode === "SKILL_TARGET_INITIAL_UNMANAGED") {
      return {
        label: "未纳入同步管理",
        description:
          "已有目录尚未纳管；可检测其中的用户技能并复制到中央库，不会自动接管。",
        tone: "warning",
        previewBlocked,
      };
    }
  }
  if (status === "missing") {
    return {
      label: "待初始化",
      description: directApply
        ? "尚未写入受管目标；分配条目后会自动初始化。"
        : "尚未写入受管目标；生成预览会在确认后初始化。",
      tone: "warning",
      previewBlocked,
    };
  }
  if (status === "policy_blocked") {
    if (diagnosticCode === "CLAUDE_POLICY_UNKNOWN") {
      return {
        label: "策略状态待确认",
        description: "无法确认 Claude 管理策略，预览已阻止。",
        tone: "warning",
        previewBlocked,
      };
    }
    return {
      label: "策略阻止",
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
