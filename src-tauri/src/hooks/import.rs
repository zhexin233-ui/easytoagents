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
    let descriptor = service::descriptor_for(environment, tool, None)?;
    let target_path = descriptor
        .path
        .clone()
        .ok_or_else(|| AppError::not_found("hookTarget", tool.as_str()))?;
    ensure_readable(&descriptor, &target_path)?;
    let scan = scan_target(
        service_tool_adapter(tool),
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
    let descriptor = service::descriptor_for(environment, input.tool, None)?;
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

fn service_tool_adapter(tool: Tool) -> &'static dyn crate::adapters::ToolAdapter {
    service::tool_adapter(tool)
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
        .and_then(|words| words.first().cloned())
        .map(|first| {
            let basename = first.rsplit(['/', '\\']).next().unwrap_or_default();
            service::HOOK_INTERPRETERS.contains(&basename)
        })
        .unwrap_or(false)
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
