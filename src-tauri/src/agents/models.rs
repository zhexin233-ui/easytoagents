//! Agents 的 RPC DTO 与中央定义校验。

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    domain::{AgentName, ManagedProjectSelectionState, SyncStatus, Tool},
    error::AppError,
};

const MAX_DESCRIPTION_BYTES: usize = 1000;
const MAX_PROMPT_BYTES: usize = 65536;

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
/// 也不接管原生文件（导入后通过分配 + 预览 / Apply 进入受管）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmAgentImportInput {
    pub tool: Tool,
    pub agents: Vec<CreateAgentInput>,
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
