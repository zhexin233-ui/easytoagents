//! Phase 3 写入内核：消费持久化 Preview，逐目标快照、原子替换、验证与恢复。

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CString, OsStr},
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
    canonical_json, hash_bytes, hash_json, load_persisted_preview, scan_target, DatabaseEntityType,
    DatabaseRowVersion, PersistedPreview, PersistedPreviewEnvelope, PersistedPreviewItem,
    TargetScan,
};
use crate::{
    adapters::{
        claude::ClaudeAdapter, codex::CodexAdapter, ManagedOwnership, RenderedTarget,
        TargetDescriptor, TargetFormat, ToolAdapter,
    },
    app::AppPaths,
    db::Database,
    domain::{ArtifactKind, ChangeKind, ProjectRoot, Scope, TargetType, Tool},
    error::{AppError, ErrorCode},
    git::{inspect_path, render_local_exclude, resolve_local_exclude},
    security::{
        create_private_file, ensure_private_directory, ensure_private_file, PRIVATE_FILE_MODE,
    },
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
    pub created_at: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct RunJournal {
    version: u32,
    run_id: String,
    operation: String,
    phase: String,
    targets: Vec<JournalTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct JournalTarget {
    target_id: String,
    target_path: String,
    snapshot_id: Option<String>,
    snapshot_path: Option<String>,
    phase: String,
    before_fingerprint: Option<String>,
    after_fingerprint: Option<String>,
    temporary_path: Option<String>,
    #[serde(default)]
    temporary_fingerprint: Option<String>,
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
}

struct SnapshotRequest<'a> {
    run_id: &'a str,
    target_id: Option<&'a str>,
    target_path: &'a Path,
    allowed_root: &'a Path,
    central_root: Option<&'a Path>,
    expected_before_fingerprint: &'a str,
}

#[derive(Debug, Clone)]
enum PathState {
    Missing,
    File {
        bytes: Vec<u8>,
        hash: String,
        mode: u32,
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
        .map_err(|_| AppError::new(ErrorCode::WriteInProgress, "写入互斥锁不可用", false))?;
    paths.audit_permissions()?;
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
    database
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM snapshots WHERE run_id = ?1)",
            [run_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::database(&database_path, "detect_run_snapshots"))
}

fn journal_reports_crash(paths: &AppPaths, run_id: &str) -> bool {
    let path = paths.journals().join(format!("{run_id}.json"));
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<RunJournal>(&bytes).ok())
        .is_some_and(|journal| {
            journal.phase.starts_with("crashed")
                || journal
                    .targets
                    .iter()
                    .any(|target| target.phase.starts_with("crashed"))
        })
}

fn settle_unhandled_apply_error(
    database: &mut Database,
    run_id: &str,
    error_code: ErrorCode,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    database
        .connection_mut()
        .execute(
            "UPDATE sync_runs
             SET status = 'failed', error_code = ?2,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status IN ('applying', 'restoring')",
            params![run_id, error_code.as_str()],
        )
        .map_err(|_| AppError::database(&database_path, "settle_apply_error"))?;
    Ok(())
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
        operation: "apply".to_owned(),
        phase: "claimed".to_owned(),
        targets: Vec::new(),
    };
    persist_journal(paths, &journal)?;

    let work = build_target_work(&preview, inputs)?;
    let mutations = flatten_mutations(&work)?;
    let mut snapshots = Vec::with_capacity(mutations.len());
    journal.phase = "snapshotting".to_owned();
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
            phase: "snapshotted".to_owned(),
            before_fingerprint: Some(snapshot.state.fingerprint()),
            after_fingerprint: None,
            temporary_path: None,
            temporary_fingerprint: None,
        });
        snapshots.push(snapshot);
        persist_journal(paths, &journal)?;
    }

    journal.phase = "applying".to_owned();
    persist_journal(paths, &journal)?;
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
                journal.targets[mutation_index].phase = "crashed_before_target".to_owned();
                persist_journal(paths, &journal)?;
                return Err(AppError::atomic_write(
                    &mutation.path.to_string_lossy(),
                    "simulated_crash_before_target",
                ));
            }
        }
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
        if capture_path_state(&mutation.path)?.fingerprint()
            != snapshots[mutation_index].state.fingerprint()
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
            &snapshots[mutation_index].state.fingerprint(),
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
    journal.phase = "ready_to_finalize_database".to_owned();
    for target in &mut journal.targets {
        target.phase = "verified".to_owned();
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
            journal.phase = "crashed_before_database_finalize".to_owned();
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
        journal.phase = "crashed_during_database_finalize".to_owned();
        persist_journal(paths, &journal)?;
        return Err(error);
    }
    if fault.decide(&ApplyFaultEvent::AfterDatabaseFinalize) == ApplyFaultDecision::Crash {
        journal.phase = "crashed_after_database_finalize".to_owned();
        persist_journal(paths, &journal)?;
        return Err(AppError::database(
            &database.path().to_string_lossy(),
            "simulated_crash_after_database_finalize",
        ));
    }
    journal.phase = "succeeded".to_owned();
    // DB 已经原子 finalize 后，journal 的最终装饰性状态失败不能触发外部回滚，
    // 否则会把已提交基线与文件内容拆成两个真相。ready_to_finalize 仍是 durable 证据。
    let _ = persist_journal(paths, &journal);
    Ok(ApplyResult {
        run_id: preview_id.to_owned(),
        status: "succeeded".to_owned(),
        applied_targets: u32::try_from(work.len())
            .map_err(|_| AppError::invalid_input("targetCount", "目标数量超出 RPC 安全范围"))?,
        snapshot_count: u32::try_from(snapshots.len())
            .map_err(|_| AppError::invalid_input("snapshotCount", "快照数量超出 RPC 安全范围"))?,
    })
}

fn mutation_may_have_changed_target(target: &JournalTarget) -> bool {
    target.after_fingerprint.is_some()
        || matches!(
            target.phase.as_str(),
            "directory_create_pending"
                | "directory_created"
                | "renamed"
                | "removed"
                | "written"
                | "crashed_after_rename"
                | "crashed_after_target"
        )
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
        .map_err(|_| AppError::database(&database_path, "begin_claim_preview"))?;
    let run = transaction
        .query_row(
            "SELECT kind, status FROM sync_runs WHERE id = ?1",
            [preview_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "read_preview_claim"))?
        .ok_or_else(|| AppError::not_found("preview", preview_id))?;
    if run.0 != "preview" || run.1 != "previewed" {
        return Err(AppError::preview_already_consumed(preview_id, &run.1));
    }
    if let Some((run_id, status)) = active_writer(&transaction, Some(preview_id), &database_path)? {
        return Err(AppError::write_in_progress(&run_id, &status));
    }
    let updated = transaction
        .execute(
            "UPDATE sync_runs
             SET kind = 'apply', status = 'applying', journal_path = ?2
             WHERE id = ?1 AND kind = 'preview' AND status = 'previewed'",
            params![preview_id, journal_path.to_string_lossy()],
        )
        .map_err(|_| AppError::write_in_progress(preview_id, "applying"))?;
    if updated != 1 {
        return Err(AppError::preview_already_consumed(
            preview_id,
            "not_previewed",
        ));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_claim_preview"))
}

fn active_writer(
    transaction: &Transaction<'_>,
    except_run: Option<&str>,
    database_path: &str,
) -> Result<Option<(String, String)>, AppError> {
    transaction
        .query_row(
            "SELECT id, status FROM sync_runs
             WHERE status IN ('applying', 'restoring', 'rollback_failed')
               AND (?1 IS NULL OR id != ?1)
             ORDER BY CASE WHEN status IN ('applying', 'restoring') THEN 0 ELSE 1 END,
                      started_at
             LIMIT 1",
            [except_run],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| AppError::database(database_path, "read_active_writer"))
}

fn mark_run_stale(database: &mut Database, run_id: &str) -> Result<(), AppError> {
    let path = database.path().to_string_lossy().into_owned();
    database
        .connection_mut()
        .execute(
            "UPDATE sync_runs
             SET status = 'stale', error_code = 'STALE_PREVIEW',
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status = 'applying'",
            [run_id],
        )
        .map_err(|_| AppError::database(&path, "mark_stale_preview"))?;
    Ok(())
}

fn validate_preview_inputs(
    database: &Database,
    preview: &PersistedPreview,
    inputs: &[ApplyTargetInput],
) -> Result<(), AppError> {
    if preview.items.len() != inputs.len() {
        return Err(AppError::stale_preview(&preview.preview_id, "targetSet"));
    }
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
    let database_path = database.path().to_string_lossy().into_owned();
    let mut expected_versions = BTreeMap::new();
    for item in &preview.items {
        if item.change_kind == ChangeKind::Conflict || item.error_code.is_some() {
            return Err(AppError::conflict("preview", "包含冲突的 Preview 不能应用"));
        }
        let input = by_path
            .get(item.target_path.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &item.target_id))?;
        if input.descriptor != item.envelope.descriptor
            || input.ownership != item.envelope.ownership
            || input.descriptor.path.as_deref() != Some(item.target_path.as_str())
        {
            return Err(AppError::stale_preview(
                &preview.preview_id,
                &item.target_id,
            ));
        }
        if hash_json(&canonical_json(&input.desired_projection))
            != item.envelope.desired_managed_hash
        {
            return Err(AppError::stale_preview(
                &preview.preview_id,
                &item.target_id,
            ));
        }
        if input.delete_target && item.change_kind != ChangeKind::Delete {
            return Err(AppError::invalid_input(
                "deleteTarget",
                "只有 delete Preview 可以删除 whole-document 目标",
            ));
        }
        if input.delete_target && input.ownership != ManagedOwnership::WholeDocument {
            return Err(AppError::invalid_input(
                "deleteTarget",
                "只有 whole-document 所有权可以删除整个目标",
            ));
        }
        validate_descriptor_identity(
            database,
            item,
            preview.scope,
            preview.project_id.as_deref(),
            &database_path,
        )?;
        validate_managed_item_inputs(
            database.connection(),
            item,
            input,
            &preview.preview_id,
            &database_path,
        )?;
        record_expected_versions(item, &mut expected_versions)?;
        validate_allowed_path(
            Path::new(&item.target_path),
            &input.allowed_root,
            input.descriptor.format == TargetFormat::SymlinkDirectory,
        )?;
        validate_preview_hashes(item, input)?;
    }
    verify_database_versions(
        database.connection(),
        &expected_versions,
        &preview.preview_id,
        &database_path,
    )
}

fn validate_managed_item_inputs(
    connection: &rusqlite::Connection,
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    preview_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    let expected_versions = item
        .envelope
        .row_versions
        .iter()
        .filter(|row| row.entity_type == DatabaseEntityType::ManagedItem)
        .map(|row| (row.entity_id.as_str(), row.row_version))
        .collect::<BTreeMap<_, _>>();
    let mut ids = BTreeSet::new();
    for managed_item in &input.managed_items {
        if !ids.insert(managed_item.id.as_str())
            || managed_item.resource_kind != input.descriptor.artifact_kind
            || Uuid::parse_str(&managed_item.id).is_err()
            || Uuid::parse_str(&managed_item.resource_id).is_err()
            || managed_item.external_key.is_empty()
            || !is_sha256(&managed_item.last_applied_item_hash)
        {
            return Err(AppError::invalid_input(
                "managedItems",
                "managed item 的身份、类型、名称或 hash 无效",
            ));
        }
        let existing = connection
            .query_row(
                "SELECT target_id, row_version FROM managed_items WHERE id = ?1",
                [&managed_item.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|_| AppError::database(database_path, "preflight_managed_item"))?;
        match existing {
            Some((target_id, row_version)) => {
                let expected = expected_versions
                    .get(managed_item.id.as_str())
                    .copied()
                    .ok_or_else(|| AppError::stale_preview(preview_id, &managed_item.id))?;
                if target_id != item.target_id || u32::try_from(row_version).ok() != Some(expected)
                {
                    return Err(AppError::stale_preview(preview_id, &managed_item.id));
                }
            }
            None => {
                if expected_versions.contains_key(managed_item.id.as_str()) {
                    return Err(AppError::stale_preview(preview_id, &managed_item.id));
                }
                let conflicting_id = connection
                    .query_row(
                        "SELECT id FROM managed_items WHERE target_id = ?1 AND external_key = ?2",
                        params![item.target_id, managed_item.external_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|_| AppError::database(database_path, "preflight_managed_item_key"))?;
                if conflicting_id.is_some() {
                    return Err(AppError::conflict(
                        "managedItems",
                        "managed item 外部名称已被其他基线占用",
                    ));
                }
            }
        }
    }
    for remove_id in &input.remove_managed_item_ids {
        if !ids.insert(remove_id) || Uuid::parse_str(remove_id).is_err() {
            return Err(AppError::invalid_input(
                "managedItems",
                "managed item 删除身份无效或与更新重复",
            ));
        }
        let expected = expected_versions
            .get(remove_id.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(preview_id, remove_id))?;
        let actual = connection
            .query_row(
                "SELECT row_version FROM managed_items WHERE id = ?1 AND target_id = ?2",
                params![remove_id, item.target_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|_| AppError::database(database_path, "preflight_remove_managed_item"))?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(expected) {
            return Err(AppError::stale_preview(preview_id, remove_id));
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_descriptor_identity(
    database: &Database,
    item: &PersistedPreviewItem,
    preview_scope: Scope,
    preview_project_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    let identity = database
        .connection()
        .query_row(
            "SELECT target.row_version, target.tool, target.artifact_kind, target.scope,
                    target.project_id, target.target_path, project.root_path
             FROM managed_targets AS target
             LEFT JOIN projects AS project ON project.id = target.project_id
             WHERE target.id = ?1",
            [&item.target_id],
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
        .map_err(|_| AppError::database(database_path, "verify_apply_descriptor"))?
        .ok_or_else(|| AppError::stale_preview("persisted", &item.target_id))?;
    let descriptor = &item.envelope.descriptor;
    if u32::try_from(identity.0).ok() != Some(item.envelope.target_row_version)
        || identity.1 != descriptor.tool.as_str()
        || identity.2 != descriptor.artifact_kind.as_str()
        || identity.3 != descriptor.scope.as_str()
        || descriptor.scope != preview_scope
        || identity.4.as_deref() != preview_project_id
        || identity.5 != item.target_path
        || identity.6 != descriptor.project_root
    {
        return Err(AppError::stale_preview("persisted", &item.target_id));
    }
    Ok(())
}

fn record_expected_versions(
    item: &PersistedPreviewItem,
    versions: &mut BTreeMap<(DatabaseEntityType, String), u32>,
) -> Result<(), AppError> {
    let target = DatabaseRowVersion {
        entity_type: DatabaseEntityType::ManagedTarget,
        entity_id: item.target_id.clone(),
        row_version: item.envelope.target_row_version,
    };
    for row in std::iter::once(&target).chain(item.envelope.row_versions.iter()) {
        let key = (row.entity_type, row.entity_id.clone());
        if versions
            .insert(key, row.row_version)
            .is_some_and(|existing| existing != row.row_version)
        {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Preview 的 row_version 互相矛盾",
            ));
        }
    }
    Ok(())
}

fn verify_database_versions(
    connection: &rusqlite::Connection,
    versions: &BTreeMap<(DatabaseEntityType, String), u32>,
    preview_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    for ((entity_type, entity_id), expected) in versions {
        let query = format!(
            "SELECT row_version FROM {} WHERE id = ?1",
            entity_type.table()
        );
        let actual = connection
            .query_row(&query, [entity_id], |row| row.get::<_, i64>(0))
            .optional()
            .map_err(|_| AppError::database(database_path, "verify_apply_row_version"))?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(*expected) {
            return Err(AppError::stale_preview(preview_id, entity_id));
        }
    }
    Ok(())
}

fn revalidate_database_preflight(
    database: &Database,
    preview: &PersistedPreview,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let mut expected_versions = BTreeMap::new();
    for item in &preview.items {
        validate_descriptor_identity(
            database,
            item,
            preview.scope,
            preview.project_id.as_deref(),
            &database_path,
        )?;
        record_expected_versions(item, &mut expected_versions)?;
    }
    verify_database_versions(
        database.connection(),
        &expected_versions,
        &preview.preview_id,
        &database_path,
    )
}

fn validate_preview_hashes(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
) -> Result<(), AppError> {
    let adapter = adapter_for(input.descriptor.tool);
    match scan_target(adapter, &input.descriptor, &input.ownership) {
        TargetScan::Missing
            if item.envelope.current_full_hash.is_none()
                && item.envelope.current_managed_hash.is_none() =>
        {
            Ok(())
        }
        TargetScan::Observed(observed)
            if item.envelope.current_full_hash.as_deref() == Some(&observed.full_hash)
                && item.envelope.current_managed_hash.as_deref()
                    == Some(&observed.managed_hash) =>
        {
            Ok(())
        }
        _ => Err(AppError::stale_preview("persisted", &item.target_id)),
    }
}

fn adapter_for(tool: Tool) -> &'static dyn ToolAdapter {
    static CLAUDE: ClaudeAdapter = ClaudeAdapter;
    static CODEX: CodexAdapter = CodexAdapter;
    match tool {
        Tool::Claude => &CLAUDE,
        Tool::Codex => &CODEX,
    }
}

fn build_target_work<'a>(
    preview: &'a PersistedPreview,
    inputs: &'a [ApplyTargetInput],
) -> Result<Vec<TargetWork<'a>>, AppError> {
    let inputs = inputs
        .iter()
        .map(|input| (input.descriptor.path.as_deref().unwrap_or_default(), input))
        .collect::<BTreeMap<_, _>>();
    let mut work = Vec::with_capacity(preview.items.len());
    let mut exclude_patterns = BTreeMap::<PathBuf, (String, PathBuf, BTreeSet<String>)>::new();
    for (target_index, item) in preview.items.iter().enumerate() {
        let input = inputs
            .get(item.target_path.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &item.target_id))?;
        let mut mutations = build_target_mutations(item, input, target_index)?;
        if item.envelope.exclude_from_git {
            let project_root = input.descriptor.project_root.as_deref().ok_or_else(|| {
                AppError::invalid_input("excludeFromGit", "只有项目目标可以写入本地 exclude")
            })?;
            let project_root = ProjectRoot::parse(Path::new(project_root))?;
            let current_git = inspect_path(&project_root, Path::new(&item.target_path))?;
            if current_git.tracked {
                return Err(AppError::stale_preview(
                    &preview.preview_id,
                    &item.target_id,
                ));
            }
            let exclude = resolve_local_exclude(&project_root)?;
            let relative = Path::new(&item.target_path)
                .strip_prefix(project_root.as_str())
                .map_err(|_| AppError::invalid_input("targetPath", "项目目标不在登记项目根内"))?;
            let pattern = format!(
                "/{}",
                relative
                    .to_str()
                    .ok_or_else(|| {
                        AppError::invalid_input("targetPath", "项目目标路径不是 UTF-8")
                    })?
                    .trim_start_matches('/')
            );
            exclude_patterns
                .entry(exclude)
                .or_insert_with(|| {
                    (
                        item.target_id.clone(),
                        input.allowed_root.clone(),
                        BTreeSet::new(),
                    )
                })
                .2
                .insert(pattern);
        }
        work.push(TargetWork {
            item,
            input,
            mutations: std::mem::take(&mut mutations),
        });
    }
    for (exclude_path, (target_id, allowed_root, patterns)) in exclude_patterns {
        let state = capture_path_state(&exclude_path)?;
        let expected_before_fingerprint = state.fingerprint();
        let (existing, mode) = match state {
            PathState::File { bytes, mode, .. } => (bytes, mode),
            PathState::Missing => (Vec::new(), PRIVATE_FILE_MODE),
            _ => {
                return Err(AppError::conflict("gitExclude", "Git exclude 不是普通文件"));
            }
        };
        let rendered = render_local_exclude(&existing, patterns.into_iter())?;
        if rendered != existing {
            let owner_index = work
                .iter_mut()
                .position(|target| target.item.target_id == target_id)
                .expect("exclude 的受管目标必须存在");
            work[owner_index].mutations.push(PendingMutation {
                target_id,
                target_index: owner_index,
                path: exclude_path,
                allowed_root,
                central_root: None,
                expected_before_fingerprint,
                expected_after_fingerprint: PathState::File {
                    hash: hash_bytes(&rendered),
                    bytes: rendered.clone(),
                    mode,
                }
                .fingerprint(),
                mutation: Mutation::WriteFile {
                    bytes: rendered,
                    mode,
                },
            });
        }
    }
    Ok(work)
}

fn build_target_mutations(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    target_index: usize,
) -> Result<Vec<PendingMutation>, AppError> {
    if matches!(
        item.change_kind,
        ChangeKind::Unchanged | ChangeKind::Warning
    ) {
        return Ok(Vec::new());
    }
    validate_preview_hashes(item, input)?;
    let path = PathBuf::from(&item.target_path);
    if input.delete_target {
        let expected_before_fingerprint = capture_path_state(&path)?.fingerprint();
        validate_preview_hashes(item, input)?;
        return Ok(vec![PendingMutation {
            target_id: item.target_id.clone(),
            target_index,
            path,
            allowed_root: input.allowed_root.clone(),
            central_root: input.central_skills_root.clone(),
            expected_before_fingerprint,
            expected_after_fingerprint: PathState::Missing.fingerprint(),
            mutation: Mutation::Remove,
        }]);
    }
    if input.descriptor.format == TargetFormat::SymlinkDirectory {
        let mutations = build_symlink_mutations(item, input, target_index)?;
        validate_preview_hashes(item, input)?;
        return Ok(mutations);
    }
    let adapter = adapter_for(input.descriptor.tool);
    let scan = scan_target(adapter, &input.descriptor, &input.ownership);
    let current = match &scan {
        TargetScan::Observed(observed) => Some(observed.document()),
        TargetScan::Missing => None,
        _ => return Err(AppError::stale_preview("persisted", &item.target_id)),
    };
    let RenderedTarget::File(bytes) = adapter.render(
        &input.descriptor,
        current,
        &input.desired_projection,
        &input.ownership,
    )?;
    let current_state = capture_path_state(&path)?;
    let mode = match &current_state {
        PathState::File { mode, .. } => *mode,
        PathState::Missing => PRIVATE_FILE_MODE,
        _ => {
            return Err(AppError::conflict(
                "targetPath",
                "文件目标被未知目录或链接占用",
            ));
        }
    };
    let expected_before_fingerprint = current_state.fingerprint();
    validate_preview_hashes(item, input)?;
    Ok(vec![PendingMutation {
        target_id: item.target_id.clone(),
        target_index,
        path,
        allowed_root: input.allowed_root.clone(),
        central_root: None,
        expected_before_fingerprint,
        expected_after_fingerprint: PathState::File {
            hash: hash_bytes(&bytes),
            bytes: bytes.clone(),
            mode,
        }
        .fingerprint(),
        mutation: Mutation::WriteFile { bytes, mode },
    }])
}

fn build_symlink_mutations(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    target_index: usize,
) -> Result<Vec<PendingMutation>, AppError> {
    let central_root = input
        .central_skills_root
        .as_ref()
        .ok_or_else(|| AppError::invalid_input("centralSkillsRoot", "Skills 写入缺少中央库边界"))?;
    let names = match &input.ownership {
        ManagedOwnership::SymlinkNames(names) => names,
        _ => {
            return Err(AppError::invalid_input(
                "managedOwnership",
                "Skills 目录必须使用受管子链接名称",
            ));
        }
    };
    let desired = input
        .desired_projection
        .as_object()
        .ok_or_else(|| AppError::invalid_input("desiredProjection", "Skills 投影必须是对象"))?;
    let directory = Path::new(&item.target_path);
    let mut mutations = if desired.is_empty() {
        Vec::new()
    } else {
        build_missing_skill_directories(
            directory,
            &input.allowed_root,
            central_root,
            &item.target_id,
            target_index,
        )?
    };
    for name in names {
        validate_child_name(name)?;
        let child = directory.join(name);
        match desired.get(name) {
            Some(value) => {
                let link_target =
                    value
                        .get("linkTarget")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            AppError::invalid_input(
                                "desiredProjection",
                                "Skills 链接缺少 linkTarget",
                            )
                        })?;
                let canonical_link_target =
                    validate_central_link_target(&child, Path::new(link_target), central_root)?;
                let current = validate_existing_managed_link(&child, central_root, false)?;
                mutations.push(PendingMutation {
                    target_id: item.target_id.clone(),
                    target_index,
                    path: child,
                    allowed_root: input.allowed_root.clone(),
                    central_root: Some(central_root.clone()),
                    expected_before_fingerprint: current.fingerprint(),
                    expected_after_fingerprint: PathState::Symlink {
                        link_target: canonical_link_target.clone(),
                    }
                    .fingerprint(),
                    mutation: Mutation::ReplaceSymlink {
                        link_target: canonical_link_target,
                        central_root: central_root.clone(),
                    },
                });
            }
            None => {
                let current = capture_path_state(&child)?;
                if !matches!(&current, PathState::Missing) {
                    let current = validate_existing_managed_link(&child, central_root, true)?;
                    mutations.push(PendingMutation {
                        target_id: item.target_id.clone(),
                        target_index,
                        path: child,
                        allowed_root: input.allowed_root.clone(),
                        central_root: Some(central_root.clone()),
                        expected_before_fingerprint: current.fingerprint(),
                        expected_after_fingerprint: PathState::Missing.fingerprint(),
                        mutation: Mutation::Remove,
                    });
                }
            }
        }
    }
    Ok(mutations)
}

