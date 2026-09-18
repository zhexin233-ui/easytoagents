/// 从 / 逐段 openat；只允许入口自身显式链接，不跟随任意祖先链接。
fn open_directory_chain(path: &Path) -> Result<(File, Vec<DirectoryIdentity>), AppError> {
    if !path.is_absolute() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "来源路径必须为绝对路径",
        ));
    }
    let mut directory = open_directory_nofollow(Path::new("/"), "open_skill_ancestor")?;
    let mut current = PathBuf::from("/");
    let mut identities = Vec::new();
    for component in path.components() {
        let segment = match component {
            Component::RootDir => continue,
            Component::Normal(segment) => segment,
            _ => {
                return Err(AppError::invalid_input(
                    "sourcePath",
                    "来源路径含不安全片段",
                ))
            }
        };
        current.push(segment);
        let before = lstat_at(&directory, segment, &current)?;
        if !before.is_dir() || before.is_symlink() {
            return Err(AppError::invalid_input(
                "sourcePath",
                "来源祖先必须是真实目录",
            ));
        }
        directory = open_directory_at_nofollow(&directory, segment, &current)?;
        let metadata = directory
            .metadata()
            .map_err(|error| map_read_error(error, &current, "stat_skill_ancestor"))?;
        // 祖先的其它子目录可能被并发创建；这里只绑定祖先身份与权限，
        // 不把无关兄弟的目录大小或链接计数当作本技能漂移。
        if before.identity.device != metadata.dev()
            || before.identity.inode != metadata.ino()
            || before.identity.mode != metadata.mode()
        {
            return Err(AppError::conflict("sourcePath", "来源祖先身份在读取时变化"));
        }
        identities.push(DirectoryIdentity {
            path: current.clone(),
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
        });
    }
    Ok((directory, identities))
}

pub(super) fn enumerate_skill_entries(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let (directory, _) = open_directory_chain(root)?;
    let mut names = read_directory_names(&directory, root)?;
    names.sort();
    let mut result = Vec::new();
    for name in names {
        let path = root.join(&name);
        let metadata = lstat_at(&directory, &name, &path)?;
        if metadata.is_dir() || metadata.is_symlink() {
            path_text(&path, "sourcePath")?;
            result.push(path);
        }
    }
    Ok(result)
}

pub(super) fn resolve_skill_source(
    root: &Path,
    entry: &Path,
) -> Result<SkillSourceEvidence, AppError> {
    resolve_skill_source_excluding(root, entry, &[])
}

pub(super) fn resolve_skill_source_excluding(
    root: &Path,
    entry: &Path,
    excluded: &[PathBuf],
) -> Result<SkillSourceEvidence, AppError> {
    if entry.parent() != Some(root) {
        return Err(AppError::invalid_input(
            "sourcePath",
            "只允许显式来源中的直属入口",
        ));
    }
    let (_, mut directories) = open_directory_chain(root)?;
    let mut current = entry.to_path_buf();
    let mut links = Vec::new();
    let mut visited = BTreeSet::new();
    loop {
        if excluded.iter().any(|path| current.starts_with(path)) {
            return Err(AppError::invalid_input(
                "builtin",
                "内置技能不在本次导入范围",
            ));
        }
        if !visited.insert(current.clone()) || links.len() > 32 {
            return Err(AppError::invalid_input(
                "sourcePath",
                "来源链接循环或超过 32 跳限制",
            ));
        }
        let parent_path = current
            .parent()
            .ok_or_else(|| AppError::invalid_input("sourcePath", "来源链接目标过于宽泛"))?;
        let name = current
            .file_name()
            .ok_or_else(|| AppError::invalid_input("sourcePath", "来源链接缺少目录名"))?;
        let (parent, ancestors) = open_directory_chain(parent_path)?;
        directories.extend(ancestors);
        let before = lstat_at(&parent, name, &current)?;
        if before.is_symlink() {
            let target = read_link_at(&parent, name, &current)?;
            if lstat_at(&parent, name, &current)?.identity != before.identity {
                return Err(AppError::conflict("sourcePath", "来源链接在读取时变化"));
            }
            links.push(SourceLink {
                path: current.clone(),
                target: target.clone(),
                identity: before.identity,
            });
            let joined = if target.is_absolute() {
                target
            } else {
                parent_path.join(target)
            };
            let mut normalized = PathBuf::from("/");
            for component in joined.components() {
                match component {
                    Component::RootDir | Component::CurDir => {}
                    Component::Normal(segment) => normalized.push(segment),
                    Component::ParentDir => {
                        // 不能把缺失目录或链接祖先前的 .. 静默折叠成另一个来源。
                        let (_, ancestors) = open_directory_chain(&normalized)?;
                        directories.extend(ancestors);
                        if !normalized.pop() {
                            return Err(AppError::invalid_input(
                                "sourcePath",
                                "来源链接越过文件系统根",
                            ));
                        }
                    }
                    _ => return Err(AppError::invalid_input("sourcePath", "来源链接路径无效")),
                }
            }
            current = normalized;
            continue;
        }
        if !before.is_dir() || current.components().count() < 3 {
            return Err(AppError::invalid_input(
                "sourcePath",
                "来源必须是非宽泛的技能目录",
            ));
        }
        let directory = open_directory_at_nofollow(&parent, name, &current)?;
        ensure_same_identity(
            before.identity,
            &directory
                .metadata()
                .map_err(|error| map_read_error(error, &current, "stat_resolved_skill"))?,
            &current,
        )?;
        return Ok(SkillSourceEvidence {
            root: root.to_path_buf(),
            entry: entry.to_path_buf(),
            resolved: current,
            directories,
            links,
            identity: before.identity,
        });
    }
}

