//! 原生全局 Hooks 的只读发现与显式导入（仅创建中央记录，不接管基线）。
//!
//! 与 MCP 导入的差异：hooks 的原生条目是匿名数组元素（无稳定名称键），
//! 无法像 MCP 那样在导入事务内精确登记 per-item 基线。因此导入只把原生
//! 定义转换为中央记录；用户随后通过常规分配 + 预览/Apply 进入受管状态
//! （内容一致时 Apply 近似 no-op，无删除风险）。

use std::collections::BTreeSet;

use serde_json::Value;

use super::{
    service, ConfirmHookImportInput, DiscoverHookImportInput, HookImportCandidateDto,
    HookImportCandidateStatus as Status, HookImportPreviewDto, HookImportResultDto,
};
use crate::{
    adapters::{CapabilityState, ExplicitEnvironment, PolicyState, TargetDescriptor},
    db::Database,
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
            let rows = native_hook_rows(&observed, tool);
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

    let existing = service::list_hooks(database)?;
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
        if existing.iter().any(|hook| {
            hook.event == event
                && hook.matcher == candidate.matcher
                && hook.command == command
                && hook.timeout_seconds == timeout_seconds
        }) {
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
        service::create_hook(database, hook)?;
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

/// 拍平原生投影为 `(原生事件键, matcher, 条目)` 列表；Cursor 的 matcher 属于
/// 条目本身，其余工具属于 matcher 组。
fn native_hook_rows(
    observed: &crate::sync::ObservedTarget,
    tool: Tool,
) -> Vec<(String, String, Value)> {
    let mut rows = Vec::new();
    let Some(events) =
        service::projection_value_at(&observed.managed_projection, service::events_root(tool))
    else {
        return rows;
    };
    for (native_event, groups) in events.as_object().into_iter().flatten() {
        for group in groups.as_array().into_iter().flatten() {
            let group_matcher = group.get("matcher").and_then(Value::as_str);
            let items: &[Value] = match group.get("hooks").and_then(Value::as_array) {
                Some(hooks) => hooks,
                None => std::slice::from_ref(group),
            };
            for hook in items {
                let matcher = hook
                    .get("matcher")
                    .and_then(Value::as_str)
                    .or(group_matcher)
                    .unwrap_or_default()
                    .to_owned();
                rows.push((native_event.clone(), matcher, hook.clone()));
            }
        }
    }
    rows
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
