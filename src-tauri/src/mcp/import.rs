//! 原生全局 MCP 的只读发现、显式选择与安全接管。

use std::collections::{BTreeMap, BTreeSet};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{
    service, ConfirmMcpImportInput, McpImportAction, McpImportCandidateDto,
    McpImportCandidateStatus as Status, McpImportPreviewDto, McpImportResultDto, McpServerInput,
    ValidatedMcpConfiguration,
};
use crate::{
    adapters::{
        CapabilityState, ExplicitEnvironment, ManagedOwnership, PolicyState, TargetDescriptor,
    },
    db::{mcp, mcp_imports as repository, Database},
    domain::{EntityId, McpTransport, Tool},
    error::{AppError, ErrorCode},
    security::{contains_detectable_secret, SecretRedactor},
    sync::{hash_json, scan_target, ManagedTargetBaseline, TargetScan},
};

#[derive(Deserialize, Serialize)]
struct CandidateEvidence {
    name: String,
    item_hash: String,
    reuse_id: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct ImportEvidence {
    descriptor: TargetDescriptor,
    database_fingerprint: String,
    candidates: BTreeMap<String, CandidateEvidence>,
}

struct NativeMcp {
    descriptor: TargetDescriptor,
    full_hash: Option<String>,
    items: Map<String, Value>,
}

pub fn discover_mcp_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &SecretRedactor,
    tool: Tool,
) -> Result<McpImportPreviewDto, AppError> {
    let native = read_native(environment, tool)?;
    let target_path = native.descriptor.path.clone().expect("扫描已验证目标路径");
    let fingerprint = repository::state_fingerprint(database.connection(), tool, &target_path)?;
    let records = mcp::list_mcp_servers(database)?;
    let baseline = service::find_mcp_target_baseline(database, &native.descriptor, None)?;
    let existing = baseline
        .as_ref()
        .map(|baseline| mcp::list_managed_mcp_items(database, &baseline.target_id))
        .transpose()?
        .unwrap_or_default();
    let mut local_redactor = redactor.clone();
    // 从私有中央记录恢复证据，不能依赖本进程是否已执行过 CRUD 或同步预览。
    for record in &records {
        service::register_configuration_secrets(
            &mut local_redactor,
            &service::configuration_from_record(record)?,
        );
    }
    register_native_secrets(&mut local_redactor, &native.items);
    let mut evidence = ImportEvidence {
        descriptor: native.descriptor.clone(),
        database_fingerprint: fingerprint.clone(),
        candidates: BTreeMap::new(),
    };
    let mut candidates = Vec::new();
    for (name, raw) in &native.items {
        let id = Uuid::new_v4().to_string();
        let mut candidate = McpImportCandidateDto {
            candidate_id: id.clone(),
            name: local_redactor.redact_text(name),
            transport: None,
            status: Status::Invalid,
            action: None,
            reason: None,
            redacted_projection: Value::Null,
        };
        let configuration = match parse_native_item(tool, name, raw) {
            Ok(value) if !local_redactor.contains_secret(name) => value,
            Ok(_) => {
                candidate.reason = Some("name 含已识别的凭据，不能导入。".to_owned());
                candidates.push(candidate);
                continue;
            }
            Err((status, reason)) => {
                candidate.status = status;
                candidate.reason = Some(reason);
                candidates.push(candidate);
                continue;
            }
        };
        candidate.transport = Some(configuration.transport);
        let ordinary_fields = configuration
            .command
            .iter()
            .map(|value| ("command", value))
            .chain(configuration.url.iter().map(|value| ("url", value)))
            .chain(configuration.args.iter().map(|value| ("args", value)));
        if let Some((field, _)) = ordinary_fields
            .into_iter()
            .find(|(_, value)| local_redactor.contains_secret(value))
        {
            candidate.reason = Some(format!("{field} 含已识别的凭据，请改用 env 或 headers。"));
            candidates.push(candidate);
            continue;
        }
        // 先对完整条目脱敏；拒绝项不会回传未经校验的普通字段。
        candidate.redacted_projection = local_redactor.redact_structure(raw).into_value();
        if existing.iter().any(|item| item.external_key == *name) {
            candidate.status = Status::AlreadyManaged;
            candidate.reason = Some("该工具中的条目已纳入管理，无需重复导入。".to_owned());
            candidates.push(candidate);
            continue;
        }
        if native
            .items
            .keys()
            .filter(|key| key.eq_ignore_ascii_case(name))
            .count()
            > 1
        {
            candidate.status = Status::NameConflict;
            candidate.reason =
                Some("原生配置存在仅大小写不同的名称，不能同时纳入中央库。".to_owned());
            candidates.push(candidate);
            continue;
        }
        let matching = records
            .iter()
            .find(|record| record.name.eq_ignore_ascii_case(name));
        let reuse_id = if let Some(record) = matching {
            if record.name != *name || service::configuration_from_record(record)? != configuration
            {
                candidate.status = Status::NameConflict;
                candidate.reason = Some(
                    "中央库已有同名但配置不同的 MCP，或名称仅大小写不同；不会覆盖。".to_owned(),
                );
                candidates.push(candidate);
                continue;
            }
            if repository::has_project_assignment(database, tool, &record.id)? {
                candidate.status = Status::NameConflict;
                candidate.reason =
                    Some("该 MCP 在来源工具中已有项目分配，不能重复分配为全局。".to_owned());
                candidates.push(candidate);
                continue;
            }
            Some(record.id.clone())
        } else {
            None
        };
        candidate.status = Status::Importable;
        candidate.action = Some(if reuse_id.is_some() {
            McpImportAction::Reuse
        } else {
            McpImportAction::Create
        });
        evidence.candidates.insert(
            id,
            CandidateEvidence {
                name: name.clone(),
                item_hash: hash_json(raw),
                reuse_id,
            },
        );
        candidates.push(candidate);
    }
    if repository::state_fingerprint(database.connection(), tool, &target_path)? != fingerprint {
        return Err(AppError::conflict(
            "import",
            "检测期间中央配置已变化，请重新检测",
        ));
    }
    let preview_id = (!evidence.candidates.is_empty()).then(|| Uuid::new_v4().to_string());
    let preview = McpImportPreviewDto {
        preview_id: preview_id.clone(),
        tool,
        target_path: target_path.clone(),
        candidates,
        message: if native.full_hash.is_none() {
            Some("未发现该工具的全局 MCP 配置文件。".to_owned())
        } else if native.items.is_empty() {
            Some("配置文件中没有全局 MCP 条目。".to_owned())
        } else {
            None
        },
    };
    if let Some(id) = preview_id {
        repository::persist_preview(
            database,
            &repository::McpImportPreviewRecord {
                id,
                tool,
                target_path,
                observed_full_hash: native.full_hash.expect("存在候选时必有原文件"),
                context_json: serialize_import(&evidence)?,
                redacted_preview_json: serialize_import(&preview)?,
                status: "previewed".to_owned(),
            },
        )?;
    }
    Ok(preview)
}

