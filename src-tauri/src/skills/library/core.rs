#[derive(Debug)]
struct TreeDigest {
    hash: String,
    files: Vec<String>,
    skill_md: Option<String>,
}

#[derive(Default)]
struct WalkLimits<'a> {
    entries: usize,
    total_bytes: u64,
    skill_md: Option<String>,
    budget: Option<&'a Cell<u64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
struct FileIdentity {
    device: u64,
    inode: u64,
    mode: u32,
    links: u64,
    size: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            links: metadata.nlink(),
            size: metadata.size(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NodeMetadata {
    identity: FileIdentity,
}

impl NodeMetadata {
    fn from_stat(stat: &libc::stat) -> Self {
        Self {
            identity: FileIdentity {
                device: stat.st_dev as u64,
                inode: stat.st_ino,
                mode: stat.st_mode as u32,
                links: stat.st_nlink as u64,
                size: stat.st_size.max(0) as u64,
            },
        }
    }

    fn is_dir(self) -> bool {
        self.identity.mode & u32::from(libc::S_IFMT) == u32::from(libc::S_IFDIR)
    }

    fn is_file(self) -> bool {
        self.identity.mode & u32::from(libc::S_IFMT) == u32::from(libc::S_IFREG)
    }

    fn is_symlink(self) -> bool {
        self.identity.mode & u32::from(libc::S_IFMT) == u32::from(libc::S_IFLNK)
    }

    fn nlink(self) -> u64 {
        self.identity.links
    }

    fn mode(self) -> u32 {
        self.identity.mode
    }
}

pub(crate) fn prepare_skill_import(
    paths: &AppPaths,
    source: &Path,
) -> Result<PreparedSkillImport, AppError> {
    prepare_skill_import_budgeted(paths, source, None, None)
}

pub(crate) fn prepare_github_skill_import(
    paths: &AppPaths,
    source: &Path,
    normalized_url: &str,
) -> Result<PreparedSkillImport, AppError> {
    let mut prepared = prepare_skill_import(paths, source)?;
    prepared.source_path = normalized_url.to_owned();
    Ok(prepared)
}

pub(super) fn prepare_discovered_skill_import(
    paths: &AppPaths,
    evidence: &SkillSourceEvidence,
    budget: &Cell<u64>,
) -> Result<PreparedSkillImport, AppError> {
    verify_skill_source(evidence)?;
    prepare_skill_import_budgeted(
        paths,
        &evidence.resolved,
        Some(budget),
        Some(evidence.identity),
    )
}

fn prepare_skill_import_budgeted(
    paths: &AppPaths,
    source: &Path,
    budget: Option<&Cell<u64>>,
    expected_source_identity: Option<FileIdentity>,
) -> Result<PreparedSkillImport, AppError> {
    validate_source_root(paths, source)?;
    let (source, source_identity) = canonical_source_directory(source)?;
    if expected_source_identity.is_some_and(|expected| expected != source_identity) {
        return Err(AppError::conflict(
            "sourcePath",
            "Skill 来源在复制前已被替换",
        ));
    }
    let source_text = path_text(&source, "sourcePath")?;
    let id = Uuid::new_v4().to_string();
    let staging_path = paths.staging().join(format!("skill-import-{id}"));
    if fs::symlink_metadata(&staging_path).is_ok() {
        return Err(AppError::conflict(
            "centralSkill",
            "Skill 导入临时目录已存在",
        ));
    }

    create_private_directory(&staging_path)?;
    let staging_identity =
        FileIdentity::from_metadata(&fs::symlink_metadata(&staging_path).map_err(|error| {
            AppError::invalid_input("staging", "无法核验临时目录").with_source(error)
        })?);
    let result = (|| {
        let copied =
            digest_tree_budgeted(&source, Some(&staging_path), Some(source_identity), budget)?;
        let source_after = digest_tree_budgeted(&source, None, Some(source_identity), budget)?;
        let staging_after = digest_tree_budgeted(&staging_path, None, None, budget)?;
        if copied.hash != source_after.hash || copied.hash != staging_after.hash {
            return Err(AppError::conflict(
                "sourcePath",
                "Skill 来源在导入过程中发生变化",
            ));
        }
        let skill_md = staging_after
            .skill_md
            .ok_or_else(|| AppError::invalid_input("SKILL.md", "Skill 缺少普通 SKILL.md"))?;
        let (name, frontmatter) = parse_skill_frontmatter(&skill_md)?;
        // 中央副本以 frontmatter.name 命名；重名目录提前失败，避免拖到 finalize 才发现。
        let central_path = paths.central_skills().join(&name);
        if fs::symlink_metadata(&central_path).is_ok() {
            return Err(AppError::conflict("centralSkill", "中央已存在同名技能目录"));
        }
        sync_directory(&staging_path)?;
        Ok(PreparedSkillImport {
            id,
            name,
            source_path: source_text,
            central_path: path_text(&central_path, "centralPath")?,
            content_hash: copied.hash,
            frontmatter,
            staging_path: staging_path.clone(),
            finalized: false,
            directory_identity: FileIdentity::from_metadata(
                &fs::symlink_metadata(&staging_path).map_err(|error| {
                    AppError::invalid_input("staging", "无法核验临时目录").with_source(error)
                })?,
            ),
        })
    })();
    if result.is_err() {
        let (directory, _) = open_directory_chain(&staging_path)?;
        let actual = FileIdentity::from_metadata(&directory.metadata().map_err(|error| {
            AppError::invalid_input("staging", "无法核验临时目录").with_source(error)
        })?);
        if actual.device != staging_identity.device
            || actual.inode != staging_identity.inode
            || actual.mode != staging_identity.mode
        {
            return Err(AppError::conflict("staging", "临时目录身份变化，拒绝删除"));
        }
        remove_owned_directory(&staging_path, paths.staging())?;
    }
    result
}

pub(crate) fn finalize_skill_import(
    paths: &AppPaths,
    prepared: &mut PreparedSkillImport,
) -> Result<(), AppError> {
    finalize_skill_import_budgeted(paths, prepared, None)
}

pub(super) fn finalize_skill_import_budgeted(
    paths: &AppPaths,
    prepared: &mut PreparedSkillImport,
    budget: Option<&Cell<u64>>,
) -> Result<(), AppError> {
    if prepared.finalized {
        return Err(AppError::conflict(
            "centralSkill",
            "Skill 导入已经完成原子入库",
        ));
    }
    let central_path = Path::new(&prepared.central_path);
    validate_direct_child(central_path, paths.central_skills(), &prepared.name)?;
    verify_prepared_import_budgeted(paths, prepared, budget)?;
    match fs::symlink_metadata(central_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => {
            return Err(AppError::conflict(
                "centralSkill",
                "中央 Skill 目标已被占用",
            ))
        }
    }
    rename_import_exclusively(&prepared.staging_path, central_path)?;
    // rename 成功后正式目录已经存在。后续 fsync 即使失败，清理逻辑也必须
    // 针对正式目录，不能误以为 staging 仍存在。
    prepared.finalized = true;
    sync_directory(paths.staging())?;
    sync_directory(paths.central_skills())?;
    Ok(())
}

/// 原子拒绝已存在的目标；存在检查与普通 rename 之间不能留下覆盖窗口。
pub(crate) fn rename_import_exclusively(source: &Path, destination: &Path) -> Result<(), AppError> {
    let parent = |path: &Path| {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::invalid_input("centralPath", "导入目录缺少父路径"))
    };
    let (source_parent, _) = open_directory_chain(&parent(source)?)?;
    let (destination_parent, _) = open_directory_chain(&parent(destination)?)?;
    let source_name = c_name(
        source
            .file_name()
            .ok_or_else(|| AppError::invalid_input("sourcePath", "导入目录缺少名称"))?,
    )?;
    let destination_name = c_name(
        destination
            .file_name()
            .ok_or_else(|| AppError::invalid_input("centralPath", "中央目录缺少名称"))?,
    )?;
    // SAFETY: 两个父目录 fd 与单段 NUL 结尾名称在调用期间有效；禁止覆盖目标。
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            source_parent.as_raw_fd(),
            source_name.as_ptr(),
            destination_parent.as_raw_fd(),
            destination_name.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    // SAFETY: Linux renameat2 的参数与上面相同，RENAME_NOREPLACE 提供同一合同。
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            source_parent.as_raw_fd(),
            source_name.as_ptr(),
            destination_parent.as_raw_fd(),
            destination_name.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let result = -1;
    if result != 0 {
        return Err(AppError::atomic_write(
            &destination.to_string_lossy(),
            "rename_skill_into_central_library",
        )
        .with_source(io::Error::last_os_error()));
    }
    Ok(())
}

pub(crate) fn cleanup_failed_import(
    paths: &AppPaths,
    prepared: &PreparedSkillImport,
) -> Result<(), AppError> {
    let path = if prepared.finalized {
        Path::new(&prepared.central_path)
    } else {
        &prepared.staging_path
    };
    let owner = if prepared.finalized {
        paths.central_skills()
    } else {
        paths.staging()
    };
    verify_prepared_import(paths, prepared)?;
    remove_owned_directory(path, owner)
}

pub(super) fn verify_prepared_import(
    paths: &AppPaths,
    prepared: &PreparedSkillImport,
) -> Result<(), AppError> {
    verify_prepared_import_budgeted(paths, prepared, None)
}

pub(super) fn verify_prepared_import_budgeted(
    paths: &AppPaths,
    prepared: &PreparedSkillImport,
    budget: Option<&Cell<u64>>,
) -> Result<(), AppError> {
    let (path, owner, name) = if prepared.finalized {
        (
            Path::new(&prepared.central_path),
            paths.central_skills(),
            prepared.name.clone(),
        )
    } else {
        (
            prepared.staging_path.as_path(),
            paths.staging(),
            format!("skill-import-{}", prepared.id),
        )
    };
    validate_direct_child(path, owner, &name)?;
    let (directory, _) = open_directory_chain(path)?;
    ensure_same_identity(
        prepared.directory_identity,
        &directory.metadata().map_err(|error| {
            AppError::invalid_input("centralSkill", "无法核验本次导入目录").with_source(error)
        })?,
        path,
    )?;
    if digest_tree_budgeted(path, None, Some(prepared.directory_identity), budget)?.hash
        != prepared.content_hash
    {
        return Err(AppError::conflict(
            "centralSkill",
            "导入副本已变化，拒绝处理未知内容",
        ));
    }
    Ok(())
}

pub(crate) fn inspect_central_skill(
    paths: &AppPaths,
    id: &str,
    name: &str,
    central_path: &str,
    expected_hash: &str,
    stored_status: SkillStatus,
    include_content: bool,
) -> Result<CentralSkillInspection, AppError> {
    // 列表场景（不取正文）允许命中 stat 指纹缓存，避免每次列表都全树读哈希；
    // 需要正文时总是完整读取并刷新缓存。
    let digest = match inspect_central_skill_tree(paths, id, name, central_path, !include_content)?
    {
        Ok(digest) => digest,
        Err(inspection) => return Ok(inspection),
    };
    if digest.hash != expected_hash || stored_status != SkillStatus::Ready {
        return Ok(CentralSkillInspection {
            status: SkillStatus::Invalid,
            diagnostic_code: Some("CENTRAL_SKILL_CONTENT_CHANGED"),
            files: digest.files,
            skill_md: None,
        });
    }
    let skill_md = if include_content {
        let text = read_regular_utf8(
            &Path::new(central_path).join("SKILL.md"),
            MAX_SKILL_MD_BYTES,
            "SKILL.md",
        )?;
        parse_skill_frontmatter(&text)?;
        Some(text)
    } else {
        None
    };
    Ok(CentralSkillInspection {
        status: SkillStatus::Ready,
        diagnostic_code: None,
        files: digest.files,
        skill_md,
    })
}

/// 采纳专用读取：路径/类型/canonical/digest 与 `inspect_central_skill` 相同，
/// 但在 hash 漂移时仍解析 `SKILL.md`。不得用于普通列表或内容预览 RPC。
pub(crate) fn read_central_skill_for_adoption(
    paths: &AppPaths,
    id: &str,
    name: &str,
    central_path: &str,
) -> Result<AdoptedCentralSkill, AppError> {
    let digest = match inspect_central_skill_tree(paths, id, name, central_path, false)? {
        Ok(digest) => digest,
        Err(inspection) => {
            return Err(conflict_from_central_inspection(inspection.diagnostic_code))
        }
    };
    let text = digest
        .skill_md
        .ok_or_else(|| AppError::invalid_input("SKILL.md", "Skill 必须包含普通 SKILL.md 文件"))?;
    let (parsed_name, frontmatter) = parse_skill_frontmatter(&text)?;
    Ok(AdoptedCentralSkill {
        name: parsed_name,
        content_hash: digest.hash,
        frontmatter,
    })
}

fn inspect_central_skill_tree(
    paths: &AppPaths,
    id: &str,
    name: &str,
    central_path: &str,
    allow_cached_digest: bool,
) -> Result<Result<TreeDigest, CentralSkillInspection>, AppError> {
    let path = Path::new(central_path);
    validate_central_skill_directory(path, paths.central_skills(), id, name)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Err(CentralSkillInspection {
                status: SkillStatus::Missing,
                diagnostic_code: Some("CENTRAL_SKILL_MISSING"),
                files: Vec::new(),
                skill_md: None,
            }));
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return Err(
                AppError::permission(central_path, "lstat_central_skill").with_source(error)
            );
        }
        Err(error) => {
            return Err(
                AppError::invalid_input("centralPath", "中央 Skill 无法安全读取")
                    .with_source(error),
            )
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(Err(CentralSkillInspection {
            status: SkillStatus::Invalid,
            diagnostic_code: Some("CENTRAL_SKILL_TYPE_CHANGED"),
            files: Vec::new(),
            skill_md: None,
        }));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        AppError::permission(central_path, "canonicalize_central_skill").with_source(error)
    })?;
    if canonical != path {
        return Ok(Err(CentralSkillInspection {
            status: SkillStatus::Invalid,
            diagnostic_code: Some("CENTRAL_SKILL_PATH_CHANGED"),
            files: Vec::new(),
            skill_md: None,
        }));
    }
    match digest_tree_cached(path, allow_cached_digest) {
        Ok(digest) => Ok(Ok(digest)),
        Err(error) if error.code() == crate::error::ErrorCode::PermissionDenied => Err(error),
        Err(_) => Ok(Err(CentralSkillInspection {
            status: SkillStatus::Invalid,
            diagnostic_code: Some("CENTRAL_SKILL_INVALID"),
            files: Vec::new(),
            skill_md: None,
        })),
    }
}

