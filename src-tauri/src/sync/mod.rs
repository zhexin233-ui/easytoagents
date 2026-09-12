//! 只读发现、双 hash 漂移分类、Preview 持久化与受控写入编排。

mod apply;
pub(crate) mod managed;

pub use apply::{
    apply_persisted_preview, delete_snapshots, detect_interrupted_run, list_snapshots,
    preview_restore, restore_snapshot, ApplyFaultDecision, ApplyFaultEvent, ApplyFaultInjector,
    ApplyResult, ApplyTargetInput, DeleteSnapshotsInput, DeleteSnapshotsResultDto,
    InterruptedRunPlan, ManagedItemApply, NoApplyFault, RestorePreview, SnapshotDeleteFailureDto,
    SnapshotStorageKind, SnapshotSummary,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Component, Path, PathBuf},
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use specta::Type;
use uuid::Uuid;

use crate::{
    adapters::{
        validate_managed_ownership, CapabilityState, DirectoryEntry, ManagedOwnership,
        ObservedDocument, ObservedRaw, PolicyState, PromptOverrideState, TargetDescriptor,
        TargetTrustState, ToolAdapter,
    },
    db::Database,
    domain::{ChangeKind, Scope, SyncStatus, TargetType},
    error::{AppError, ErrorCode},
    git::GitPathStatus,
    security::SecretRedactor,
};

pub const WARNING_EXTERNAL_NON_OWNED_CHANGE: &str = "EXTERNAL_NON_OWNED_CHANGE";
pub const WARNING_GIT_TRACKED: &str = "GIT_TRACKED";
pub const WARNING_GIT_IGNORED: &str = "GIT_IGNORED";
/// Hooks 初始接管：基线为空且观测到的 hooks 子树没有任何条目。
pub const HOOK_TARGET_INITIAL_EMPTY_HOOKS: &str = "HOOK_TARGET_INITIAL_EMPTY_HOOKS";
pub const ERROR_EXTERNAL_OWNED_CHANGE: &str = "EXTERNAL_OWNED_CHANGE";
pub const ERROR_MANAGED_ITEM_BASELINE_MISMATCH: &str = "MANAGED_ITEM_BASELINE_MISMATCH";
pub const ERROR_TARGET_TYPE_CHANGED: &str = "TARGET_TYPE_CHANGED";
pub const ERROR_CLAUDE_POLICY_UNKNOWN: &str = "CLAUDE_POLICY_UNKNOWN";
pub const ERROR_CODEX_TRUST_UNKNOWN: &str = "CODEX_TRUST_UNKNOWN";
pub const ERROR_INCOMPLETE_BASELINE: &str = "INCOMPLETE_MANAGED_BASELINE";
pub const WARNING_CODEX_PROMPT_OVERRIDE: &str = "CODEX_PROMPT_OVERRIDE_DETECTED";
pub const WARNING_CODEX_PROMPT_OVERRIDE_UNKNOWN: &str = "CODEX_PROMPT_OVERRIDE_UNKNOWN";
pub const WARNING_SKILL_TAKEOVER_CONFIRMATION: &str = "SKILL_TAKEOVER_REQUIRES_CONFIRMATION";
pub const WARNING_PROJECT_NATIVE_RESOURCE_CONFIRMATION: &str =
    "PROJECT_NATIVE_RESOURCE_REQUIRES_CONFIRMATION";