pub(super) fn verify_skill_source(evidence: &SkillSourceEvidence) -> Result<(), AppError> {
    if resolve_skill_source(&evidence.root, &evidence.entry)? != *evidence {
        return Err(AppError::conflict(
            "sourcePath",
            "Skill 来源入口或目录身份已经变化，请重新检测",
        ));
    }
    Ok(())
}

pub(super) fn inspect_skill_source(
    evidence: &SkillSourceEvidence,
    budget: &Cell<u64>,
) -> Result<SourceSkillInspection, AppError> {
    verify_skill_source(evidence)?;
    let (directory, _) = open_directory_chain(&evidence.resolved)?;
    let metadata = lstat_at(
        &directory,
        OsStr::new("SKILL.md"),
        &evidence.resolved.join("SKILL.md"),
    )?;
    if !metadata.is_file()
        || metadata.is_symlink()
        || metadata.nlink() != 1
        || metadata.identity.size > MAX_SKILL_MD_BYTES
    {
        return Err(AppError::invalid_input(
            "SKILL.md",
            "Skill 必须包含有界普通 SKILL.md 文件",
        ));
    }
    let digest = digest_tree_budgeted(
        &evidence.resolved,
        None,
        Some(evidence.identity),
        Some(budget),
    )?;
    let text = digest
        .skill_md
        .ok_or_else(|| AppError::invalid_input("SKILL.md", "Skill 必须包含普通 SKILL.md 文件"))?;
    let (name, frontmatter) = parse_skill_frontmatter(&text)?;
    verify_skill_source(evidence)?;
    Ok(SourceSkillInspection {
        name,
        description: frontmatter
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        hash: digest.hash,
    })
}

/// 复核正式 Skills 根的直属入口，并返回接管所需的入口身份与完整树 hash。
pub(crate) fn inspect_skill_takeover_entry(
    entry: &Path,
) -> Result<SkillTakeoverInspection, AppError> {
    let root = entry
        .parent()
        .ok_or_else(|| AppError::invalid_input("sourcePath", "Skill 入口缺少父目录"))?;
    let evidence = resolve_skill_source(root, entry)?;
    let budget = Cell::new(MAX_TOTAL_BYTES.saturating_add(MAX_FILES as u64));
    let inspection = inspect_skill_source(&evidence, &budget)?;
    let entry_metadata = fs::symlink_metadata(entry)
        .map_err(|error| map_read_error(error, entry, "stat_takeover_entry"))?;
    let identity = FileIdentity::from_metadata(&entry_metadata);
    let (entry_type, fingerprint) = if entry_metadata.file_type().is_symlink() {
        let link_target = fs::read_link(entry)
            .map_err(|error| map_read_error(error, entry, "read_takeover_link"))?;
        (
            SkillTakeoverEntryKind::ExternalSymlink,
            hash_json(&serde_json::json!({
                "type": "external_symlink",
                "linkTarget": link_target,
                "device": identity.device,
                "inode": identity.inode,
                "mode": identity.mode,
            })),
        )
    } else if entry_metadata.is_dir() {
        (
            SkillTakeoverEntryKind::Directory,
            hash_json(&serde_json::json!({
                "type": "directory",
                "device": identity.device,
                "inode": identity.inode,
                "mode": identity.mode,
                "hash": inspection.hash,
            })),
        )
    } else {
        return Err(AppError::invalid_input(
            "sourcePath",
            "接管入口必须是目录或目录符号链接",
        ));
    };
    Ok(SkillTakeoverInspection {
        name: inspection.name,
        entry_type,
        fingerprint,
        content_hash: inspection.hash,
        resolved: evidence.resolved,
    })
}

