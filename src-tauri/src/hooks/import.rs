//! 原生全局 Hooks 的只读发现与显式导入。
//!
//! 与 MCP 导入的差异：hooks 的原生条目是匿名数组元素（无稳定名称键），
//! 无法像 MCP 那样在导入事务内精确登记 per-item 基线。因此导入不接管
//! 目标基线，而是：
//! 1. 解析命令中的脚本路径，可接管的脚本在确认后**复制到中央目录**
//!    （`central_hooks/<hook_id>/`，复制不移动，原文件保留），中央命令
//!    重写为引用中央副本；
//! 2. 用户分配并同步后，原生配置直接引用中央副本，消除对原路径的依赖。

use std::collections::BTreeSet;

use serde_json::Value;

use super::{
    service, ConfirmHookImportInput, DiscoverHookImportInput, HookImportCandidateDto,
    HookImportCandidateStatus as Status, HookImportPreviewDto, HookImportResultDto,
};
use crate::{
    adapters::{CapabilityState, ExplicitEnvironment, PolicyState, TargetDescriptor},
    db::{hooks as hook_repository, Database},
    domain::{HookEvent, Tool},
    error::{AppError, ErrorCode},
    security::contains_detectable_secret,
    sync::{scan_target, TargetScan},
};

pub fn discover_hook_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &DiscoverHookImportInput,
) -> Result<HookImportPreviewDto, AppError> {
    let tool = input.tool;
    let descriptor = service::hook_target_descriptor(environment, tool, None)?;
    let target_path = descriptor
        .path
        .clone()
        .ok_or_else(|| AppError::not_found("hookTarget", tool.as_str()))?;
    ensure_readable(&descriptor, &target_path)?;
    let scan = scan_target(
        tool.adapter(),
        &descriptor,
        &service::build_hook_ownership(tool),
    );
    let (rows, message) = match scan {
        TargetScan::Missing => (
            Vec::new(),
            Some("未发现该工具的全局 hooks 配置文件。".to_owned()),
        ),
        TargetScan::Observed(observed) => {
            let rows = service::native_hook_rows(&observed, tool);
            if rows.is_empty() {
                (rows, Some("配置文件中没有 hooks 条目。".to_owned()))
            } else {
                (rows, None)
            }
        }
        TargetScan::ParseError => return Err(AppError::parse(&target_path, "hooks")),
        TargetScan::PermissionDenied => {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "无法安全读取全局 hooks 配置",
                true,
            ));
        }
        TargetScan::TargetTypeChanged(_) => {
            return Err(AppError::conflict(
                "path",
                "全局 hooks 路径或祖先不是安全的普通文件",
            ));
        }
        _ => {
            return Err(AppError::conflict(
                "import",
                "无法安全检测全局 hooks 配置，请重新检测",
            ));
        }
    };

    let existing = hook_repository::list_hooks(database)?;
    let mut used_names: BTreeSet<String> = existing
        .iter()
        .map(|hook| hook.name.to_lowercase())
        .collect();
    let mut candidates = Vec::new();
    for (native_event, matcher, entry) in rows {
        let command = entry
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let timeout_seconds = parse_timeout_seconds(&entry);
        let event = canonical_event(tool, &native_event);
        let mut candidate = HookImportCandidateDto {
            candidate_id: uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            event,
            matcher: (!matcher.is_empty()).then(|| matcher.clone()),
            command: command.clone(),
            timeout_seconds,
            status: Status::Invalid,
            script_adopted: false,
            script_source_path: None,
            reason: None,
        };
        if command.trim().is_empty() {
            candidate.reason = Some("条目缺少 command，无法导入。".to_owned());
            candidates.push(candidate);
            continue;
        }
        if contains_detectable_secret("command", &command)
            || contains_detectable_secret("matcher", &matcher)
        {
            candidate.reason = Some("条目包含可识别的凭据，不能导入。".to_owned());
            candidates.push(candidate);
            continue;
        }
        let entry_type = entry.get("type").and_then(Value::as_str);
        if let Some(reason) = unsupported_entry_reason(tool, entry_type) {
            candidate.reason = Some(reason);
            candidates.push(candidate);
            continue;
        }
        let Some(event) = event else {
            candidate.status = Status::UnsupportedEvent;
            candidate.reason =
                Some("该事件不在统一事件模型内（工具特有事件暂不支持跨工具管理）。".to_owned());
            candidates.push(candidate);
            continue;
        };
        candidate.event = Some(event);
        // 脚本接管解析：可接管时计算脚本内容哈希用于 AlreadyManaged 去重；
        // 首 token 是解释器却没找到可接管脚本时给出提示（命令将原样保存）。
        let script_hash = match service::resolve_script_adoption(&command, environment.home()) {
            Some((_, source_path)) => {
                candidate.script_adopted = true;
                candidate.script_source_path = Some(source_path.to_string_lossy().into_owned());
                Some(service::script_content_hash(&source_path)?)
            }
            None => {
                if looks_like_interpreter_command(&command) {
                    candidate.reason = Some(
                        "未找到可解析的脚本文件（如项目级变量路径或相对路径），命令将原样保存。"
                            .to_owned(),
                    );
                }
                None
            }
        };
        let already_managed = existing.iter().any(|hook| {
            hook.event == event
                && hook.matcher == candidate.matcher
                && hook.timeout_seconds == timeout_seconds
                && match (&script_hash, &hook.script_hash) {
                    (Some(hash), Some(existing_hash)) => hash == existing_hash,
                    (None, None) => hook.command == command,
                    _ => false,
                }
        });
        if already_managed {
            candidate.status = Status::AlreadyManaged;
            candidate.reason = Some("中央库已存在相同定义的 Hook。".to_owned());
            candidates.push(candidate);
            continue;
        }
        let base = suggested_name(&native_event, &command);
        let mut index = 1;
        let name = loop {
            let candidate_name = if index == 1 {
                base.clone()
            } else {
                format!("{base}-{index}")
            };
            if used_names.insert(candidate_name.to_lowercase()) {
                break candidate_name;
            }
            index += 1;
        };
        candidate.name = name;
        candidate.status = Status::Importable;
        candidates.push(candidate);
    }
    Ok(HookImportPreviewDto {
        tool,
        target_path,
        candidates,
        message,
    })
}

