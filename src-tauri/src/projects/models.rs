use serde::{Deserialize, Serialize};
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