/// 安全复制一个已经通过 Skill 树合同的目录，并复核源/副本完整 hash。
pub(crate) fn copy_skill_tree(
    source: &Path,
    destination: &Path,
    expected_hash: &str,
) -> Result<(), AppError> {
    let owner = destination
        .parent()
        .ok_or_else(|| AppError::invalid_input("snapshotPath", "目录快照缺少父目录"))?;
    if fs::symlink_metadata(destination).is_ok() {
        return Err(AppError::conflict("snapshotPath", "目录快照目标已经存在"));
    }
    create_private_directory(destination)?;
    let copied = (|| {
        let source_digest = digest_tree(source, Some(destination))?;
        sync_directory(destination)?;
        let source_after = digest_tree(source, None)?;
        let destination_after = digest_tree(destination, None)?;
        if source_digest.hash != expected_hash
            || source_after.hash != expected_hash
            || destination_after.hash != expected_hash
        {
            return Err(AppError::conflict(
                "skillTree",
                "Skill 目录树在复制过程中发生变化",
            ));
        }
        Ok(())
    })();
    if copied.is_err() {
        remove_owned_directory(destination, owner)?;
    }
    copied
}

pub(crate) fn verify_skill_tree(path: &Path, expected_hash: &str) -> Result<(), AppError> {
    let digest = digest_tree(path, None)?;
    if digest.hash != expected_hash {
        return Err(AppError::conflict("skillTree", "Skill 目录树 hash 已变化"));
    }
    Ok(())
}

pub(crate) fn remove_skill_tree(
    path: &Path,
    owner: &Path,
    expected_hash: &str,
) -> Result<(), AppError> {
    verify_skill_tree(path, expected_hash)?;
    remove_owned_directory(path, owner)
}

/// 原生 Skill 入口被替换成中央链接期间的可回滚证据。
///
/// `quarantine_path` 只允许位于应用私有 staging 的直属层；目录接管时它是
/// 已从目标移出的完整目录，外部 symlink 接管时它只是一个链接本身，绝不会
/// 跟随链接删除真实的外部目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeSkillEntrySwap {
    pub entry_path: PathBuf,
    pub quarantine_path: PathBuf,
    pub central_path: PathBuf,
    pub entry_type: SkillTakeoverEntryKind,
    pub fingerprint: String,
    pub content_hash: String,
}

