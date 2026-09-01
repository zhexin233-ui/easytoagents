//! Skill 目录的安全复制、稳定 hash 与中央库所有权证明。

use std::{
    cell::Cell,
    collections::BTreeSet,
    ffi::{CStr, CString, OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{symlink, MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    app::AppPaths,
    domain::{ArtifactName, SkillStatus},
    error::AppError,
    sync::hash_json,
};

const MAX_FILES: usize = 4_096;
const MAX_DEPTH: usize = 32;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SKILL_MD_BYTES: u64 = 512 * 1024;
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RELATIVE_PATH_BYTES: usize = 1_024;

#[derive(Debug)]
pub(crate) struct PreparedSkillImport {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub central_path: String,
    pub content_hash: String,
    pub frontmatter: Value,
    staging_path: PathBuf,
    finalized: bool,
    directory_identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CentralSkillInspection {
    pub status: SkillStatus,
    pub diagnostic_code: Option<&'static str>,
    pub files: Vec<String>,
    pub skill_md: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillTakeoverEntryKind {
    ExternalSymlink,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillTakeoverInspection {
    pub entry_type: SkillTakeoverEntryKind,
    pub fingerprint: String,
    pub content_hash: String,
    pub resolved: PathBuf,
}

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
    let staging_identity = FileIdentity::from_metadata(
        &fs::symlink_metadata(&staging_path)
            .map_err(|_| AppError::invalid_input("staging", "无法核验临时目录"))?,
    );
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
                &fs::symlink_metadata(&staging_path)
                    .map_err(|_| AppError::invalid_input("staging", "无法核验临时目录"))?,
            ),
        })
    })();
    if result.is_err() {
        let (directory, _) = open_directory_chain(&staging_path)?;
        let actual = FileIdentity::from_metadata(
            &directory
                .metadata()
                .map_err(|_| AppError::invalid_input("staging", "无法核验临时目录"))?,
        );
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
        ));
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
        &directory
            .metadata()
            .map_err(|_| AppError::invalid_input("centralSkill", "无法核验本次导入目录"))?,
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
    let path = Path::new(central_path);
    validate_central_skill_directory(path, paths.central_skills(), id, name)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CentralSkillInspection {
                status: SkillStatus::Missing,
                diagnostic_code: Some("CENTRAL_SKILL_MISSING"),
                files: Vec::new(),
                skill_md: None,
            });
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return Err(AppError::permission(central_path, "lstat_central_skill"));
        }
        Err(_) => {
            return Err(AppError::invalid_input(
                "centralPath",
                "中央 Skill 无法安全读取",
            ))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(CentralSkillInspection {
            status: SkillStatus::Invalid,
            diagnostic_code: Some("CENTRAL_SKILL_TYPE_CHANGED"),
            files: Vec::new(),
            skill_md: None,
        });
    }
    let canonical = fs::canonicalize(path)
        .map_err(|_| AppError::permission(central_path, "canonicalize_central_skill"))?;
    if canonical != path {
        return Ok(CentralSkillInspection {
            status: SkillStatus::Invalid,
            diagnostic_code: Some("CENTRAL_SKILL_PATH_CHANGED"),
            files: Vec::new(),
            skill_md: None,
        });
    }
    let digest = match digest_tree(path, None) {
        Ok(digest) => digest,
        Err(error) if error.code() == crate::error::ErrorCode::PermissionDenied => {
            return Err(error)
        }
        Err(_) => {
            return Ok(CentralSkillInspection {
                status: SkillStatus::Invalid,
                diagnostic_code: Some("CENTRAL_SKILL_INVALID"),
                files: Vec::new(),
                skill_md: None,
            })
        }
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
        let text = read_regular_utf8(&path.join("SKILL.md"), MAX_SKILL_MD_BYTES, "SKILL.md")?;
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
    fs::rename(central_path, &quarantine)
        .map_err(|_| AppError::atomic_write(central_path, "quarantine_central_skill"))?;
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
    fs::rename(quarantine, central_path)
        .map_err(|_| AppError::atomic_write(central_path, "restore_quarantined_skill"))?;
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
    let metadata = fs::symlink_metadata(source).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => AppError::not_found("skillSource", &source.to_string_lossy()),
        io::ErrorKind::PermissionDenied => {
            AppError::permission(&source.to_string_lossy(), "lstat_skill_source")
        }
        _ => AppError::invalid_input("sourcePath", "Skill 来源无法安全读取"),
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 来源必须是真实目录，不能是符号链接",
        ));
    }
    let canonical = fs::canonicalize(source).map_err(|_| {
        AppError::permission(&source.to_string_lossy(), "canonicalize_skill_source")
    })?;
    if canonical.starts_with(paths.data_root()) {
        return Err(AppError::invalid_input(
            "sourcePath",
            "不能从应用私有目录重新导入 Skill",
        ));
    }
    let skill_md = source.join("SKILL.md");
    let skill_md_metadata =
        fs::symlink_metadata(&skill_md).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => {
                AppError::invalid_input("SKILL.md", "Skill 目录缺少 SKILL.md")
            }
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&skill_md.to_string_lossy(), "lstat_skill_md")
            }
            _ => AppError::invalid_input("SKILL.md", "SKILL.md 无法安全读取"),
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
    let canonical = fs::canonicalize(source).map_err(|_| {
        AppError::permission(&source.to_string_lossy(), "canonicalize_skill_source")
    })?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|_| AppError::not_found("skillSource", &canonical.to_string_lossy()))?;
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
    let canonical_root = fs::canonicalize(source)
        .map_err(|_| AppError::permission(&source.to_string_lossy(), "canonicalize_skill_tree"))?;
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

