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
        .map_err(|error| {
            AppError::database(&database_path, "begin_finish_apply").with_source(error)
        })?;
    let mut versions = BTreeMap::new();
    for item in &preview.items {
        record_expected_versions(item, &mut versions)?;
    }
    verify_database_versions(&transaction, &versions, &preview.preview_id, &database_path)?;
    let inputs_by_path = inputs_by_target_path(preview, inputs)?;
    for verification in verifications {
        let preview_item = preview
            .items
            .iter()
            .find(|item| item.target_id == verification.target_id)
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &verification.target_id))?;
        let input = inputs_by_path
            .get(preview_item.target_path.as_str())
            .copied()
            .ok_or_else(|| AppError::stale_preview(&preview.preview_id, &verification.target_id))?;
        if let Some(evidence) = input.project_native_action.as_ref() {
            apply_native_resource_changes(
                &transaction,
                preview,
                preview_item,
                evidence,
                &database_path,
            )?;
        } else {
            let projection = serde_json::to_string(&verification.projection).map_err(|error| {
                AppError::database(&database_path, "serialize_managed_baseline").with_source(error)
            })?;
            let expected_version = preview_item.envelope.target_row_version;
            let updated = crate::db::sync::update_managed_target_baseline(
                &transaction,
                &verification.target_id,
                verification.full_hash.as_deref(),
                verification.managed_hash.as_deref(),
                &projection,
                expected_version,
                &database_path,
            )?;
            if updated != 1 {
                return Err(AppError::stale_preview(
                    &preview.preview_id,
                    &verification.target_id,
                ));
            }
            apply_managed_item_changes(
                &transaction,
                preview_item,
                input,
                &preview.preview_id,
                &database_path,
            )?;
        }
        let item_updates = crate::db::sync::mark_sync_item_in_sync(
            &transaction,
            &preview.preview_id,
            &verification.target_id,
            &database_path,
        )?;
        if item_updates != 1 {
            return Err(AppError::stale_preview(
                &preview.preview_id,
                &verification.target_id,
            ));
        }
    }
    let run_updates = crate::db::sync::finish_sync_run(
        &transaction,
        &preview.preview_id,
        "applying",
        &database_path,
        "finish_apply_run",
    )?;
    if run_updates != 1 {
        return Err(AppError::write_in_progress(
            &preview.preview_id,
            "not_applying",
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_finish_apply").with_source(error)
    })
}

fn apply_native_resource_changes(
    transaction: &Transaction<'_>,
    preview: &PersistedPreview,
    preview_item: &PersistedPreviewItem,
    evidence: &super::ProjectNativeResourceEvidence,
    database_path: &str,
) -> Result<(), AppError> {
    use super::NativeResourceActionKind;
    match evidence.action {
        NativeResourceActionKind::Disable => {
            let snapshot_path = native_disable_snapshot_path(preview_item, evidence);
            let snapshot_id = crate::db::sync::find_latest_snapshot_id(
                transaction,
                &preview.preview_id,
                &preview_item.target_id,
                &snapshot_path,
                database_path,
            )?;
            crate::db::native_resources::mark_disabled_in_transaction(
                transaction,
                &evidence.resource_id,
                evidence.resource_row_version,
                &snapshot_id,
                &evidence.observed_item_hash,
                database_path,
            )
        }
        NativeResourceActionKind::Restore => {
            crate::db::native_resources::mark_restored_in_transaction(
                transaction,
                &evidence.resource_id,
                evidence.resource_row_version,
                &evidence.observed_item_hash,
                database_path,
            )
        }
    }
}

fn native_disable_snapshot_path(
    preview_item: &PersistedPreviewItem,
    evidence: &super::ProjectNativeResourceEvidence,
) -> String {
    use super::NativeResourceEntryType;
    match evidence.entry_type {
        NativeResourceEntryType::Directory | NativeResourceEntryType::Symlink => {
            Path::new(&preview_item.target_path)
                .join(&evidence.external_key)
                .to_string_lossy()
                .into_owned()
        }
        NativeResourceEntryType::McpEntry => preview_item.target_path.clone(),
    }
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
            .map_err(|error| {
                AppError::database(database_path, "read_managed_item_baseline").with_source(error)
            })?;
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
                .map_err(|error| {
                    AppError::database(database_path, "update_managed_item").with_source(error)
                })?;
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
                .map_err(|error| {
                    AppError::database(database_path, "insert_managed_item").with_source(error)
                })?;
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
            .map_err(|error| {
                AppError::database(database_path, "delete_managed_item").with_source(error)
            })?;
        if deleted != 1 {
            return Err(AppError::stale_preview(preview_id, remove_id));
        }
    }
    Ok(())
}