pub(crate) use managed::managed_record_row_version;
pub(crate) use managed::safe_row_version;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SkillTakeoverEntryType {
    ExternalSymlink,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTakeoverEntry {
    pub name: String,
    pub entry_path: String,
    pub entry_type: SkillTakeoverEntryType,
    pub expected_fingerprint: String,
    pub content_hash: String,
    pub central_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeResourceActionKind {
    Disable,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeResourceEntryType {
    McpEntry,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNativeResourceEvidence {
    pub resource_id: String,
    pub resource_row_version: u32,
    pub action: NativeResourceActionKind,
    pub entry_type: NativeResourceEntryType,
    pub external_key: String,
    pub observed_item_hash: String,
    pub expected_fingerprint: Option<String>,
    pub content_hash: Option<String>,
    pub restore_snapshot_id: Option<String>,
    pub restore_snapshot_path: Option<String>,
    pub restore_link_target: Option<String>,
    pub restore_file_mode: Option<u32>,
}

pub struct ObservedTarget {
    pub target_type: TargetType,
    pub full_hash: String,
    pub managed_hash: String,
    pub managed_projection: Value,
    document: ObservedDocument,
}

impl ObservedTarget {
    pub fn document(&self) -> &ObservedDocument {
        &self.document
    }
}

pub enum TargetScan {
    Observed(Box<ObservedTarget>),
    Missing,
    /// 领域服务已读取目标，但逐项受管基线不再匹配；不得进入合并或清理流程。
    ManagedItemBaselineMismatch,
    ParseError,
    PermissionDenied,
    TargetTypeChanged(TargetType),
    Failed,
    Unavailable,
}

/// 读取、解析并投影一个目标。所有路径来自 TargetDescriptor，不读取进程环境。
pub fn scan_target(
    adapter: &dyn ToolAdapter,
    target: &TargetDescriptor,
    ownership: &ManagedOwnership,
) -> TargetScan {
    if adapter.tool() != target.tool || validate_managed_ownership(target, ownership).is_err() {
        return TargetScan::Failed;
    }
    let Some(path) = target.path() else {
        return TargetScan::Unavailable;
    };
    if let Some(failure) = inspect_target_ancestors(path) {
        return failure;
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return TargetScan::Missing,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return TargetScan::PermissionDenied;
        }
        Err(_) => return TargetScan::Failed,
    };
    let observed_type = target_type(&metadata);
    if observed_type != target.format.expected_type() {
        return TargetScan::TargetTypeChanged(observed_type);
    }

    let (raw, full_hash) = match target.format.expected_type() {
        TargetType::File => {
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    return TargetScan::PermissionDenied;
                }
                Err(_) => return TargetScan::Failed,
            };
            let full_hash = hash_bytes(&bytes);
            (ObservedRaw::File(bytes), full_hash)
        }
        TargetType::Directory => {
            let (entries, full_hash) = match read_directory_target(path) {
                Ok(result) => result,
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    return TargetScan::PermissionDenied;
                }
                Err(_) => return TargetScan::Failed,
            };
            (ObservedRaw::Directory(entries), full_hash)
        }
        TargetType::Symlink | TargetType::Missing => {
            return TargetScan::TargetTypeChanged(observed_type);
        }
    };

    let document = match adapter.parse(target, raw) {
        Ok(document) => document,
        Err(error) if error.code() == ErrorCode::PermissionDenied => {
            return TargetScan::PermissionDenied;
        }
        Err(_) => return TargetScan::ParseError,
    };
    let managed_projection = match adapter.project_managed(&document, ownership) {
        Ok(projection) => projection,
        Err(_) => return TargetScan::ParseError,
    };
    let managed_hash = hash_json(&managed_projection);

    TargetScan::Observed(Box::new(ObservedTarget {
        target_type: observed_type,
        full_hash,
        managed_hash,
        managed_projection,
        document,
    }))
}

fn inspect_target_ancestors(path: &Path) -> Option<TargetScan> {
    if !path.is_absolute()
        || path == Path::new("/")
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Some(TargetScan::Failed);
    }
    let parent = path.parent()?;
    let mut current = PathBuf::new();
    for component in parent.components() {
        match component {
            Component::RootDir => current.push(Path::new("/")),
            Component::Normal(segment) => current.push(segment),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Some(TargetScan::Failed);
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Some(TargetScan::TargetTypeChanged(TargetType::Symlink));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Some(TargetScan::TargetTypeChanged(target_type(&metadata)));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Some(TargetScan::Missing);
            }
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                return Some(TargetScan::PermissionDenied);
            }
            Err(_) => return Some(TargetScan::Failed),
        }
    }
    None
}

/// 读取目录目标的全部条目与完整 hash；`scan_target` 与启动基线对账共用同一口径。
pub(crate) fn read_directory_target(
    path: &Path,
) -> io::Result<(BTreeMap<String, DirectoryEntry>, String)> {
    let entries = read_directory_entries(path)?;
    let full_hash = hash_bytes(&serde_json::to_vec(&entries)?);
    Ok((entries, full_hash))
}

fn read_directory_entries(path: &Path) -> io::Result<BTreeMap<String, DirectoryEntry>> {
    let mut entries = BTreeMap::new();
    for child in fs::read_dir(path)? {
        let child = child?;
        let name = child.file_name().into_string().map_err(|_| {
            // 文件名本身可能包含用户数据；不要把不可解码的原始字节复制到
            // 后续 AppError source 或日志中。
            io::Error::new(io::ErrorKind::InvalidData, "目录项名称不是 UTF-8")
        })?;
        let metadata = fs::symlink_metadata(child.path())?;
        let entry_type = target_type(&metadata);
        let link_target = if entry_type == TargetType::Symlink {
            Some(
                fs::read_link(child.path())?
                    .to_str()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "链接目标不是 UTF-8")
                    })?
                    .to_owned(),
            )
        } else {
            None
        };
        entries.insert(
            name,
            DirectoryEntry {
                target_type: entry_type,
                link_target,
            },
        );
    }
    Ok(entries)
}

