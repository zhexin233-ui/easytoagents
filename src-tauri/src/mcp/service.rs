//! MCP 领域服务、敏感 DTO 投影与持久化 Preview/Apply 编排。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension};
use serde_json::{Map, Value};
use uuid::Uuid;

use super::{
    ApplyMcpPreviewInput, DeleteMcpResultDto, McpProjectDto, McpProjectOptionDto,
    McpProjectOptionsInput, McpProjectSelectionState, McpServerDto, McpServerInput,
    McpTargetStatusDto, PreviewMcpSyncInput, ReadoptMcpTargetInput, ReadoptMcpTargetResultDto,
    SetGlobalMcpAssignmentInput, SetProjectMcpAssignmentInput, UpdateMcpServerInput,
    ValidatedMcpConfiguration, VersionedMcpInput,
};
use crate::{
    adapters::{
        canonicalize_project_root, descriptor_allowed_root, descriptor_mcp_container,
        find_descriptor, native_mcp_container, projection_value_at, ClaudeCustomizationPolicyProbe,
        ClaudeUserMcpCapabilityProbe, DiscoveryContext, ManagedOwnership, TargetDescriptor,
        ASSIGNABLE_MCP_TOOLS,
    },
    app::AppPaths,
    db::{
        mcp::{self as repository, ManagedMcpItemRecord, McpProjectRecord, McpServerRecord},
        Database,
    },
    domain::{ArtifactKind, McpTransport, ProjectRoot, Scope, SyncStatus, Tool},
    error::AppError,
    git::inspect_path,
    security::{contains_detectable_secret, SecretRedactor},
    sync::managed::{
        collect_row_versions, list_global_target_statuses, project_dto as managed_project_dto,
        readopt_with_scan, McpManagedArtifact,
    },
    sync::{
        apply_persisted_preview, build_preview_plan, hash_json, load_managed_target_baseline,
        load_persisted_preview, persist_preview, safe_row_version, scan_target, ApplyResult,
        ApplyTargetInput, DatabaseRowVersion, ManagedItemApply, ManagedTargetBaseline,
        NoApplyFault, PreviewPlan, PreviewTargetRequest, TargetScan,
    },
};

pub fn list_mcp_servers(
    database: &Database,
    redactor: &SecretRedactor,
) -> Result<Vec<McpServerDto>, AppError> {
    // 列表只发两条 SQL（记录 + 全部全局分配），逐条组装不再回库。
    let mut global_tools = repository::global_tools_for_all_mcp(database)?;
    repository::list_mcp_servers(database)?
        .iter()
        .map(|record| {
            mcp_dto_with_tools(
                record,
                redactor,
                global_tools.remove(&record.id).unwrap_or_default(),
            )
        })
        .collect()
}

pub fn get_mcp_server(
    database: &Database,
    redactor: &SecretRedactor,
    id: &str,
) -> Result<McpServerDto, AppError> {
    let record = repository::get_mcp_server(database, id)?;
    mcp_dto(database, &record, redactor)
}

pub fn create_mcp_server(
    database: &mut Database,
    redactor: &mut SecretRedactor,
    input: &McpServerInput,
) -> Result<McpServerDto, AppError> {
    let value = ValidatedMcpConfiguration::from_create(input)?;
    register_configuration_secrets(redactor, &value);
    let record = repository::insert_mcp_server(database, &value)?;
    mcp_dto(database, &record, redactor)
}

pub fn update_mcp_server(
    database: &mut Database,
    redactor: &mut SecretRedactor,
    input: &UpdateMcpServerInput,
) -> Result<McpServerDto, AppError> {
    let current = repository::get_mcp_server(database, &input.id)?;
    let current_value = configuration_from_record(&current, redactor)?;
    let value = ValidatedMcpConfiguration::from_update(
        input,
        &current_value.headers,
        &current_value.env,
        &current_value.extra,
    )?;
    register_configuration_secrets(redactor, &value);
    let record = repository::update_mcp_server(database, &input.id, input.row_version, &value)?;
    mcp_dto(database, &record, redactor)
}

pub fn set_mcp_enabled(
    database: &mut Database,
    redactor: &SecretRedactor,
    input: &VersionedMcpInput,
    enabled: bool,
) -> Result<McpServerDto, AppError> {
    let record = repository::set_mcp_enabled(database, &input.id, input.row_version, enabled)?;
    mcp_dto(database, &record, redactor)
}

