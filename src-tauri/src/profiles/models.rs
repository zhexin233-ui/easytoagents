//! Provider 与提示词的 RPC DTO 及工具级字段合同。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::{
    adapters::{
        PolicyState, PromptOverrideState, TargetCapability, ToolAvailabilityState,
        CLAUDE_RESERVED_ENV_KEYS,
    },
    domain::{ArtifactKind, ArtifactName, SyncScopeDto, Tool},
    error::AppError,
    security::env_entry_is_manageable,
};

pub const CODEX_BEARER_TOKEN_WARNING: &str =
    "Codex 官方不推荐在配置文件中保存明文 experimental_bearer_token。";
pub const NEW_SESSION_NOTICE: &str = "渠道与全局提示词通常从新的 Claude/Codex 会话开始生效。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
pub enum ClaudeCredentialEnvKey {
    #[serde(rename = "ANTHROPIC_API_KEY")]
    ApiKey,
    #[serde(rename = "ANTHROPIC_AUTH_TOKEN")]
    AuthToken,
}

impl ClaudeCredentialEnvKey {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "ANTHROPIC_API_KEY",
            Self::AuthToken => "ANTHROPIC_AUTH_TOKEN",
        }
    }

    pub fn from_stable_str(value: &str) -> Option<Self> {
        match value {
            "ANTHROPIC_API_KEY" => Some(Self::ApiKey),
            "ANTHROPIC_AUTH_TOKEN" => Some(Self::AuthToken),
            _ => None,
        }
    }
}

/// 渠道的认证方式：`ApiKey` 走第三方/自定义接入地址加密钥；`OfficialLogin`
/// 不保存任何接入地址或密钥，原生配置回到工具自带的官方账号登录。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthKind {
    #[default]
    ApiKey,
    OfficialLogin,
}

