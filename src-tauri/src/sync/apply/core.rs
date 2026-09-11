use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CString, OsStr},
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use uuid::Uuid;

use super::{
    hash_bytes, hash_json, load_persisted_preview, scan_target, DatabaseEntityType,
    DatabaseRowVersion, PersistedPreview, PersistedPreviewEnvelope, PersistedPreviewItem,
    SkillTakeoverEntryType, TargetScan,
};
use crate::{
    adapters::{ManagedOwnership, RenderedTarget, TargetDescriptor, TargetFormat},
    app::AppPaths,
    db::{sync::active_writer, Database},
    domain::{ArtifactKind, ChangeKind, ProjectRoot, Scope, TargetType},
    error::{AppError, ErrorCode, RecoveryAction},
    git::{inspect_path, render_local_exclude, resolve_local_exclude},
    security::{
        create_private_file, ensure_private_directory, ensure_private_file, PRIVATE_FILE_MODE,
    },
    skills::library::{self as skill_library, SkillTakeoverEntryKind},
};

#[derive(Debug, Clone)]
pub struct ApplyTargetInput {
    pub descriptor: TargetDescriptor,
    pub ownership: ManagedOwnership,
    pub desired_projection: Value,
    /// 已存在且已 canonicalize 的隔离写入根；每次写前都会重新复核。
    pub allowed_root: PathBuf,
    /// Skills 链接目标必须位于这个中央库内；其他格式保持 None。
    pub central_skills_root: Option<PathBuf>,
    /// Whole-document 删除必须由上层明确声明，不能仅凭空投影猜测。
    pub delete_target: bool,
    /// 成功事务中同步更新的 managed item 基线；外部写入失败时不会提交。
    pub managed_items: Vec<ManagedItemApply>,
    /// 只有 Preview 绑定了对应 ManagedItem row_version 时才允许删除。
    pub remove_managed_item_ids: Vec<String>,
    /// 只接受持久化 Preview 中绑定的首次 Skills 接管证据。
    pub skill_takeover_entries: Vec<super::SkillTakeoverEntry>,
    /// 只接受持久化 Preview 中绑定的项目原生资源禁用/恢复证据。
    pub project_native_action: Option<super::ProjectNativeResourceEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedItemApply {
    pub id: String,
    pub resource_kind: ArtifactKind,
    pub resource_id: String,
    pub external_key: String,
    pub last_applied_item_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub run_id: String,
    pub status: String,
    pub applied_targets: u32,
    pub snapshot_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyFaultEvent {
    BeforeTarget { index: usize, path: PathBuf },
    BeforeRename { index: usize, path: PathBuf },
    AfterRename { index: usize, path: PathBuf },
    AfterTarget { index: usize, path: PathBuf },
    BeforeDatabaseFinalize,
    AfterDatabaseFinalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyFaultDecision {
    Continue,
    Fail,
    Crash,
}

pub trait ApplyFaultInjector: Send + Sync {
    fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision;
}

#[derive(Debug, Default)]
pub struct NoApplyFault;

impl ApplyFaultInjector for NoApplyFault {
    fn decide(&self, _event: &ApplyFaultEvent) -> ApplyFaultDecision {
        ApplyFaultDecision::Continue
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    pub snapshot_id: String,
    pub run_id: String,
    pub target_id: Option<String>,
    pub target_path: String,
    pub target_type: TargetType,
    pub storage_kind: SnapshotStorageKind,
    pub restorable: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotStorageKind {
    PayloadFile,
    MetadataOnly,
    DirectoryTree,
}

impl SnapshotStorageKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PayloadFile => "payload_file",
            Self::MetadataOnly => "metadata_only",
            Self::DirectoryTree => "directory_tree",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSnapshotsInput {
    pub snapshot_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDeleteFailureDto {
    pub snapshot_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSnapshotsResultDto {
    pub deleted_ids: Vec<String>,
    pub failures: Vec<SnapshotDeleteFailureDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InterruptedTargetPlan {
    pub target_id: String,
    pub target_path: String,
    pub snapshot_id: Option<String>,
    pub phase: String,
    pub current_type: Option<TargetType>,
    pub current_fingerprint: Option<String>,
    pub error_code: Option<ErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InterruptedRunPlan {
    pub run_id: String,
    pub status: String,
    pub journal_available: bool,
    pub targets: Vec<InterruptedTargetPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub preview_id: String,
    pub snapshot_id: String,
    pub target_path: String,
    pub current_type: TargetType,
    pub snapshot_type: TargetType,
    pub storage_kind: SnapshotStorageKind,
}

/// journal 中记录的阶段。序列化字符串与历史 journal 逐一相同；旧文件里未知的
/// 字符串落到 `Unknown`，并按"可能已改变目标"的保守口径处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetPhase {
    Applying,
    Claimed,
    CrashedAfterDatabaseFinalize,
    CrashedAfterRename,
    CrashedAfterRestoreTree,
    CrashedAfterTakeover,
    CrashedAfterTarget,
    CrashedBeforeDatabaseFinalize,
    CrashedBeforeNativeLink,
    CrashedBeforeNativeRemove,
    CrashedBeforeRename,
    CrashedBeforeRestoreTree,
    CrashedBeforeTakeover,
    CrashedBeforeTarget,
    CrashedDuringDatabaseFinalize,
    DirectoryCreateFailed,
    DirectoryCreatePending,
    DirectoryCreated,
    DirectoryRestorePending,
    DirectoryRestored,
    ExternalChangeAfterWrite,
    NativeLinkPending,
    ReadyToFinalizeDatabase,
    Removed,
    RenameFailed,
    RenamePending,
    Renamed,
    RollbackFailed,
    RolledBack,
    RollingBack,
    Snapshotted,
    Snapshotting,
    Succeeded,
    TakeoverLinkFailed,
    TakeoverLinked,
    TakeoverQuarantined,
    TakeoverRenameFailed,
    TakeoverRenamePending,
    Verified,
    Writing,
    Written,
    #[serde(other)]
    Unknown,
}

/// 兼容早期内部名称；新代码应使用更准确的 `TargetPhase`。
#[allow(dead_code)]
pub type JournalPhase = TargetPhase;

impl TargetPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applying => "applying",
            Self::Claimed => "claimed",
            Self::CrashedAfterDatabaseFinalize => "crashed_after_database_finalize",
            Self::CrashedAfterRename => "crashed_after_rename",
            Self::CrashedAfterRestoreTree => "crashed_after_restore_tree",
            Self::CrashedAfterTakeover => "crashed_after_takeover",
            Self::CrashedAfterTarget => "crashed_after_target",
            Self::CrashedBeforeDatabaseFinalize => "crashed_before_database_finalize",
            Self::CrashedBeforeNativeLink => "crashed_before_native_link",
            Self::CrashedBeforeNativeRemove => "crashed_before_native_remove",
            Self::CrashedBeforeRename => "crashed_before_rename",
            Self::CrashedBeforeRestoreTree => "crashed_before_restore_tree",
            Self::CrashedBeforeTakeover => "crashed_before_takeover",
            Self::CrashedBeforeTarget => "crashed_before_target",
            Self::CrashedDuringDatabaseFinalize => "crashed_during_database_finalize",
            Self::DirectoryCreateFailed => "directory_create_failed",
            Self::DirectoryCreatePending => "directory_create_pending",
            Self::DirectoryCreated => "directory_created",
            Self::DirectoryRestorePending => "directory_restore_pending",
            Self::DirectoryRestored => "directory_restored",
            Self::ExternalChangeAfterWrite => "external_change_after_write",
            Self::NativeLinkPending => "native_link_pending",
            Self::ReadyToFinalizeDatabase => "ready_to_finalize_database",
            Self::Removed => "removed",
            Self::RenameFailed => "rename_failed",
            Self::RenamePending => "rename_pending",
            Self::Renamed => "renamed",
            Self::RollbackFailed => "rollback_failed",
            Self::RolledBack => "rolled_back",
            Self::RollingBack => "rolling_back",
            Self::Snapshotted => "snapshotted",
            Self::Snapshotting => "snapshotting",
            Self::Succeeded => "succeeded",
            Self::TakeoverLinkFailed => "takeover_link_failed",
            Self::TakeoverLinked => "takeover_linked",
            Self::TakeoverQuarantined => "takeover_quarantined",
            Self::TakeoverRenameFailed => "takeover_rename_failed",
            Self::TakeoverRenamePending => "takeover_rename_pending",
            Self::Verified => "verified",
            Self::Writing => "writing",
            Self::Written => "written",
            Self::Unknown => "unknown",
        }
    }

    /// 该阶段是否意味着目标可能已被本次 run 改动（回滚判定）。无通配分支：
    /// 新增阶段时编译器强制在这里表态。
    pub const fn may_have_changed_target(self) -> bool {
        match self {
            Self::CrashedAfterRename => true,
            Self::CrashedAfterRestoreTree => true,
            Self::CrashedAfterTakeover => true,
            Self::CrashedAfterTarget => true,
            Self::DirectoryCreatePending => true,
            Self::DirectoryCreated => true,
            Self::DirectoryRestored => true,
            Self::Removed => true,
            Self::Renamed => true,
            Self::TakeoverLinked => true,
            Self::TakeoverQuarantined => true,
            Self::Written => true,
            Self::Applying => false,
            Self::Claimed => false,
            Self::CrashedAfterDatabaseFinalize => false,
            Self::CrashedBeforeDatabaseFinalize => false,
            Self::CrashedBeforeNativeLink => false,
            Self::CrashedBeforeNativeRemove => false,
            Self::CrashedBeforeRename => false,
            Self::CrashedBeforeRestoreTree => false,
            Self::CrashedBeforeTakeover => false,
            Self::CrashedBeforeTarget => false,
            Self::CrashedDuringDatabaseFinalize => false,
            Self::DirectoryCreateFailed => false,
            Self::DirectoryRestorePending => false,
            Self::ExternalChangeAfterWrite => false,
            Self::NativeLinkPending => false,
            Self::ReadyToFinalizeDatabase => false,
            Self::RenameFailed => false,
            Self::RenamePending => false,
            Self::RollbackFailed => false,
            Self::RolledBack => false,
            Self::RollingBack => false,
            Self::Snapshotted => false,
            Self::Snapshotting => false,
            Self::Succeeded => false,
            Self::TakeoverLinkFailed => false,
            Self::TakeoverRenameFailed => false,
            Self::TakeoverRenamePending => false,
            Self::Verified => false,
            Self::Writing => false,
            Self::Unknown => true,
        }
    }

    pub const fn is_crashed(self) -> bool {
        match self {
            Self::CrashedAfterDatabaseFinalize => true,
            Self::CrashedAfterRename => true,
            Self::CrashedAfterRestoreTree => true,
            Self::CrashedAfterTakeover => true,
            Self::CrashedAfterTarget => true,
            Self::CrashedBeforeDatabaseFinalize => true,
            Self::CrashedBeforeNativeLink => true,
            Self::CrashedBeforeNativeRemove => true,
            Self::CrashedBeforeRename => true,
            Self::CrashedBeforeRestoreTree => true,
            Self::CrashedBeforeTakeover => true,
            Self::CrashedBeforeTarget => true,
            Self::CrashedDuringDatabaseFinalize => true,
            Self::Applying => false,
            Self::Claimed => false,
            Self::DirectoryCreateFailed => false,
            Self::DirectoryCreatePending => false,
            Self::DirectoryCreated => false,
            Self::DirectoryRestorePending => false,
            Self::DirectoryRestored => false,
            Self::ExternalChangeAfterWrite => false,
            Self::NativeLinkPending => false,
            Self::ReadyToFinalizeDatabase => false,
            Self::Removed => false,
            Self::RenameFailed => false,
            Self::RenamePending => false,
            Self::Renamed => false,
            Self::RollbackFailed => false,
            Self::RolledBack => false,
            Self::RollingBack => false,
            Self::Snapshotted => false,
            Self::Snapshotting => false,
            Self::Succeeded => false,
            Self::TakeoverLinkFailed => false,
            Self::TakeoverLinked => false,
            Self::TakeoverQuarantined => false,
            Self::TakeoverRenameFailed => false,
            Self::TakeoverRenamePending => false,
            Self::Verified => false,
            Self::Writing => false,
            Self::Written => false,
            Self::Unknown => false,
        }
    }
}

impl fmt::Display for TargetPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalOperation {
    Apply,
    Restore,
    #[serde(other)]
    Unknown,
}

impl JournalOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Restore => "restore",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for JournalOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 回滚失败时附带的诊断：原始错误的稳定码与 operation（都已经过 allowlist/脱敏）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct JournalFailure {
    code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RunJournal {
    version: u32,
    run_id: String,
    operation: JournalOperation,
    phase: TargetPhase,
    targets: Vec<JournalTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failure: Option<JournalFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct JournalTarget {
    target_id: String,
    target_path: String,
    snapshot_id: Option<String>,
    snapshot_path: Option<String>,
    phase: TargetPhase,
    before_fingerprint: Option<String>,
    after_fingerprint: Option<String>,
    temporary_path: Option<String>,
    #[serde(default)]
    temporary_fingerprint: Option<String>,
    #[serde(default)]
    quarantine_path: Option<String>,
    #[serde(default)]
    quarantine_fingerprint: Option<String>,
    #[serde(default)]
    takeover_entry_type: Option<SkillTakeoverEntryType>,
    #[serde(default)]
    directory_tree_hash: Option<String>,
    #[serde(default)]
    snapshot_storage_kind: Option<SnapshotStorageKind>,
}

#[derive(Debug, Clone)]
struct SnapshotRecord {
    id: String,
    run_id: String,
    target_id: Option<String>,
    target_path: PathBuf,
    snapshot_path: PathBuf,
    allowed_root: PathBuf,
    central_root: Option<PathBuf>,
    row_version: u32,
    state: PathState,
    storage_kind: SnapshotStorageKind,
    directory_tree_hash: Option<String>,
}

struct SnapshotRequest<'a> {
    run_id: &'a str,
    target_id: Option<&'a str>,
    target_path: &'a Path,
    allowed_root: &'a Path,
    central_root: Option<&'a Path>,
    expected_before_fingerprint: &'a str,
    directory_tree_hash: Option<&'a str>,
    /// 规划阶段已读取的状态；lstat 签名一致时直接复用，否则重新读取。
    known_state: Option<&'a PathState>,
}

/// 只用 lstat 得到的文件签名：用于"内容是否可能变化"的廉价复核。
/// 任一字段变化即回退到完整读取比对；签名相同则复用已读取的内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct StatSignature {
    device: u64,
    inode: u64,
    size: u64,
    mtime: i64,
    mtime_nanos: i64,
    mode: u32,
}

impl StatSignature {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            mtime: metadata.mtime(),
            mtime_nanos: metadata.mtime_nsec(),
            mode: metadata.mode() & 0o7777,
        }
    }
}

/// 目标当前的 lstat 结果是否与已知状态一致（不读内容）。`false` 只表示需要完整复核。
fn cheap_state_matches(state: &PathState, path: &Path) -> bool {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return error.kind() == io::ErrorKind::NotFound && matches!(state, PathState::Missing);
        }
    };
    let file_type = metadata.file_type();
    match state {
        PathState::Missing => false,
        PathState::File { stat, .. } => {
            file_type.is_file() && *stat == StatSignature::from_metadata(&metadata)
        }
        PathState::Symlink { link_target } => {
            file_type.is_symlink() && fs::read_link(path).is_ok_and(|target| &target == link_target)
        }
        PathState::Directory { device, inode } => {
            file_type.is_dir() && metadata.dev() == *device && metadata.ino() == *inode
        }
    }
}

#[cfg(test)]
thread_local! {
    /// 测试用：统计 apply 内核对目标文件的完整读取次数（按线程，测试并行互不干扰）。
    pub(crate) static TARGET_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// 测试用：统计 apply 内核发起的 fsync（文件与目录）次数。
    pub(crate) static FSYNC_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn read_target_bytes(path: &Path) -> io::Result<Vec<u8>> {
    #[cfg(test)]
    TARGET_READS.with(|count| count.set(count.get() + 1));
    fs::read(path)
}

fn fsync_file(file: &File, path: &Path, operation: &'static str) -> Result<(), AppError> {
    #[cfg(test)]
    FSYNC_CALLS.with(|count| count.set(count.get() + 1));
    file.sync_all().map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), operation).with_source(error)
    })
}

