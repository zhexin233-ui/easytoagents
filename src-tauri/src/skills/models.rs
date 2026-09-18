//! Skills RPC 合同。中央正文只通过显式内容预览接口返回。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::{
    domain::{SkillStatus, SyncScopeDto, SyncStatus, Tool, TrustStatus},
    sync::{PreviewPlan, SkillTakeoverEntryType},
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedSkillRecord {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub central_path: String,
    pub content_hash: String,
    pub frontmatter: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkillInput {
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportGithubSkillInput {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionedSkillInput {
    pub id: String,
    pub row_version: u32,
}

/// 将已存在的 Skill managed target 中可唯一配对的原生目录采纳回中央库。
///
/// 该合同与 Prompt/Hook/Agent 的外部变化动作保持一致：调用方必须先消费同一份
/// ExternalChangePlan，传入目标身份、目标行版本、managed item/资源行版本和双
/// observed hash。它不接受 preview id，也不复用中央内容采纳（后者不会复制原生
/// 目录或更新 target projection）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptSkillNativeInput {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub target_id: String,
    pub target_row_version: u32,
    pub target_path: String,
    /// Preview 绑定的中央 Skill、项目和 managed item 行版本；目标行版本单独传入，
    /// 若调用方同时携带目标行也必须与 `target_id/target_row_version` 一致。
    pub row_versions: Vec<crate::sync::DatabaseRowVersion>,
    pub observed_full_hash: Option<String>,
    pub observed_managed_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdoptSkillNativeResultDto {
    pub tool: Tool,
    /// 实际更新了中央 source tree 的 Skill 名称；没有变化的条目不会重复报告。
    pub adopted: Vec<String>,
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillDto {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub central_path: String,
    pub content_hash: String,
    pub description: String,
    pub status: SkillStatus,
    pub diagnostic_code: Option<String>,
    pub global_tools: Vec<Tool>,
    pub row_version: u32,
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillContentPreviewDto {
    pub id: String,
    pub name: String,
    pub skill_md: String,
    pub files: Vec<String>,
    pub content_hash: String,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSkillResultDto {
    pub id: String,
    pub deleted: bool,
    #[specta(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_sync_scopes: Option<Vec<SyncScopeDto>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalSkillAssignmentInput {
    pub tool: Tool,
    pub skill_id: String,
    pub assigned: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetProjectSkillAssignmentInput {
    pub project_id: String,
    pub tool: Tool,
    pub skill_id: String,
    pub assigned: bool,
    pub skill_row_version: u32,
    pub project_row_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SkillProjectSelectionState {
    Inherited,
    Selected,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillProjectOptionDto {
    pub skill_id: String,
    pub name: String,
    pub status: SkillStatus,
    pub state: SkillProjectSelectionState,
    pub selectable: bool,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillProjectDto {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub codex_trust_status: TrustStatus,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillProjectOptionsInput {
    pub project_id: String,
    pub tool: Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSkillSyncInput {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub exclude_from_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplySkillPreviewInput {
    pub preview_id: String,
    pub tool: Tool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillTargetStatusDto {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub target_path: Option<String>,
    pub status: SyncStatus,
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SkillImportSourceKind {
    ClaudeGlobal,
    CodexHome,
    CodexAgents,
    CursorHome,
    CursorAgents,
    ZcodeHome,
    ZcodeAgents,
    OpencodeGlobal,
    PiAgentGlobal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SkillImportSourceStatus {
    Ready,
    Missing,
    Empty,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportSourceDto {
    pub kind: SkillImportSourceKind,
    pub path: String,
    pub status: SkillImportSourceStatus,
    pub diagnostic_code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SkillImportCandidateStatus {
    Importable,
    AlreadyImported,
    NameConflict,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportCandidateDto {
    pub candidate_id: String,
    pub name: String,
    pub description: String,
    pub source_paths: Vec<String>,
    pub status: SkillImportCandidateStatus,
    pub reason: Option<String>,
    pub existing_skill_id: Option<String>,
    pub takeover_eligible: bool,
    pub takeover_entry_type: Option<SkillTakeoverEntryType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportPreviewDto {
    pub preview_id: Option<String>,
    pub tool: Tool,
    pub sources: Vec<SkillImportSourceDto>,
    pub candidates: Vec<SkillImportCandidateDto>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmSkillImportInput {
    pub preview_id: String,
    pub candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportResultDto {
    pub tool: Tool,
    pub created_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrepareSkillTakeoverInput {
    pub preview_id: String,
    pub candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillTakeoverPreviewResultDto {
    pub tool: Tool,
    pub assigned_count: u32,
    pub reused_count: u32,
    pub plan: PreviewPlan,
}
