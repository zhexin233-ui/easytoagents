pub fn list_snapshots(database: &Database) -> Result<Vec<SnapshotSummary>, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let mut snapshots = Vec::new();
    for row in crate::db::sync::list_snapshot_summaries(database.connection(), &database_path)? {
        let target_type = parse_target_type(&row.target_type)?;
        let storage_kind = parse_snapshot_storage_kind(&row.storage_kind)?;
        snapshots.push(SnapshotSummary {
            snapshot_id: row.id,
            run_id: row.run_id,
            target_id: row.target_id,
            target_path: row.target_path,
            target_type,
            storage_kind,
            restorable: storage_kind != SnapshotStorageKind::MetadataOnly
                || target_type != TargetType::Directory,
            created_at: row.created_at,
        });
    }
    Ok(snapshots)
}

fn snapshot_delete_failure(snapshot_id: &str, error: &AppError) -> SnapshotDeleteFailureDto {
    SnapshotDeleteFailureDto {
        snapshot_id: snapshot_id.to_owned(),
        code: error.code().as_str().to_owned(),
        message: error.message().to_owned(),
    }
}

/// 批量删除私有快照：单项失败 best-effort，成功项在一个 Immediate 事务中统一删行。
/// 命令层不做整单失败——只有基础设施级错误（锁、审计、DB）才整体 `Err`。
pub fn delete_snapshots(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    input: &DeleteSnapshotsInput,
) -> Result<DeleteSnapshotsResultDto, AppError> {
    let _write_guard = write_operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    paths.audit_run_scope([])?;
    let database_path = database.path().to_string_lossy().into_owned();

    let mut ordered_ids: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for snapshot_id in &input.snapshot_ids {
        if seen.insert(snapshot_id.as_str()) {
            ordered_ids.push(snapshot_id.clone());
        }
    }

    let mut failures: Vec<SnapshotDeleteFailureDto> = Vec::new();
    let mut removable: Vec<(String, String, PathBuf, SnapshotStorageKind, Option<String>)> =
        Vec::new();
    for snapshot_id in &ordered_ids {
        let Some(row) = crate::db::sync::load_snapshot_for_delete(
            database.connection(),
            snapshot_id,
            &database_path,
        )? else {
            failures.push(snapshot_delete_failure(
                snapshot_id,
                &AppError::not_found("snapshot", snapshot_id),
            ));
            continue;
        };
        if crate::db::sync::has_active_sync_run(
            database.connection(),
            &row.run_id,
            &database_path,
        )? {
            failures.push(snapshot_delete_failure(
                snapshot_id,
                &AppError::new(
                    ErrorCode::Conflict,
                    "快照仍被活动 run 引用，需等待恢复完成后删除",
                    true,
                )
                .with_action(RecoveryAction::ReviewConflict),
            ));
            continue;
        }
        let referenced = crate::db::native_resources::snapshot_is_referenced(
            database.connection(),
            snapshot_id,
            &database_path,
        )?;
        if referenced {
            failures.push(snapshot_delete_failure(
                snapshot_id,
                &AppError::conflict(
                    "snapshot",
                    "快照仍被已禁用的项目原生资源引用，请先恢复该资源",
                ),
            ));
            continue;
        }
        let snapshot_path = PathBuf::from(row.snapshot_path);
        let storage_kind = match parse_snapshot_storage_kind(&row.storage_kind) {
            Ok(value) => value,
            Err(error) => {
                failures.push(snapshot_delete_failure(snapshot_id, &error));
                continue;
            }
        };
        if let Err(error) = validate_snapshot_storage_path(
            paths,
            &row.run_id,
            snapshot_id,
            &snapshot_path,
            storage_kind,
        ) {
            failures.push(snapshot_delete_failure(snapshot_id, &error));
            continue;
        }
        removable.push((
            snapshot_id.clone(),
            row.run_id,
            snapshot_path,
            storage_kind,
            row.content_hash,
        ));
    }

    paths.audit_run_scope(removable.iter().map(|(_, run_id, ..)| run_id.as_str()))?;
    let mut deleted_ids: Vec<String> = Vec::with_capacity(removable.len());
    if !removable.is_empty() {
        // 先在一个事务里退役数据库行：把身份登记进 retired_snapshot_cleanup 再删
        // snapshots 行并提交。之后逐个删文件；删成功就清掉队列项，删失败的留在
        // 队列由 Database::open 重试。这样提交失败时行与文件都还在（可重试），
        // 文件删除失败也不会留下指向缺失文件的活行或无人认领的孤儿文件。
        let transaction = database
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                AppError::database(&database_path, "begin_delete_snapshots").with_source(error)
            })?;
        for (snapshot_id, run_id, snapshot_path, storage_kind, content_hash) in &removable {
            crate::db::sync::queue_and_delete_snapshot(
                &transaction,
                snapshot_id,
                run_id,
                snapshot_path,
                storage_kind.as_str(),
                content_hash.as_deref(),
                &database_path,
            )?;
        }
        transaction.commit().map_err(|error| {
            AppError::database(&database_path, "commit_delete_snapshots").with_source(error)
        })?;
    }

    for (snapshot_id, _run_id, snapshot_path, storage_kind, content_hash) in &removable {
        deleted_ids.push(snapshot_id.clone());
        let removal = if fs::symlink_metadata(snapshot_path)
            .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
        {
            Ok(())
        } else {
            match storage_kind {
                SnapshotStorageKind::DirectoryTree => content_hash
                    .as_deref()
                    .ok_or_else(|| AppError::conflict("snapshot", "目录树快照缺少 hash"))
                    .and_then(|hash| {
                        let owner = snapshot_path.parent().ok_or_else(|| {
                            AppError::invalid_input("snapshotPath", "快照缺少父目录")
                        })?;
                        skill_library::remove_skill_tree(snapshot_path, owner, hash)
                    }),
                SnapshotStorageKind::PayloadFile | SnapshotStorageKind::MetadataOnly => {
                    fs::remove_file(snapshot_path).map_err(|error| {
                        AppError::atomic_write(&snapshot_path.to_string_lossy(), "remove_snapshot")
                            .with_source(error)
                    })
                }
            }
        };
        match removal {
            Ok(()) => {}
            Err(error) if error.code() == ErrorCode::NotFound => {}
            // 行已退役；文件留给启动清理队列重试，不再回报为条目失败。
            Err(_) => continue,
        }
        crate::db::sync::dequeue_snapshot_cleanup(
            database.connection(),
            snapshot_id,
            &database_path,
        )?;
    }

    Ok(DeleteSnapshotsResultDto {
        deleted_ids,
        failures,
    })
}

