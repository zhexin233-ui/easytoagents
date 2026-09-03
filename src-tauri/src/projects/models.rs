use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::{
    adapters::{CapabilityState, PolicyState, TargetTrustState},
    domain::{ArtifactKind, SyncStatus, Tool, TrustStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPathStatus {
    Valid,
    Missing,
    PermissionDenied,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GitRepositoryStatus {
    Repository,
    NotRepository,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTargetStatusDto {
    pub tool: Tool,
    pub artifact_kind: ArtifactKind,
    pub target_path: Option<String>,
    pub capability: CapabilityState,
    pub policy: PolicyState,
    pub trust: TargetTrustState,
    pub status: SyncStatus,
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub path_status: ProjectPathStatus,
    pub git_status: GitRepositoryStatus,
    pub codex_trust_status: TrustStatus,
    pub claude_policy_status: PolicyState,
    pub targets: Vec<ProjectTargetStatusDto>,
    pub native_resources: ProjectNativeResourceSummaryDto,
    pub last_scanned_at: Option<String>,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RegisterProjectInput {
    pub display_name: String,
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionedProjectInput {
    pub id: String,
    pub row_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveProjectResultDto {
    pub id: String,
    pub removed: bool,
    pub native_configuration_left_unmanaged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNativeResourceKind {
    Mcp,
    Skill,
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNativeResourceState {
    Active,
    Disabled,
    Missing,
    Conflict,
}

impl ProjectNativeResourceState {
    pub fn from_stable_str(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            "missing" => Some(Self::Missing),
            "conflict" => Some(Self::Conflict),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Missing => "missing",
            Self::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNativeEntryType {
    McpEntry,
    Directory,
    Symlink,
    PromptFile,
}

impl ProjectNativeEntryType {
    pub fn from_stable_str(value: &str) -> Option<Self> {
        match value {
            "mcp_entry" => Some(Self::McpEntry),
            "directory" => Some(Self::Directory),
            "symlink" => Some(Self::Symlink),
            "prompt_file" => Some(Self::PromptFile),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::McpEntry => "mcp_entry",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::PromptFile => "prompt_file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectNativeResourceAction {
    Disable,
    Restore,
}

impl ProjectNativeResourceAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Restore => "restore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNativeResourceSummaryDto {
    pub active: u32,
    pub disabled: u32,
    pub missing: u32,
    pub conflict: u32,
}

impl ProjectNativeResourceSummaryDto {
    pub fn empty() -> Self {
        Self {
            active: 0,
            disabled: 0,
            missing: 0,
            conflict: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNativeResourceDto {
    pub id: String,
    pub project_id: String,
    pub tool: Tool,
    pub artifact_kind: ArtifactKind,
    pub display_name: String,
    pub target_path: String,
    pub entry_type: ProjectNativeEntryType,
    pub state: ProjectNativeResourceState,
    pub row_version: u32,
    pub can_disable: bool,
    pub can_restore: bool,
    pub diagnostic_codes: Vec<String>,
    pub safe_summary: Value,
    pub disabled_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNativeResourceQueryInput {
    pub project_id: String,
    pub tool: Tool,
    pub artifact_kind: ArtifactKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProjectNativeResourceActionInput {
    pub resource_id: String,
    pub row_version: u32,
    pub action: ProjectNativeResourceAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyProjectNativeResourcePreviewInput {
    pub preview_id: String,
}