pub fn confirm_hook_import(
    database: &mut Database,
    paths: &crate::app::AppPaths,
    environment: &ExplicitEnvironment,
    input: &ConfirmHookImportInput,
) -> Result<HookImportResultDto, AppError> {
    if input.hooks.is_empty() {
        return Err(AppError::invalid_input("hooks", "请选择要导入的 Hook"));
    }
    // 目标可用性（capability/policy）与事件支持按工具入口校验；
    // 定义复用中央 create 校验（含名称唯一性）。
    let descriptor = service::hook_target_descriptor(environment, input.tool, None)?;
    ensure_readable(&descriptor, descriptor.path.as_deref().unwrap_or_default())?;
    let mut created = 0u32;
    for hook in &input.hooks {
        service::hook_event_supported(input.tool, hook.event)?;
        service::create_hook(database, paths, hook)?;
        created += 1;
    }
    Ok(HookImportResultDto {
        tool: input.tool,
        created_count: created,
    })
}

fn ensure_readable(descriptor: &TargetDescriptor, path: &str) -> Result<(), AppError> {
    if descriptor.capability.state != CapabilityState::Supported {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "工具不可用，不能导入 hooks",
            true,
        ));
    }
    if descriptor.policy != PolicyState::Allowed {
        return Err(AppError::policy_blocked(
            descriptor.tool.as_str(),
            path,
            if descriptor.policy == PolicyState::Unknown {
                "CLAUDE_POLICY_UNKNOWN"
            } else {
                "CLAUDE_POLICY_BLOCKED"
            },
        ));
    }
    Ok(())
}