#[derive(Debug, Clone)]
enum PathState {
    Missing,
    File {
        bytes: Vec<u8>,
        hash: String,
        mode: u32,
        /// 读取内容时的 lstat 签名；仅用于廉价复核，不参与 fingerprint。
        stat: StatSignature,
    },
    Symlink {
        link_target: PathBuf,
    },
    Directory {
        device: u64,
        inode: u64,
    },
}

impl PathState {
    fn target_type(&self) -> TargetType {
        match self {
            Self::Missing => TargetType::Missing,
            Self::File { .. } => TargetType::File,
            Self::Symlink { .. } => TargetType::Symlink,
            Self::Directory { .. } => TargetType::Directory,
        }
    }

    fn content_hash(&self) -> Option<&str> {
        match self {
            Self::File { hash, .. } => Some(hash),
            _ => None,
        }
    }

    fn mode(&self) -> Option<u32> {
        match self {
            Self::File { mode, .. } => Some(*mode),
            _ => None,
        }
    }

    fn link_target(&self) -> Option<&Path> {
        match self {
            Self::Symlink { link_target } => Some(link_target),
            _ => None,
        }
    }

    fn fingerprint(&self) -> String {
        let value = match self {
            Self::Missing => json!({ "type": "missing" }),
            Self::File { hash, mode, .. } => {
                json!({ "type": "file", "hash": hash, "mode": mode })
            }
            Self::Symlink { link_target } => json!({
                "type": "symlink",
                "linkTarget": link_target.to_string_lossy(),
            }),
            Self::Directory { device, inode } => {
                json!({ "type": "directory", "device": device, "inode": inode })
            }
        };
        hash_json(&value)
    }
}