#[derive(Clone)]
struct CachedTreeDigest {
    stat_fingerprint: String,
    hash: String,
    files: Vec<String>,
}

/// 中央 Skill 目录树摘要缓存：键为 canonical 路径，值绑定整棵树的 stat 指纹
/// （每个条目的相对路径、类型、大小、mtime 纳秒、权限位、链接目标）。任何文件
/// 变动都会改变 mtime/size，从而自然失效；只有"同一纳秒内等长改写"才可能命中
/// 陈旧项，而 Apply/接管路径总是重新完整校验树哈希，不依赖这里的结果。
static TREE_DIGEST_CACHE: std::sync::Mutex<
    std::collections::BTreeMap<std::path::PathBuf, CachedTreeDigest>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

#[cfg(test)]
thread_local! {
    /// 测试用：统计完整树摘要（读全部文件内容）的次数。
    pub(crate) static FULL_DIGEST_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn clear_tree_digest_cache() {
    TREE_DIGEST_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

fn digest_tree_cached(path: &Path, allow_cached: bool) -> Result<TreeDigest, AppError> {
    let stat_fingerprint = tree_stat_fingerprint(path)?;
    if allow_cached {
        let cache = TREE_DIGEST_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = cache.get(path) {
            if cached.stat_fingerprint == stat_fingerprint {
                return Ok(TreeDigest {
                    hash: cached.hash.clone(),
                    files: cached.files.clone(),
                    skill_md: None,
                });
            }
        }
    }
    #[cfg(test)]
    FULL_DIGEST_CALLS.with(|count| count.set(count.get() + 1));
    let digest = digest_tree(path, None)?;
    TREE_DIGEST_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            path.to_path_buf(),
            CachedTreeDigest {
                stat_fingerprint,
                hash: digest.hash.clone(),
                files: digest.files.clone(),
            },
        );
    Ok(digest)
}

/// 只读 lstat 的树指纹：不打开任何文件内容。遍历上限与完整摘要一致。
fn tree_stat_fingerprint(root: &Path) -> Result<String, AppError> {
    let mut hasher = Sha256::new();
    let mut entries = 0_usize;
    stat_walk(root, Path::new(""), 0, &mut hasher, &mut entries)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn stat_walk(
    root: &Path,
    relative: &Path,
    depth: usize,
    hasher: &mut Sha256,
    entries: &mut usize,
) -> Result<(), AppError> {
    if depth > MAX_DEPTH {
        return Err(AppError::invalid_input("sourcePath", "Skill 目录层级过深"));
    }
    let directory = root.join(relative);
    let mut names = fs::read_dir(&directory)
        .map_err(|error| map_read_error(error, &directory, "read_skill_directory"))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| map_read_error(error, &directory, "read_skill_directory_entry"))?;
    names.sort();
    for name in names {
        *entries += 1;
        if *entries > MAX_FILES {
            return Err(AppError::invalid_input(
                "sourcePath",
                "Skill 文件数量超出限制",
            ));
        }
        let child_relative = relative.join(&name);
        let child = root.join(&child_relative);
        let metadata = fs::symlink_metadata(&child)
            .map_err(|error| map_read_error(error, &child, "lstat_skill_entry"))?;
        let file_type = metadata.file_type();
        let kind: u8 = if file_type.is_symlink() {
            b'L'
        } else if file_type.is_dir() {
            b'D'
        } else if file_type.is_file() {
            b'F'
        } else {
            b'S'
        };
        hasher.update([kind]);
        hasher.update(child_relative.as_os_str().as_bytes());
        hasher.update([0]);
        hasher.update(metadata.len().to_be_bytes());
        hasher.update((metadata.mode() & 0o7777).to_be_bytes());
        hasher.update(metadata.mtime().to_be_bytes());
        hasher.update(metadata.mtime_nsec().to_be_bytes());
        if file_type.is_symlink() {
            let target = fs::read_link(&child)
                .map_err(|error| map_read_error(error, &child, "read_skill_link"))?;
            hasher.update(target.as_os_str().as_bytes());
        } else if file_type.is_dir() {
            stat_walk(root, &child_relative, depth + 1, hasher, entries)?;
        }
    }
    Ok(())
}

fn conflict_from_central_inspection(diagnostic_code: Option<&'static str>) -> AppError {
    match diagnostic_code {
        Some("CENTRAL_SKILL_MISSING") => {
            AppError::conflict("centralSkill", "中央 Skill 目录已缺失，不能采纳当前文件")
        }
        Some("CENTRAL_SKILL_TYPE_CHANGED") => {
            AppError::conflict("centralSkill", "中央 Skill 类型已变化，不能采纳当前文件")
        }
        Some("CENTRAL_SKILL_PATH_CHANGED") => {
            AppError::conflict("centralSkill", "中央 Skill 路径已变化，不能采纳当前文件")
        }
        _ => AppError::conflict(
            "centralSkill",
            "中央 Skill 内容无法安全读取，不能采纳当前文件",
        ),
    }
}

pub(crate) fn quarantine_central_skill(
    paths: &AppPaths,
    id: &str,
    name: &str,
    central_path: &str,
    expected_hash: &str,
) -> Result<Option<PathBuf>, AppError> {
    let inspection = inspect_central_skill(
        paths,
        id,
        name,
        central_path,
        expected_hash,
        SkillStatus::Ready,
        false,
    )?;
    if inspection.status == SkillStatus::Missing {
        return Ok(None);
    }
    if inspection.status != SkillStatus::Ready {
        return Err(AppError::conflict(
            "centralSkill",
            "中央 Skill 内容或类型已变化，拒绝删除未知目录",
        ));
    }
    let quarantine = paths
        .staging()
        .join(format!("skill-delete-{id}-{}", Uuid::new_v4()));
    fs::rename(central_path, &quarantine).map_err(|error| {
        AppError::atomic_write(central_path, "quarantine_central_skill").with_source(error)
    })?;
    let post_rename_validation = (|| {
        sync_directory(paths.central_skills())?;
        sync_directory(paths.staging())?;
        if digest_tree(&quarantine, None)?.hash != expected_hash {
            return Err(AppError::conflict(
                "centralSkill",
                "中央 Skill 在隔离删除前发生变化",
            ));
        }
        Ok(())
    })();
    if let Err(error) = post_rename_validation {
        if restore_quarantined_skill(paths, &quarantine, central_path).is_err() {
            return Err(AppError::rollback_failed(
                id,
                central_path,
                &quarantine.to_string_lossy(),
            ));
        }
        return Err(error);
    }
    Ok(Some(quarantine))
}

pub(crate) fn restore_quarantined_skill(
    paths: &AppPaths,
    quarantine: &Path,
    central_path: &str,
) -> Result<(), AppError> {
    match fs::symlink_metadata(central_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => {
            return Err(AppError::conflict(
                "centralSkill",
                "中央 Skill 恢复位置已被其他条目占用",
            ))
        }
    }
    fs::rename(quarantine, central_path).map_err(|error| {
        AppError::atomic_write(central_path, "restore_quarantined_skill").with_source(error)
    })?;
    sync_directory(paths.staging())?;
    sync_directory(paths.central_skills())
}

pub(crate) fn delete_quarantined_skill(
    paths: &AppPaths,
    quarantine: &Path,
    expected_hash: &str,
) -> Result<(), AppError> {
    if digest_tree(quarantine, None)?.hash != expected_hash {
        return Err(AppError::conflict(
            "centralSkill",
            "隔离目录内容发生变化，拒绝递归删除未知内容",
        ));
    }
    remove_owned_directory(quarantine, paths.staging())
}

fn validate_source_root(paths: &AppPaths, source: &Path) -> Result<(), AppError> {
    if !source.is_absolute()
        || source == Path::new("/")
        || source.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 来源必须是无相对片段的非根绝对路径",
        ));
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        let app_error = match error.kind() {
            io::ErrorKind::NotFound => {
                AppError::not_found("skillSource", &source.to_string_lossy())
            }
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&source.to_string_lossy(), "lstat_skill_source")
            }
            _ => AppError::invalid_input("sourcePath", "Skill 来源无法安全读取"),
        };
        app_error.with_source(error)
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 来源必须是真实目录，不能是符号链接",
        ));
    }
    let canonical = fs::canonicalize(source).map_err(|error| {
        AppError::permission(&source.to_string_lossy(), "canonicalize_skill_source")
            .with_source(error)
    })?;
    if canonical.starts_with(paths.data_root()) {
        return Err(AppError::invalid_input(
            "sourcePath",
            "不能从应用私有目录重新导入 Skill",
        ));
    }
    let skill_md = source.join("SKILL.md");
    let skill_md_metadata = fs::symlink_metadata(&skill_md).map_err(|error| {
        let app_error = match error.kind() {
            io::ErrorKind::NotFound => {
                AppError::invalid_input("SKILL.md", "Skill 目录缺少 SKILL.md")
            }
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&skill_md.to_string_lossy(), "lstat_skill_md")
            }
            _ => AppError::invalid_input("SKILL.md", "SKILL.md 无法安全读取"),
        };
        app_error.with_source(error)
    })?;
    if skill_md_metadata.file_type().is_symlink() || !skill_md_metadata.is_file() {
        return Err(AppError::invalid_input(
            "SKILL.md",
            "SKILL.md 必须是普通文件，不能是链接或特殊文件",
        ));
    }
    if skill_md_metadata.len() > MAX_SKILL_MD_BYTES {
        return Err(AppError::invalid_input("SKILL.md", "SKILL.md 超出大小限制"));
    }
    Ok(())
}

