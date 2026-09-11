/// journal 采用追加写：每个阶段把完整 `RunJournal` 压成一行 JSON 追加到
/// `<run_id>.json` 末尾并 fsync 一次；读取方取最后一条完整行。以前每个阶段都
/// 走临时文件 + rename + 目录 fsync（3 次 fsync），单目标一次 apply 要写十几次。
/// 崩溃截断只会损坏最后一行，前一条完整状态仍可恢复；历史的单对象 pretty JSON
/// 由 `read_journal` 兼容解析。
fn persist_journal(paths: &AppPaths, journal: &RunJournal) -> Result<(), AppError> {
    tracing::info!(
        run_id = %journal.run_id,
        operation = %journal.operation,
        phase = %journal.phase,
        targets = journal.targets.len(),
        "journal phase"
    );
    let journal_path = paths.journals().join(format!("{}.json", journal.run_id));
    validate_allowed_path(&journal_path, paths.journals(), false)?;
    let mut line = serde_json::to_vec(journal).map_err(|error| {
        AppError::atomic_write(&journal_path.to_string_lossy(), "serialize_journal")
            .with_source(error)
    })?;
    line.push(b'\n');
    let created = match fs::symlink_metadata(&journal_path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => false,
        Ok(_) => {
            return Err(AppError::conflict(
                "journal",
                "journal 路径被非普通文件占用",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => {
            return Err(
                AppError::atomic_write(&journal_path.to_string_lossy(), "lstat_journal")
                    .with_source(error),
            );
        }
    };
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(PRIVATE_FILE_MODE)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&journal_path)
        .map_err(|error| {
            AppError::atomic_write(&journal_path.to_string_lossy(), "open_journal")
                .with_source(error)
        })?;
    // 旧格式（整文件单对象、无尾换行）上追加时先补一个换行，避免新行粘在 `}` 后面。
    if !created && !journal_ends_with_newline(&journal_path)? {
        line.insert(0, b'\n');
    }
    file.write_all(&line)
        .and_then(|_| file.flush())
        .map_err(|error| {
            AppError::atomic_write(&journal_path.to_string_lossy(), "append_journal")
                .with_source(error)
        })?;
    fsync_file(&file, &journal_path, "sync_journal")?;
    if created {
        ensure_private_file(&journal_path)?;
        sync_directory(paths.journals())?;
    }
    Ok(())
}

fn journal_ends_with_newline(journal_path: &Path) -> Result<bool, AppError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(journal_path)
        .map_err(|error| {
            AppError::atomic_write(&journal_path.to_string_lossy(), "open_journal")
                .with_source(error)
        })?;
    let length = file
        .metadata()
        .map_err(|error| {
            AppError::atomic_write(&journal_path.to_string_lossy(), "stat_journal")
                .with_source(error)
        })?
        .len();
    if length == 0 {
        return Ok(true);
    }
    file.seek(SeekFrom::End(-1)).map_err(|error| {
        AppError::atomic_write(&journal_path.to_string_lossy(), "seek_journal").with_source(error)
    })?;
    let mut last = [0_u8; 1];
    file.read_exact(&mut last).map_err(|error| {
        AppError::atomic_write(&journal_path.to_string_lossy(), "read_journal").with_source(error)
    })?;
    Ok(last[0] == b'\n')
}

/// 解析 journal：优先取最后一条完整的单行 JSON（追加格式），否则按整文件单对象
/// （旧格式）解析。两种格式的字段完全相同。
fn parse_journal(bytes: &[u8]) -> Option<RunJournal> {
    for line in bytes.rsplit(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Ok(journal) = serde_json::from_slice::<RunJournal>(line) {
            return Some(journal);
        }
    }
    serde_json::from_slice::<RunJournal>(bytes).ok()
}

fn read_journal(journal_path: &Path) -> Result<Option<RunJournal>, AppError> {
    let bytes = match fs::read(journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(
                AppError::permission(&journal_path.to_string_lossy(), "read_journal")
                    .with_source(error),
            );
        }
    };
    parse_journal(&bytes)
        .map(Some)
        .ok_or_else(|| AppError::parse(&journal_path.to_string_lossy(), "journal"))
}