#[derive(Debug, Clone)]
enum Mutation {
    CreateDirectory,
    WriteFile {
        bytes: Vec<u8>,
        mode: u32,
    },
    Remove,
    ReplaceSymlink {
        link_target: PathBuf,
        central_root: PathBuf,
        allow_external_target: bool,
    },
    TakeoverSymlink {
        link_target: PathBuf,
        central_root: PathBuf,
        entry_type: SkillTakeoverEntryType,
        content_hash: String,
        evidence_fingerprint: String,
    },
    RestoreDirectoryTree {
        snapshot_path: PathBuf,
        content_hash: String,
    },
    RemoveNativeSkill {
        entry_type: super::NativeResourceEntryType,
        content_hash: Option<String>,
    },
    RestoreNativeSymlink {
        link_target: PathBuf,
    },
}

#[derive(Debug, Clone)]
struct PendingMutation {
    target_id: String,
    target_index: usize,
    path: PathBuf,
    allowed_root: PathBuf,
    central_root: Option<PathBuf>,
    expected_before_fingerprint: String,
    expected_after_fingerprint: String,
    /// 规划阶段已读取的目标状态；快照阶段经廉价复核后直接复用，省一次全量读取。
    before_state: Option<PathState>,
    mutation: Mutation,
}

#[derive(Clone, Copy)]
struct ExpectedPathFingerprint<'a> {
    run_id: &'a str,
    target_id: &'a str,
    fingerprint: &'a str,
}

