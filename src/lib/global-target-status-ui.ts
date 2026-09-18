import type { SyncStatus } from "@/bindings/commands";
import type { SyncStatusBadgeTone } from "@/components/sync-status-badge";
import {
  presentTargetDiagnostic,
  type TargetDiagnosticContext,
} from "@/lib/diagnostic-presentations";

/**
 * 旧页面使用的目标状态投影接口。
 *
 * 具体诊断文案统一由 `diagnostic-presentations` 持有；这里保留包装层，
 * 让既有的预览阻断和首次目录状态行为继续由共享入口决定。
 */
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
  context: TargetDiagnosticContext = {},
): GlobalTargetStatusPresentation {
  const previewBlocked = globalPreviewBlockingStatuses.has(status);

  // 首次目录提示只在对应的扫描状态成立时生效；若后端同时返回了不匹配的
  // 旧码，仍按真实状态渲染，避免把普通失败误报成首次接管/待同步。
  if (
    diagnosticCode === "SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED" &&
    status !== "external_owned_change"
  ) {
    return globalTargetStatusPresentation(status, null, context);
  }
  if (
    diagnosticCode === "SKILL_TARGET_INITIAL_SYNC_PENDING" &&
    status !== "missing" &&
    status !== "external_non_owned_change"
  ) {
    return globalTargetStatusPresentation(status, null, context);
  }
  if (
    (diagnosticCode === "SKILL_TARGET_INITIAL_EMPTY" ||
      diagnosticCode === "SKILL_TARGET_INITIAL_UNMANAGED") &&
    status !== "external_non_owned_change"
  ) {
    return globalTargetStatusPresentation(status, null, context);
  }

  // 首次扫描的三个状态是用户决策提示，不应被普通的外部变化文案覆盖。
  if (
    diagnosticCode === "SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED" &&
    status === "external_owned_change"
  ) {
    const presentation = presentTargetDiagnostic(
      status,
      diagnosticCode,
      context,
    );
    return {
      label: presentation.label,
      description: presentation.description,
      tone: presentation.tone,
      previewBlocked,
    };
  }
  if (
    diagnosticCode === "SKILL_TARGET_INITIAL_SYNC_PENDING" &&
    (status === "missing" || status === "external_non_owned_change")
  ) {
    const presentation = presentTargetDiagnostic(
      status,
      diagnosticCode,
      context,
    );
    return {
      label: presentation.label,
      description: presentation.description,
      tone: presentation.tone,
      previewBlocked,
    };
  }
  if (status === "external_non_owned_change") {
    if (diagnosticCode === "SKILL_TARGET_INITIAL_EMPTY") {
      const presentation = presentTargetDiagnostic(
        status,
        diagnosticCode,
        context,
      );
      return {
        label: presentation.label,
        description: presentation.description,
        tone: presentation.tone,
        previewBlocked,
      };
    }
    if (diagnosticCode === "SKILL_TARGET_INITIAL_UNMANAGED") {
      const presentation = presentTargetDiagnostic(
        status,
        diagnosticCode,
        context,
      );
      return {
        label: presentation.label,
        description: presentation.description,
        tone: presentation.tone,
        previewBlocked,
      };
    }
    // 这些码只是扫描结果对状态的内部标记；状态徽章和 ExternalChangeActions
    // 已经提供了完整的用户语义，不再用一条重复说明覆盖它。
    if (
      diagnosticCode === "EXTERNAL_NON_OWNED_CHANGE" ||
      diagnosticCode === "CENTRAL_SKILL_CONTENT_CHANGED"
    ) {
      return { description: null, previewBlocked };
    }
  }
  if (
    status === "external_owned_change" &&
    diagnosticCode === "EXTERNAL_OWNED_CHANGE"
  ) {
    return { description: null, previewBlocked };
  }

  // 无诊断码时保留旧状态卡的空描述（状态徽章本身已经提供标签）；有诊断码
  // 时始终经过 registry，未知码也必须有安全的可执行 fallback。
  if (!diagnosticCode) {
    if (status === "missing") {
      return {
        label: "待初始化",
        description: "尚未写入受管目标；分配条目后会自动初始化。",
        tone: "warning",
        previewBlocked,
      };
    }
    if (status === "policy_blocked") {
      return {
        label: "策略阻止",
        description: "当前工具策略阻止修改该目标。",
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

  const presentation = presentTargetDiagnostic(status, diagnosticCode, context);
  return {
    label: presentation.label,
    description: presentation.description,
    tone: presentation.tone,
    previewBlocked: presentation.previewBlocked,
  };
}