fn target_type(metadata: &fs::Metadata) -> TargetType {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        TargetType::Symlink
    } else if file_type.is_file() {
        TargetType::File
    } else if file_type.is_dir() {
        TargetType::Directory
    } else {
        TargetType::Missing
    }
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `serde_json::Map` 在本 crate 未开启 `preserve_order`，底层是 `BTreeMap`，
/// 序列化天然按键有序；不再深拷贝重建对象再序列化。
/// `tests::serde_json_serializes_object_keys_in_sorted_order` 守住这个前提。
pub fn hash_json(value: &Value) -> String {
    // `Value` 的 Display 输出与 `to_vec` 相同（紧凑 JSON），且对任意 Value 都不会失败。
    hash_bytes(value.to_string().as_bytes())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedTargetBaseline {
    pub target_id: String,
    pub target_row_version: i64,
    pub full_hash: Option<String>,
    pub managed_hash: Option<String>,
}

pub fn load_managed_target_baseline(
    database: &Database,
    target_id: &str,
) -> Result<ManagedTargetBaseline, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let row =
        crate::db::sync::load_managed_target_baseline(database.connection(), target_id, &path)?
            .ok_or_else(|| AppError::not_found("managedTarget", target_id))?;
    Ok(ManagedTargetBaseline {
        target_id: target_id.to_owned(),
        target_row_version: row.row_version,
        full_hash: row.baseline_full_hash,
        managed_hash: row.baseline_managed_hash,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftAssessment {
    pub status: SyncStatus,
    pub can_merge: bool,
    pub diagnostic_codes: Vec<String>,
}

pub fn assess_drift(
    target: &TargetDescriptor,
    baseline: &ManagedTargetBaseline,
    scan: &TargetScan,
) -> DriftAssessment {
    match target.capability.state {
        CapabilityState::ToolNotInstalled | CapabilityState::Unsupported => {
            return assessment(
                SyncStatus::Failed,
                false,
                target
                    .capability
                    .diagnostic_code
                    .clone()
                    .into_iter()
                    .collect(),
            );
        }
        CapabilityState::Supported => {}
    }
    match target.policy {
        PolicyState::Blocked => {
            return assessment(
                SyncStatus::PolicyBlocked,
                false,
                vec!["CLAUDE_POLICY_BLOCKED".to_owned()],
            );
        }
        PolicyState::Unknown => {
            return assessment(
                SyncStatus::PolicyBlocked,
                false,
                vec![ERROR_CLAUDE_POLICY_UNKNOWN.to_owned()],
            );
        }
        PolicyState::Allowed => {}
    }
    match target.trust {
        TargetTrustState::Untrusted => {
            return assessment(
                SyncStatus::Untrusted,
                false,
                vec!["CODEX_PROJECT_UNTRUSTED".to_owned()],
            );
        }
        TargetTrustState::Unknown => {
            return assessment(
                SyncStatus::Untrusted,
                false,
                vec![ERROR_CODEX_TRUST_UNKNOWN.to_owned()],
            );
        }
        TargetTrustState::NotRequired | TargetTrustState::Trusted => {}
    }

    if baseline.full_hash.is_some() != baseline.managed_hash.is_some() {
        return assessment(
            SyncStatus::ExternalOwnedChange,
            false,
            vec![ERROR_INCOMPLETE_BASELINE.to_owned()],
        );
    }

    match scan {
        TargetScan::Missing => assessment(SyncStatus::Missing, true, Vec::new()),
        TargetScan::ManagedItemBaselineMismatch => assessment(
            SyncStatus::ExternalOwnedChange,
            false,
            vec![ERROR_MANAGED_ITEM_BASELINE_MISMATCH.to_owned()],
        ),
        TargetScan::ParseError => assessment(
            SyncStatus::ParseError,
            false,
            vec!["TARGET_PARSE_ERROR".to_owned()],
        ),
        TargetScan::PermissionDenied => assessment(
            SyncStatus::PermissionDenied,
            false,
            vec!["TARGET_PERMISSION_DENIED".to_owned()],
        ),
        TargetScan::TargetTypeChanged(_) => assessment(
            SyncStatus::TargetTypeChanged,
            false,
            vec![ERROR_TARGET_TYPE_CHANGED.to_owned()],
        ),
        TargetScan::Failed | TargetScan::Unavailable => assessment(
            SyncStatus::Failed,
            false,
            vec!["TARGET_READ_FAILED".to_owned()],
        ),
        TargetScan::Observed(observed) => match (&baseline.full_hash, &baseline.managed_hash) {
            (Some(full), Some(managed))
                if full == &observed.full_hash && managed == &observed.managed_hash =>
            {
                assessment(SyncStatus::InSync, true, Vec::new())
            }
            (_, Some(managed)) if managed == &observed.managed_hash => assessment(
                SyncStatus::ExternalNonOwnedChange,
                true,
                vec![WARNING_EXTERNAL_NON_OWNED_CHANGE.to_owned()],
            ),
            (None, None) if projection_is_empty(&observed.managed_projection) => assessment(
                SyncStatus::ExternalNonOwnedChange,
                true,
                vec![WARNING_EXTERNAL_NON_OWNED_CHANGE.to_owned()],
            ),
            _ => assessment(
                SyncStatus::ExternalOwnedChange,
                false,
                vec![ERROR_EXTERNAL_OWNED_CHANGE.to_owned()],
            ),
        },
    }
}

fn assessment(
    status: SyncStatus,
    can_merge: bool,
    diagnostic_codes: Vec<String>,
) -> DriftAssessment {
    DriftAssessment {
        status,
        can_merge,
        diagnostic_codes,
    }
}

fn projection_is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(object) => object.is_empty(),
        Value::Array(values) => values.is_empty(),
        Value::String(value) => value.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseEntityType {
    ProviderProfile,
    PromptProfile,
    McpServer,
    Skill,
    Hook,
    Agent,
    Project,
    ManagedTarget,
    ManagedItem,
    ProjectNativeResource,
}

impl DatabaseEntityType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderProfile => "provider_profile",
            Self::PromptProfile => "prompt_profile",
            Self::McpServer => "mcp_server",
            Self::Skill => "skill",
            Self::Hook => "hook",
            Self::Agent => "agent",
            Self::Project => "project",
            Self::ManagedTarget => "managed_target",
            Self::ManagedItem => "managed_item",
            Self::ProjectNativeResource => "project_native_resource",
        }
    }

    const fn table(self) -> &'static str {
        match self {
            Self::ProviderProfile => "provider_profiles",
            Self::PromptProfile => "prompt_profiles",
            Self::McpServer => "mcp_servers",
            Self::Skill => "skills",
            Self::Hook => "hooks",
            Self::Agent => "agents",
            Self::Project => "projects",
            Self::ManagedTarget => "managed_targets",
            Self::ManagedItem => "managed_items",
            Self::ProjectNativeResource => "project_native_resources",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseRowVersion {
    pub entity_type: DatabaseEntityType,
    pub entity_id: String,
    pub row_version: u32,
}

pub struct PreviewTargetRequest {
    pub descriptor: TargetDescriptor,
    pub ownership: ManagedOwnership,
    pub baseline: ManagedTargetBaseline,
    pub scan: TargetScan,
    /// 条目基线不一致的外部键；仅条目级托管的服务（当前为 MCP）填写。
    pub baseline_mismatched_items: Vec<String>,
    /// 该冲突是否可通过「以当前内容重新接管」解除；由服务端按 drift 类别判定。
    pub readopt_available: bool,
    pub desired_projection: Value,
    pub row_versions: Vec<DatabaseRowVersion>,
    pub git: Option<GitPathStatus>,
    /// 只有预览界面显式确认后才能置为 true；tracked 目标会被强制忽略。
    pub exclude_from_git: bool,
    /// 仅首次全局 Skills 接管使用；普通 Preview 必须保持空集合。
    pub skill_takeover_entries: Vec<SkillTakeoverEntry>,
    /// 仅项目原生资源禁用/恢复使用；普通 Preview 必须保持 None。
    pub project_native_action: Option<ProjectNativeResourceEvidence>,
    /// 仅 Hooks 初始接管使用：目标从未纳入基线且观测到的 hooks 子树没有任何
    /// 条目（例如仅存在空的 `hooks` 键）。此时允许合并（呈现为新增/更新），
    /// 而不是把空受管键误判为外部改写。子树已有内容时由服务保持 false，
    /// 维持 Conflict 以便用户先导入或显式重新接管。
    pub hook_initial_adopt: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTargetPlan {
    pub target_id: String,
    pub descriptor: TargetDescriptor,
    pub ownership: ManagedOwnership,
    pub change_kind: ChangeKind,
    pub status: SyncStatus,
    pub current_full_hash: Option<String>,
    pub current_managed_hash: Option<String>,
    pub desired_managed_hash: String,
    pub target_row_version: u32,
    pub row_versions: Vec<DatabaseRowVersion>,
    pub redacted_diff: Value,
    pub warning_codes: Vec<String>,
    pub baseline_mismatched_items: Vec<String>,
    pub readopt_available: bool,
    pub error_code: Option<ErrorCode>,
    pub git: Option<GitPathStatus>,
    pub exclude_from_git: bool,
    #[specta(skip)]
    pub skill_takeover_entries: Vec<SkillTakeoverEntry>,
    #[specta(skip)]
    pub project_native_action: Option<ProjectNativeResourceEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPlan {
    pub preview_id: String,
    pub scope: Scope,
    pub project_id: Option<String>,
    pub db_version: u32,
    pub targets: Vec<PreviewTargetPlan>,
    pub warning_codes: Vec<String>,
}

pub fn build_preview_plan(
    scope: Scope,
    project_id: Option<String>,
    requests: Vec<PreviewTargetRequest>,
    redactor: &SecretRedactor,
) -> Result<PreviewPlan, AppError> {
    if (scope == Scope::Global && project_id.is_some())
        || (scope == Scope::Project && project_id.is_none())
    {
        return Err(AppError::invalid_input(
            "projectId",
            "Preview scope 与项目标识不匹配",
        ));
    }

    let mut targets = Vec::with_capacity(requests.len());
    let mut all_warning_codes = BTreeSet::new();
    let mut all_row_versions = Vec::new();
    let mut expected_row_versions = BTreeMap::new();
    let mut target_ids = BTreeSet::new();
    for request in requests {
        if !target_ids.insert(request.baseline.target_id.clone()) {
            return Err(AppError::invalid_input(
                "targetId",
                "Preview 不能重复包含同一受管目标",
            ));
        }
        if request.descriptor.scope != scope
            || (scope == Scope::Project && request.descriptor.project_root.is_none())
        {
            return Err(AppError::invalid_input(
                "targetScope",
                "Preview 包含其他 scope 的目标",
            ));
        }
        let mut assessment = assess_drift(&request.descriptor, &request.baseline, &request.scan);
        if request.hook_initial_adopt
            && assessment.status == SyncStatus::ExternalOwnedChange
            && request.baseline.full_hash.is_none()
            && request.baseline.managed_hash.is_none()
        {
            assessment = DriftAssessment {
                status: SyncStatus::ExternalNonOwnedChange,
                can_merge: true,
                diagnostic_codes: vec![HOOK_TARGET_INITIAL_EMPTY_HOOKS.to_owned()],
            };
        }
        let (current_full_hash, current_managed_hash, before_projection) = match &request.scan {
            TargetScan::Observed(observed) => (
                Some(observed.full_hash.clone()),
                Some(observed.managed_hash.clone()),
                observed.managed_projection.clone(),
            ),
            _ => (None, None, Value::Null),
        };
        let desired_projection = request.desired_projection.clone();
        let desired_hash = hash_json(&desired_projection);
        let current_matches_desired = current_managed_hash.as_ref() == Some(&desired_hash);

        let takeover_allows_merge = takeover_entries_cover_projection(
            &request.skill_takeover_entries,
            &before_projection,
            &desired_projection,
            request.baseline.full_hash.is_none() && request.baseline.managed_hash.is_none(),
        );
        let native_allows_merge = native_action_allows_merge(
            request.project_native_action.as_ref(),
            &request.scan,
            &desired_projection,
        );
        let (mut change_kind, error_code) =
            if !assessment.can_merge && !takeover_allows_merge && !native_allows_merge {
                (
                    ChangeKind::Conflict,
                    Some(error_code_for_status(assessment.status)),
                )
            } else if matches!(request.scan, TargetScan::Missing) {
                (ChangeKind::Add, None)
            } else if current_matches_desired {
                if assessment.status == SyncStatus::ExternalNonOwnedChange {
                    (ChangeKind::Warning, None)
                } else {
                    (ChangeKind::Unchanged, None)
                }
            } else if projection_is_empty(&desired_projection) {
                (ChangeKind::Delete, None)
            } else {
                (ChangeKind::Update, None)
            };

        let mut warning_codes = assessment.diagnostic_codes;
        if takeover_allows_merge {
            // 显式接管证据已覆盖冲突，预览只保留需要用户确认的接管提示。
            warning_codes.retain(|code| code != ERROR_EXTERNAL_OWNED_CHANGE);
            warning_codes.push(WARNING_SKILL_TAKEOVER_CONFIRMATION.to_owned());
        }
        if native_allows_merge {
            warning_codes.push(WARNING_PROJECT_NATIVE_RESOURCE_CONFIRMATION.to_owned());
        }
        match request.descriptor.prompt_override {
            PromptOverrideState::Present => {
                warning_codes.push(WARNING_CODEX_PROMPT_OVERRIDE.to_owned());
            }
            PromptOverrideState::Unknown => {
                warning_codes.push(WARNING_CODEX_PROMPT_OVERRIDE_UNKNOWN.to_owned());
            }
            PromptOverrideState::NotApplicable | PromptOverrideState::NotPresent => {}
        }
        if let Some(git) = &request.git {
            if git.tracked {
                warning_codes.push(WARNING_GIT_TRACKED.to_owned());
            }
            if git.ignored {
                warning_codes.push(WARNING_GIT_IGNORED.to_owned());
            }
        }
        let exclude_from_git = request
            .git
            .as_ref()
            .is_some_and(|git| git.is_repository && !git.tracked)
            && request.exclude_from_git;
        warning_codes.sort();
        warning_codes.dedup();
        if change_kind == ChangeKind::Unchanged && !warning_codes.is_empty() {
            change_kind = ChangeKind::Warning;
        }
        all_warning_codes.extend(warning_codes.iter().cloned());

        let mut target_redactor = redactor.clone();
        for selector in &request.descriptor.sensitive_selectors {
            target_redactor.register_selector(selector);
        }
        let redacted_diff = json!({
            "before": target_redactor.redact_structure(&before_projection).into_value(),
            "after": target_redactor.redact_structure(&desired_projection).into_value(),
        });

        let target_row_version =
            u32::try_from(request.baseline.target_row_version).map_err(|error| {
                AppError::invalid_input("rowVersion", "数据库 row_version 超出 RPC 安全范围")
                    .with_source(error)
            })?;
        let target_version = DatabaseRowVersion {
            entity_type: DatabaseEntityType::ManagedTarget,
            entity_id: request.baseline.target_id.clone(),
            row_version: target_row_version,
        };
        record_row_version(&mut expected_row_versions, &target_version)?;
        all_row_versions.push(target_version);
        for row in &request.row_versions {
            record_row_version(&mut expected_row_versions, row)?;
        }
        all_row_versions.extend(request.row_versions.iter().cloned());
        targets.push(PreviewTargetPlan {
            target_id: request.baseline.target_id,
            descriptor: request.descriptor,
            ownership: request.ownership,
            change_kind,
            status: assessment.status,
            current_full_hash,
            current_managed_hash,
            desired_managed_hash: desired_hash,
            target_row_version,
            row_versions: request.row_versions,
            redacted_diff,
            warning_codes,
            baseline_mismatched_items: request.baseline_mismatched_items,
            readopt_available: request.readopt_available,
            error_code,
            git: request.git,
            exclude_from_git,
            skill_takeover_entries: request.skill_takeover_entries,
            project_native_action: request.project_native_action,
        });
    }

    Ok(PreviewPlan {
        preview_id: Uuid::new_v4().to_string(),
        scope,
        project_id,
        db_version: fingerprint_row_versions(&all_row_versions),
        targets,
        warning_codes: all_warning_codes.into_iter().collect(),
    })
}

fn takeover_entries_cover_projection(
    entries: &[SkillTakeoverEntry],
    before: &Value,
    desired: &Value,
    initial_baseline: bool,
) -> bool {
    if entries.is_empty() || !initial_baseline {
        return false;
    }
    let (Some(before), Some(desired)) = (before.as_object(), desired.as_object()) else {
        return false;
    };
    let names = entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>();
    entries.iter().all(|entry| {
        before.contains_key(&entry.name)
            && desired
                .get(&entry.name)
                .and_then(|value| value.get("linkTarget"))
                .and_then(Value::as_str)
                == Some(entry.central_path.as_str())
    }) && before.iter().all(|(name, value)| {
        desired.get(name).map_or(true, |expected| {
            expected == value || names.contains(name.as_str())
        })
    })
}

fn native_action_allows_merge(
    evidence: Option<&ProjectNativeResourceEvidence>,
    scan: &TargetScan,
    desired: &Value,
) -> bool {
    let Some(evidence) = evidence else {
        return false;
    };
    match evidence.action {
        NativeResourceActionKind::Disable => {
            !projection_is_empty(&scan_managed_projection(scan)) && projection_is_empty(desired)
        }
        NativeResourceActionKind::Restore => match evidence.entry_type {
            NativeResourceEntryType::McpEntry
            | NativeResourceEntryType::Directory
            | NativeResourceEntryType::Symlink => {
                (matches!(scan, TargetScan::Missing)
                    || projection_is_empty(&scan_managed_projection(scan)))
                    && !projection_is_empty(desired)
            }
        },
    }
}

fn scan_managed_projection(scan: &TargetScan) -> Value {
    match scan {
        TargetScan::Observed(observed) => observed.managed_projection.clone(),
        _ => Value::Null,
    }
}

fn record_row_version(
    expected: &mut BTreeMap<(DatabaseEntityType, String), u32>,
    row: &DatabaseRowVersion,
) -> Result<(), AppError> {
    let key = (row.entity_type, row.entity_id.clone());
    if expected
        .insert(key, row.row_version)
        .is_some_and(|current| current != row.row_version)
    {
        return Err(AppError::invalid_input(
            "rowVersions",
            "同一数据库实体包含互相矛盾的 row_version",
        ));
    }
    Ok(())
}

fn error_code_for_status(status: SyncStatus) -> ErrorCode {
    match status {
        SyncStatus::ParseError => ErrorCode::ParseError,
        SyncStatus::PermissionDenied => ErrorCode::PermissionDenied,
        SyncStatus::PolicyBlocked => ErrorCode::PolicyBlocked,
        SyncStatus::Untrusted => ErrorCode::UntrustedProject,
        SyncStatus::ExternalOwnedChange | SyncStatus::TargetTypeChanged => ErrorCode::Conflict,
        SyncStatus::Failed | SyncStatus::Missing => ErrorCode::NotFound,
        SyncStatus::InSync | SyncStatus::ExternalNonOwnedChange => ErrorCode::Conflict,
    }
}

fn fingerprint_row_versions(rows: &[DatabaseRowVersion]) -> u32 {
    let normalized = rows
        .iter()
        .map(|row| {
            (
                format!("{}:{}", row.entity_type.as_str(), row.entity_id),
                row.row_version,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let digest = Sha256::digest(Value::from_iter(normalized).to_string().as_bytes());
    let mut bytes = [0_u8; 4];
    bytes.copy_from_slice(&digest[..4]);
    u32::from_be_bytes(bytes)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedPreviewEnvelope {
    pub descriptor: TargetDescriptor,
    pub ownership: ManagedOwnership,
    pub current_full_hash: Option<String>,
    pub current_managed_hash: Option<String>,
    pub desired_managed_hash: String,
    pub target_row_version: u32,
    pub row_versions: Vec<DatabaseRowVersion>,
    pub redacted_diff: Value,
    pub git: Option<GitPathStatus>,
    pub exclude_from_git: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_snapshot_row_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_current_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_target_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_root: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_takeover_entries: Vec<SkillTakeoverEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_native_action: Option<ProjectNativeResourceEvidence>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersistedPreviewItem {
    pub target_id: String,
    pub target_path: String,
    pub change_kind: ChangeKind,
    pub status: SyncStatus,
    pub envelope: PersistedPreviewEnvelope,
    pub warning_codes: Vec<String>,
    pub error_code: Option<ErrorCode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersistedPreview {
    pub preview_id: String,
    pub scope: Scope,
    pub project_id: Option<String>,
    pub db_version: i64,
    pub items: Vec<PersistedPreviewItem>,
}

/// 在单个 SQLite 事务中保存 preview；不修改任何 Claude/Codex/Git 目标。
pub fn persist_preview(database: &mut Database, plan: &PreviewPlan) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    database.with_immediate_transaction(|transaction| {
        persist_preview_in_connection(transaction, plan, &database_path)
    })
}

pub(crate) fn persist_preview_in_connection(
    connection: &Connection,
    plan: &PreviewPlan,
    database_path: &str,
) -> Result<(), AppError> {
    verify_preview_row_versions(connection, plan, database_path)?;
    crate::db::sync::insert_sync_run(
        connection,
        crate::db::sync::SyncRunInsert {
            id: &plan.preview_id,
            kind: "preview",
            status: "previewed",
            scope: plan.scope.as_str(),
            project_id: plan.project_id.as_deref(),
            db_version: i64::from(plan.db_version),
            database_path,
            operation: "insert_preview_run",
        },
    )?;

    for (target_order, target) in plan.targets.iter().enumerate() {
        let envelope = PersistedPreviewEnvelope {
            descriptor: target.descriptor.clone(),
            ownership: target.ownership.clone(),
            current_full_hash: target.current_full_hash.clone(),
            current_managed_hash: target.current_managed_hash.clone(),
            desired_managed_hash: target.desired_managed_hash.clone(),
            target_row_version: target.target_row_version,
            row_versions: target.row_versions.clone(),
            redacted_diff: target.redacted_diff.clone(),
            git: target.git.clone(),
            exclude_from_git: target.exclude_from_git,
            restore_snapshot_id: None,
            restore_snapshot_row_version: None,
            restore_current_fingerprint: None,
            restore_target_path: None,
            allowed_root: None,
            skill_takeover_entries: target.skill_takeover_entries.clone(),
            project_native_action: target.project_native_action.clone(),
        };
        let envelope_json = serde_json::to_string(&envelope).map_err(|error| {
            AppError::database(database_path, "serialize_preview_item").with_source(error)
        })?;
        let warning_codes_json = serde_json::to_string(&target.warning_codes).map_err(|error| {
            AppError::database(database_path, "serialize_warning_codes").with_source(error)
        })?;
        let target_order = i64::try_from(target_order).map_err(|error| {
            AppError::invalid_input("targetOrder", "Preview 目标顺序超出数据库范围")
                .with_source(error)
        })?;
        crate::db::sync::insert_sync_item(
            connection,
            crate::db::sync::SyncItemInsert {
                id: &Uuid::new_v4().to_string(),
                run_id: &plan.preview_id,
                target_id: &target.target_id,
                change_kind: target.change_kind.as_str(),
                status: target.status.as_str(),
                redacted_diff_json: &envelope_json,
                warning_codes_json: &warning_codes_json,
                error_code: target.error_code.map(ErrorCode::as_str),
                target_order,
                database_path,
                operation: "insert_preview_item",
            },
        )?;
    }
    Ok(())
}

fn verify_preview_row_versions(
    transaction: &Connection,
    plan: &PreviewPlan,
    database_path: &str,
) -> Result<(), AppError> {
    let mut expected = BTreeMap::new();
    for target in &plan.targets {
        let identity = crate::db::sync::load_managed_target_identity(
            transaction,
            &target.target_id,
            database_path,
            "verify_preview_target_identity",
        )?
        .ok_or_else(|| AppError::stale_preview(&plan.preview_id, &target.target_id))?;
        if u32::try_from(identity.row_version).ok() != Some(target.target_row_version) {
            return Err(AppError::stale_preview(&plan.preview_id, &target.target_id));
        }
        if identity.tool != target.descriptor.tool.as_str()
            || identity.artifact_kind != target.descriptor.artifact_kind.as_str()
            || identity.scope != target.descriptor.scope.as_str()
            || identity.project_id != plan.project_id
            || target.descriptor.path.as_deref() != Some(identity.target_path.as_str())
            || target.descriptor.project_root != identity.project_root
        {
            return Err(AppError::invalid_input(
                "targetDescriptor",
                "Preview 目标描述与数据库受管目标不一致",
            ));
        }
        record_row_version(
            &mut expected,
            &DatabaseRowVersion {
                entity_type: DatabaseEntityType::ManagedTarget,
                entity_id: target.target_id.clone(),
                row_version: target.target_row_version,
            },
        )?;
        for row in &target.row_versions {
            record_row_version(&mut expected, row)?;
        }
    }
    for ((entity_type, entity_id), expected_version) in expected {
        let actual = crate::db::sync::load_row_version(
            transaction,
            entity_type.table(),
            &entity_id,
            database_path,
            "verify_preview_row_version",
        )?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(expected_version) {
            return Err(AppError::stale_preview(&plan.preview_id, &entity_id));
        }
    }
    Ok(())
}

pub fn load_persisted_preview(
    database: &Database,
    preview_id: &str,
) -> Result<PersistedPreview, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let run =
        crate::db::sync::load_persisted_preview_run(database.connection(), preview_id, &path)?
            .ok_or_else(|| AppError::not_found("preview", preview_id))?;
    let mut items = Vec::new();
    for row in
        crate::db::sync::load_persisted_preview_items(database.connection(), preview_id, &path)?
    {
        items.push(PersistedPreviewItem {
            target_id: row.target_id,
            target_path: row.target_path,
            change_kind: parse_change_kind(&row.change_kind)?,
            status: parse_sync_status(&row.status)?,
            envelope: serde_json::from_str(&row.redacted_diff_json).map_err(|error| {
                AppError::database(&path, "parse_preview_envelope").with_source(error)
            })?,
            warning_codes: serde_json::from_str(&row.warning_codes_json).map_err(|error| {
                AppError::database(&path, "parse_warning_codes").with_source(error)
            })?,
            error_code: row
                .error_code
                .as_deref()
                .map(parse_error_code)
                .transpose()?,
        });
    }

    Ok(PersistedPreview {
        preview_id: preview_id.to_owned(),
        scope: parse_scope(&run.scope)?,
        project_id: run.project_id,
        db_version: run.db_version,
        items,
    })
}

fn parse_scope(value: &str) -> Result<Scope, AppError> {
    match value {
        "global" => Ok(Scope::Global),
        "project" => Ok(Scope::Project),
        _ => Err(AppError::invalid_input("scope", "数据库包含未知 scope")),
    }
}

fn parse_change_kind(value: &str) -> Result<ChangeKind, AppError> {
    match value {
        "add" => Ok(ChangeKind::Add),
        "update" => Ok(ChangeKind::Update),
        "delete" => Ok(ChangeKind::Delete),
        "unchanged" => Ok(ChangeKind::Unchanged),
        "warning" => Ok(ChangeKind::Warning),
        "conflict" => Ok(ChangeKind::Conflict),
        _ => Err(AppError::invalid_input(
            "changeKind",
            "数据库包含未知 change kind",
        )),
    }
}

fn parse_sync_status(value: &str) -> Result<SyncStatus, AppError> {
    match value {
        "in_sync" => Ok(SyncStatus::InSync),
        "external_non_owned_change" => Ok(SyncStatus::ExternalNonOwnedChange),
        "external_owned_change" => Ok(SyncStatus::ExternalOwnedChange),
        "missing" => Ok(SyncStatus::Missing),
        "parse_error" => Ok(SyncStatus::ParseError),
        "permission_denied" => Ok(SyncStatus::PermissionDenied),
        "policy_blocked" => Ok(SyncStatus::PolicyBlocked),
        "untrusted" => Ok(SyncStatus::Untrusted),
        "target_type_changed" => Ok(SyncStatus::TargetTypeChanged),
        "failed" => Ok(SyncStatus::Failed),
        _ => Err(AppError::invalid_input(
            "syncStatus",
            "数据库包含未知同步状态",
        )),
    }
}

fn parse_error_code(value: &str) -> Result<ErrorCode, AppError> {
    ErrorCode::from_stable_str(value)
        .ok_or_else(|| AppError::invalid_input("errorCode", "数据库包含未知错误码"))
}

#[cfg(test)]
include!("tests.rs");