struct TargetWork<'a> {
    item: &'a PersistedPreviewItem,
    input: &'a ApplyTargetInput,
    mutations: Vec<PendingMutation>,
}

#[derive(Debug)]
enum MutationFailure {
    Error(AppError),
    Crash(AppError),
}

impl From<AppError> for MutationFailure {
    fn from(value: AppError) -> Self {
        Self::Error(value)
    }
}

pub fn apply_persisted_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    preview_id: &str,
    inputs: &[ApplyTargetInput],
    fault: &dyn ApplyFaultInjector,
) -> Result<ApplyResult, AppError> {
    let _write_guard = write_operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    paths.audit_run_scope([preview_id])?;
    let journal_path = paths.journals().join(format!("{preview_id}.json"));
    claim_preview(database, preview_id, &journal_path)?;

    let result = apply_claimed_preview(database, paths, preview_id, inputs, fault);
    if let Err(error) = &result {
        if error.code() == ErrorCode::StalePreview {
            mark_run_stale(database, preview_id)?;
        } else if !journal_reports_crash(paths, preview_id)
            && !run_has_snapshots(database, preview_id).unwrap_or(true)
        {
            settle_unhandled_apply_error(database, preview_id, error.code())?;
        }
    }
    result
}

fn run_has_snapshots(database: &Database, run_id: &str) -> Result<bool, AppError> {
    let database_path = database.path().to_string_lossy();
    crate::db::sync::has_snapshots_for_run(database.connection(), run_id, &database_path)
}