#[allow(clippy::too_many_arguments)]
fn walk_directory(
    source_root_directory: &File,
    source_directory: &File,
    source_root: &Path,
    relative: &Path,
    destination_root: Option<&Path>,
    depth: usize,
    hasher: &mut Sha256,
    files: &mut Vec<String>,
    limits: &mut WalkLimits,
) -> Result<(), AppError> {
    if depth > MAX_DEPTH {
        return Err(AppError::invalid_input("sourcePath", "Skill 目录层级过深"));
    }
    let directory = source_root.join(relative);
    let mut entries = read_directory_names(source_directory, &directory)?;
    entries.sort();
    for entry in entries {
        limits.entries += 1;
        if limits.entries > MAX_FILES {
            return Err(AppError::invalid_input(
                "sourcePath",
                "Skill 文件数量超出限制",
            ));
        }
        let name = entry
            .into_string()
            .map_err(|_| AppError::invalid_input("sourcePath", "Skill 路径必须是 UTF-8"))?;
        if name == "." || name == ".." || name.contains('/') || name.contains('\0') {
            return Err(AppError::invalid_input(
                "sourcePath",
                "Skill 包含不安全路径名",
            ));
        }
        let child_relative = relative.join(&name);
        let relative_text = path_text(&child_relative, "sourcePath")?;
        if relative_text.len() > MAX_RELATIVE_PATH_BYTES {
            return Err(AppError::invalid_input("sourcePath", "Skill 相对路径过长"));
        }
        let source_child = source_root.join(&child_relative);
        let metadata = lstat_at(source_directory, OsStr::new(&name), &source_child)?;
        let destination_child = destination_root.map(|root| root.join(&child_relative));
        if metadata.is_dir() && !metadata.is_symlink() {
            let child_directory =
                open_directory_at_nofollow(source_directory, OsStr::new(&name), &source_child)?;
            ensure_same_identity(
                metadata.identity,
                &child_directory.metadata().map_err(|error| {
                    map_read_error(error, &source_child, "stat_open_skill_directory")
                })?,
                &source_child,
            )?;
            hash_record(hasher, b'D', &relative_text, &[]);
            if let Some(destination) = &destination_child {
                create_private_directory(destination)?;
            }
            walk_directory(
                source_root_directory,
                &child_directory,
                source_root,
                &child_relative,
                destination_root,
                depth + 1,
                hasher,
                files,
                limits,
            )?;
            if let Some(destination) = &destination_child {
                sync_directory(destination)?;
            }
            ensure_same_identity(
                metadata.identity,
                &child_directory.metadata().map_err(|error| {
                    map_read_error(error, &source_child, "restat_skill_directory")
                })?,
                &source_child,
            )?;
        } else if metadata.is_file() && !metadata.is_symlink() {
            if metadata.nlink() != 1 {
                return Err(AppError::invalid_input(
                    "sourcePath",
                    "Skill 不允许硬链接文件",
                ));
            }
            let input = open_file_at_nofollow(source_directory, OsStr::new(&name), &source_child)?;
            let bytes = copy_regular_file(
                input,
                &metadata,
                &source_child,
                destination_child.as_deref(),
                metadata.mode(),
                limits,
            )?;
            if relative_text == "SKILL.md" {
                if bytes.len() as u64 > MAX_SKILL_MD_BYTES {
                    return Err(AppError::invalid_input("SKILL.md", "SKILL.md 超出大小限制"));
                }
                limits.skill_md =
                    Some(String::from_utf8(bytes.clone()).map_err(|_| {
                        AppError::invalid_input("SKILL.md", "Skill 内容必须是 UTF-8")
                    })?);
            }
            hash_file_record(hasher, &relative_text, metadata.mode(), &bytes);
            files.push(relative_text);
        } else if metadata.is_symlink() {
            let raw_target = read_link_at(source_directory, OsStr::new(&name), &source_child)?;
            let metadata_after = lstat_at(source_directory, OsStr::new(&name), &source_child)?;
            if metadata.identity != metadata_after.identity {
                return Err(AppError::conflict(
                    "sourcePath",
                    "Skill 来源在读取过程中发生变化",
                ));
            }
            validate_source_symlink(
                source_root_directory,
                &child_relative,
                &raw_target,
                source_root,
            )?;
            let target_text = path_text(&raw_target, "sourcePath")?;
            hash_record(hasher, b'L', &relative_text, target_text.as_bytes());
            if let Some(destination) = &destination_child {
                symlink(&raw_target, destination).map_err(|_| {
                    AppError::atomic_write(&destination.to_string_lossy(), "copy_skill_symlink")
                })?;
            }
            files.push(relative_text);
        } else {
            return Err(AppError::invalid_input(
                "sourcePath",
                "Skill 包含 socket、FIFO、设备等特殊文件",
            ));
        }
    }
    Ok(())
}