/// 回滚失败的 journal 只保留稳定错误码、allowlist 后的 operation 与脱敏 source，
/// 从不写入原始错误文本。
fn journal_failure(error: &AppError) -> JournalFailure {
    JournalFailure {
        code: error.code().as_str().to_owned(),
        operation: error
            .details()
            .and_then(|details| details.get("operation"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            // Conflict/not-found errors can be raised during rollback without
            // an operation detail; retain a stable diagnostic operation rather
            // than leaving rollback_failed without the required context.
            .or_else(|| Some("rollback".to_owned())),
        source: error.source().map(str::to_owned),
    }
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
    journal.phase = TargetPhase::RollingBack;
    persist_journal(paths, journal)?;
    for snapshot_index in applied.iter().rev() {
        let snapshot = &snapshots[*snapshot_index];
        let expected_after = journal.targets[*snapshot_index]
            .after_fingerprint
            .as_deref();
        if let Err(rollback_error) = restore_snapshot_record(
            snapshot,
            expected_after,
            snapshot.central_root.as_deref(),
            &snapshot.allowed_root,
        ) {
            journal.phase = TargetPhase::RollbackFailed;
            journal.targets[*snapshot_index].phase = TargetPhase::RollbackFailed;
            journal.failure = Some(journal_failure(&rollback_error));
            persist_journal(paths, journal)?;
            update_failed_run(
                database,
                run_id,
                "rollback_failed",
                ErrorCode::RollbackFailed,
            )?;
            let diagnostic = rollback_error
                .source()
                .map(str::to_owned)
                .unwrap_or_else(|| rollback_error.to_string());
            return Err(AppError::rollback_failed(
                run_id,
                &snapshot.target_path.to_string_lossy(),
                &snapshot.id,
            )
            .with_source(diagnostic));
        }
        journal.targets[*snapshot_index].phase = TargetPhase::RolledBack;
        persist_journal(paths, journal)?;
    }
    if let Err(rollback_error) = cleanup_takeover_quarantines(journal) {
        journal.phase = TargetPhase::RollbackFailed;
        journal.failure = Some(journal_failure(&rollback_error));
        persist_journal(paths, journal)?;
        update_failed_run(
            database,
            run_id,
            "rollback_failed",
            ErrorCode::RollbackFailed,
        )?;
        let diagnostic = rollback_error
            .source()
            .map(str::to_owned)
            .unwrap_or_else(|| rollback_error.to_string());
        return Err(
            AppError::rollback_failed(run_id, "takeoverQuarantine", "cleanup")
                .with_source(diagnostic),
        );
    }
    journal.phase = TargetPhase::RolledBack;
    persist_journal(paths, journal)?;
    let status = if original_error.code() == ErrorCode::StalePreview {
        "stale"
    } else {
        "rolled_back"
    };
    update_failed_run(database, run_id, status, original_error.code())?;
    Err(original_error)
}

fn cleanup_takeover_quarantines(journal: &mut RunJournal) -> Result<(), AppError> {
    for target in &mut journal.targets {
        cleanup_takeover_quarantine(target)?;
    }
    Ok(())
}

fn cleanup_takeover_quarantine(target: &mut JournalTarget) -> Result<(), AppError> {
    let Some(quarantine_text) = target.quarantine_path.clone() else {
        return Ok(());
    };
    let quarantine = PathBuf::from(&quarantine_text);
    let target_path = Path::new(&target.target_path);
    if quarantine.parent() != target_path.parent() {
        return Err(AppError::conflict(
            "takeoverQuarantine",
            "接管隔离项不在目标同目录",
        ));
    }
    let owned = quarantine
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(".easytoagents-"))
        .and_then(|name| name.strip_suffix(".takeover"))
        .is_some_and(|id| Uuid::parse_str(id).is_ok());
    if !owned {
        return Err(AppError::conflict(
            "takeoverQuarantine",
            "接管隔离项名称不属于应用",
        ));
    }
    let state = capture_path_state(&quarantine)?;
    if matches!(&state, PathState::Missing) {
        target.quarantine_path = None;
        target.quarantine_fingerprint = None;
        return Ok(());
    }
    if target.quarantine_fingerprint.as_deref() != Some(&state.fingerprint()) {
        return Err(AppError::conflict(
            "takeoverQuarantine",
            "接管隔离项身份已经变化",
        ));
    }
    match state {
        PathState::Symlink { .. } | PathState::File { .. } => {
            fs::remove_file(&quarantine).map_err(|error| {
                AppError::atomic_write(&quarantine.to_string_lossy(), "cleanup_takeover")
                    .with_source(error)
            })?;
        }
        PathState::Directory { .. } => {
            let hash = target
                .directory_tree_hash
                .as_deref()
                .ok_or_else(|| AppError::conflict("takeoverQuarantine", "目录隔离项缺少树 hash"))?;
            skill_library::remove_skill_tree(&quarantine, parent_of(&quarantine)?, hash)?;
        }
        PathState::Missing => {}
    }
    sync_directory(parent_of(&quarantine)?)?;
    target.quarantine_path = None;
    target.quarantine_fingerprint = None;
    Ok(())
}

fn update_failed_run(
    database: &mut Database,
    run_id: &str,
    status: &str,
    error_code: ErrorCode,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let updated = crate::db::sync::update_sync_run_status(
        database.connection(),
        run_id,
        status,
        error_code.persisted().as_str(),
        &database_path,
    )?;
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
                fs::remove_file(&snapshot.target_path).map_err(|error| {
                    AppError::atomic_write(
                        &snapshot.target_path.to_string_lossy(),
                        "rollback_remove_file",
                    )
                    .with_source(error)
                })?;
                sync_directory(parent_of(&snapshot.target_path)?)
            }
            PathState::Symlink { link_target } => {
                if let Some(central_root) = central_root {
                    validate_central_link_target(
                        &snapshot.target_path,
                        &link_target,
                        central_root,
                    )?;
                }
                validate_allowed_path(&snapshot.target_path, allowed_root, false)?;
                verify_expected_path_state(
                    &snapshot.target_path,
                    ExpectedPathFingerprint {
                        run_id: &snapshot.run_id,
                        target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                        fingerprint: &current_fingerprint,
                    },
                )?;
                fs::remove_file(&snapshot.target_path).map_err(|error| {
                    AppError::atomic_write(
                        &snapshot.target_path.to_string_lossy(),
                        "rollback_remove_symlink",
                    )
                    .with_source(error)
                })?;
                sync_directory(parent_of(&snapshot.target_path)?)
            }
            PathState::Directory { .. } => {
                verify_expected_path_state(
                    &snapshot.target_path,
                    ExpectedPathFingerprint {
                        run_id: &snapshot.run_id,
                        target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                        fingerprint: &current_fingerprint,
                    },
                )?;
                fs::remove_dir(&snapshot.target_path).map_err(|error| {
                    AppError::conflict("rollbackTarget", "Skills 回滚只删除本次创建且仍为空的目录")
                        .with_source(error)
                })?;
                sync_directory(parent_of(&snapshot.target_path)?)
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
                None,
            )
            .map_err(|failure| match failure {
                MutationFailure::Error(error) | MutationFailure::Crash(error) => error,
            })
        }
        PathState::Symlink { link_target } => {
            let expected = ExpectedPathFingerprint {
                run_id: &snapshot.run_id,
                target_id: snapshot.target_id.as_deref().unwrap_or("snapshot"),
                fingerprint: &current_fingerprint,
            };
            if let Some(central_root) = central_root {
                replace_symlink_without_journal(
                    &snapshot.target_path,
                    link_target,
                    central_root,
                    allowed_root,
                    expected,
                )
            } else {
                restore_external_symlink_without_central(
                    &snapshot.target_path,
                    link_target,
                    allowed_root,
                    expected,
                )
            }
        }
        PathState::Directory { .. }
            if snapshot.storage_kind == SnapshotStorageKind::DirectoryTree =>
        {
            replace_directory_tree_without_journal(
                &snapshot.target_path,
                &snapshot.snapshot_path,
                snapshot
                    .directory_tree_hash
                    .as_deref()
                    .ok_or_else(|| AppError::invalid_input("snapshot", "目录树快照缺少 hash"))?,
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
            "旧目录占位快照不能递归恢复",
        )),
    }
}