fn journal_reports_crash(paths: &AppPaths, run_id: &str) -> bool {
    let path = paths.journals().join(format!("{run_id}.json"));
    fs::read(path)
        .ok()
        .and_then(|bytes| parse_journal(&bytes))
        .is_some_and(|journal| {
            journal.operation == JournalOperation::Unknown
                || journal.phase.may_have_changed_target()
                || journal.phase.is_crashed()
                || journal.targets.iter().any(|target| {
                    target.phase.may_have_changed_target() || target.phase.is_crashed()
                })
        })
}

fn settle_unhandled_apply_error(
    database: &mut Database,
    run_id: &str,
    error_code: ErrorCode,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    crate::db::sync::settle_sync_run_error(
        database.connection(),
        run_id,
        error_code.persisted().as_str(),
        &database_path,
    )
}

fn apply_claimed_preview(
    database: &mut Database,
    paths: &AppPaths,
    preview_id: &str,
    inputs: &[ApplyTargetInput],
    fault: &dyn ApplyFaultInjector,
) -> Result<ApplyResult, AppError> {
    let preview = load_persisted_preview(database, preview_id)?;
    validate_preview_inputs(database, &preview, inputs)?;
    let mut journal = RunJournal {
        version: 1,
        run_id: preview_id.to_owned(),
        operation: JournalOperation::Apply,
        phase: TargetPhase::Claimed,
        targets: Vec::new(),
        failure: None,
    };
    persist_journal(paths, &journal)?;

    let work = build_target_work(&preview, inputs)?;
    let mutations = flatten_mutations(&work)?;
    let mut snapshots = Vec::with_capacity(mutations.len());
    journal.phase = TargetPhase::Snapshotting;
    persist_journal(paths, &journal)?;
    for mutation in &mutations {
        let snapshot = match create_snapshot(
            database,
            paths,
            SnapshotRequest {
                run_id: preview_id,
                target_id: Some(&mutation.target_id),
                target_path: &mutation.path,
                allowed_root: &mutation.allowed_root,
                central_root: mutation.central_root.as_deref(),
                expected_before_fingerprint: &mutation.expected_before_fingerprint,
                directory_tree_hash: native_or_takeover_directory_hash(&mutation.mutation),
                known_state: mutation.before_state.as_ref(),
            },
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return finish_failed_apply(
                    database,
                    paths,
                    preview_id,
                    &mut journal,
                    &snapshots,
                    &[],
                    error,
                );
            }
        };
        journal.targets.push(JournalTarget {
            target_id: mutation.target_id.clone(),
            target_path: mutation.path.to_string_lossy().into_owned(),
            snapshot_id: Some(snapshot.id.clone()),
            snapshot_path: Some(snapshot.snapshot_path.to_string_lossy().into_owned()),
            phase: TargetPhase::Snapshotted,
            before_fingerprint: Some(snapshot.state.fingerprint()),
            after_fingerprint: None,
            temporary_path: None,
            temporary_fingerprint: None,
            quarantine_path: None,
            quarantine_fingerprint: None,
            takeover_entry_type: match &mutation.mutation {
                Mutation::TakeoverSymlink { entry_type, .. } => Some(*entry_type),
                _ => None,
            },
            directory_tree_hash: match &mutation.mutation {
                Mutation::TakeoverSymlink {
                    entry_type: SkillTakeoverEntryType::Directory,
                    content_hash,
                    ..
                }
                | Mutation::RestoreDirectoryTree { content_hash, .. } => Some(content_hash.clone()),
                Mutation::RemoveNativeSkill {
                    entry_type: super::NativeResourceEntryType::Directory,
                    content_hash: Some(content_hash),
                } => Some(content_hash.clone()),
                _ => None,
            },
            snapshot_storage_kind: Some(snapshot.storage_kind),
        });
        snapshots.push(snapshot);
        persist_journal(paths, &journal)?;
    }

    journal.phase = TargetPhase::Applying;
    persist_journal(paths, &journal)?;
    // 数据库预检整轮只做一次；循环内用 `PRAGMA data_version` 侦测是否有其它连接
    // 提交过写入，只有版本变化时才重做完整预检。
    if let Err(error) = revalidate_database_preflight(database, &preview) {
        return finish_failed_apply(
            database,
            paths,
            preview_id,
            &mut journal,
            &snapshots,
            &[],
            error,
        );
    }
    let mut data_version = read_data_version(database)?;
    let mut applied = Vec::new();
    for (mutation_index, mutation) in mutations.iter().enumerate() {
        let event = ApplyFaultEvent::BeforeTarget {
            index: mutation.target_index,
            path: mutation.path.clone(),
        };
        match fault.decide(&event) {
            ApplyFaultDecision::Continue => {}
            ApplyFaultDecision::Fail => {
                let error =
                    AppError::atomic_write(&mutation.path.to_string_lossy(), "fault_before_target");
                return finish_failed_apply(
                    database,
                    paths,
                    preview_id,
                    &mut journal,
                    &snapshots,
                    &applied,
                    error,
                );
            }
            ApplyFaultDecision::Crash => {
                journal.targets[mutation_index].phase = TargetPhase::CrashedBeforeTarget;
                persist_journal(paths, &journal)?;
                return Err(AppError::atomic_write(
                    &mutation.path.to_string_lossy(),
                    "simulated_crash_before_target",
                ));
            }
        }
        let current_data_version = read_data_version(database)?;
        if current_data_version != data_version {
            if let Err(error) = revalidate_database_preflight(database, &preview) {
                return finish_failed_apply(
                    database,
                    paths,
                    preview_id,
                    &mut journal,
                    &snapshots,
                    &applied,
                    error,
                );
            }
            data_version = current_data_version;
        }
        let before_state = &snapshots[mutation_index].state;
        if !cheap_state_matches(before_state, &mutation.path)
            && capture_path_state(&mutation.path)?.fingerprint() != before_state.fingerprint()
        {
            return finish_failed_apply(
                database,
                paths,
                preview_id,
                &mut journal,
                &snapshots,
                &applied,
                AppError::stale_preview(preview_id, &mutation.target_id),
            );
        }

        match apply_mutation(
            paths,
            &mut journal,
            mutation_index,
            mutation,
            before_state,
            fault,
        ) {
            Ok(()) => applied.push(mutation_index),
            Err(MutationFailure::Crash(error)) => return Err(error),
            Err(MutationFailure::Error(error)) => {
                if mutation_may_have_changed_target(&journal.targets[mutation_index]) {
                    applied.push(mutation_index);
                }
                return finish_failed_apply(
                    database,
                    paths,
                    preview_id,
                    &mut journal,
                    &snapshots,
                    &applied,
                    error,
                );
            }
        }
    }

    let verifications = match verify_all_targets(&work) {
        Ok(verifications) => verifications,
        Err(error) => {
            return finish_failed_apply(
                database,
                paths,
                preview_id,
                &mut journal,
                &snapshots,
                &applied,
                error,
            );
        }
    };
    journal.phase = TargetPhase::ReadyToFinalizeDatabase;
    for target in &mut journal.targets {
        target.phase = TargetPhase::Verified;
    }
    if let Err(error) = persist_journal(paths, &journal) {
        return finish_failed_apply(
            database,
            paths,
            preview_id,
            &mut journal,
            &snapshots,
            &applied,
            error,
        );
    }
    match fault.decide(&ApplyFaultEvent::BeforeDatabaseFinalize) {
        ApplyFaultDecision::Continue => {}
        ApplyFaultDecision::Fail => {
            return finish_failed_apply(
                database,
                paths,
                preview_id,
                &mut journal,
                &snapshots,
                &applied,
                AppError::database(
                    &database.path().to_string_lossy(),
                    "fault_before_database_finalize",
                ),
            );
        }
        ApplyFaultDecision::Crash => {
            journal.phase = TargetPhase::CrashedBeforeDatabaseFinalize;
            persist_journal(paths, &journal)?;
            return Err(AppError::database(
                &database.path().to_string_lossy(),
                "simulated_crash_before_database_finalize",
            ));
        }
    }
    if let Err(error) = finish_successful_apply(database, &preview, inputs, &verifications) {
        // SQLite commit 的 I/O 错误可能发生在提交边界两侧。此时不能猜测 DB
        // 是否已经持久化，更不能据此自动反向覆盖外部目标；保留活动 run 交给恢复流。
        journal.phase = TargetPhase::CrashedDuringDatabaseFinalize;
        persist_journal(paths, &journal)?;
        return Err(error);
    }
    if fault.decide(&ApplyFaultEvent::AfterDatabaseFinalize) == ApplyFaultDecision::Crash {
        journal.phase = TargetPhase::CrashedAfterDatabaseFinalize;
        persist_journal(paths, &journal)?;
        return Err(AppError::database(
            &database.path().to_string_lossy(),
            "simulated_crash_after_database_finalize",
        ));
    }
    let _ = cleanup_takeover_quarantines(&mut journal);
    journal.phase = TargetPhase::Succeeded;
    // DB 已经原子 finalize 后，journal 的最终装饰性状态失败不能触发外部回滚，
    // 否则会把已提交基线与文件内容拆成两个真相。ready_to_finalize 仍是 durable 证据。
    let _ = persist_journal(paths, &journal);
    Ok(ApplyResult {
        run_id: preview_id.to_owned(),
        status: "succeeded".to_owned(),
        applied_targets: u32::try_from(work.len()).map_err(|error| {
            AppError::invalid_input("targetCount", "目标数量超出 RPC 安全范围").with_source(error)
        })?,
        snapshot_count: u32::try_from(snapshots.len()).map_err(|error| {
            AppError::invalid_input("snapshotCount", "快照数量超出 RPC 安全范围").with_source(error)
        })?,
    })
}

