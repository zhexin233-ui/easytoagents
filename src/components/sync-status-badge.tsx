import type { ChangeKind, SyncStatus } from "@/bindings/commands";
import { toneClass } from "@/lib/tone-class";

const statusLabels: Record<SyncStatus, string> = {
  in_sync: "✓ 已同步",
  external_non_owned_change: "△ 非受管变更",
  external_owned_change: "! 受管内容冲突",
  missing: "○ 目标缺失",
  parse_error: "! 格式错误",
  permission_denied: "! 权限不足",
  policy_blocked: "⛔ 策略阻止",
  untrusted: "⛔ 项目未信任",
  target_type_changed: "! 目标类型变化",
  failed: "× 检测失败",
};

const changeLabels: Record<ChangeKind, string> = {
  add: "新增",
  update: "更新",
  delete: "删除",
  unchanged: "不变",
  warning: "警告",
  conflict: "冲突",
};

const blockingStatuses = new Set<SyncStatus>([
  "external_owned_change",
  "parse_error",
  "permission_denied",
  "policy_blocked",
  "untrusted",
  "target_type_changed",
  "failed",
]);

export type SyncStatusBadgeTone = "blocked" | "warning" | "success" | "muted";

export function SyncStatusBadge({
  status,
  changeKind,
  label,
  labels,
  tone,
}: {
  status: SyncStatus;
  changeKind?: ChangeKind;
  label?: string | undefined;
  labels?: Partial<Record<SyncStatus, string>>;
  tone?: SyncStatusBadgeTone | undefined;
}) {
  const blocked = blockingStatuses.has(status) || changeKind === "conflict";
  const warning =
    status === "external_non_owned_change" ||
    status === "missing" ||
    changeKind === "warning";
  const resolvedTone =
    tone ?? (blocked ? "blocked" : warning ? "warning" : "success");
  const statusLabel = label ?? labels?.[status] ?? statusLabels[status];
  return (
    <span
      className={`inline-flex rounded-full border px-2 py-1 text-xs font-medium ${toneClass(
        resolvedTone === "blocked"
          ? "destructive"
          : resolvedTone === "warning"
            ? "warning"
            : resolvedTone === "muted"
              ? "neutral"
              : "success",
      )}`}
    >
      {changeKind ? `${changeLabels[changeKind]} · ` : ""}
      {statusLabel}
      <span className="sr-only">{status}</span>
    </span>
  );
}