fn replace_directory_tree_without_journal(
    path: &Path,
    snapshot_path: &Path,
    content_hash: &str,
    central_root: Option<&Path>,
    allowed_root: &Path,
    expected_current: ExpectedPathFingerprint<'_>,
) -> Result<(), AppError> {
    validate_allowed_path(path, allowed_root, false)?;
    skill_library::verify_skill_tree(snapshot_path, content_hash)?;
    let current = verify_expected_path_state(path, expected_current)?;
    let current_missing = matches!(&current, PathState::Missing);
    if let PathState::Symlink { link_target } = &current {
        let central_root = central_root
            .ok_or_else(|| AppError::conflict("rollbackTarget", "没有中央库证据时拒绝替换链接"))?;
        validate_central_link_target(path, link_target, central_root)?;
    } else if !current_missing {
        return Err(AppError::conflict(
            "rollbackTarget",
            "目录树恢复拒绝覆盖外部文件或目录",
        ));
    }
    let parent = parent_of(path)?;
    let temporary = parent.join(format!(".easytoagents-{}.restore.d", Uuid::new_v4()));
    let quarantine = parent.join(format!(".easytoagents-{}.rollback", Uuid::new_v4()));
    skill_library::copy_skill_tree(snapshot_path, &temporary, content_hash)?;
    if !current_missing {
        fs::rename(path, &quarantine).map_err(|error| {
            AppError::atomic_write(&path.to_string_lossy(), "quarantine_rollback")
                .with_source(error)
        })?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if !current_missing {
            let _ = fs::rename(&quarantine, path);
        }
        let _ = skill_library::remove_skill_tree(&temporary, parent, content_hash);
        return Err(
            AppError::atomic_write(&path.to_string_lossy(), "restore_directory_tree")
                .with_source(error),
        );
    }
    if !current_missing {
        fs::remove_file(&quarantine).map_err(|error| {
            AppError::atomic_write(&quarantine.to_string_lossy(), "cleanup_rollback_link")
                .with_source(error)
        })?;
    }
    sync_directory(parent)
}

