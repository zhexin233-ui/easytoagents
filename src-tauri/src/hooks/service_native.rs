/// 统一事件 + matcher 到各工具原生事件映射；同一 `(event, matcher)` 的多条
/// hook 合并进同一 matcher 组（Cursor 为扁平数组，matcher 属于条目），
/// 事件键、组序与组内条目按确定性顺序排列，保证幂等渲染。
fn build_native_events(tool: Tool, records: &[HookRecord]) -> Result<Value, AppError> {
    if tool == Tool::Cursor {
        type NativeHookEntries = BTreeMap<String, Vec<(String, Value)>>;
        let mut events: NativeHookEntries = BTreeMap::new();
        for record in records {
            hook_event_supported(tool, record.event)?;
            let matcher = record.matcher.clone().unwrap_or_default();
            events
                .entry(record.event.native_key(tool).to_owned())
                .or_default()
                .push((matcher, native_hook_entry(tool, record)?));
        }
        let mut result = Map::new();
        for (native_event, mut entries) in events {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            result.insert(
                native_event,
                Value::Array(entries.into_iter().map(|(_, entry)| entry).collect()),
            );
        }
        return Ok(Value::Object(result));
    }
    type NativeHookGroups = BTreeMap<String, BTreeMap<String, Vec<(String, Value)>>>;
    let mut events: NativeHookGroups = BTreeMap::new();
    for record in records {
        hook_event_supported(tool, record.event)?;
        let matcher = record.matcher.clone().unwrap_or_default();
        events
            .entry(record.event.native_key(tool).to_owned())
            .or_default()
            .entry(matcher)
            .or_default()
            .push((record.name.to_lowercase(), native_hook_entry(tool, record)?));
    }
    let mut result = Map::new();
    for (native_event, groups) in events {
        let mut group_list = Vec::new();
        for (matcher, mut entries) in groups {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut group = Map::new();
            if !matcher.is_empty() {
                group.insert("matcher".to_owned(), Value::String(matcher));
            }
            group.insert(
                "hooks".to_owned(),
                Value::Array(entries.into_iter().map(|(_, entry)| entry).collect()),
            );
            group_list.push(Value::Object(group));
        }
        result.insert(native_event, Value::Array(group_list));
    }
    Ok(Value::Object(result))
}

/// 单条中央 Hook 的原生条目投影（与原生文件中的条目同形，用于条目级哈希）。
fn native_hook_entry(tool: Tool, record: &HookRecord) -> Result<Value, AppError> {
    let mut entry = Map::new();
    if tool != Tool::Cursor {
        entry.insert("type".to_owned(), Value::String("command".to_owned()));
    }
    entry.insert("command".to_owned(), Value::String(record.command.clone()));
    if let Some(timeout) = record.timeout_seconds {
        entry.insert("timeout".to_owned(), Value::Number(timeout.into()));
    }
    if tool == Tool::Cursor {
        if let Some(matcher) = record.matcher.as_deref().filter(|value| !value.is_empty()) {
            entry.insert("matcher".to_owned(), Value::String(matcher.to_owned()));
        }
    }
    Ok(Value::Object(entry))
}

/// per-hook managed item 的确定性外部键：`<Event>|<身份哈希前 16 位>|<matcher>`。
/// matcher 含正则元字符（可能包含 `|`），因此固定放在末段，解析用
/// `splitn(3, '|')`；身份哈希覆盖 name/matcher/command/timeout，确保
/// preflight 的 (target, external_key) 唯一性约束不碰撞。
fn hook_external_key(record: &HookRecord) -> String {
    let identity = hash_json(&json!({
        "name": record.name,
        "matcher": record.matcher,
        "command": record.command,
        "timeout": record.timeout_seconds,
    }));
    format!(
        "{}|{}|{}",
        record.event.as_str(),
        &identity[..identity.len().min(16)],
        record.matcher.as_deref().unwrap_or_default()
    )
}

/// 校验受管条目基线：在全部原生条目内容哈希中寻找 item 的 last_applied
/// hash；找不到即漂移。数组无名称键，因此以内容哈希匹配替代 MCP 的名称匹配。
pub(crate) fn verify_hook_item_baselines(
    scan: TargetScan,
    tool: Tool,
    existing: &[ManagedHookItemRecord],
) -> (TargetScan, Vec<String>) {
    if existing.is_empty() {
        return (scan, Vec::new());
    }
    let events_path = events_root(tool);
    let mismatched = match &scan {
        TargetScan::Observed(observed) => {
            let hashes = native_entry_hashes(observed, events_path);
            existing
                .iter()
                .filter(|item| !hashes.contains(item.last_applied_item_hash.as_str()))
                .map(|item| item.external_key.clone())
                .collect::<Vec<_>>()
        }
        TargetScan::Missing => existing
            .iter()
            .map(|item| item.external_key.clone())
            .collect(),
        _ => Vec::new(),
    };
    let matches = match &scan {
        TargetScan::Observed(_) => mismatched.is_empty(),
        TargetScan::Missing => false,
        _ => return (scan, Vec::new()),
    };
    if matches {
        (scan, Vec::new())
    } else {
        (TargetScan::ManagedItemBaselineMismatch, mismatched)
    }
}

