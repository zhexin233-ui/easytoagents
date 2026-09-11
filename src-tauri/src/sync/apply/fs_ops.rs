struct CreatedDirectory {
    parent: File,
    name: CString,
}

fn create_private_directory_nofollow(
    path: &Path,
    allowed_root: &Path,
) -> Result<CreatedDirectory, AppError> {
    validate_allowed_path(path, allowed_root, false)?;
    let relative = path.strip_prefix(allowed_root).map_err(|error| {
        AppError::invalid_input("targetPath", "Skills 目录位于允许根之外").with_source(error)
    })?;
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
        let app_error = match error.kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), "mkdirat_skill_target")
            }
            io::ErrorKind::AlreadyExists => {
                AppError::stale_preview("persisted", &path.to_string_lossy())
            }
            _ => AppError::atomic_write(&path.to_string_lossy(), "create_skill_target_directory"),
        };
        return Err(app_error.with_source(error));
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
    directory.sync_all().map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), "sync_skill_directory").with_source(error)
    })?;
    let parent = parent_of(path)?;
    created.parent.sync_all().map_err(|error| {
        AppError::atomic_write(&parent.to_string_lossy(), "sync_skill_directory_parent")
            .with_source(error)
    })
}

fn c_path_segment(segment: &OsStr, field: &'static str) -> Result<CString, AppError> {
    if segment.as_bytes().contains(&b'/') {
        return Err(AppError::invalid_input(field, "路径段不能包含分隔符"));
    }
    CString::new(segment.as_bytes())
        .map_err(|error| AppError::invalid_input(field, "路径段不能包含 NUL").with_source(error))
}

fn open_directory_nofollow(path: &Path, operation: &'static str) -> Result<File, AppError> {
    let path_c = CString::new(path.as_os_str().as_bytes()).map_err(|error| {
        AppError::invalid_input("allowedRoot", "路径不能包含 NUL").with_source(error)
    })?;
    // SAFETY: path_c 是合法 C 路径；成功返回的 fd 立即交给 File 管理。
    let descriptor = unsafe {
        libc::open(
            path_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        let error = io::Error::last_os_error();
        let app_error = match error.kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), operation)
            }
            _ => AppError::conflict("targetPath", "Skills 目录祖先无法安全打开"),
        };
        return Err(app_error.with_source(error));
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
        let error = io::Error::last_os_error();
        let app_error = match error.kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&display_path.to_string_lossy(), "openat_skill_parent")
            }
            _ => AppError::conflict("targetPath", "Skills 目录祖先已变化、缺失或变为链接"),
        };
        return Err(app_error.with_source(error));
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

/// 已验证目标的父目录；根路径或空路径在这里意味着不变量被打破，返回内部错误而不是 panic。
fn parent_of(path: &Path) -> Result<&Path, AppError> {
    path.parent()
        .ok_or_else(|| AppError::internal("目标路径缺少父目录"))
}

fn validate_allowed_path(
    path: &Path,
    allowed_root: &Path,
    allow_root_target: bool,
) -> Result<(), AppError> {
    validate_normal_absolute(path, "targetPath")?;
    validate_normal_absolute(allowed_root, "allowedRoot")?;
    let root_metadata = fs::symlink_metadata(allowed_root).map_err(|error| {
        let app_error = match error.kind() {
            io::ErrorKind::NotFound => {
                AppError::not_found("allowedRoot", &allowed_root.to_string_lossy())
            }
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&allowed_root.to_string_lossy(), "lstat_allowed_root")
            }
            _ => AppError::invalid_input("allowedRoot", "写入根无法安全读取"),
        };
        app_error.with_source(error)
    })?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(AppError::conflict(
            "allowedRoot",
            "写入根必须是无链接的真实目录",
        ));
    }
    let canonical_root = fs::canonicalize(allowed_root).map_err(|error| {
        AppError::permission(&allowed_root.to_string_lossy(), "canonicalize_allowed_root")
            .with_source(error)
    })?;
    if canonical_root != allowed_root {
        return Err(AppError::conflict(
            "allowedRoot",
            "写入根不是 canonical 路径",
        ));
    }
    let relative = path.strip_prefix(allowed_root).map_err(|error| {
        AppError::invalid_input("targetPath", "目标位于允许写入根之外").with_source(error)
    })?;
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
        let parent_relative = parent.strip_prefix(allowed_root).map_err(|error| {
            AppError::invalid_input("targetPath", "目标父目录越界").with_source(error)
        })?;
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
                    )
                    .with_source(error));
                }
                Err(error) => {
                    return Err(
                        AppError::invalid_input("targetPath", "目标父目录无法安全读取")
                            .with_source(error),
                    );
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
            return Err(
                AppError::permission(&path.to_string_lossy(), "lstat_target").with_source(error),
            );
        }
        Err(error) => {
            return Err(
                AppError::atomic_write(&path.to_string_lossy(), "lstat_target").with_source(error),
            );
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let link_target = fs::read_link(path).map_err(|error| {
            AppError::atomic_write(&path.to_string_lossy(), "read_link").with_source(error)
        })?;
        return Ok(PathState::Symlink { link_target });
    }
    if metadata.is_file() {
        let bytes = read_target_bytes(path).map_err(|error| match error.kind() {
            io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), "read_target").with_source(error)
            }
            _ => AppError::atomic_write(&path.to_string_lossy(), "read_target").with_source(error),
        })?;
        return Ok(PathState::File {
            hash: hash_bytes(&bytes),
            bytes,
            mode: metadata.permissions().mode() & 0o7777,
            stat: StatSignature::from_metadata(&metadata),
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

/// 先用 lstat 签名与已知状态比对；一致则复用已读内容，否则回退到完整读取比对。
/// 语义与 `verify_expected_path_state` 完全相同，只是省掉一次全量读取。
fn verify_known_path_state(
    path: &Path,
    known: Option<&PathState>,
    expected: ExpectedPathFingerprint<'_>,
) -> Result<PathState, AppError> {
    match known {
        Some(state)
            if cheap_state_matches(state, path) && state.fingerprint() == expected.fingerprint =>
        {
            Ok(state.clone())
        }
        _ => verify_expected_path_state(path, expected),
    }
}

