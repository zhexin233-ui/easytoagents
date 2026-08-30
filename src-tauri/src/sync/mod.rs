//! 只读发现、双 hash 漂移分类、Preview 持久化与受控写入编排。

mod apply;

pub use apply::{
    apply_persisted_preview, detect_interrupted_run, list_snapshots, preview_restore,
    restore_snapshot, ApplyFaultDecision, ApplyFaultEvent, ApplyFaultInjector, ApplyResult,
    ApplyTargetInput, InterruptedRunPlan, ManagedItemApply, NoApplyFault, RestorePreview,
    SnapshotSummary,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Component, Path, PathBuf},
};

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
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
pub const ERROR_EXTERNAL_OWNED_CHANGE: &str = "EXTERNAL_OWNED_CHANGE";
pub const ERROR_MANAGED_ITEM_BASELINE_MISMATCH: &str = "MANAGED_ITEM_BASELINE_MISMATCH";
pub const ERROR_TARGET_TYPE_CHANGED: &str = "TARGET_TYPE_CHANGED";
pub const ERROR_CLAUDE_POLICY_UNKNOWN: &str = "CLAUDE_POLICY_UNKNOWN";
pub const ERROR_CODEX_TRUST_UNKNOWN: &str = "CODEX_TRUST_UNKNOWN";
pub const ERROR_INCOMPLETE_BASELINE: &str = "INCOMPLETE_MANAGED_BASELINE";
pub const WARNING_CODEX_PROMPT_OVERRIDE: &str = "CODEX_PROMPT_OVERRIDE_DETECTED";
pub const WARNING_CODEX_PROMPT_OVERRIDE_UNKNOWN: &str = "CODEX_PROMPT_OVERRIDE_UNKNOWN";

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
        Ok(projection) => canonical_json(&projection),
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
        let name = child
            .file_name()
            .into_string()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "目录项名称不是 UTF-8"))?;
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

pub fn hash_json(value: &Value) -> String {
    hash_bytes(
        &serde_json::to_vec(&canonical_json(value)).expect("serde_json::Value 必须始终可以序列化"),
    )
}