impl ProviderAuthKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::OfficialLogin => "official_login",
        }
    }

    pub fn from_stable_str(value: &str) -> Option<Self> {
        match value {
            "api_key" => Some(Self::ApiKey),
            "official_login" => Some(Self::OfficialLogin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOptionsInput {
    /// 旧前端不传时默认 `api_key`；创建后不可更改。
    #[serde(default)]
    pub auth_kind: ProviderAuthKind,
    pub credential_env_key: Option<ClaudeCredentialEnvKey>,
    pub extra_env: BTreeMap<String, String>,
    pub wire_api: Option<String>,
    pub zcode_kind: Option<String>,
    /// OpenCode provider SDK package (for example
    /// `@ai-sdk/openai-compatible`). Stored as metadata; the app never
    /// installs or executes the package.
    pub opencode_npm: Option<String>,
    pub opencode_api: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileInput {
    pub tool: Tool,
    pub name: String,
    pub api_base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub options: ProviderOptionsInput,
    pub activate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case", tag = "action", content = "value")]
pub enum SecretUpdate {
    Keep,
    Clear,
    Replace(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderProfileInput {
    pub id: String,
    pub name: String,
    pub api_base_url: String,
    pub api_key: SecretUpdate,
    pub default_model: String,
    pub options: ProviderOptionsInput,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CopyProviderProfileInput {
    pub source_id: String,
    pub target_tool: Tool,
    pub target_name: String,
    pub activate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionedProfileInput {
    pub id: String,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileDto {
    pub id: String,
    pub tool: Tool,
    pub name: String,
    pub api_base_url: String,
    pub api_key_configured: bool,
    pub default_model: String,
    pub options: ProviderOptionsDto,
    /// 仅 Pi 有值：API 格式与模型列表的只读摘要。
    pub pi: Option<PiProviderSummaryDto>,
    pub is_active: bool,
    pub row_version: u32,
    /// 仅中央 mutation 返回；列表/详情响应省略该字段。
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOptionsDto {
    pub auth_kind: ProviderAuthKind,
    pub credential_env_key: Option<ClaudeCredentialEnvKey>,
    pub extra_env: BTreeMap<String, String>,
    pub provider_id: Option<String>,
    pub wire_api: Option<String>,
    pub zcode_kind: Option<String>,
    pub opencode_npm: Option<String>,
    pub opencode_api: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromptProfileInput {
    pub name: String,
    pub body: String,
}

/// 全局启用/停用一份提示词档案到指定工具；每工具至多一份生效，
/// 启用新档案会自动替换该工具的原生效档案。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalPromptAssignmentInput {
    pub tool: Tool,
    pub prompt_profile_id: String,
    pub assigned: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePromptProfileInput {
    pub id: String,
    pub name: String,
    pub body: String,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromptProfileDto {
    pub id: String,
    pub name: String,
    pub body: String,
    pub global_tools: Vec<Tool>,
    pub imported_from_path: Option<String>,
    pub row_version: u32,
    /// 仅中央 mutation 返回；列表响应省略该字段。
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

/// 候选的可导入性：只有 `importable` 可以在界面上勾选。
///
/// `already_managed` 用于一个目标文件承载多个 Provider 条目的工具（Pi）：
/// 该原生条目已经有一份中央档案，重复导入必须被拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderImportCandidateStatus {
    Importable,
    AlreadyManaged,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderImportCandidateDto {
    /// 服务端签发的不透明候选 id；前端只回传它，不参与还原原生 key。
    pub candidate_id: String,
    pub provider_id: String,
    pub suggested_name: String,
    pub status: ProviderImportCandidateStatus,
    /// 不可导入时的稳定原因码；不回显任何凭据。
    pub reason: Option<String>,
    pub auth_kind: ProviderAuthKind,
    pub default_provider: bool,
    pub api_base_url: String,
    pub api_key_configured: bool,
    pub default_model: String,
    /// 只读展示：Provider 的 API 格式（`providers.<id>.api` 等）。
    pub api_format: Option<String>,
    pub model_count: u32,
    pub redacted_projection: Value,
    /// 原生 env 中疑似凭据或格式不受支持、因此未纳入管理的键名（不含值）。
    pub skipped_env_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderImportPreviewDto {
    /// 至少一个可导入候选时由服务端签发的预览 id；否则为空。
    pub preview_id: Option<String>,
    pub tool: Tool,
    pub target_path: String,
    pub candidates: Vec<ProviderImportCandidateDto>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmProviderImportItem {
    pub candidate_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmProviderImportInput {
    pub preview_id: String,
    pub items: Vec<ConfirmProviderImportItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptProviderNativeInput {
    /// 统一 ExternalChangePlan 采纳动作的 run id；仅用于让仓储层排除当前
    /// 已 claim 的写者，旧的内部调用可留空并继续检查其它 active writer。
    #[specta(skip)]
    #[serde(skip)]
    pub preview_id: Option<String>,
    pub tool: Tool,
    /// 统一 ExternalChangePlan 绑定的 managed target 身份与行版本。
    #[specta(skip)]
    #[serde(skip)]
    pub target_id: String,
    #[specta(skip)]
    #[serde(skip)]
    pub target_row_version: u32,
    pub target_path: String,
    /// 用户所看预览绑定的行版本；缺少漂移渠道的条目或已过期都会拒绝。
    pub row_versions: Vec<crate::sync::DatabaseRowVersion>,
    /// 外部变化计划观察到的完整 hash；再次读取目标后必须一致，避免把
    /// 计划生成后发生的第三方修改误当成用户已授权的内容。
    pub observed_full_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptProviderNativeResultDto {
    pub tool: Tool,
    /// 被采纳的原生渠道显示名；未漂移的渠道不会出现在这里。
    pub adopted: Vec<String>,
    /// 原生采纳会更新中央档案；范围由工具目标唯一确定。
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

/// 被动扫描后按原生 Prompt 正文更新中央档案所需的 Preview 证据。
///
/// `target_id`/`target_row_version` 与 `row_versions` 中的 Prompt 档案版本都
/// 来自同一份 `ExternalChangePlan`。调用方不能只传目标路径或重新读取当前
/// row_version，否则旧页面可能把另一个窗口刚编辑的中央档案静默覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptPromptNativeInput {
    pub tool: Tool,
    pub target_id: String,
    pub target_row_version: u32,
    pub target_path: String,
    /// 用户所看预览绑定的唯一 active Prompt 档案行版本。
    pub row_versions: Vec<crate::sync::DatabaseRowVersion>,
    /// 外部变化计划观察到的完整目标 hash；消费前必须重新扫描并一致。
    pub observed_full_hash: Option<String>,
    /// 受管正文 projection 的 hash；与完整 hash 一起防止只做 baseline-only
    /// readopt，也让消费动作绑定到同一份 observed document。
    pub observed_managed_hash: Option<String>,
}

/// Prompt 原生采纳结果。正文不回传到 RPC；前端只需要稳定身份和受影响范围
/// 去刷新中央档案及同步状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptPromptNativeResultDto {
    pub tool: Tool,
    pub adopted: Vec<String>,
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderImportResultDto {
    pub tool: Tool,
    pub imported_count: u32,
    /// 导入并启用渠道时返回精确的全局 Provider 目标；未形成有效投影时为空。
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

/// Pi 渠道的只读摘要：让用户看到导入确实保留了 API 格式与模型列表，
/// 只用非敏感字段（绝不包含 `headers`/`apiKey`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PiProviderModelDto {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PiProviderSummaryDto {
    pub api_format: Option<String>,
    pub models: Vec<PiProviderModelDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromptImportPreviewDto {
    pub preview_id: String,
    pub tool: Tool,
    pub target_path: String,
    pub suggested_name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmImportInput {
    pub preview_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProfilePreviewInput {
    pub preview_id: String,
    pub tool: Tool,
    pub artifact_kind: ArtifactKind,
}

/// 以当前原生 Provider 内容重新接管目标基线；该操作只更新应用数据库，
/// 不会直接写入原生配置文件。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadoptProviderTargetInput {
    pub tool: Tool,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadoptProviderTargetResultDto {
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolProfileStatusDto {
    pub tool: Tool,
    pub availability: ToolAvailabilityState,
    pub installation_version: Option<String>,
    /// 安装探针的稳定诊断码；解释探测为何是当前状态（如 PATH 条目被跳过）。
    pub installation_probe_diagnostic: Option<String>,
    pub provider_target_path: Option<String>,
    pub prompt_target_path: Option<String>,
    pub provider_capability: TargetCapability,
    pub prompt_capability: TargetCapability,
    pub prompt_override: PromptOverrideState,
    pub provider_policy: PolicyState,
    pub new_session_notice: String,
    pub bearer_token_warning: Option<String>,
}

/// Provider/Prompt 全局目标的只读现场状态。该 DTO 只描述扫描结果，不能作为
/// Apply 证据；需要执行动作时必须重新请求 `ExternalChangePlan`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileTargetStatusDto {
    pub artifact_kind: crate::domain::ArtifactKind,
    pub tool: Tool,
    pub target_path: Option<String>,
    pub status: crate::domain::SyncStatus,
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProfileResultDto {
    pub id: String,
    pub deleted: bool,
    /// 删除前捕获的范围，用于清理已生效的原生投影。
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredProviderConfig {
    /// 旧记录没有该字段：Codex 的 `openai` provider 视为官方登录，其余视为 API Key。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_kind: Option<ProviderAuthKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_env_key: Option<ClaudeCredentialEnvKey>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_api: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zcode_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opencode_npm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opencode_api: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_provider_fields: BTreeMap<String, Value>,
}

impl StoredProviderConfig {
    pub fn from_input(
        tool: Tool,
        provider_id: &str,
        options: ProviderOptionsInput,
        extra_provider_fields: BTreeMap<String, Value>,
    ) -> Result<Self, AppError> {
        let auth_kind = options.auth_kind;
        match tool {
            Tool::Claude => {
                if options.wire_api.is_some() {
                    return Err(AppError::invalid_input(
                        "wireApi",
                        "Claude Provider 不支持 Codex wire_api",
                    ));
                }
                if !extra_provider_fields.is_empty() {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "Claude Provider 不能保留 Codex 扩展字段",
                    ));
                }
                validate_extra_env(&options.extra_env)?;
                // 官方登录不写任何凭据键，因此不保留 credential_env_key。
                let credential_env_key = match auth_kind {
                    ProviderAuthKind::ApiKey => Some(
                        options
                            .credential_env_key
                            .unwrap_or(ClaudeCredentialEnvKey::ApiKey),
                    ),
                    ProviderAuthKind::OfficialLogin => None,
                };
                Ok(Self {
                    auth_kind: Some(auth_kind),
                    credential_env_key,
                    extra_env: options.extra_env,
                    provider_id: None,
                    wire_api: None,
                    zcode_kind: None,
                    opencode_npm: None,
                    opencode_api: None,
                    extra_provider_fields: BTreeMap::new(),
                })
            }
            Tool::Codex => {
                if options.credential_env_key.is_some() || !options.extra_env.is_empty() {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "Codex Provider 不支持 Claude env 选项",
                    ));
                }
                if options.zcode_kind.is_some() {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "Codex Provider 不支持 ZCode API 格式",
                    ));
                }
                if auth_kind == ProviderAuthKind::OfficialLogin {
                    if provider_id != CODEX_OPENAI_PROVIDER_ID {
                        return Err(AppError::invalid_input(
                            "providerId",
                            "Codex 官方账号登录渠道只能使用内置 openai provider",
                        ));
                    }
                    if options.wire_api.is_some() || !extra_provider_fields.is_empty() {
                        return Err(AppError::invalid_input(
                            "providerOptions",
                            "Codex 官方账号登录渠道不需要 wire_api 或扩展字段",
                        ));
                    }
                }
                validate_wire_api(options.wire_api.as_deref())?;
                validate_codex_extra_provider_fields(&extra_provider_fields)?;
                Ok(Self {
                    auth_kind: Some(auth_kind),
                    credential_env_key: None,
                    extra_env: BTreeMap::new(),
                    provider_id: Some(provider_id.to_owned()),
                    wire_api: options.wire_api,
                    zcode_kind: None,
                    opencode_npm: None,
                    opencode_api: None,
                    extra_provider_fields,
                })
            }
            Tool::Zcode => {
                reject_official_login(auth_kind)?;
                if options.credential_env_key.is_some() || !options.extra_env.is_empty() {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "ZCode Provider 不支持 Claude env 选项",
                    ));
                }
                if options.wire_api.is_some() {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "ZCode Provider 不支持 Codex wire_api",
                    ));
                }
                if !extra_provider_fields.is_empty() {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "ZCode Provider 不能保留 Codex 扩展字段",
                    ));
                }
                let kind = options.zcode_kind.unwrap_or_else(|| "anthropic".to_owned());
                validate_zcode_provider_kind(&kind)?;
                Ok(Self {
                    auth_kind: Some(auth_kind),
                    credential_env_key: None,
                    extra_env: BTreeMap::new(),
                    provider_id: Some(provider_id.to_owned()),
                    wire_api: None,
                    zcode_kind: Some(kind),
                    opencode_npm: None,
                    opencode_api: None,
                    extra_provider_fields: BTreeMap::new(),
                })
            }
            Tool::Cursor => Err(AppError::invalid_input(
                "tool",
                "CURSOR_PROVIDER_UNSUPPORTED",
            )),
            // Pi Provider：`models.json` 的 `providers.<id>` 条目。除公共字段外
            // 只保留 `extra_provider_fields`（headers、modelOverrides、name 等
            // 未知/未受管字段原样保留）；Pi 没有其他工具的专属选项。
            Tool::Pi => {
                reject_official_login(auth_kind)?;
                if options.credential_env_key.is_some()
                    || !options.extra_env.is_empty()
                    || options.wire_api.is_some()
                    || options.zcode_kind.is_some()
                    || options.opencode_npm.is_some()
                    || options.opencode_api.is_some()
                {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "Pi Provider 不支持其他工具的专属选项",
                    ));
                }
                Ok(Self {
                    auth_kind: Some(auth_kind),
                    credential_env_key: None,
                    extra_env: BTreeMap::new(),
                    provider_id: Some(provider_id.to_owned()),
                    wire_api: None,
                    zcode_kind: None,
                    opencode_npm: None,
                    opencode_api: None,
                    extra_provider_fields,
                })
            }
            Tool::Opencode => {
                reject_official_login(auth_kind)?;
                if options.credential_env_key.is_some()
                    || !options.extra_env.is_empty()
                    || options.wire_api.is_some()
                    || options.zcode_kind.is_some()
                {
                    return Err(AppError::invalid_input(
                        "providerOptions",
                        "OpenCode Provider 不支持其他工具的专属选项",
                    ));
                }
                let npm = options.opencode_npm.ok_or_else(|| {
                    AppError::invalid_input("providerOptions", "OpenCode Provider 缺少 npm SDK")
                })?;
                validate_opencode_sdk(&npm)?;
                let api = options.opencode_api.unwrap_or_else(|| "openai".to_owned());
                validate_opencode_api(&api)?;
                Ok(Self {
                    auth_kind: Some(auth_kind),
                    credential_env_key: None,
                    extra_env: BTreeMap::new(),
                    provider_id: Some(provider_id.to_owned()),
                    wire_api: None,
                    zcode_kind: None,
                    opencode_npm: Some(npm),
                    opencode_api: Some(api),
                    extra_provider_fields,
                })
            }
        }
    }

    /// 旧记录缺少 `authKind` 时按既有语义推导：Codex 内置 `openai` 就是官方登录。
    pub fn effective_auth_kind(&self, tool: Tool) -> ProviderAuthKind {
        self.auth_kind.unwrap_or_else(|| {
            if tool == Tool::Codex && self.provider_id.as_deref() == Some(CODEX_OPENAI_PROVIDER_ID)
            {
                ProviderAuthKind::OfficialLogin
            } else {
                ProviderAuthKind::ApiKey
            }
        })
    }

    pub fn options_dto(&self, tool: Tool) -> ProviderOptionsDto {
        ProviderOptionsDto {
            auth_kind: self.effective_auth_kind(tool),
            credential_env_key: self.credential_env_key,
            extra_env: self.extra_env.clone(),
            provider_id: self.provider_id.clone(),
            wire_api: self.wire_api.clone(),
            zcode_kind: self.zcode_kind.clone(),
            opencode_npm: self.opencode_npm.clone(),
            opencode_api: self.opencode_api.clone(),
        }
    }
}

pub(crate) const CODEX_OPENAI_PROVIDER_ID: &str = "openai";

fn reject_official_login(auth_kind: ProviderAuthKind) -> Result<(), AppError> {
    if auth_kind == ProviderAuthKind::OfficialLogin {
        return Err(AppError::invalid_input(
            "providerOptions",
            "该工具不支持官方账号登录渠道",
        ));
    }
    Ok(())
}

/// ZCode provider entry 的 kind 来自应用自身的 API 格式枚举
/// （`~/.zcode/v2/config.json` 实测取值 anthropic；应用端点建议包含 openai/gemini）。
fn validate_zcode_provider_kind(kind: &str) -> Result<(), AppError> {
    if matches!(kind, "anthropic" | "openai" | "gemini") {
        Ok(())
    } else {
        Err(AppError::invalid_input(
            "providerOptions",
            "ZCode Provider API 格式只支持 anthropic、openai 或 gemini",
        ))
    }
}

fn validate_opencode_sdk(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 200
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == b'\0')
    {
        return Err(AppError::invalid_input(
            "providerOptions",
            "OpenCode npm SDK 标识无效",
        ));
    }
    Ok(())
}

fn validate_opencode_api(value: &str) -> Result<(), AppError> {
    if value.is_empty() || value.len() > 64 || value.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(AppError::invalid_input(
            "providerOptions",
            "OpenCode SDK 协议标识无效",
        ));
    }
    Ok(())
}

fn validate_codex_extra_provider_fields(fields: &BTreeMap<String, Value>) -> Result<(), AppError> {
    if fields.contains_key("auth")
        || fields.contains_key("env_key")
        || fields
            .get("requires_openai_auth")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(AppError::invalid_input(
            "providerOptions",
            "直接 bearer token 不能与 auth、env_key 或 OpenAI 内置认证组合",
        ));
    }
    Ok(())
}

/// Provider 公共字段的单一校验入口；按认证方式区分接入地址与密钥是否必填。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProviderFieldsInput<'a> {
    pub tool: Tool,
    pub auth_kind: ProviderAuthKind,
    pub name: &'a str,
    pub api_base_url: &'a str,
    /// `None` 与空串等价，表示没有本地密钥。
    pub api_key: Option<&'a str>,
    pub default_model: &'a str,
}

pub(crate) fn validate_provider_fields(input: &ProviderFieldsInput<'_>) -> Result<(), AppError> {
    ArtifactName::parse(input.name.to_owned())?;
    let api_key = input.api_key.filter(|value| !value.is_empty());
    match input.auth_kind {
        ProviderAuthKind::ApiKey => {
            // Claude/Codex 的接入地址可空：留空表示使用工具的官方端点配合自己的 API Key；
            // ZCode/OpenCode 的 provider 条目没有 baseURL 就不完整，仍然必填。
            let base_url_required = matches!(input.tool, Tool::Zcode | Tool::Opencode);
            if base_url_required || !input.api_base_url.trim().is_empty() {
                validate_api_base_url(
                    input.api_base_url,
                    "API 地址必须是无凭据的绝对 HTTP(S) URL",
                )?;
            }
            match api_key {
                Some(value) => validate_non_empty_text(value, "apiKey", "API Key 不能为空")?,
                None => return Err(AppError::invalid_input("apiKey", "API Key 不能为空")),
            }
        }
        ProviderAuthKind::OfficialLogin => {
            if !input.api_base_url.trim().is_empty() {
                return Err(AppError::invalid_input(
                    "apiBaseUrl",
                    "官方账号登录渠道不需要 API 地址",
                ));
            }
            if api_key.is_some() {
                return Err(AppError::invalid_input(
                    "apiKey",
                    "官方账号登录渠道使用工具自身的登录凭据，不能保存本地 API Key",
                ));
            }
        }
    }
    // OpenCode 的 `model` 必须引用 provider/model，ZCode 条目也以模型为键；
    // Claude/Codex 缺省时由工具使用自己的默认模型。
    if !input.default_model.is_empty() || matches!(input.tool, Tool::Zcode | Tool::Opencode) {
        validate_non_empty_text(input.default_model, "defaultModel", "默认模型不能为空")?;
    }
    Ok(())
}

/// 空串在数据库中存为 NULL，DTO 再还原为空串。
pub(crate) fn optional_text(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

pub(crate) fn validate_prompt_fields(name: &str, body: &str) -> Result<(), AppError> {
    ArtifactName::parse(name.to_owned())?;
    if body.trim().is_empty() || body.contains('\0') {
        return Err(AppError::invalid_input(
            "body",
            "提示词正文不能为空且不能包含 NUL",
        ));
    }
    Ok(())
}

fn validate_api_base_url(value: &str, reason: &'static str) -> Result<(), AppError> {
    let parsed = url::Url::parse(value)
        .map_err(|error| AppError::invalid_input("apiBaseUrl", reason).with_source(error))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::invalid_input("apiBaseUrl", reason));
    }
    Ok(())
}

fn validate_non_empty_text(
    value: &str,
    field: &'static str,
    reason: &'static str,
) -> Result<(), AppError> {
    if value.trim().is_empty() || value.contains('\0') {
        Err(AppError::invalid_input(field, reason))
    } else {
        Ok(())
    }
}

fn validate_extra_env(extra_env: &BTreeMap<String, String>) -> Result<(), AppError> {
    for (key, value) in extra_env {
        if CLAUDE_RESERVED_ENV_KEYS.contains(&key.as_str()) {
            return Err(AppError::invalid_input(
                "extraEnv",
                "额外 Claude env 不能包含由专用字段承载的保留键",
            ));
        }
        if value
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n'))
        {
            return Err(AppError::invalid_input(
                "extraEnv",
                "额外 Claude env 值不能包含 NUL 或换行",
            ));
        }
        if !env_entry_is_manageable(key, value) {
            if key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            {
                return Err(AppError::invalid_input(
                    "extraEnv",
                    "额外 Claude env key 必须是非保留的大写字母、数字或下划线",
                ));
            }
            return Err(AppError::invalid_input(
                "extraEnv",
                "可识别的认证、token、密码或 cookie 必须使用专用密钥字段，不能作为普通扩展 env 返回",
            ));
        }
    }
    Ok(())
}

/// Codex 官方支持的两种传输协议（`config.md` 的 `wire_api`）。
fn validate_wire_api(wire_api: Option<&str>) -> Result<(), AppError> {
    match wire_api {
        None | Some("responses") | Some("chat") => Ok(()),
        Some(_) => Err(AppError::invalid_input(
            "wireApi",
            "Codex wire_api 仅支持 responses 或 chat",
        )),
    }
}
