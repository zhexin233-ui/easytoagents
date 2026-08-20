//! Provider 与提示词的 RPC DTO 及工具级字段合同。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::{
    adapters::{PolicyState, PromptOverrideState},
    domain::{ArtifactKind, ArtifactName, Tool},
    error::AppError,
    security::contains_detectable_secret,
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOptionsInput {
    pub credential_env_key: Option<ClaudeCredentialEnvKey>,
    pub extra_env: BTreeMap<String, String>,
    pub wire_api: Option<String>,
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
    pub is_active: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOptionsDto {
    pub credential_env_key: Option<ClaudeCredentialEnvKey>,
    pub extra_env: BTreeMap<String, String>,
    pub provider_id: Option<String>,
    pub wire_api: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PromptProfileInput {
    pub tool: Tool,
    pub name: String,
    pub body: String,
    pub activate: bool,
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
    pub tool: Tool,
    pub name: String,
    pub body: String,
    pub is_active: bool,
    pub imported_from_path: Option<String>,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderImportPreviewDto {
    pub preview_id: String,
    pub tool: Tool,
    pub target_path: String,
    pub suggested_name: String,
    pub api_base_url: String,
    pub api_key_configured: bool,
    pub default_model: String,
    pub redacted_projection: Value,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolProfileStatusDto {
    pub tool: Tool,
    pub provider_target_path: String,
    pub prompt_target_path: String,
    pub prompt_override: PromptOverrideState,
    pub provider_policy: PolicyState,
    pub new_session_notice: String,
    pub bearer_token_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProfileResultDto {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_env_key: Option<ClaudeCredentialEnvKey>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_api: Option<String>,
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
                Ok(Self {
                    credential_env_key: Some(
                        options
                            .credential_env_key
                            .unwrap_or(ClaudeCredentialEnvKey::ApiKey),
                    ),
                    extra_env: options.extra_env,
                    provider_id: None,
                    wire_api: None,
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
                validate_wire_api(options.wire_api.as_deref())?;
                validate_codex_extra_provider_fields(&extra_provider_fields)?;
                Ok(Self {
                    credential_env_key: None,
                    extra_env: BTreeMap::new(),
                    provider_id: Some(provider_id.to_owned()),
                    wire_api: options.wire_api,
                    extra_provider_fields,
                })
            }
        }
    }

    pub fn options_dto(&self) -> ProviderOptionsDto {
        ProviderOptionsDto {
            credential_env_key: self.credential_env_key,
            extra_env: self.extra_env.clone(),
            provider_id: self.provider_id.clone(),
            wire_api: self.wire_api.clone(),
        }
    }
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

pub(crate) fn validate_provider_fields(
    name: &str,
    api_base_url: &str,
    api_key: &str,
    default_model: &str,
) -> Result<(), AppError> {
    ArtifactName::parse(name.to_owned())?;
    validate_api_base_url(api_base_url, "API 地址必须是无凭据的绝对 HTTP(S) URL")?;
    validate_non_empty_text(api_key, "apiKey", "API Key 不能为空")?;
    validate_non_empty_text(default_model, "defaultModel", "默认模型不能为空")
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
    let parsed =
        url::Url::parse(value).map_err(|_| AppError::invalid_input("apiBaseUrl", reason))?;
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
    const RESERVED: &[&str] = &[
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
    ];
    for (key, value) in extra_env {
        if key.is_empty()
            || key.len() > 128
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            || RESERVED.contains(&key.as_str())
        {
            return Err(AppError::invalid_input(
                "extraEnv",
                "额外 Claude env key 必须是非保留的大写字母、数字或下划线",
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
        if contains_detectable_secret(key, value) {
            return Err(AppError::invalid_input(
                "extraEnv",
                "可识别的认证、token、密码或 cookie 必须使用专用密钥字段，不能作为普通扩展 env 返回",
            ));
        }
    }
    Ok(())
}

fn validate_wire_api(wire_api: Option<&str>) -> Result<(), AppError> {
    match wire_api {
        None | Some("responses") => Ok(()),
        Some(_) => Err(AppError::invalid_input(
            "wireApi",
            "Codex wire_api 仅支持 responses",
        )),
    }
}