fn build_missing_skill_directories(
    directory: &Path,
    allowed_root: &Path,
    central_root: &Path,
    target_id: &str,
    target_index: usize,
) -> Result<Vec<PendingMutation>, AppError> {
    let relative = directory
        .strip_prefix(allowed_root)
        .map_err(|_| AppError::invalid_input("targetPath", "Skills 目录位于允许根之外"))?;
    if relative.as_os_str().is_empty() {
        return Ok(Vec::new());
    }
    let mut current = allowed_root.to_path_buf();
    let mut mutations = Vec::new();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(AppError::invalid_input(
                "targetPath",
                "Skills 目录包含相对路径片段",
            ));
        };
        current.push(segment);
        match capture_path_state(&current)? {
            PathState::Directory { .. } => {}
            PathState::Missing => mutations.push(PendingMutation {
                target_id: target_id.to_owned(),
                target_index,
                path: current.clone(),
                allowed_root: allowed_root.to_path_buf(),
                central_root: Some(central_root.to_path_buf()),
                expected_before_fingerprint: PathState::Missing.fingerprint(),
                // 新目录的设备/inode 只有 mkdir 后才能确定；apply_mutation 会
                // 把实际身份写入 durable journal，并以该身份约束回滚。
                expected_after_fingerprint: String::new(),
                mutation: Mutation::CreateDirectory,
            }),
            PathState::File { .. } | PathState::Symlink { .. } => {
                return Err(AppError::conflict(
                    "skillTarget",
                    "Skills 目录祖先被普通文件或未知链接占用",
                ));
            }
        }
    }
    Ok(mutations)
}

fn validate_child_name(name: &str) -> Result<(), AppError> {
    let path = Path::new(name);
    if name.is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(AppError::invalid_input(
            "skillName",
            "Skill 链接名称必须是单个安全路径段",
        ));
    }
    Ok(())
}

struct CreatedDirectory {
    parent: File,
    name: CString,
}

fn create_private_directory_nofollow(
    path: &Path,
    allowed_root: &Path,
) -> Result<CreatedDirectory, AppError> {
    validate_allowed_path(path, allowed_root, false)?;
    let relative = path
        .strip_prefix(allowed_root)
        .map_err(|_| AppError::invalid_input("targetPath", "Skills 目录位于允许根之外"))?;
    let name = relative
        .file_name()
        .ok_or_else(|| AppError::invalid_input("targetPath", "Skills 目录缺少名称"))?;
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new(""));
    let mut parent = open_directory_nofollow(allowed_root, "open_skill_allowed_root")?;
    let mut display = allowed_root.to_path_buf();
    for component in parent_relative.components() {
        let Component::Normal(segment) = component else {
            return Err(AppError::invalid_input(
                "targetPath",
                "Skills 目录父路径包含相对片段",
            ));
        };
        display.push(segment);
        parent = open_directory_at_nofollow(&parent, segment, &display)?;
    }
    let name_c = c_path_segment(name, "targetPath")?;
    // SAFETY: parent fd 在调用期间有效，name_c 是单个 NUL 结尾路径段。
    let created = unsafe { libc::mkdirat(parent.as_raw_fd(), name_c.as_ptr(), 0o700) };
    if created != 0 {
        let error = io::Error::last_os_error();
        return Err(match error.kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), "mkdirat_skill_target")
            }
            io::ErrorKind::AlreadyExists => {
                AppError::stale_preview("persisted", &path.to_string_lossy())
            }
            _ => AppError::atomic_write(&path.to_string_lossy(), "create_skill_target_directory"),
        });
    }
    Ok(CreatedDirectory {
        parent,
        name: name_c,
    })
}

fn finalize_created_directory(created: CreatedDirectory, path: &Path) -> Result<(), AppError> {
    // SAFETY: parent fd 在调用期间有效，name 是刚由 mkdirat 创建的单一路径段。
    let descriptor = unsafe {
        libc::openat(
            created.parent.as_raw_fd(),
            created.name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(AppError::atomic_write(
            &path.to_string_lossy(),
            "open_created_skill_directory",
        ));
    }
    // SAFETY: descriptor 是本函数刚取得且尚未被其他所有者接管的有效 fd。
    let directory = unsafe { File::from_raw_fd(descriptor) };
    // SAFETY: directory fd 是本函数持有的有效目录描述符；权限只会收紧到 0700。
    if unsafe { libc::fchmod(directory.as_raw_fd(), 0o700) } != 0 {
        return Err(AppError::permission(
            &path.to_string_lossy(),
            "chmod_skill_target_directory",
        ));
    }
    directory
        .sync_all()
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "sync_skill_directory"))?;
    created.parent.sync_all().map_err(|_| {
        AppError::atomic_write(
            &path
                .parent()
                .expect("Skills 目录必须有父目录")
                .to_string_lossy(),
            "sync_skill_directory_parent",
        )
    })
}

fn c_path_segment(segment: &OsStr, field: &'static str) -> Result<CString, AppError> {
    if segment.as_bytes().contains(&b'/') {
        return Err(AppError::invalid_input(field, "路径段不能包含分隔符"));
    }
    CString::new(segment.as_bytes())
        .map_err(|_| AppError::invalid_input(field, "路径段不能包含 NUL"))
}

fn open_directory_nofollow(path: &Path, operation: &'static str) -> Result<File, AppError> {
    let path_c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| AppError::invalid_input("allowedRoot", "路径不能包含 NUL"))?;
    // SAFETY: path_c 是合法 C 路径；成功返回的 fd 立即交给 File 管理。
    let descriptor = unsafe {
        libc::open(
            path_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(match io::Error::last_os_error().kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), operation)
            }
            _ => AppError::conflict("targetPath", "Skills 目录祖先无法安全打开"),
        });
    }
    // SAFETY: descriptor 是本函数刚取得且尚未被其他所有者接管的有效 fd。
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn open_directory_at_nofollow(
    parent: &File,
    name: &OsStr,
    display_path: &Path,
) -> Result<File, AppError> {
    let name_c = c_path_segment(name, "targetPath")?;
    // SAFETY: parent fd 在调用期间有效，O_NOFOLLOW 阻止路径段链接逃逸。
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(match io::Error::last_os_error().kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&display_path.to_string_lossy(), "openat_skill_parent")
            }
            _ => AppError::conflict("targetPath", "Skills 目录祖先已变化、缺失或变为链接"),
        });
    }
    // SAFETY: descriptor 是本函数刚取得且尚未被其他所有者接管的有效 fd。
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn validate_existing_managed_link(
    path: &Path,
    central_root: &Path,
    require_existing: bool,
) -> Result<PathState, AppError> {
    match capture_path_state(path)? {
        state @ PathState::Missing if !require_existing => Ok(state),
        PathState::Symlink { link_target } => {
            validate_central_link_target(path, &link_target, central_root)?;
            Ok(PathState::Symlink { link_target })
        }
        PathState::Missing => Err(AppError::stale_preview(
            "persisted",
            &path.to_string_lossy(),
        )),
        PathState::Directory { .. } | PathState::File { .. } => Err(AppError::conflict(
            "skillTarget",
            "普通目录或文件占用 Skill 目标，拒绝覆盖或删除",
        )),
    }
}

fn flatten_mutations(work: &[TargetWork<'_>]) -> Result<Vec<PendingMutation>, AppError> {
    let mut seen = BTreeSet::new();
    let mut flattened = Vec::new();
    for target in work {
        for mutation in &target.mutations {
            if !seen.insert(mutation.path.clone()) {
                return Err(AppError::conflict(
                    "targetPath",
                    "多个 Preview 项会修改同一路径",
                ));
            }
            flattened.push(mutation.clone());
        }
    }
    Ok(flattened)
}

fn validate_allowed_path(
    path: &Path,
    allowed_root: &Path,
    allow_root_target: bool,
) -> Result<(), AppError> {
    validate_normal_absolute(path, "targetPath")?;
    validate_normal_absolute(allowed_root, "allowedRoot")?;
    let root_metadata = fs::symlink_metadata(allowed_root).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => {
            AppError::not_found("allowedRoot", &allowed_root.to_string_lossy())
        }
        io::ErrorKind::PermissionDenied => {
            AppError::permission(&allowed_root.to_string_lossy(), "lstat_allowed_root")
        }
        _ => AppError::invalid_input("allowedRoot", "写入根无法安全读取"),
    })?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(AppError::conflict(
            "allowedRoot",
            "写入根必须是无链接的真实目录",
        ));
    }
    let canonical_root = fs::canonicalize(allowed_root).map_err(|_| {
        AppError::permission(&allowed_root.to_string_lossy(), "canonicalize_allowed_root")
    })?;
    if canonical_root != allowed_root {
        return Err(AppError::conflict(
            "allowedRoot",
            "写入根不是 canonical 路径",
        ));
    }
    let relative = path
        .strip_prefix(allowed_root)
        .map_err(|_| AppError::invalid_input("targetPath", "目标位于允许写入根之外"))?;
    if relative.as_os_str().is_empty() && !allow_root_target {
        return Err(AppError::invalid_input(
            "targetPath",
            "文件目标不能覆盖允许写入根本身",
        ));
    }
    let parent = if relative.as_os_str().is_empty() {
        allowed_root
    } else {
        path.parent()
            .ok_or_else(|| AppError::invalid_input("targetPath", "目标缺少父目录"))?
    };
    let mut current = allowed_root.to_path_buf();
    if parent != allowed_root {
        let parent_relative = parent
            .strip_prefix(allowed_root)
            .map_err(|_| AppError::invalid_input("targetPath", "目标父目录越界"))?;
        for component in parent_relative.components() {
            let Component::Normal(segment) = component else {
                return Err(AppError::invalid_input(
                    "targetPath",
                    "目标父目录包含相对片段",
                ));
            };
            current.push(segment);
            let metadata = match fs::symlink_metadata(&current) {
                Ok(metadata) => metadata,
                // 缺失祖先本身不是越界证据。Skills Apply 会把每层 mkdir
                // 作为独立快照 mutation，并在真正创建前重新验证已有祖先。
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    return Err(AppError::permission(
                        &current.to_string_lossy(),
                        "lstat_target_parent",
                    ));
                }
                Err(_) => {
                    return Err(AppError::invalid_input(
                        "targetPath",
                        "目标父目录无法安全读取",
                    ));
                }
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AppError::conflict(
                    "targetPath",
                    "目标祖先包含未知链接或非目录入口",
                ));
            }
        }
    }
    Ok(())
}

fn validate_normal_absolute(path: &Path, field: &'static str) -> Result<(), AppError> {
    if !path.is_absolute()
        || path == Path::new("/")
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::invalid_input(
            field,
            "路径必须是无相对片段的非根绝对路径",
        ));
    }
    Ok(())
}

fn capture_path_state(path: &Path) -> Result<PathState, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(PathState::Missing),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return Err(AppError::permission(
                &path.to_string_lossy(),
                "lstat_target",
            ));
        }
        Err(_) => {
            return Err(AppError::atomic_write(
                &path.to_string_lossy(),
                "lstat_target",
            ));
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let link_target = fs::read_link(path)
            .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "read_link"))?;
        return Ok(PathState::Symlink { link_target });
    }
    if metadata.is_file() {
        let bytes = fs::read(path).map_err(|error| match error.kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), "read_target")
            }
            _ => AppError::atomic_write(&path.to_string_lossy(), "read_target"),
        })?;
        return Ok(PathState::File {
            hash: hash_bytes(&bytes),
            bytes,
            mode: metadata.permissions().mode() & 0o7777,
        });
    }
    if metadata.is_dir() {
        return Ok(PathState::Directory {
            device: metadata.dev(),
            inode: metadata.ino(),
        });
    }
    Err(AppError::conflict("targetPath", "目标是未知特殊文件类型"))
}

