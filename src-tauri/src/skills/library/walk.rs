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
        let name = entry.into_string().map_err(|_| {
            AppError::invalid_input("sourcePath", "Skill 路径必须是 UTF-8")
                .with_source("Skill 文件名包含无效 UTF-8")
        })?;
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
                limits.skill_md = Some(String::from_utf8(bytes.clone()).map_err(|error| {
                    AppError::invalid_input("SKILL.md", "Skill 内容必须是 UTF-8").with_source(error)
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
                symlink(&raw_target, destination).map_err(|error| {
                    AppError::atomic_write(&destination.to_string_lossy(), "copy_skill_symlink")
                        .with_source(error)
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
    let capacity = usize::try_from(metadata.len()).map_err(|error| {
        AppError::invalid_input("sourcePath", "Skill 文件大小超出平台限制").with_source(error)
    })?;
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
            .map_err(|error| {
                AppError::atomic_write(&destination.to_string_lossy(), "create_staged_skill_file")
                    .with_source(error)
            })?;
        output
            .set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|error| {
                AppError::permission(&destination.to_string_lossy(), "chmod_staged_skill_file")
                    .with_source(error)
            })?;
        output.write_all(&bytes).map_err(|error| {
            AppError::atomic_write(&destination.to_string_lossy(), "write_staged_skill_file")
                .with_source(error)
        })?;
        output.flush().map_err(|error| {
            AppError::atomic_write(&destination.to_string_lossy(), "flush_staged_skill_file")
                .with_source(error)
        })?;
        output.sync_all().map_err(|error| {
            AppError::atomic_write(&destination.to_string_lossy(), "sync_staged_skill_file")
                .with_source(error)
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
                open_directory_at_nofollow(&directory, segment, &target_path).map_err(|error| {
                    AppError::invalid_input("sourcePath", "Skill 符号链接包含循环、断链或链接目录")
                        .with_source(error)
                })?;
        } else {
            let metadata = lstat_at(&directory, segment, &target_path).map_err(|error| {
                AppError::invalid_input("sourcePath", "Skill 符号链接目标无法读取")
                    .with_source(error)
            })?;
            if !metadata.is_file() || metadata.is_symlink() || metadata.nlink() != 1 {
                return Err(AppError::invalid_input(
                    "sourcePath",
                    "Skill 只允许指向目录内普通非硬链接文件的符号链接",
                ));
            }
            let opened =
                open_file_at_nofollow(&directory, segment, &target_path).map_err(|error| {
                    AppError::invalid_input("sourcePath", "Skill 符号链接目标无法安全打开")
                        .with_source(error)
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
        .map_err(|error| AppError::invalid_input(field, "路径不能包含 NUL").with_source(error))
}

fn c_name(name: &OsStr) -> Result<CString, AppError> {
    CString::new(name.as_bytes()).map_err(|error| {
        AppError::invalid_input("sourcePath", "Skill 路径名不能包含 NUL").with_source(error)
    })
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
    let length = usize::try_from(length).map_err(|error| {
        AppError::invalid_input("sourcePath", "Skill 链接目标长度无效").with_source(error)
    })?;
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
    let frontmatter: Value = serde_yaml_ng::from_str(&text[start..end]).map_err(|error| {
        AppError::invalid_input("SKILL.md", "SKILL.md frontmatter 不是合法 YAML").with_source(error)
    })?;
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
    String::from_utf8(bytes).map_err(|error| {
        AppError::invalid_input(field, "Skill 内容必须是 UTF-8").with_source(error)
    })
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
    fs::create_dir(path).map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), "create_skill_directory").with_source(error)
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        AppError::permission(&path.to_string_lossy(), "chmod_skill_directory").with_source(error)
    })?;
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
    fs::remove_dir_all(path).map_err(|error| {
        AppError::atomic_write(&path.to_string_lossy(), "remove_owned_skill_tree")
            .with_source(error)
    })?;
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
        .map_err(|error| {
            AppError::atomic_write(&path.to_string_lossy(), "sync_skill_directory")
                .with_source(error)
        })
}

fn path_text(path: &Path, field: &'static str) -> Result<String, AppError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::invalid_input(field, "路径必须是 UTF-8"))
}

fn map_read_error(error: io::Error, path: &Path, operation: &'static str) -> AppError {
    let app_error = match error.kind() {
        io::ErrorKind::NotFound => AppError::not_found("skillPath", &path.to_string_lossy()),
        io::ErrorKind::PermissionDenied => AppError::permission(&path.to_string_lossy(), operation),
        _ => AppError::invalid_input("sourcePath", "Skill 目录无法稳定读取"),
    };
    app_error.with_source(error)
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