fn canonical_source_directory(source: &Path) -> Result<(PathBuf, FileIdentity), AppError> {
    let canonical = fs::canonicalize(source).map_err(|error| {
        AppError::permission(&source.to_string_lossy(), "canonicalize_skill_source")
            .with_source(error)
    })?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|error| {
        AppError::not_found("skillSource", &canonical.to_string_lossy()).with_source(error)
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 来源必须是真实目录",
        ));
    }
    Ok((canonical, FileIdentity::from_metadata(&metadata)))
}

fn digest_tree(source: &Path, destination: Option<&Path>) -> Result<TreeDigest, AppError> {
    digest_tree_with_root_identity(source, destination, None)
}

fn digest_tree_with_root_identity(
    source: &Path,
    destination: Option<&Path>,
    expected_root_identity: Option<FileIdentity>,
) -> Result<TreeDigest, AppError> {
    digest_tree_budgeted(source, destination, expected_root_identity, None)
}

fn digest_tree_budgeted(
    source: &Path,
    destination: Option<&Path>,
    expected_root_identity: Option<FileIdentity>,
    budget: Option<&Cell<u64>>,
) -> Result<TreeDigest, AppError> {
    let canonical_root = fs::canonicalize(source).map_err(|error| {
        AppError::permission(&source.to_string_lossy(), "canonicalize_skill_tree")
            .with_source(error)
    })?;
    if canonical_root != source {
        return Err(AppError::conflict(
            "sourcePath",
            "Skill 目录在读取过程中改变了 canonical 身份",
        ));
    }
    let (source_directory, _) = open_directory_chain(source)?;
    let opened_root = source_directory
        .metadata()
        .map_err(|error| map_read_error(error, source, "stat_skill_root"))?;
    if !opened_root.is_dir() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 根必须是普通目录",
        ));
    }
    let opened_root_identity = FileIdentity::from_metadata(&opened_root);
    if expected_root_identity.is_some_and(|expected| expected != opened_root_identity) {
        return Err(AppError::conflict(
            "sourcePath",
            "Skill 来源根目录在导入开始前发生变化",
        ));
    }
    let mut hasher = Sha256::new();
    let mut files = Vec::new();
    let mut limits = WalkLimits {
        budget,
        ..WalkLimits::default()
    };
    walk_directory(
        &source_directory,
        &source_directory,
        source,
        Path::new(""),
        destination,
        0,
        &mut hasher,
        &mut files,
        &mut limits,
    )?;
    ensure_same_identity(
        opened_root_identity,
        &source_directory
            .metadata()
            .map_err(|error| map_read_error(error, source, "restat_skill_root"))?,
        source,
    )?;
    Ok(TreeDigest {
        hash: format!("{:x}", hasher.finalize()),
        files,
        skill_md: limits.skill_md,
    })
}