fn verify_expected_path_state(
    path: &Path,
    expected: ExpectedPathFingerprint<'_>,
) -> Result<PathState, AppError> {
    let current = capture_path_state(path)?;
    if current.fingerprint() != expected.fingerprint {
        return Err(AppError::stale_preview(expected.run_id, expected.target_id));
    }
    Ok(current)
}

fn create_snapshot(
    database: &mut Database,
    paths: &AppPaths,
    request: SnapshotRequest<'_>,
) -> Result<SnapshotRecord, AppError> {
    let SnapshotRequest {
        run_id,
        target_id,
        target_path,
        allowed_root,
        central_root,
        expected_before_fingerprint,
    } = request;
    validate_allowed_path(target_path, allowed_root, false)?;
    let state = capture_path_state(target_path)?;
    if state.fingerprint() != expected_before_fingerprint {
        let target = target_id
            .map(str::to_owned)
            .unwrap_or_else(|| target_path.to_string_lossy().into_owned());
        return Err(AppError::stale_preview(run_id, &target));
    }
    let snapshot_id = Uuid::new_v4().to_string();
    let run_directory = paths.snapshots().join(run_id);
    ensure_private_directory(&run_directory)?;
    // run 目录本身也必须在快照根中 durable，不能只 fsync 其内部文件。
    sync_directory(paths.snapshots())?;
    let snapshot_path = run_directory.join(format!("{snapshot_id}.snapshot"));
    let mut snapshot_file = create_private_file(&snapshot_path)?;
    if let PathState::File { bytes, .. } = &state {
        snapshot_file.write_all(bytes).map_err(|_| {
            AppError::atomic_write(&snapshot_path.to_string_lossy(), "write_snapshot")
        })?;
    }
    snapshot_file
        .flush()
        .map_err(|_| AppError::atomic_write(&snapshot_path.to_string_lossy(), "flush_snapshot"))?;
    snapshot_file
        .sync_all()
        .map_err(|_| AppError::atomic_write(&snapshot_path.to_string_lossy(), "sync_snapshot"))?;
    ensure_private_file(&snapshot_path)?;
    sync_directory(&run_directory)?;

    let database_path = database.path().to_string_lossy().into_owned();
    let insert = database.connection_mut().execute(
        "INSERT INTO snapshots(
            id, run_id, target_id, target_path, snapshot_path, content_hash,
            file_mode, target_type, link_target
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            snapshot_id,
            run_id,
            target_id,
            target_path.to_string_lossy(),
            snapshot_path.to_string_lossy(),
            state.content_hash(),
            state.mode().map(i64::from),
            state.target_type().as_str(),
            state.link_target().map(|target| target.to_string_lossy()),
        ],
    );
    if insert.is_err() {
        let _ = fs::remove_file(&snapshot_path);
        return Err(AppError::database(&database_path, "insert_snapshot"));
    }
    Ok(SnapshotRecord {
        id: snapshot_id,
        run_id: run_id.to_owned(),
        target_id: target_id.map(str::to_owned),
        target_path: target_path.to_path_buf(),
        snapshot_path,
        allowed_root: allowed_root.to_path_buf(),
        central_root: central_root.map(Path::to_path_buf),
        row_version: 1,
        state,
    })
}

fn persist_journal(paths: &AppPaths, journal: &RunJournal) -> Result<(), AppError> {
    let journal_path = paths.journals().join(format!("{}.json", journal.run_id));
    validate_allowed_path(&journal_path, paths.journals(), false)?;
    let bytes = serde_json::to_vec_pretty(journal).map_err(|_| {
        AppError::atomic_write(&journal_path.to_string_lossy(), "serialize_journal")
    })?;
    atomic_replace_file(
        &journal_path,
        &bytes,
        PRIVATE_FILE_MODE,
        paths.journals(),
        None,
        None,
    )
    .map_err(|failure| match failure {
        MutationFailure::Error(error) | MutationFailure::Crash(error) => error,
    })
}

fn apply_mutation(
    paths: &AppPaths,
    journal: &mut RunJournal,
    journal_index: usize,
    mutation: &PendingMutation,
    expected_before_fingerprint: &str,
    fault: &dyn ApplyFaultInjector,
) -> Result<(), MutationFailure> {
    validate_allowed_path(&mutation.path, &mutation.allowed_root, false)?;
    let expected_run_id = journal.run_id.clone();
    let expected_target_id = mutation.target_id.clone();
    let expected = ExpectedPathFingerprint {
        run_id: &expected_run_id,
        target_id: &expected_target_id,
        fingerprint: expected_before_fingerprint,
    };
    verify_expected_path_state(&mutation.path, expected)?;
    journal.targets[journal_index].phase = "writing".to_owned();
    persist_journal(paths, journal)?;
    match &mutation.mutation {
        Mutation::CreateDirectory => {
            if !matches!(
                verify_expected_path_state(&mutation.path, expected)?,
                PathState::Missing
            ) {
                return Err(AppError::stale_preview(&expected_run_id, &expected_target_id).into());
            }
            journal.targets[journal_index].phase = "directory_create_pending".to_owned();
            persist_journal(paths, journal)?;
            let created =
                match create_private_directory_nofollow(&mutation.path, &mutation.allowed_root) {
                    Ok(created) => created,
                    Err(error) => {
                        journal.targets[journal_index].phase = "directory_create_failed".to_owned();
                        persist_journal(paths, journal)?;
                        return Err(error.into());
                    }
                };
            journal.targets[journal_index].phase = "directory_created".to_owned();
            persist_journal(paths, journal)?;
            finalize_created_directory(created, &mutation.path)?;
        }
        Mutation::WriteFile { bytes, mode } => atomic_replace_file(
            &mutation.path,
            bytes,
            *mode,
            &mutation.allowed_root,
            Some(expected),
            Some((mutation.target_index, fault, paths, journal, journal_index)),
        )?,
        Mutation::Remove => {
            validate_allowed_path(&mutation.path, &mutation.allowed_root, false)?;
            let state = verify_expected_path_state(&mutation.path, expected)?;
            match state {
                PathState::File { .. } => {
                    fs::remove_file(&mutation.path).map_err(|_| {
                        AppError::atomic_write(&mutation.path.to_string_lossy(), "remove_target")
                    })?;
                    journal.targets[journal_index].phase = "removed".to_owned();
                    sync_directory(mutation.path.parent().expect("目标必须有父目录"))?;
                }
                PathState::Symlink { link_target } => {
                    let central_root = mutation.central_root.as_deref().ok_or_else(|| {
                        AppError::conflict("targetPath", "没有中央库所有权证据时拒绝删除链接")
                    })?;
                    validate_central_link_target(&mutation.path, &link_target, central_root)?;
                    fs::remove_file(&mutation.path).map_err(|_| {
                        AppError::atomic_write(&mutation.path.to_string_lossy(), "remove_symlink")
                    })?;
                    journal.targets[journal_index].phase = "removed".to_owned();
                    sync_directory(mutation.path.parent().expect("目标必须有父目录"))?;
                }
                PathState::Missing => {}
                PathState::Directory { .. } => {
                    if mutation.central_root.is_none() {
                        return Err(AppError::conflict("targetPath", "拒绝删除普通目录").into());
                    }
                    fs::remove_dir(&mutation.path).map_err(|_| {
                        AppError::conflict(
                            "targetPath",
                            "只允许删除由 Skills Apply 创建且仍为空的目录",
                        )
                    })?;
                    journal.targets[journal_index].phase = "removed".to_owned();
                    sync_directory(mutation.path.parent().expect("目标必须有父目录"))?;
                }
            }
        }
        Mutation::ReplaceSymlink {
            link_target,
            central_root,
        } => atomic_replace_symlink(
            &mutation.path,
            link_target,
            central_root,
            &mutation.allowed_root,
            expected,
            mutation.target_index,
            fault,
            paths,
            journal,
            journal_index,
        )?,
    }
    let state = capture_path_state(&mutation.path)?;
    let expected_after_fingerprint = if matches!(&mutation.mutation, Mutation::CreateDirectory) {
        if !matches!(state, PathState::Directory { .. }) {
            journal.targets[journal_index].phase = "external_change_after_write".to_owned();
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Error(AppError::stale_preview(
                &expected_run_id,
                &expected_target_id,
            )));
        }
        state.fingerprint()
    } else {
        mutation.expected_after_fingerprint.clone()
    };
    if state.fingerprint() != expected_after_fingerprint {
        journal.targets[journal_index].phase = "external_change_after_write".to_owned();
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        persist_journal(paths, journal)?;
        return Err(MutationFailure::Error(AppError::stale_preview(
            &expected_run_id,
            &expected_target_id,
        )));
    }
    journal.targets[journal_index].phase = "written".to_owned();
    journal.targets[journal_index].after_fingerprint = Some(expected_after_fingerprint);
    journal.targets[journal_index].temporary_path = None;
    journal.targets[journal_index].temporary_fingerprint = None;
    persist_journal(paths, journal)?;
    match fault.decide(&ApplyFaultEvent::AfterTarget {
        index: mutation.target_index,
        path: mutation.path.clone(),
    }) {
        ApplyFaultDecision::Continue => Ok(()),
        ApplyFaultDecision::Fail => Err(MutationFailure::Error(AppError::atomic_write(
            &mutation.path.to_string_lossy(),
            "fault_after_target",
        ))),
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = "crashed_after_target".to_owned();
            persist_journal(paths, journal)?;
            Err(MutationFailure::Crash(AppError::atomic_write(
                &mutation.path.to_string_lossy(),
                "simulated_crash_after_target",
            )))
        }
    }
}

type RenameFaultContext<'a> = (
    usize,
    &'a dyn ApplyFaultInjector,
    &'a AppPaths,
    &'a mut RunJournal,
    usize,
);

fn atomic_replace_file(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    allowed_root: &Path,
    expected_current: Option<ExpectedPathFingerprint<'_>>,
    fault_context: Option<RenameFaultContext<'_>>,
) -> Result<(), MutationFailure> {
    validate_allowed_path(path, allowed_root, false)?;
    let current = match expected_current {
        Some(expected) => verify_expected_path_state(path, expected)?,
        None => capture_path_state(path)?,
    };
    match current {
        PathState::Missing | PathState::File { .. } => {}
        PathState::Directory { .. } | PathState::Symlink { .. } => {
            return Err(AppError::conflict("targetPath", "文件原子写拒绝覆盖目录或链接").into());
        }
    }
    let parent = path.parent().expect("已验证的目标必须有父目录");
    let temporary = parent.join(format!(".easytoagents-{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE_MODE)
        .open(&temporary)
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "create_temporary"))?;
    if let Err(error) = file
        .write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .and_then(|_| fs::set_permissions(&temporary, fs::Permissions::from_mode(mode & 0o7777)))
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&temporary);
        let _ = error;
        return Err(AppError::atomic_write(&path.to_string_lossy(), "flush_temporary").into());
    }
    drop(file);
    if let Some((index, fault, paths, journal, journal_index)) = fault_context {
        journal.targets[journal_index].phase = "rename_pending".to_owned();
        journal.targets[journal_index].temporary_path =
            Some(temporary.to_string_lossy().into_owned());
        journal.targets[journal_index].temporary_fingerprint =
            Some(capture_path_state(&temporary)?.fingerprint());
        if let Err(error) = persist_journal(paths, journal) {
            let _ = fs::remove_file(&temporary);
            let _ = sync_directory(parent);
            journal.targets[journal_index].temporary_path = None;
            journal.targets[journal_index].temporary_fingerprint = None;
            return Err(MutationFailure::Error(error));
        }
        match fault.decide(&ApplyFaultEvent::BeforeRename {
            index,
            path: path.to_path_buf(),
        }) {
            ApplyFaultDecision::Continue => {}
            ApplyFaultDecision::Fail => {
                let _ = fs::remove_file(&temporary);
                let _ = sync_directory(parent);
                journal.targets[journal_index].phase = "rename_failed".to_owned();
                journal.targets[journal_index].temporary_path = None;
                journal.targets[journal_index].temporary_fingerprint = None;
                return Err(MutationFailure::Error(AppError::atomic_write(
                    &path.to_string_lossy(),
                    "fault_before_rename",
                )));
            }
            ApplyFaultDecision::Crash => {
                journal.targets[journal_index].phase = "crashed_before_rename".to_owned();
                persist_journal(paths, journal)?;
                return Err(MutationFailure::Crash(AppError::atomic_write(
                    &path.to_string_lossy(),
                    "simulated_crash_before_rename",
                )));
            }
        }
        validate_allowed_path(path, allowed_root, false)?;
        if let Some(expected) = expected_current {
            if let Err(error) = verify_expected_path_state(path, expected) {
                let _ = fs::remove_file(&temporary);
                let _ = sync_directory(parent);
                journal.targets[journal_index].phase = "rename_failed".to_owned();
                journal.targets[journal_index].temporary_path = None;
                journal.targets[journal_index].temporary_fingerprint = None;
                persist_journal(paths, journal)?;
                return Err(MutationFailure::Error(error));
            }
        }
        if fs::rename(&temporary, path).is_err() {
            let _ = fs::remove_file(&temporary);
            let _ = sync_directory(parent);
            journal.targets[journal_index].phase = "rename_failed".to_owned();
            journal.targets[journal_index].temporary_path = None;
            journal.targets[journal_index].temporary_fingerprint = None;
            return Err(AppError::atomic_write(&path.to_string_lossy(), "rename_temporary").into());
        }
        journal.targets[journal_index].phase = "renamed".to_owned();
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        sync_directory(parent)?;
        persist_journal(paths, journal)?;
        match fault.decide(&ApplyFaultEvent::AfterRename {
            index,
            path: path.to_path_buf(),
        }) {
            ApplyFaultDecision::Continue => {}
            ApplyFaultDecision::Fail => {
                return Err(MutationFailure::Error(AppError::atomic_write(
                    &path.to_string_lossy(),
                    "fault_after_rename",
                )));
            }
            ApplyFaultDecision::Crash => {
                journal.targets[journal_index].phase = "crashed_after_rename".to_owned();
                persist_journal(paths, journal)?;
                return Err(MutationFailure::Crash(AppError::atomic_write(
                    &path.to_string_lossy(),
                    "simulated_crash_after_rename",
                )));
            }
        }
    } else {
        if fs::rename(&temporary, path).is_err() {
            let _ = fs::remove_file(&temporary);
            let _ = sync_directory(parent);
            return Err(AppError::atomic_write(&path.to_string_lossy(), "rename_temporary").into());
        }
        sync_directory(parent)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn atomic_replace_symlink(
    path: &Path,
    link_target: &Path,
    central_root: &Path,
    allowed_root: &Path,
    expected_current: ExpectedPathFingerprint<'_>,
    index: usize,
    fault: &dyn ApplyFaultInjector,
    paths: &AppPaths,
    journal: &mut RunJournal,
    journal_index: usize,
) -> Result<(), MutationFailure> {
    validate_allowed_path(path, allowed_root, false)?;
    let canonical_target = validate_central_link_target(path, link_target, central_root)?;
    match verify_expected_path_state(path, expected_current)? {
        PathState::Missing => {}
        PathState::Symlink {
            link_target: current,
        } => {
            validate_central_link_target(path, &current, central_root)?;
        }
        PathState::File { .. } | PathState::Directory { .. } => {
            return Err(
                AppError::conflict("skillTarget", "Skill 链接拒绝覆盖普通文件或目录").into(),
            );
        }
    }
    let parent = path.parent().expect("已验证的链接必须有父目录");
    let temporary = parent.join(format!(".easytoagents-{}.link", Uuid::new_v4()));
    symlink(&canonical_target, &temporary)
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "create_temporary_symlink"))?;
    journal.targets[journal_index].phase = "rename_pending".to_owned();
    journal.targets[journal_index].temporary_path = Some(temporary.to_string_lossy().into_owned());
    journal.targets[journal_index].temporary_fingerprint =
        Some(capture_path_state(&temporary)?.fingerprint());
    if let Err(error) = persist_journal(paths, journal) {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        return Err(MutationFailure::Error(error));
    }
    match fault.decide(&ApplyFaultEvent::BeforeRename {
        index,
        path: path.to_path_buf(),
    }) {
        ApplyFaultDecision::Continue => {}
        ApplyFaultDecision::Fail => {
            let _ = fs::remove_file(&temporary);
            let _ = sync_directory(parent);
            journal.targets[journal_index].phase = "rename_failed".to_owned();
            journal.targets[journal_index].temporary_path = None;
            journal.targets[journal_index].temporary_fingerprint = None;
            return Err(MutationFailure::Error(AppError::atomic_write(
                &path.to_string_lossy(),
                "fault_before_symlink_rename",
            )));
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = "crashed_before_rename".to_owned();
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Crash(AppError::atomic_write(
                &path.to_string_lossy(),
                "simulated_crash_before_symlink_rename",
            )));
        }
    }
    validate_allowed_path(path, allowed_root, false)?;
    if let Err(error) = verify_expected_path_state(path, expected_current) {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        journal.targets[journal_index].phase = "rename_failed".to_owned();
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        persist_journal(paths, journal)?;
        return Err(MutationFailure::Error(error));
    }
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        journal.targets[journal_index].phase = "rename_failed".to_owned();
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        return Err(AppError::atomic_write(&path.to_string_lossy(), "rename_symlink").into());
    }
    journal.targets[journal_index].phase = "renamed".to_owned();
    journal.targets[journal_index].temporary_path = None;
    journal.targets[journal_index].temporary_fingerprint = None;
    sync_directory(parent)?;
    persist_journal(paths, journal)?;
    match fault.decide(&ApplyFaultEvent::AfterRename {
        index,
        path: path.to_path_buf(),
    }) {
        ApplyFaultDecision::Continue => {}
        ApplyFaultDecision::Fail => {
            return Err(MutationFailure::Error(AppError::atomic_write(
                &path.to_string_lossy(),
                "fault_after_symlink_rename",
            )));
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = "crashed_after_rename".to_owned();
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Crash(AppError::atomic_write(
                &path.to_string_lossy(),
                "simulated_crash_after_symlink_rename",
            )));
        }
    }
    Ok(())
}