pub(crate) fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect::<Map<_, _>>(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
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
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT row_version, baseline_full_hash, baseline_managed_hash
             FROM managed_targets WHERE id = ?1",
            [target_id],
            |row| {
                Ok(ManagedTargetBaseline {
                    target_id: target_id.to_owned(),
                    target_row_version: row.get(0)?,
                    full_hash: row.get(1)?,
                    managed_hash: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|_| AppError::database(&path, "load_managed_target_baseline"))?
        .ok_or_else(|| AppError::not_found("managedTarget", target_id))
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
    Project,
    ManagedTarget,
    ManagedItem,
}

impl DatabaseEntityType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderProfile => "provider_profile",
            Self::PromptProfile => "prompt_profile",
            Self::McpServer => "mcp_server",
            Self::Skill => "skill",
            Self::Project => "project",
            Self::ManagedTarget => "managed_target",
            Self::ManagedItem => "managed_item",
        }
    }

    const fn table(self) -> &'static str {
        match self {
            Self::ProviderProfile => "provider_profiles",
            Self::PromptProfile => "prompt_profiles",
            Self::McpServer => "mcp_servers",
            Self::Skill => "skills",
            Self::Project => "projects",
            Self::ManagedTarget => "managed_targets",
            Self::ManagedItem => "managed_items",
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
        let assessment = assess_drift(&request.descriptor, &request.baseline, &request.scan);
        let (current_full_hash, current_managed_hash, before_projection) = match &request.scan {
            TargetScan::Observed(observed) => (
                Some(observed.full_hash.clone()),
                Some(observed.managed_hash.clone()),
                observed.managed_projection.clone(),
            ),
            _ => (None, None, Value::Null),
        };
        let desired_projection = canonical_json(&request.desired_projection);
        let desired_hash = hash_json(&desired_projection);
        let current_matches_desired = current_managed_hash.as_ref() == Some(&desired_hash);

        let (mut change_kind, error_code) = if !assessment.can_merge {
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
            u32::try_from(request.baseline.target_row_version).map_err(|_| {
                AppError::invalid_input("rowVersion", "数据库 row_version 超出 RPC 安全范围")
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
    let digest =
        Sha256::digest(serde_json::to_vec(&normalized).expect("row version 快照必须可以序列化"));
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
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_preview"))?;
    verify_preview_row_versions(&transaction, plan, &database_path)?;
    transaction
        .execute(
            "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version)
             VALUES (?1, 'preview', 'previewed', ?2, ?3, ?4)",
            params![
                plan.preview_id,
                plan.scope.as_str(),
                plan.project_id,
                plan.db_version
            ],
        )
        .map_err(|_| AppError::database(&database_path, "insert_preview_run"))?;

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
        };
        let envelope_json = serde_json::to_string(&envelope)
            .map_err(|_| AppError::database(&database_path, "serialize_preview_item"))?;
        let warning_codes_json = serde_json::to_string(&target.warning_codes)
            .map_err(|_| AppError::database(&database_path, "serialize_warning_codes"))?;
        transaction
            .execute(
                "INSERT INTO sync_items(
                    id, run_id, target_id, change_kind, status,
                    redacted_diff_json, warning_codes_json, error_code, target_order
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    Uuid::new_v4().to_string(),
                    plan.preview_id,
                    target.target_id,
                    target.change_kind.as_str(),
                    target.status.as_str(),
                    envelope_json,
                    warning_codes_json,
                    target.error_code.map(ErrorCode::as_str),
                    target_order,
                ],
            )
            .map_err(|_| AppError::database(&database_path, "insert_preview_item"))?;
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_preview"))
}

fn verify_preview_row_versions(
    transaction: &Transaction<'_>,
    plan: &PreviewPlan,
    database_path: &str,
) -> Result<(), AppError> {
    let mut expected = BTreeMap::new();
    for target in &plan.targets {
        let identity = transaction
            .query_row(
                "SELECT target.row_version, target.tool, target.artifact_kind, target.scope,
                        target.project_id, target.target_path, project.root_path
                 FROM managed_targets AS target
                 LEFT JOIN projects AS project ON project.id = target.project_id
                 WHERE target.id = ?1",
                [&target.target_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| AppError::database(database_path, "verify_preview_target_identity"))?
            .ok_or_else(|| AppError::stale_preview(&plan.preview_id, &target.target_id))?;
        if u32::try_from(identity.0).ok() != Some(target.target_row_version) {
            return Err(AppError::stale_preview(&plan.preview_id, &target.target_id));
        }
        if identity.1 != target.descriptor.tool.as_str()
            || identity.2 != target.descriptor.artifact_kind.as_str()
            || identity.3 != target.descriptor.scope.as_str()
            || identity.4 != plan.project_id
            || target.descriptor.path.as_deref() != Some(identity.5.as_str())
            || target.descriptor.project_root != identity.6
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
        let query = format!(
            "SELECT row_version FROM {} WHERE id = ?1",
            entity_type.table()
        );
        let actual = transaction
            .query_row(&query, [&entity_id], |row| row.get::<_, i64>(0))
            .optional()
            .map_err(|_| AppError::database(database_path, "verify_preview_row_version"))?;
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
    let path = database.path().to_string_lossy();
    let run = database
        .connection()
        .query_row(
            "SELECT scope, project_id, db_version
             FROM sync_runs WHERE id = ?1",
            [preview_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(&path, "load_preview_run"))?
        .ok_or_else(|| AppError::not_found("preview", preview_id))?;

    let mut statement = database
        .connection()
        .prepare(
            "SELECT item.target_id, target.target_path, item.change_kind, item.status,
                    item.redacted_diff_json, item.warning_codes_json, item.error_code
             FROM sync_items AS item
             JOIN managed_targets AS target ON target.id = item.target_id
             WHERE item.run_id = ?1 ORDER BY item.target_order, item.id",
        )
        .map_err(|_| AppError::database(&path, "prepare_preview_items"))?;
    let rows = statement
        .query_map([preview_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|_| AppError::database(&path, "query_preview_items"))?;
    let mut items = Vec::new();
    for row in rows {
        let (target_id, target_path, change_kind, status, envelope, warnings, error_code) =
            row.map_err(|_| AppError::database(&path, "read_preview_item"))?;
        items.push(PersistedPreviewItem {
            target_id,
            target_path,
            change_kind: parse_change_kind(&change_kind)?,
            status: parse_sync_status(&status)?,
            envelope: serde_json::from_str(&envelope)
                .map_err(|_| AppError::database(&path, "parse_preview_envelope"))?,
            warning_codes: serde_json::from_str(&warnings)
                .map_err(|_| AppError::database(&path, "parse_warning_codes"))?,
            error_code: error_code.as_deref().map(parse_error_code).transpose()?,
        });
    }

    Ok(PersistedPreview {
        preview_id: preview_id.to_owned(),
        scope: parse_scope(&run.0)?,
        project_id: run.1,
        db_version: run.2,
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
mod tests {
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        path::PathBuf,
    };

    use rusqlite::params;
    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        assess_drift, build_preview_plan, load_managed_target_baseline, load_persisted_preview,
        persist_preview, scan_target, DatabaseEntityType, DatabaseRowVersion,
        ManagedTargetBaseline, PreviewTargetRequest, TargetScan, ERROR_EXTERNAL_OWNED_CHANGE,
        ERROR_INCOMPLETE_BASELINE, WARNING_CODEX_PROMPT_OVERRIDE,
        WARNING_EXTERNAL_NON_OWNED_CHANGE,
    };
    use crate::{
        adapters::{
            canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
            ConservativeClaudeCustomizationPolicyProbe, ConservativeClaudeUserMcpProbe,
            DiscoveryContext, ExplicitEnvironment, ManagedOwnership, PolicyState, TargetTrustState,
            ToolAdapter, ToolAvailability,
        },
        app::AppPaths,
        db::Database,
        domain::{ArtifactKind, ChangeKind, Scope, SyncStatus},
        security::SecretRedactor,
    };

    const TARGET_ID: &str = "10000000-0000-4000-8000-000000000001";

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/phase2")
            .join(name)
    }

    fn isolated_environment(home: &std::path::Path) -> ExplicitEnvironment {
        ExplicitEnvironment::new(home, None, None, ToolAvailability::all_installed())
            .unwrap()
            .with_claude_provider_policy(PolicyState::Allowed)
    }

    fn claude_targets(environment: &ExplicitEnvironment) -> Vec<crate::adapters::TargetDescriptor> {
        let probe = ConservativeClaudeUserMcpProbe;
        ClaudeAdapter
            .discover(&DiscoveryContext {
                environment,
                project_root: None,
                claude_user_mcp_probe: &probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
    }

    #[test]
    fn all_file_fixtures_parse_only_inside_an_explicit_isolated_matrix() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project_path = home.join("project");
        fs::create_dir(&project_path).unwrap();
        let project = canonicalize_project_root(&project_path).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        fs::create_dir_all(environment.codex_home()).unwrap();
        fs::create_dir_all(project_path.join(".codex")).unwrap();
        fs::copy(
            fixture("claude-settings.json"),
            environment.claude_config_dir().join("settings.json"),
        )
        .unwrap();
        fs::copy(fixture("claude-user-mcp.json"), home.join(".claude.json")).unwrap();
        fs::copy(
            fixture("claude-project-mcp.json"),
            project_path.join(".mcp.json"),
        )
        .unwrap();
        fs::copy(
            fixture("claude-prompt.md"),
            environment.claude_config_dir().join("CLAUDE.md"),
        )
        .unwrap();
        let codex_config = fs::read_to_string(fixture("codex-config.toml"))
            .unwrap()
            .replace("/fixture/project", project.as_str());
        fs::write(environment.codex_home().join("config.toml"), codex_config).unwrap();
        fs::copy(
            fixture("codex-project-config.toml"),
            project_path.join(".codex/config.toml"),
        )
        .unwrap();
        fs::copy(
            fixture("codex-prompt.md"),
            environment.codex_home().join("AGENTS.md"),
        )
        .unwrap();

        let probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let claude = ClaudeAdapter.discover(&context).unwrap();
        let codex = CodexAdapter.discover(&context).unwrap();
        let cases: Vec<(
            &dyn ToolAdapter,
            &crate::adapters::TargetDescriptor,
            ManagedOwnership,
        )> = vec![
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| target.artifact_kind == ArtifactKind::Provider)
                    .unwrap(),
                ManagedOwnership::selectors([vec!["env", "ANTHROPIC_API_KEY"]]),
            ),
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcpServers", "fixture-user"]]),
            ),
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcpServers", "fixture-project"]]),
            ),
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                    .unwrap(),
                ManagedOwnership::WholeDocument,
            ),
            (
                &CodexAdapter,
                codex
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcp_servers", "fixture_user"]]),
            ),
            (
                &CodexAdapter,
                codex
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcp_servers", "fixture_project"]]),
            ),
            (
                &CodexAdapter,
                codex
                    .iter()
                    .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                    .unwrap(),
                ManagedOwnership::WholeDocument,
            ),
        ];
        for (adapter, target, ownership) in cases {
            assert!(matches!(
                scan_target(adapter, target, &ownership),
                TargetScan::Observed(_)
            ));
            assert!(target.path().unwrap().starts_with(&home));
        }
    }

    #[test]
    fn scanner_distinguishes_missing_empty_corrupt_permission_and_target_type() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        let targets = claude_targets(&environment);
        let mut settings = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap()
            .clone();
        let prompt = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        assert!(matches!(
            scan_target(&ClaudeAdapter, prompt, &ManagedOwnership::WholeDocument),
            TargetScan::Missing
        ));

        let empty = home.join("empty.json");
        fs::write(&empty, b"").unwrap();
        settings.path = Some(empty.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &settings,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::ParseError
        ));

        let corrupt = home.join("corrupt.json");
        fs::write(&corrupt, b"{broken").unwrap();
        settings.path = Some(corrupt.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &settings,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::ParseError
        ));

        let unreadable = home.join("unreadable.json");
        fs::copy(fixture("claude-settings.json"), &unreadable).unwrap();
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        settings.path = Some(unreadable.to_str().unwrap().to_owned());
        let unreadable_scan = scan_target(
            &ClaudeAdapter,
            &settings,
            &ManagedOwnership::selectors([vec!["env"]]),
        );
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(unreadable_scan, TargetScan::PermissionDenied));

        let directory = home.join("settings-directory");
        fs::create_dir(&directory).unwrap();
        settings.path = Some(directory.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &settings,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::TargetTypeChanged(_)
        ));
    }

    #[test]
    fn scanner_rejects_late_symlink_ancestors_and_invalid_managed_shapes() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let late_claude_root = home.join("late-claude-root");
        let environment = ExplicitEnvironment::new(
            &home,
            Some(late_claude_root.clone()),
            None,
            ToolAvailability::all_installed(),
        )
        .unwrap();
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        let outside = home.join("outside-config");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("settings.json"), "{\"env\":{}}\n").unwrap();
        symlink(&outside, &late_claude_root).unwrap();
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &descriptor,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::TargetTypeChanged(crate::domain::TargetType::Symlink)
        ));

        let safe_path = home.join("invalid-managed-shape.json");
        fs::write(&safe_path, "{\"env\":\"external scalar\"}\n").unwrap();
        let mut safe_descriptor = descriptor;
        safe_descriptor.path = Some(safe_path.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &safe_descriptor,
                &ManagedOwnership::selectors([vec!["env", "ANTHROPIC_API_KEY"]])
            ),
            TargetScan::ParseError
        ));
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &safe_descriptor,
                &ManagedOwnership::selectors([vec!["permissions"]])
            ),
            TargetScan::Failed
        ));
    }

    #[test]
    fn scanner_rejects_corrupt_toml_and_accepts_empty_markdown() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.codex_home()).unwrap();
        let probe = ConservativeClaudeUserMcpProbe;
        let targets = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &environment,
                project_root: None,
                claude_user_mcp_probe: &probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap();
        let config = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        fs::write(config.path().unwrap(), "[broken\n").unwrap();
        assert!(matches!(
            scan_target(
                &CodexAdapter,
                config,
                &ManagedOwnership::selectors([vec!["model"]])
            ),
            TargetScan::ParseError
        ));

        let prompt = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        fs::write(prompt.path().unwrap(), b"").unwrap();
        assert!(matches!(
            scan_target(&CodexAdapter, prompt, &ManagedOwnership::WholeDocument),
            TargetScan::Observed(_)
        ));
    }

    #[test]
    fn skills_fixture_is_read_as_links_without_following_or_writing_targets() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        let targets = claude_targets(&environment);
        let skills = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Skill)
            .unwrap();
        fs::create_dir_all(skills.path().unwrap()).unwrap();
        let source = fixture("skills/fixture-skill");
        symlink(&source, skills.path().unwrap().join("fixture-skill")).unwrap();
        fs::create_dir(skills.path().unwrap().join("external-directory")).unwrap();

        let scan = scan_target(
            &ClaudeAdapter,
            skills,
            &ManagedOwnership::SymlinkNames(vec!["fixture-skill".to_owned()]),
        );
        let TargetScan::Observed(observed) = scan else {
            panic!("Skills 目录必须被成功读取");
        };
        assert_eq!(
            observed.managed_projection["fixture-skill"]["targetType"],
            "symlink"
        );
        assert_eq!(
            observed.managed_projection["fixture-skill"]["linkTarget"],
            source.to_str().unwrap()
        );
        assert!(source.join("SKILL.md").is_file());
    }

    #[test]
    fn dual_hash_allows_only_non_owned_changes_to_merge() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        fs::copy(fixture("claude-settings.json"), descriptor.path().unwrap()).unwrap();
        let ownership = ManagedOwnership::selectors([
            vec!["env", "ANTHROPIC_BASE_URL"],
            vec!["env", "ANTHROPIC_API_KEY"],
        ]);
        let first = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let TargetScan::Observed(first_observed) = &first else {
            panic!("fixture 必须能解析");
        };
        let baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 1,
            full_hash: Some(first_observed.full_hash.clone()),
            managed_hash: Some(first_observed.managed_hash.clone()),
        };
        assert_eq!(
            assess_drift(&descriptor, &baseline, &first).status,
            SyncStatus::InSync
        );

        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(descriptor.path().unwrap()).unwrap()).unwrap();
        document["permissions"]["allow"] = json!(["Read", "Glob"]);
        fs::write(
            descriptor.path().unwrap(),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        let non_owned = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let non_owned_assessment = assess_drift(&descriptor, &baseline, &non_owned);
        assert_eq!(
            non_owned_assessment.status,
            SyncStatus::ExternalNonOwnedChange
        );
        assert!(non_owned_assessment.can_merge);
        assert!(non_owned_assessment
            .diagnostic_codes
            .contains(&WARNING_EXTERNAL_NON_OWNED_CHANGE.to_owned()));

        document["env"]["ANTHROPIC_BASE_URL"] = json!("https://external.invalid");
        fs::write(
            descriptor.path().unwrap(),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        let owned = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let owned_assessment = assess_drift(&descriptor, &baseline, &owned);
        assert_eq!(owned_assessment.status, SyncStatus::ExternalOwnedChange);
        assert!(!owned_assessment.can_merge);
        assert!(owned_assessment
            .diagnostic_codes
            .contains(&ERROR_EXTERNAL_OWNED_CHANGE.to_owned()));
    }

    #[test]
    fn policy_blocked_and_untrusted_targets_never_merge() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        let mut descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        descriptor.policy = PolicyState::Blocked;
        let baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 1,
            full_hash: None,
            managed_hash: None,
        };
        let blocked = assess_drift(&descriptor, &baseline, &TargetScan::Missing);
        assert_eq!(blocked.status, SyncStatus::PolicyBlocked);
        assert!(!blocked.can_merge);

        descriptor.policy = PolicyState::Allowed;
        descriptor.trust = TargetTrustState::Untrusted;
        let untrusted = assess_drift(&descriptor, &baseline, &TargetScan::Missing);
        assert_eq!(untrusted.status, SyncStatus::Untrusted);
        assert!(!untrusted.can_merge);

        descriptor.trust = TargetTrustState::NotRequired;
        let incomplete_baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 1,
            full_hash: Some("a".repeat(64)),
            managed_hash: None,
        };
        let incomplete = assess_drift(&descriptor, &incomplete_baseline, &TargetScan::Missing);
        assert_eq!(incomplete.status, SyncStatus::ExternalOwnedChange);
        assert!(!incomplete.can_merge);
        assert_eq!(
            incomplete.diagnostic_codes,
            vec![ERROR_INCOMPLETE_BASELINE.to_owned()]
        );
    }

    #[test]
    fn codex_prompt_override_turns_an_unchanged_preview_into_a_warning() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.codex_home()).unwrap();
        fs::write(environment.codex_home().join("AGENTS.md"), "fixture prompt").unwrap();
        fs::write(
            environment.codex_home().join("AGENTS.override.md"),
            "fixture override",
        )
        .unwrap();
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let descriptor = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        let scan = scan_target(&CodexAdapter, &descriptor, &ManagedOwnership::WholeDocument);
        let TargetScan::Observed(observed) = &scan else {
            panic!("Codex prompt fixture 必须可读取");
        };
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                descriptor,
                ownership: ManagedOwnership::WholeDocument,
                baseline: ManagedTargetBaseline {
                    target_id: TARGET_ID.to_owned(),
                    target_row_version: 1,
                    full_hash: Some(observed.full_hash.clone()),
                    managed_hash: Some(observed.managed_hash.clone()),
                },
                scan,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: json!("fixture prompt"),
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
            }],
            &SecretRedactor::default(),
        )
        .unwrap();
        assert_eq!(plan.targets[0].status, SyncStatus::InSync);
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Warning);
        assert!(plan.targets[0]
            .warning_codes
            .contains(&WARNING_CODEX_PROMPT_OVERRIDE.to_owned()));
    }

    #[test]
    fn preview_rejects_conflicting_versions_for_the_same_database_entity() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        let error = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                descriptor,
                ownership: ManagedOwnership::WholeDocument,
                baseline: ManagedTargetBaseline {
                    target_id: TARGET_ID.to_owned(),
                    target_row_version: 1,
                    full_hash: None,
                    managed_hash: None,
                },
                scan: TargetScan::Missing,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: json!("fixture prompt"),
                row_versions: vec![DatabaseRowVersion {
                    entity_type: DatabaseEntityType::ManagedTarget,
                    entity_id: TARGET_ID.to_owned(),
                    row_version: 2,
                }],
                git: None,
                exclude_from_git: false,
            }],
            &SecretRedactor::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn codex_http_header_fixture_is_redacted_by_the_descriptor_selector() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.codex_home()).unwrap();
        fs::copy(
            fixture("codex-config.toml"),
            environment.codex_home().join("config.toml"),
        )
        .unwrap();
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let descriptor = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        let ownership = ManagedOwnership::selectors([vec!["mcp_servers", "fixture_user"]]);
        let scan = scan_target(&CodexAdapter, &descriptor, &ownership);
        let TargetScan::Observed(observed) = &scan else {
            panic!("Codex MCP fixture 必须可读取");
        };
        let desired_projection = observed.managed_projection.clone();
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                descriptor,
                ownership,
                baseline: ManagedTargetBaseline {
                    target_id: TARGET_ID.to_owned(),
                    target_row_version: 1,
                    full_hash: Some(observed.full_hash.clone()),
                    managed_hash: Some(observed.managed_hash.clone()),
                },
                scan,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection,
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
            }],
            &SecretRedactor::default(),
        )
        .unwrap();
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("fixture-codex-user-mcp-secret"));
    }

    #[test]
    fn preview_persists_row_versions_and_never_serializes_fixture_secrets() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let home = isolated_root.join("home");
        fs::create_dir(&home).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        let mut descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        descriptor.managed_selector_roots.push("custom".to_owned());
        descriptor
            .sensitive_selectors
            .push("custom/value".to_owned());
        fs::copy(fixture("claude-settings.json"), descriptor.path().unwrap()).unwrap();
        let ownership = ManagedOwnership::selectors([
            vec!["env", "ANTHROPIC_BASE_URL"],
            vec!["env", "ANTHROPIC_API_KEY"],
        ]);
        let scan = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let TargetScan::Observed(observed) = &scan else {
            panic!("fixture 必须能解析");
        };
        let baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 7,
            full_hash: Some(observed.full_hash.clone()),
            managed_hash: Some(observed.managed_hash.clone()),
        };
        let fixture_secrets = [
            "fixture-claude-provider-secret",
            "fixture-claude-policy-secret",
            "fixture-claude-user-mcp-secret",
            "fixture-claude-project-mcp-secret",
            "fixture-codex-provider-secret",
            "fixture-codex-user-mcp-secret",
            "fixture-codex-project-mcp-secret",
            "fixture-claude-prompt-secret",
            "fixture-codex-prompt-secret",
            "fixture-preview-next-secret",
        ];
        let mut redactor = SecretRedactor::default();
        for secret in fixture_secrets {
            redactor.register_secret(secret);
        }
        let desired = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://replacement.invalid",
                "ANTHROPIC_API_KEY": "fixture-preview-next-secret"
            },
            "custom": {
                "value": "fixture-selector-only-value"
            }
        });
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                descriptor: descriptor.clone(),
                ownership,
                baseline,
                scan,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: desired,
                row_versions: vec![DatabaseRowVersion {
                    entity_type: DatabaseEntityType::ProviderProfile,
                    entity_id: "20000000-0000-4000-8000-000000000002".to_owned(),
                    row_version: 11,
                }],
                git: None,
                exclude_from_git: false,
            }],
            &redactor,
        )
        .unwrap();
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Update);
        let serialized = serde_json::to_string(&plan).unwrap();
        for secret in fixture_secrets {
            assert!(
                !serialized.contains(secret),
                "Preview 泄漏 fixture secret: {secret}"
            );
        }
        assert!(!serialized.contains("fixture-selector-only-value"));

        let app_paths = AppPaths::from_data_root(isolated_root.join("app-data")).unwrap();
        let mut database = Database::open(&app_paths).unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json,
                    last_status, row_version
                 ) VALUES (?1, 'claude', 'provider', 'global', ?2, ?3, ?4, '{}', 'in_sync', 7)",
                params![
                    TARGET_ID,
                    descriptor.path.as_deref().unwrap(),
                    plan.targets[0].current_full_hash,
                    plan.targets[0].current_managed_hash,
                ],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO provider_profiles(id, tool, name, row_version)
                 VALUES (?1, 'claude', 'Fixture Provider', 11)",
                ["20000000-0000-4000-8000-000000000002"],
            )
            .unwrap();
        let stored_baseline = load_managed_target_baseline(&database, TARGET_ID).unwrap();
        assert_eq!(stored_baseline.target_row_version, 7);
        assert_eq!(stored_baseline.full_hash, plan.targets[0].current_full_hash);
        let mut mismatched_plan = plan.clone();
        mismatched_plan.preview_id = "30000000-0000-4000-8000-000000000003".to_owned();
        mismatched_plan.targets[0].descriptor.path = Some(
            home.join("wrong-settings.json")
                .to_str()
                .unwrap()
                .to_owned(),
        );
        let mismatch_error = persist_preview(&mut database, &mismatched_plan).unwrap_err();
        assert_eq!(mismatch_error.code(), crate::error::ErrorCode::InvalidInput);
        persist_preview(&mut database, &plan).unwrap();
        let persisted = load_persisted_preview(&database, &plan.preview_id).unwrap();
        assert_eq!(persisted.db_version, i64::from(plan.db_version));
        assert_eq!(persisted.items[0].envelope.target_row_version, 7);
        assert_eq!(persisted.items[0].envelope.row_versions[0].row_version, 11);
        let persisted_json: String = database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&plan.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        for secret in fixture_secrets {
            assert!(
                !persisted_json.contains(secret),
                "持久化 Preview 泄漏 fixture secret: {secret}"
            );
        }
        assert!(!persisted_json.contains("fixture-selector-only-value"));
    }

    #[test]
    fn preview_persistence_rejects_changed_database_row_versions() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let environment = isolated_environment(&home);
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        let paths = AppPaths::from_data_root(root.join("app-data")).unwrap();
        let mut database = Database::open(&paths).unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                 VALUES (?1, 'claude', 'prompt', 'global', ?2)",
                params![TARGET_ID, descriptor.path.as_deref().unwrap()],
            )
            .unwrap();
        let baseline = load_managed_target_baseline(&database, TARGET_ID).unwrap();
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                descriptor,
                ownership: ManagedOwnership::WholeDocument,
                baseline,
                scan: TargetScan::Missing,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: json!("fixture prompt"),
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
            }],
            &SecretRedactor::default(),
        )
        .unwrap();
        database
            .connection()
            .execute(
                "UPDATE managed_targets SET last_status = 'failed' WHERE id = ?1",
                [TARGET_ID],
            )
            .unwrap();

        let error = persist_preview(&mut database, &plan).unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::StalePreview);
        let count: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sync_runs WHERE id = ?1",
                [&plan.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "过期 Preview 不得留下半持久化 run");
    }

    #[test]
    fn project_fixture_is_always_under_explicit_temp_root() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        let canonical = canonicalize_project_root(&project).unwrap();
        assert!(canonical.as_str().starts_with(root.to_str().unwrap()));
    }
}
