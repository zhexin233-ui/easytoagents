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