pub fn detect_interrupted_run(
    database: &Database,
    paths: &AppPaths,
) -> Result<Option<InterruptedRunPlan>, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let Some(active) = crate::db::sync::load_active_sync_run(
        database.connection(),
        &database_path,
    )? else {
        return Ok(None);
    };
    let run_id = active.id;
    let status = active.status;
    let Some(journal_path) = active.journal_path else {
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
    let journal = read_journal(&journal_path)?
        .ok_or_else(|| AppError::parse(&journal_path.to_string_lossy(), "journal"))?;
    if journal.run_id != run_id {
        return Err(AppError::conflict(
            "journal",
            "journal 与活动 run 标识不一致",
        ));
    }
    let mut targets = Vec::new();
    for target in journal.targets {
        // v18 会从混合历史 run 中移除项目 Prompt target，但保留同一 run
        // 中仍有效的 MCP/Skill/Hook target。journal 是旧版本的文件快照，
        // 不能把已从数据库退役的 target 再暴露为可恢复对象。
        if !crate::db::sync::managed_target_exists(
            database.connection(),
            &target.target_id,
            &database_path,
        )? {
            continue;
        }
        let state = capture_path_state(Path::new(&target.target_path));
        let (current_type, current_fingerprint, error_code) = match state {
            Ok(state) => (Some(state.target_type()), Some(state.fingerprint()), None),
            Err(error) => (None, None, Some(error.code())),
        };
        targets.push(InterruptedTargetPlan {
            target_id: target.target_id,
            target_path: target.target_path,
            snapshot_id: target.snapshot_id,
            // 前端 DTO 保持字符串：旧 journal 的未知阶段以 "unknown" 透出，不改 RPC 合同。
            phase: target.phase.as_str().to_owned(),
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
    let snapshot = load_snapshot_record(database, paths, snapshot_id, allowed_root, None)?;
    paths.audit_run_scope([snapshot.run_id.as_str()])?;
    if matches!(&snapshot.state, PathState::Directory { .. })
        && snapshot.storage_kind != SnapshotStorageKind::DirectoryTree
    {
        return Err(AppError::conflict(
            "snapshot",
            "旧目录占位快照没有目录树内容，不能恢复",
        ));
    }
    let target_id = snapshot.target_id.as_deref().ok_or_else(|| {
        AppError::invalid_input("snapshotId", "旧快照缺少受管目标身份，不能自动恢复")
    })?;
    let current = capture_path_state(&snapshot.target_path)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let target_identity = crate::db::sync::load_restore_target_identity(
        database.connection(),
        target_id,
        &snapshot.run_id,
        &database_path,
        "load_restore_target_identity",
    )?
        .ok_or_else(|| AppError::not_found("managedTarget", target_id))?;
    let identity = &target_identity.identity;
    let mut envelope: PersistedPreviewEnvelope =
        serde_json::from_str(&target_identity.redacted_diff_json).map_err(|error| {
            AppError::database(&database_path, "parse_restore_descriptor").with_source(error)
        })?;
    if identity.tool != envelope.descriptor.tool.as_str()
        || identity.artifact_kind != envelope.descriptor.artifact_kind.as_str()
        || identity.scope != envelope.descriptor.scope.as_str()
        || envelope.descriptor.path.as_deref() != Some(identity.target_path.as_str())
        || identity.project_root != envelope.descriptor.project_root
        || (envelope.descriptor.scope == Scope::Global && identity.project_id.is_some())
    {
        return Err(AppError::conflict(
            "snapshot",
            "快照目标身份已与当前受管目标分离",
        ));
    }
    validate_snapshot_target_relationship(
        &snapshot.target_path,
        Path::new(&identity.target_path),
        &envelope,
    )?;
    let target_row_version = u32::try_from(identity.row_version).map_err(|error| {
        AppError::invalid_input("snapshot", "目标 row_version 超出安全范围").with_source(error)
    })?;
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
    let envelope_json = serde_json::to_string(&envelope).map_err(|error| {
        AppError::database(&database_path, "serialize_restore_preview").with_source(error)
    })?;
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_restore_preview").with_source(error)
        })?;
    crate::db::sync::insert_sync_run(
        &transaction,
        crate::db::sync::SyncRunInsert {
            id: &restore_id,
            kind: "restore",
            status: "previewed",
            scope: &identity.scope,
            project_id: identity.project_id.as_deref(),
            db_version: i64::from(envelope.target_row_version),
            database_path: &database_path,
            operation: "insert_restore_preview",
        },
    )?;
    crate::db::sync::insert_sync_item(
        &transaction,
        crate::db::sync::SyncItemInsert {
            id: &Uuid::new_v4().to_string(),
            run_id: &restore_id,
            target_id,
            change_kind: restore_change_kind(&current, &snapshot.state).as_str(),
            status: "in_sync",
            redacted_diff_json: &envelope_json,
            warning_codes_json: "[]",
            error_code: None,
            target_order: 0,
            database_path: &database_path,
            operation: "insert_restore_preview_item",
        },
    )?;
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_restore_preview").with_source(error)
    })?;
    Ok(RestorePreview {
        preview_id: restore_id,
        snapshot_id: snapshot_id.to_owned(),
        target_path: snapshot.target_path.to_string_lossy().into_owned(),
        current_type: current.target_type(),
        snapshot_type: snapshot.state.target_type(),
        storage_kind: snapshot.storage_kind,
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
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    paths.audit_run_scope([restore_preview_id])?;
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
    // 被恢复快照所属的原 run 目录也在写作用域内（恢复会读它并可能清理临时项）。
    paths.audit_run_scope([snapshot.run_id.as_str()])?;
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
        operation: JournalOperation::Restore,
        phase: TargetPhase::Snapshotting,
        targets: Vec::new(),
        failure: None,
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
            directory_tree_hash: None,
            known_state: None,
        },
    )?;
    journal.targets.push(JournalTarget {
        target_id: snapshot.target_id.clone().unwrap_or_default(),
        target_path: snapshot.target_path.to_string_lossy().into_owned(),
        snapshot_id: Some(second_snapshot.id.clone()),
        snapshot_path: Some(second_snapshot.snapshot_path.to_string_lossy().into_owned()),
        phase: TargetPhase::Snapshotted,
        before_fingerprint: Some(second_snapshot.state.fingerprint()),
        after_fingerprint: None,
        temporary_path: None,
        temporary_fingerprint: None,
        quarantine_path: None,
        quarantine_fingerprint: None,
        takeover_entry_type: None,
        directory_tree_hash: None,
        snapshot_storage_kind: Some(second_snapshot.storage_kind),
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
        &second_snapshot.state,
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
    let restored_matches_snapshot = match capture_path_state(&snapshot.target_path) {
        Ok(PathState::Directory { .. })
            if snapshot.storage_kind == SnapshotStorageKind::DirectoryTree =>
        {
            snapshot.directory_tree_hash.as_deref().is_some_and(|hash| {
                skill_library::verify_skill_tree(&snapshot.target_path, hash).is_ok()
            })
        }
        Ok(current) => current.fingerprint() == snapshot.state.fingerprint(),
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
    };
    if !restored_matches_snapshot {
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
    journal.phase = TargetPhase::ReadyToFinalizeDatabase;
    journal.targets[0].phase = TargetPhase::Verified;
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
        journal.phase = TargetPhase::CrashedDuringDatabaseFinalize;
        persist_journal(paths, &journal)?;
        return Err(error);
    }
    journal.phase = TargetPhase::Succeeded;
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
    let Some(mut journal) = read_journal(&journal_path).ok().flatten() else {
        return Ok(());
    };
    let mut changed = false;
    for target in &mut journal.targets {
        if Path::new(&target.target_path) != restored_target_path {
            continue;
        }
        if let Some(temporary_text) = target.temporary_path.clone() {
            let expected_fingerprint =
                target.temporary_fingerprint.as_deref().ok_or_else(|| {
                    AppError::conflict("temporaryPath", "旧 journal 缺少临时路径所有权指纹")
                })?;
            let temporary = PathBuf::from(temporary_text);
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
                    fs::remove_file(&temporary).map_err(|error| {
                        AppError::atomic_write(&temporary.to_string_lossy(), "cleanup_temporary")
                            .with_source(error)
                    })?;
                    sync_directory(parent_of(&temporary)?)?;
                }
                PathState::Directory { .. } => {
                    return Err(AppError::conflict(
                        "temporaryPath",
                        "拒绝删除占用临时路径的目录",
                    ));
                }
            }
            target.temporary_path = None;
            target.temporary_fingerprint = None;
            changed = true;
        }
        if let Some(quarantine) = target.quarantine_path.as_deref() {
            validate_allowed_path(Path::new(quarantine), allowed_root, false)?;
            cleanup_takeover_quarantine(target)?;
            changed = true;
        }
    }
    if changed {
        persist_journal(paths, &journal)?;
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
        .map_err(|error| {
            AppError::database(&database_path, "begin_claim_restore").with_source(error)
        })?;
    validate_restore_identity(
        &transaction,
        item,
        snapshot,
        restore_preview_id,
        &database_path,
    )?;
    let status = crate::db::sync::load_sync_run_status(
        &transaction,
        restore_preview_id,
        "restore",
        &database_path,
        "read_restore_claim",
    )?
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
        let retired = crate::db::sync::retire_interrupted_run(
            &transaction,
            &active_id,
            &database_path,
        )?;
        if retired != 1 {
            return Err(AppError::write_in_progress(&active_id, &active_status));
        }
        source_run_requires_resolution = true;
    }
    let updated = crate::db::sync::claim_restore_run(
        &transaction,
        restore_preview_id,
        journal_path,
    )?;
    if updated != 1 {
        return Err(AppError::preview_already_consumed(
            restore_preview_id,
            "not_previewed",
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_claim_restore").with_source(error)
    })?;
    Ok(source_run_requires_resolution)
}