fn validate_central_link_target(
    link_path: &Path,
    link_target: &Path,
    central_root: &Path,
) -> Result<PathBuf, AppError> {
    let canonical_root = fs::canonicalize(central_root)
        .map_err(|_| AppError::not_found("centralSkillsRoot", &central_root.to_string_lossy()))?;
    if canonical_root != central_root {
        return Err(AppError::conflict(
            "centralSkillsRoot",
            "中央 Skills 根不是 canonical 路径",
        ));
    }
    let resolved = if link_target.is_absolute() {
        link_target.to_path_buf()
    } else {
        link_path
            .parent()
            .expect("链接必须有父目录")
            .join(link_target)
    };
    let canonical = fs::canonicalize(&resolved)
        .map_err(|_| AppError::conflict("skillTarget", "断裂链接不能被证明为应用拥有"))?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|_| AppError::not_found("centralSkill", &canonical.to_string_lossy()))?;
    if !metadata.is_dir() || !canonical.starts_with(&canonical_root) || canonical == canonical_root
    {
        return Err(AppError::conflict(
            "skillTarget",
            "链接目标不在应用中央 Skills 库内",
        ));
    }
    Ok(canonical)
}

fn sync_directory(path: &Path) -> Result<(), AppError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "sync_directory"))
}

#[derive(Debug)]
struct TargetVerification {
    target_id: String,
    full_hash: Option<String>,
    managed_hash: Option<String>,
    projection: Value,
}

fn verify_all_targets(work: &[TargetWork<'_>]) -> Result<Vec<TargetVerification>, AppError> {
    let mut verifications = Vec::with_capacity(work.len());
    for target in work {
        if target.input.delete_target {
            if !matches!(
                capture_path_state(Path::new(&target.item.target_path))?,
                PathState::Missing
            ) {
                return Err(AppError::atomic_write(
                    &target.item.target_path,
                    "verify_deleted_target",
                ));
            }
            verifications.push(TargetVerification {
                target_id: target.item.target_id.clone(),
                full_hash: None,
                managed_hash: None,
                projection: Value::Null,
            });
            continue;
        }
        let adapter = adapter_for(target.input.descriptor.tool);
        let observed = match scan_target(adapter, &target.input.descriptor, &target.input.ownership)
        {
            TargetScan::Observed(observed) => observed,
            _ => {
                return Err(AppError::atomic_write(
                    &target.item.target_path,
                    "verify_written_target",
                ));
            }
        };
        if observed.managed_hash != target.item.envelope.desired_managed_hash {
            return Err(AppError::atomic_write(
                &target.item.target_path,
                "verify_managed_projection",
            ));
        }
        verifications.push(TargetVerification {
            target_id: target.item.target_id.clone(),
            full_hash: Some(observed.full_hash.clone()),
            managed_hash: Some(observed.managed_hash.clone()),
            projection: canonical_json(&target.input.desired_projection),
        });
    }
    Ok(verifications)
}

fn finish_successful_apply(
    database: &mut Database,
    preview: &PersistedPreview,
    inputs: &[ApplyTargetInput],
    verifications: &[TargetVerification],
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_finish_apply"))?;
    let mut versions = BTreeMap::new();
    for item in &preview.items {
        record_expected_versions(item, &mut versions)?;
    }
    verify_database_versions(&transaction, &versions, &preview.preview_id, &database_path)?;
    let inputs_by_path = inputs
        .iter()
        .map(|input| (input.descriptor.path.as_deref().unwrap_or_default(), input))
        .collect::<BTreeMap<_, _>>();
    for verification in verifications {
        let projection = serde_json::to_string(&verification.projection)
            .map_err(|_| AppError::database(&database_path, "serialize_managed_baseline"))?;
        let expected_version = preview
            .items
            .iter()
            .find(|item| item.target_id == verification.target_id)
            .map(|item| item.envelope.target_row_version)
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &verification.target_id))?;
        let updated = transaction
            .execute(
                "UPDATE managed_targets
                 SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                     baseline_projection_json = ?4, last_status = 'in_sync'
                 WHERE id = ?1 AND row_version = ?5",
                params![
                    verification.target_id,
                    verification.full_hash,
                    verification.managed_hash,
                    projection,
                    expected_version,
                ],
            )
            .map_err(|_| AppError::database(&database_path, "update_managed_baseline"))?;
        if updated != 1 {
            return Err(AppError::stale_preview(
                &preview.preview_id,
                &verification.target_id,
            ));
        }
        let preview_item = preview
            .items
            .iter()
            .find(|item| item.target_id == verification.target_id)
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &verification.target_id))?;
        let input = inputs_by_path
            .get(preview_item.target_path.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &verification.target_id))?;
        apply_managed_item_changes(
            &transaction,
            preview_item,
            input,
            &preview.preview_id,
            &database_path,
        )?;
        let item_updates = transaction
            .execute(
                "UPDATE sync_items SET status = 'in_sync', error_code = NULL
                 WHERE run_id = ?1 AND target_id = ?2",
                params![preview.preview_id, verification.target_id],
            )
            .map_err(|_| AppError::database(&database_path, "finish_sync_item"))?;
        if item_updates != 1 {
            return Err(AppError::stale_preview(
                &preview.preview_id,
                &verification.target_id,
            ));
        }
    }
    let run_updates = transaction
        .execute(
            "UPDATE sync_runs
             SET status = 'succeeded', error_code = NULL,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status = 'applying'",
            [&preview.preview_id],
        )
        .map_err(|_| AppError::database(&database_path, "finish_apply_run"))?;
    if run_updates != 1 {
        return Err(AppError::write_in_progress(
            &preview.preview_id,
            "not_applying",
        ));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_finish_apply"))
}

fn apply_managed_item_changes(
    transaction: &Transaction<'_>,
    preview_item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    preview_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    let expected_versions = preview_item
        .envelope
        .row_versions
        .iter()
        .filter(|row| row.entity_type == DatabaseEntityType::ManagedItem)
        .map(|row| (row.entity_id.as_str(), row.row_version))
        .collect::<BTreeMap<_, _>>();
    let mut ids = BTreeSet::new();
    for managed_item in &input.managed_items {
        if !ids.insert(managed_item.id.as_str())
            || managed_item.resource_kind != input.descriptor.artifact_kind
        {
            return Err(AppError::invalid_input(
                "managedItems",
                "managed item 重复或资源类型与目标不匹配",
            ));
        }
        let existing = transaction
            .query_row(
                "SELECT row_version FROM managed_items WHERE id = ?1 AND target_id = ?2",
                params![managed_item.id, preview_item.target_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|_| AppError::database(database_path, "read_managed_item_baseline"))?;
        if let Some(existing) = existing {
            let expected = expected_versions
                .get(managed_item.id.as_str())
                .copied()
                .ok_or_else(|| AppError::stale_preview(preview_id, &managed_item.id))?;
            if u32::try_from(existing).ok() != Some(expected) {
                return Err(AppError::stale_preview(preview_id, &managed_item.id));
            }
            let updated = transaction
                .execute(
                    "UPDATE managed_items
                     SET resource_kind = ?2, resource_id = ?3, external_key = ?4,
                         last_applied_item_hash = ?5
                     WHERE id = ?1 AND target_id = ?6 AND row_version = ?7",
                    params![
                        managed_item.id,
                        managed_item.resource_kind.as_str(),
                        managed_item.resource_id,
                        managed_item.external_key,
                        managed_item.last_applied_item_hash,
                        preview_item.target_id,
                        expected,
                    ],
                )
                .map_err(|_| AppError::database(database_path, "update_managed_item"))?;
            if updated != 1 {
                return Err(AppError::stale_preview(preview_id, &managed_item.id));
            }
        } else {
            transaction
                .execute(
                    "INSERT INTO managed_items(
                        id, target_id, resource_kind, resource_id, external_key,
                        last_applied_item_hash
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        managed_item.id,
                        preview_item.target_id,
                        managed_item.resource_kind.as_str(),
                        managed_item.resource_id,
                        managed_item.external_key,
                        managed_item.last_applied_item_hash,
                    ],
                )
                .map_err(|_| AppError::database(database_path, "insert_managed_item"))?;
        }
    }
    for remove_id in &input.remove_managed_item_ids {
        if !ids.insert(remove_id) {
            return Err(AppError::invalid_input(
                "managedItems",
                "同一 managed item 不能同时更新和删除",
            ));
        }
        let expected = expected_versions
            .get(remove_id.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(preview_id, remove_id))?;
        let deleted = transaction
            .execute(
                "DELETE FROM managed_items
                 WHERE id = ?1 AND target_id = ?2 AND row_version = ?3",
                params![remove_id, preview_item.target_id, expected],
            )
            .map_err(|_| AppError::database(database_path, "delete_managed_item"))?;
        if deleted != 1 {
            return Err(AppError::stale_preview(preview_id, remove_id));
        }
    }
    Ok(())
}

fn finish_failed_apply(
    database: &mut Database,
    paths: &AppPaths,
    run_id: &str,
    journal: &mut RunJournal,
    snapshots: &[SnapshotRecord],
    applied: &[usize],
    original_error: AppError,
) -> Result<ApplyResult, AppError> {
    journal.phase = "rolling_back".to_owned();
    persist_journal(paths, journal)?;
    for snapshot_index in applied.iter().rev() {
        let snapshot = &snapshots[*snapshot_index];
        let expected_after = journal.targets[*snapshot_index]
            .after_fingerprint
            .as_deref();
        if let Err(_rollback_error) = restore_snapshot_record(
            snapshot,
            expected_after,
            snapshot.central_root.as_deref(),
            &snapshot.allowed_root,
        ) {
            journal.phase = "rollback_failed".to_owned();
            journal.targets[*snapshot_index].phase = "rollback_failed".to_owned();
            persist_journal(paths, journal)?;
            update_failed_run(
                database,
                run_id,
                "rollback_failed",
                ErrorCode::RollbackFailed,
            )?;
            return Err(AppError::rollback_failed(
                run_id,
                &snapshot.target_path.to_string_lossy(),
                &snapshot.id,
            ));
        }
        journal.targets[*snapshot_index].phase = "rolled_back".to_owned();
        persist_journal(paths, journal)?;
    }
    journal.phase = "rolled_back".to_owned();
    persist_journal(paths, journal)?;
    let status = if original_error.code() == ErrorCode::StalePreview {
        "stale"
    } else {
        "rolled_back"
    };
    update_failed_run(database, run_id, status, original_error.code())?;
    Err(original_error)
}

fn update_failed_run(
    database: &mut Database,
    run_id: &str,
    status: &str,
    error_code: ErrorCode,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE sync_runs
             SET status = ?2, error_code = ?3,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status IN ('applying', 'restoring')",
            params![run_id, status, error_code.as_str()],
        )
        .map_err(|_| AppError::database(&database_path, "update_failed_run"))?;
    if updated != 1 {
        return Err(AppError::database(&database_path, "missing_active_run"));
    }
    Ok(())
}

fn restore_snapshot_record(
    snapshot: &SnapshotRecord,
    expected_current: Option<&str>,
    central_root: Option<&Path>,
    allowed_root: &Path,
) -> Result<(), AppError> {
    validate_allowed_path(&snapshot.target_path, allowed_root, false)?;
    let current = capture_path_state(&snapshot.target_path)?;
    let current_fingerprint = current.fingerprint();
    if expected_current.is_some_and(|expected| current_fingerprint != expected) {
        return Err(AppError::conflict("rollbackTarget", "回滚前目标已再次变化"));
    }
    match &snapshot.state {
        PathState::Missing => match current {
            PathState::Missing => Ok(()),
            PathState::File { .. } => {
                validate_allowed_path(&snapshot.target_path, allowed_root, false)?;
                verify_expected_path_state(
                    &snapshot.target_path,
                    ExpectedPathFingerprint {
                        run_id: &snapshot.run_id,
                        target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                        fingerprint: &current_fingerprint,
                    },
                )?;
                fs::remove_file(&snapshot.target_path).map_err(|_| {
                    AppError::atomic_write(
                        &snapshot.target_path.to_string_lossy(),
                        "rollback_remove_file",
                    )
                })?;
                sync_directory(snapshot.target_path.parent().expect("目标必须有父目录"))
            }
            PathState::Symlink { link_target } => {
                let central_root = central_root.ok_or_else(|| {
                    AppError::conflict("rollbackTarget", "没有中央库证据时拒绝删除链接")
                })?;
                validate_central_link_target(&snapshot.target_path, &link_target, central_root)?;
                validate_allowed_path(&snapshot.target_path, allowed_root, false)?;
                verify_expected_path_state(
                    &snapshot.target_path,
                    ExpectedPathFingerprint {
                        run_id: &snapshot.run_id,
                        target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                        fingerprint: &current_fingerprint,
                    },
                )?;
                fs::remove_file(&snapshot.target_path).map_err(|_| {
                    AppError::atomic_write(
                        &snapshot.target_path.to_string_lossy(),
                        "rollback_remove_symlink",
                    )
                })?;
                sync_directory(snapshot.target_path.parent().expect("目标必须有父目录"))
            }
            PathState::Directory { .. } => {
                if central_root.is_none() {
                    return Err(AppError::conflict(
                        "rollbackTarget",
                        "回滚拒绝删除未知普通目录",
                    ));
                }
                verify_expected_path_state(
                    &snapshot.target_path,
                    ExpectedPathFingerprint {
                        run_id: &snapshot.run_id,
                        target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                        fingerprint: &current_fingerprint,
                    },
                )?;
                fs::remove_dir(&snapshot.target_path).map_err(|_| {
                    AppError::conflict("rollbackTarget", "Skills 回滚只删除本次创建且仍为空的目录")
                })?;
                sync_directory(snapshot.target_path.parent().expect("目标必须有父目录"))
            }
        },
        PathState::File { bytes, mode, .. } => {
            let expected = ExpectedPathFingerprint {
                run_id: &snapshot.run_id,
                target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                fingerprint: &current_fingerprint,
            };
            atomic_replace_file(
                &snapshot.target_path,
                bytes,
                *mode,
                allowed_root,
                Some(expected),
                None,
            )
            .map_err(|failure| match failure {
                MutationFailure::Error(error) | MutationFailure::Crash(error) => error,
            })
        }
        PathState::Symlink { link_target } => {
            let central_root = central_root.ok_or_else(|| {
                AppError::conflict("rollbackTarget", "没有中央库证据时拒绝恢复链接")
            })?;
            // 恢复路径只由 restore_snapshot 的 journal 包装调用；普通 Apply 的链接回滚
            // 会在下方专用函数中使用真实 journal。这里直接执行同目录临时链接替换。
            replace_symlink_without_journal(
                &snapshot.target_path,
                link_target,
                central_root,
                allowed_root,
                ExpectedPathFingerprint {
                    run_id: &snapshot.run_id,
                    target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                    fingerprint: &current_fingerprint,
                },
            )
        }
        PathState::Directory { .. } => Err(AppError::conflict(
            "rollbackTarget",
            "目录快照只记录占位信息，不能递归恢复",
        )),
    }
}

fn replace_symlink_without_journal(
    path: &Path,
    link_target: &Path,
    central_root: &Path,
    allowed_root: &Path,
    expected_current: ExpectedPathFingerprint<'_>,
) -> Result<(), AppError> {
    validate_allowed_path(path, allowed_root, false)?;
    let canonical_target = validate_central_link_target(path, link_target, central_root)?;
    match verify_expected_path_state(path, expected_current)? {
        PathState::Missing => {}
        PathState::Symlink { link_target } => {
            validate_central_link_target(path, &link_target, central_root)?;
        }
        PathState::File { .. } | PathState::Directory { .. } => {
            return Err(AppError::conflict(
                "skillTarget",
                "拒绝用链接覆盖普通文件或目录",
            ));
        }
    }
    let parent = path.parent().expect("链接目标必须有父目录");
    let temporary = parent.join(format!(".easytoagents-{}.link", Uuid::new_v4()));
    symlink(canonical_target, &temporary)
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "create_restore_symlink"))?;
    validate_allowed_path(path, allowed_root, false)?;
    if let Err(error) = verify_expected_path_state(path, expected_current) {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        return Err(error);
    }
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        return Err(AppError::atomic_write(
            &path.to_string_lossy(),
            "rename_restore_symlink",
        ));
    }
    sync_directory(parent)
}

pub fn list_snapshots(database: &Database) -> Result<Vec<SnapshotSummary>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, run_id, target_id, target_path, target_type, created_at
             FROM snapshots ORDER BY created_at DESC, id DESC",
        )
        .map_err(|_| AppError::database(&database_path, "prepare_list_snapshots"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|_| AppError::database(&database_path, "query_list_snapshots"))?;
    let mut snapshots = Vec::new();
    for row in rows {
        let (snapshot_id, run_id, target_id, target_path, target_type, created_at) =
            row.map_err(|_| AppError::database(&database_path, "read_snapshot_summary"))?;
        snapshots.push(SnapshotSummary {
            snapshot_id,
            run_id,
            target_id,
            target_path,
            target_type: parse_target_type(&target_type)?,
            created_at,
        });
    }
    Ok(snapshots)
}

pub fn detect_interrupted_run(
    database: &Database,
    paths: &AppPaths,
) -> Result<Option<InterruptedRunPlan>, AppError> {
    let database_path = database.path().to_string_lossy();
    let active = database
        .connection()
        .query_row(
            "SELECT id, status, journal_path FROM sync_runs
             WHERE status IN ('applying', 'restoring', 'rollback_failed')
             ORDER BY CASE WHEN status IN ('applying', 'restoring') THEN 0 ELSE 1 END,
                      started_at
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "detect_interrupted_run"))?;
    let Some((run_id, status, journal_path)) = active else {
        return Ok(None);
    };
    let Some(journal_path) = journal_path else {
        return Ok(Some(InterruptedRunPlan {
            run_id,
            status,
            journal_available: false,
            targets: Vec::new(),
        }));
    };
    let journal_path = PathBuf::from(journal_path);
    if journal_path != paths.journals().join(format!("{run_id}.json")) {
        return Err(AppError::conflict(
            "journal",
            "活动 run 的 journal 路径与 run 身份不一致",
        ));
    }
    validate_allowed_path(&journal_path, paths.journals(), false)?;
    let metadata = match fs::symlink_metadata(&journal_path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Some(InterruptedRunPlan {
                run_id,
                status,
                journal_available: false,
                targets: Vec::new(),
            }));
        }
        _ => {
            return Err(AppError::permission(
                &journal_path.to_string_lossy(),
                "read_interrupted_journal",
            ));
        }
    };
    if metadata.permissions().mode() & 0o777 != PRIVATE_FILE_MODE {
        ensure_private_file(&journal_path)?;
    }
    let bytes = fs::read(&journal_path)
        .map_err(|_| AppError::permission(&journal_path.to_string_lossy(), "read_journal"))?;
    let journal: RunJournal = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::parse(&journal_path.to_string_lossy(), "journal"))?;
    if journal.run_id != run_id {
        return Err(AppError::conflict(
            "journal",
            "journal 与活动 run 标识不一致",
        ));
    }
    let mut targets = Vec::new();
    for target in journal.targets {
        let state = capture_path_state(Path::new(&target.target_path));
        let (current_type, current_fingerprint, error_code) = match state {
            Ok(state) => (Some(state.target_type()), Some(state.fingerprint()), None),
            Err(error) => (None, None, Some(error.code())),
        };
        targets.push(InterruptedTargetPlan {
            target_id: target.target_id,
            target_path: target.target_path,
            snapshot_id: target.snapshot_id,
            phase: target.phase,
            current_type,
            current_fingerprint,
            error_code,
        });
    }
    Ok(Some(InterruptedRunPlan {
        run_id,
        status,
        journal_available: true,
        targets,
    }))
}

