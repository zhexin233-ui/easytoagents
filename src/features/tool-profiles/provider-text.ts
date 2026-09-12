import type {
  ProviderImportPreviewDto,
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

export function providerImportCredentialText(
  preview: ProviderImportPreviewDto,
): string {
  if (preview.authKind === "official_login") {
    return "官方账号登录（不接管凭据）";
  }
  return preview.apiKeyConfigured ? "密钥已遮罩保存" : "密钥未配置";
}