fn copy_regular_file(
    input: File,
    lstat_metadata: &NodeMetadata,
    source: &Path,
    destination: Option<&Path>,
    source_mode: u32,
    limits: &mut WalkLimits,
) -> Result<Vec<u8>, AppError> {
    let metadata = input
        .metadata()
        .map_err(|error| map_read_error(error, source, "stat_open_skill_file"))?;
    ensure_same_identity(lstat_metadata.identity, &metadata, source)?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > MAX_FILE_BYTES {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 普通文件类型无效或超出大小限制",
        ));
    }
    limits.total_bytes = limits
        .total_bytes
        .checked_add(metadata.len())
        .ok_or_else(|| AppError::invalid_input("sourcePath", "Skill 总大小超出限制"))?;
    if limits.total_bytes > MAX_TOTAL_BYTES {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 总大小超出限制",
        ));
    }
    if let Some(budget) = limits.budget {
        let remaining = budget
            .get()
            .checked_sub(metadata.len().saturating_add(1))
            .ok_or_else(|| AppError::invalid_input("budget", "Skills 批量读取超出 128 MiB 限制"))?;
        budget.set(remaining);
    }
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| AppError::invalid_input("sourcePath", "Skill 文件大小超出平台限制"))?;
    let mut bytes = Vec::with_capacity(capacity);
    (&input)
        .take(metadata.len() + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| map_read_error(error, source, "read_skill_file"))?;
    if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(AppError::conflict(
            "sourcePath",
            "Skill 文件在读取过程中发生变化",
        ));
    }
    ensure_same_identity(
        FileIdentity::from_metadata(&metadata),
        &input
            .metadata()
            .map_err(|error| map_read_error(error, source, "restat_skill_file"))?,
        source,
    )?;
    if let Some(destination) = destination {
        let mode = 0o600 | (u32::from(source_mode & 0o111 != 0) * 0o100);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(destination)
            .map_err(|_| {
                AppError::atomic_write(&destination.to_string_lossy(), "create_staged_skill_file")
            })?;
        output
            .set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|_| {
                AppError::permission(&destination.to_string_lossy(), "chmod_staged_skill_file")
            })?;
        output.write_all(&bytes).map_err(|_| {
            AppError::atomic_write(&destination.to_string_lossy(), "write_staged_skill_file")
        })?;
        output.flush().map_err(|_| {
            AppError::atomic_write(&destination.to_string_lossy(), "flush_staged_skill_file")
        })?;
        output.sync_all().map_err(|_| {
            AppError::atomic_write(&destination.to_string_lossy(), "sync_staged_skill_file")
        })?;
    }
    Ok(bytes)
}

fn validate_source_symlink(
    source_root_directory: &File,
    link_relative: &Path,
    raw_target: &Path,
    source_root: &Path,
) -> Result<(), AppError> {
    if raw_target.is_absolute() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 内链接必须使用相对路径，避免保留来源绝对位置",
        ));
    }
    let resolved = normalize_internal_link(link_relative, raw_target)?;
    let mut directory = source_root_directory
        .try_clone()
        .map_err(|error| map_read_error(error, source_root, "clone_skill_root"))?;
    let mut components = resolved.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(segment) = component else {
            return Err(AppError::invalid_input(
                "sourcePath",
                "Skill 符号链接目标包含不安全路径片段",
            ));
        };
        let target_path = source_root.join(&resolved);
        if components.peek().is_some() {
            directory =
                open_directory_at_nofollow(&directory, segment, &target_path).map_err(|_| {
                    AppError::invalid_input("sourcePath", "Skill 符号链接包含循环、断链或链接目录")
                })?;
        } else {
            let metadata = lstat_at(&directory, segment, &target_path)
                .map_err(|_| AppError::invalid_input("sourcePath", "Skill 符号链接目标无法读取"))?;
            if !metadata.is_file() || metadata.is_symlink() || metadata.nlink() != 1 {
                return Err(AppError::invalid_input(
                    "sourcePath",
                    "Skill 只允许指向目录内普通非硬链接文件的符号链接",
                ));
            }
            let opened =
                open_file_at_nofollow(&directory, segment, &target_path).map_err(|_| {
                    AppError::invalid_input("sourcePath", "Skill 符号链接目标无法安全打开")
                })?;
            ensure_same_identity(
                metadata.identity,
                &opened.metadata().map_err(|error| {
                    map_read_error(error, &target_path, "stat_skill_symlink_target")
                })?,
                &target_path,
            )?;
        }
    }
    if resolved.as_os_str().is_empty() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 符号链接不能指向来源根目录",
        ));
    }
    Ok(())
}