pub fn preview_restore(
    database: &mut Database,
    paths: &AppPaths,
    snapshot_id: &str,
    allowed_root: &Path,
) -> Result<RestorePreview, AppError> {
    paths.audit_permissions()?;
    let snapshot = load_snapshot_record(database, paths, snapshot_id, allowed_root, None)?;
    let target_id = snapshot.target_id.as_deref().ok_or_else(|| {
        AppError::invalid_input("snapshotId", "旧快照缺少受管目标身份，不能自动恢复")
    })?;
    let current = capture_path_state(&snapshot.target_path)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let target_identity = database
        .connection()
        .query_row(
            "SELECT target.row_version, target.tool, target.artifact_kind, target.scope,
                    target.project_id, target.target_path, project.root_path,
                    item.redacted_diff_json
             FROM managed_targets AS target
             JOIN sync_items AS item ON item.target_id = target.id AND item.run_id = ?2
             LEFT JOIN projects AS project ON project.id = target.project_id
             WHERE target.id = ?1",
            params![target_id, snapshot.run_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "load_restore_target_identity"))?
        .ok_or_else(|| AppError::not_found("managedTarget", target_id))?;
    let mut envelope: PersistedPreviewEnvelope = serde_json::from_str(&target_identity.7)
        .map_err(|_| AppError::database(&database_path, "parse_restore_descriptor"))?;
    if target_identity.1 != envelope.descriptor.tool.as_str()
        || target_identity.2 != envelope.descriptor.artifact_kind.as_str()
        || target_identity.3 != envelope.descriptor.scope.as_str()
        || envelope.descriptor.path.as_deref() != Some(target_identity.5.as_str())
        || target_identity.6 != envelope.descriptor.project_root
        || (envelope.descriptor.scope == Scope::Global && target_identity.4.is_some())
    {
        return Err(AppError::conflict(
            "snapshot",
            "快照目标身份已与当前受管目标分离",
        ));
    }
    validate_snapshot_target_relationship(
        &snapshot.target_path,
        Path::new(&target_identity.5),
        &envelope,
    )?;
    let target_row_version = u32::try_from(target_identity.0)
        .map_err(|_| AppError::invalid_input("snapshot", "目标 row_version 超出安全范围"))?;
    let restore_id = Uuid::new_v4().to_string();
    envelope.current_full_hash = current.content_hash().map(str::to_owned);
    envelope.current_managed_hash = None;
    envelope.desired_managed_hash = snapshot.state.fingerprint();
    envelope.row_versions.clear();
    envelope.target_row_version = target_row_version;
    envelope.redacted_diff = json!({
        "snapshotId": snapshot_id,
        "targetPath": snapshot.target_path,
        "currentType": current.target_type(),
        "snapshotType": snapshot.state.target_type(),
    });
    envelope.git = None;
    envelope.exclude_from_git = false;
    envelope.restore_snapshot_id = Some(snapshot_id.to_owned());
    envelope.restore_snapshot_row_version = Some(snapshot.row_version);
    envelope.restore_current_fingerprint = Some(current.fingerprint());
    envelope.restore_target_path = Some(snapshot.target_path.to_string_lossy().into_owned());
    envelope.allowed_root = Some(allowed_root.to_string_lossy().into_owned());
    let envelope_json = serde_json::to_string(&envelope)
        .map_err(|_| AppError::database(&database_path, "serialize_restore_preview"))?;
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_restore_preview"))?;
    transaction
        .execute(
            "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version)
             VALUES (?1, 'restore', 'previewed', ?2, ?3, ?4)",
            params![
                restore_id,
                target_identity.3,
                target_identity.4,
                envelope.target_row_version
            ],
        )
        .map_err(|_| AppError::database(&database_path, "insert_restore_preview"))?;
    transaction
        .execute(
            "INSERT INTO sync_items(
                id, run_id, target_id, change_kind, status,
                redacted_diff_json, warning_codes_json
             ) VALUES (?1, ?2, ?3, ?4, 'in_sync', ?5, '[]')",
            params![
                Uuid::new_v4().to_string(),
                restore_id,
                target_id,
                restore_change_kind(&current, &snapshot.state).as_str(),
                envelope_json,
            ],
        )
        .map_err(|_| AppError::database(&database_path, "insert_restore_preview_item"))?;
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_restore_preview"))?;
    Ok(RestorePreview {
        preview_id: restore_id,
        snapshot_id: snapshot_id.to_owned(),
        target_path: snapshot.target_path.to_string_lossy().into_owned(),
        current_type: current.target_type(),
        snapshot_type: snapshot.state.target_type(),
    })
}

fn restore_change_kind(current: &PathState, snapshot: &PathState) -> ChangeKind {
    match (current, snapshot) {
        (PathState::Missing, PathState::Missing) => ChangeKind::Unchanged,
        (PathState::Missing, _) => ChangeKind::Add,
        (_, PathState::Missing) => ChangeKind::Delete,
        _ => ChangeKind::Update,
    }
}

fn validate_snapshot_target_relationship(
    snapshot_path: &Path,
    managed_target_path: &Path,
    envelope: &PersistedPreviewEnvelope,
) -> Result<(), AppError> {
    if snapshot_path == managed_target_path
        || envelope.restore_target_path.as_deref() == Some(snapshot_path.to_string_lossy().as_ref())
    {
        return Ok(());
    }
    if envelope.descriptor.format == TargetFormat::SymlinkDirectory {
        let managed_name = snapshot_path.file_name().and_then(|name| name.to_str());
        let is_managed_child = snapshot_path.parent() == Some(managed_target_path)
            && matches!(
                (&envelope.ownership, managed_name),
                (ManagedOwnership::SymlinkNames(names), Some(name)) if names.iter().any(|item| item == name)
            );
        if is_managed_child {
            return Ok(());
        }
    }
    if envelope.exclude_from_git {
        let project_root = envelope
            .descriptor
            .project_root
            .as_deref()
            .ok_or_else(|| AppError::conflict("snapshot", "Git exclude 快照缺少项目根身份"))?;
        let project_root = ProjectRoot::parse(Path::new(project_root))?;
        if resolve_local_exclude(&project_root)? == snapshot_path {
            return Ok(());
        }
    }
    Err(AppError::conflict(
        "snapshot",
        "快照路径不是受管主目标、受管子链接或已确认的 Git exclude",
    ))
}

pub fn restore_snapshot(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    restore_preview_id: &str,
    allowed_root: &Path,
    central_root: Option<&Path>,
) -> Result<ApplyResult, AppError> {
    let _write_guard = write_operations
        .lock()
        .map_err(|_| AppError::new(ErrorCode::WriteInProgress, "写入互斥锁不可用", false))?;
    paths.audit_permissions()?;
    let preview = load_persisted_preview(database, restore_preview_id)?;
    if preview.items.len() != 1 {
        return Err(AppError::invalid_input(
            "restorePreview",
            "恢复预览必须只包含一个目标",
        ));
    }
    let item = &preview.items[0];
    let snapshot_id = item
        .envelope
        .restore_snapshot_id
        .as_deref()
        .ok_or_else(|| AppError::invalid_input("restorePreview", "恢复预览缺少 snapshotId"))?;
    if item.envelope.allowed_root.as_deref() != Some(&allowed_root.to_string_lossy()) {
        return Err(AppError::stale_preview(restore_preview_id, "allowedRoot"));
    }
    let snapshot = load_snapshot_record(database, paths, snapshot_id, allowed_root, central_root)?;
    let database_path = database.path().to_string_lossy().into_owned();
    validate_restore_identity(
        database.connection(),
        item,
        &snapshot,
        restore_preview_id,
        &database_path,
    )?;
    let current = capture_path_state(&snapshot.target_path)?;
    if item.envelope.restore_current_fingerprint.as_deref() != Some(&current.fingerprint()) {
        return Err(AppError::stale_preview(
            restore_preview_id,
            &snapshot.target_path.to_string_lossy(),
        ));
    }
    let journal_path = paths.journals().join(format!("{restore_preview_id}.json"));
    let source_run_requires_resolution = claim_restore(
        database,
        restore_preview_id,
        &snapshot.run_id,
        &journal_path,
        item,
        &snapshot,
    )?;
    let result = restore_claimed_snapshot(
        database,
        paths,
        ClaimedRestoreContext {
            restore_preview_id,
            item,
            snapshot: &snapshot,
            allowed_root,
            central_root,
            source_run_requires_resolution,
        },
    );
    if let Err(error) = &result {
        if !journal_reports_crash(paths, restore_preview_id)
            && !run_has_snapshots(database, restore_preview_id).unwrap_or(true)
        {
            settle_unhandled_apply_error(database, restore_preview_id, error.code())?;
        }
    }
    result
}

struct ClaimedRestoreContext<'a> {
    restore_preview_id: &'a str,
    item: &'a PersistedPreviewItem,
    snapshot: &'a SnapshotRecord,
    allowed_root: &'a Path,
    central_root: Option<&'a Path>,
    source_run_requires_resolution: bool,
}

fn restore_claimed_snapshot(
    database: &mut Database,
    paths: &AppPaths,
    context: ClaimedRestoreContext<'_>,
) -> Result<ApplyResult, AppError> {
    let ClaimedRestoreContext {
        restore_preview_id,
        item,
        snapshot,
        allowed_root,
        central_root,
        source_run_requires_resolution,
    } = context;
    let database_path = database.path().to_string_lossy().into_owned();
    validate_restore_identity(
        database.connection(),
        item,
        snapshot,
        restore_preview_id,
        &database_path,
    )?;
    let current_fingerprint = capture_path_state(&snapshot.target_path)?.fingerprint();
    if item.envelope.restore_current_fingerprint.as_deref() != Some(&current_fingerprint) {
        update_failed_run(
            database,
            restore_preview_id,
            "stale",
            ErrorCode::StalePreview,
        )?;
        return Err(AppError::stale_preview(
            restore_preview_id,
            &snapshot.target_path.to_string_lossy(),
        ));
    }
    let mut journal = RunJournal {
        version: 1,
        run_id: restore_preview_id.to_owned(),
        operation: "restore".to_owned(),
        phase: "snapshotting".to_owned(),
        targets: Vec::new(),
    };
    persist_journal(paths, &journal)?;
    let second_snapshot = create_snapshot(
        database,
        paths,
        SnapshotRequest {
            run_id: restore_preview_id,
            target_id: snapshot.target_id.as_deref(),
            target_path: &snapshot.target_path,
            allowed_root,
            central_root,
            expected_before_fingerprint: &current_fingerprint,
        },
    )?;
    journal.targets.push(JournalTarget {
        target_id: snapshot.target_id.clone().unwrap_or_default(),
        target_path: snapshot.target_path.to_string_lossy().into_owned(),
        snapshot_id: Some(second_snapshot.id.clone()),
        snapshot_path: Some(second_snapshot.snapshot_path.to_string_lossy().into_owned()),
        phase: "snapshotted".to_owned(),
        before_fingerprint: Some(second_snapshot.state.fingerprint()),
        after_fingerprint: None,
        temporary_path: None,
        temporary_fingerprint: None,
    });
    persist_journal(paths, &journal)?;
    if let Err(error) = validate_restore_identity(
        database.connection(),
        item,
        snapshot,
        restore_preview_id,
        &database_path,
    ) {
        return finish_failed_apply(
            database,
            paths,
            restore_preview_id,
            &mut journal,
            &[second_snapshot],
            &[],
            error,
        );
    }
    if let Err(error) = cleanup_interrupted_temporaries(
        paths,
        &snapshot.run_id,
        &snapshot.target_path,
        allowed_root,
    ) {
        return finish_failed_apply(
            database,
            paths,
            restore_preview_id,
            &mut journal,
            &[second_snapshot],
            &[],
            error,
        );
    }
    if let Err(error) = validate_restore_identity(
        database.connection(),
        item,
        snapshot,
        restore_preview_id,
        &database_path,
    ) {
        return finish_failed_apply(
            database,
            paths,
            restore_preview_id,
            &mut journal,
            &[second_snapshot],
            &[],
            error,
        );
    }
    let mutation = match mutation_from_snapshot(snapshot, allowed_root, central_root) {
        Ok(mutation) => mutation,
        Err(error) => {
            return finish_failed_apply(
                database,
                paths,
                restore_preview_id,
                &mut journal,
                &[second_snapshot],
                &[],
                error,
            );
        }
    };
    let applied = match apply_mutation(
        paths,
        &mut journal,
        0,
        &mutation,
        &second_snapshot.state.fingerprint(),
        &NoApplyFault,
    ) {
        Ok(()) => vec![0],
        Err(MutationFailure::Error(error) | MutationFailure::Crash(error)) => {
            let applied = mutation_may_have_changed_target(&journal.targets[0])
                .then_some(0)
                .into_iter()
                .collect::<Vec<_>>();
            return finish_failed_apply(
                database,
                paths,
                restore_preview_id,
                &mut journal,
                &[second_snapshot],
                &applied,
                error,
            );
        }
    };
    match capture_path_state(&snapshot.target_path) {
        Ok(current) if current.fingerprint() == snapshot.state.fingerprint() => {}
        Ok(_) => {
            return finish_failed_apply(
                database,
                paths,
                restore_preview_id,
                &mut journal,
                &[second_snapshot],
                &applied,
                AppError::atomic_write(
                    &snapshot.target_path.to_string_lossy(),
                    "verify_restored_snapshot",
                ),
            );
        }
        Err(error) => {
            return finish_failed_apply(
                database,
                paths,
                restore_preview_id,
                &mut journal,
                &[second_snapshot],
                &applied,
                error,
            );
        }
    }
    journal.phase = "ready_to_finalize_database".to_owned();
    journal.targets[0].phase = "verified".to_owned();
    if let Err(error) = persist_journal(paths, &journal) {
        return finish_failed_apply(
            database,
            paths,
            restore_preview_id,
            &mut journal,
            &[second_snapshot],
            &applied,
            error,
        );
    }
    let source_run_resolved = if source_run_requires_resolution {
        match interrupted_run_matches_before_state(paths, &snapshot.run_id) {
            Ok(resolved) => resolved,
            Err(error) => {
                return finish_failed_apply(
                    database,
                    paths,
                    restore_preview_id,
                    &mut journal,
                    &[second_snapshot],
                    &applied,
                    error,
                );
            }
        }
    } else {
        false
    };
    if let Err(error) = finish_restore_success(
        database,
        restore_preview_id,
        snapshot.target_id.as_deref(),
        item.envelope.target_row_version,
        source_run_requires_resolution.then_some(snapshot.run_id.as_str()),
        source_run_resolved,
    ) {
        journal.phase = "crashed_during_database_finalize".to_owned();
        persist_journal(paths, &journal)?;
        return Err(error);
    }
    journal.phase = "succeeded".to_owned();
    let _ = persist_journal(paths, &journal);
    Ok(ApplyResult {
        run_id: restore_preview_id.to_owned(),
        status: "succeeded".to_owned(),
        applied_targets: 1,
        snapshot_count: 1,
    })
}

fn cleanup_interrupted_temporaries(
    paths: &AppPaths,
    source_run_id: &str,
    restored_target_path: &Path,
    allowed_root: &Path,
) -> Result<(), AppError> {
    let journal_path = paths.journals().join(format!("{source_run_id}.json"));
    let Ok(bytes) = fs::read(&journal_path) else {
        return Ok(());
    };
    let journal: RunJournal = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::parse(&journal_path.to_string_lossy(), "journal"))?;
    for target in journal.targets {
        if Path::new(&target.target_path) != restored_target_path {
            continue;
        }
        let Some(temporary) = target.temporary_path else {
            continue;
        };
        let expected_fingerprint = target.temporary_fingerprint.ok_or_else(|| {
            AppError::conflict("temporaryPath", "旧 journal 缺少临时路径所有权指纹")
        })?;
        let temporary = PathBuf::from(temporary);
        validate_allowed_path(&temporary, allowed_root, false)?;
        if temporary.parent() != restored_target_path.parent() {
            return Err(AppError::conflict(
                "temporaryPath",
                "journal 临时路径不在对应目标同目录",
            ));
        }
        let name = temporary
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let owned_name = name
            .strip_prefix(".easytoagents-")
            .and_then(|name| {
                name.strip_suffix(".tmp")
                    .or_else(|| name.strip_suffix(".link"))
            })
            .is_some_and(|id| Uuid::parse_str(id).is_ok());
        if !owned_name {
            return Err(AppError::conflict(
                "temporaryPath",
                "journal 中的临时路径不属于应用",
            ));
        }
        match capture_path_state(&temporary)? {
            PathState::Missing => {}
            state @ (PathState::File { .. } | PathState::Symlink { .. }) => {
                if state.fingerprint() != expected_fingerprint {
                    return Err(AppError::conflict(
                        "temporaryPath",
                        "临时路径内容已变化，拒绝删除未知内容",
                    ));
                }
                fs::remove_file(&temporary).map_err(|_| {
                    AppError::atomic_write(&temporary.to_string_lossy(), "cleanup_temporary")
                })?;
                sync_directory(temporary.parent().expect("临时路径必须有父目录"))?;
            }
            PathState::Directory { .. } => {
                return Err(AppError::conflict(
                    "temporaryPath",
                    "拒绝删除占用临时路径的目录",
                ));
            }
        }
    }
    Ok(())
}

