fn apply_mutation(
    paths: &AppPaths,
    journal: &mut RunJournal,
    journal_index: usize,
    mutation: &PendingMutation,
    before_state: &PathState,
    fault: &dyn ApplyFaultInjector,
) -> Result<(), MutationFailure> {
    validate_allowed_path(&mutation.path, &mutation.allowed_root, false)?;
    let expected_run_id = journal.run_id.clone();
    let expected_target_id = mutation.target_id.clone();
    let expected_before_fingerprint = before_state.fingerprint();
    let expected = ExpectedPathFingerprint {
        run_id: &expected_run_id,
        target_id: &expected_target_id,
        fingerprint: &expected_before_fingerprint,
    };
    verify_known_path_state(&mutation.path, Some(before_state), expected)?;
    journal.targets[journal_index].phase = TargetPhase::Writing;
    persist_journal(paths, journal)?;
    match &mutation.mutation {
        Mutation::CreateDirectory => {
            if !matches!(
                verify_expected_path_state(&mutation.path, expected)?,
                PathState::Missing
            ) {
                return Err(AppError::stale_preview(&expected_run_id, &expected_target_id).into());
            }
            journal.targets[journal_index].phase = TargetPhase::DirectoryCreatePending;
            persist_journal(paths, journal)?;
            let created =
                match create_private_directory_nofollow(&mutation.path, &mutation.allowed_root) {
                    Ok(created) => created,
                    Err(error) => {
                        journal.targets[journal_index].phase = TargetPhase::DirectoryCreateFailed;
                        persist_journal(paths, journal)?;
                        return Err(error.into());
                    }
                };
            journal.targets[journal_index].phase = TargetPhase::DirectoryCreated;
            persist_journal(paths, journal)?;
            finalize_created_directory(created, &mutation.path)?;
        }
        Mutation::WriteFile { bytes, mode } => atomic_replace_file(
            &mutation.path,
            bytes,
            *mode,
            &mutation.allowed_root,
            Some(expected),
            Some(before_state),
            Some(ApplyContext {
                index: mutation.target_index,
                fault,
                paths,
                journal,
                journal_index,
            }),
        )?,
        Mutation::Remove => {
            validate_allowed_path(&mutation.path, &mutation.allowed_root, false)?;
            let state = verify_expected_path_state(&mutation.path, expected)?;
            match state {
                PathState::File { .. } => {
                    fs::remove_file(&mutation.path).map_err(|error| {
                        AppError::atomic_write(&mutation.path.to_string_lossy(), "remove_target")
                            .with_source(error)
                    })?;
                    journal.targets[journal_index].phase = TargetPhase::Removed;
                    sync_directory(parent_of(&mutation.path)?)?;
                }
                PathState::Symlink { link_target } => {
                    let central_root = mutation.central_root.as_deref().ok_or_else(|| {
                        AppError::conflict("targetPath", "没有中央库所有权证据时拒绝删除链接")
                    })?;
                    validate_central_link_target(&mutation.path, &link_target, central_root)?;
                    fs::remove_file(&mutation.path).map_err(|error| {
                        AppError::atomic_write(&mutation.path.to_string_lossy(), "remove_symlink")
                            .with_source(error)
                    })?;
                    journal.targets[journal_index].phase = TargetPhase::Removed;
                    sync_directory(parent_of(&mutation.path)?)?;
                }
                PathState::Missing => {}
                PathState::Directory { .. } => {
                    if mutation.central_root.is_none() {
                        return Err(AppError::conflict("targetPath", "拒绝删除普通目录").into());
                    }
                    fs::remove_dir(&mutation.path).map_err(|error| {
                        AppError::conflict(
                            "targetPath",
                            "只允许删除由 Skills Apply 创建且仍为空的目录",
                        )
                        .with_source(error)
                    })?;
                    journal.targets[journal_index].phase = TargetPhase::Removed;
                    sync_directory(parent_of(&mutation.path)?)?;
                }
            }
        }
        Mutation::ReplaceSymlink {
            link_target,
            central_root,
            allow_external_target,
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
            *allow_external_target,
        )?,
        Mutation::TakeoverSymlink {
            link_target,
            central_root,
            entry_type,
            content_hash,
            evidence_fingerprint,
        } => atomic_takeover_symlink(
            &mutation.path,
            link_target,
            central_root,
            &mutation.allowed_root,
            expected,
            *entry_type,
            content_hash,
            evidence_fingerprint,
            mutation.target_index,
            fault,
            paths,
            journal,
            journal_index,
        )?,
        Mutation::RestoreDirectoryTree {
            snapshot_path,
            content_hash,
        } => atomic_restore_directory_tree(
            &mutation.path,
            snapshot_path,
            content_hash,
            mutation.central_root.as_deref(),
            &mutation.allowed_root,
            expected,
            mutation.target_index,
            fault,
            paths,
            journal,
            journal_index,
        )?,
        Mutation::RemoveNativeSkill {
            entry_type,
            content_hash,
        } => apply_remove_native_skill(
            mutation,
            *entry_type,
            content_hash.as_deref(),
            expected,
            fault,
            paths,
            journal,
            journal_index,
        )?,
        Mutation::RestoreNativeSymlink { link_target } => apply_restore_native_symlink(
            mutation,
            link_target,
            expected,
            fault,
            paths,
            journal,
            journal_index,
        )?,
    }
    let state = capture_path_state(&mutation.path)?;
    let expected_after_fingerprint = if matches!(
        &mutation.mutation,
        Mutation::CreateDirectory | Mutation::RestoreDirectoryTree { .. }
    ) {
        if !matches!(state, PathState::Directory { .. }) {
            journal.targets[journal_index].phase = TargetPhase::ExternalChangeAfterWrite;
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Error(AppError::stale_preview(
                &expected_run_id,
                &expected_target_id,
            )));
        }
        if let Mutation::RestoreDirectoryTree { content_hash, .. } = &mutation.mutation {
            skill_library::verify_skill_tree(&mutation.path, content_hash)?;
        }
        state.fingerprint()
    } else {
        mutation.expected_after_fingerprint.clone()
    };
    if state.fingerprint() != expected_after_fingerprint {
        journal.targets[journal_index].phase = TargetPhase::ExternalChangeAfterWrite;
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        persist_journal(paths, journal)?;
        return Err(MutationFailure::Error(AppError::stale_preview(
            &expected_run_id,
            &expected_target_id,
        )));
    }
    journal.targets[journal_index].phase = TargetPhase::Written;
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
            journal.targets[journal_index].phase = TargetPhase::CrashedAfterTarget;
            persist_journal(paths, journal)?;
            Err(MutationFailure::Crash(AppError::atomic_write(
                &mutation.path.to_string_lossy(),
                "simulated_crash_after_target",
            )))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_remove_native_skill(
    mutation: &PendingMutation,
    entry_type: super::NativeResourceEntryType,
    content_hash: Option<&str>,
    expected: ExpectedPathFingerprint<'_>,
    fault: &dyn ApplyFaultInjector,
    paths: &AppPaths,
    journal: &mut RunJournal,
    journal_index: usize,
) -> Result<(), MutationFailure> {
    use super::NativeResourceEntryType;
    match fault.decide(&ApplyFaultEvent::BeforeRename {
        index: mutation.target_index,
        path: mutation.path.clone(),
    }) {
        ApplyFaultDecision::Continue => {}
        ApplyFaultDecision::Fail => {
            return Err(AppError::atomic_write(
                &mutation.path.to_string_lossy(),
                "fault_before_native_remove",
            )
            .into());
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = TargetPhase::CrashedBeforeNativeRemove;
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Crash(AppError::atomic_write(
                &mutation.path.to_string_lossy(),
                "simulated_crash_before_native_remove",
            )));
        }
    }
    verify_expected_path_state(&mutation.path, expected)?;
    match entry_type {
        NativeResourceEntryType::Symlink => {
            let PathState::Symlink { .. } = capture_path_state(&mutation.path)? else {
                return Err(AppError::stale_preview(expected.run_id, expected.target_id).into());
            };
            fs::remove_file(&mutation.path).map_err(|error| {
                AppError::atomic_write(&mutation.path.to_string_lossy(), "remove_native_symlink")
                    .with_source(error)
            })?;
        }
        NativeResourceEntryType::Directory => {
            let hash = content_hash.ok_or_else(|| {
                AppError::invalid_input("projectNativeAction", "目录删除缺少树 hash")
            })?;
            skill_library::remove_skill_tree(&mutation.path, parent_of(&mutation.path)?, hash)?;
        }
        NativeResourceEntryType::McpEntry => {
            return Err(AppError::invalid_input(
                "projectNativeAction",
                "非 Skill 入口不能走目录删除",
            )
            .into());
        }
    }
    sync_directory(parent_of(&mutation.path)?)?;
    journal.targets[journal_index].phase = TargetPhase::Removed;
    persist_journal(paths, journal)?;
    Ok(())
}

