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
        directory_tree_hash,
        known_state,
    } = request;
    validate_allowed_path(target_path, allowed_root, false)?;
    let state = match known_state {
        Some(known) if cheap_state_matches(known, target_path) => known.clone(),
        _ => capture_path_state(target_path)?,
    };
    if state.fingerprint() != expected_before_fingerprint {
        let target = target_id
            .map(str::to_owned)
            .unwrap_or_else(|| target_path.to_string_lossy().into_owned());
        return Err(AppError::stale_preview(run_id, &target));
    }
    let snapshot_id = Uuid::new_v4().to_string();
    let run_directory = paths.snapshots().join(run_id);
    let run_directory_created = fs::symlink_metadata(&run_directory).is_err();
    ensure_private_directory(&run_directory)?;
    if run_directory_created {
        // run 目录本身也必须在快照根中 durable，不能只 fsync 其内部文件；
        // 同一 run 的后续快照复用已 durable 的目录，不再重复 fsync 快照根。
        sync_directory(paths.snapshots())?;
    }
    let storage_kind = match (&state, directory_tree_hash) {
        (PathState::File { .. }, _) => SnapshotStorageKind::PayloadFile,
        (PathState::Directory { .. }, Some(_)) => SnapshotStorageKind::DirectoryTree,
        _ => SnapshotStorageKind::MetadataOnly,
    };
    let snapshot_path = run_directory.join(match storage_kind {
        SnapshotStorageKind::DirectoryTree => format!("{snapshot_id}.snapshot.d"),
        SnapshotStorageKind::PayloadFile | SnapshotStorageKind::MetadataOnly => {
            format!("{snapshot_id}.snapshot")
        }
    });
    match storage_kind {
        SnapshotStorageKind::DirectoryTree => {
            let expected_hash =
                directory_tree_hash.ok_or_else(|| AppError::internal("目录树快照必须绑定 hash"))?;
            skill_library::copy_skill_tree(target_path, &snapshot_path, expected_hash)?;
        }
        SnapshotStorageKind::PayloadFile | SnapshotStorageKind::MetadataOnly => {
            let mut snapshot_file = create_private_file(&snapshot_path)?;
            if let PathState::File { bytes, .. } = &state {
                snapshot_file.write_all(bytes).map_err(|error| {
                    AppError::atomic_write(&snapshot_path.to_string_lossy(), "write_snapshot")
                        .with_source(error)
                })?;
            }
            snapshot_file.flush().map_err(|error| {
                AppError::atomic_write(&snapshot_path.to_string_lossy(), "flush_snapshot")
                    .with_source(error)
            })?;
            fsync_file(&snapshot_file, &snapshot_path, "sync_snapshot")?;
            ensure_private_file(&snapshot_path)?;
        }
    }
    sync_directory(&run_directory)?;

    let database_path = database.path().to_string_lossy().into_owned();
    if let Err(error) = crate::db::sync::insert_snapshot(
        database.connection(),
        &database_path,
        crate::db::sync::SnapshotInsert {
            id: &snapshot_id,
            run_id,
            target_id,
            target_path,
            snapshot_path: &snapshot_path,
            content_hash: directory_tree_hash.or_else(|| state.content_hash()),
            file_mode: state.mode(),
            target_type: state.target_type().as_str(),
            link_target: state.link_target(),
            storage_kind: storage_kind.as_str(),
        },
    ) {
        match storage_kind {
            SnapshotStorageKind::DirectoryTree => {
                if let Some(hash) = directory_tree_hash {
                    let _ = skill_library::remove_skill_tree(&snapshot_path, &run_directory, hash);
                }
            }
            SnapshotStorageKind::PayloadFile | SnapshotStorageKind::MetadataOnly => {
                let _ = fs::remove_file(&snapshot_path);
            }
        }
        return Err(error);
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
        storage_kind,
        directory_tree_hash: directory_tree_hash.map(str::to_owned),
    })
}