fn claim_restore(
    database: &mut Database,
    restore_preview_id: &str,
    source_run_id: &str,
    journal_path: &Path,
    item: &PersistedPreviewItem,
    snapshot: &SnapshotRecord,
) -> Result<bool, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_claim_restore"))?;
    validate_restore_identity(
        &transaction,
        item,
        snapshot,
        restore_preview_id,
        &database_path,
    )?;
    let status = transaction
        .query_row(
            "SELECT status FROM sync_runs WHERE id = ?1 AND kind = 'restore'",
            [restore_preview_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "read_restore_claim"))?
        .ok_or_else(|| AppError::not_found("restorePreview", restore_preview_id))?;
    if status != "previewed" {
        return Err(AppError::preview_already_consumed(
            restore_preview_id,
            &status,
        ));
    }
    let mut source_run_requires_resolution = false;
    if let Some((active_id, active_status)) =
        active_writer(&transaction, Some(restore_preview_id), &database_path)?
    {
        if active_id != source_run_id {
            return Err(AppError::write_in_progress(&active_id, &active_status));
        }
        let retired = transaction
            .execute(
                "UPDATE sync_runs
                 SET status = 'rollback_failed', error_code = 'ROLLBACK_FAILED',
                     finished_at = NULL
                 WHERE id = ?1 AND status IN ('applying', 'restoring', 'rollback_failed')",
                [&active_id],
            )
            .map_err(|_| AppError::database(&database_path, "retire_interrupted_run"))?;
        if retired != 1 {
            return Err(AppError::write_in_progress(&active_id, &active_status));
        }
        source_run_requires_resolution = true;
    }
    let updated = transaction
        .execute(
            "UPDATE sync_runs SET status = 'restoring', journal_path = ?2
             WHERE id = ?1 AND kind = 'restore' AND status = 'previewed'",
            params![restore_preview_id, journal_path.to_string_lossy()],
        )
        .map_err(|_| AppError::write_in_progress(restore_preview_id, "restoring"))?;
    if updated != 1 {
        return Err(AppError::preview_already_consumed(
            restore_preview_id,
            "not_previewed",
        ));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_claim_restore"))?;
    Ok(source_run_requires_resolution)
}