fn normalize_internal_link(link_relative: &Path, raw_target: &Path) -> Result<PathBuf, AppError> {
    let mut normalized = link_relative
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    for component in raw_target.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(AppError::invalid_input(
                        "sourcePath",
                        "Skill 符号链接逃逸出来源目录",
                    ));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::invalid_input(
                    "sourcePath",
                    "Skill 符号链接目标必须位于来源目录内",
                ));
            }
        }
    }
    Ok(normalized)
}

fn read_directory_names(directory: &File, display_path: &Path) -> Result<Vec<OsString>, AppError> {
    // fdopendir 会接管 fd，因此先复制一份，原 File 仍用于后续 openat。
    // SAFETY: directory fd 在调用期间有效。
    let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
    if duplicate < 0 {
        return Err(map_read_error(
            io::Error::last_os_error(),
            display_path,
            "duplicate_skill_directory",
        ));
    }
    // SAFETY: duplicate 是有效且尚未被其他所有者接管的目录 fd。
    let stream = unsafe { libc::fdopendir(duplicate) };
    if stream.is_null() {
        // fdopendir 失败时不会接管 fd。
        // SAFETY: duplicate 仍由本函数独占。
        unsafe { libc::close(duplicate) };
        return Err(map_read_error(
            io::Error::last_os_error(),
            display_path,
            "open_skill_directory_stream",
        ));
    }
    let mut names = Vec::new();
    loop {
        clear_errno();
        // SAFETY: stream 在 closedir 前保持有效；返回指针只在下一次 readdir 前读取。
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            let error = current_errno();
            // SAFETY: stream 由本函数持有且只关闭一次。
            unsafe { libc::closedir(stream) };
            if error == 0 {
                break;
            }
            return Err(map_read_error(
                io::Error::from_raw_os_error(error),
                display_path,
                "read_skill_directory_entry",
            ));
        }
        // SAFETY: POSIX dirent.d_name 是本次 readdir 返回的 NUL 结尾名称。
        let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if bytes != b"." && bytes != b".." {
            if names.len() >= MAX_FILES {
                // SAFETY: stream 仍由本函数独占，超限后立即关闭。
                unsafe { libc::closedir(stream) };
                return Err(AppError::invalid_input(
                    "sourcePath",
                    "Skill 目录条目数量超出限制",
                ));
            }
            names.push(OsString::from_vec(bytes.to_vec()));
        }
    }
    Ok(names)
}

#[cfg(target_os = "macos")]
fn errno_pointer() -> *mut libc::c_int {
    // SAFETY: __error 返回当前线程 errno 的有效指针。
    unsafe { libc::__error() }
}

#[cfg(not(target_os = "macos"))]
fn errno_pointer() -> *mut libc::c_int {
    // SAFETY: __errno_location 返回当前线程 errno 的有效指针。
    unsafe { libc::__errno_location() }
}

fn clear_errno() {
    // SAFETY: errno_pointer 指向当前线程可写 errno。
    unsafe { *errno_pointer() = 0 };
}

fn current_errno() -> libc::c_int {
    // SAFETY: errno_pointer 指向当前线程可读 errno。
    unsafe { *errno_pointer() }
}

fn c_path(path: &Path, field: &'static str) -> Result<CString, AppError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| AppError::invalid_input(field, "路径不能包含 NUL"))
}

fn c_name(name: &OsStr) -> Result<CString, AppError> {
    CString::new(name.as_bytes())
        .map_err(|_| AppError::invalid_input("sourcePath", "Skill 路径名不能包含 NUL"))
}

