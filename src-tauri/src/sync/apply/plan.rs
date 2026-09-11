fn build_target_work<'a>(
    preview: &'a PersistedPreview,
    inputs: &'a [ApplyTargetInput],
) -> Result<Vec<TargetWork<'a>>, AppError> {
    let inputs = inputs_by_target_path(preview, inputs)?;
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
                .map_err(|error| {
                    AppError::invalid_input("targetPath", "项目目标不在登记项目根内")
                        .with_source(error)
                })?;
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
                .ok_or_else(|| AppError::internal("exclude 的受管目标必须存在"))?;
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
                    stat: StatSignature::default(),
                }
                .fingerprint(),
                before_state: None,
                mutation: Mutation::WriteFile {
                    bytes: rendered,
                    mode,
                },
            });
        }
    }
    Ok(work)
}

fn build_native_resource_mutations(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    evidence: &super::ProjectNativeResourceEvidence,
    target_index: usize,
) -> Result<Vec<PendingMutation>, AppError> {
    use super::NativeResourceEntryType;
    validate_preview_hashes(item, input)?;
    match (evidence.entry_type, evidence.action) {
        (NativeResourceEntryType::McpEntry, _) => {
            build_file_native_mutations(item, input, evidence, target_index)
        }
        (NativeResourceEntryType::Directory | NativeResourceEntryType::Symlink, _) => {
            build_skill_native_mutations(item, input, evidence, target_index)
        }
    }
}

fn build_file_native_mutations(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    evidence: &super::ProjectNativeResourceEvidence,
    target_index: usize,
) -> Result<Vec<PendingMutation>, AppError> {
    let path = PathBuf::from(&item.target_path);
    if input.delete_target {
        let expected_before_fingerprint = capture_path_state(&path)?.fingerprint();
        return Ok(vec![PendingMutation {
            target_id: item.target_id.clone(),
            target_index,
            path,
            allowed_root: input.allowed_root.clone(),
            central_root: None,
            expected_before_fingerprint,
            expected_after_fingerprint: PathState::Missing.fingerprint(),
            before_state: None,
            mutation: Mutation::Remove,
        }]);
    }
    let adapter = input.descriptor.tool.adapter();
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
    let mode = evidence.restore_file_mode.unwrap_or(match &current_state {
        PathState::File { mode, .. } => *mode,
        PathState::Missing => PRIVATE_FILE_MODE,
        _ => {
            return Err(AppError::conflict(
                "targetPath",
                "文件目标被未知目录或链接占用",
            ));
        }
    });
    Ok(vec![PendingMutation {
        target_id: item.target_id.clone(),
        target_index,
        path,
        allowed_root: input.allowed_root.clone(),
        central_root: None,
        expected_before_fingerprint: current_state.fingerprint(),
        expected_after_fingerprint: PathState::File {
            hash: hash_bytes(&bytes),
            bytes: bytes.clone(),
            mode,
            stat: StatSignature::default(),
        }
        .fingerprint(),
        before_state: Some(current_state),
        mutation: Mutation::WriteFile { bytes, mode },
    }])
}

fn build_skill_native_mutations(
    item: &PersistedPreviewItem,
    input: &ApplyTargetInput,
    evidence: &super::ProjectNativeResourceEvidence,
    target_index: usize,
) -> Result<Vec<PendingMutation>, AppError> {
    use super::{NativeResourceActionKind, NativeResourceEntryType};
    validate_child_name(&evidence.external_key)?;
    let directory = Path::new(&item.target_path);
    let child = directory.join(&evidence.external_key);
    let mut mutations = if evidence.action == NativeResourceActionKind::Restore {
        build_missing_project_directories(
            directory,
            &input.allowed_root,
            &item.target_id,
            target_index,
        )?
    } else {
        Vec::new()
    };
    match evidence.action {
        NativeResourceActionKind::Disable => {
            let current = capture_path_state(&child)?;
            match evidence.entry_type {
                NativeResourceEntryType::Directory => {
                    if !matches!(current, PathState::Directory { .. }) {
                        return Err(AppError::stale_preview("persisted", &item.target_id));
                    }
                    let content_hash = evidence.content_hash.clone().ok_or_else(|| {
                        AppError::invalid_input("projectNativeAction", "目录禁用缺少树 hash")
                    })?;
                    mutations.push(PendingMutation {
                        target_id: item.target_id.clone(),
                        target_index,
                        path: child,
                        allowed_root: input.allowed_root.clone(),
                        central_root: None,
                        expected_before_fingerprint: current.fingerprint(),
                        expected_after_fingerprint: PathState::Missing.fingerprint(),
                        before_state: None,
                        mutation: Mutation::RemoveNativeSkill {
                            entry_type: NativeResourceEntryType::Directory,
                            content_hash: Some(content_hash),
                        },
                    });
                }
                NativeResourceEntryType::Symlink => {
                    if !matches!(current, PathState::Symlink { .. }) {
                        return Err(AppError::stale_preview("persisted", &item.target_id));
                    }
                    mutations.push(PendingMutation {
                        target_id: item.target_id.clone(),
                        target_index,
                        path: child,
                        allowed_root: input.allowed_root.clone(),
                        central_root: None,
                        expected_before_fingerprint: current.fingerprint(),
                        expected_after_fingerprint: PathState::Missing.fingerprint(),
                        before_state: None,
                        mutation: Mutation::RemoveNativeSkill {
                            entry_type: NativeResourceEntryType::Symlink,
                            content_hash: None,
                        },
                    });
                }
                _ => {
                    return Err(AppError::invalid_input(
                        "projectNativeAction",
                        "Skill 入口类型无效",
                    ));
                }
            }
        }
        NativeResourceActionKind::Restore => {
            let current = capture_path_state(&child)?;
            if !matches!(current, PathState::Missing) {
                return Err(AppError::conflict(
                    "targetPath",
                    "恢复目标已被占用，拒绝覆盖",
                ));
            }
            match evidence.entry_type {
                NativeResourceEntryType::Directory => {
                    let snapshot_path =
                        evidence.restore_snapshot_path.as_deref().ok_or_else(|| {
                            AppError::invalid_input("projectNativeAction", "目录恢复缺少快照路径")
                        })?;
                    let content_hash = evidence.content_hash.clone().ok_or_else(|| {
                        AppError::invalid_input("projectNativeAction", "目录恢复缺少树 hash")
                    })?;
                    mutations.push(PendingMutation {
                        target_id: item.target_id.clone(),
                        target_index,
                        path: child,
                        allowed_root: input.allowed_root.clone(),
                        central_root: None,
                        expected_before_fingerprint: current.fingerprint(),
                        expected_after_fingerprint: String::new(),
                        before_state: None,
                        mutation: Mutation::RestoreDirectoryTree {
                            snapshot_path: PathBuf::from(snapshot_path),
                            content_hash,
                        },
                    });
                }
                NativeResourceEntryType::Symlink => {
                    let link_target = evidence.restore_link_target.as_deref().ok_or_else(|| {
                        AppError::invalid_input("projectNativeAction", "符号链接恢复缺少目标")
                    })?;
                    mutations.push(PendingMutation {
                        target_id: item.target_id.clone(),
                        target_index,
                        path: child,
                        allowed_root: input.allowed_root.clone(),
                        central_root: None,
                        expected_before_fingerprint: current.fingerprint(),
                        expected_after_fingerprint: PathState::Symlink {
                            link_target: PathBuf::from(link_target),
                        }
                        .fingerprint(),
                        before_state: None,
                        mutation: Mutation::RestoreNativeSymlink {
                            link_target: PathBuf::from(link_target),
                        },
                    });
                }
                _ => {
                    return Err(AppError::invalid_input(
                        "projectNativeAction",
                        "Skill 入口类型无效",
                    ));
                }
            }
        }
    }
    Ok(mutations)
}