fn mutation_may_have_changed_target(target: &JournalTarget) -> bool {
    target.after_fingerprint.is_some() || target.phase.may_have_changed_target()
}

fn claim_preview(
    database: &mut Database,
    preview_id: &str,
    journal_path: &Path,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_claim_preview").with_source(error)
        })?;
    let run = crate::db::sync::load_sync_run_kind_status(
        &transaction,
        preview_id,
        &database_path,
        "read_preview_claim",
    )?
        .ok_or_else(|| AppError::not_found("preview", preview_id))?;
    if run.0 != "preview" || run.1 != "previewed" {
        return Err(AppError::preview_already_consumed(preview_id, &run.1));
    }
    if let Some((run_id, status)) = active_writer(&transaction, Some(preview_id), &database_path)? {
        return Err(AppError::write_in_progress(&run_id, &status));
    }
    let updated = crate::db::sync::update_sync_run_for_apply(
        &transaction,
        preview_id,
        journal_path,
    )?;
    if updated != 1 {
        return Err(AppError::preview_already_consumed(
            preview_id,
            "not_previewed",
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_claim_preview").with_source(error)
    })
}

fn mark_run_stale(database: &mut Database, run_id: &str) -> Result<(), AppError> {
    let path = database.path().to_string_lossy().into_owned();
    crate::db::sync::mark_sync_run_stale(database.connection(), run_id, &path)
}