fn open_directory_nofollow(path: &Path, operation: &'static str) -> Result<File, AppError> {
    let path_c = c_path(path, "sourcePath")?;
    // SAFETY: path_c 是以 NUL 结尾且不含内部 NUL 的只读路径；返回 fd 立即交给 File 管理。
    let descriptor = unsafe {
        libc::open(
            path_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(map_read_error(io::Error::last_os_error(), path, operation));
    }
    // SAFETY: descriptor 是本函数刚取得且尚未被其他所有者接管的有效 fd。
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn open_directory_at_nofollow(
    parent: &File,
    name: &OsStr,
    display_path: &Path,
) -> Result<File, AppError> {
    let name_c = c_name(name)?;
    // SAFETY: parent fd 在调用期间有效，name_c 是单个 NUL 结尾路径段。
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(map_read_error(
            io::Error::last_os_error(),
            display_path,
            "open_skill_directory_nofollow",
        ));
    }
    // SAFETY: descriptor 是本函数刚取得且尚未被其他所有者接管的有效 fd。
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn open_file_at_nofollow(
    parent: &File,
    name: &OsStr,
    display_path: &Path,
) -> Result<File, AppError> {
    let name_c = c_name(name)?;
    // SAFETY: parent fd 在调用期间有效，O_NOFOLLOW 阻止最后一段被替换为链接。
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(map_read_error(
            io::Error::last_os_error(),
            display_path,
            "open_skill_file_nofollow",
        ));
    }
    // SAFETY: descriptor 是本函数刚取得且尚未被其他所有者接管的有效 fd。
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn lstat_at(parent: &File, name: &OsStr, display_path: &Path) -> Result<NodeMetadata, AppError> {
    let name_c = c_name(name)?;
    // SAFETY: stat 是有效输出缓冲区；parent fd 与 name_c 在调用期间有效。
    let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
    // SAFETY: fstatat 只写入 stat，AT_SYMLINK_NOFOLLOW 保证最后一段不被跟随。
    let result = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name_c.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        return Err(map_read_error(
            io::Error::last_os_error(),
            display_path,
            "lstat_skill_entry",
        ));
    }
    Ok(NodeMetadata::from_stat(&stat))
}

fn read_link_at(parent: &File, name: &OsStr, display_path: &Path) -> Result<PathBuf, AppError> {
    let name_c = c_name(name)?;
    let mut buffer = vec![0_u8; MAX_RELATIVE_PATH_BYTES + 1];
    // SAFETY: parent fd、name_c 和可写 buffer 在调用期间有效；readlinkat 不追加 NUL。
    let length = unsafe {
        libc::readlinkat(
            parent.as_raw_fd(),
            name_c.as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if length < 0 {
        return Err(map_read_error(
            io::Error::last_os_error(),
            display_path,
            "read_skill_symlink",
        ));
    }
    let length = usize::try_from(length)
        .map_err(|_| AppError::invalid_input("sourcePath", "Skill 链接目标长度无效"))?;
    if length == buffer.len() {
        return Err(AppError::invalid_input(
            "sourcePath",
            "Skill 链接目标路径过长",
        ));
    }
    buffer.truncate(length);
    Ok(PathBuf::from(OsString::from_vec(buffer)))
}

fn ensure_same_identity(
    before: FileIdentity,
    after: &fs::Metadata,
    _path: &Path,
) -> Result<(), AppError> {
    if before != FileIdentity::from_metadata(after) {
        return Err(AppError::conflict(
            "sourcePath",
            "Skill 来源在读取过程中发生变化",
        ));
    }
    Ok(())
}

fn parse_skill_frontmatter(text: &str) -> Result<(String, Value), AppError> {
    let mut offset = 0usize;
    let mut yaml_start = None;
    let mut yaml_end = None;
    let mut body_start = None;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let clean = line.trim_end_matches(['\r', '\n']);
        if index == 0 {
            if clean != "---" {
                return Err(AppError::invalid_input(
                    "SKILL.md",
                    "SKILL.md 必须以 YAML frontmatter 开始",
                ));
            }
            yaml_start = Some(line.len());
        } else if clean == "---" {
            yaml_end = Some(offset);
            body_start = Some(offset + line.len());
            break;
        }
        offset += line.len();
    }
    let start = yaml_start
        .ok_or_else(|| AppError::invalid_input("SKILL.md", "SKILL.md 缺少 YAML frontmatter"))?;
    let end = yaml_end.ok_or_else(|| {
        AppError::invalid_input("SKILL.md", "SKILL.md frontmatter 缺少结束分隔线")
    })?;
    let body_start = body_start.ok_or_else(|| {
        AppError::invalid_input("SKILL.md", "SKILL.md frontmatter 缺少结束分隔线")
    })?;
    if text[body_start..].trim().is_empty() {
        return Err(AppError::invalid_input(
            "SKILL.md",
            "SKILL.md 必须包含非空工作流正文",
        ));
    }
    let frontmatter: Value = serde_yaml::from_str(&text[start..end])
        .map_err(|_| AppError::invalid_input("SKILL.md", "SKILL.md frontmatter 不是合法 YAML"))?;
    let object = frontmatter
        .as_object()
        .ok_or_else(|| AppError::invalid_input("SKILL.md", "SKILL.md frontmatter 必须是对象"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid_input("SKILL.md", "frontmatter.name 必须是字符串"))?;
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::invalid_input("SKILL.md", "frontmatter.description 必须是字符串")
        })?;
    if description.trim().is_empty() || description.chars().count() > 1_024 {
        return Err(AppError::invalid_input(
            "SKILL.md",
            "frontmatter.description 必须为 1 到 1024 个字符",
        ));
    }
    validate_skill_name(name)?;
    Ok((name.to_owned(), frontmatter))
}

fn validate_skill_name(name: &str) -> Result<(), AppError> {
    ArtifactName::parse(name.to_owned())?;
    let valid = name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if !valid {
        return Err(AppError::invalid_input(
            "SKILL.md",
            "frontmatter.name 仅允许小写字母、数字和单个连字符，最长 64 字节",
        ));
    }
    if name.eq_ignore_ascii_case("synced") {
        return Err(AppError::invalid_input(
            "SKILL.md",
            "frontmatter.name 不能使用 Claude 保留目录 synced",
        ));
    }
    Ok(())
}

fn read_regular_utf8(path: &Path, limit: u64, field: &'static str) -> Result<String, AppError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| map_read_error(error, path, "open_skill_content"))?;
    let metadata = file
        .metadata()
        .map_err(|error| map_read_error(error, path, "stat_skill_content"))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(AppError::invalid_input(
            field,
            "Skill 内容类型无效或超出大小限制",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| map_read_error(error, path, "read_skill_content"))?;
    if bytes.len() as u64 > limit {
        return Err(AppError::invalid_input(field, "Skill 内容超出大小限制"));
    }
    String::from_utf8(bytes).map_err(|_| AppError::invalid_input(field, "Skill 内容必须是 UTF-8"))
}

fn hash_record(hasher: &mut Sha256, kind: u8, path: &str, payload: &[u8]) {
    hasher.update([kind]);
    hasher.update((path.len() as u64).to_be_bytes());
    hasher.update(path.as_bytes());
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
}

fn hash_file_record(hasher: &mut Sha256, path: &str, mode: u32, bytes: &[u8]) {
    hasher.update([b'F']);
    hasher.update((path.len() as u64).to_be_bytes());
    hasher.update(path.as_bytes());
    // 中央库统一私有权限，但可执行性是 Skill 语义的一部分，必须纳入 hash。
    hasher.update([u8::from(mode & 0o111 != 0)]);
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn create_private_directory(path: &Path) -> Result<(), AppError> {
    fs::create_dir(path)
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "create_skill_directory"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| AppError::permission(&path.to_string_lossy(), "chmod_skill_directory"))?;
    Ok(())
}

fn remove_owned_directory(path: &Path, owner: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::invalid_input("centralPath", "受管目录缺少父目录"))?;
    if parent != owner || path == owner {
        return Err(AppError::conflict(
            "centralPath",
            "拒绝递归删除不属于指定私有根的目录",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        _ => {
            return Err(AppError::conflict(
                "centralPath",
                "拒绝递归删除未知文件或符号链接",
            ))
        }
    }
    fs::remove_dir_all(path)
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "remove_owned_skill_tree"))?;
    sync_directory(owner)
}

fn validate_direct_child(path: &Path, owner: &Path, expected_name: &str) -> Result<(), AppError> {
    if !path.is_absolute()
        || path.parent() != Some(owner)
        || path.file_name().and_then(|name| name.to_str()) != Some(expected_name)
    {
        return Err(AppError::conflict(
            "centralPath",
            "中央 Skill 路径与数据库身份不匹配",
        ));
    }
    Ok(())
}

/// 名称化目录是当前布局；启动迁移完成前，历史记录可能仍以记录 id 命名，两种都必须可用。
pub(crate) fn validate_central_skill_directory(
    path: &Path,
    owner: &Path,
    id: &str,
    name: &str,
) -> Result<(), AppError> {
    if validate_direct_child(path, owner, name).is_ok()
        || validate_direct_child(path, owner, id).is_ok()
    {
        Ok(())
    } else {
        Err(AppError::conflict(
            "centralPath",
            "中央 Skill 路径与数据库身份不匹配",
        ))
    }
}

pub(crate) fn sync_directory(path: &Path) -> Result<(), AppError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| AppError::atomic_write(&path.to_string_lossy(), "sync_skill_directory"))
}