fn apply_restore_native_symlink(
    mutation: &PendingMutation,
    link_target: &Path,
    expected: ExpectedPathFingerprint<'_>,
    fault: &dyn ApplyFaultInjector,
    paths: &AppPaths,
    journal: &mut RunJournal,
    journal_index: usize,
) -> Result<(), MutationFailure> {
    if !matches!(
        verify_expected_path_state(&mutation.path, expected)?,
        PathState::Missing
    ) {
        return Err(AppError::conflict("targetPath", "恢复目标已被占用，拒绝覆盖").into());
    }
    let parent = parent_of(&mutation.path)?;
    let temporary = parent.join(format!(".easytoagents-{}.link", Uuid::new_v4()));
    symlink(link_target, &temporary).map_err(|error| {
        AppError::atomic_write(&temporary.to_string_lossy(), "create_native_restore_link")
            .with_source(error)
    })?;
    journal.targets[journal_index].phase = TargetPhase::NativeLinkPending;
    journal.targets[journal_index].temporary_path = Some(temporary.to_string_lossy().into_owned());
    journal.targets[journal_index].temporary_fingerprint =
        Some(capture_path_state(&temporary)?.fingerprint());
    persist_journal(paths, journal)?;
    match fault.decide(&ApplyFaultEvent::BeforeRename {
        index: mutation.target_index,
        path: mutation.path.clone(),
    }) {
        ApplyFaultDecision::Continue => {}
        ApplyFaultDecision::Fail => {
            let _ = fs::remove_file(&temporary);
            return Err(AppError::atomic_write(
                &mutation.path.to_string_lossy(),
                "fault_before_native_link",
            )
            .into());
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = TargetPhase::CrashedBeforeNativeLink;
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Crash(AppError::atomic_write(
                &mutation.path.to_string_lossy(),
                "simulated_crash_before_native_link",
            )));
        }
    }
    if let Err(error) = skill_library::rename_import_exclusively(&temporary, &mutation.path) {
        let _ = fs::remove_file(&temporary);
        let diagnostic = error
            .source()
            .map(str::to_owned)
            .unwrap_or_else(|| error.to_string());
        return Err(
            AppError::conflict("targetPath", "恢复目标已被占用，拒绝覆盖")
                .with_source(diagnostic)
                .into(),
        );
    }
    sync_directory(parent)?;
    journal.targets[journal_index].phase = TargetPhase::Renamed;
    journal.targets[journal_index].temporary_path = None;
    journal.targets[journal_index].temporary_fingerprint = None;
    persist_journal(paths, journal)?;
    Ok(())
}

