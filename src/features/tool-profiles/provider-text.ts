import type {
  PiProviderSummaryDto,
  ProviderImportCandidateDto,
  ProviderImportCandidateStatus,
  ProviderProfileDto,
  Tool,
} from "@/bindings/commands";

/** 支持"官方账号登录"渠道类型的工具；其余工具只有 API Key 渠道。 */
export const OFFICIAL_LOGIN_TOOLS: ReadonlySet<Tool> = new Set<Tool>([
  "claude",
  "codex",
]);

export function isOfficialLoginProfile(profile: ProviderProfileDto): boolean {
  return profile.options.authKind === "official_login";
}

/** 渠道列表与导入预览共用的模型文案；空模型表示交给工具默认值。 */
export function providerModelText(defaultModel: string): string {
  return defaultModel || "工具默认模型";
}

export function providerCredentialText(profile: ProviderProfileDto): string {
  if (isOfficialLoginProfile(profile)) {
    return "官方账号登录";
  }
  return profile.apiKeyConfigured ? "密钥已遮罩保存" : "密钥未配置";
}

/** 导入候选的凭据文案；不再依赖预览级字段。 */
export function providerCandidateCredentialText(
  candidate: ProviderImportCandidateDto,
): string {
  if (candidate.authKind === "official_login") {
    return "官方账号登录（不接管凭据）";
  }
  return candidate.apiKeyConfigured ? "密钥已遮罩保存" : "密钥未配置";
}

export const providerCandidateStatusText: Record<
  ProviderImportCandidateStatus,
  string
> = {
  importable: "可导入",
  already_managed: "已纳入管理",
  invalid: "配置无效",
};

/** 候选原因码的固定文案；未知码只显示原始码，不回显任何原生值。 */
const candidateReasonText: Record<string, string> = {
  PI_PROVIDER_ENTRY_INVALID: "原生条目不是 JSON 对象",
  PI_PROVIDER_ID_INVALID: "原生 provider id 非法",
  PI_PROVIDER_FIELDS_INVALID: "缺少接入地址或 API Key",
};

export function providerCandidateReasonText(reason: string): string {
  return candidateReasonText[reason] ?? reason;
}

/**
 * Pi 渠道的只读摘要：API 格式与模型数量。
 *
 * 导入后档案只保存名称、地址、密钥与默认模型，`api` 与完整 `models` 由档案的
 * 扩展字段承载；这里把它显示出来，用户才能确认「其他内容确实保留了」。
 */
export function piProviderSummaryText(
  summary: PiProviderSummaryDto | null,
): string | null {
  if (!summary) return null;
  const parts: string[] = [];
  if (summary.apiFormat) {
    parts.push(`API 格式 ${summary.apiFormat}`);
  }
  parts.push(`${summary.models.length} 个模型`);
  return parts.join(" · ");
}

/** 模型 id 列表，用于 title 悬浮提示（不含任何凭据）。 */
export function piProviderModelsTitle(
  summary: PiProviderSummaryDto | null,
): string | undefined {
  if (!summary || summary.models.length === 0) return undefined;
  return summary.models
    .map((model) => (model.name ? `${model.id}（${model.name}）` : model.id))
    .join("\n");
}
