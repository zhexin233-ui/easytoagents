//! Hooks 的 RPC DTO 与中央定义校验。

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    domain::{ArtifactName, HookEvent, SyncStatus, Tool, TrustStatus},
    error::AppError,
    security::contains_detectable_secret,
};

const MAX_COMMAND_BYTES: usize = 4000;
const MAX_MATCHER_BYTES: usize = 500;
const MAX_TIMEOUT_SECONDS: i32 = 3600;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateHookInput {
    pub name: String,
    pub event: HookEvent,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_seconds: Option<i32>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateHookInput {
    pub id: String,
    pub name: String,
    pub event: HookEvent,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_seconds: Option<i32>,
    pub enabled: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionedHookInput {
    pub id: String,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteHookResultDto {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookDto {
    pub id: String,
    pub name: String,
    pub event: HookEvent,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_seconds: Option<i32>,
    pub enabled: bool,
    pub global_tools: Vec<Tool>,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalHookAssignmentInput {
    pub tool: Tool,
    pub hook_id: String,
    pub assigned: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetProjectHookAssignmentInput {
    pub project_id: String,
    pub tool: Tool,
    pub hook_id: String,
    pub assigned: bool,
    pub hook_row_version: u32,
    pub project_row_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HookProjectSelectionState {
    Inherited,
    Selected,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookProjectOptionDto {
    pub hook_id: String,
    pub name: String,
    pub event: HookEvent,
    pub enabled: bool,
    pub state: HookProjectSelectionState,
    pub selectable: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookProjectDto {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub codex_trust_status: TrustStatus,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookProjectOptionsInput {
    pub project_id: String,
    pub tool: Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewHookSyncInput {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub exclude_from_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyHookPreviewInput {
    pub preview_id: String,
    pub tool: Tool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadoptHookTargetInput {
    pub tool: Tool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadoptHookTargetResultDto {
    pub target_path: String,
    pub updated_item_count: u32,
    pub removed_item_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookTargetStatusDto {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub target_path: Option<String>,
    pub status: SyncStatus,
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HookImportCandidateStatus {
    Importable,
    AlreadyManaged,
    NameConflict,
    UnsupportedEvent,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookImportCandidateDto {
    pub candidate_id: String,
    pub name: String,
    pub event: Option<HookEvent>,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_seconds: Option<i32>,
    pub status: HookImportCandidateStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookImportPreviewDto {
    pub tool: Tool,
    pub target_path: String,
    pub candidates: Vec<HookImportCandidateDto>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverHookImportInput {
    pub tool: Tool,
}

/// 用户显式确认导入的条目；服务端只做中央校验，不引用持久化预览。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmHookImportInput {
    pub tool: Tool,
    pub hooks: Vec<CreateHookInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookImportResultDto {
    pub tool: Tool,
    pub created_count: u32,
}

/// 中央 Hook 定义校验：名称、事件、匹配器、命令、超时的统一入口。
pub(crate) fn validate_hook_definition(
    name: &str,
    _event: HookEvent,
    matcher: Option<&str>,
    command: &str,
    timeout_seconds: Option<i32>,
) -> Result<(String, Option<String>, String, Option<i32>), AppError> {
    let name = ArtifactName::parse(name.to_owned())?;
    let command = command.trim();
    if command.is_empty()
        || command.contains('\0')
        || command.len() > MAX_COMMAND_BYTES
        || command
            .chars()
            .any(|c| c.is_control() && c != '\t' && c != '\n')
    {
        return Err(AppError::invalid_input(
            "command",
            "command 不能为空，不能包含 NUL 或非法控制字符，且不能超过 4000 字符",
        ));
    }
    // 与 MCP args 同一口径：可识别的凭据必须走环境变量，不能进入命令行与预览明文。
    if contains_detectable_secret("command", command) {
        return Err(AppError::invalid_input(
            "command",
            "command 不能包含可识别的令牌或密钥；请改用环境变量引用",
        ));
    }
    let matcher = match matcher.map(str::trim) {
        None | Some("") => None,
        Some(matcher) if matcher.contains('\0') || matcher.len() > MAX_MATCHER_BYTES => {
            return Err(AppError::invalid_input(
                "matcher",
                "matcher 不能包含 NUL 且不能超过 500 字符",
            ));
        }
        Some(matcher) => Some(matcher.to_owned()),
    };
    let timeout_seconds = match timeout_seconds {
        None => None,
        Some(value) if value <= 0 || value > MAX_TIMEOUT_SECONDS => {
            return Err(AppError::invalid_input(
                "timeoutSeconds",
                "timeoutSeconds 必须在 1..=3600 秒之间",
            ));
        }
        Some(value) => Some(value),
    };
    Ok((
        name.as_str().to_owned(),
        matcher,
        command.to_owned(),
        timeout_seconds,
    ))
}
