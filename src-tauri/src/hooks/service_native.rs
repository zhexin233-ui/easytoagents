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
    // 保留 observation 及其 hash。Apply 会用它们作为并发变更栅栏；不匹配列表
    // 仍作为诊断证据保留。
    if matches {
        (scan, Vec::new())
    } else {
        (scan, mismatched)
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
        affected_sync_scopes: None,
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
    if HOOK_INTERPRETERS.contains(&basename) {
        return true;
    }
    // 接受带版本号的 Python 启动器（如 python3.11），但不接受仅仅以
    // `python` 开头的任意二进制；否则脚本复制路径可能把未建模命令认成安全
    // 解释器。
    let Some(version) = basename.strip_prefix("python") else {
        return false;
    };
    !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
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

// ---------------------------------------------------------------------------
// 原生 Hook 采纳：严格映射、脚本 staging 与中央事务
// ---------------------------------------------------------------------------

const HOOK_NATIVE_MATCH_OR_IMPORT: &str = "MATCH_OR_IMPORT_REQUIRED";

/// 原生 Hook 的已验证表示。`native_entry` 保留原生条目原样，用于更新
/// managed item 的条目 hash；中央命令仍保留原生命令文本，保证采纳后下一次
///现场扫描与 baseline 都是 in-sync。脚本本体另外复制到中央私有目录留作中央
///资产，绝不因为采纳动作擅自改写原生目标。
#[derive(Debug, Clone)]
struct ParsedNativeHook {
    event: HookEvent,
    matcher: Option<String>,
    command: String,
    timeout_seconds: Option<i32>,
    native_entry: Value,
    script_source: Option<PathBuf>,
}

#[derive(Debug)]
struct StagedHookScript {
    source: PathBuf,
    hash: String,
    stage_path: PathBuf,
    final_path: PathBuf,
    /// 中央目录中旧资产的 CAS 证据。`None` 表示该 Hook 之前没有中央脚本，
    /// 因而目标路径必须不存在；不能把任意同名文件当成可覆盖资产。
    expected_final_hash: Option<String>,
    backup_path: Option<PathBuf>,
    installed: bool,
}

/// 采纳唯一可证明映射的原生 Hook 内容。
///
/// 该入口只更新已有中央 Hook、已有 managed item 和已有目标 baseline；不新建、
/// 删除或重绑中央记录，不改 assignment，也不调用 baseline-only readopt。调用方
/// 应先 claim 对应的 ExternalChangePlan；本函数以 target/item/Hook row version
/// 和 full/managed hash 再做一次 CAS 校验。
pub fn adopt_hook_native(
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    input: AdoptHookNativeInput,
) -> Result<AdoptHookNativeResultDto, AppError> {
    ensure_hooks_supported(input.tool)?;
    if input.target_path.trim().is_empty() {
        return Err(AppError::invalid_input(
            "targetPath",
            "Hook 采纳缺少目标路径",
        ));
    }
    let target_path = Path::new(&input.target_path);
    if !target_path.is_absolute()
        || target_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir
                    | std::path::Component::ParentDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(AppError::invalid_input(
            "targetPath",
            "Hook 采纳目标路径必须是绝对 canonical 路径",
        ));
    }
    let project = input
        .project_id
        .as_deref()
        .map(|id| mcp_repository::get_project(database, id))
        .transpose()?;
    let project_root = project
        .as_ref()
        .map(|project| canonical_project(&project.root_path))
        .transpose()?;
    let descriptor = hook_target_descriptor(environment, input.tool, project_root.as_ref())?;
    if descriptor.path.as_deref() != Some(input.target_path.as_str()) {
        return Err(AppError::invalid_input(
            "targetPath",
            "Hook 目标路径与当前工具 descriptor 不一致",
        ));
    }
    ensure_hook_adopt_descriptor(&descriptor, &input.target_path)?;
    let allowed_root = descriptor_allowed_root(&descriptor)?;
    if !target_path.starts_with(&allowed_root) {
        return Err(AppError::conflict(
            "targetPath",
            "Hook 目标路径超出 descriptor 安全边界",
        ));
    }

    let baseline = find_hook_target_baseline(
        database,
        &descriptor,
        project.as_ref().map(|project| project.id.as_str()),
    )?
    .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
    if baseline.target_id != input.target_id
        || safe_row_version(baseline.target_row_version)? != input.target_row_version
    {
        return Err(AppError::stale_preview(
            "adoptHookNative",
            &input.target_path,
        ));
    }
    if baseline.full_hash.is_none() != baseline.managed_hash.is_none()
        || baseline.full_hash.is_none()
    {
        return Err(AppError::conflict(
            "hookAdopt",
            "受管 Hook baseline 不完整，必须进入匹配或导入",
        ));
    }
    let expected_full_hash = input.observed_full_hash.as_deref().ok_or_else(|| {
        AppError::stale_preview("adoptHookNative", &input.target_path)
    })?;
    let expected_managed_hash = input.observed_managed_hash.as_deref().ok_or_else(|| {
        AppError::stale_preview("adoptHookNative", &input.target_path)
    })?;
    if !is_sha256(expected_full_hash) || !is_sha256(expected_managed_hash) {
        return Err(AppError::invalid_input(
            "observedHash",
            "Hook 采纳缺少有效的目标 hash",
        ));
    }

    let ownership = build_hook_ownership(input.tool);
    let observed = scan_hook_for_adoption(input.tool, &descriptor, &ownership)?;
    if observed.full_hash != expected_full_hash || observed.managed_hash != expected_managed_hash {
        return Err(AppError::stale_preview(
            "adoptHookNative",
            &input.target_path,
        ));
    }
    let parsed_rows = validate_native_hook_projection(&observed, input.tool, environment.home())?;

    let existing_items = repository::list_managed_hook_items(database, &baseline.target_id)?;
    let assigned_records = adopted_hook_assignments(
        database,
        input.tool,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    // 先检查调用方绑定的行版本，再解析 managed item 身份。这样中央记录在
    // 预览后发生变化时稳定返回 stale，而不会被过期 external key 误报为匹配失败。
    validate_hook_adoption_row_versions(
        &input.row_versions,
        project.as_ref(),
        &assigned_records,
        &existing_items,
        &input.target_id,
        input.target_row_version,
    )?;
    let pairs = match_native_hook_rows(&assigned_records, &existing_items, &parsed_rows)?;
    if pairs.is_empty() {
        return Err(AppError::conflict(
            "hookAdopt",
            HOOK_NATIVE_MATCH_OR_IMPORT,
        ));
    }

    let mut staged_scripts = Vec::new();
    let mut hook_updates = Vec::with_capacity(pairs.len());
    let mut item_updates = Vec::with_capacity(pairs.len());
    for (row_index, item_index) in &pairs {
        let item = &existing_items[*item_index];
        let record = assigned_records
            .iter()
            .find(|record| record.id == item.resource_id)
            .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
        let parsed = &parsed_rows[*row_index];
        let (script_name, script_hash) = if let Some(source) = parsed.script_source.as_ref() {
            let result = (|| {
                let source = validate_hook_script_source(
                    source,
                    paths,
                    environment,
                    &descriptor,
                    project_root.as_ref(),
                )?;
                let (bytes, hash) = read_stable_hook_script(&source)?;
                let file_name = record
                    .script_name
                    .as_deref()
                    .map(validate_hook_script_name)
                    .transpose()?
                    .unwrap_or_else(|| sanitize_script_file_name(&source));
                let staged = stage_hook_script(
                    paths,
                    &record.id,
                    &source,
                    &bytes,
                    &hash,
                    &file_name,
                    record.script_hash.as_deref(),
                )?;
                staged_scripts.push(staged);
                Ok((Some(file_name), Some(hash)))
            })();
            match result {
                Ok(value) => value,
                Err(error) => {
                    cleanup_uninstalled_hook_scripts(&mut staged_scripts);
                    return Err(error);
                }
            }
        } else {
            (None, None)
        };

        // assignment event 是该目标的生效事件。采纳不会擅自修改中央建议事件
        //（`hooks.event`）。
        let adopted_record = HookRecord {
            id: record.id.clone(),
            name: record.name.clone(),
            event: parsed.event,
            matcher: parsed.matcher.clone(),
            command: parsed.command.clone(),
            timeout_seconds: parsed.timeout_seconds,
            enabled: record.enabled,
            script_name: script_name.clone(),
            script_hash: script_hash.clone(),
            row_version: record.row_version,
        };
        hook_updates.push(repository::NativeHookAdoption {
            id: record.id.clone(),
            row_version: safe_row_version(record.row_version)?,
            event: parsed.event,
            matcher: parsed.matcher.clone(),
            command: parsed.command.clone(),
            timeout_seconds: parsed.timeout_seconds,
            script_name,
            script_hash,
        });
        item_updates.push(repository::NativeHookItemAdoption {
            id: item.id.clone(),
            target_id: baseline.target_id.clone(),
            row_version: safe_row_version(item.row_version)?,
            resource_id: record.id.clone(),
            external_key: hook_external_key(&adopted_record),
            last_applied_item_hash: hash_json(&parsed.native_entry),
        });
    }

    // 脚本本体不在 JSON target 的 full/managed hash 中；只改脚本字节时，
    // native projection 仍可能与 baseline 拥有相同 hash。因此除了投影 hash
    // 外，还要把中央 Hook 字段和脚本元数据的实际变化视为真正的受管变化。
    // 若两者都没有变化，该动作只能是非受管字段变化或重复动作，不能刷新
    // baseline 冒充采纳；即使 baseline 本身已不一致，也不得成为 baseline-only
    // readopt 的旁路。
    let central_hook_changed = hook_updates.iter().any(|update| {
        assigned_records
            .iter()
            .find(|record| record.id == update.id)
            .is_some_and(|record| {
                record.event != update.event
                    || record.matcher != update.matcher
                    || record.command != update.command
                    || record.timeout_seconds != update.timeout_seconds
                    || record.script_name != update.script_name
                    || record.script_hash != update.script_hash
            })
    });
    if !central_hook_changed {
        cleanup_uninstalled_hook_scripts(&mut staged_scripts);
        return Err(AppError::conflict(
            "hookAdopt",
            "没有可直接采纳的受管 Hook 变化，请进入匹配或导入流程",
        ));
    }

    if let Err(error) = verify_hook_script_sources(&staged_scripts) {
        cleanup_uninstalled_hook_scripts(&mut staged_scripts);
        return Err(error);
    }
    if let Err(error) = rescan_hook_for_adoption(
        input.tool,
        &descriptor,
        &ownership,
        expected_full_hash,
        expected_managed_hash,
        &input.target_path,
    ) {
        cleanup_uninstalled_hook_scripts(&mut staged_scripts);
        return Err(error);
    }
    if let Err(error) = verify_hook_script_sources(&staged_scripts) {
        cleanup_uninstalled_hook_scripts(&mut staged_scripts);
        return Err(error);
    }
    if let Err(error) = install_staged_hook_scripts(&mut staged_scripts) {
        return restore_hook_scripts_or_error(&mut staged_scripts, &input.target_path, error);
    }

    // 采纳不会重写原生目标。私有脚本安装后重新扫描，避免并发目标编辑被
    // 合并进中央事务所使用的 observation。
    let final_observed = match rescan_hook_for_adoption(
        input.tool,
        &descriptor,
        &ownership,
        expected_full_hash,
        expected_managed_hash,
        &input.target_path,
    ) {
        Ok(observed) => observed,
        Err(error) => {
            return restore_hook_scripts_or_error(&mut staged_scripts, &input.target_path, error)
        }
    };
    if let Err(error) = verify_hook_script_sources(&staged_scripts) {
        return restore_hook_scripts_or_error(&mut staged_scripts, &input.target_path, error);
    }
    let projection_json = match serde_json::to_string(&final_observed.managed_projection) {
        Ok(projection_json) => projection_json,
        Err(error) => {
            let error = AppError::invalid_input("managedBaseline", "Hook 接管基线无法序列化")
                .with_source(error);
            return restore_hook_scripts_or_error(&mut staged_scripts, &input.target_path, error);
        }
    };
    let target_adoption = repository::NativeHookTargetAdoption {
        target_id: input.target_id.clone(),
        target_row_version: input.target_row_version,
        target_path: input.target_path.clone(),
        tool: input.tool,
        scope: if project.is_some() { Scope::Project } else { Scope::Global },
        project_id: input.project_id.clone(),
        observed_full_hash: final_observed.full_hash.clone(),
        observed_managed_hash: final_observed.managed_hash.clone(),
        baseline_projection_json: projection_json,
        row_versions: input.row_versions.clone(),
    };
    // SQLite 的 IMMEDIATE 锁不能锁住目标文件。把最后一次 native scan 和
    // staging source 验证放进仓储事务的校验闭包，使 Hook/item/baseline 写入前、
    // 中、后的每个检查点都重新确认同一份磁盘 observation；目标若在事务期间
    // 被改动，仓储会回滚所有中央更新并由下方恢复脚本副本。
    let committed_full_hash = final_observed.full_hash.clone();
    let committed_managed_hash = final_observed.managed_hash.clone();
    let committed_projection = final_observed.managed_projection.clone();
    let committed_descriptor = descriptor.clone();
    let committed_ownership = ownership.clone();
    let committed_target_path = input.target_path.clone();
    let committed_tool = input.tool;
    let adoption_result = repository::adopt_native_hooks(
        database,
        &target_adoption,
        &hook_updates,
        &item_updates,
        || {
            verify_hook_script_sources(&staged_scripts)?;
            let observed = rescan_hook_for_adoption(
                committed_tool,
                &committed_descriptor,
                &committed_ownership,
                &committed_full_hash,
                &committed_managed_hash,
                &committed_target_path,
            )?;
            if observed.managed_projection != committed_projection {
                return Err(AppError::stale_preview(
                    "adoptHookNative",
                    &committed_target_path,
                ));
            }
            Ok(())
        },
    );
    if let Err(error) = adoption_result {
        return restore_hook_scripts_or_error(&mut staged_scripts, &input.target_path, error);
    }
    cleanup_committed_hook_scripts(&mut staged_scripts);
    let mut adopted = hook_updates.into_iter().map(|hook| hook.id).collect::<Vec<_>>();
    adopted.sort();

    Ok(AdoptHookNativeResultDto {
        tool: input.tool,
        project_id: input.project_id.clone(),
        adopted,
        affected_sync_scopes: Some(vec![match project {
            Some(project) => SyncScopeDto::project(
                ArtifactKind::Hook,
                input.tool,
                project.id,
            ),
            None => SyncScopeDto::global(ArtifactKind::Hook, input.tool),
        }]),
    })
}

fn ensure_hook_adopt_descriptor(
    descriptor: &TargetDescriptor,
    target_path: &str,
) -> Result<(), AppError> {
    if descriptor.capability.state != CapabilityState::Supported {
        return Err(AppError::invalid_input(
            "capability",
            "Hook 目标能力不支持原生采纳",
        ));
    }
    if descriptor.policy != PolicyState::Allowed {
        return Err(AppError::policy_blocked(
            descriptor.tool.as_str(),
            target_path,
            if descriptor.policy == PolicyState::Unknown {
                "CLAUDE_POLICY_UNKNOWN"
            } else {
                "CLAUDE_POLICY_BLOCKED"
            },
        ));
    }
    if matches!(
        descriptor.trust,
        TargetTrustState::Unknown | TargetTrustState::Untrusted
    ) {
        return Err(AppError::untrusted_project(
            descriptor.tool.as_str(),
            target_path,
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn scan_hook_for_adoption(
    tool: Tool,
    descriptor: &TargetDescriptor,
    ownership: &ManagedOwnership,
) -> Result<ObservedTarget, AppError> {
    match scan_target(tool.adapter(), descriptor, ownership) {
        TargetScan::Observed(observed) => Ok(*observed),
        TargetScan::Missing => Err(AppError::conflict(
            "hookAdopt",
            "目标不存在，无法从原生内容采纳",
        )),
        TargetScan::ParseError => Err(AppError::parse(
            descriptor.path.as_deref().unwrap_or_default(),
            descriptor.format.as_str(),
        )),
        TargetScan::PermissionDenied => Err(AppError::permission(
            descriptor.path.as_deref().unwrap_or_default(),
            "read_hook_target",
        )),
        TargetScan::TargetTypeChanged(_) => Err(AppError::conflict(
            "hookAdopt",
            "目标类型已变化，无法安全采纳",
        )),
        TargetScan::Failed | TargetScan::Unavailable | TargetScan::ManagedItemBaselineMismatch => {
            Err(AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))
        }
    }
}

fn rescan_hook_for_adoption(
    tool: Tool,
    descriptor: &TargetDescriptor,
    ownership: &ManagedOwnership,
    expected_full_hash: &str,
    expected_managed_hash: &str,
    target_path: &str,
) -> Result<ObservedTarget, AppError> {
    let observed = scan_hook_for_adoption(tool, descriptor, ownership)?;
    if observed.full_hash != expected_full_hash || observed.managed_hash != expected_managed_hash {
        return Err(AppError::stale_preview("adoptHookNative", target_path));
    }
    Ok(observed)
}

fn validate_native_hook_projection(
    observed: &ObservedTarget,
    tool: Tool,
    home: &Path,
) -> Result<Vec<ParsedNativeHook>, AppError> {
    let managed = observed
        .managed_projection
        .as_object()
        .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
    let events = match tool {
        Tool::Cursor => {
            ensure_native_hook_keys(managed, &["version", "hooks"])?;
            if managed.get("version").and_then(Value::as_i64) != Some(1) {
                return Err(AppError::conflict(
                    "hookAdopt",
                    "原生 Hook 版本字段无法无损映射",
                ));
            }
            managed
                .get("hooks")
                .and_then(Value::as_object)
                .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?
        }
        Tool::Zcode => {
            ensure_native_hook_keys(managed, &["hooks"])?;
            let hooks = managed
                .get("hooks")
                .and_then(Value::as_object)
                .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
            if hooks.get("enabled") != Some(&Value::Bool(true)) {
                return Err(AppError::conflict(
                    "assignment",
                    "ZCode Hook runner 状态不是可直接采纳的 enabled",
                ));
            }
            ensure_native_hook_keys(hooks, &["enabled", "events"])?;
            hooks
                .get("events")
                .and_then(Value::as_object)
                .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?
        }
        Tool::Claude | Tool::Codex => {
            ensure_native_hook_keys(managed, &["hooks"])?;
            managed
                .get("hooks")
                .and_then(Value::as_object)
                .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?
        }
        Tool::Opencode | Tool::Pi => {
            return Err(AppError::invalid_input(
                "capability",
                "该工具没有可采纳的声明式 Hook 合同",
            ))
        }
    };
    for (native_event, groups) in events {
        let event = canonical_native_hook_event(tool, native_event).ok_or_else(|| {
            AppError::conflict("event", "原生 Hook 事件无法映射到中央事件")
        })?;
        if !event.supported_for_tool(tool) {
            return Err(AppError::conflict(
                "event",
                "原生 Hook 事件不是该工具支持的事件",
            ));
        }
        let groups = groups
            .as_array()
            .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
        for group in groups {
            let object = group
                .as_object()
                .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
            if tool == Tool::Cursor {
                // Cursor 使用 event -> entry[] 的扁平结构；matcher 属于条目本身，
                // `hooks`/`type` 无法在此结构中无损表示。
                ensure_native_hook_keys(object, &["command", "timeout", "matcher"])?;
            } else {
                ensure_native_hook_keys(object, &["matcher", "hooks"])?;
                if object.contains_key("matcher")
                    && (!object["matcher"].is_string()
                        || object["matcher"].as_str().is_some_and(str::is_empty))
                {
                    return Err(AppError::conflict(
                        "matcher",
                        "原生 Hook matcher 不能为空或必须是字符串",
                    ));
                }
                let entries = object
                    .get("hooks")
                    .and_then(Value::as_array)
                    .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
                if entries.is_empty() {
                    return Err(AppError::conflict(
                        "hookAdopt",
                        "空的原生 Hook 分组无法唯一映射",
                    ));
                }
                for entry in entries {
                    if !entry.is_object() {
                        return Err(AppError::conflict(
                            "hookAdopt",
                            HOOK_NATIVE_MATCH_OR_IMPORT,
                        ));
                    }
                }
            }
        }
    }

    native_hook_rows(observed, tool)
        .into_iter()
        .map(|(native_event, matcher, entry)| {
            let event = canonical_native_hook_event(tool, &native_event).ok_or_else(|| {
                AppError::conflict("event", "原生 Hook 事件无法映射到中央事件")
            })?;
            parse_native_hook_entry(tool, event, &matcher, entry, home)
        })
        .collect()
}

fn ensure_native_hook_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), AppError> {
    if object.keys().all(|key| allowed.iter().any(|allowed| key == allowed)) {
        Ok(())
    } else {
        Err(AppError::conflict(
            "hookAdopt",
            "原生 Hook 含有中央模型未建模的字段",
        ))
    }
}

fn canonical_native_hook_event(tool: Tool, native_event: &str) -> Option<HookEvent> {
    let canonical = match tool {
        Tool::Cursor => match native_event {
            "sessionStart" => "SessionStart",
            "sessionEnd" => "SessionEnd",
            "preToolUse" => "PreToolUse",
            "postToolUse" => "PostToolUse",
            "postToolUseFailure" => "PostToolUseFailure",
            "subagentStart" => "SubagentStart",
            "subagentStop" => "SubagentStop",
            "preCompact" => "PreCompact",
            "stop" => "Stop",
            _ => return None,
        },
        _ => native_event,
    };
    HookEvent::from_stable_str(canonical)
}

fn parse_native_hook_entry(
    tool: Tool,
    event: HookEvent,
    matcher: &str,
    entry: Value,
    home: &Path,
) -> Result<ParsedNativeHook, AppError> {
    let object = entry
        .as_object()
        .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
    if let Some(value) = object.get("matcher") {
        if !value.is_string() || value.as_str().is_some_and(str::is_empty) {
            return Err(AppError::conflict(
                "matcher",
                "原生 Hook matcher 不能为空或必须是字符串",
            ));
        }
    }
    let raw_matcher = object.get("matcher").and_then(Value::as_str);
    let matcher = if matcher.is_empty() {
        None
    } else {
        Some(matcher.to_owned())
    };
    if raw_matcher.is_some() && tool != Tool::Cursor {
        return Err(AppError::conflict(
            "matcher",
            "该工具的 matcher 必须位于原生分组而非条目",
        ));
    }
    let matcher = if tool == Tool::Cursor {
        raw_matcher.map(str::to_owned)
    } else {
        matcher
    };
    if tool == Tool::Cursor {
        ensure_native_hook_keys(object, &["command", "timeout", "matcher"])?;
    } else {
        ensure_native_hook_keys(object, &["type", "command", "timeout"])?;
        if object.get("type").and_then(Value::as_str) != Some("command") {
            return Err(AppError::conflict(
                "type",
                "原生 Hook 类型不是可无损映射的 command",
            ));
        }
    }
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::conflict("command", HOOK_NATIVE_MATCH_OR_IMPORT))?;
    let timeout_seconds = parse_native_hook_timeout(object)?;
    let (_, normalized_matcher, normalized_command, timeout_seconds) = validate_hook_definition(
        "native-hook",
        event,
        matcher.as_deref(),
        command,
        timeout_seconds,
    )?;
    // 中央校验会 trim 这些字段。接受非规范的原生空白会导致中央期望投影与
    // 刚采纳的原生投影永远 hash 不同，因此宁可进入匹配/导入，不在这里猜测
    // 用户想保留的排版语义。
    if normalized_matcher.as_deref() != matcher.as_deref() || normalized_command != command {
        return Err(AppError::conflict(
            "hookAdopt",
            "原生 Hook 字段不是中央可无损保留的规范形式",
        ));
    }
    if normalized_matcher
        .as_deref()
        .is_some_and(|value| contains_detectable_secret("matcher", value))
    {
        return Err(AppError::conflict(
            "matcher",
            "原生 Hook matcher 含有可识别的敏感信息",
        ));
    }
    let matcher = normalized_matcher;
    let command = normalized_command;
    let words = split_shell_words(&command).ok_or_else(|| {
        AppError::conflict("command", "命令包含复杂 shell 语法，请进入匹配或导入流程")
    })?;
    if !is_simple_hook_command(&command, &words) {
        return Err(AppError::conflict(
            "command",
            "命令包含复杂 shell 语法，请进入匹配或导入流程",
        ));
    }
    let script_source = if interpreter_scan_start(&words).is_some() {
        if words.iter().any(|word| {
            matches!(
                word.as_str(),
                "-c" | "--command" | "-e" | "--eval" | "--exec"
            )
        }) {
            return Err(AppError::conflict(
                "command",
                "解释器命令不是可安全接管的脚本调用",
            ));
        }
        Some(
            resolve_script_adoption(&command, home)
                .map(|(_, path)| path)
                .ok_or_else(|| {
                    AppError::conflict(
                        "command",
                        "解释器命令不是可安全接管的脚本调用",
                    )
                })?,
        )
    } else {
        None
    };
    Ok(ParsedNativeHook {
        event,
        matcher,
        command,
        timeout_seconds,
        native_entry: entry,
        script_source,
    })
}

fn parse_native_hook_timeout(object: &Map<String, Value>) -> Result<Option<i32>, AppError> {
    let Some(value) = object.get("timeout") else {
        return Ok(None);
    };
    let seconds = value.as_i64().ok_or_else(|| {
        AppError::invalid_input("timeoutSeconds", "原生 Hook timeout 必须是整数秒")
    })?;
    i32::try_from(seconds).map(Some).map_err(|error| {
        AppError::invalid_input("timeoutSeconds", "原生 Hook timeout 超出安全范围")
            .with_source(error)
    })
}

fn is_simple_hook_command(command: &str, words: &[String]) -> bool {
    if words.is_empty()
        || command
            .chars()
            .any(|character| {
                matches!(
                    character,
                    ';' | '&'
                        | '|'
                        | '>'
                        | '<'
                        | '`'
                        | '\\'
                        | '\n'
                        | '\r'
                        | '('
                        | ')'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '*'
                        | '?'
                        | '#'
                        | '!'
                )
            })
    {
        return false;
    }
    // 只接受明确的 `$HOME/…` 写法；其他变量、命令替换或花括号展开都属于未建模
    // 的 shell 语义。
    words.iter().all(|word| {
        !word.contains('$') || word.starts_with("$HOME/")
    })
}

fn adopted_hook_assignments(
    database: &Database,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<Vec<HookRecord>, AppError> {
    let mut records = repository::list_assigned_hooks(database, tool, project_id)?
        .into_iter()
        .filter(|record| record.enabled)
        .collect::<Vec<_>>();
    // 项目 descriptor 只代表显式项目文件。全局 assignment 从独立的全局 descriptor
    // 继承，不能重复放入该目标的 managed item 集合。
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let mut seen = BTreeSet::new();
    for record in &records {
        if !seen.insert(record.id.as_str()) {
            return Err(AppError::conflict(
                "assignment",
                "同一 Hook 在目标中存在重复 assignment",
            ));
        }
    }
    Ok(records)
}

/// 校验 ExternalChangePlan 携带的完整行版本集合。Hook 目标预览会绑定
/// Project（项目目标）、全部有效中央 Hook 和全部 managed item；目标行自身
/// 由 `target_row_version` 单独绑定。少一行、多一行、重复行或实体类型不符都
/// 不能被当作同一份采纳证据。
fn validate_hook_adoption_row_versions(
    rows: &[DatabaseRowVersion],
    project: Option<&McpProjectRecord>,
    records: &[HookRecord],
    items: &[ManagedHookItemRecord],
    target_id: &str,
    target_row_version: u32,
) -> Result<(), AppError> {
    let mut expected = BTreeMap::new();
    if let Some(project) = project {
        expected.insert(
            (DatabaseEntityType::Project, project.id.clone()),
            safe_row_version(project.row_version)?,
        );
    }
    for record in records {
        expected.insert(
            (DatabaseEntityType::Hook, record.id.clone()),
            safe_row_version(record.row_version)?,
        );
    }
    for item in items {
        expected.insert(
            (DatabaseEntityType::ManagedItem, item.id.clone()),
            safe_row_version(item.row_version)?,
        );
    }

    let mut actual = BTreeMap::new();
    for row in rows {
        if !matches!(
            row.entity_type,
            DatabaseEntityType::Project
                | DatabaseEntityType::Hook
                | DatabaseEntityType::ManagedItem
                | DatabaseEntityType::ManagedTarget
        ) {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Hook 原生采纳包含不支持的数据库行版本",
            ));
        }
        let key = (row.entity_type, row.entity_id.clone());
        if actual.insert(key, row.row_version).is_some() {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Hook 原生采纳包含重复的行版本证据",
            ));
        }
    }

    // 目标版本是独立输入；如果调用方也携带它，必须仍然指向同一目标，
    // 但不要求它出现在 PreviewTargetRequest.row_versions 中。
    if let Some(version) = actual.remove(&(DatabaseEntityType::ManagedTarget, target_id.into())) {
        if version != target_row_version {
            return Err(AppError::stale_preview("adoptHookNative", target_id));
        }
    }
    if actual != expected {
        return Err(AppError::stale_preview("adoptHookNative", "rowVersions"));
    }
    Ok(())
}