/// 在无覆盖窗口的前提下，把已验证的原生目录/外部链接入口换成中央链接。
///
/// 调用方必须先用同一份 ExternalChangePlan 证明入口 identity/hash，并在此
/// 之前把中央新副本放入正式路径；这里仍会再次 inspect，拒绝入口竞态、中央
/// 路径类型错误和临时目录占用。
pub(crate) fn replace_native_skill_entry_with_central_link(
    paths: &AppPaths,
    entry: &Path,
    central: &Path,
    expected_entry_type: SkillTakeoverEntryKind,
    expected_fingerprint: &str,
    expected_hash: &str,
    expected_name: &str,
) -> Result<NativeSkillEntrySwap, AppError> {
    validate_native_entry_paths(paths, entry, central)?;
    let before = inspect_skill_takeover_entry(entry)?;
    ensure_takeover_evidence(
        &before,
        expected_entry_type,
        expected_fingerprint,
        expected_hash,
    )?;
    verify_central_link_target(central)?;

    let parent = entry
        .parent()
        .ok_or_else(|| AppError::invalid_input("targetPath", "Skill 入口缺少父目录"))?;
    let suffix = Uuid::new_v4().to_string();
    let temporary_link = parent.join(format!(".easytoagents-skill-link-{suffix}"));
    let quarantine = paths
        .staging()
        .join(format!("skill-native-adopt-{suffix}"));
    if fs::symlink_metadata(&temporary_link).is_ok()
        || fs::symlink_metadata(&quarantine).is_ok()
    {
        return Err(AppError::conflict(
            "staging",
            "Skill 原生采纳临时路径已经存在",
        ));
    }

    symlink(central, &temporary_link).map_err(|error| {
        AppError::atomic_write(&temporary_link.to_string_lossy(), "stage_native_skill_link")
            .with_source(error)
    })?;
    let result = (|| {
        let current = inspect_skill_takeover_entry(entry)?;
        ensure_takeover_evidence(
            &current,
            expected_entry_type,
            expected_fingerprint,
            expected_hash,
        )?;
        fs::rename(entry, &quarantine).map_err(|error| {
            AppError::atomic_write(&entry.to_string_lossy(), "quarantine_native_skill_entry")
                .with_source(error)
        })?;
        sync_directory(parent)?;
        sync_directory(paths.staging())?;
        rename_import_exclusively(&temporary_link, entry)?;
        sync_directory(parent)?;
        let installed = fs::symlink_metadata(entry).map_err(|error| {
            AppError::atomic_write(&entry.to_string_lossy(), "verify_native_skill_link")
                .with_source(error)
        })?;
        if !installed.file_type().is_symlink() || fs::read_link(entry).ok().as_deref() != Some(central)
        {
            return Err(AppError::conflict(
                "targetPath",
                "Skill 原生入口未安装到预期中央链接",
            ));
        }
        // 中央路径在预检与入口替换之间也可能被外部进程改成符号链接；
        // 安装后重新核验，避免把私有 central 名称当成外部逃逸跳板。
        verify_central_link_target(central)?;
        let linked_skill = inspect_skill_takeover_entry(entry)?;
        if linked_skill.content_hash != expected_hash || linked_skill.name != expected_name {
            return Err(AppError::conflict(
                "centralSkill",
                "Skill 中央链接目标在安装后发生变化",
            ));
        }
        Ok(NativeSkillEntrySwap {
            entry_path: entry.to_path_buf(),
            quarantine_path: quarantine.clone(),
            central_path: central.to_path_buf(),
            entry_type: expected_entry_type,
            fingerprint: expected_fingerprint.to_owned(),
            content_hash: expected_hash.to_owned(),
        })
    })();
    let _ = fs::remove_file(&temporary_link);
    match result {
        Ok(swap) => Ok(swap),
        Err(error) => {
            // 失败时尽量把入口与原内容恢复；如果恢复也失败，保留 staging
            // 证据并升级为 rollback_failed，禁止静默丢失用户目录。
            let quarantine_exists = fs::symlink_metadata(&quarantine).is_ok();
            let entry_exists = fs::symlink_metadata(entry).is_ok();
            let restored = if !quarantine_exists {
                // 失败发生在移出原入口之前；只要入口仍在，就没有需要恢复的
                // staging 证据。入口若也消失则保守地报告回滚失败。
                entry_exists
            } else if !entry_exists {
                fs::rename(&quarantine, entry).is_ok() && sync_directory(parent).is_ok()
            } else {
                // rename 已完成且后续校验失败：若入口仍是我们刚装的中央
                // 链接，可以先移除再恢复；未知占用则必须保留两边证据。
                match fs::read_link(entry) {
                    Ok(target) if target == central => {
                        fs::remove_file(entry).is_ok()
                            && fs::rename(&quarantine, entry).is_ok()
                            && sync_directory(parent).is_ok()
                    }
                    _ => false,
                }
            };
            if restored {
                Err(error)
            } else {
                Err(AppError::rollback_failed(
                    "skill-native-adopt",
                    &entry.to_string_lossy(),
                    &quarantine.to_string_lossy(),
                ))
            }
        }
    }
}

/// 回滚一个已经成功替换的原生 Skill 入口；所有 identity/hash 都会在删除
/// 中央链接前重新校验，避免覆盖用户在失败期间新放入的条目。
pub(crate) fn rollback_native_skill_entry_swap(
    paths: &AppPaths,
    swap: &NativeSkillEntrySwap,
) -> Result<(), AppError> {
    validate_native_entry_paths(paths, &swap.entry_path, &swap.central_path)?;
    let quarantined = inspect_skill_takeover_entry(&swap.quarantine_path)?;
    ensure_takeover_evidence(
        &quarantined,
        swap.entry_type,
        &swap.fingerprint,
        &swap.content_hash,
    )?;
    let installed = fs::symlink_metadata(&swap.entry_path).map_err(|error| {
        map_read_error(error, &swap.entry_path, "stat_native_skill_link_for_rollback")
    })?;
    if !installed.file_type().is_symlink()
        || fs::read_link(&swap.entry_path).ok().as_deref() != Some(swap.central_path.as_path())
    {
        return Err(AppError::conflict(
            "targetPath",
            "Skill 原生入口已被其他内容占用，拒绝回滚覆盖",
        ));
    }
    fs::remove_file(&swap.entry_path).map_err(|error| {
        AppError::atomic_write(
            &swap.entry_path.to_string_lossy(),
            "remove_native_skill_link_for_rollback",
        )
        .with_source(error)
    })?;
    fs::rename(&swap.quarantine_path, &swap.entry_path).map_err(|error| {
        AppError::atomic_write(
            &swap.entry_path.to_string_lossy(),
            "restore_native_skill_entry",
        )
        .with_source(error)
    })?;
    let parent = swap
        .entry_path
        .parent()
        .ok_or_else(|| AppError::invalid_input("targetPath", "Skill 入口缺少父目录"))?;
    sync_directory(parent)?;
    sync_directory(paths.staging())
}