/// Cursor 原生 camelCase → 统一 PascalCase；无法映射（工具特有事件）返回 None。
fn canonical_event(tool: Tool, native_event: &str) -> Option<HookEvent> {
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

fn parse_timeout_seconds(entry: &Value) -> Option<i32> {
    let timeout = entry.get("timeout").and_then(Value::as_i64);
    // ZCode command 型的 timeoutMs（毫秒、优先）只在整秒时保真转换。
    let timeout_ms = entry.get("timeoutMs").and_then(Value::as_i64);
    let seconds = match (timeout, timeout_ms) {
        (_, Some(ms)) if ms % 1000 == 0 && ms > 0 => Some(ms / 1000),
        (value, _) => value,
    }?;
    i32::try_from(seconds).ok()
}

fn unsupported_entry_reason(tool: Tool, entry_type: Option<&str>) -> Option<String> {
    match entry_type {
        None | Some("command") => None,
        Some("process") if tool == Tool::Zcode => {
            Some("process 型 hook 暂不支持导入（仅支持 command 型）。".to_owned())
        }
        Some("prompt") if tool == Tool::Cursor => {
            Some("prompt 型 hook 暂不支持导入（仅支持 command 型）。".to_owned())
        }
        Some(other) => Some(format!("type {other} 暂不支持导入（仅支持 command 型）。")),
    }
}

fn looks_like_interpreter_command(command: &str) -> bool {
    service::split_shell_words(command)
        .and_then(|ref words| service::interpreter_scan_start(words))
        .is_some()
}

fn suggested_name(native_event: &str, command: &str) -> String {
    let first_token = command
        .split_whitespace()
        .next()
        .unwrap_or("hook")
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("hook");
    let mut sanitized: String = first_token
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    sanitized.truncate(40);
    let sanitized = sanitized.trim_matches('-');
    let prefix = if sanitized.is_empty() {
        "hook"
    } else {
        sanitized
    };
    format!("{native_event}-{prefix}")
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

    use serde_json::json;
    use tempfile::tempdir;

    use super::{confirm_hook_import, discover_hook_import};
    use crate::{
        adapters::{ExplicitEnvironment, ToolAvailability},
        app::AppPaths,
        db::{hooks as hook_repository, Database},
        domain::{HookEvent, Tool},
        error::ErrorCode,
        hooks::{
            ConfirmHookImportInput, CreateHookInput, DiscoverHookImportInput,
            HookImportCandidateDto, HookImportCandidateStatus as Status,
        },
    };

    struct Fixture {
        _temporary: tempfile::TempDir,
        home: PathBuf,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            let codex_home = root.join("codex-home");
            for directory in [&home, &codex_home, &home.join(".claude")] {
                fs::create_dir_all(directory).unwrap();
            }
            let paths = AppPaths::from_data_root(root.join("app-data")).unwrap();
            let database = Database::open(&paths).unwrap();
            let environment = ExplicitEnvironment::new(
                &home,
                None,
                Some(codex_home),
                ToolAvailability::all_installed(),
            )
            .unwrap();
            Self {
                _temporary: temporary,
                home,
                paths,
                database,
                environment,
            }
        }

        fn codex_hooks(&self) -> PathBuf {
            self.environment.codex_home().join("hooks.json")
        }

        fn write_codex_hooks(&self, hooks: serde_json::Value) {
            fs::write(
                self.codex_hooks(),
                serde_json::to_string_pretty(&json!({
                    "description": "保留我",
                    "hooks": hooks,
                }))
                .unwrap(),
            )
            .unwrap();
        }

        fn write_script(&self, relative: &str, body: &str) -> PathBuf {
            let path = self.home.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            path
        }

        fn discover(&mut self) -> Vec<HookImportCandidateDto> {
            discover_hook_import(
                &mut self.database,
                &self.environment,
                &DiscoverHookImportInput { tool: Tool::Codex },
            )
            .unwrap()
            .candidates
        }
    }

    fn command_entry(command: &str, timeout: Option<i64>) -> serde_json::Value {
        let mut entry = json!({ "type": "command", "command": command });
        if let Some(timeout) = timeout {
            entry["timeout"] = json!(timeout);
        }
        entry
    }

    fn to_create_input(candidate: &HookImportCandidateDto) -> CreateHookInput {
        CreateHookInput {
            name: candidate.name.clone(),
            event: candidate.event.expect("可导入候选必须带事件"),
            matcher: candidate.matcher.clone(),
            command: candidate.command.clone(),
            timeout_seconds: candidate.timeout_seconds,
            enabled: true,
            script_source_path: candidate.script_source_path.clone(),
        }
    }

    #[test]
    fn discover_reports_missing_file_and_empty_hooks_without_candidates() {
        let mut fixture = Fixture::new();
        let preview = discover_hook_import(
            &mut fixture.database,
            &fixture.environment,
            &DiscoverHookImportInput { tool: Tool::Codex },
        )
        .unwrap();
        assert_eq!(preview.tool, Tool::Codex);
        assert_eq!(preview.target_path, fixture.codex_hooks().to_string_lossy());
        assert!(preview.candidates.is_empty());
        assert_eq!(
            preview.message.as_deref(),
            Some("未发现该工具的全局 hooks 配置文件。")
        );

        fixture.write_codex_hooks(json!({}));
        let preview = discover_hook_import(
            &mut fixture.database,
            &fixture.environment,
            &DiscoverHookImportInput { tool: Tool::Codex },
        )
        .unwrap();
        assert!(preview.candidates.is_empty());
        assert_eq!(
            preview.message.as_deref(),
            Some("配置文件中没有 hooks 条目。")
        );
    }

    #[test]
    fn discover_lists_inline_and_script_candidates_as_importable() {
        let mut fixture = Fixture::new();
        let script = fixture.write_script("scripts/guard.sh", "#!/bin/sh\nexit 0\n");
        let script_text = script.to_string_lossy().into_owned();
        fixture.write_codex_hooks(json!({
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [command_entry("echo inline-hook", Some(30))] },
                { "hooks": [command_entry(&format!("bash {script_text}"), None)] }
            ]
        }));

        let candidates = fixture.discover();
        assert_eq!(candidates.len(), 2);
        let inline = &candidates[0];
        assert_eq!(inline.status, Status::Importable);
        assert_eq!(inline.event, Some(HookEvent::PreToolUse));
        assert_eq!(inline.matcher.as_deref(), Some("Bash"));
        assert_eq!(inline.command, "echo inline-hook");
        assert_eq!(inline.timeout_seconds, Some(30));
        assert!(!inline.script_adopted);
        assert!(inline.script_source_path.is_none());
        assert_eq!(inline.name, "PreToolUse-echo");

        let scripted = &candidates[1];
        assert_eq!(scripted.status, Status::Importable);
        assert!(scripted.matcher.is_none());
        assert!(scripted.script_adopted);
        assert_eq!(
            scripted.script_source_path.as_deref(),
            Some(script_text.as_str())
        );
        assert_eq!(scripted.name, "PreToolUse-bash");
        assert!(scripted.reason.is_none());
        // 候选 ID 是随机 UUID，两个候选互不相同。
        assert_ne!(inline.candidate_id, scripted.candidate_id);
    }

    #[test]
    fn confirm_import_creates_central_hooks_and_copies_scripts_without_moving_them() {
        let mut fixture = Fixture::new();
        let script = fixture.write_script("scripts/guard.sh", "#!/bin/sh\nexit 0\n");
        fixture.write_codex_hooks(json!({
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [command_entry("echo inline-hook", Some(30))] },
                { "hooks": [command_entry(&format!("bash {}", script.display()), None)] }
            ]
        }));
        let candidates = fixture.discover();
        let result = confirm_hook_import(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ConfirmHookImportInput {
                tool: Tool::Codex,
                hooks: candidates.iter().map(to_create_input).collect(),
            },
        )
        .unwrap();
        assert_eq!(result.tool, Tool::Codex);
        assert_eq!(result.created_count, 2);

        let hooks = hook_repository::list_hooks(&fixture.database).unwrap();
        assert_eq!(hooks.len(), 2);
        let inline = hooks
            .iter()
            .find(|hook| hook.name == "PreToolUse-echo")
            .unwrap();
        assert_eq!(inline.command, "echo inline-hook");
        assert!(inline.script_name.is_none());
        assert!(inline.script_hash.is_none());

        let scripted = hooks
            .iter()
            .find(|hook| hook.name == "PreToolUse-bash")
            .unwrap();
        let script_name = scripted
            .script_name
            .as_deref()
            .expect("脚本型 Hook 必须记录脚本名");
        assert!(scripted.script_hash.is_some());
        let central_copy = fixture
            .paths
            .central_hooks()
            .join(&scripted.id)
            .join(script_name);
        assert!(central_copy.is_file(), "脚本必须复制到中央目录");
        assert_eq!(fs::read(&central_copy).unwrap(), fs::read(&script).unwrap());
        assert!(script.is_file(), "原脚本只复制不移动");
        // 中央命令改为引用中央副本，不再依赖原路径。
        assert!(scripted
            .command
            .contains(&central_copy.to_string_lossy().into_owned()));
        assert!(!scripted
            .command
            .contains(&script.to_string_lossy().into_owned()));
        // 导入只改中央状态，原生文件保持原样。
        assert!(fs::read_to_string(fixture.codex_hooks())
            .unwrap()
            .contains("保留我"));
    }

    #[test]
    fn discover_marks_duplicates_as_already_managed_by_definition_or_script_content() {
        let mut fixture = Fixture::new();
        let script = fixture.write_script("scripts/guard.sh", "#!/bin/sh\nexit 0\n");
        fixture.write_codex_hooks(json!({
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [command_entry("echo inline-hook", Some(30))] },
                { "hooks": [command_entry(&format!("bash {}", script.display()), None)] }
            ]
        }));
        let candidates = fixture.discover();
        confirm_hook_import(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ConfirmHookImportInput {
                tool: Tool::Codex,
                hooks: candidates.iter().map(to_create_input).collect(),
            },
        )
        .unwrap();

        // 原样重新检测：两条都已被中央库管理。
        let candidates = fixture.discover();
        assert!(candidates
            .iter()
            .all(|candidate| candidate.status == Status::AlreadyManaged));
        assert!(candidates.iter().all(|candidate| candidate.name.is_empty()));

        // 脚本型按内容哈希去重：同内容换路径仍视为已管理；内容变化则重新可导入。
        let moved = fixture.write_script("elsewhere/guard.sh", "#!/bin/sh\nexit 0\n");
        let changed = fixture.write_script("elsewhere/changed.sh", "#!/bin/sh\nexit 1\n");
        fixture.write_codex_hooks(json!({
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [command_entry("echo inline-hook", Some(31))] },
                { "hooks": [command_entry(&format!("bash {}", moved.display()), None)] },
                { "hooks": [command_entry(&format!("bash {}", changed.display()), None)] }
            ]
        }));
        let candidates = fixture.discover();
        assert_eq!(candidates.len(), 3);
        // 内联型按完整定义去重：超时变化即视为新定义。
        assert_eq!(candidates[0].status, Status::Importable);
        assert_eq!(candidates[1].status, Status::AlreadyManaged);
        assert_eq!(candidates[2].status, Status::Importable);
        // 同一轮内建议名去重，不与中央库已有名称冲突。
        assert_eq!(candidates[0].name, "PreToolUse-echo-2");
        assert_eq!(candidates[2].name, "PreToolUse-bash-2");
    }

    #[test]
    fn discover_flags_invalid_secret_unsupported_and_unresolvable_entries() {
        let mut fixture = Fixture::new();
        fixture.write_codex_hooks(json!({
            "PreToolUse": [
                { "hooks": [
                    { "type": "command" },
                    command_entry("curl -H 'Authorization: Bearer abcdef0123456789' https://x.test", None),
                    { "type": "prompt", "command": "echo prompt-type" },
                    command_entry("bash ./relative/missing.sh", None)
                ] }
            ],
            "Notification": [
                { "hooks": [command_entry("echo unsupported-for-codex", None)] }
            ],
            "CustomVendorEvent": [
                { "hooks": [command_entry("echo unknown-event", None)] }
            ]
        }));
        let candidates = fixture.discover();
        assert_eq!(candidates.len(), 6);
        // 原生行按事件分组扫描，输出顺序不保证与 JSON 书写顺序一致；按 command 定位。
        let find = |command: &str| {
            candidates
                .iter()
                .find(|candidate| candidate.command == command)
                .unwrap_or_else(|| panic!("缺少候选 {command}"))
        };

        let missing_command = find("");
        assert_eq!(missing_command.status, Status::Invalid);
        assert_eq!(
            missing_command.reason.as_deref(),
            Some("条目缺少 command，无法导入。")
        );

        let secret = find("curl -H 'Authorization: Bearer abcdef0123456789' https://x.test");
        assert_eq!(secret.status, Status::Invalid);
        assert_eq!(
            secret.reason.as_deref(),
            Some("条目包含可识别的凭据，不能导入。")
        );

        let wrong_type = find("echo prompt-type");
        assert_eq!(wrong_type.status, Status::Invalid);
        assert!(wrong_type
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("type prompt 暂不支持导入")));

        // 解释器命令但脚本不可解析：仍可导入，命令原样保存并附提示。
        let unresolvable = find("bash ./relative/missing.sh");
        assert_eq!(unresolvable.status, Status::Importable);
        assert!(!unresolvable.script_adopted);
        assert!(unresolvable.script_source_path.is_none());
        assert!(unresolvable
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("命令将原样保存")));

        // 统一模型内但 Codex 合同不支持的事件：发现阶段仍列出，确认阶段拒绝。
        let notification = find("echo unsupported-for-codex");
        assert_eq!(notification.status, Status::Importable);
        assert_eq!(notification.event, Some(HookEvent::Notification));
        let error = confirm_hook_import(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ConfirmHookImportInput {
                tool: Tool::Codex,
                hooks: vec![to_create_input(notification)],
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(hook_repository::list_hooks(&fixture.database)
            .unwrap()
            .is_empty());

        let unknown_event = find("echo unknown-event");
        assert_eq!(unknown_event.status, Status::UnsupportedEvent);
        assert!(unknown_event.event.is_none());
    }

    #[test]
    fn confirm_rejects_empty_selection_and_invalid_script_sources() {
        let mut fixture = Fixture::new();
        let error = confirm_hook_import(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ConfirmHookImportInput {
                tool: Tool::Codex,
                hooks: Vec::new(),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);

        // 声称接管的脚本不存在：fail-closed，不创建任何中央记录或中央脚本目录。
        let error = confirm_hook_import(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ConfirmHookImportInput {
                tool: Tool::Codex,
                hooks: vec![CreateHookInput {
                    name: "ghost".to_owned(),
                    event: HookEvent::PreToolUse,
                    matcher: None,
                    command: "bash /nonexistent/ghost.sh".to_owned(),
                    timeout_seconds: None,
                    enabled: true,
                    script_source_path: Some("/nonexistent/ghost.sh".to_owned()),
                }],
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(hook_repository::list_hooks(&fixture.database)
            .unwrap()
            .is_empty());
        assert_eq!(
            fs::read_dir(fixture.paths.central_hooks()).unwrap().count(),
            0
        );
    }
}