pub fn delete_mcp_server(
    database: &mut Database,
    input: &VersionedMcpInput,
) -> Result<DeleteMcpResultDto, AppError> {
    repository::delete_mcp_server(database, &input.id, input.row_version)?;
    Ok(DeleteMcpResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

pub fn set_global_mcp_assignment(
    database: &mut Database,
    redactor: &SecretRedactor,
    input: &SetGlobalMcpAssignmentInput,
) -> Result<McpServerDto, AppError> {
    let record = repository::set_global_assignment(
        database,
        input.tool,
        &input.mcp_id,
        input.assigned,
        input.row_version,
    )?;
    mcp_dto(database, &record, redactor)
}

pub fn set_project_mcp_assignment(
    database: &mut Database,
    redactor: &SecretRedactor,
    input: &SetProjectMcpAssignmentInput,
) -> Result<McpServerDto, AppError> {
    let record = repository::set_project_assignment(
        database,
        &input.project_id,
        input.tool,
        &input.mcp_id,
        input.assigned,
        input.mcp_row_version,
        input.project_row_version,
    )?;
    mcp_dto(database, &record, redactor)
}

pub fn list_mcp_projects(database: &Database) -> Result<Vec<McpProjectDto>, AppError> {
    repository::list_projects(database)?
        .iter()
        .map(managed_project_dto::<McpProjectRecord>)
        .map(|result| {
            result.map(|value| McpProjectDto {
                id: value.id,
                display_name: value.display_name,
                root_path: value.root_path,
                codex_trust_status: value.codex_trust_status,
                row_version: value.row_version,
            })
        })
        .collect()
}

pub fn list_mcp_project_options(
    database: &Database,
    input: &McpProjectOptionsInput,
) -> Result<Vec<McpProjectOptionDto>, AppError> {
    repository::get_project(database, &input.project_id)?;
    let global = repository::list_assigned_mcp_servers(database, input.tool, None)?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    let selected =
        repository::list_assigned_mcp_servers(database, input.tool, Some(&input.project_id))?
            .into_iter()
            .map(|record| record.id)
            .collect::<BTreeSet<_>>();
    repository::list_mcp_servers(database)?
        .into_iter()
        .map(|record| {
            let state = if global.contains(&record.id) {
                McpProjectSelectionState::Inherited
            } else if selected.contains(&record.id) {
                McpProjectSelectionState::Selected
            } else {
                McpProjectSelectionState::Available
            };
            Ok(McpProjectOptionDto {
                mcp_id: record.id,
                name: record.name,
                enabled: record.enabled,
                state,
                selectable: state != McpProjectSelectionState::Inherited,
                row_version: safe_row_version(record.row_version)?,
            })
        })
        .collect()
}

pub fn preview_mcp_sync(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &PreviewMcpSyncInput,
) -> Result<PreviewPlan, AppError> {
    preview_mcp_sync_with_probes(
        database,
        environment,
        redactor,
        input,
        environment.claude_user_mcp_probe(),
        environment.claude_customization_policy_probe(),
    )
}

pub fn preview_mcp_sync_with_probes(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &PreviewMcpSyncInput,
    user_probe: &dyn ClaudeUserMcpCapabilityProbe,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<PreviewPlan, AppError> {
    let prepared = prepare_mcp_sync(
        database,
        environment,
        redactor,
        input,
        user_probe,
        policy_probe,
    )?;
    let scope = prepared.scope;
    let project_id = prepared.project.as_ref().map(|project| project.id.clone());
    let requests = prepared
        .target
        .map(|target| {
            vec![PreviewTargetRequest {
                descriptor: target.descriptor,
                ownership: target.ownership,
                baseline: target.baseline,
                scan: target.scan,
                baseline_mismatched_items: target.baseline_mismatched_items,
                readopt_available: target.readopt_available,
                desired_projection: target.desired_projection,
                row_versions: target.row_versions,
                git: target.git,
                exclude_from_git: input.exclude_from_git,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
                hook_initial_adopt: false,
            }]
        })
        .unwrap_or_default();
    let plan = build_preview_plan(scope, project_id, requests, redactor)?;
    persist_preview(database, &plan)?;
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_mcp_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &ApplyMcpPreviewInput,
) -> Result<ApplyResult, AppError> {
    apply_mcp_preview_with_probes(
        write_operations,
        database,
        paths,
        environment,
        redactor,
        input,
        environment.claude_user_mcp_probe(),
        environment.claude_customization_policy_probe(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn apply_mcp_preview_with_probes(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &ApplyMcpPreviewInput,
    user_probe: &dyn ClaudeUserMcpCapabilityProbe,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<ApplyResult, AppError> {
    let persisted = load_persisted_preview(database, &input.preview_id)?;
    let preview_input = PreviewMcpSyncInput {
        tool: input.tool,
        project_id: input.project_id.clone(),
        exclude_from_git: persisted
            .items
            .first()
            .is_some_and(|item| item.envelope.exclude_from_git),
    };
    let prepared = prepare_mcp_sync(
        database,
        environment,
        redactor,
        &preview_input,
        user_probe,
        policy_probe,
    )?;
    if persisted.scope != prepared.scope
        || persisted.project_id != input.project_id
        || persisted.items.iter().any(|item| {
            item.envelope.descriptor.tool != input.tool
                || item.envelope.descriptor.artifact_kind != ArtifactKind::Mcp
        })
    {
        return Err(AppError::stale_preview(&input.preview_id, "mcpTarget"));
    }
    let apply_inputs = prepared
        .target
        .map(|target| {
            vec![ApplyTargetInput {
                descriptor: target.descriptor,
                ownership: target.ownership,
                desired_projection: target.desired_projection,
                allowed_root: target.allowed_root,
                central_skills_root: None,
                delete_target: false,
                managed_items: target.managed_items,
                remove_managed_item_ids: target.remove_managed_item_ids,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }]
        })
        .unwrap_or_default();
    if persisted.items.len() != apply_inputs.len() {
        return Err(AppError::stale_preview(&input.preview_id, "mcpTargets"));
    }
    apply_persisted_preview(
        write_operations,
        database,
        paths,
        &input.preview_id,
        &apply_inputs,
        &NoApplyFault,
    )
}

/// 以当前磁盘内容重新接管受管目标：仅刷新目标级与条目级基线，解除
/// 「外部改写受管内容」类冲突。不改中央意图，也不写原生文件；下一次
/// 预览会基于新基线通过校验，Apply 时按中央意图重新写入受管内容。
pub fn readopt_mcp_target(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &ReadoptMcpTargetInput,
) -> Result<ReadoptMcpTargetResultDto, AppError> {
    let project = input
        .project_id
        .as_deref()
        .map(|id| repository::get_project(database, id))
        .transpose()?;
    let project_root = project
        .as_ref()
        .map(|project| canonical_project(&project.root_path))
        .transpose()?;
    let descriptor = mcp_target_descriptor(
        environment,
        input.tool,
        project_root.as_ref(),
        environment.claude_user_mcp_probe(),
        environment.claude_customization_policy_probe(),
    )?;
    let baseline = find_mcp_target_baseline(
        database,
        &descriptor,
        project.as_ref().map(|project| project.id.as_str()),
    )?
    .ok_or_else(|| AppError::not_found("managedTarget", "该目标尚未纳入受管基线，无需重新接管"))?;
    // 基线刷新必须与下一次预览使用同一套 ownership 口径，否则目标级
    // managed hash 会再次判定为外部改写。
    let container = descriptor_mcp_container(&descriptor)?;
    let desired_records = repository::list_assigned_mcp_servers(
        database,
        input.tool,
        project.as_ref().map(|project| project.id.as_str()),
    )?
    .into_iter()
    .filter(|record| record.enabled)
    .collect::<Vec<_>>();
    let inherited_records = if project.is_some() {
        repository::list_assigned_mcp_servers(database, input.tool, None)?
            .into_iter()
            .filter(|record| record.enabled)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let existing_items = repository::list_managed_mcp_items(database, &baseline.target_id)?;
    let ownership = build_mcp_ownership(
        &container,
        &desired_records,
        &inherited_records,
        &existing_items,
    );
    let scan = scan_target(input.tool.adapter(), &descriptor, &ownership);
    let outcome = readopt_with_scan::<McpManagedArtifact>(
        database,
        &baseline,
        &existing_items,
        &scan,
        &descriptor,
    )?;
    Ok(ReadoptMcpTargetResultDto {
        target_path: outcome.target_path,
        updated_item_count: outcome.updated_item_count,
        removed_item_count: outcome.removed_item_count,
    })
}

pub fn list_global_mcp_target_statuses(
    database: &Database,
    environment: &crate::adapters::ExplicitEnvironment,
) -> Result<Vec<McpTargetStatusDto>, AppError> {
    list_global_target_statuses::<McpManagedArtifact, _, _>(
        database,
        ASSIGNABLE_MCP_TOOLS,
        |tool| {
            mcp_target_descriptor(
                environment,
                tool,
                None,
                environment.claude_user_mcp_probe(),
                environment.claude_customization_policy_probe(),
            )
        },
        |database, tool, descriptor| {
            descriptor
                .path
                .as_deref()
                .map(|path| load_target_status(database, tool, None, path))
                .transpose()
                .map(|status| status.flatten().map(|status| (status, None)))
        },
    )
    .map(|statuses| {
        statuses
            .into_iter()
            .map(|value| McpTargetStatusDto {
                tool: value.tool,
                project_id: value.project_id,
                target_path: value.target_path,
                status: value.status,
                diagnostic_code: value.diagnostic_code,
            })
            .collect()
    })
}

struct PreparedMcpSync {
    scope: Scope,
    project: Option<McpProjectRecord>,
    target: Option<PreparedMcpTarget>,
}

struct PreparedMcpTarget {
    descriptor: TargetDescriptor,
    ownership: ManagedOwnership,
    baseline: ManagedTargetBaseline,
    scan: TargetScan,
    baseline_mismatched_items: Vec<String>,
    readopt_available: bool,
    desired_projection: Value,
    row_versions: Vec<DatabaseRowVersion>,
    git: Option<crate::git::GitPathStatus>,
    allowed_root: PathBuf,
    managed_items: Vec<ManagedItemApply>,
    remove_managed_item_ids: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn prepare_mcp_sync(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &PreviewMcpSyncInput,
    user_probe: &dyn ClaudeUserMcpCapabilityProbe,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<PreparedMcpSync, AppError> {
    let project = input
        .project_id
        .as_deref()
        .map(|id| repository::get_project(database, id))
        .transpose()?;
    let scope = if project.is_some() {
        Scope::Project
    } else {
        Scope::Global
    };
    let project_root = project
        .as_ref()
        .map(|project| canonical_project(&project.root_path))
        .transpose()?;
    let descriptor = mcp_target_descriptor(
        environment,
        input.tool,
        project_root.as_ref(),
        user_probe,
        policy_probe,
    )?;
    let desired_records = repository::list_assigned_mcp_servers(
        database,
        input.tool,
        project.as_ref().map(|project| project.id.as_str()),
    )?
    .into_iter()
    .filter(|record| record.enabled)
    .collect::<Vec<_>>();
    let inherited_records = if scope == Scope::Project {
        repository::list_assigned_mcp_servers(database, input.tool, None)?
            .into_iter()
            .filter(|record| record.enabled)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let existing_baseline = find_mcp_target_baseline(
        database,
        &descriptor,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    if desired_records.is_empty() && inherited_records.is_empty() && existing_baseline.is_none() {
        return Ok(PreparedMcpSync {
            scope,
            project,
            target: None,
        });
    }
    // 项目层只有全局继承项时，先以只读扫描确认是否存在外部同名条目。
    // 没有碰撞就不创建 managed_targets 行，也不生成空的原生配置文件。
    if desired_records.is_empty() && existing_baseline.is_none() {
        let container = native_container(input.tool);
        let ownership = build_mcp_ownership(container, &[], &inherited_records, &[]);
        let scan = scan_target(input.tool.adapter(), &descriptor, &ownership);
        if inherited_projection_is_absent(&scan, container) {
            return Ok(PreparedMcpSync {
                scope,
                project,
                target: None,
            });
        }
    }
    let baseline = match existing_baseline {
        Some(baseline) => baseline,
        None => ensure_mcp_target(database, &descriptor, project.as_ref())?,
    };
    let existing_items = repository::list_managed_mcp_items(database, &baseline.target_id)?;
    if desired_records.is_empty() && inherited_records.is_empty() && existing_items.is_empty() {
        return Ok(PreparedMcpSync {
            scope,
            project,
            target: None,
        });
    }

    let container = native_container(input.tool);
    let desired = build_desired_projection(input.tool, container, &desired_records, redactor)?;
    let ownership = build_mcp_ownership(
        container,
        &desired_records,
        &inherited_records,
        &existing_items,
    );
    let (scan, baseline_mismatched_items) = verify_managed_item_baselines(
        scan_target(input.tool.adapter(), &descriptor, &ownership),
        container,
        &existing_items,
    );
    // 重新接管只对「外部改写了受管内容」这一类冲突有意义；策略、信任、解析失败
    // 等其他阻塞状态必须走各自的恢复路径。
    let readopt_available = crate::sync::assess_drift(&descriptor, &baseline, &scan).status
        == SyncStatus::ExternalOwnedChange;
    // 项目层只有全局继承项时不拥有任何原生条目。仍扫描继承名称以发现外部同名
    // 冲突，但在目标缺失或这些名称均不存在时，不生成空 `.mcp.json`/TOML 写入。
    if desired_records.is_empty()
        && existing_items.is_empty()
        && inherited_projection_is_absent(&scan, container)
    {
        return Ok(PreparedMcpSync {
            scope,
            project,
            target: None,
        });
    }
    let (managed_items, remove_managed_item_ids) = build_managed_item_changes(
        input.tool,
        container,
        &desired_records,
        &existing_items,
        redactor,
    )?;
    let row_versions = collect_row_versions::<McpManagedArtifact>(
        database.connection(),
        &database.path().to_string_lossy(),
        project
            .as_ref()
            .map(|project| (project.id.as_str(), project.row_version)),
        desired_records.iter().chain(inherited_records.iter()),
        &existing_items,
    )?;
    let git = project_root
        .as_ref()
        .zip(descriptor.path.as_deref())
        .map(|(root, path)| inspect_path(root, Path::new(path)))
        .transpose()?;
    let allowed_root = descriptor_allowed_root(&descriptor)?;
    Ok(PreparedMcpSync {
        scope,
        project,
        target: Some(PreparedMcpTarget {
            descriptor,
            ownership,
            baseline,
            scan,
            baseline_mismatched_items,
            readopt_available,
            desired_projection: desired,
            row_versions,
            git,
            allowed_root,
            managed_items,
            remove_managed_item_ids,
        }),
    })
}

fn inherited_projection_is_absent(scan: &TargetScan, container: &[&str]) -> bool {
    match scan {
        TargetScan::Missing => true,
        TargetScan::Observed(observed) => {
            match projection_value_at(&observed.managed_projection, container)
                .and_then(Value::as_object)
            {
                Some(items) => items.is_empty(),
                None => true,
            }
        }
        TargetScan::ManagedItemBaselineMismatch
        | TargetScan::ParseError
        | TargetScan::PermissionDenied
        | TargetScan::TargetTypeChanged(_)
        | TargetScan::Unavailable
        | TargetScan::Failed => false,
    }
}

pub(super) fn mcp_target_descriptor(
    environment: &crate::adapters::ExplicitEnvironment,
    tool: Tool,
    project_root: Option<&ProjectRoot>,
    user_probe: &dyn ClaudeUserMcpCapabilityProbe,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<TargetDescriptor, AppError> {
    let context = DiscoveryContext {
        environment,
        project_root,
        claude_user_mcp_probe: user_probe,
        claude_customization_policy_probe: policy_probe,
    };
    find_descriptor(
        tool,
        &context,
        ArtifactKind::Mcp,
        if project_root.is_some() {
            Scope::Project
        } else {
            Scope::Global
        },
        "mcpTarget",
        tool.as_str(),
    )
}

/// MCP 条目在原生文件中的容器路径；ZCode 是官方定义的嵌套键 `mcp.servers`。
pub(super) fn native_container(tool: Tool) -> &'static [&'static str] {
    native_mcp_container(tool)
}

#[cfg(test)]
pub(super) fn service_projection_get<'a>(value: &'a Value, path: &[&str]) -> &'a Value {
    projection_value_at(value, path).unwrap_or(&Value::Null)
}

fn nest_at(path: &[&str], leaf: Value) -> Value {
    path.iter().rev().fold(leaf, |child, segment| {
        Value::Object(Map::from_iter([(segment.to_string(), child)]))
    })
}

fn build_desired_projection(
    tool: Tool,
    container: &[&str],
    records: &[McpServerRecord],
    redactor: &mut SecretRedactor,
) -> Result<Value, AppError> {
    let mut servers = Map::new();
    for record in records {
        let configuration = configuration_from_record(record, redactor)?;
        register_configuration_secrets(redactor, &configuration);
        servers.insert(record.name.clone(), native_mcp_item(tool, &configuration)?);
    }
    if servers.is_empty() {
        Ok(Value::Object(Map::new()))
    } else {
        Ok(nest_at(container, Value::Object(servers)))
    }
}

fn stdio_command(value: &ValidatedMcpConfiguration) -> Result<String, AppError> {
    value
        .command
        .clone()
        .ok_or_else(|| AppError::internal("stdio MCP 配置缺少已验证的 command"))
}

fn http_url(value: &ValidatedMcpConfiguration) -> Result<String, AppError> {
    value
        .url
        .clone()
        .ok_or_else(|| AppError::internal("streamable_http MCP 配置缺少已验证的 url"))
}

/// 把 env/headers 这类字符串映射投影为 JSON 对象。`BTreeMap<String, String>`
/// 序列化实际不会失败，但生产路径不允许 `unwrap`；错误经 `with_source` 脱敏后
/// 只保留内部原因，不把映射内容带出 RPC 边界。
fn string_map_value(map: &BTreeMap<String, String>) -> Result<Value, AppError> {
    serde_json::to_value(map)
        .map_err(|error| AppError::internal("MCP 字符串映射序列化失败").with_source(error))
}

fn native_mcp_item(tool: Tool, value: &ValidatedMcpConfiguration) -> Result<Value, AppError> {
    let mut object = value
        .extra
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::invalid_input("extra", "MCP 扩展字段必须是对象"))?;
    match (tool, value.transport) {
        (Tool::Claude | Tool::Cursor | Tool::Zcode, McpTransport::Stdio) => {
            object.insert("type".to_owned(), Value::String("stdio".to_owned()));
            object.insert("command".to_owned(), Value::String(stdio_command(value)?));
            if !value.args.is_empty() {
                object.insert(
                    "args".to_owned(),
                    Value::Array(value.args.iter().cloned().map(Value::String).collect()),
                );
            }
            if !value.env.is_empty() {
                object.insert("env".to_owned(), string_map_value(&value.env)?);
            }
        }
        (Tool::Claude | Tool::Cursor | Tool::Zcode, McpTransport::StreamableHttp) => {
            object.insert("type".to_owned(), Value::String("http".to_owned()));
            object.insert("url".to_owned(), Value::String(http_url(value)?));
            if !value.headers.is_empty() {
                object.insert("headers".to_owned(), string_map_value(&value.headers)?);
            }
        }
        (Tool::Codex, McpTransport::Stdio) => {
            object.insert("command".to_owned(), Value::String(stdio_command(value)?));
            if !value.args.is_empty() {
                object.insert(
                    "args".to_owned(),
                    Value::Array(value.args.iter().cloned().map(Value::String).collect()),
                );
            }
            if !value.env.is_empty() {
                object.insert("env".to_owned(), string_map_value(&value.env)?);
            }
            object.insert("enabled".to_owned(), Value::Bool(true));
        }
        (Tool::Codex, McpTransport::StreamableHttp) => {
            object.insert("url".to_owned(), Value::String(http_url(value)?));
            if !value.headers.is_empty() {
                object.insert("http_headers".to_owned(), string_map_value(&value.headers)?);
            }
            object.insert("enabled".to_owned(), Value::Bool(true));
        }
        (Tool::Opencode, McpTransport::Stdio) => {
            object.insert("type".to_owned(), Value::String("local".to_owned()));
            let mut command = vec![Value::String(stdio_command(value)?)];
            command.extend(value.args.iter().cloned().map(Value::String));
            object.insert("command".to_owned(), Value::Array(command));
            if !value.env.is_empty() {
                object.insert("environment".to_owned(), string_map_value(&value.env)?);
            }
            object.insert("enabled".to_owned(), Value::Bool(value.enabled));
        }
        (Tool::Opencode, McpTransport::StreamableHttp) => {
            object.insert("type".to_owned(), Value::String("remote".to_owned()));
            object.insert("url".to_owned(), Value::String(http_url(value)?));
            if !value.headers.is_empty() {
                object.insert("headers".to_owned(), string_map_value(&value.headers)?);
            }
            object.insert("enabled".to_owned(), Value::Bool(value.enabled));
        }
    }
    Ok(Value::Object(object))
}

fn build_mcp_ownership(
    container: &[&str],
    desired: &[McpServerRecord],
    inherited: &[McpServerRecord],
    existing: &[ManagedMcpItemRecord],
) -> ManagedOwnership {
    let names = desired
        .iter()
        .chain(inherited.iter())
        .map(|record| record.name.clone())
        .chain(existing.iter().map(|item| item.external_key.clone()))
        .collect::<BTreeSet<_>>();
    ManagedOwnership::selectors(names.into_iter().map(|name| {
        container
            .iter()
            .copied()
            .chain(std::iter::once(name.as_str()))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    }))
}

fn verify_managed_item_baselines(
    scan: TargetScan,
    container: &[&str],
    existing: &[ManagedMcpItemRecord],
) -> (TargetScan, Vec<String>) {
    if existing.is_empty() {
        return (scan, Vec::new());
    }
    let mismatched = match &scan {
        TargetScan::Observed(observed) => {
            projection_value_at(&observed.managed_projection, container)
                .and_then(Value::as_object)
                .map_or_else(
                    || {
                        existing
                            .iter()
                            .map(|item| item.external_key.clone())
                            .collect()
                    },
                    |items| {
                        existing
                            .iter()
                            .filter(|item| {
                                items.get(&item.external_key).map_or(true, |value| {
                                    hash_json(value) != item.last_applied_item_hash
                                })
                            })
                            .map(|item| item.external_key.clone())
                            .collect()
                    },
                )
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
    _container: &[&str],
    desired: &[McpServerRecord],
    existing: &[ManagedMcpItemRecord],
    redactor: &SecretRedactor,
) -> Result<(Vec<ManagedItemApply>, Vec<String>), AppError> {
    let mut by_resource = BTreeMap::new();
    let mut by_external_key = BTreeMap::new();
    for item in existing {
        if by_resource
            .insert(item.resource_id.as_str(), item)
            .is_some()
            || by_external_key
                .insert(item.external_key.as_str(), item)
                .is_some()
        {
            return Err(AppError::conflict(
                "managedItems",
                "同一目标存在重复的 MCP managed item 基线",
            ));
        }
    }
    let mut used = BTreeSet::new();
    let mut updates = Vec::new();
    for record in desired {
        let configuration = configuration_from_record(record, redactor)?;
        let native = native_mcp_item(tool, &configuration)?;
        let existing_item = by_resource
            .get(record.id.as_str())
            .or_else(|| by_external_key.get(record.name.as_str()))
            .copied();
        let id = existing_item
            .map(|item| item.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        used.insert(id.clone());
        updates.push(ManagedItemApply {
            id,
            resource_kind: ArtifactKind::Mcp,
            resource_id: record.id.clone(),
            external_key: record.name.clone(),
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

fn ensure_mcp_target(
    database: &mut Database,
    descriptor: &TargetDescriptor,
    project: Option<&McpProjectRecord>,
) -> Result<ManagedTargetBaseline, AppError> {
    let target_path = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::not_found("mcpTarget", descriptor.tool.as_str()))?;
    let database_path = database.path().to_string_lossy().into_owned();
    let project_id = project.map(|project| project.id.as_str());
    let existing = find_mcp_target_baseline(database, descriptor, project_id)?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    database
        .connection_mut()
        .execute(
            "INSERT INTO managed_targets(
                id, tool, artifact_kind, scope, project_id, target_path
             ) VALUES (?1, ?2, 'mcp', ?3, ?4, ?5)",
            params![
                id,
                descriptor.tool.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
        )
        .map_err(|error| {
            AppError::database(&database_path, "insert_mcp_managed_target").with_source(error)
        })?;
    load_managed_target_baseline(database, &id)
}

pub(super) fn find_mcp_target_baseline(
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
             WHERE tool = ?1 AND artifact_kind = 'mcp' AND scope = ?2
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
            AppError::database(&database_path, "find_mcp_managed_target").with_source(error)
        })
}

fn mcp_dto(
    database: &Database,
    record: &McpServerRecord,
    redactor: &SecretRedactor,
) -> Result<McpServerDto, AppError> {
    mcp_dto_with_tools(
        record,
        redactor,
        <McpManagedArtifact as crate::sync::managed::ManagedArtifact>::global_tools(
            database, &record.id,
        )?,
    )
}

fn mcp_dto_with_tools(
    record: &McpServerRecord,
    redactor: &SecretRedactor,
    global_tools: Vec<Tool>,
) -> Result<McpServerDto, AppError> {
    let value = configuration_from_record(record, redactor)?;
    Ok(McpServerDto {
        id: record.id.clone(),
        name: record.name.clone(),
        transport: record.transport,
        command: record.command.clone(),
        args: value.args,
        url: record.url.clone(),
        header_names: value.headers.keys().cloned().collect(),
        env_names: value.env.keys().cloned().collect(),
        redacted_extra: redactor.redact_structure(&value.extra).into_value(),
        enabled: record.enabled,
        global_tools,
        row_version: crate::sync::managed_record_row_version::<
            crate::sync::managed::McpManagedArtifact,
        >(record)?,
    })
}

pub(super) fn configuration_from_record(
    record: &McpServerRecord,
    redactor: &SecretRedactor,
) -> Result<ValidatedMcpConfiguration, AppError> {
    let input = McpServerInput {
        name: record.name.clone(),
        transport: record.transport,
        command: record.command.clone(),
        args: serde_json::from_str(&record.args_json).map_err(|error| {
            AppError::invalid_input("args", "数据库中的 MCP args 无效")
                .with_source_redacted(error, redactor)
        })?,
        url: record.url.clone(),
        headers: serde_json::from_str(&record.headers_json).map_err(|error| {
            AppError::invalid_input("headers", "数据库中的 MCP headers 无效")
                .with_source_redacted(error, redactor)
        })?,
        env: serde_json::from_str(&record.env_json).map_err(|error| {
            AppError::invalid_input("env", "数据库中的 MCP env 无效")
                .with_source_redacted(error, redactor)
        })?,
        extra: serde_json::from_str(&record.extra_json).map_err(|error| {
            AppError::invalid_input("extra", "数据库中的 MCP extra 无效")
                .with_source_redacted(error, redactor)
        })?,
        enabled: record.enabled,
    };
    ValidatedMcpConfiguration::from_create(&input)
}

pub(crate) fn register_native_projection_secrets(redactor: &mut SecretRedactor, value: &Value) {
    match value {
        Value::Object(object) => {
            for env_key in ["env", "environment"] {
                let Some(Value::Object(env)) = object.get(env_key) else {
                    continue;
                };
                for (key, value) in env {
                    if let Some(text) = value.as_str() {
                        register_environment_value(redactor, key, text);
                    }
                }
            }
            for header_key in ["headers", "http_headers", "env_http_headers"] {
                if let Some(Value::Object(headers)) = object.get(header_key) {
                    for value in headers.values() {
                        if let Some(text) = value.as_str() {
                            redactor.register_secret(text);
                        }
                    }
                }
            }
            if let Some(auth) = object.get("auth") {
                register_detectable_extra_secrets(redactor, Some("auth"), auth);
            }
            register_detectable_extra_secrets(redactor, None, value);
        }
        _ => register_detectable_extra_secrets(redactor, None, value),
    }
}

pub(super) fn register_configuration_secrets(
    redactor: &mut SecretRedactor,
    value: &ValidatedMcpConfiguration,
) {
    for secret in value.headers.values() {
        redactor.register_secret(secret.clone());
    }
    for (key, value) in &value.env {
        register_environment_value(redactor, key, value);
    }
    register_detectable_extra_secrets(redactor, None, &value.extra);
}

pub(super) fn register_environment_value(redactor: &mut SecretRedactor, key: &str, value: &str) {
    // 只有明确的运行变量及合法值形状可免于凭据判定；未知用途仍保守保护。
    if !contains_detectable_secret(key, value) && is_runtime_environment_value(key, value) {
        redactor.register_private_value(value);
    } else {
        redactor.register_secret(value);
    }
}

fn is_runtime_environment_value(key: &str, value: &str) -> bool {
    let absolute_path = |value: &str| {
        std::path::Path::new(value).is_absolute()
            && !value.chars().any(char::is_control)
            && !std::path::Path::new(value)
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
    };
    match key {
        "NODE_REPL_NODE_PATH" | "CODEX_HOME" | "HOME" | "TMPDIR" | "NODE_BINARY" => {
            absolute_path(value)
        }
        "PATH" | "NODE_PATH" | "PYTHONPATH" => value.split(':').all(absolute_path),
        "BROWSER_USE_TINYSKY_ENABLED" | "CI" | "NO_COLOR" => {
            matches!(value, "0" | "1" | "true" | "false")
        }
        "NODE_REPL_NATIVE_PIPE_CONNECT_TIMEOUT_MS" => {
            !value.is_empty()
                && value.bytes().all(|byte| byte.is_ascii_digit())
                && value.parse::<u64>().is_ok()
        }
        _ => false,
    }
}

pub(super) fn register_detectable_extra_secrets(
    redactor: &mut SecretRedactor,
    key: Option<&str>,
    value: &Value,
) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                register_detectable_extra_secrets(redactor, Some(key), value);
            }
        }
        Value::Array(values) => {
            for value in values {
                register_detectable_extra_secrets(redactor, key, value);
            }
        }
        Value::String(value) if key.is_some_and(|key| contains_detectable_secret(key, value)) => {
            redactor.register_secret(value.clone());
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
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
             WHERE tool = ?1 AND artifact_kind = 'mcp'
               AND ifnull(project_id, '') = ifnull(?2, '') AND target_path = ?3",
            params![tool.as_str(), project_id, target_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| AppError::database(&path, "load_mcp_target_status").with_source(error))?;
    status.map(parse_sync_status).transpose()
}

fn parse_sync_status(value: String) -> Result<SyncStatus, AppError> {
    match value.as_str() {
        "in_sync" => Ok(SyncStatus::InSync),
        "external_non_owned_change" => Ok(SyncStatus::ExternalNonOwnedChange),
        "external_owned_change" => Ok(SyncStatus::ExternalOwnedChange),
        "missing" => Ok(SyncStatus::Missing),
        "parse_error" => Ok(SyncStatus::ParseError),
        "permission_denied" => Ok(SyncStatus::PermissionDenied),
        "policy_blocked" => Ok(SyncStatus::PolicyBlocked),
        "untrusted" => Ok(SyncStatus::Untrusted),
        "target_type_changed" => Ok(SyncStatus::TargetTypeChanged),
        "failed" => Ok(SyncStatus::Failed),
        _ => Err(AppError::invalid_input(
            "syncStatus",
            "数据库包含未知 MCP 同步状态",
        )),
    }
}

#[cfg(test)]
include!("tests.rs");