fn build_missing_project_directories(
    directory: &Path,
    allowed_root: &Path,
    target_id: &str,
    target_index: usize,
) -> Result<Vec<PendingMutation>, AppError> {
    build_missing_skill_directories(
        directory,
        allowed_root,
        allowed_root,
        target_id,
        target_index,
    )
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
    if let Some(evidence) = input.project_native_action.as_ref() {
        return build_native_resource_mutations(item, input, evidence, target_index);
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
            before_state: None,
            mutation: Mutation::Remove,
        }]);
    }
    if input.descriptor.format == TargetFormat::SymlinkDirectory {
        let mutations = build_symlink_mutations(item, input, target_index)?;
        validate_preview_hashes(item, input)?;
        return Ok(mutations);
    }
    let adapter = input.descriptor.tool.adapter();
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
            stat: StatSignature::default(),
        }
        .fingerprint(),
        before_state: Some(current_state),
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
    let takeover_entries = input
        .skill_takeover_entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
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
                if let Some(takeover) = takeover_entries.get(name.as_str()) {
                    let current_inspection = skill_library::inspect_skill_takeover_entry(&child)
                        .map_err(|error| {
                            AppError::stale_preview("persisted", &item.target_id).with_source(error)
                        })?;
                    let current_type = match current_inspection.entry_type {
                        SkillTakeoverEntryKind::ExternalSymlink => {
                            SkillTakeoverEntryType::ExternalSymlink
                        }
                        SkillTakeoverEntryKind::Directory => SkillTakeoverEntryType::Directory,
                    };
                    if current_type != takeover.entry_type
                        || current_inspection.fingerprint != takeover.expected_fingerprint
                        || current_inspection.content_hash != takeover.content_hash
                        || takeover.central_path != link_target
                    {
                        return Err(AppError::stale_preview("persisted", &item.target_id));
                    }
                    let current = capture_path_state(&child)?;
                    if !matches!(
                        (&current, takeover.entry_type),
                        (
                            PathState::Symlink { .. },
                            SkillTakeoverEntryType::ExternalSymlink
                        ) | (
                            PathState::Directory { .. },
                            SkillTakeoverEntryType::Directory
                        )
                    ) {
                        return Err(AppError::stale_preview("persisted", &item.target_id));
                    }
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
                        before_state: None,
                        mutation: Mutation::TakeoverSymlink {
                            link_target: canonical_link_target,
                            central_root: central_root.clone(),
                            entry_type: takeover.entry_type,
                            content_hash: takeover.content_hash.clone(),
                            evidence_fingerprint: takeover.expected_fingerprint.clone(),
                        },
                    });
                    continue;
                }
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
                    before_state: None,
                    mutation: Mutation::ReplaceSymlink {
                        link_target: canonical_link_target,
                        central_root: central_root.clone(),
                        allow_external_target: false,
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
                        before_state: None,
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
    let relative = directory.strip_prefix(allowed_root).map_err(|error| {
        AppError::invalid_input("targetPath", "Skills 目录位于允许根之外").with_source(error)
    })?;
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
                before_state: None,
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