pub fn confirm_mcp_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &ConfirmMcpImportInput,
) -> Result<McpImportResultDto, AppError> {
    let preview = repository::get_preview(database, &input.preview_id)?;
    if preview.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &preview.id,
            &preview.status,
        ));
    }
    let evidence: ImportEvidence = serde_json::from_str(&preview.context_json)
        .map_err(|_| AppError::invalid_input("importPreview", "导入证据无效，请重新检测"))?;
    let selected = input.candidate_ids.iter().collect::<BTreeSet<_>>();
    if selected.is_empty()
        || selected.len() != input.candidate_ids.len()
        || selected
            .iter()
            .any(|id| !evidence.candidates.contains_key(*id))
    {
        return Err(AppError::invalid_input(
            "candidateIds",
            "请选择本次检测中的有效 MCP，且不能重复选择",
        ));
    }
    let native = read_native(environment, preview.tool)?;
    if native.descriptor != evidence.descriptor
        || native.descriptor.path.as_deref() != Some(preview.target_path.as_str())
        || native.full_hash.as_deref() != Some(preview.observed_full_hash.as_str())
        || repository::state_fingerprint(database.connection(), preview.tool, &preview.target_path)?
            != evidence.database_fingerprint
    {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let baseline = service::find_mcp_target_baseline(database, &native.descriptor, None)?;
    let mut owned = verified_existing_items(database, &native, baseline.as_ref())?;
    let records = mcp::list_mcp_servers(database)?;
    let mut imported = Vec::new();
    for id in selected {
        EntityId::parse(id)?;
        let candidate = &evidence.candidates[id];
        let raw = native
            .items
            .get(&candidate.name)
            .ok_or_else(|| AppError::stale_preview(&preview.id, &preview.target_path))?;
        if hash_json(raw) != candidate.item_hash || owned.contains_key(&candidate.name) {
            return Err(AppError::stale_preview(&preview.id, &preview.target_path));
        }
        let configuration = parse_native_item(preview.tool, &candidate.name, raw)
            .map_err(|_| AppError::stale_preview(&preview.id, &preview.target_path))?;
        if let Some(reuse_id) = &candidate.reuse_id {
            let record = records
                .iter()
                .find(|record| &record.id == reuse_id)
                .ok_or_else(|| AppError::stale_preview(&preview.id, &preview.target_path))?;
            if service::configuration_from_record(record)? != configuration {
                return Err(AppError::stale_preview(&preview.id, &preview.target_path));
            }
        }
        owned.insert(candidate.name.clone(), raw.clone());
        imported.push(repository::ImportedMcpItem {
            configuration,
            reuse_id: candidate.reuse_id.clone(),
            item_hash: candidate.item_hash.clone(),
        });
    }
    let projection = item_projection(preview.tool, owned);
    let result = repository::adopt_import(
        database,
        &preview,
        &evidence.database_fingerprint,
        baseline.as_ref(),
        &projection,
        &imported,
        || {
            let current = read_native(environment, preview.tool)?;
            if current.descriptor != evidence.descriptor
                || current.full_hash.as_deref() != Some(preview.observed_full_hash.as_str())
            {
                return Err(AppError::stale_preview(&preview.id, &preview.target_path));
            }
            Ok(())
        },
    )?;
    for item in &imported {
        service::register_configuration_secrets(redactor, &item.configuration);
    }
    Ok(result)
}

