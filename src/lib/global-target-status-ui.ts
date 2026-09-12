import type { SyncStatus } from "@/bindings/commands";
import type { SyncStatusBadgeTone } from "@/components/sync-status-badge";

interface GlobalTargetStatusPresentation {
  label?: string;
  description: string | null;
  tone?: SyncStatusBadgeTone;
  previewBlocked: boolean;
}

const agentDiagnosticPresentations: Record<
  string,
  Pick<GlobalTargetStatusPresentation, "label" | "description" | "tone">
> = {
  AGENTS_UNSUPPORTED: {
    label: "不支持 Agents",
    description: "当前工具不支持 Agents 目标，无法生成同步预览。",
    tone: "blocked",
  },
  AGENT_FRONTMATTER_INVALID: {
    label: "Agent 格式错误",
    description:
      "Agent 文件的 frontmatter 或 TOML 格式无效，请修复后重新检测。",
    tone: "blocked",
  },
  AGENT_REQUIRED_FIELD_MISSING: {
    label: "Agent 缺少必填字段",
    description: "Agent 必须包含描述和正文，请补齐后重新检测。",
    tone: "warning",
  },
  AGENT_NAME_INVALID: {
    label: "Agent 名称无效",
    description: "名称只能使用小写字母、数字和连字符，长度为 1–64 个字符。",
    tone: "warning",
  },
  AGENT_FIELD_INVALID: {
    label: "Agent 字段无效",
    description: "Agent 字段超出长度限制或包含无效内容，请修复后重新检测。",
    tone: "warning",
  },
  AGENT_NAME_CONFLICT: {
    label: "Agent 名称冲突",
    description: "中央库中已有同名 Agent，请先处理名称冲突。",
    tone: "blocked",
  },
  AGENT_FILE_TOO_LARGE: {
    label: "Agent 文件过大",
    description: "Agent 文件超过可导入大小限制，无法导入。",
    tone: "warning",
  },
  ZCODE_PROJECT_AGENTS_UNSUPPORTED: {
    label: "ZCode 不支持项目 Agents",
    description: "ZCode 官方暂不支持项目级 Agents；请在全局 Agents 页面管理。",
    tone: "blocked",
  },
};

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
  const agentDiagnostic = diagnosticCode
    ? agentDiagnosticPresentations[diagnosticCode]
    : undefined;
  if (agentDiagnostic) {
    return { ...agentDiagnostic, previewBlocked };
  }
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