fn restore_external_symlink_without_central(
    path: &Path,
    link_target: &Path,
    allowed_root: &Path,
    expected_current: ExpectedPathFingerprint<'_>,
) -> Result<(), AppError> {
    validate_allowed_path(path, allowed_root, false)?;
    if !matches!(
        verify_expected_path_state(path, expected_current)?,
        PathState::Missing
    ) {
        return Err(AppError::conflict(
            "targetPath",
            "恢复目标已被占用，拒绝覆盖",
        ));
    }
    let parent = parent_of(path)?;
    let temporary = parent.join(format!(".easytoagents-{}.link", Uuid::new_v4()));
    symlink(link_target, &temporary).map_err(|error| {
        AppError::atomic_write(&temporary.to_string_lossy(), "create_native_rollback_link")
            .with_source(error)
    })?;
    if let Err(error) = skill_library::rename_import_exclusively(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        let diagnostic = error
            .source()
            .map(str::to_owned)
            .unwrap_or_else(|| error.to_string());
        return Err(
            AppError::conflict("targetPath", "恢复目标已被占用，拒绝覆盖").with_source(diagnostic),
        );
    }
    sync_directory(parent)
}

fn replace_symlink_without_journal(
    path: &Path,
    link_target: &Path,
    central_root: &Path,
    allowed_root: &Path,
    expected_current: ExpectedPathFingerprint<'_>,
) -> Result<(), AppError> {
    validate_allowed_path(path, allowed_root, false)?;
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
    let parent = parent_of(path)?;
    let temporary = parent.join(format!(".easytoagents-{}.link", Uuid::new_v4()));
    symlink(link_target, &temporary).map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), "create_restore_symlink").with_source(error)
    })?;
    validate_allowed_path(path, allowed_root, false)?;
    if let Err(error) = verify_expected_path_state(path, expected_current) {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        return Err(
            AppError::atomic_write(&path.to_string_lossy(), "rename_restore_symlink")
                .with_source(error),
        );
    }
    sync_directory(parent)
}