fn interrupted_run_matches_before_state(paths: &AppPaths, run_id: &str) -> Result<bool, AppError> {
    let journal_path = paths.journals().join(format!("{run_id}.json"));
    validate_allowed_path(&journal_path, paths.journals(), false)?;
    let Some(journal) = read_journal(&journal_path)? else {
        return Ok(false);
    };
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
    let identity = crate::db::sync::load_managed_target_identity(
        connection,
        &item.target_id,
        database_path,
        "verify_restore_identity",
    )?
        .ok_or_else(|| AppError::stale_preview(preview_id, &item.target_id))?;
    let descriptor = &item.envelope.descriptor;
    if u32::try_from(identity.row_version).ok() != Some(item.envelope.target_row_version)
        || identity.tool != descriptor.tool.as_str()
        || identity.artifact_kind != descriptor.artifact_kind.as_str()
        || identity.scope != descriptor.scope.as_str()
        || identity.target_path != item.target_path
        || descriptor.path.as_deref() != Some(identity.target_path.as_str())
        || identity.project_root != descriptor.project_root
        || (descriptor.scope == Scope::Global && identity.project_id.is_some())
    {
        return Err(AppError::stale_preview(preview_id, &item.target_id));
    }
    validate_snapshot_target_relationship(
        &snapshot.target_path,
        Path::new(&identity.target_path),
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
        PathState::Symlink { link_target } => {
            if let Some(central_root) = central_root {
                Mutation::ReplaceSymlink {
                    link_target: link_target.clone(),
                    central_root: central_root.to_path_buf(),
                    allow_external_target: true,
                }
            } else {
                Mutation::RestoreNativeSymlink {
                    link_target: link_target.clone(),
                }
            }
        }
        PathState::Directory { .. }
            if snapshot.storage_kind == SnapshotStorageKind::DirectoryTree =>
        {
            Mutation::RestoreDirectoryTree {
                snapshot_path: snapshot.snapshot_path.clone(),
                content_hash: snapshot
                    .directory_tree_hash
                    .clone()
                    .ok_or_else(|| AppError::invalid_input("snapshot", "目录树快照缺少 hash"))?,
            }
        }
        PathState::Directory { .. } => {
            return Err(AppError::conflict("snapshot", "旧目录占位快照不能递归恢复"));
        }
    };
    Ok(PendingMutation {
        target_id: snapshot.target_id.clone().unwrap_or_default(),
        target_index: 0,
        path: snapshot.target_path.clone(),
        allowed_root: allowed_root.to_path_buf(),
        central_root: central_root.map(Path::to_path_buf),
        expected_before_fingerprint: capture_path_state(&snapshot.target_path)?.fingerprint(),
        expected_after_fingerprint: if snapshot.storage_kind == SnapshotStorageKind::DirectoryTree {
            String::new()
        } else {
            snapshot.state.fingerprint()
        },
        before_state: None,
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
        .map_err(|error| {
            AppError::database(&database_path, "begin_finish_restore").with_source(error)
        })?;
    if let Some(target_id) = target_id {
        let target_updates = crate::db::sync::mark_restored_target(
            &transaction,
            target_id,
            expected_target_row_version,
            &database_path,
        )?;
        if target_updates != 1 {
            return Err(AppError::stale_preview(run_id, target_id));
        }
    }
    let run_updates = crate::db::sync::finish_sync_run(
        &transaction,
        run_id,
        "restoring",
        &database_path,
        "finish_restore_run",
    )?;
    if run_updates != 1 {
        return Err(AppError::write_in_progress(run_id, "not_restoring"));
    }
    if let Some(source_run_id) = source_run_id {
        let source_updates = crate::db::sync::finish_source_recovery(
            &transaction,
            source_run_id,
            source_run_resolved,
            &database_path,
        )?;
        if source_updates != 1 {
            return Err(AppError::write_in_progress(
                source_run_id,
                "source_recovery_changed",
            ));
        }
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_finish_restore").with_source(error)
    })
}

/// 快照必须位于 `snapshots_root/<run_id>/<snapshot_id>.snapshot[.d]`，
/// 且父目录 canonicalize 后仍在快照根内；恢复与删除共用这条越权边界。
fn validate_snapshot_storage_path(
    paths: &AppPaths,
    run_id: &str,
    snapshot_id: &str,
    snapshot_path: &Path,
    storage_kind: SnapshotStorageKind,
) -> Result<(), AppError> {
    validate_normal_absolute(snapshot_path, "snapshotPath")?;
    let expected_snapshot_path = paths.snapshots().join(run_id).join(match storage_kind {
        SnapshotStorageKind::DirectoryTree => format!("{snapshot_id}.snapshot.d"),
        SnapshotStorageKind::PayloadFile | SnapshotStorageKind::MetadataOnly => {
            format!("{snapshot_id}.snapshot")
        }
    });
    if snapshot_path != expected_snapshot_path {
        return Err(AppError::conflict(
            "snapshotPath",
            "快照路径与 run/snapshot 身份不一致",
        ));
    }
    let snapshot_parent = snapshot_path
        .parent()
        .ok_or_else(|| AppError::invalid_input("snapshotPath", "快照缺少父目录"))?;
    let canonical_parent = fs::canonicalize(snapshot_parent).map_err(|error| {
        AppError::not_found("snapshotDirectory", &snapshot_parent.to_string_lossy())
            .with_source(error)
    })?;
    if canonical_parent != snapshot_parent || !canonical_parent.starts_with(paths.snapshots()) {
        return Err(AppError::conflict("snapshotPath", "快照父目录包含未知链接"));
    }
    Ok(())
}

fn load_snapshot_record(
    database: &Database,
    paths: &AppPaths,
    snapshot_id: &str,
    allowed_root: &Path,
    central_root: Option<&Path>,
) -> Result<SnapshotRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let row = crate::db::sync::load_snapshot_record(
        database.connection(),
        snapshot_id,
        &database_path,
    )?
        .ok_or_else(|| AppError::not_found("snapshot", snapshot_id))?;
    let target_path = PathBuf::from(&row.target_path);
    validate_allowed_path(&target_path, allowed_root, false)?;
    let snapshot_path = PathBuf::from(&row.snapshot_path);
    let storage_kind = parse_snapshot_storage_kind(&row.storage_kind)?;
    validate_snapshot_storage_path(paths, &row.run_id, snapshot_id, &snapshot_path, storage_kind)?;
    match storage_kind {
        SnapshotStorageKind::DirectoryTree => {
            let metadata = fs::symlink_metadata(&snapshot_path).map_err(|error| {
                AppError::not_found("snapshotTree", &snapshot_path.to_string_lossy())
                    .with_source(error)
            })?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(AppError::conflict("snapshot", "目录树快照类型无效"));
            }
        }
        SnapshotStorageKind::PayloadFile | SnapshotStorageKind::MetadataOnly => {
            ensure_private_file(&snapshot_path)?;
        }
    }
    let state =
        match row.target_type.as_str() {
            "missing" => PathState::Missing,
            "file" => {
                let bytes = fs::read(&snapshot_path).map_err(|error| {
                    AppError::permission(&snapshot_path.to_string_lossy(), "read_snapshot")
                        .with_source(error)
                })?;
                let hash = hash_bytes(&bytes);
                if row.content_hash.as_deref() != Some(&hash) {
                    return Err(AppError::conflict("snapshot", "快照内容 hash 不匹配"));
                }
                PathState::File {
                    bytes,
                    hash,
                    mode: row
                        .file_mode
                        .and_then(|mode| u32::try_from(mode).ok())
                        .ok_or_else(|| AppError::invalid_input("snapshot", "文件快照缺少 mode"))?,
                    stat: StatSignature::default(),
                }
            }
            "symlink" => {
                let link_target = PathBuf::from(row.link_target.ok_or_else(|| {
                    AppError::invalid_input("snapshot", "链接快照缺少 linkTarget")
                })?);
                PathState::Symlink { link_target }
            }
            "directory" => {
                if storage_kind == SnapshotStorageKind::DirectoryTree {
                    let hash = row.content_hash.as_deref().ok_or_else(|| {
                        AppError::invalid_input("snapshot", "目录树快照缺少 hash")
                    })?;
                    skill_library::verify_skill_tree(&snapshot_path, hash)?;
                }
                // 原始目录身份只用于旧 metadata-only 记录的保守不可恢复语义；
                // directory_tree 的恢复结果会按完整树 hash 验证。
                PathState::Directory {
                    device: 0,
                    inode: 0,
                }
            }
            _ => return Err(AppError::invalid_input("snapshot", "未知快照目标类型")),
        };
    Ok(SnapshotRecord {
        id: snapshot_id.to_owned(),
        run_id: row.run_id,
        target_id: row.target_id,
        target_path,
        snapshot_path,
        allowed_root: allowed_root.to_path_buf(),
        central_root: central_root.map(Path::to_path_buf),
        row_version: u32::try_from(row.row_version).map_err(|error| {
            AppError::invalid_input("snapshot", "快照 row_version 超出安全范围").with_source(error)
        })?,
        state,
        storage_kind,
        directory_tree_hash: if storage_kind == SnapshotStorageKind::DirectoryTree {
            row.content_hash
        } else {
            None
        },
    })
}

fn parse_snapshot_storage_kind(value: &str) -> Result<SnapshotStorageKind, AppError> {
    match value {
        "payload_file" => Ok(SnapshotStorageKind::PayloadFile),
        "metadata_only" => Ok(SnapshotStorageKind::MetadataOnly),
        "directory_tree" => Ok(SnapshotStorageKind::DirectoryTree),
        _ => Err(AppError::invalid_input(
            "snapshotStorageKind",
            "数据库包含未知快照存储类型",
        )),
    }
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