/// 原子替换在重命名阶段需要的故障注入与日志上下文。
///
/// 以前这里使用五元组，调用方与解构顺序很容易错位；显式结构体也让
/// `apply` 的写入上下文成为可审阅的边界。
struct ApplyContext<'a> {
    index: usize,
    fault: &'a dyn ApplyFaultInjector,
    paths: &'a AppPaths,
    journal: &'a mut RunJournal,
    journal_index: usize,
}

fn atomic_replace_file(
    path: &Path,
    bytes: &[u8],
    mode: u32,
    allowed_root: &Path,
    expected_current: Option<ExpectedPathFingerprint<'_>>,
    known_state: Option<&PathState>,
    fault_context: Option<ApplyContext<'_>>,
) -> Result<(), MutationFailure> {
    validate_allowed_path(path, allowed_root, false)?;
    let current = match expected_current {
        Some(expected) => verify_known_path_state(path, known_state, expected)?,
        None => capture_path_state(path)?,
    };
    match current {
        PathState::Missing | PathState::File { .. } => {}
        PathState::Directory { .. } | PathState::Symlink { .. } => {
            return Err(AppError::conflict("targetPath", "文件原子写拒绝覆盖目录或链接").into());
        }
    }
    let parent = parent_of(path)?;
    // Cursor 规则文件位于 `rules/` 子目录，父目录可能尚不存在；逐分量安全
    // 创建（拒绝 symlink 祖先），已存在的父目录维持既有行为不动。
    if fs::symlink_metadata(parent).is_err() {
        crate::security::ensure_private_directory(parent).map_err(|error| {
            AppError::atomic_write(&path.to_string_lossy(), "create_parent").with_source(error)
        })?;
    }
    let temporary = parent.join(format!(".easytoagents-{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE_MODE)
        .open(&temporary)
        .map_err(|error| {
            AppError::atomic_write(&path.to_string_lossy(), "create_temporary").with_source(error)
        })?;
    // 先落数据再改权限，最后一次 fsync 同时覆盖内容与元数据；
    // 以前是两次 sync_all（写后一次、chmod 后一次），多出的那次纯属浪费。
    if let Err(error) = file
        .write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| fs::set_permissions(&temporary, fs::Permissions::from_mode(mode & 0o7777)))
    {
        let _ = fs::remove_file(&temporary);
        return Err(
            AppError::atomic_write(&path.to_string_lossy(), "flush_temporary")
                .with_source(error)
                .into(),
        );
    }
    if let Err(error) = fsync_file(&file, path, "flush_temporary") {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    drop(file);
    if let Some(context) = fault_context {
        context.journal.targets[context.journal_index].phase = TargetPhase::RenamePending;
        context.journal.targets[context.journal_index].temporary_path =
            Some(temporary.to_string_lossy().into_owned());
        // 临时文件的内容与权限就是刚写入并 fsync 的 bytes/mode，直接由内存计算指纹，
        // 不再把刚写的文件整个读回来。
        context.journal.targets[context.journal_index].temporary_fingerprint = Some(
            PathState::File {
                hash: hash_bytes(bytes),
                bytes: Vec::new(),
                mode: mode & 0o7777,
                stat: StatSignature::default(),
            }
            .fingerprint(),
        );
        if let Err(error) = persist_journal(context.paths, context.journal) {
            let _ = fs::remove_file(&temporary);
            let _ = sync_directory(parent);
            context.journal.targets[context.journal_index].temporary_path = None;
            context.journal.targets[context.journal_index].temporary_fingerprint = None;
            return Err(MutationFailure::Error(error));
        }
        match context.fault.decide(&ApplyFaultEvent::BeforeRename {
            index: context.index,
            path: path.to_path_buf(),
        }) {
            ApplyFaultDecision::Continue => {}
            ApplyFaultDecision::Fail => {
                let _ = fs::remove_file(&temporary);
                let _ = sync_directory(parent);
                context.journal.targets[context.journal_index].phase = TargetPhase::RenameFailed;
                context.journal.targets[context.journal_index].temporary_path = None;
                context.journal.targets[context.journal_index].temporary_fingerprint = None;
                return Err(MutationFailure::Error(AppError::atomic_write(
                    &path.to_string_lossy(),
                    "fault_before_rename",
                )));
            }
            ApplyFaultDecision::Crash => {
                context.journal.targets[context.journal_index].phase = TargetPhase::CrashedBeforeRename;
                persist_journal(context.paths, context.journal)?;
                return Err(MutationFailure::Crash(AppError::atomic_write(
                    &path.to_string_lossy(),
                    "simulated_crash_before_rename",
                )));
            }
        }
        validate_allowed_path(path, allowed_root, false)?;
        if let Some(expected) = expected_current {
            if let Err(error) = verify_known_path_state(path, known_state, expected) {
                let _ = fs::remove_file(&temporary);
                let _ = sync_directory(parent);
                context.journal.targets[context.journal_index].phase = TargetPhase::RenameFailed;
                context.journal.targets[context.journal_index].temporary_path = None;
                context.journal.targets[context.journal_index].temporary_fingerprint = None;
                persist_journal(context.paths, context.journal)?;
                return Err(MutationFailure::Error(error));
            }
        }
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            let _ = sync_directory(parent);
            context.journal.targets[context.journal_index].phase = TargetPhase::RenameFailed;
            context.journal.targets[context.journal_index].temporary_path = None;
            context.journal.targets[context.journal_index].temporary_fingerprint = None;
            return Err(
                AppError::atomic_write(&path.to_string_lossy(), "rename_temporary")
                    .with_source(error)
                    .into(),
            );
        }
        context.journal.targets[context.journal_index].phase = TargetPhase::Renamed;
        context.journal.targets[context.journal_index].temporary_path = None;
        context.journal.targets[context.journal_index].temporary_fingerprint = None;
        sync_directory(parent)?;
        persist_journal(context.paths, context.journal)?;
        match context.fault.decide(&ApplyFaultEvent::AfterRename {
            index: context.index,
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
                context.journal.targets[context.journal_index].phase = TargetPhase::CrashedAfterRename;
                persist_journal(context.paths, context.journal)?;
                return Err(MutationFailure::Crash(AppError::atomic_write(
                    &path.to_string_lossy(),
                    "simulated_crash_after_rename",
                )));
            }
        }
    } else {
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            let _ = sync_directory(parent);
            return Err(
                AppError::atomic_write(&path.to_string_lossy(), "rename_temporary")
                    .with_source(error)
                    .into(),
            );
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
    allow_external_target: bool,
) -> Result<(), MutationFailure> {
    validate_allowed_path(path, allowed_root, false)?;
    let canonical_target = if allow_external_target {
        link_target.to_path_buf()
    } else {
        validate_central_link_target(path, link_target, central_root)?
    };
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
    let parent = parent_of(path)?;
    let temporary = parent.join(format!(".easytoagents-{}.link", Uuid::new_v4()));
    symlink(&canonical_target, &temporary).map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), "create_temporary_symlink")
            .with_source(error)
    })?;
    journal.targets[journal_index].phase = TargetPhase::RenamePending;
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
            journal.targets[journal_index].phase = TargetPhase::RenameFailed;
            journal.targets[journal_index].temporary_path = None;
            journal.targets[journal_index].temporary_fingerprint = None;
            return Err(MutationFailure::Error(AppError::atomic_write(
                &path.to_string_lossy(),
                "fault_before_symlink_rename",
            )));
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = TargetPhase::CrashedBeforeRename;
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
        journal.targets[journal_index].phase = TargetPhase::RenameFailed;
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        persist_journal(paths, journal)?;
        return Err(MutationFailure::Error(error));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        journal.targets[journal_index].phase = TargetPhase::RenameFailed;
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        return Err(
            AppError::atomic_write(&path.to_string_lossy(), "rename_symlink")
                .with_source(error)
                .into(),
        );
    }
    journal.targets[journal_index].phase = TargetPhase::Renamed;
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
            journal.targets[journal_index].phase = TargetPhase::CrashedAfterRename;
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Crash(AppError::atomic_write(
                &path.to_string_lossy(),
                "simulated_crash_after_symlink_rename",
            )));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn atomic_takeover_symlink(
    path: &Path,
    link_target: &Path,
    central_root: &Path,
    allowed_root: &Path,
    expected_current: ExpectedPathFingerprint<'_>,
    entry_type: SkillTakeoverEntryType,
    content_hash: &str,
    evidence_fingerprint: &str,
    index: usize,
    fault: &dyn ApplyFaultInjector,
    paths: &AppPaths,
    journal: &mut RunJournal,
    journal_index: usize,
) -> Result<(), MutationFailure> {
    validate_allowed_path(path, allowed_root, false)?;
    let canonical_target = validate_central_link_target(path, link_target, central_root)?;
    let current_state = verify_expected_path_state(path, expected_current)?;
    if !matches!(
        (&current_state, entry_type),
        (
            PathState::Symlink { .. },
            SkillTakeoverEntryType::ExternalSymlink
        ) | (
            PathState::Directory { .. },
            SkillTakeoverEntryType::Directory
        )
    ) {
        return Err(
            AppError::stale_preview(expected_current.run_id, expected_current.target_id).into(),
        );
    }
    verify_takeover_inspection(path, entry_type, content_hash, evidence_fingerprint)?;
    let parent = parent_of(path)?;
    let temporary = parent.join(format!(".easytoagents-{}.link", Uuid::new_v4()));
    let quarantine = parent.join(format!(".easytoagents-{}.takeover", Uuid::new_v4()));
    symlink(&canonical_target, &temporary).map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), "create_takeover_link").with_source(error)
    })?;
    journal.targets[journal_index].phase = TargetPhase::TakeoverRenamePending;
    journal.targets[journal_index].temporary_path = Some(temporary.to_string_lossy().into_owned());
    journal.targets[journal_index].temporary_fingerprint =
        Some(capture_path_state(&temporary)?.fingerprint());
    persist_journal(paths, journal)?;
    match fault.decide(&ApplyFaultEvent::BeforeRename {
        index,
        path: path.to_path_buf(),
    }) {
        ApplyFaultDecision::Continue => {}
        ApplyFaultDecision::Fail => {
            let _ = fs::remove_file(&temporary);
            journal.targets[journal_index].phase = TargetPhase::TakeoverRenameFailed;
            journal.targets[journal_index].temporary_path = None;
            journal.targets[journal_index].temporary_fingerprint = None;
            let _ = persist_journal(paths, journal);
            return Err(
                AppError::atomic_write(&path.to_string_lossy(), "fault_before_takeover").into(),
            );
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = TargetPhase::CrashedBeforeTakeover;
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Crash(AppError::atomic_write(
                &path.to_string_lossy(),
                "simulated_crash_before_takeover",
            )));
        }
    }
    validate_allowed_path(path, allowed_root, false)?;
    verify_expected_path_state(path, expected_current)?;
    verify_takeover_inspection(path, entry_type, content_hash, evidence_fingerprint)?;
    fs::rename(path, &quarantine).map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), "quarantine_takeover_entry")
            .with_source(error)
    })?;
    sync_directory(parent)?;
    journal.targets[journal_index].phase = TargetPhase::TakeoverQuarantined;
    journal.targets[journal_index].quarantine_path =
        Some(quarantine.to_string_lossy().into_owned());
    journal.targets[journal_index].quarantine_fingerprint =
        Some(capture_path_state(&quarantine)?.fingerprint());
    persist_journal(paths, journal)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::rename(&quarantine, path);
        let _ = fs::remove_file(&temporary);
        let _ = sync_directory(parent);
        journal.targets[journal_index].phase = TargetPhase::TakeoverLinkFailed;
        journal.targets[journal_index].temporary_path = None;
        journal.targets[journal_index].temporary_fingerprint = None;
        journal.targets[journal_index].quarantine_path = None;
        journal.targets[journal_index].quarantine_fingerprint = None;
        let _ = persist_journal(paths, journal);
        return Err(
            AppError::atomic_write(&path.to_string_lossy(), "install_takeover_link")
                .with_source(error)
                .into(),
        );
    }
    sync_directory(parent)?;
    journal.targets[journal_index].phase = TargetPhase::TakeoverLinked;
    journal.targets[journal_index].temporary_path = None;
    journal.targets[journal_index].temporary_fingerprint = None;
    persist_journal(paths, journal)?;
    match fault.decide(&ApplyFaultEvent::AfterRename {
        index,
        path: path.to_path_buf(),
    }) {
        ApplyFaultDecision::Continue => Ok(()),
        ApplyFaultDecision::Fail => {
            Err(AppError::atomic_write(&path.to_string_lossy(), "fault_after_takeover").into())
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = TargetPhase::CrashedAfterTakeover;
            persist_journal(paths, journal)?;
            Err(MutationFailure::Crash(AppError::atomic_write(
                &path.to_string_lossy(),
                "simulated_crash_after_takeover",
            )))
        }
    }
}