fn native_or_takeover_directory_hash(mutation: &Mutation) -> Option<&str> {
    match mutation {
        Mutation::TakeoverSymlink {
            entry_type: SkillTakeoverEntryType::Directory,
            content_hash,
            ..
        } => Some(content_hash.as_str()),
        Mutation::RemoveNativeSkill {
            entry_type: super::NativeResourceEntryType::Directory,
            content_hash: Some(content_hash),
        } => Some(content_hash.as_str()),
        _ => None,
    }
}

/// 以精确目标路径索引 Apply 输入。没有路径的描述符不能参与 Apply，重复路径也
/// 不能静默互相覆盖；两者都必须显式失败，而不是折叠成同一个空字符串键
/// （那会让后续按 `target_path` 查找时拿到错误的输入）。
fn inputs_by_target_path<'a>(
    preview: &PersistedPreview,
    inputs: &'a [ApplyTargetInput],
) -> Result<BTreeMap<&'a str, &'a ApplyTargetInput>, AppError> {
    let mut by_path = BTreeMap::new();
    for input in inputs {
        let path = input
            .descriptor
            .path
            .as_deref()
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, "unsupportedTarget"))?;
        if by_path.insert(path, input).is_some() {
            return Err(AppError::invalid_input(
                "targetPath",
                "Apply 不能包含重复目标路径",
            ));
        }
    }
    Ok(by_path)
}
