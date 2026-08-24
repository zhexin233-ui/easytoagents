//! Skills RPC 合同。中央正文只通过显式内容预览接口返回。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::domain::{SkillStatus, SyncStatus, Tool, TrustStatus};

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
pub struct VersionedSkillInput {
    pub id: String,
    pub row_version: u32,
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
