//! Agents 的 RPC DTO 与中央定义校验。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::{
    domain::{AgentName, ManagedProjectSelectionState, SyncStatus, Tool},
    error::AppError,
};

const MAX_DESCRIPTION_BYTES: usize = 1000;
const MAX_PROMPT_BYTES: usize = 65536;
const MAX_SETTINGS_BYTES: usize = 16 * 1024;

/// Claude frontmatter 的首期工具特有设置白名单。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeAgentSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ClaudeAgentColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
}

/// Claude 官方允许的 agent 颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ClaudeAgentColor {
    Red,
    Blue,
    Green,
    Yellow,
    Purple,
    Orange,
    Pink,
    Cyan,
}

/// Codex agent 的首期工具特有设置白名单。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexAgentSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_reasoning_effort: Option<CodexReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<BTreeMap<String, bool>>,
}

/// Codex 自定义 agent 支持的 reasoning effort。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CodexReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

/// Agent 的工具特有覆盖层。缺省工具或 null 表示没有覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolSettingsDto {
    pub claude: Option<ClaudeAgentSettings>,
    pub codex: Option<CodexAgentSettings>,
}

/// 校验并规范化后的持久化覆盖层。值使用 DTO 的 camelCase 键，投影层再
/// 映射到对应工具的原生键名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedToolSettings {
    pub(crate) tool: Tool,
    pub(crate) value: Value,
}