fn verified_existing_items(
    database: &Database,
    native: &NativeMcp,
    baseline: Option<&ManagedTargetBaseline>,
) -> Result<Map<String, Value>, AppError> {
    let mut owned = Map::new();
    let Some(baseline) = baseline else {
        return Ok(owned);
    };
    let drift = || {
        AppError::conflict(
            "import",
            "已有受管 MCP 的基线或内容发生变化，请先处理同步冲突",
        )
    };
    for item in mcp::list_managed_mcp_items(database, &baseline.target_id)? {
        let raw = native.items.get(&item.external_key).ok_or_else(drift)?;
        if hash_json(raw) != item.last_applied_item_hash {
            return Err(drift());
        }
        owned.insert(item.external_key, raw.clone());
    }
    match (&baseline.full_hash, &baseline.managed_hash) {
        (None, None) if owned.is_empty() => {}
        (Some(_), Some(hash))
            if *hash == hash_json(&item_projection(native.descriptor.tool, owned.clone())) => {}
        _ => return Err(drift()),
    }
    Ok(owned)
}

fn item_projection(tool: Tool, items: Map<String, Value>) -> Value {
    if items.is_empty() {
        json!({})
    } else {
        json!({service::native_container(tool): items})
    }
}

fn read_native(environment: &ExplicitEnvironment, tool: Tool) -> Result<NativeMcp, AppError> {
    let descriptor = service::descriptor_for(
        environment,
        tool,
        None,
        environment.claude_user_mcp_probe(),
        environment.claude_customization_policy_probe(),
    )?;
    if descriptor.capability.state != CapabilityState::Supported {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "工具不可用或全局 MCP 路径未经确认，不能导入",
            true,
        ));
    }
    let path = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::not_found("mcpTarget", tool.as_str()))?;
    if descriptor.policy != PolicyState::Allowed {
        return Err(AppError::policy_blocked(
            tool.as_str(),
            path,
            if descriptor.policy == PolicyState::Unknown {
                "CLAUDE_POLICY_UNKNOWN"
            } else {
                "CLAUDE_POLICY_BLOCKED"
            },
        ));
    }
    let container = service::native_container(tool);
    let scan = scan_target(
        service::tool_adapter(tool),
        &descriptor,
        &ManagedOwnership::selectors([[container]]),
    );
    match scan {
        TargetScan::Missing => Ok(NativeMcp {
            descriptor,
            full_hash: None,
            items: Map::new(),
        }),
        TargetScan::Observed(observed) => {
            let items = match observed.managed_projection.get(container) {
                None => Map::new(),
                Some(Value::Object(items)) => items.clone(),
                Some(_) => return Err(AppError::parse(path, "MCP")),
            };
            Ok(NativeMcp {
                descriptor,
                full_hash: Some(observed.full_hash),
                items,
            })
        }
        TargetScan::ParseError => Err(AppError::parse(path, "MCP")),
        TargetScan::PermissionDenied => Err(AppError::new(
            ErrorCode::PermissionDenied,
            "无法安全读取全局 MCP 配置",
            true,
        )),
        TargetScan::TargetTypeChanged(_) => Err(AppError::conflict(
            "path",
            "全局 MCP 路径或祖先不是安全的普通文件/目录",
        )),
        _ => Err(AppError::conflict(
            "import",
            "无法安全检测全局 MCP 配置，请重新检测",
        )),
    }
}

type CandidateError = (Status, String);