fn build_managed_item_changes(
    tool: Tool,
    desired: &[HookRecord],
    existing: &[ManagedHookItemRecord],
) -> Result<(Vec<ManagedItemApply>, Vec<String>), AppError> {
    let mut by_resource = BTreeMap::new();
    let mut by_external_key = BTreeSet::new();
    for item in existing {
        if by_resource
            .insert(item.resource_id.as_str(), item)
            .is_some()
            || !by_external_key.insert(item.external_key.as_str())
        {
            return Err(AppError::conflict(
                "managedItems",
                "同一目标存在重复的 Hook managed item 基线",
            ));
        }
    }
    let mut used = BTreeSet::new();
    let mut updates = Vec::new();
    for record in desired {
        let native = native_hook_entry(tool, record)?;
        let external_key = hook_external_key(record);
        let existing_item = by_resource.get(record.id.as_str()).copied();
        let id = existing_item
            .map(|item| item.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        used.insert(id.clone());
        updates.push(ManagedItemApply {
            id,
            resource_kind: ArtifactKind::Hook,
            resource_id: record.id.clone(),
            external_key,
            last_applied_item_hash: hash_json(&native),
        });
    }
    let removals = existing
        .iter()
        .filter(|item| !used.contains(&item.id))
        .map(|item| item.id.clone())
        .collect();
    Ok((updates, removals))
}

fn ensure_hook_target(
    database: &mut Database,
    descriptor: &TargetDescriptor,
    project: Option<&McpProjectRecord>,
) -> Result<ManagedTargetBaseline, AppError> {
    let target_path = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::not_found("hookTarget", descriptor.tool.as_str()))?;
    let database_path = database.path().to_string_lossy().into_owned();
    let project_id = project.map(|project| project.id.as_str());
    let existing = find_hook_target_baseline(database, descriptor, project_id)?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    database
        .connection_mut()
        .execute(
            "INSERT INTO managed_targets(
                id, tool, artifact_kind, scope, project_id, target_path
             ) VALUES (?1, ?2, 'hook', ?3, ?4, ?5)",
            params![
                id,
                descriptor.tool.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
        )
        .map_err(|error| {
            AppError::database(&database_path, "insert_hook_managed_target").with_source(error)
        })?;
    load_managed_target_baseline(database, &id)
}

