//! MCP 的 RPC DTO、敏感字段更新合同与跨工具结构化校验。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::{
    domain::{ArtifactName, McpTransport, SyncStatus, Tool, TrustStatus},
    error::AppError,
    security::contains_detectable_secret,
};

const MAX_EXTRA_BYTES: usize = 64 * 1024;
const MAX_EXTRA_DEPTH: usize = 8;
const MAX_MAP_ENTRIES: usize = 256;
const MAX_MAP_VALUE_BYTES: usize = 64 * 1024;
const RESERVED_EXTRA_KEYS: &[&str] = &[
    "type",
    "transport",
    "command",
    "args",
    "url",
    "headers",
    "http_headers",
    "env_http_headers",
    "env",
    "enabled",
    "disabled",
];

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInput {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
    pub extra: Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case", tag = "action", content = "value")]
pub enum SensitiveMapUpdate {
    Keep,
    Clear,
    Replace(BTreeMap<String, String>),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case", tag = "action", content = "value")]
pub enum SensitiveJsonUpdate {
    Keep,
    Clear,
    Replace(Value),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMcpServerInput {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: SensitiveMapUpdate,
    pub env: SensitiveMapUpdate,
    pub extra: SensitiveJsonUpdate,
    pub enabled: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionedMcpInput {
    pub id: String,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub header_names: Vec<String>,
    pub env_names: Vec<String>,
    pub redacted_extra: Value,
    pub enabled: bool,
    pub global_tools: Vec<Tool>,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMcpResultDto {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalMcpAssignmentInput {
    pub tool: Tool,
    pub mcp_id: String,
    pub assigned: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetProjectMcpAssignmentInput {
    pub project_id: String,
    pub tool: Tool,
    pub mcp_id: String,
    pub assigned: bool,
    pub mcp_row_version: u32,
    pub project_row_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum McpProjectSelectionState {
    Inherited,
    Selected,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpProjectOptionDto {
    pub mcp_id: String,
    pub name: String,
    pub enabled: bool,
    pub state: McpProjectSelectionState,
    pub selectable: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpProjectDto {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub codex_trust_status: TrustStatus,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpProjectOptionsInput {
    pub project_id: String,
    pub tool: Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMcpSyncInput {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub exclude_from_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyMcpPreviewInput {
    pub preview_id: String,
    pub tool: Tool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpTargetStatusDto {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub target_path: Option<String>,
    pub status: SyncStatus,
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum McpImportCandidateStatus {
    Importable,
    AlreadyManaged,
    NameConflict,
    Disabled,
    Unsupported,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum McpImportAction {
    Create,
    Reuse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpImportCandidateDto {
    pub candidate_id: String,
    pub name: String,
    pub transport: Option<McpTransport>,
    pub status: McpImportCandidateStatus,
    pub action: Option<McpImportAction>,
    pub reason: Option<String>,
    pub redacted_projection: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpImportPreviewDto {
    pub preview_id: Option<String>,
    pub tool: Tool,
    pub target_path: String,
    pub candidates: Vec<McpImportCandidateDto>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmMcpImportInput {
    pub preview_id: String,
    pub candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpImportResultDto {
    pub tool: Tool,
    pub created_count: u32,
    pub reused_count: u32,
    pub assigned_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValidatedMcpConfiguration {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
    pub extra: Value,
    pub enabled: bool,
}

impl ValidatedMcpConfiguration {
    pub fn from_create(input: &McpServerInput) -> Result<Self, AppError> {
        validate_configuration(
            &input.name,
            input.transport,
            input.command.clone(),
            input.args.clone(),
            input.url.clone(),
            input.headers.clone(),
            input.env.clone(),
            input.extra.clone(),
            input.enabled,
        )
    }

    pub fn from_update(
        input: &UpdateMcpServerInput,
        current_headers: &BTreeMap<String, String>,
        current_env: &BTreeMap<String, String>,
        current_extra: &Value,
    ) -> Result<Self, AppError> {
        let headers = match &input.headers {
            SensitiveMapUpdate::Keep => current_headers.clone(),
            SensitiveMapUpdate::Clear => BTreeMap::new(),
            SensitiveMapUpdate::Replace(value) => value.clone(),
        };
        let env = match &input.env {
            SensitiveMapUpdate::Keep => current_env.clone(),
            SensitiveMapUpdate::Clear => BTreeMap::new(),
            SensitiveMapUpdate::Replace(value) => value.clone(),
        };
        let extra = match &input.extra {
            SensitiveJsonUpdate::Keep => current_extra.clone(),
            SensitiveJsonUpdate::Clear => Value::Object(Default::default()),
            SensitiveJsonUpdate::Replace(value) => value.clone(),
        };
        validate_configuration(
            &input.name,
            input.transport,
            input.command.clone(),
            input.args.clone(),
            input.url.clone(),
            headers,
            env,
            extra,
            input.enabled,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_configuration(
    name: &str,
    transport: McpTransport,
    command: Option<String>,
    args: Vec<String>,
    url: Option<String>,
    headers: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    extra: Value,
    enabled: bool,
) -> Result<ValidatedMcpConfiguration, AppError> {
    ArtifactName::parse(name.to_owned())?;
    let command = normalize_optional(command, "command")?;
    let url = normalize_optional(url, "url")?;
    validate_args(&args)?;
    validate_headers(&headers)?;
    validate_env(&env)?;
    validate_extra(&extra)?;

    match transport {
        McpTransport::Stdio => {
            if command.is_none() || url.is_some() || !headers.is_empty() {
                return Err(AppError::invalid_input(
                    "transport",
                    "stdio 必须填写 command，且不能填写 url 或 headers",
                ));
            }
        }
        McpTransport::StreamableHttp => {
            if url.is_none() || command.is_some() || !args.is_empty() || !env.is_empty() {
                return Err(AppError::invalid_input(
                    "transport",
                    "streamable_http 必须填写 url，且不能填写 command、args 或 env",
                ));
            }
            validate_http_url(url.as_deref().expect("已验证 HTTP URL 存在"))?;
        }
    }

    Ok(ValidatedMcpConfiguration {
        name: name.to_owned(),
        transport,
        command,
        args,
        url,
        headers,
        env,
        extra,
        enabled,
    })
}

fn normalize_optional(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<String>, AppError> {
    match value {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if value.trim().is_empty() || value.contains('\0') => Err(
            AppError::invalid_input(field, "字段不能为空白且不能包含 NUL"),
        ),
        Some(value) => Ok(Some(value)),
    }
}

fn validate_args(args: &[String]) -> Result<(), AppError> {
    if args.len() > 256
        || args
            .iter()
            .any(|value| value.contains('\0') || value.len() > 16 * 1024)
    {
        return Err(AppError::invalid_input(
            "args",
            "args 最多 256 项，单项不能包含 NUL 或超过 16 KiB",
        ));
    }
    if args.iter().any(|value| {
        contains_detectable_secret("argument", value) || looks_like_secret_argument_name(value)
    }) {
        return Err(AppError::invalid_input(
            "args",
            "args 不能包含可识别的令牌或密钥；请改用 env 或 HTTP headers",
        ));
    }
    Ok(())
}

fn validate_headers(headers: &BTreeMap<String, String>) -> Result<(), AppError> {
    if headers.len() > MAX_MAP_ENTRIES {
        return Err(AppError::invalid_input(
            "headers",
            "headers 最多包含 256 项",
        ));
    }
    for (key, value) in headers {
        if key.is_empty()
            || key.len() > 128
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || value.len() > MAX_MAP_VALUE_BYTES
            || value
                .chars()
                .any(|character| character.is_control() && character != '\t')
        {
            return Err(AppError::invalid_input(
                "headers",
                "header 名称只能包含字母、数字、连字符或下划线，值不能包含非法控制字符或超过 64 KiB",
            ));
        }
    }
    Ok(())
}

fn validate_env(env: &BTreeMap<String, String>) -> Result<(), AppError> {
    if env.len() > MAX_MAP_ENTRIES {
        return Err(AppError::invalid_input("env", "env 最多包含 256 项"));
    }
    for (key, value) in env {
        let mut bytes = key.bytes();
        let valid_start = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
        if !valid_start
            || key.len() > 128
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || value.contains('\0')
            || value.len() > MAX_MAP_VALUE_BYTES
        {
            return Err(AppError::invalid_input(
                "env",
                "env key 必须是合法环境变量名，值不能包含 NUL 或超过 64 KiB",
            ));
        }
    }
    Ok(())
}

fn validate_http_url(value: &str) -> Result<(), AppError> {
    let parsed = url::Url::parse(value)
        .map_err(|_| AppError::invalid_input("url", "MCP URL 必须是无凭据的绝对 HTTP(S) URL"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::invalid_input(
            "url",
            "MCP URL 必须是无凭据、无 fragment 的绝对 HTTP(S) URL",
        ));
    }
    if parsed
        .query_pairs()
        .any(|(key, value)| contains_detectable_secret(&key, &value))
    {
        return Err(AppError::invalid_input(
            "url",
            "MCP URL query 不能包含可识别的令牌或密钥",
        ));
    }
    Ok(())
}

fn looks_like_secret_argument_name(value: &str) -> bool {
    let normalized = value
        .trim()
        .trim_start_matches('-')
        .split(['=', ':'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    value.trim_start().starts_with('-')
        && [
            "authorization",
            "apikey",
            "token",
            "secret",
            "password",
            "cookie",
            "credential",
            "bearer",
        ]
        .iter()
        .any(|marker| normalized.contains(marker))
}

fn validate_extra(extra: &Value) -> Result<(), AppError> {
    let object = extra
        .as_object()
        .ok_or_else(|| AppError::invalid_input("extra", "MCP 扩展字段必须是 JSON 对象"))?;
    if serde_json::to_vec(extra).map_or(true, |bytes| bytes.len() > MAX_EXTRA_BYTES) {
        return Err(AppError::invalid_input(
            "extra",
            "MCP 扩展字段不能超过 64 KiB",
        ));
    }
    for key in object.keys() {
        if key.is_empty()
            || key.chars().any(char::is_control)
            || RESERVED_EXTRA_KEYS
                .iter()
                .any(|reserved| key.eq_ignore_ascii_case(reserved))
        {
            return Err(AppError::invalid_input(
                "extra",
                "MCP 扩展字段名称无效或与结构化字段冲突",
            ));
        }
    }
    validate_portable_extra(extra, 0)
}

fn validate_portable_extra(value: &Value, depth: usize) -> Result<(), AppError> {
    if depth > MAX_EXTRA_DEPTH {
        return Err(AppError::invalid_input(
            "extra",
            "MCP 扩展字段嵌套不能超过 8 层",
        ));
    }
    match value {
        Value::Null => Err(AppError::invalid_input(
            "extra",
            "MCP 扩展字段不能包含 TOML 无法表示的 null",
        )),
        Value::Array(values) => {
            if values
                .iter()
                .any(|value| matches!(value, Value::Array(_) | Value::Object(_) | Value::Null))
            {
                return Err(AppError::invalid_input(
                    "extra",
                    "MCP 扩展数组只能包含跨工具可表示的标量",
                ));
            }
            for value in values {
                validate_portable_extra(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            for (key, value) in object {
                if key.is_empty() || key.chars().any(char::is_control) {
                    return Err(AppError::invalid_input(
                        "extra",
                        "MCP 扩展字段不能包含空名称或控制字符",
                    ));
                }
                validate_portable_extra(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Number(number)
            if number
                .as_u64()
                .is_some_and(|number| i64::try_from(number).is_err()) =>
        {
            Err(AppError::invalid_input(
                "extra",
                "MCP 扩展字段整数超出 TOML 可表示范围",
            ))
        }
        Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{McpServerInput, ValidatedMcpConfiguration};
    use crate::domain::McpTransport;

    fn input(transport: McpTransport) -> McpServerInput {
        McpServerInput {
            name: "fixture".to_owned(),
            transport,
            command: Some("npx".to_owned()),
            args: vec!["server".to_owned()],
            url: None,
            headers: BTreeMap::new(),
            env: BTreeMap::from([("TOKEN".to_owned(), "fixture-secret".to_owned())]),
            extra: json!({"startup_timeout_sec": 10}),
            enabled: true,
        }
    }

    #[test]
    fn transport_fields_are_mutually_exclusive_and_required() {
        assert!(ValidatedMcpConfiguration::from_create(&input(McpTransport::Stdio)).is_ok());
        let mut http = input(McpTransport::StreamableHttp);
        http.command = None;
        http.args.clear();
        http.env.clear();
        http.url = Some("https://mcp.example.test/rpc?tenant=fixture".to_owned());
        http.headers
            .insert("Authorization".to_owned(), "Bearer fixture".to_owned());
        assert!(ValidatedMcpConfiguration::from_create(&http).is_ok());
        http.command = Some("invalid".to_owned());
        assert!(ValidatedMcpConfiguration::from_create(&http).is_err());
    }

    #[test]
    fn url_maps_and_extra_fail_closed_on_unsafe_values() {
        let mut http = input(McpTransport::StreamableHttp);
        http.command = None;
        http.args.clear();
        http.env.clear();
        http.url = Some("https://user:pass@mcp.example.test/#secret".to_owned());
        assert!(ValidatedMcpConfiguration::from_create(&http).is_err());

        let mut stdio = input(McpTransport::Stdio);
        stdio.env = BTreeMap::from([("INVALID-KEY".to_owned(), "value".to_owned())]);
        assert!(ValidatedMcpConfiguration::from_create(&stdio).is_err());
        stdio.env.clear();
        stdio.extra = json!({"command": "shadow"});
        assert!(ValidatedMcpConfiguration::from_create(&stdio).is_err());
        stdio.extra = json!({"nested": null});
        assert!(ValidatedMcpConfiguration::from_create(&stdio).is_err());
        stdio.extra = json!({"integer": u64::MAX});
        assert!(ValidatedMcpConfiguration::from_create(&stdio).is_err());
    }

    #[test]
    fn ordinary_rpc_fields_reject_detectable_secrets_and_oversized_values() {
        let mut stdio = input(McpTransport::Stdio);
        stdio.args = vec!["--token".to_owned(), "never-return-this".to_owned()];
        let error = ValidatedMcpConfiguration::from_create(&stdio).unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("never-return-this"));
        stdio.args = vec!["--endpoint=https://example.test".to_owned()];
        assert!(ValidatedMcpConfiguration::from_create(&stdio).is_ok());

        let mut http = input(McpTransport::StreamableHttp);
        http.command = None;
        http.args.clear();
        http.env.clear();
        http.url = Some("https://mcp.example.test/rpc?token=never-return-this".to_owned());
        let error = ValidatedMcpConfiguration::from_create(&http).unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("never-return-this"));
        http.url = Some("https://mcp.example.test/rpc?tenant=fixture".to_owned());
        http.headers
            .insert("X-Fixture".to_owned(), "x".repeat(64 * 1024 + 1));
        assert!(ValidatedMcpConfiguration::from_create(&http).is_err());
    }
}