fn interrupted_run_matches_before_state(paths: &AppPaths, run_id: &str) -> Result<bool, AppError> {
    let journal_path = paths.journals().join(format!("{run_id}.json"));
    validate_allowed_path(&journal_path, paths.journals(), false)?;
    let bytes = match fs::read(&journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => {
            return Err(AppError::permission(
                &journal_path.to_string_lossy(),
                "read_source_journal",
            ));
        }
    };
    let journal: RunJournal = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::parse(&journal_path.to_string_lossy(), "journal"))?;
    if journal.run_id != run_id || journal.targets.is_empty() {
        return Ok(false);
    }
    for target in journal.targets {
        let Some(expected) = target.before_fingerprint else {
            return Ok(false);
        };
        if capture_path_state(Path::new(&target.target_path))?.fingerprint() != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_restore_identity(
    connection: &rusqlite::Connection,
    item: &PersistedPreviewItem,
    snapshot: &SnapshotRecord,
    preview_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    if snapshot.target_id.as_deref() != Some(item.target_id.as_str())
        || item.envelope.restore_target_path.as_deref()
            != Some(snapshot.target_path.to_string_lossy().as_ref())
        || item.envelope.restore_snapshot_row_version != Some(snapshot.row_version)
    {
        return Err(AppError::stale_preview(preview_id, "snapshotIdentity"));
    }
    let identity = connection
        .query_row(
            "SELECT target.row_version, target.tool, target.artifact_kind, target.scope,
                    target.project_id, target.target_path, project.root_path
             FROM managed_targets AS target
             LEFT JOIN projects AS project ON project.id = target.project_id
             WHERE target.id = ?1",
            [&item.target_id],
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
        .map_err(|_| AppError::database(database_path, "verify_restore_identity"))?
        .ok_or_else(|| AppError::stale_preview(preview_id, &item.target_id))?;
    let descriptor = &item.envelope.descriptor;
    if u32::try_from(identity.0).ok() != Some(item.envelope.target_row_version)
        || identity.1 != descriptor.tool.as_str()
        || identity.2 != descriptor.artifact_kind.as_str()
        || identity.3 != descriptor.scope.as_str()
        || identity.5 != item.target_path
        || descriptor.path.as_deref() != Some(identity.5.as_str())
        || identity.6 != descriptor.project_root
        || (descriptor.scope == Scope::Global && identity.4.is_some())
    {
        return Err(AppError::stale_preview(preview_id, &item.target_id));
    }
    validate_snapshot_target_relationship(
        &snapshot.target_path,
        Path::new(&identity.5),
        &item.envelope,
    )?;
    Ok(())
}

fn mutation_from_snapshot(
    snapshot: &SnapshotRecord,
    allowed_root: &Path,
    central_root: Option<&Path>,
) -> Result<PendingMutation, AppError> {
    let mutation = match &snapshot.state {
        PathState::Missing => Mutation::Remove,
        PathState::File { bytes, mode, .. } => Mutation::WriteFile {
            bytes: bytes.clone(),
            mode: *mode,
        },
        PathState::Symlink { link_target } => Mutation::ReplaceSymlink {
            link_target: link_target.clone(),
            central_root: central_root
                .ok_or_else(|| {
                    AppError::invalid_input("centralSkillsRoot", "恢复链接必须提供中央库边界")
                })?
                .to_path_buf(),
        },
        PathState::Directory { .. } => {
            return Err(AppError::conflict("snapshot", "普通目录快照不能递归恢复"));
        }
    };
    Ok(PendingMutation {
        target_id: snapshot.target_id.clone().unwrap_or_default(),
        target_index: 0,
        path: snapshot.target_path.clone(),
        allowed_root: allowed_root.to_path_buf(),
        central_root: central_root.map(Path::to_path_buf),
        expected_before_fingerprint: capture_path_state(&snapshot.target_path)?.fingerprint(),
        expected_after_fingerprint: snapshot.state.fingerprint(),
        mutation,
    })
}

fn finish_restore_success(
    database: &mut Database,
    run_id: &str,
    target_id: Option<&str>,
    expected_target_row_version: u32,
    source_run_id: Option<&str>,
    source_run_resolved: bool,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_finish_restore"))?;
    if let Some(target_id) = target_id {
        let target_updates = transaction
            .execute(
                "UPDATE managed_targets SET last_status = 'external_owned_change'
                 WHERE id = ?1 AND row_version = ?2",
                params![target_id, expected_target_row_version],
            )
            .map_err(|_| AppError::database(&database_path, "mark_restored_target"))?;
        if target_updates != 1 {
            return Err(AppError::stale_preview(run_id, target_id));
        }
    }
    let run_updates = transaction
        .execute(
            "UPDATE sync_runs
             SET status = 'succeeded', error_code = NULL,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status = 'restoring'",
            [run_id],
        )
        .map_err(|_| AppError::database(&database_path, "finish_restore_run"))?;
    if run_updates != 1 {
        return Err(AppError::write_in_progress(run_id, "not_restoring"));
    }
    if let Some(source_run_id) = source_run_id {
        let source_updates = transaction
            .execute(
                "UPDATE sync_runs
                 SET status = CASE WHEN ?2 THEN 'rolled_back' ELSE 'rollback_failed' END,
                     error_code = CASE WHEN ?2 THEN NULL ELSE 'ROLLBACK_FAILED' END,
                     finished_at = CASE
                         WHEN ?2 THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                         ELSE NULL
                     END
                 WHERE id = ?1 AND status = 'rollback_failed'",
                params![source_run_id, source_run_resolved],
            )
            .map_err(|_| AppError::database(&database_path, "finish_source_recovery"))?;
        if source_updates != 1 {
            return Err(AppError::write_in_progress(
                source_run_id,
                "source_recovery_changed",
            ));
        }
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_finish_restore"))
}

fn load_snapshot_record(
    database: &Database,
    paths: &AppPaths,
    snapshot_id: &str,
    allowed_root: &Path,
    central_root: Option<&Path>,
) -> Result<SnapshotRecord, AppError> {
    let database_path = database.path().to_string_lossy();
    let row = database
        .connection()
        .query_row(
            "SELECT run_id, target_id, target_path, snapshot_path, content_hash,
                    file_mode, target_type, link_target, row_version
             FROM snapshots WHERE id = ?1",
            [snapshot_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "load_snapshot"))?
        .ok_or_else(|| AppError::not_found("snapshot", snapshot_id))?;
    let target_path = PathBuf::from(&row.2);
    validate_allowed_path(&target_path, allowed_root, false)?;
    let snapshot_path = PathBuf::from(&row.3);
    validate_normal_absolute(&snapshot_path, "snapshotPath")?;
    let expected_snapshot_path = paths
        .snapshots()
        .join(&row.0)
        .join(format!("{snapshot_id}.snapshot"));
    if snapshot_path != expected_snapshot_path {
        return Err(AppError::conflict(
            "snapshotPath",
            "快照路径与 run/snapshot 身份不一致",
        ));
    }
    let snapshot_parent = snapshot_path
        .parent()
        .ok_or_else(|| AppError::invalid_input("snapshotPath", "快照缺少父目录"))?;
    let canonical_parent = fs::canonicalize(snapshot_parent).map_err(|_| {
        AppError::not_found("snapshotDirectory", &snapshot_parent.to_string_lossy())
    })?;
    if canonical_parent != snapshot_parent || !canonical_parent.starts_with(paths.snapshots()) {
        return Err(AppError::conflict("snapshotPath", "快照父目录包含未知链接"));
    }
    ensure_private_file(&snapshot_path)?;
    let state = match row.6.as_str() {
        "missing" => PathState::Missing,
        "file" => {
            let bytes = fs::read(&snapshot_path).map_err(|_| {
                AppError::permission(&snapshot_path.to_string_lossy(), "read_snapshot")
            })?;
            let hash = hash_bytes(&bytes);
            if row.4.as_deref() != Some(&hash) {
                return Err(AppError::conflict("snapshot", "快照内容 hash 不匹配"));
            }
            PathState::File {
                bytes,
                hash,
                mode: row
                    .5
                    .and_then(|mode| u32::try_from(mode).ok())
                    .ok_or_else(|| AppError::invalid_input("snapshot", "文件快照缺少 mode"))?,
            }
        }
        "symlink" => {
            let link_target =
                PathBuf::from(row.7.ok_or_else(|| {
                    AppError::invalid_input("snapshot", "链接快照缺少 linkTarget")
                })?);
            if let Some(central_root) = central_root {
                validate_central_link_target(&target_path, &link_target, central_root)?;
            }
            PathState::Symlink { link_target }
        }
        // 目录快照不包含目录树，恢复路径会保守拒绝这种记录；零身份不能
        // 与任何真实目录 fingerprint 匹配。
        "directory" => PathState::Directory {
            device: 0,
            inode: 0,
        },
        _ => return Err(AppError::invalid_input("snapshot", "未知快照目标类型")),
    };
    Ok(SnapshotRecord {
        id: snapshot_id.to_owned(),
        run_id: row.0,
        target_id: row.1,
        target_path,
        snapshot_path,
        allowed_root: allowed_root.to_path_buf(),
        central_root: central_root.map(Path::to_path_buf),
        row_version: u32::try_from(row.8)
            .map_err(|_| AppError::invalid_input("snapshot", "快照 row_version 超出安全范围"))?,
        state,
    })
}

fn parse_target_type(value: &str) -> Result<TargetType, AppError> {
    match value {
        "file" => Ok(TargetType::File),
        "directory" => Ok(TargetType::Directory),
        "symlink" => Ok(TargetType::Symlink),
        "missing" => Ok(TargetType::Missing),
        _ => Err(AppError::invalid_input(
            "targetType",
            "数据库包含未知目标类型",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::symlink,
        path::{Path, PathBuf},
        process::Command,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Barrier, Mutex,
        },
        thread,
    };

    use rusqlite::params;
    use serde_json::{json, Value};
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::{
        apply_persisted_preview, claim_preview, detect_interrupted_run, list_snapshots,
        preview_restore, restore_snapshot, ApplyFaultDecision, ApplyFaultEvent, ApplyFaultInjector,
        ApplyTargetInput, ManagedItemApply, NoApplyFault,
    };
    use crate::{
        adapters::{
            CapabilityState, ManagedOwnership, PolicyState, PromptOverrideState, SymlinkPolicy,
            TargetCapability, TargetDescriptor, TargetFormat, TargetTrustState,
        },
        app::AppPaths,
        db::Database,
        domain::{ArtifactKind, ProjectRoot, Scope, Tool},
        error::ErrorCode,
        git::inspect_path,
        security::{mode, SecretRedactor, PRIVATE_DIRECTORY_MODE, PRIVATE_FILE_MODE},
        sync::{
            build_preview_plan, load_managed_target_baseline, persist_preview, scan_target,
            ManagedTargetBaseline, PreviewTargetRequest, TargetScan,
        },
    };

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        targets: PathBuf,
        paths: AppPaths,
        database: Database,
        write_lock: Mutex<()>,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let targets = root.join("targets");
            fs::create_dir(&targets).unwrap();
            let paths = AppPaths::from_data_root(root.join("private/app-data")).unwrap();
            let database = Database::open(&paths).unwrap();
            Self {
                _temporary: temporary,
                root,
                targets,
                paths,
                database,
                write_lock: Mutex::new(()),
            }
        }
    }

    fn file_descriptor(path: &Path, scope: Scope, project_root: Option<&Path>) -> TargetDescriptor {
        TargetDescriptor {
            tool: Tool::Claude,
            artifact_kind: ArtifactKind::Prompt,
            scope,
            project_root: project_root.map(|root| root.to_string_lossy().into_owned()),
            path: Some(path.to_string_lossy().into_owned()),
            format: TargetFormat::Markdown,
            managed_selector_roots: vec!["$document".to_owned()],
            sensitive_selectors: Vec::new(),
            capability: TargetCapability {
                state: CapabilityState::Supported,
                diagnostic_code: None,
            },
            policy: PolicyState::Allowed,
            trust: TargetTrustState::NotRequired,
            prompt_override: PromptOverrideState::NotApplicable,
            symlink_policy: SymlinkPolicy::Reject,
        }
    }

    fn insert_target_and_request(
        database: &Database,
        target_id: &str,
        descriptor: TargetDescriptor,
        desired: Value,
        git: Option<crate::git::GitPathStatus>,
        exclude_from_git: bool,
        project_id: Option<&str>,
    ) -> PreviewTargetRequest {
        let ownership = ManagedOwnership::WholeDocument;
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &descriptor,
            &ownership,
        );
        let (full_hash, managed_hash, projection) = match &scan {
            TargetScan::Observed(observed) => (
                Some(observed.full_hash.clone()),
                Some(observed.managed_hash.clone()),
                observed.managed_projection.clone(),
            ),
            TargetScan::Missing => (None, None, Value::Null),
            _ => panic!("测试目标必须是普通文件或缺失目标"),
        };
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, project_id, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'claude', ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    target_id,
                    descriptor.artifact_kind.as_str(),
                    descriptor.scope.as_str(),
                    project_id,
                    descriptor.path.as_deref().unwrap(),
                    full_hash,
                    managed_hash,
                    serde_json::to_string(&projection).unwrap(),
                ],
            )
            .unwrap();
        PreviewTargetRequest {
            descriptor,
            ownership,
            baseline: ManagedTargetBaseline {
                target_id: target_id.to_owned(),
                target_row_version: 1,
                full_hash,
                managed_hash,
            },
            scan,
            desired_projection: desired,
            row_versions: Vec::new(),
            git,
            exclude_from_git,
        }
    }

    fn persist_requests(
        database: &mut Database,
        scope: Scope,
        project_id: Option<String>,
        requests: Vec<PreviewTargetRequest>,
    ) -> String {
        let plan =
            build_preview_plan(scope, project_id, requests, &SecretRedactor::default()).unwrap();
        let preview_id = plan.preview_id.clone();
        persist_preview(database, &plan).unwrap();
        preview_id
    }

    fn input(
        descriptor: TargetDescriptor,
        desired: Value,
        allowed_root: &Path,
    ) -> ApplyTargetInput {
        ApplyTargetInput {
            descriptor,
            ownership: ManagedOwnership::WholeDocument,
            desired_projection: desired,
            allowed_root: allowed_root.to_path_buf(),
            central_skills_root: None,
            delete_target: false,
            managed_items: Vec::new(),
            remove_managed_item_ids: Vec::new(),
        }
    }

    #[derive(Clone, Copy)]
    enum InjectPhase {
        BeforeTarget,
        BeforeRename,
        AfterRename,
        AfterTarget,
        BeforeDatabaseFinalize,
        AfterDatabaseFinalize,
    }

    struct InjectFault {
        target_index: usize,
        phase: InjectPhase,
        decision: ApplyFaultDecision,
        sabotage: Option<PathBuf>,
    }

    impl ApplyFaultInjector for InjectFault {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            let matches = match (self.phase, event) {
                (InjectPhase::BeforeTarget, ApplyFaultEvent::BeforeTarget { index, .. })
                | (InjectPhase::BeforeRename, ApplyFaultEvent::BeforeRename { index, .. })
                | (InjectPhase::AfterRename, ApplyFaultEvent::AfterRename { index, .. })
                | (InjectPhase::AfterTarget, ApplyFaultEvent::AfterTarget { index, .. }) => {
                    *index == self.target_index
                }
                (InjectPhase::BeforeDatabaseFinalize, ApplyFaultEvent::BeforeDatabaseFinalize)
                | (InjectPhase::AfterDatabaseFinalize, ApplyFaultEvent::AfterDatabaseFinalize) => {
                    true
                }
                _ => false,
            };
            if !matches {
                return ApplyFaultDecision::Continue;
            }
            if let Some(path) = &self.sabotage {
                if path.is_dir() {
                    let _ = fs::remove_dir(path);
                } else {
                    let _ = fs::remove_file(path);
                }
                let _ = fs::create_dir(path);
            }
            self.decision
        }
    }

    struct ChangeBeforeWrite {
        target: PathBuf,
        database_path: Option<PathBuf>,
        target_id: Option<String>,
        before_rename: bool,
        after_rename: bool,
        changed: AtomicBool,
    }

    impl ApplyFaultInjector for ChangeBeforeWrite {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            let should_change = if self.after_rename {
                matches!(event, ApplyFaultEvent::AfterRename { .. })
            } else if self.before_rename {
                matches!(event, ApplyFaultEvent::BeforeRename { .. })
            } else {
                matches!(event, ApplyFaultEvent::BeforeTarget { .. })
            };
            if should_change && !self.changed.swap(true, Ordering::SeqCst) {
                if let (Some(database_path), Some(target_id)) =
                    (&self.database_path, &self.target_id)
                {
                    rusqlite::Connection::open(database_path)
                        .unwrap()
                        .execute(
                            "UPDATE managed_targets SET last_status = 'failed' WHERE id = ?1",
                            [target_id],
                        )
                        .unwrap();
                } else {
                    fs::write(&self.target, "external-race").unwrap();
                }
            }
            ApplyFaultDecision::Continue
        }
    }

    #[test]
    fn nth_target_failure_restores_all_prior_targets_without_partial_files() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("first.md");
        let second = fixture.targets.join("second.md");
        fs::write(&first, "old-first").unwrap();
        fs::write(&second, "old-second").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                "10000000-0000-4000-8000-000000000001",
                first_descriptor.clone(),
                json!("new-first"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                "10000000-0000-4000-8000-000000000002",
                second_descriptor.clone(),
                json!("new-second"),
                None,
                false,
                None,
            ),
        ];
        let preview_id = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[
                input(first_descriptor, json!("new-first"), &fixture.targets),
                input(second_descriptor, json!("new-second"), &fixture.targets),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Fail,
                sabotage: None,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        assert_eq!(fs::read_to_string(&first).unwrap(), "old-first");
        assert_eq!(fs::read_to_string(&second).unwrap(), "old-second");
        let status: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT status FROM sync_runs WHERE id = ?1",
                [&preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "rolled_back");
        assert_eq!(list_snapshots(&fixture.database).unwrap().len(), 2);
        assert!(fs::read_dir(&fixture.targets).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".easytoagents-")
        }));
    }

    #[test]
    fn rollback_failure_preserves_journal_snapshots_and_unknown_directory() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("first.md");
        let second = fixture.targets.join("second.md");
        fs::write(&first, "old-first").unwrap();
        fs::write(&second, "old-second").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                "11000000-0000-4000-8000-000000000001",
                first_descriptor.clone(),
                json!("new-first"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                "11000000-0000-4000-8000-000000000002",
                second_descriptor.clone(),
                json!("new-second"),
                None,
                false,
                None,
            ),
        ];
        let preview_id = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[
                input(first_descriptor, json!("new-first"), &fixture.targets),
                input(second_descriptor, json!("new-second"), &fixture.targets),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Fail,
                sabotage: Some(first.clone()),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::RollbackFailed);
        assert!(first.is_dir(), "回滚绝不能递归删除故障注入的未知目录");
        assert!(fixture
            .paths
            .journals()
            .join(format!("{preview_id}.json"))
            .is_file());
        assert_eq!(list_snapshots(&fixture.database).unwrap().len(), 2);
    }

    #[test]
    fn same_and_different_preview_claims_are_atomic_across_connections() {
        fn run_case(same_preview: bool) {
            let mut fixture = Fixture::new();
            let first = fixture.targets.join("one.md");
            let second = fixture.targets.join("two.md");
            fs::write(&first, "one").unwrap();
            fs::write(&second, "two").unwrap();
            let first_descriptor = file_descriptor(&first, Scope::Global, None);
            let first_request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                first_descriptor,
                json!("next-one"),
                None,
                false,
                None,
            );
            let first_preview = persist_requests(
                &mut fixture.database,
                Scope::Global,
                None,
                vec![first_request],
            );
            let second_preview = if same_preview {
                first_preview.clone()
            } else {
                let second_descriptor = file_descriptor(&second, Scope::Global, None);
                let second_request = insert_target_and_request(
                    &fixture.database,
                    &Uuid::new_v4().to_string(),
                    second_descriptor,
                    json!("next-two"),
                    None,
                    false,
                    None,
                );
                persist_requests(
                    &mut fixture.database,
                    Scope::Global,
                    None,
                    vec![second_request],
                )
            };
            let second_database = Database::open(&fixture.paths).unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let paths = fixture.paths.clone();
            let first_id = first_preview.clone();
            let first_barrier = Arc::clone(&barrier);
            let first_thread = thread::spawn(move || {
                let mut database = fixture.database;
                first_barrier.wait();
                let journal = paths.journals().join(format!("{first_id}.json"));
                claim_preview(&mut database, &first_id, &journal)
            });
            let paths = fixture.paths.clone();
            let second_barrier = Arc::clone(&barrier);
            let second_thread = thread::spawn(move || {
                let mut database = second_database;
                second_barrier.wait();
                let journal = paths.journals().join(format!("{second_preview}.json"));
                claim_preview(&mut database, &second_preview, &journal)
            });
            let results = [first_thread.join().unwrap(), second_thread.join().unwrap()];
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
            let error = results.into_iter().find_map(Result::err).unwrap();
            assert_eq!(
                error.code(),
                if same_preview {
                    ErrorCode::PreviewAlreadyConsumed
                } else {
                    ErrorCode::WriteInProgress
                }
            );
            assert_eq!(fs::read_to_string(&first).unwrap(), "one");
            assert_eq!(fs::read_to_string(&second).unwrap(), "two");
        }
        run_case(true);
        run_case(false);
    }

    #[test]
    fn external_change_after_preview_is_stale_and_never_overwritten() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("prompt.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            "12000000-0000-4000-8000-000000000001",
            descriptor.clone(),
            json!("desired"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        fs::write(&target, "external").unwrap();
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("desired"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(target).unwrap(), "external");
    }

    #[test]
    fn database_version_change_after_preview_is_stale_without_external_write() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("prompt.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let target_id = "12500000-0000-4000-8000-000000000001";
        let request = insert_target_and_request(
            &fixture.database,
            target_id,
            descriptor.clone(),
            json!("desired"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET last_status = 'failed' WHERE id = ?1",
                [target_id],
            )
            .unwrap();
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("desired"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
    }

    #[test]
    fn database_and_file_changes_in_the_write_window_are_never_overwritten() {
        let mut database_fixture = Fixture::new();
        let database_target = database_fixture.targets.join("database-race.md");
        fs::write(&database_target, "old").unwrap();
        let database_descriptor = file_descriptor(&database_target, Scope::Global, None);
        let database_target_id = Uuid::new_v4().to_string();
        let database_request = insert_target_and_request(
            &database_fixture.database,
            &database_target_id,
            database_descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let database_preview = persist_requests(
            &mut database_fixture.database,
            Scope::Global,
            None,
            vec![database_request],
        );
        let database_path = database_fixture.database.path().to_path_buf();
        let error = apply_persisted_preview(
            &database_fixture.write_lock,
            &mut database_fixture.database,
            &database_fixture.paths,
            &database_preview,
            &[input(
                database_descriptor,
                json!("new"),
                &database_fixture.targets,
            )],
            &ChangeBeforeWrite {
                target: database_target.clone(),
                database_path: Some(database_path),
                target_id: Some(database_target_id),
                before_rename: false,
                after_rename: false,
                changed: AtomicBool::new(false),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(database_target).unwrap(), "old");

        let mut file_fixture = Fixture::new();
        let file_target = file_fixture.targets.join("file-race.md");
        fs::write(&file_target, "old").unwrap();
        let race_descriptor = file_descriptor(&file_target, Scope::Global, None);
        let file_request = insert_target_and_request(
            &file_fixture.database,
            &Uuid::new_v4().to_string(),
            race_descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let file_preview = persist_requests(
            &mut file_fixture.database,
            Scope::Global,
            None,
            vec![file_request],
        );
        let error = apply_persisted_preview(
            &file_fixture.write_lock,
            &mut file_fixture.database,
            &file_fixture.paths,
            &file_preview,
            &[input(race_descriptor, json!("new"), &file_fixture.targets)],
            &ChangeBeforeWrite {
                target: file_target.clone(),
                database_path: None,
                target_id: None,
                before_rename: true,
                after_rename: false,
                changed: AtomicBool::new(false),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(file_target).unwrap(), "external-race");
        assert!(fs::read_dir(&file_fixture.targets).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".easytoagents-")
        }));

        let mut after_fixture = Fixture::new();
        let after_target = after_fixture.targets.join("after-rename-race.md");
        fs::write(&after_target, "old").unwrap();
        let after_descriptor = file_descriptor(&after_target, Scope::Global, None);
        let after_request = insert_target_and_request(
            &after_fixture.database,
            &Uuid::new_v4().to_string(),
            after_descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let after_preview = persist_requests(
            &mut after_fixture.database,
            Scope::Global,
            None,
            vec![after_request],
        );
        let error = apply_persisted_preview(
            &after_fixture.write_lock,
            &mut after_fixture.database,
            &after_fixture.paths,
            &after_preview,
            &[input(
                after_descriptor,
                json!("new"),
                &after_fixture.targets,
            )],
            &ChangeBeforeWrite {
                target: after_target.clone(),
                database_path: None,
                target_id: None,
                before_rename: false,
                after_rename: true,
                changed: AtomicBool::new(false),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(after_target).unwrap(), "external-race");
    }

    #[test]
    fn snapshot_refuses_content_changed_after_the_render_preflight() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("snapshot-race.md");
        fs::write(&target, "old").unwrap();
        let expected = super::capture_path_state(&target).unwrap().fingerprint();
        fs::write(&target, "external-before-snapshot").unwrap();

        let error = super::create_snapshot(
            &mut fixture.database,
            &fixture.paths,
            super::SnapshotRequest {
                run_id: &Uuid::new_v4().to_string(),
                target_id: Some("16000000-0000-4000-8000-000000000001"),
                target_path: &target,
                allowed_root: &fixture.targets,
                central_root: None,
                expected_before_fingerprint: &expected,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(target).unwrap(),
            "external-before-snapshot"
        );
    }

    #[test]
    fn whole_document_delete_is_snapshotted_and_can_be_restored() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("prompt.md");
        fs::write(&target, "old-content").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let target_id = "12600000-0000-4000-8000-000000000001";
        let request = insert_target_and_request(
            &fixture.database,
            target_id,
            descriptor.clone(),
            json!(""),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        let mut delete_input = input(descriptor, json!(""), &fixture.targets);
        delete_input.delete_target = true;
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[delete_input],
            &NoApplyFault,
        )
        .unwrap();
        assert!(!target.exists());
        let hashes: (Option<String>, Option<String>) = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_full_hash, baseline_managed_hash
                 FROM managed_targets WHERE id = ?1",
                [target_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(hashes, (None, None));
        let snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.run_id == preview_id)
            .unwrap();
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "old-content");
    }

    #[test]
    fn selector_owned_delete_can_never_remove_the_whole_file() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("settings.json");
        fs::write(&target, r#"{"env":{"OWNED":"old"},"user":"keep"}"#).unwrap();
        let mut descriptor = file_descriptor(&target, Scope::Global, None);
        descriptor.artifact_kind = ArtifactKind::Provider;
        descriptor.format = TargetFormat::Json;
        descriptor.managed_selector_roots = vec!["env".to_owned()];
        let ownership = ManagedOwnership::selectors([["env", "OWNED"]]);
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &descriptor,
            &ownership,
        );
        let TargetScan::Observed(observed) = &scan else {
            panic!("JSON fixture 必须可扫描");
        };
        let target_id = Uuid::new_v4().to_string();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'claude', 'provider', 'global', ?2, ?3, ?4, ?5)",
                params![
                    target_id,
                    descriptor.path.as_deref().unwrap(),
                    observed.full_hash,
                    observed.managed_hash,
                    serde_json::to_string(&observed.managed_projection).unwrap(),
                ],
            )
            .unwrap();
        let preview_id = persist_requests(
            &mut fixture.database,
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                descriptor: descriptor.clone(),
                ownership: ownership.clone(),
                baseline: ManagedTargetBaseline {
                    target_id,
                    target_row_version: 1,
                    full_hash: Some(observed.full_hash.clone()),
                    managed_hash: Some(observed.managed_hash.clone()),
                },
                scan,
                desired_projection: json!({}),
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
            }],
        );
        let mut apply_input = input(descriptor, json!({}), &fixture.targets);
        apply_input.ownership = ownership;
        apply_input.delete_target = true;
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[apply_input],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert_eq!(
            fs::read_to_string(target).unwrap(),
            r#"{"env":{"OWNED":"old"},"user":"keep"}"#
        );
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
    }

    #[test]
    fn crashes_before_and_after_rename_block_writes_and_restore_from_a_second_snapshot() {
        for phase in [InjectPhase::BeforeRename, InjectPhase::AfterRename] {
            let mut fixture = Fixture::new();
            let target = fixture.targets.join("prompt.md");
            fs::write(&target, "old").unwrap();
            let descriptor = file_descriptor(&target, Scope::Global, None);
            let request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                descriptor.clone(),
                json!("new"),
                None,
                false,
                None,
            );
            let preview_id =
                persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
            let error = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &preview_id,
                &[input(descriptor.clone(), json!("new"), &fixture.targets)],
                &InjectFault {
                    target_index: 0,
                    phase,
                    decision: ApplyFaultDecision::Crash,
                    sabotage: None,
                },
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
            let recovery = detect_interrupted_run(&fixture.database, &fixture.paths)
                .unwrap()
                .expect("崩溃后必须检测到活动 run");
            assert_eq!(recovery.run_id, preview_id);
            assert!(recovery.journal_available);
            assert_eq!(recovery.targets.len(), 1);
            match phase {
                InjectPhase::BeforeRename => {
                    assert_eq!(fs::read_to_string(&target).unwrap(), "old")
                }
                InjectPhase::AfterRename => assert_eq!(fs::read_to_string(&target).unwrap(), "new"),
                InjectPhase::BeforeTarget
                | InjectPhase::AfterTarget
                | InjectPhase::BeforeDatabaseFinalize
                | InjectPhase::AfterDatabaseFinalize => unreachable!(),
            }

            let blocked_target = fixture.targets.join("blocked.md");
            fs::write(&blocked_target, "blocked-old").unwrap();
            let blocked_descriptor = file_descriptor(&blocked_target, Scope::Global, None);
            let blocked_request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                blocked_descriptor.clone(),
                json!("blocked-new"),
                None,
                false,
                None,
            );
            let blocked_preview = persist_requests(
                &mut fixture.database,
                Scope::Global,
                None,
                vec![blocked_request],
            );
            let blocked = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &blocked_preview,
                &[input(
                    blocked_descriptor,
                    json!("blocked-new"),
                    &fixture.targets,
                )],
                &NoApplyFault,
            )
            .unwrap_err();
            assert_eq!(blocked.code(), ErrorCode::WriteInProgress);
            assert_eq!(fs::read_to_string(blocked_target).unwrap(), "blocked-old");

            let snapshot = list_snapshots(&fixture.database)
                .unwrap()
                .into_iter()
                .find(|snapshot| snapshot.run_id == preview_id)
                .unwrap();
            let restore_preview = preview_restore(
                &mut fixture.database,
                &fixture.paths,
                &snapshot.snapshot_id,
                &fixture.targets,
            )
            .unwrap();
            restore_snapshot(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &restore_preview.preview_id,
                &fixture.targets,
                None,
            )
            .unwrap();
            assert_eq!(fs::read_to_string(&target).unwrap(), "old");
            let restore_snapshot_count: i64 = fixture
                .database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM snapshots WHERE run_id = ?1",
                    [&restore_preview.preview_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(restore_snapshot_count, 1, "恢复前必须创建二次快照");
            assert!(fs::read_dir(&fixture.targets).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".easytoagents-")
            }));
        }
    }

    #[test]
    fn crash_at_nth_target_records_partial_progress_for_recovery() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("first.md");
        let second = fixture.targets.join("second.md");
        fs::write(&first, "first-old").unwrap();
        fs::write(&second, "second-old").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                first_descriptor.clone(),
                json!("first-new"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                second_descriptor.clone(),
                json!("second-new"),
                None,
                false,
                None,
            ),
        ];
        let preview_id = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[
                input(first_descriptor, json!("first-new"), &fixture.targets),
                input(second_descriptor, json!("second-new"), &fixture.targets),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Crash,
                sabotage: None,
            },
        )
        .unwrap_err();
        assert_eq!(fs::read_to_string(first).unwrap(), "first-new");
        assert_eq!(fs::read_to_string(second).unwrap(), "second-old");
        let recovery = detect_interrupted_run(&fixture.database, &fixture.paths)
            .unwrap()
            .unwrap();
        assert_eq!(recovery.targets.len(), 2);
        assert!(recovery
            .targets
            .iter()
            .any(|target| target.phase == "written"));
        assert!(recovery
            .targets
            .iter()
            .any(|target| target.phase == "crashed_before_target"));
    }

    #[test]
    fn restoring_one_snapshot_keeps_a_partial_multi_target_run_blocking() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("partial-first.md");
        let second = fixture.targets.join("partial-second.md");
        fs::write(&first, "first-old").unwrap();
        fs::write(&second, "second-old").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                first_descriptor,
                json!("first-new"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                second_descriptor,
                json!("second-new"),
                None,
                false,
                None,
            ),
        ];
        let source_run = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        let persisted = super::load_persisted_preview(&fixture.database, &source_run).unwrap();
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &source_run,
            &[
                input(
                    persisted.items[0].envelope.descriptor.clone(),
                    json!("first-new"),
                    &fixture.targets,
                ),
                input(
                    persisted.items[1].envelope.descriptor.clone(),
                    json!("second-new"),
                    &fixture.targets,
                ),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Crash,
                sabotage: None,
            },
        )
        .unwrap_err();
        let snapshots = list_snapshots(&fixture.database).unwrap();
        let second_snapshot = snapshots
            .iter()
            .find(|snapshot| {
                snapshot.run_id == source_run && snapshot.target_path == second.to_string_lossy()
            })
            .unwrap();
        let second_restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &second_snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &second_restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap();
        let source_status: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT status FROM sync_runs WHERE id = ?1",
                [&source_run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_status, "rollback_failed");
        assert_eq!(fs::read_to_string(&first).unwrap(), "first-new");
        assert_eq!(fs::read_to_string(&second).unwrap(), "second-old");

        let first_snapshot = snapshots
            .iter()
            .find(|snapshot| {
                snapshot.run_id == source_run && snapshot.target_path == first.to_string_lossy()
            })
            .unwrap();
        let first_restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &first_snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &first_restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap();
        let source_status: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT status FROM sync_runs WHERE id = ?1",
                [&source_run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_status, "rolled_back");
        assert_eq!(fs::read_to_string(first).unwrap(), "first-old");
    }

    #[test]
    fn crashes_on_both_sides_of_database_finalize_never_guess_rollback() {
        for phase in [
            InjectPhase::BeforeDatabaseFinalize,
            InjectPhase::AfterDatabaseFinalize,
        ] {
            let mut fixture = Fixture::new();
            let target = fixture.targets.join("finalize.md");
            fs::write(&target, "old").unwrap();
            let descriptor = file_descriptor(&target, Scope::Global, None);
            let request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                descriptor.clone(),
                json!("new"),
                None,
                false,
                None,
            );
            let preview_id =
                persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
            let error = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &preview_id,
                &[input(descriptor, json!("new"), &fixture.targets)],
                &InjectFault {
                    target_index: 0,
                    phase,
                    decision: ApplyFaultDecision::Crash,
                    sabotage: None,
                },
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::DatabaseError);
            assert_eq!(fs::read_to_string(&target).unwrap(), "new");
            let status: String = fixture
                .database
                .connection()
                .query_row(
                    "SELECT status FROM sync_runs WHERE id = ?1",
                    [&preview_id],
                    |row| row.get(0),
                )
                .unwrap();
            match phase {
                InjectPhase::BeforeDatabaseFinalize => {
                    assert_eq!(status, "applying");
                    assert!(detect_interrupted_run(&fixture.database, &fixture.paths)
                        .unwrap()
                        .is_some());
                }
                InjectPhase::AfterDatabaseFinalize => {
                    assert_eq!(status, "succeeded");
                    assert!(detect_interrupted_run(&fixture.database, &fixture.paths)
                        .unwrap()
                        .is_none());
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn invalid_managed_item_intent_is_rejected_before_snapshot_or_external_write() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("managed-item.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        let mut apply_input = input(descriptor, json!("new"), &fixture.targets);
        apply_input.managed_items.push(ManagedItemApply {
            id: Uuid::new_v4().to_string(),
            resource_kind: ArtifactKind::Prompt,
            resource_id: Uuid::new_v4().to_string(),
            external_key: "active-prompt".to_owned(),
            last_applied_item_hash: "not-a-sha256".to_owned(),
        });
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[apply_input],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
    }

    fn applied_missing_snapshot_fixture() -> (Fixture, PathBuf, String) {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("created.md");
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("created"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("created"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap();
        let snapshot_id = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.run_id == preview_id)
            .unwrap()
            .snapshot_id;
        (fixture, target, snapshot_id)
    }

    #[test]
    fn restore_never_deletes_unknown_directory_or_external_symlink() {
        let (mut directory_fixture, directory_target, directory_snapshot) =
            applied_missing_snapshot_fixture();
        fs::remove_file(&directory_target).unwrap();
        fs::create_dir(&directory_target).unwrap();
        let preview = preview_restore(
            &mut directory_fixture.database,
            &directory_fixture.paths,
            &directory_snapshot,
            &directory_fixture.targets,
        )
        .unwrap();
        let error = restore_snapshot(
            &directory_fixture.write_lock,
            &mut directory_fixture.database,
            &directory_fixture.paths,
            &preview.preview_id,
            &directory_fixture.targets,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert!(directory_target.is_dir());

        let (mut link_fixture, link_target, link_snapshot) = applied_missing_snapshot_fixture();
        fs::remove_file(&link_target).unwrap();
        let outside = link_fixture.root.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, &link_target).unwrap();
        let central = link_fixture.root.join("central");
        fs::create_dir(&central).unwrap();
        let preview = preview_restore(
            &mut link_fixture.database,
            &link_fixture.paths,
            &link_snapshot,
            &link_fixture.targets,
        )
        .unwrap();
        let error = restore_snapshot(
            &link_fixture.write_lock,
            &mut link_fixture.database,
            &link_fixture.paths,
            &preview.preview_id,
            &link_fixture.targets,
            Some(&central),
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(fs::read_link(&link_target).unwrap(), outside);
    }

    #[test]
    fn restore_rechecks_snapshot_row_version_before_any_external_write() {
        let (mut fixture, target, snapshot_id) = applied_missing_snapshot_fixture();
        let preview = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE snapshots SET target_type = target_type WHERE id = ?1",
                [&snapshot_id],
            )
            .unwrap();
        let error = restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(target).unwrap(), "created");
        let second_snapshot_count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE run_id = ?1",
                [&preview.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(second_snapshot_count, 0);
    }

    #[test]
    fn recovery_cleanup_refuses_a_replaced_temporary_file() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("temporary.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("new"), &fixture.targets)],
            &InjectFault {
                target_index: 0,
                phase: InjectPhase::BeforeRename,
                decision: ApplyFaultDecision::Crash,
                sabotage: None,
            },
        )
        .unwrap_err();
        let journal: super::RunJournal = serde_json::from_slice(
            &fs::read(fixture.paths.journals().join(format!("{preview_id}.json"))).unwrap(),
        )
        .unwrap();
        let temporary = PathBuf::from(
            journal.targets[0]
                .temporary_path
                .as_deref()
                .expect("rename 前崩溃必须记录临时路径"),
        );
        fs::write(&temporary, "unknown replacement").unwrap();
        let snapshot_id = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.run_id == preview_id)
            .unwrap()
            .snapshot_id;
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        let error = restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(
            fs::read_to_string(&temporary).unwrap(),
            "unknown replacement"
        );
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
    }

    #[test]
    fn late_ancestor_symlink_escape_is_rejected_without_touching_outside() {
        let mut fixture = Fixture::new();
        let parent = fixture.targets.join("nested");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("prompt.md");
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("desired"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        fs::remove_dir(&parent).unwrap();
        let outside = fixture.root.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, &parent).unwrap();
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("desired"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap_err();
        assert!(matches!(
            error.code(),
            ErrorCode::Conflict | ErrorCode::StalePreview
        ));
        assert!(!outside.join("prompt.md").exists());
    }

    fn skill_descriptor(path: &Path) -> TargetDescriptor {
        TargetDescriptor {
            tool: Tool::Claude,
            artifact_kind: ArtifactKind::Skill,
            scope: Scope::Global,
            project_root: None,
            path: Some(path.to_string_lossy().into_owned()),
            format: TargetFormat::SymlinkDirectory,
            managed_selector_roots: vec!["$children".to_owned()],
            sensitive_selectors: Vec::new(),
            capability: TargetCapability::supported(),
            policy: PolicyState::Allowed,
            trust: TargetTrustState::NotRequired,
            prompt_override: PromptOverrideState::NotApplicable,
            symlink_policy: SymlinkPolicy::ManagedChildrenOnly,
        }
    }

    fn skill_request(
        database: &Database,
        target_id: &str,
        descriptor: TargetDescriptor,
        ownership: ManagedOwnership,
        desired: Value,
    ) -> PreviewTargetRequest {
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &descriptor,
            &ownership,
        );
        let (full_hash, managed_hash, projection) = match &scan {
            TargetScan::Observed(observed) => (
                Some(observed.full_hash.clone()),
                Some(observed.managed_hash.clone()),
                observed.managed_projection.clone(),
            ),
            TargetScan::Missing => (None, None, Value::Null),
            _ => panic!("Skills 目录 fixture 必须存在或缺失"),
        };
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'claude', 'skill', 'global', ?2, ?3, ?4, ?5)",
                params![
                    target_id,
                    descriptor.path.as_deref().unwrap(),
                    full_hash,
                    managed_hash,
                    serde_json::to_string(&projection).unwrap(),
                ],
            )
            .unwrap();
        PreviewTargetRequest {
            descriptor,
            ownership,
            baseline: ManagedTargetBaseline {
                target_id: target_id.to_owned(),
                target_row_version: 1,
                full_hash,
                managed_hash,
            },
            scan,
            desired_projection: desired,
            row_versions: Vec::new(),
            git: None,
            exclude_from_git: false,
        }
    }

    #[test]
    fn rollback_preserves_a_concurrently_replaced_created_skill_directory() {
        let mut fixture = Fixture::new();
        let skills = fixture.targets.join("nested/skills");
        let created_parent = fixture.targets.join("nested");
        let central = fixture.root.join("central-skills");
        let central_skill = central.join("owned-skill");
        fs::create_dir_all(&central_skill).unwrap();
        let descriptor = skill_descriptor(&skills);
        let ownership = ManagedOwnership::SymlinkNames(vec!["owned-skill".to_owned()]);
        let desired = json!({
            "owned-skill": {
                "targetType": "symlink",
                "linkTarget": central_skill.to_string_lossy(),
            }
        });
        let request = skill_request(
            &fixture.database,
            "13000000-0000-4000-8000-000000000099",
            descriptor.clone(),
            ownership.clone(),
            desired.clone(),
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);

        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[ApplyTargetInput {
                descriptor,
                ownership,
                desired_projection: desired,
                allowed_root: fixture.targets.clone(),
                central_skills_root: Some(central),
                delete_target: false,
                managed_items: Vec::new(),
                remove_managed_item_ids: Vec::new(),
            }],
            &InjectFault {
                target_index: 0,
                phase: InjectPhase::AfterTarget,
                decision: ApplyFaultDecision::Fail,
                sabotage: Some(created_parent.clone()),
            },
        )
        .unwrap_err();

        assert_eq!(error.code(), ErrorCode::RollbackFailed);
        assert!(created_parent.is_dir());
        assert!(fs::read_dir(&created_parent).unwrap().next().is_none());
        assert!(fixture
            .paths
            .journals()
            .join(format!("{preview_id}.json"))
            .is_file());
    }

    #[test]
    fn managed_symlink_uses_atomic_rename_and_refuses_directory_or_external_link() {
        let mut fixture = Fixture::new();
        let skills = fixture.targets.join("skills");
        let central = fixture.root.join("central-skills");
        let central_skill = central.join("owned-skill");
        fs::create_dir(&skills).unwrap();
        fs::create_dir_all(&central_skill).unwrap();
        let descriptor = skill_descriptor(&skills);
        let ownership = ManagedOwnership::SymlinkNames(vec!["owned-skill".to_owned()]);
        let desired = json!({
            "owned-skill": {
                "targetType": "symlink",
                "linkTarget": central_skill.to_string_lossy(),
            }
        });
        let request = skill_request(
            &fixture.database,
            "13000000-0000-4000-8000-000000000001",
            descriptor.clone(),
            ownership.clone(),
            desired.clone(),
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[ApplyTargetInput {
                descriptor,
                ownership,
                desired_projection: desired,
                allowed_root: fixture.targets.clone(),
                central_skills_root: Some(central.clone()),
                delete_target: false,
                managed_items: Vec::new(),
                remove_managed_item_ids: Vec::new(),
            }],
            &NoApplyFault,
        )
        .unwrap();
        assert_eq!(
            fs::canonicalize(skills.join("owned-skill")).unwrap(),
            central_skill
        );
        assert!(fs::read_dir(&skills).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".easytoagents-")
        }));
        let child_path = skills.join("owned-skill");
        let child_snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| {
                snapshot.run_id == preview_id
                    && snapshot.target_path == child_path.to_string_lossy()
            })
            .unwrap();
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &child_snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &fixture.targets,
            Some(&central),
        )
        .unwrap();
        assert!(fs::symlink_metadata(child_path).is_err());

        for occupied_by_directory in [true, false] {
            let mut fixture = Fixture::new();
            let skills = fixture.targets.join("skills");
            let central = fixture.root.join("central-skills");
            fs::create_dir(&skills).unwrap();
            fs::create_dir(&central).unwrap();
            let child = skills.join("unknown");
            let outside = fixture.root.join("outside-skill");
            fs::create_dir(&outside).unwrap();
            if occupied_by_directory {
                fs::create_dir(&child).unwrap();
            } else {
                symlink(&outside, &child).unwrap();
            }
            let descriptor = skill_descriptor(&skills);
            let ownership = ManagedOwnership::SymlinkNames(vec!["unknown".to_owned()]);
            let request = skill_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                descriptor.clone(),
                ownership.clone(),
                json!({}),
            );
            let preview_id =
                persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
            let error = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &preview_id,
                &[ApplyTargetInput {
                    descriptor,
                    ownership,
                    desired_projection: json!({}),
                    allowed_root: fixture.targets.clone(),
                    central_skills_root: Some(central),
                    delete_target: false,
                    managed_items: Vec::new(),
                    remove_managed_item_ids: Vec::new(),
                }],
                &NoApplyFault,
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::Conflict);
            if occupied_by_directory {
                assert!(child.is_dir());
            } else {
                assert_eq!(fs::read_link(child).unwrap(), outside);
            }
        }
    }

    fn git_init(path: &Path) {
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn tracked_is_warning_only_and_untracked_exclude_requires_explicit_confirmation() {
        let mut fixture = Fixture::new();
        let repository = fixture.targets.join("repository");
        fs::create_dir(&repository).unwrap();
        git_init(&repository);
        let project_root = fs::canonicalize(&repository).unwrap();
        let project_id = "14000000-0000-4000-8000-000000000001";
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO projects(id, display_name, root_path, is_git_repo)
                 VALUES (?1, 'fixture', ?2, 1)",
                params![project_id, project_root.to_string_lossy()],
            )
            .unwrap();
        let project = ProjectRoot::parse(&project_root).unwrap();
        let exclude_path = repository.join(".git/info/exclude");
        let original_exclude = fs::read(&exclude_path).unwrap();

        let tracked_target = repository.join("tracked.md");
        fs::write(&tracked_target, "tracked-old").unwrap();
        assert!(Command::new("git")
            .args(["add", "--", "tracked.md"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());
        let tracked_descriptor =
            file_descriptor(&tracked_target, Scope::Project, Some(&project_root));
        let mut tracked_descriptor = tracked_descriptor;
        tracked_descriptor.artifact_kind = ArtifactKind::Mcp;
        let tracked_git = inspect_path(&project, &tracked_target).unwrap();
        assert!(tracked_git.tracked);
        let tracked_request = insert_target_and_request(
            &fixture.database,
            "14000000-0000-4000-8000-000000000002",
            tracked_descriptor.clone(),
            json!("tracked-new"),
            Some(tracked_git),
            true,
            Some(project_id),
        );
        let tracked_plan = build_preview_plan(
            Scope::Project,
            Some(project_id.to_owned()),
            vec![tracked_request],
            &SecretRedactor::default(),
        )
        .unwrap();
        assert!(tracked_plan.targets[0]
            .warning_codes
            .iter()
            .any(|code| code == "GIT_TRACKED"));
        assert!(!tracked_plan.targets[0].exclude_from_git);
        let tracked_preview = tracked_plan.preview_id.clone();
        persist_preview(&mut fixture.database, &tracked_plan).unwrap();
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &tracked_preview,
            &[input(
                tracked_descriptor,
                json!("tracked-new"),
                &project_root,
            )],
            &NoApplyFault,
        )
        .unwrap();
        assert_eq!(fs::read(&exclude_path).unwrap(), original_exclude);

        let untracked_target = repository.join(".codex/config.toml");
        fs::create_dir(repository.join(".codex")).unwrap();
        let untracked_descriptor =
            file_descriptor(&untracked_target, Scope::Project, Some(&project_root));
        let mut untracked_descriptor = untracked_descriptor;
        untracked_descriptor.artifact_kind = ArtifactKind::Mcp;
        let untracked_git = inspect_path(&project, &untracked_target).unwrap();
        assert!(!untracked_git.tracked);
        let target_id = "14000000-0000-4000-8000-000000000003";
        let untracked_request = insert_target_and_request(
            &fixture.database,
            target_id,
            untracked_descriptor.clone(),
            json!("local-only"),
            Some(untracked_git),
            true,
            Some(project_id),
        );
        let untracked_preview = persist_requests(
            &mut fixture.database,
            Scope::Project,
            Some(project_id.to_owned()),
            vec![untracked_request],
        );
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &untracked_preview,
            &[input(
                untracked_descriptor.clone(),
                json!("local-only"),
                &project_root,
            )],
            &NoApplyFault,
        )
        .unwrap();
        let exclude_once = fs::read_to_string(&exclude_path).unwrap();
        assert_eq!(exclude_once.matches("EasyToAgents managed").count(), 2);
        assert!(exclude_once.contains("/.codex/config.toml"));
        assert!(!repository.join(".gitignore").exists());

        let ownership = ManagedOwnership::WholeDocument;
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &untracked_descriptor,
            &ownership,
        );
        let baseline = load_managed_target_baseline(&fixture.database, target_id).unwrap();
        let second_request = PreviewTargetRequest {
            descriptor: untracked_descriptor.clone(),
            ownership,
            baseline,
            scan,
            desired_projection: json!("local-only"),
            row_versions: Vec::new(),
            git: Some(inspect_path(&project, &untracked_target).unwrap()),
            exclude_from_git: true,
        };
        let second_preview = persist_requests(
            &mut fixture.database,
            Scope::Project,
            Some(project_id.to_owned()),
            vec![second_request],
        );
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &second_preview,
            &[input(
                untracked_descriptor,
                json!("local-only"),
                &project_root,
            )],
            &NoApplyFault,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&exclude_path).unwrap(), exclude_once);
        assert!(!repository.join(".gitignore").exists());

        let exclude_snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| {
                snapshot.run_id == untracked_preview
                    && snapshot.target_path == exclude_path.to_string_lossy()
            })
            .unwrap();
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &exclude_snapshot.snapshot_id,
            &project_root,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &project_root,
            None,
        )
        .unwrap();
        assert_eq!(fs::read(&exclude_path).unwrap(), original_exclude);
        assert!(!repository.join(".gitignore").exists());
    }

    #[test]
    fn journal_and_database_preview_never_contain_registered_secret() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("secret.md");
        fs::write(&target, "old-safe").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            "15000000-0000-4000-8000-000000000001",
            descriptor.clone(),
            json!("fixture-phase3-secret"),
            None,
            false,
            None,
        );
        let mut redactor = SecretRedactor::default();
        redactor.register_secret("fixture-phase3-secret");
        let plan = build_preview_plan(Scope::Global, None, vec![request], &redactor).unwrap();
        let preview_id = plan.preview_id.clone();
        persist_preview(&mut fixture.database, &plan).unwrap();
        let managed_item_id = "15000000-0000-4000-8000-000000000002";
        let mut apply_input = input(descriptor, json!("fixture-phase3-secret"), &fixture.targets);
        apply_input.managed_items.push(ManagedItemApply {
            id: managed_item_id.to_owned(),
            resource_kind: ArtifactKind::Prompt,
            resource_id: "15000000-0000-4000-8000-000000000003".to_owned(),
            external_key: "active-prompt".to_owned(),
            last_applied_item_hash: "a".repeat(64),
        });
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[apply_input],
            &NoApplyFault,
        )
        .unwrap();
        let journal =
            fs::read_to_string(fixture.paths.journals().join(format!("{preview_id}.json")))
                .unwrap();
        assert!(!journal.contains("fixture-phase3-secret"));
        assert_eq!(
            mode(&fixture.paths.snapshots().join(&preview_id)).unwrap(),
            PRIVATE_DIRECTORY_MODE
        );
        assert_eq!(
            mode(&fixture.paths.journals().join(format!("{preview_id}.json"))).unwrap(),
            PRIVATE_FILE_MODE
        );
        let snapshot_path: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT snapshot_path FROM snapshots WHERE run_id = ?1 LIMIT 1",
                [&preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mode(Path::new(&snapshot_path)).unwrap(), PRIVATE_FILE_MODE);
        let preview_row: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!preview_row.contains("fixture-phase3-secret"));
        let managed_count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_items WHERE id = ?1",
                [managed_item_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(managed_count, 1);
    }
}