/// 成功提交中央/target DB 后清理被隔离的原生入口。目录与外部 symlink 使用
/// 不同删除合同，后者永远只 unlink 入口，不会碰链接真实目标。
pub(crate) fn cleanup_native_skill_entry_swap(
    paths: &AppPaths,
    swap: &NativeSkillEntrySwap,
) -> Result<(), AppError> {
    validate_native_entry_paths(paths, &swap.entry_path, &swap.central_path)?;
    let quarantined = inspect_skill_takeover_entry(&swap.quarantine_path)?;
    ensure_takeover_evidence(
        &quarantined,
        swap.entry_type,
        &swap.fingerprint,
        &swap.content_hash,
    )?;
    match swap.entry_type {
        SkillTakeoverEntryKind::Directory => {
            remove_skill_tree(&swap.quarantine_path, paths.staging(), &swap.content_hash)?;
        }
        SkillTakeoverEntryKind::ExternalSymlink => {
            fs::remove_file(&swap.quarantine_path).map_err(|error| {
                AppError::atomic_write(
                    &swap.quarantine_path.to_string_lossy(),
                    "remove_quarantined_native_skill_link",
                )
                .with_source(error)
            })?;
            sync_directory(paths.staging())?;
        }
    }
    Ok(())
}

/// 从 staging 中已完整复制且 hash 复核的 Skill 读取 frontmatter。读取前后
/// 再次 inspect，保证 DB 即将写入的名称/hash 不来自竞态中的半棵目录树。
pub(crate) fn read_skill_frontmatter_for_adoption(
    path: &Path,
    expected_hash: &str,
) -> Result<(String, Value), AppError> {
    let before = inspect_skill_takeover_entry(path)?;
    if before.content_hash != expected_hash {
        return Err(AppError::conflict(
            "skillTree",
            "待采纳 Skill staging hash 与观察值不一致",
        ));
    }
    let text = read_regular_utf8(
        &before.resolved.join("SKILL.md"),
        MAX_SKILL_MD_BYTES,
        "SKILL.md",
    )?;
    let parsed = parse_skill_frontmatter(&text)?;
    let after = inspect_skill_takeover_entry(path)?;
    if after != before {
        return Err(AppError::conflict(
            "skillTree",
            "Skill staging 在读取 frontmatter 时发生变化",
        ));
    }
    Ok(parsed)
}

fn ensure_takeover_evidence(
    actual: &SkillTakeoverInspection,
    expected_entry_type: SkillTakeoverEntryKind,
    expected_fingerprint: &str,
    expected_hash: &str,
) -> Result<(), AppError> {
    if actual.entry_type != expected_entry_type
        || actual.fingerprint != expected_fingerprint
        || actual.content_hash != expected_hash
    {
        return Err(AppError::stale_preview(
            "adoptSkillNative",
            "skillEntry",
        ));
    }
    Ok(())
}

fn validate_native_entry_paths(
    paths: &AppPaths,
    entry: &Path,
    central: &Path,
) -> Result<(), AppError> {
    if !entry.is_absolute()
        || entry.parent().is_none()
        || entry.file_name().is_none()
        || !central.is_absolute()
        || central.parent() != Some(paths.central_skills())
        || entry == paths.data_root()
        || entry == paths.staging()
    {
        return Err(AppError::conflict(
            "targetPath",
            "Skill 原生入口或中央路径不是安全直属路径",
        ));
    }
    let parent = entry.parent().expect("checked above");
    if parent == entry || parent == paths.data_root() || parent == paths.staging() {
        return Err(AppError::conflict(
            "targetPath",
            "Skill 原生入口父目录不是外部目标目录",
        ));
    }
    Ok(())
}

fn verify_central_link_target(central: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(central).map_err(|error| {
        map_read_error(error, central, "stat_native_skill_central")
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::conflict(
            "centralSkill",
            "Skill 中央链接目标必须是普通目录",
        ));
    }
    let canonical = fs::canonicalize(central).map_err(|error| {
        map_read_error(error, central, "canonicalize_native_skill_central")
    })?;
    if canonical != central {
        return Err(AppError::conflict(
            "centralSkill",
            "Skill 中央链接目标路径发生变化",
        ));
    }
    Ok(())
}