fn verify_takeover_inspection(
    path: &Path,
    entry_type: SkillTakeoverEntryType,
    content_hash: &str,
    evidence_fingerprint: &str,
) -> Result<(), AppError> {
    let inspection = skill_library::inspect_skill_takeover_entry(path)?;
    let actual_type = match inspection.entry_type {
        SkillTakeoverEntryKind::ExternalSymlink => SkillTakeoverEntryType::ExternalSymlink,
        SkillTakeoverEntryKind::Directory => SkillTakeoverEntryType::Directory,
    };
    if actual_type != entry_type
        || inspection.content_hash != content_hash
        || inspection.fingerprint != evidence_fingerprint
    {
        return Err(AppError::conflict(
            "skillTakeover",
            "Skill 接管入口在确认前发生变化",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn atomic_restore_directory_tree(
    path: &Path,
    snapshot_path: &Path,
    content_hash: &str,
    central_root: Option<&Path>,
    allowed_root: &Path,
    expected_current: ExpectedPathFingerprint<'_>,
    index: usize,
    fault: &dyn ApplyFaultInjector,
    paths: &AppPaths,
    journal: &mut RunJournal,
    journal_index: usize,
) -> Result<(), MutationFailure> {
    validate_allowed_path(path, allowed_root, false)?;
    skill_library::verify_skill_tree(snapshot_path, content_hash)?;
    match verify_expected_path_state(path, expected_current)? {
        PathState::Missing => {}
        PathState::Symlink { link_target } => {
            let central_root = central_root.ok_or_else(|| {
                AppError::conflict("restoreTarget", "没有中央库证据时拒绝替换链接")
            })?;
            validate_central_link_target(path, &link_target, central_root)?;
        }
        PathState::File { .. } | PathState::Directory { .. } => {
            return Err(
                AppError::conflict("restoreTarget", "目录树恢复拒绝覆盖普通文件或目录").into(),
            );
        }
    }
    let parent = parent_of(path)?;
    let temporary = parent.join(format!(".easytoagents-{}.restore.d", Uuid::new_v4()));
    let quarantine = parent.join(format!(".easytoagents-{}.takeover", Uuid::new_v4()));
    skill_library::copy_skill_tree(snapshot_path, &temporary, content_hash)?;
    journal.targets[journal_index].phase = TargetPhase::DirectoryRestorePending;
    journal.targets[journal_index].temporary_path = Some(temporary.to_string_lossy().into_owned());
    journal.targets[journal_index].temporary_fingerprint =
        Some(capture_path_state(&temporary)?.fingerprint());
    persist_journal(paths, journal)?;
    match fault.decide(&ApplyFaultEvent::BeforeRename {
        index,
        path: path.to_path_buf(),
    }) {
        ApplyFaultDecision::Continue => {}
        ApplyFaultDecision::Fail => {
            let _ = skill_library::remove_skill_tree(&temporary, parent, content_hash);
            return Err(AppError::atomic_write(
                &path.to_string_lossy(),
                "fault_before_restore_tree",
            )
            .into());
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = TargetPhase::CrashedBeforeRestoreTree;
            persist_journal(paths, journal)?;
            return Err(MutationFailure::Crash(AppError::atomic_write(
                &path.to_string_lossy(),
                "simulated_crash_before_restore_tree",
            )));
        }
    }
    validate_allowed_path(path, allowed_root, false)?;
    let current = verify_expected_path_state(path, expected_current)?;
    if let PathState::Symlink { link_target } = &current {
        let central_root = central_root
            .ok_or_else(|| AppError::conflict("restoreTarget", "没有中央库证据时拒绝替换链接"))?;
        validate_central_link_target(path, link_target, central_root)?;
        fs::rename(path, &quarantine).map_err(|error| {
            AppError::atomic_write(&path.to_string_lossy(), "quarantine_restore_link")
                .with_source(error)
        })?;
        journal.targets[journal_index].quarantine_path =
            Some(quarantine.to_string_lossy().into_owned());
        journal.targets[journal_index].quarantine_fingerprint =
            Some(capture_path_state(&quarantine)?.fingerprint());
        sync_directory(parent)?;
        persist_journal(paths, journal)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if !matches!(current, PathState::Missing) {
            let _ = fs::rename(&quarantine, path);
        }
        let _ = skill_library::remove_skill_tree(&temporary, parent, content_hash);
        let _ = sync_directory(parent);
        return Err(
            AppError::atomic_write(&path.to_string_lossy(), "install_restored_tree")
                .with_source(error)
                .into(),
        );
    }
    sync_directory(parent)?;
    journal.targets[journal_index].phase = TargetPhase::DirectoryRestored;
    journal.targets[journal_index].temporary_path = None;
    journal.targets[journal_index].temporary_fingerprint = None;
    persist_journal(paths, journal)?;
    match fault.decide(&ApplyFaultEvent::AfterRename {
        index,
        path: path.to_path_buf(),
    }) {
        ApplyFaultDecision::Continue => Ok(()),
        ApplyFaultDecision::Fail => {
            Err(AppError::atomic_write(&path.to_string_lossy(), "fault_after_restore_tree").into())
        }
        ApplyFaultDecision::Crash => {
            journal.targets[journal_index].phase = TargetPhase::CrashedAfterRestoreTree;
            persist_journal(paths, journal)?;
            Err(MutationFailure::Crash(AppError::atomic_write(
                &path.to_string_lossy(),
                "simulated_crash_after_restore_tree",
            )))
        }
    }
}

fn validate_central_link_target(
    link_path: &Path,
    link_target: &Path,
    central_root: &Path,
) -> Result<PathBuf, AppError> {
    let canonical_root = fs::canonicalize(central_root).map_err(|error| {
        AppError::not_found("centralSkillsRoot", &central_root.to_string_lossy()).with_source(error)
    })?;
    if canonical_root != central_root {
        return Err(AppError::conflict(
            "centralSkillsRoot",
            "中央 Skills 根不是 canonical 路径",
        ));
    }
    let resolved = if link_target.is_absolute() {
        link_target.to_path_buf()
    } else {
        parent_of(link_path)?.join(link_target)
    };
    let canonical = fs::canonicalize(&resolved).map_err(|error| {
        AppError::conflict("skillTarget", "断裂链接不能被证明为应用拥有").with_source(error)
    })?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|error| {
        AppError::not_found("centralSkill", &canonical.to_string_lossy()).with_source(error)
    })?;
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
    #[cfg(test)]
    FSYNC_CALLS.with(|count| count.set(count.get() + 1));
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            AppError::atomic_write(&path.to_string_lossy(), "sync_directory").with_source(error)
        })
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
        let adapter = target.input.descriptor.tool.adapter();
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
            projection: target.input.desired_projection.clone(),
        });
    }
    Ok(verifications)
}