fn parse_native_item(
    tool: Tool,
    name: &str,
    raw: &Value,
) -> Result<ValidatedMcpConfiguration, CandidateError> {
    let mut object = raw.as_object().cloned().ok_or_else(|| {
        (
            Status::Invalid,
            "MCP 条目必须是字段对象，不能是标量或数组。".to_owned(),
        )
    })?;
    let enabled: Option<bool> = take_optional(&mut object, "enabled")?;
    let disabled: Option<bool> = take_optional(&mut object, "disabled")?;
    if enabled == Some(false) || disabled == Some(true) {
        return Err((
            Status::Disabled,
            "原生条目已停用，本次仅展示，不启用或接管。".to_owned(),
        ));
    }
    let native_type: Option<String> = take_optional(&mut object, "type")?;
    if object.contains_key("env_http_headers") {
        return Err((
            Status::Unsupported,
            "env_http_headers 环境变量引用暂不能保真导入，原配置保持不变。".to_owned(),
        ));
    }
    let command: Option<String> = take_optional(&mut object, "command")?;
    let url: Option<String> = take_optional(&mut object, "url")?;
    let transport = match (
        native_type.as_deref(),
        command.is_some(),
        url.is_some(),
    ) {
        (None | Some("stdio"), true, false) => McpTransport::Stdio,
        (None | Some("http" | "streamable_http"), false, true) => {
            McpTransport::StreamableHttp
        }
        (Some(kind), _, _) if !matches!(kind, "stdio" | "http" | "streamable_http") => {
            return Err((
                Status::Unsupported,
                "type 协议暂不支持；仅支持 stdio、http 和 streamable_http。".to_owned(),
            ))
        }
        _ => return Err((Status::Invalid,
            "type 与 command/url 不匹配：stdio 仅允许 command，HTTP 仅允许 url，不能同时填写或同时缺失。".to_owned())),
    };
    for (field, value) in
        std::iter::once(("name", name)).chain(command.as_deref().map(|value| ("command", value)))
    {
        if contains_detectable_secret(field, value) {
            return Err((
                Status::Invalid,
                format!("{field} 含可识别的凭据，请使用 env 或 headers。"),
            ));
        }
    }
    let input = McpServerInput {
        name: name.to_owned(),
        transport,
        command,
        url,
        args: take_optional(&mut object, "args")?.unwrap_or_default(),
        env: take_optional(&mut object, "env")?.unwrap_or_default(),
        headers: take_optional(
            &mut object,
            if tool == Tool::Claude {
                "headers"
            } else {
                "http_headers"
            },
        )?
        .unwrap_or_default(),
        extra: Value::Object(object),
        enabled: true,
    };
    ValidatedMcpConfiguration::from_create(&input).map_err(|error| {
        // 中央校验的 INVALID_INPUT 仅含编译期固定 field/reason，不拼接原生值。
        let details = error.details();
        let field = details
            .and_then(|details| details.get("field"))
            .and_then(Value::as_str);
        let reason = details
            .and_then(|details| details.get("reason"))
            .and_then(Value::as_str);
        match (error.code(), field, reason) {
            (ErrorCode::InvalidInput, Some(field), Some(reason))
                if matches!(
                    field,
                    "name" | "command" | "args" | "url" | "headers" | "env" | "extra" | "transport"
                ) =>
            {
                (Status::Invalid, format!("{field}：{reason}。"))
            }
            _ => (Status::Invalid, "条目未通过中央 MCP 配置校验。".to_owned()),
        }
    })
}

fn take_optional<T: DeserializeOwned>(
    object: &mut Map<String, Value>,
    key: &'static str,
) -> Result<Option<T>, CandidateError> {
    object
        .remove(key)
        .map(|value| {
            serde_json::from_value(value).map_err(|_| {
                let expected = match key {
                    "enabled" | "disabled" => "布尔值",
                    "args" => "字符串数组",
                    "env" | "headers" | "http_headers" => "字符串映射",
                    _ => "字符串",
                };
                (
                    Status::Invalid,
                    format!("{key} 必须是{expected}，不能为 null 或其它类型。"),
                )
            })
        })
        .transpose()
}

fn register_native_secrets(redactor: &mut SecretRedactor, items: &Map<String, Value>) {
    for raw in items.values() {
        for key in ["headers", "http_headers", "env_http_headers", "env"] {
            if let Some(values) = raw.get(key).and_then(Value::as_object) {
                for (name, value) in values {
                    if let Some(value) = value.as_str() {
                        if key == "env" {
                            service::register_environment_value(redactor, name, value);
                        } else {
                            redactor.register_secret(value);
                        }
                    }
                }
            }
        }
        service::register_detectable_extra_secrets(redactor, None, raw);
    }
}

fn serialize_import(value: &impl Serialize) -> Result<String, AppError> {
    serde_json::to_string(value)
        .map_err(|_| AppError::invalid_input("importPreview", "导入预览无法序列化"))
}