fn parse_hook_item_identity(
    item: &ManagedHookItemRecord,
) -> Result<(HookEvent, String, String), AppError> {
    let mut parts = item.external_key.splitn(3, '|');
    let event = parts
        .next()
        .and_then(HookEvent::from_stable_str)
        .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
    let identity = parts
        .next()
        .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
    let matcher = parts
        .next()
        .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
    if identity.len() != 16
        || !identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT));
    }
    Ok((event, matcher.to_owned(), identity.to_owned()))
}

fn match_native_hook_rows(
    records: &[HookRecord],
    items: &[ManagedHookItemRecord],
    rows: &[ParsedNativeHook],
) -> Result<Vec<(usize, usize)>, AppError> {
    if records.len() != items.len() || items.len() != rows.len() || items.is_empty() {
        return Err(AppError::conflict(
            "hookAdopt",
            HOOK_NATIVE_MATCH_OR_IMPORT,
        ));
    }
    let mut identities = Vec::with_capacity(items.len());
    for item in items {
        if !is_sha256(&item.last_applied_item_hash) {
            return Err(AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT));
        }
        let (event, matcher, identity) = parse_hook_item_identity(item)?;
        let record = records
            .iter()
            .find(|record| record.id == item.resource_id)
            .ok_or_else(|| AppError::conflict("hookAdopt", HOOK_NATIVE_MATCH_OR_IMPORT))?;
        if record.event != event || record.matcher.as_deref().unwrap_or_default() != matcher {
            return Err(AppError::conflict(
                "assignment",
                "原生 Hook assignment 含义与 managed item 基线不一致",
            ));
        }
        // 短 identity 组件不能用于猜测原生行，但仍必须是当前中央记录生成的
        // identity。否则损坏或过期的 managed item 可能让表面有效的
        // event+matcher 配对被误认为可以直接采纳。
        if item.external_key != hook_external_key(record) {
            return Err(AppError::conflict(
                "hookAdopt",
                "Hook managed item 身份基线与中央记录不一致",
            ));
        }
        identities.push((event, matcher, identity));
    }

    let mut row_to_item = vec![None; rows.len()];
    let mut used_items = BTreeSet::new();
    for (row_index, row) in rows.iter().enumerate() {
        let row_hash = hash_json(&row.native_entry);
        let row_matcher = row.matcher.as_deref().unwrap_or_default();
        // 非 Cursor 原生条目 hash 有意省略外层 matcher/event。如果未变化的条目
        // hash 出现在另一个 assignment 下，把它当成命令变化会悄悄移动 Hook 的
        // assignment。先拒绝这份证据，再进行较弱的 event/matcher 匹配；用户可
        // 在匹配或导入流程中明确选择 identity。
        if identities.iter().enumerate().any(|(item_index, (event, matcher, _))| {
            items[item_index].last_applied_item_hash == row_hash
                && (*event != row.event || matcher != row_matcher)
        }) {
            return Err(AppError::conflict(
                "assignment",
                "原生 Hook 条目疑似改变了 event 或 matcher，无法直接采纳",
            ));
        }
        let candidates = identities
            .iter()
            .enumerate()
            .filter(|(item_index, (event, matcher, _))| {
                !used_items.contains(item_index)
                    && *event == row.event
                    && matcher == row_matcher
                    && items[*item_index].last_applied_item_hash == row_hash
            })
            .map(|(item_index, _)| item_index)
            .collect::<Vec<_>>();
        if candidates.len() > 1 {
            return Err(AppError::conflict(
                "hookAdopt",
                HOOK_NATIVE_MATCH_OR_IMPORT,
            ));
        }
        if let Some(item_index) = candidates.first().copied() {
            row_to_item[row_index] = Some(item_index);
            used_items.insert(item_index);
        }
    }

    loop {
        let mut progress = false;
        for (row_index, row) in rows.iter().enumerate() {
            if row_to_item[row_index].is_some() {
                continue;
            }
            let row_matcher = row.matcher.as_deref().unwrap_or_default();
            let candidates = identities
                .iter()
                .enumerate()
                .filter(|(item_index, (event, matcher, _))| {
                    !used_items.contains(item_index)
                        && *event == row.event
                        && matcher == row_matcher
                })
                .map(|(item_index, _)| item_index)
                .collect::<Vec<_>>();
            if candidates.len() == 1 {
                let item_index = candidates[0];
                row_to_item[row_index] = Some(item_index);
                used_items.insert(item_index);
                progress = true;
            }
        }
        if progress {
            continue;
        }
        for (row_index, row) in rows.iter().enumerate() {
            if row_to_item[row_index].is_some() {
                continue;
            }
            let candidates = identities
                .iter()
                .enumerate()
                .filter(|(item_index, (event, _, _))| {
                    !used_items.contains(item_index) && *event == row.event
                })
                .map(|(item_index, _)| item_index)
                .collect::<Vec<_>>();
            if candidates.len() == 1 {
                let item_index = candidates[0];
                row_to_item[row_index] = Some(item_index);
                used_items.insert(item_index);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    if row_to_item.iter().any(Option::is_none) || used_items.len() != items.len() {
        return Err(AppError::conflict(
            "hookAdopt",
            HOOK_NATIVE_MATCH_OR_IMPORT,
        ));
    }
    Ok(row_to_item
        .into_iter()
        .enumerate()
        .map(|(row_index, item_index)| (row_index, item_index.unwrap_or_default()))
        .collect())
}

fn validate_hook_script_name(name: &str) -> Result<String, AppError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || name.chars().any(|character| character.is_control())
    {
        return Err(AppError::conflict(
            "scriptSourcePath",
            "中央脚本文件名不安全",
        ));
    }
    Ok(name.to_owned())
}

fn validate_hook_script_source(
    source: &Path,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    descriptor: &TargetDescriptor,
    project_root: Option<&ProjectRoot>,
) -> Result<PathBuf, AppError> {
    if !source.is_absolute()
        || source.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir
                    | std::path::Component::ParentDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(AppError::conflict(
            "scriptSourcePath",
            "脚本路径不是安全的绝对路径",
        ));
    }
    reject_symlink_components(source).map_err(|_| {
        AppError::conflict(
            "scriptSourcePath",
            "脚本路径包含 symlink 或路径逃逸",
        )
    })?;
    let canonical = fs::canonicalize(source).map_err(|_| {
        AppError::conflict(
            "scriptSourcePath",
            "脚本路径无法安全解析，请进入匹配或导入流程",
        )
    })?;
    let mut roots = vec![
        environment.home().to_path_buf(),
        descriptor_allowed_root(descriptor)?,
        paths.central_hooks().to_path_buf(),
    ];
    if let Some(project_root) = project_root {
        roots.push(PathBuf::from(project_root.as_str()));
    }
    let allowed = roots.into_iter().filter_map(|root| fs::canonicalize(root).ok());
    if !allowed.into_iter().any(|root| canonical.starts_with(root)) {
        return Err(AppError::conflict(
            "scriptSourcePath",
            "脚本路径超出目标与项目安全边界",
        ));
    }
    Ok(canonical)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HookScriptStamp {
    length: u64,
    modified: Option<std::time::SystemTime>,
}

fn read_stable_hook_script(path: &Path) -> Result<(Vec<u8>, String), AppError> {
    let read_once = |path: &Path| -> Result<(Vec<u8>, HookScriptStamp), AppError> {
        let metadata = fs::symlink_metadata(path).map_err(|_| {
            AppError::invalid_input("scriptSourcePath", "脚本不存在或不可读取")
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
        let bytes = fs::read(path).map_err(|_| {
            AppError::invalid_input("scriptSourcePath", "脚本不可读取")
        })?;
        let after = fs::symlink_metadata(path).map_err(|_| {
            AppError::stale_preview("adoptHookNative", "hookScript")
        })?;
        if after.file_type().is_symlink()
            || !after.is_file()
            || after.len() != metadata.len()
            || bytes.len() as u64 != after.len()
        {
            return Err(AppError::stale_preview("adoptHookNative", "hookScript"));
        }
        Ok((
            bytes,
            HookScriptStamp {
                length: after.len(),
                modified: after.modified().ok(),
            },
        ))
    };
    let (first, first_stamp) = read_once(path)?;
    let (second, second_stamp) = read_once(path)?;
    if first_stamp != second_stamp || first != second {
        return Err(AppError::stale_preview("adoptHookNative", "hookScript"));
    }
    let hash = hash_bytes(&first);
    Ok((first, hash))
}

fn stage_hook_script(
    paths: &AppPaths,
    hook_id: &str,
    source: &Path,
    bytes: &[u8],
    hash: &str,
    file_name: &str,
    expected_final_hash: Option<&str>,
) -> Result<StagedHookScript, AppError> {
    let stage_path = paths
        .staging()
        .join(format!("hook-adopt-{}.stage", Uuid::new_v4()));
    let final_path = paths.central_hooks().join(hook_id).join(file_name);
    let result = (|| {
        ensure_private_directory(paths.staging())?;
        let mut file = create_private_file(&stage_path).map_err(|error| {
            AppError::atomic_write("hook-staging", "create_hook_script_stage").with_source(error)
        })?;
        std::io::Write::write_all(&mut file, bytes).map_err(|error| {
            AppError::atomic_write("hook-staging", "write_hook_script_stage").with_source(error)
        })?;
        std::io::Write::flush(&mut file).map_err(|error| {
            AppError::atomic_write("hook-staging", "flush_hook_script_stage").with_source(error)
        })?;
        file.sync_all().map_err(|error| {
            AppError::atomic_write("hook-staging", "sync_hook_script_stage").with_source(error)
        })?;
        Ok(StagedHookScript {
            source: source.to_path_buf(),
            hash: hash.to_owned(),
            stage_path: stage_path.clone(),
            final_path,
            expected_final_hash: expected_final_hash.map(str::to_owned),
            backup_path: None,
            installed: false,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&stage_path);
    }
    result
}

fn verify_hook_script_sources(scripts: &[StagedHookScript]) -> Result<(), AppError> {
    for script in scripts {
        let (bytes, hash) = read_stable_hook_script(&script.source)?;
        if hash != script.hash || hash_bytes(&bytes) != script.hash {
            return Err(AppError::stale_preview("adoptHookNative", "hookScript"));
        }
        // 安装后中央副本也属于本次事务证据；如果另一个进程在数据库
        // 提交前替换它，必须回滚中央行和已安装副本，而不能只信任源文件。
        if script.installed {
            let (central_bytes, central_hash) = read_stable_hook_script(&script.final_path)?;
            if central_hash != script.hash || hash_bytes(&central_bytes) != script.hash {
                return Err(AppError::stale_preview(
                    "adoptHookNative",
                    "centralHookScript",
                ));
            }
        }
    }
    Ok(())
}

fn install_staged_hook_scripts(scripts: &mut [StagedHookScript]) -> Result<(), AppError> {
    for script in scripts {
        let parent = script.final_path.parent().ok_or_else(|| {
            AppError::atomic_write("hook-script", "resolve_hook_script_parent")
        })?;
        ensure_private_directory(parent)?;
        reject_symlink_components(&script.final_path).map_err(|error| {
            AppError::atomic_write("hook-script", "validate_hook_script_destination")
                .with_source(error)
        })?;
        match fs::symlink_metadata(&script.final_path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(AppError::atomic_write(
                        "hook-script",
                        "reject_hook_script_destination",
                    ));
                }
                let expected = script.expected_final_hash.as_deref().ok_or_else(|| {
                    AppError::conflict(
                        "hook-script",
                        "中央脚本目标已存在但没有可验证的旧资产 hash",
                    )
                })?;
                let (_, actual_hash) = read_stable_hook_script(&script.final_path)?;
                if actual_hash != expected {
                    return Err(AppError::stale_preview(
                        "adoptHookNative",
                        "centralHookScript",
                    ));
                }
                let backup_path = script
                    .stage_path
                    .with_extension(format!("backup-{}", Uuid::new_v4()));
                fs::rename(&script.final_path, &backup_path).map_err(|error| {
                    AppError::atomic_write("hook-script", "backup_hook_script").with_source(error)
                })?;
                script.backup_path = Some(backup_path);
                if let Some(expected) = script.expected_final_hash.as_deref() {
                    let backup = script.backup_path.as_ref().ok_or_else(|| {
                        AppError::atomic_write("hook-script", "track_hook_script_backup")
                    })?;
                    let (_, actual_hash) = read_stable_hook_script(backup)?;
                    if actual_hash != expected {
                        return Err(AppError::stale_preview(
                            "adoptHookNative",
                            "centralHookScript",
                        ));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if script.expected_final_hash.is_some() {
                    return Err(AppError::stale_preview(
                        "adoptHookNative",
                        "centralHookScript",
                    ));
                }
            }
            Err(error) => {
                return Err(AppError::atomic_write(
                    "hook-script",
                    "inspect_hook_script_destination",
                )
                .with_source(error));
            }
        }
        fs::rename(&script.stage_path, &script.final_path).map_err(|error| {
            AppError::atomic_write("hook-script", "install_hook_script").with_source(error)
        })?;
        script.installed = true;
        ensure_private_file(&script.final_path).map_err(|error| {
            AppError::atomic_write("hook-script", "validate_installed_hook_script")
                .with_source(error)
        })?;
    }
    Ok(())
}

fn remove_exact_hook_file(path: &Path) -> Result<(), std::io::Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn restore_hook_scripts(scripts: &mut [StagedHookScript]) -> Result<(), AppError> {
    let mut failure = None;
    for script in scripts.iter_mut().rev() {
        if script.installed {
            if let Err(error) = remove_exact_hook_file(&script.final_path) {
                failure = Some(error);
                continue;
            }
            script.installed = false;
        }
        if let Some(backup) = script.backup_path.take() {
            if let Err(error) = fs::rename(&backup, &script.final_path) {
                failure = Some(error);
            }
        }
        let _ = remove_exact_hook_file(&script.stage_path);
    }
    if let Some(error) = failure {
        Err(AppError::rollback_failed(
            "adoptHookNative",
            "hook-script",
            "staging",
        )
        .with_source(error))
    } else {
        Ok(())
    }
}

fn restore_hook_scripts_or_error(
    scripts: &mut [StagedHookScript],
    target_path: &str,
    original: AppError,
) -> Result<AdoptHookNativeResultDto, AppError> {
    match restore_hook_scripts(scripts) {
        Ok(()) => Err(original),
        Err(error) => Err(AppError::rollback_failed(
            "adoptHookNative",
            target_path,
            "hook-script",
        )
        .with_source(error.source().unwrap_or("hook script rollback failed"))),
    }
}

fn cleanup_uninstalled_hook_scripts(scripts: &mut [StagedHookScript]) {
    for script in scripts {
        if !script.installed {
            let _ = remove_exact_hook_file(&script.stage_path);
        }
        if let Some(backup) = script.backup_path.take() {
            let _ = remove_exact_hook_file(&backup);
        }
    }
}

fn cleanup_committed_hook_scripts(scripts: &mut [StagedHookScript]) {
    for script in scripts {
        let _ = remove_exact_hook_file(&script.stage_path);
        if let Some(backup) = script.backup_path.take() {
            let _ = remove_exact_hook_file(&backup);
        }
    }
}