impl ValidatedToolSettings {
    pub(crate) fn value(&self) -> &Value {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentInput {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentInput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub enabled: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionedAgentInput {
    pub id: String,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAgentResultDto {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub enabled: bool,
    pub global_assignments: Vec<Tool>,
    pub tool_settings: AgentToolSettingsDto,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetAgentToolSettingsInput {
    pub agent_id: String,
    pub tool: Tool,
    pub settings: Option<Value>,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalAgentAssignmentInput {
    pub tool: Tool,
    pub agent_id: String,
    pub assigned: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetProjectAgentAssignmentInput {
    pub project_id: String,
    pub tool: Tool,
    pub agent_id: String,
    pub assigned: bool,
    pub agent_row_version: u32,
    pub project_row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProjectDto {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub codex_trust_status: crate::domain::TrustStatus,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProjectOptionsInput {
    pub project_id: String,
    pub tool: Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentProjectOptionDto {
    pub agent_id: String,
    pub name: String,
    pub enabled: bool,
    pub state: ManagedProjectSelectionState,
    pub selectable: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAgentSyncInput {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub exclude_from_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyAgentPreviewInput {
    pub preview_id: String,
    pub tool: Tool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadoptAgentTargetInput {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadoptAgentTargetResultDto {
    pub target_path: String,
}

/// 全局目标状态卡按工具聚合的一条记录；`files` 可展开到单文件状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolTargetStatusDto {
    pub tool: Tool,
    pub directory_path: Option<String>,
    pub aggregate_status: SyncStatus,
    /// 工具能力/策略层诊断；文件级漂移诊断仍位于 `files`。
    pub diagnostic_code: Option<String>,
    pub files: Vec<AgentFileTargetStatusDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentFileTargetStatusDto {
    pub target_path: String,
    pub status: SyncStatus,
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverAgentImportInput {
    pub tool: Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportCandidateDto {
    pub candidate_id: String,
    pub source_path: String,
    /// 建议名称：frontmatter `name`（缺省时为文件名去扩展名）。
    pub name: String,
    /// 缺少必填字段时可能为空。
    pub description: String,
    pub prompt: String,
    /// 将被交集投影丢弃的工具特有 frontmatter / TOML 键名（知情丢弃）。
    pub dropped_fields: Vec<String>,
    /// 将按工具白名单保留并写入覆盖层的字段名。
    pub retained_fields: Vec<String>,
    /// 候选确认时写入该工具覆盖层的规范化 JSON。
    pub tool_settings: Option<Value>,
    pub importable: bool,
    /// `AGENT_FRONTMATTER_INVALID` / `AGENT_REQUIRED_FIELD_MISSING` /
    /// `AGENT_NAME_INVALID` / `AGENT_NAME_CONFLICT`。
    pub diagnostic_code: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportPreviewDto {
    pub tool: Tool,
    pub directory_path: String,
    pub candidates: Vec<AgentImportCandidateDto>,
    pub message: Option<String>,
}

/// 用户显式确认导入的条目；服务端只做中央校验，不引用持久化预览，
/// 不改写原生文件；首次分配时若交集字段仍一致则自动登记当前基线。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmAgentImportInput {
    pub tool: Tool,
    pub agents: Vec<ConfirmAgentImportAgent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmAgentImportAgent {
    pub definition: CreateAgentInput,
    pub tool_settings: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportResultDto {
    pub tool: Tool,
    pub created_count: u32,
}

/// 中央 Agent 定义校验：名称交集规则、描述与正文长度约束的统一入口。
pub(crate) fn validate_agent_definition(
    name: &str,
    description: &str,
    prompt: &str,
    enabled: bool,
) -> Result<crate::db::agents::ValidatedAgentDefinition, AppError> {
    let name = AgentName::parse(name.to_owned())?;
    let description = description.trim();
    if description.is_empty()
        || description.len() > MAX_DESCRIPTION_BYTES
        || description.contains('\0')
    {
        return Err(AppError::invalid_input(
            "description",
            "description 不能为空，不能包含 NUL，且不能超过 1000 字符",
        ));
    }
    let prompt = prompt.trim();
    if prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES || prompt.contains('\0') {
        return Err(AppError::invalid_input(
            "prompt",
            "prompt 不能为空，不能包含 NUL，且不能超过 65536 字符",
        ));
    }
    Ok(crate::db::agents::ValidatedAgentDefinition {
        name: name.as_str().to_owned(),
        description: description.to_owned(),
        prompt: prompt.to_owned(),
        enabled,
    })
}

const AGENT_TOOL_SETTINGS_UNSUPPORTED: &str = "AGENT_TOOL_SETTINGS_UNSUPPORTED";
const AGENT_FIELD_INVALID: &str = "AGENT_FIELD_INVALID";

fn invalid_tool_settings() -> AppError {
    AppError::invalid_input("settings", AGENT_FIELD_INVALID)
}

fn validate_model(model: Option<String>) -> Result<Option<String>, AppError> {
    let Some(model) = model else {
        return Ok(None);
    };
    let trimmed = model.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || trimmed.contains('\0')
        || trimmed.chars().any(|character| character.is_whitespace())
    {
        return Err(invalid_tool_settings());
    }
    Ok(Some(trimmed.to_owned()))
}

fn valid_tool_item(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && value.len() <= 128
        && chars.all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '_' | ':' | '*' | '(' | ')' | '.' | '/' | '-' | ' '
                )
        })
}

fn normalize_tools(tools: Option<Vec<String>>) -> Result<Option<Vec<String>>, AppError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    if tools.len() > 64 {
        return Err(invalid_tool_settings());
    }
    let mut normalized = Vec::with_capacity(tools.len());
    for tool in tools {
        let tool = tool.trim().to_owned();
        if !valid_tool_item(&tool) {
            return Err(invalid_tool_settings());
        }
        if !normalized.contains(&tool) {
            normalized.push(tool);
        }
    }
    if normalized.is_empty() {
        Ok(None)
    } else {
        Ok(Some(normalized))
    }
}

fn valid_feature_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    key.split('.').all(|part| {
        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        first.is_ascii_lowercase()
            && chars.all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
    })
}

fn normalize_features(
    features: Option<BTreeMap<String, bool>>,
) -> Result<Option<BTreeMap<String, bool>>, AppError> {
    let Some(features) = features else {
        return Ok(None);
    };
    if features.is_empty() {
        return Ok(None);
    }
    if features.len() > 64 || features.keys().any(|key| !valid_feature_key(key)) {
        return Err(invalid_tool_settings());
    }
    Ok(Some(features))
}

fn enforce_settings_size(value: &Value) -> Result<(), AppError> {
    let size = serde_json::to_vec(value)
        .map_err(|_| invalid_tool_settings())?
        .len();
    if size > MAX_SETTINGS_BYTES {
        return Err(invalid_tool_settings());
    }
    Ok(())
}

/// 校验并规范化工具特有设置。
///
/// `None` 表示空对象（没有任何覆盖），调用方应删除数据库行。未知工具和
/// 未知字段均 fail-closed，不把原生配置自由透传进中央库。
pub(crate) fn validate_agent_tool_settings(
    tool: Tool,
    value: &Value,
) -> Result<Option<ValidatedToolSettings>, AppError> {
    if !matches!(tool, Tool::Claude | Tool::Codex) {
        return Err(AppError::invalid_input(
            "tool",
            AGENT_TOOL_SETTINGS_UNSUPPORTED,
        ));
    }
    if !value.is_object() {
        return Err(invalid_tool_settings());
    }
    match tool {
        Tool::Claude => {
            let settings: ClaudeAgentSettings =
                serde_json::from_value(value.clone()).map_err(|_| invalid_tool_settings())?;
            let settings = ClaudeAgentSettings {
                model: validate_model(settings.model)?,
                color: settings.color,
                tools: normalize_tools(settings.tools)?,
            };
            let normalized = serde_json::to_value(settings).map_err(|_| invalid_tool_settings())?;
            enforce_settings_size(&normalized)?;
            if normalized
                .as_object()
                .map(|object| object.is_empty())
                .unwrap_or(false)
            {
                Ok(None)
            } else {
                Ok(Some(ValidatedToolSettings {
                    tool,
                    value: normalized,
                }))
            }
        }
        Tool::Codex => {
            let settings: CodexAgentSettings =
                serde_json::from_value(value.clone()).map_err(|_| invalid_tool_settings())?;
            let settings = CodexAgentSettings {
                model: validate_model(settings.model)?,
                model_reasoning_effort: settings.model_reasoning_effort,
                features: normalize_features(settings.features)?,
            };
            let normalized = serde_json::to_value(settings).map_err(|_| invalid_tool_settings())?;
            enforce_settings_size(&normalized)?;
            if normalized
                .as_object()
                .map(|object| object.is_empty())
                .unwrap_or(false)
            {
                Ok(None)
            } else {
                Ok(Some(ValidatedToolSettings {
                    tool,
                    value: normalized,
                }))
            }
        }
        Tool::Cursor | Tool::Zcode | Tool::Opencode => {
            unreachable!("unsupported tools returned above")
        }
    }
}

pub(crate) fn tool_settings_dto(
    settings: &BTreeMap<Tool, Value>,
) -> Result<AgentToolSettingsDto, AppError> {
    let claude = settings
        .get(&Tool::Claude)
        .map(|value| {
            let decoded: ClaudeAgentSettings = serde_json::from_value(value.clone())
                .map_err(|_| AppError::internal("数据库中的 Claude Agent 覆盖层无法解析"))?;
            Ok(
                if decoded.model.is_none() && decoded.color.is_none() && decoded.tools.is_none() {
                    None
                } else {
                    Some(decoded)
                },
            )
        })
        .transpose()?
        .flatten();
    let codex = settings
        .get(&Tool::Codex)
        .map(|value| {
            let decoded: CodexAgentSettings = serde_json::from_value(value.clone())
                .map_err(|_| AppError::internal("数据库中的 Codex Agent 覆盖层无法解析"))?;
            Ok(
                if decoded.model.is_none()
                    && decoded.model_reasoning_effort.is_none()
                    && decoded.features.is_none()
                {
                    None
                } else {
                    Some(decoded)
                },
            )
        })
        .transpose()?
        .flatten();
    Ok(AgentToolSettingsDto { claude, codex })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{tool_settings_dto, validate_agent_tool_settings, AgentToolSettingsDto};
    use crate::domain::Tool;

    #[test]
    fn tool_settings_are_strictly_validated_and_normalized() {
        let value = serde_json::json!({
            "model": " sonnet ",
            "color": "cyan",
            "tools": ["Read", "Read", "Bash"],
        });
        let normalized = validate_agent_tool_settings(Tool::Claude, &value)
            .unwrap()
            .unwrap();
        assert_eq!(normalized.value["model"], "sonnet");
        assert_eq!(
            normalized.value["tools"],
            serde_json::json!(["Read", "Bash"])
        );
        assert!(
            validate_agent_tool_settings(Tool::Claude, &serde_json::json!({"unknown": true}))
                .is_err()
        );
        assert!(
            validate_agent_tool_settings(Tool::Claude, &serde_json::json!({"color": 3})).is_err()
        );
        assert!(validate_agent_tool_settings(
            Tool::Codex,
            &serde_json::json!({
                "features": {"Bad-Key": true}
            })
        )
        .is_err());
        assert!(validate_agent_tool_settings(Tool::Cursor, &serde_json::json!({})).is_err());
        assert!(
            validate_agent_tool_settings(Tool::Claude, &serde_json::json!({}))
                .unwrap()
                .is_none()
        );
        let empty = AgentToolSettingsDto::default();
        assert!(empty.claude.is_none() && empty.codex.is_none());

        let mut persisted_empty = BTreeMap::new();
        persisted_empty.insert(Tool::Claude, serde_json::json!({}));
        let dto = tool_settings_dto(&persisted_empty).unwrap();
        assert!(dto.claude.is_none());
    }
}