fn path_text(path: &Path, field: &'static str) -> Result<String, AppError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::invalid_input(field, "路径必须是 UTF-8"))
}

fn map_read_error(error: io::Error, path: &Path, operation: &'static str) -> AppError {
    match error.kind() {
        io::ErrorKind::NotFound => AppError::not_found("skillPath", &path.to_string_lossy()),
        io::ErrorKind::PermissionDenied => AppError::permission(&path.to_string_lossy(), operation),
        _ => AppError::invalid_input("sourcePath", "Skill 目录无法稳定读取"),
    }
}

/// 只持久化路径与身份；不包含技能正文或任意 frontmatter。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct SkillSourceEvidence {
    pub root: PathBuf,
    pub entry: PathBuf,
    pub resolved: PathBuf,
    directories: Vec<DirectoryIdentity>,
    links: Vec<SourceLink>,
    identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct DirectoryIdentity {
    path: PathBuf,
    device: u64,
    inode: u64,
    mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct SourceLink {
    path: PathBuf,
    target: PathBuf,
    identity: FileIdentity,
}

pub(super) struct SourceSkillInspection {
    pub name: String,
    pub description: String,
    pub hash: String,
}

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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{fs::symlink, fs::PermissionsExt, net::UnixListener},
    };

    use tempfile::TempDir;

    use super::{
        canonical_source_directory, delete_quarantined_skill, digest_tree,
        digest_tree_with_root_identity, finalize_skill_import, prepare_skill_import,
        quarantine_central_skill,
    };
    use crate::app::AppPaths;

    struct Fixture {
        _temporary: TempDir,
        paths: AppPaths,
        root: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let paths = AppPaths::from_data_root(root.join("private/app-data")).unwrap();
            paths.initialize().unwrap();
            Self {
                _temporary: temporary,
                paths,
                root,
            }
        }

        fn source(&self, name: &str) -> std::path::PathBuf {
            let source = self.root.join(name);
            fs::create_dir(&source).unwrap();
            source
        }
    }

    fn write_valid_skill(source: &std::path::Path, name: &str) {
        fs::write(
            source.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: 隔离测试 Skill\n---\n\n# {name}\n"),
        )
        .unwrap();
        fs::create_dir(source.join("scripts")).unwrap();
        fs::write(source.join("scripts/run.sh"), "#!/bin/sh\necho fixture\n").unwrap();
    }

    #[test]
    fn valid_import_is_staged_hashed_and_atomically_copied_without_touching_source() {
        let fixture = Fixture::new();
        let source = fixture.source("source");
        write_valid_skill(&source, "fixture-skill");
        fs::set_permissions(
            source.join("scripts/run.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        symlink("scripts/run.sh", source.join("runner")).unwrap();
        let before_skill = fs::read(source.join("SKILL.md")).unwrap();
        let before_script = fs::read(source.join("scripts/run.sh")).unwrap();

        let mut prepared = prepare_skill_import(&fixture.paths, &source).unwrap();
        assert_eq!(prepared.name, "fixture-skill");
        assert_eq!(prepared.content_hash.len(), 64);
        assert_eq!(
            prepared.content_hash,
            digest_tree(&source, None).unwrap().hash
        );
        finalize_skill_import(&fixture.paths, &mut prepared).unwrap();

        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), before_skill);
        assert_eq!(
            fs::read(source.join("scripts/run.sh")).unwrap(),
            before_script
        );
        assert!(source.join("runner").is_symlink());
        let central = std::path::Path::new(&prepared.central_path);
        assert_eq!(
            central.file_name().and_then(|name| name.to_str()),
            Some("fixture-skill"),
            "中央副本目录必须以 frontmatter.name 命名"
        );
        assert_eq!(
            prepared.content_hash,
            digest_tree(central, None).unwrap().hash
        );
        assert_eq!(fs::read(central.join("SKILL.md")).unwrap(), before_skill);
        assert!(central.join("runner").is_symlink());
        assert_eq!(
            fs::metadata(central).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(central.join("scripts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(central.join("SKILL.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(central.join("scripts/run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(source.join("scripts/run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755,
            "导入不得修改来源权限"
        );
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn same_name_import_conflicts_without_leaving_partial_copies() {
        let fixture = Fixture::new();
        let first = fixture.source("first");
        write_valid_skill(&first, "fixture-skill");
        let mut prepared = prepare_skill_import(&fixture.paths, &first).unwrap();
        finalize_skill_import(&fixture.paths, &mut prepared).unwrap();

        // 来源目录名与 frontmatter.name 不同：中央命名只看 frontmatter.name。
        let second = fixture.source("second");
        write_valid_skill(&second, "fixture-skill");
        assert!(prepare_skill_import(&fixture.paths, &second).is_err());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
        assert_eq!(
            fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn missing_or_malformed_frontmatter_is_rejected_and_staging_is_cleaned() {
        for (name, content) in [
            ("missing", "# no frontmatter\n"),
            ("broken-yaml", "---\nname: [\ndescription: broken\n---\n"),
            ("missing-description", "---\nname: fixture-skill\n---\n"),
            (
                "unsafe-name",
                "---\nname: ../escape\ndescription: bad\n---\n",
            ),
            (
                "empty-body",
                "---\nname: empty-body\ndescription: bad\n---\n\n",
            ),
            (
                "reserved-name",
                "---\nname: synced\ndescription: reserved\n---\n\n# body\n",
            ),
        ] {
            let fixture = Fixture::new();
            let source = fixture.source(name);
            fs::write(source.join("SKILL.md"), content).unwrap();
            assert!(prepare_skill_import(&fixture.paths, &source).is_err());
            assert!(fs::read_dir(fixture.paths.staging())
                .unwrap()
                .next()
                .is_none());
            assert!(fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .next()
                .is_none());
        }
    }

    #[test]
    fn escape_broken_cycle_directory_links_and_special_files_are_rejected() {
        for case in ["escape", "broken", "cycle", "directory", "special"] {
            let fixture = Fixture::new();
            let source = fixture.source(case);
            write_valid_skill(&source, "fixture-skill");
            match case {
                "escape" => {
                    fs::write(fixture.root.join("outside.txt"), "outside").unwrap();
                    symlink("../outside.txt", source.join("bad-link")).unwrap();
                }
                "broken" => symlink("missing.txt", source.join("bad-link")).unwrap(),
                "cycle" => {
                    symlink("loop-b", source.join("loop-a")).unwrap();
                    symlink("loop-a", source.join("loop-b")).unwrap();
                }
                "directory" => symlink("scripts", source.join("linked-directory")).unwrap(),
                "special" => {
                    UnixListener::bind(source.join("socket")).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                prepare_skill_import(&fixture.paths, &source).is_err(),
                "{case}"
            );
            assert!(source.exists(), "来源目录不能被删除：{case}");
            assert!(fs::read_dir(fixture.paths.staging())
                .unwrap()
                .next()
                .is_none());
        }
    }

    #[test]
    fn source_root_symlink_and_missing_skill_md_are_rejected() {
        let fixture = Fixture::new();
        let source = fixture.source("real");
        let alias = fixture.root.join("alias");
        symlink(&source, &alias).unwrap();
        assert!(prepare_skill_import(&fixture.paths, &alias).is_err());
        assert!(prepare_skill_import(&fixture.paths, &source).is_err());
    }

    #[test]
    fn oversized_and_unreadable_content_is_rejected_without_partial_copies() {
        let fixture = Fixture::new();
        let oversized = fixture.source("oversized");
        write_valid_skill(&oversized, "oversized-skill");
        let large = fs::File::create(oversized.join("large.bin")).unwrap();
        large.set_len(8 * 1024 * 1024 + 1).unwrap();
        assert!(prepare_skill_import(&fixture.paths, &oversized).is_err());

        let unreadable = fixture.source("unreadable");
        write_valid_skill(&unreadable, "unreadable-skill");
        let unreadable_file = unreadable.join("asset.txt");
        fs::write(&unreadable_file, "private").unwrap();
        fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o000)).unwrap();
        let result = prepare_skill_import(&fixture.paths, &unreadable);
        fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(result.is_err());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn hard_linked_files_are_rejected_without_modifying_the_source() {
        let fixture = Fixture::new();
        let source = fixture.source("hard-link");
        write_valid_skill(&source, "hard-link-skill");
        fs::write(source.join("asset.txt"), "same inode").unwrap();
        fs::hard_link(source.join("asset.txt"), source.join("asset-alias.txt")).unwrap();

        assert!(prepare_skill_import(&fixture.paths, &source).is_err());
        assert_eq!(fs::read(source.join("asset.txt")).unwrap(), b"same inode");
        assert_eq!(
            fs::read(source.join("asset-alias.txt")).unwrap(),
            b"same inode"
        );
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn root_replacement_and_executable_mode_changes_are_detected_by_stable_hash() {
        let fixture = Fixture::new();
        let source = fixture.source("identity");
        write_valid_skill(&source, "identity-skill");
        let script = source.join("scripts/run.sh");
        let non_executable_hash = digest_tree(&source, None).unwrap().hash;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let executable_hash = digest_tree(&source, None).unwrap().hash;
        assert_ne!(non_executable_hash, executable_hash);

        let (canonical, identity) = canonical_source_directory(&source).unwrap();
        let moved = fixture.root.join("identity-original");
        fs::rename(&source, &moved).unwrap();
        fs::create_dir(&source).unwrap();
        write_valid_skill(&source, "replacement-skill");
        assert!(digest_tree_with_root_identity(&canonical, None, Some(identity)).is_err());
        assert!(moved.join("SKILL.md").is_file());
    }

    #[test]
    fn changed_quarantine_is_never_recursively_deleted() {
        let fixture = Fixture::new();
        let source = fixture.source("quarantine");
        write_valid_skill(&source, "quarantine-skill");
        let mut prepared = prepare_skill_import(&fixture.paths, &source).unwrap();
        finalize_skill_import(&fixture.paths, &mut prepared).unwrap();
        let quarantine = quarantine_central_skill(
            &fixture.paths,
            &prepared.id,
            &prepared.name,
            &prepared.central_path,
            &prepared.content_hash,
        )
        .unwrap()
        .unwrap();
        fs::write(quarantine.join("unknown.txt"), "external change").unwrap();

        assert!(
            delete_quarantined_skill(&fixture.paths, &quarantine, &prepared.content_hash).is_err()
        );
        assert!(quarantine.join("unknown.txt").is_file());
        assert!(!std::path::Path::new(&prepared.central_path).exists());
    }
    #[test]
    fn failed_import_cleanup_preserves_replaced_or_changed_operation_directories() {
        for replace in [false, true] {
            let fixture = Fixture::new();
            let source = fixture.source("source");
            write_valid_skill(&source, "one");
            let mut prepared = prepare_skill_import(&fixture.paths, &source).unwrap();
            finalize_skill_import(&fixture.paths, &mut prepared).unwrap();
            if replace {
                fs::rename(
                    &prepared.central_path,
                    fixture.root.join("preserved-original"),
                )
                .unwrap();
                fs::create_dir(&prepared.central_path).unwrap();
            }
            let sentinel = std::path::Path::new(&prepared.central_path).join("unknown.txt");
            fs::write(&sentinel, "preserve me").unwrap();
            assert!(super::cleanup_failed_import(&fixture.paths, &prepared).is_err());
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), "preserve me");
        }
    }
    #[test]
    fn exclusive_finalize_rename_never_replaces_an_existing_directory() {
        let fixture = Fixture::new();
        let source = fixture.source("staged");
        let destination = fixture.source("existing");
        assert!(super::rename_import_exclusively(&source, &destination).is_err());
        assert!(source.is_dir());
        assert!(destination.is_dir());
    }
}