pub(super) fn find_hook_target_baseline(
    database: &Database,
    descriptor: &TargetDescriptor,
    project_id: Option<&str>,
) -> Result<Option<ManagedTargetBaseline>, AppError> {
    let Some(target_path) = descriptor.path.as_deref() else {
        return Ok(None);
    };
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, row_version, baseline_full_hash, baseline_managed_hash
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = 'hook' AND scope = ?2
               AND ifnull(project_id, '') = ifnull(?3, '') AND target_path = ?4",
            params![
                descriptor.tool.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
            |row| {
                Ok(ManagedTargetBaseline {
                    target_id: row.get(0)?,
                    target_row_version: row.get(1)?,
                    full_hash: row.get(2)?,
                    managed_hash: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database_path, "find_hook_managed_target").with_source(error)
        })
}

// ---------------------------------------------------------------------------
// DTO 辅助
// ---------------------------------------------------------------------------

fn hook_dto(database: &Database, record: &HookRecord) -> Result<HookDto, AppError> {
    hook_dto_with_assignments(
        record,
        repository::global_assignments_for_hook(database, &record.id)?,
    )
}

fn hook_dto_with_assignments(
    record: &HookRecord,
    assignments: Vec<(Tool, HookEvent)>,
) -> Result<HookDto, AppError> {
    let global_assignments = assignments
        .into_iter()
        .map(|(tool, event)| super::HookGlobalAssignmentDto { tool, event })
        .collect();
    Ok(HookDto {
        id: record.id.clone(),
        name: record.name.clone(),
        event: record.event,
        matcher: record.matcher.clone(),
        command: record.command.clone(),
        timeout_seconds: record.timeout_seconds,
        enabled: record.enabled,
        script_name: record.script_name.clone(),
        global_assignments,
        row_version: crate::sync::managed_record_row_version::<crate::sync::managed::HookManagedArtifact>(
            record,
        )?,
    })
}

pub(crate) fn validated_definition(
    name: &str,
    event: HookEvent,
    matcher: Option<&str>,
    command: &str,
    timeout_seconds: Option<i32>,
    enabled: bool,
) -> Result<repository::ValidatedHookDefinition, AppError> {
    let (name, matcher, command, timeout_seconds) =
        validate_hook_definition(name, event, matcher, command, timeout_seconds)?;
    Ok(repository::ValidatedHookDefinition {
        name,
        event,
        matcher,
        command,
        timeout_seconds,
        enabled,
        script_name: None,
        script_hash: None,
    })
}

fn canonical_project(path: &str) -> Result<ProjectRoot, AppError> {
    let canonical = canonicalize_project_root(Path::new(path))?;
    if canonical.as_str() != path {
        return Err(AppError::conflict(
            "projectRoot",
            "登记项目根与当前 canonical 路径不一致",
        ));
    }
    Ok(canonical)
}

fn load_target_status(
    database: &Database,
    tool: Tool,
    project_id: Option<&str>,
    target_path: &str,
) -> Result<Option<SyncStatus>, AppError> {
    let path = database.path().to_string_lossy();
    let status = database
        .connection()
        .query_row(
            "SELECT last_status FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = 'hook'
               AND ifnull(project_id, '') = ifnull(?2, '') AND target_path = ?3",
            params![tool.as_str(), project_id, target_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database(&path, "load_hook_target_status").with_source(error))?;
    status.map(parse_sync_status).transpose()
}

// ---------------------------------------------------------------------------
// 脚本接管（导入与手动创建共用）
// ---------------------------------------------------------------------------

/// 可接管脚本的大小上限。
const MAX_HOOK_SCRIPT_BYTES: u64 = 512 * 1024;
/// 首 token 命中解释器才尝试脚本接管；其余命令一律 inline。
pub(crate) const HOOK_INTERPRETERS: &[&str] = &[
    "bash", "sh", "zsh", "python", "python3", "perl", "ruby", "node",
];

/// 计算脚本文件内容 SHA-256（导入去重用）；文件安全性由调用方先校验。
pub(crate) fn script_content_hash(path: &Path) -> Result<String, AppError> {
    let bytes = fs::read(path).map_err(|error| {
        AppError::invalid_input("scriptSourcePath", "脚本不可读取").with_source(error)
    })?;
    Ok(hash_bytes(&bytes))
}

struct AdoptedScript {
    file_name: String,
    hash: String,
    original_path: PathBuf,
    central_path: PathBuf,
}

/// 极简 shell 分词：按空白切分，尊重单双引号。反引号、`$"` 等复杂语法
/// 不支持——引用该语法的命令不会被脚本接管（保守不猜测）。
pub(crate) fn split_shell_words(command: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut has_word = false;
    let mut quote: Option<char> = None;
    for character in command.chars() {
        match quote {
            Some(quoted) if character == quoted => quote = None,
            Some(_) => current.push(character),
            None if character == '\'' || character == '"' => {
                quote = Some(character);
                has_word = true;
            }
            None if character.is_whitespace() => {
                if has_word {
                    words.push(std::mem::take(&mut current));
                    has_word = false;
                }
            }
            None => {
                current.push(character);
                has_word = true;
            }
        }
    }
    // 未闭合的引号视为复杂语法，拒绝接管。
    if quote.is_some() {
        return None;
    }
    if has_word {
        words.push(current);
    }
    Some(words)
}

/// 解析命令中的可接管脚本：返回（token 下标，展开后的绝对路径）。
/// 仅当首 token 是解释器且某个 token 命中既有普通文件（支持 `~/` 与
/// `$HOME/` 展开）时返回；`${CLAUDE_PROJECT_DIR}` 等项目级变量路径与
/// 相对路径无法安全解析，不接管。
pub(crate) fn resolve_script_adoption(command: &str, home: &Path) -> Option<(usize, PathBuf)> {
    let words = split_shell_words(command)?;
    // `/usr/bin/env <解释器> <脚本>` 形式的脚本扫描起点在解释器之后。
    let scan_start = interpreter_scan_start(&words)?;
    for (index, word) in words.iter().enumerate().skip(scan_start) {
        if word.starts_with('-') {
            continue;
        }
        if let Some(path) = expand_script_path(word, home) {
            return Some((index, path));
        }
    }
    None
}

/// 返回脚本 token 的扫描起点（解释器 token 的下一个下标）。
/// 支持 `bash …` 与 `/usr/bin/env [-flags] python3 …` 两种形式；
/// 首个 token（或 env 后首个非 flag token）不是已知解释器则返回 None。
pub(crate) fn interpreter_scan_start(words: &[String]) -> Option<usize> {
    let first = words.first()?;
    let basename = first.rsplit(['/', '\\']).next()?;
    if basename == "env" {
        // env 间接层：跳过其 flag（如 -S、-i），下一个 token 必须是已知解释器。
        let mut index = 1;
        while words.get(index).is_some_and(|word| word.starts_with('-')) {
            index += 1;
        }
        let interpreter = words.get(index)?.rsplit(['/', '\\']).next()?;
        if !is_interpreter_basename(interpreter) {
            return None;
        }
        Some(index + 1)
    } else if is_interpreter_basename(basename) {
        Some(1)
    } else {
        None
    }
}

/// 解释器名匹配：固定名单 + `python*` 系列（python3.11 等次版本号）。
fn is_interpreter_basename(basename: &str) -> bool {
    HOOK_INTERPRETERS.contains(&basename) || basename.starts_with("python")
}

/// 展开 `~/` 与 `$HOME/` 前缀并校验目标是既有普通文件（拒绝链接/特殊文件）。
fn expand_script_path(word: &str, home: &Path) -> Option<PathBuf> {
    let expanded = if let Some(rest) = word.strip_prefix("~/") {
        home.join(rest)
    } else if let Some(rest) = word.strip_prefix("$HOME/") {
        home.join(rest)
    } else {
        PathBuf::from(word)
    };
    if !expanded.is_absolute() {
        return None;
    }
    let metadata = fs::symlink_metadata(&expanded).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    metadata
        .len()
        .le(&MAX_HOOK_SCRIPT_BYTES)
        .then_some(expanded)
}

/// 把脚本本体复制进中央目录（0600），返回接管结果。失败时清理半成品。
fn adopt_script(paths: &AppPaths, hook_id: &str, source: &str) -> Result<AdoptedScript, AppError> {
    let source_path = PathBuf::from(source);
    let metadata = fs::symlink_metadata(&source_path).map_err(|error| {
        AppError::invalid_input("scriptSourcePath", "脚本不存在或不可读取").with_source(error)
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::invalid_input(
            "scriptSourcePath",
            "脚本必须是普通文件，不能是链接或特殊文件",
        ));
    }
    if metadata.len() > MAX_HOOK_SCRIPT_BYTES {
        return Err(AppError::invalid_input(
            "scriptSourcePath",
            "脚本不能超过 512 KiB",
        ));
    }
    let bytes = fs::read(&source_path).map_err(|error| {
        AppError::invalid_input("scriptSourcePath", "脚本不可读取").with_source(error)
    })?;
    let file_name = sanitize_script_file_name(&source_path);
    let directory = paths.central_hooks().join(hook_id);
    ensure_private_directory(&directory)?;
    let central_path = directory.join(&file_name);
    let mut file = create_private_file(&central_path).map_err(|error| {
        AppError::invalid_input("scriptSourcePath", "中央脚本目录不可写").with_source(error)
    })?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|error| {
        AppError::invalid_input("scriptSourcePath", "中央脚本写入失败").with_source(error)
    })?;
    drop(file);
    Ok(AdoptedScript {
        hash: hash_bytes(&bytes),
        file_name,
        original_path: source_path,
        central_path,
    })
}

/// 中央脚本文件名：保留 basename 中的安全字符，其余替换为 `_`。
fn sanitize_script_file_name(source: &Path) -> String {
    let basename = source
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("hook.sh");
    let sanitized: String = basename
        .chars()
        .take(100)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(|c| c == '.' || c == '_');
    if trimmed.is_empty() {
        "hook.sh".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// 把命令分词结果中命中原脚本的 token 替换为带引号的中央路径。
fn rewrite_command_with_script(
    _validated_command: &str,
    original_command: &str,
    original_path: &Path,
    central_path: &Path,
) -> Result<String, AppError> {
    let mut words = split_shell_words(original_command).ok_or_else(|| {
        AppError::conflict("command", "命令包含不支持的 shell 语法，无法接管脚本")
    })?;
    let target = original_path.to_string_lossy();
    let position = words
        .iter()
        .position(|word| word == &target)
        .ok_or_else(|| AppError::stale_preview("script", "脚本路径与检测时不一致，请重新检测"))?;
    words[position] = format!("\"{}\"", central_path.to_string_lossy());
    let rewritten = words.join(" ");
    if rewritten.len() > 4000 {
        return Err(AppError::invalid_input(
            "command",
            "接管后的命令超过 4000 字符",
        ));
    }
    Ok(rewritten)
}

fn parse_sync_status(value: String) -> Result<SyncStatus, AppError> {
    crate::domain::SyncStatus::from_stable_str(&value)
        .ok_or_else(|| AppError::database("managed_targets", "解析 last_status 失败"))
}
