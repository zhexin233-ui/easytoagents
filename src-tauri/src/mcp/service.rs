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
    McpTargetStatusDto, PreviewMcpSyncInput, SetGlobalMcpAssignmentInput,
    SetProjectMcpAssignmentInput, UpdateMcpServerInput, ValidatedMcpConfiguration,
    VersionedMcpInput,
};
use crate::{
    adapters::{
        canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
        ClaudeCustomizationPolicyProbe, ClaudeUserMcpCapabilityProbe,
        ConservativeClaudeCustomizationPolicyProbe, ConservativeClaudeUserMcpProbe,
        DiscoveryContext, ManagedOwnership, TargetDescriptor, ToolAdapter,
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
    sync::{
        apply_persisted_preview, build_preview_plan, hash_json, load_managed_target_baseline,
        load_persisted_preview, persist_preview, scan_target, ApplyResult, ApplyTargetInput,
        DatabaseEntityType, DatabaseRowVersion, ManagedItemApply, ManagedTargetBaseline,
        NoApplyFault, PreviewPlan, PreviewTargetRequest, TargetScan,
    },
};

pub fn list_mcp_servers(
    database: &Database,
    redactor: &SecretRedactor,
) -> Result<Vec<McpServerDto>, AppError> {
    repository::list_mcp_servers(database)?
        .iter()
        .map(|record| mcp_dto(database, record, redactor))
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
    let current_value = configuration_from_record(&current)?;
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
        .map(project_dto)
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
    let user_probe = ConservativeClaudeUserMcpProbe;
    let policy_probe = ConservativeClaudeCustomizationPolicyProbe;
    preview_mcp_sync_with_probes(
        database,
        environment,
        redactor,
        input,
        &user_probe,
        &policy_probe,
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
                desired_projection: target.desired_projection,
                row_versions: target.row_versions,
                git: target.git,
                exclude_from_git: input.exclude_from_git,
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
    let user_probe = ConservativeClaudeUserMcpProbe;
    let policy_probe = ConservativeClaudeCustomizationPolicyProbe;
    apply_mcp_preview_with_probes(
        write_operations,
        database,
        paths,
        environment,
        redactor,
        input,
        &user_probe,
        &policy_probe,
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

pub fn list_global_mcp_target_statuses(
    database: &Database,
    environment: &crate::adapters::ExplicitEnvironment,
) -> Result<Vec<McpTargetStatusDto>, AppError> {
    let user_probe = ConservativeClaudeUserMcpProbe;
    let policy_probe = ConservativeClaudeCustomizationPolicyProbe;
    [Tool::Claude, Tool::Codex]
        .into_iter()
        .map(|tool| {
            let descriptor = descriptor_for(environment, tool, None, &user_probe, &policy_probe)?;
            let target_path = descriptor.path.clone();
            let persisted = target_path
                .as_deref()
                .map(|path| load_target_status(database, tool, None, path))
                .transpose()?
                .flatten();
            let (status, diagnostic_code) =
                if descriptor.capability.state != crate::adapters::CapabilityState::Supported {
                    (
                        SyncStatus::Failed,
                        descriptor.capability.diagnostic_code.clone(),
                    )
                } else if descriptor.policy != crate::adapters::PolicyState::Allowed {
                    (
                        SyncStatus::PolicyBlocked,
                        Some("CLAUDE_POLICY_BLOCKED_OR_UNKNOWN".to_owned()),
                    )
                } else {
                    (persisted.unwrap_or(SyncStatus::Missing), None)
                };
            Ok(McpTargetStatusDto {
                tool,
                project_id: None,
                target_path,
                status,
                diagnostic_code,
            })
        })
        .collect()
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
    let descriptor = descriptor_for(
        environment,
        input.tool,
        project_root.as_ref(),
        user_probe,
        policy_probe,
    )?;
    let baseline = ensure_mcp_target(database, &descriptor, project.as_ref())?;
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
    let scan = verify_managed_item_baselines(
        scan_target(tool_adapter(input.tool), &descriptor, &ownership),
        container,
        &existing_items,
    );
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
    let (managed_items, remove_managed_item_ids) =
        build_managed_item_changes(input.tool, container, &desired_records, &existing_items)?;
    let row_versions = collect_row_versions(
        database,
        project.as_ref(),
        desired_records.iter().chain(inherited_records.iter()),
        &existing_items,
    )?;
    let git = project_root
        .as_ref()
        .zip(descriptor.path.as_deref())
        .map(|(root, path)| inspect_path(root, Path::new(path)))
        .transpose()?;
    let allowed_root = project_root.as_ref().map_or_else(
        || match input.tool {
            Tool::Claude => environment.home().to_path_buf(),
            Tool::Codex => environment.codex_home().to_path_buf(),
        },
        |root| PathBuf::from(root.as_str()),
    );
    Ok(PreparedMcpSync {
        scope,
        project,
        target: Some(PreparedMcpTarget {
            descriptor,
            ownership,
            baseline,
            scan,
            desired_projection: desired,
            row_versions,
            git,
            allowed_root,
            managed_items,
            remove_managed_item_ids,
        }),
    })
}

fn inherited_projection_is_absent(scan: &TargetScan, container: &str) -> bool {
    match scan {
        TargetScan::Missing => true,
        TargetScan::Observed(observed) => match observed
            .managed_projection
            .get(container)
            .and_then(Value::as_object)
        {
            Some(items) => items.is_empty(),
            None => true,
        },
        TargetScan::ManagedItemBaselineMismatch
        | TargetScan::ParseError
        | TargetScan::PermissionDenied
        | TargetScan::TargetTypeChanged(_)
        | TargetScan::Unavailable
        | TargetScan::Failed => false,
    }
}

fn descriptor_for(
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
    tool_adapter(tool)
        .discover(&context)?
        .into_iter()
        .find(|descriptor| {
            descriptor.artifact_kind == ArtifactKind::Mcp
                && descriptor.scope
                    == if project_root.is_some() {
                        Scope::Project
                    } else {
                        Scope::Global
                    }
        })
        .ok_or_else(|| AppError::not_found("mcpTarget", tool.as_str()))
}

fn tool_adapter(tool: Tool) -> &'static dyn ToolAdapter {
    static CLAUDE: ClaudeAdapter = ClaudeAdapter;
    static CODEX: CodexAdapter = CodexAdapter;
    match tool {
        Tool::Claude => &CLAUDE,
        Tool::Codex => &CODEX,
    }
}

fn native_container(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "mcpServers",
        Tool::Codex => "mcp_servers",
    }
}

fn build_desired_projection(
    tool: Tool,
    container: &str,
    records: &[McpServerRecord],
    redactor: &mut SecretRedactor,
) -> Result<Value, AppError> {
    let mut servers = Map::new();
    for record in records {
        let configuration = configuration_from_record(record)?;
        register_configuration_secrets(redactor, &configuration);
        servers.insert(record.name.clone(), native_mcp_item(tool, &configuration)?);
    }
    if servers.is_empty() {
        Ok(Value::Object(Map::new()))
    } else {
        Ok(Value::Object(Map::from_iter([(
            container.to_owned(),
            Value::Object(servers),
        )])))
    }
}

fn native_mcp_item(tool: Tool, value: &ValidatedMcpConfiguration) -> Result<Value, AppError> {
    let mut object = value
        .extra
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::invalid_input("extra", "MCP 扩展字段必须是对象"))?;
    match (tool, value.transport) {
        (Tool::Claude, McpTransport::Stdio) => {
            object.insert("type".to_owned(), Value::String("stdio".to_owned()));
            object.insert(
                "command".to_owned(),
                Value::String(value.command.clone().expect("stdio command 已验证")),
            );
            if !value.args.is_empty() {
                object.insert(
                    "args".to_owned(),
                    Value::Array(value.args.iter().cloned().map(Value::String).collect()),
                );
            }
            if !value.env.is_empty() {
                object.insert("env".to_owned(), serde_json::to_value(&value.env).unwrap());
            }
        }
        (Tool::Claude, McpTransport::StreamableHttp) => {
            object.insert("type".to_owned(), Value::String("http".to_owned()));
            object.insert(
                "url".to_owned(),
                Value::String(value.url.clone().expect("HTTP URL 已验证")),
            );
            if !value.headers.is_empty() {
                object.insert(
                    "headers".to_owned(),
                    serde_json::to_value(&value.headers).unwrap(),
                );
            }
        }
        (Tool::Codex, McpTransport::Stdio) => {
            object.insert(
                "command".to_owned(),
                Value::String(value.command.clone().expect("stdio command 已验证")),
            );
            if !value.args.is_empty() {
                object.insert(
                    "args".to_owned(),
                    Value::Array(value.args.iter().cloned().map(Value::String).collect()),
                );
            }
            if !value.env.is_empty() {
                object.insert("env".to_owned(), serde_json::to_value(&value.env).unwrap());
            }
            object.insert("enabled".to_owned(), Value::Bool(true));
        }
        (Tool::Codex, McpTransport::StreamableHttp) => {
            object.insert(
                "url".to_owned(),
                Value::String(value.url.clone().expect("HTTP URL 已验证")),
            );
            if !value.headers.is_empty() {
                object.insert(
                    "http_headers".to_owned(),
                    serde_json::to_value(&value.headers).unwrap(),
                );
            }
            object.insert("enabled".to_owned(), Value::Bool(true));
        }
    }
    Ok(Value::Object(object))
}

fn build_mcp_ownership(
    container: &str,
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
    ManagedOwnership::selectors(
        names
            .into_iter()
            .map(|name| vec![container.to_owned(), name]),
    )
}

fn verify_managed_item_baselines(
    scan: TargetScan,
    container: &str,
    existing: &[ManagedMcpItemRecord],
) -> TargetScan {
    if existing.is_empty() {
        return scan;
    }
    let matches = match &scan {
        TargetScan::Observed(observed) => observed
            .managed_projection
            .get(container)
            .and_then(Value::as_object)
            .is_some_and(|items| {
                existing.iter().all(|item| {
                    items
                        .get(&item.external_key)
                        .is_some_and(|value| hash_json(value) == item.last_applied_item_hash)
                })
            }),
        TargetScan::Missing => false,
        _ => return scan,
    };
    if matches {
        scan
    } else {
        TargetScan::ManagedItemBaselineMismatch
    }
}

fn build_managed_item_changes(
    tool: Tool,
    _container: &str,
    desired: &[McpServerRecord],
    existing: &[ManagedMcpItemRecord],
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
        let configuration = configuration_from_record(record)?;
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

fn collect_row_versions<'a>(
    database: &Database,
    project: Option<&McpProjectRecord>,
    records: impl Iterator<Item = &'a McpServerRecord>,
    items: &[ManagedMcpItemRecord],
) -> Result<Vec<DatabaseRowVersion>, AppError> {
    let mut versions = BTreeMap::<(DatabaseEntityType, String), u32>::new();
    if let Some(project) = project {
        versions.insert(
            (DatabaseEntityType::Project, project.id.clone()),
            safe_row_version(project.row_version)?,
        );
    }
    for record in records {
        versions.insert(
            (DatabaseEntityType::McpServer, record.id.clone()),
            safe_row_version(record.row_version)?,
        );
    }
    for item in items {
        versions.insert(
            (DatabaseEntityType::ManagedItem, item.id.clone()),
            safe_row_version(item.row_version)?,
        );
        if let Ok(record) = repository::get_mcp_server(database, &item.resource_id) {
            versions.insert(
                (DatabaseEntityType::McpServer, record.id),
                safe_row_version(record.row_version)?,
            );
        }
    }
    Ok(versions
        .into_iter()
        .map(
            |((entity_type, entity_id), row_version)| DatabaseRowVersion {
                entity_type,
                entity_id,
                row_version,
            },
        )
        .collect())
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
    let existing = database
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
        .map_err(|_| AppError::database(&database_path, "find_mcp_managed_target"))?;
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
        .map_err(|_| AppError::database(&database_path, "insert_mcp_managed_target"))?;
    load_managed_target_baseline(database, &id)
}

fn mcp_dto(
    database: &Database,
    record: &McpServerRecord,
    redactor: &SecretRedactor,
) -> Result<McpServerDto, AppError> {
    let value = configuration_from_record(record)?;
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
        global_tools: repository::global_tools_for_mcp(database, &record.id)?,
        row_version: safe_row_version(record.row_version)?,
    })
}

fn project_dto(project: &McpProjectRecord) -> Result<McpProjectDto, AppError> {
    Ok(McpProjectDto {
        id: project.id.clone(),
        display_name: project.display_name.clone(),
        root_path: project.root_path.clone(),
        codex_trust_status: project.codex_trust_status,
        row_version: safe_row_version(project.row_version)?,
    })
}

fn configuration_from_record(
    record: &McpServerRecord,
) -> Result<ValidatedMcpConfiguration, AppError> {
    let input = McpServerInput {
        name: record.name.clone(),
        transport: record.transport,
        command: record.command.clone(),
        args: serde_json::from_str(&record.args_json)
            .map_err(|_| AppError::invalid_input("args", "数据库中的 MCP args 无效"))?,
        url: record.url.clone(),
        headers: serde_json::from_str(&record.headers_json)
            .map_err(|_| AppError::invalid_input("headers", "数据库中的 MCP headers 无效"))?,
        env: serde_json::from_str(&record.env_json)
            .map_err(|_| AppError::invalid_input("env", "数据库中的 MCP env 无效"))?,
        extra: serde_json::from_str(&record.extra_json)
            .map_err(|_| AppError::invalid_input("extra", "数据库中的 MCP extra 无效"))?,
        enabled: record.enabled,
    };
    ValidatedMcpConfiguration::from_create(&input)
}

fn register_configuration_secrets(
    redactor: &mut SecretRedactor,
    value: &ValidatedMcpConfiguration,
) {
    for secret in value.headers.values().chain(value.env.values()) {
        redactor.register_secret(secret.clone());
    }
    register_detectable_extra_secrets(redactor, None, &value.extra);
}

fn register_detectable_extra_secrets(
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

fn safe_row_version(value: i64) -> Result<u32, AppError> {
    u32::try_from(value)
        .map_err(|_| AppError::invalid_input("rowVersion", "数据库 row_version 超出 RPC 范围"))
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
        .map_err(|_| AppError::database(&path, "load_mcp_target_status"))?;
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
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use serde_json::{json, Value};
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::{
        apply_mcp_preview_with_probes, create_mcp_server, list_mcp_project_options,
        preview_mcp_sync_with_probes, set_global_mcp_assignment, set_project_mcp_assignment,
        update_mcp_server,
    };
    use crate::{
        adapters::{
            ConservativeClaudeUserMcpProbe, ExplicitEnvironment, ToolAvailability,
            VerifiedClaudeCustomizationPolicyEvidence, VerifiedClaudeUserMcpEvidence,
        },
        app::AppPaths,
        db::Database,
        domain::{McpTransport, Tool},
        error::ErrorCode,
        mcp::{
            ApplyMcpPreviewInput, McpProjectOptionsInput, McpProjectSelectionState, McpServerInput,
            PreviewMcpSyncInput, SensitiveJsonUpdate, SensitiveMapUpdate,
            SetGlobalMcpAssignmentInput, SetProjectMcpAssignmentInput, UpdateMcpServerInput,
        },
        security::SecretRedactor,
    };

    const HEADER_SECRET: &str = "Bearer phase5-header-secret";
    const ENV_SECRET: &str = "phase5-env-secret";
    const EXTRA_SECRET: &str = "phase5-extra-secret";

    struct Fixture {
        _temporary: TempDir,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
        home: std::path::PathBuf,
        project: std::path::PathBuf,
        project_id: String,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("isolated-home");
            let project = root.join("project");
            fs::create_dir(&home).unwrap();
            fs::create_dir(&project).unwrap();
            fs::create_dir(home.join(".claude")).unwrap();
            fs::create_dir(home.join(".codex")).unwrap();
            fs::create_dir(project.join(".codex")).unwrap();
            let home = fs::canonicalize(home).unwrap();
            let project = fs::canonicalize(project).unwrap();
            let environment =
                ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                    .unwrap()
                    .with_claude_installation_version("fixture-1.0.0")
                    .unwrap();
            let paths = AppPaths::from_data_root(root.join("private/app/data")).unwrap();
            let database = Database::open(&paths).unwrap();
            let project_id = Uuid::new_v4().to_string();
            database
                .connection()
                .execute(
                    "INSERT INTO projects(
                        id, display_name, root_path, is_git_repo, codex_trust_status
                     ) VALUES (?1, '隔离项目', ?2, 0, 'unknown')",
                    rusqlite::params![project_id, project.to_string_lossy()],
                )
                .unwrap();
            Self {
                _temporary: temporary,
                paths,
                database,
                environment,
                home,
                project,
                project_id,
            }
        }

        fn allowed_policy(&self) -> VerifiedClaudeCustomizationPolicyEvidence {
            VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting("fixture-1.0.0", None)
                .unwrap()
        }
    }

    fn stdio_input(name: &str) -> McpServerInput {
        McpServerInput {
            name: name.to_owned(),
            transport: McpTransport::Stdio,
            command: Some("npx".to_owned()),
            args: vec!["-y".to_owned(), "fixture-server".to_owned()],
            url: None,
            headers: BTreeMap::new(),
            env: BTreeMap::from([("MCP_TOKEN".to_owned(), ENV_SECRET.to_owned())]),
            extra: json!({
                "startup_timeout_sec": 10,
                "nested": {"api_token": EXTRA_SECRET}
            }),
            enabled: true,
        }
    }

    fn http_input(name: &str) -> McpServerInput {
        McpServerInput {
            name: name.to_owned(),
            transport: McpTransport::StreamableHttp,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/rpc?tenant=fixture".to_owned()),
            headers: BTreeMap::from([("Authorization".to_owned(), HEADER_SECRET.to_owned())]),
            env: BTreeMap::new(),
            extra: json!({"request_timeout_sec": 30}),
            enabled: true,
        }
    }

    #[test]
    fn crud_cas_assignment_and_dto_redaction_are_enforced() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let server = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("Fixture MCP"),
        )
        .unwrap();
        let serialized = serde_json::to_string(&server).unwrap();
        assert!(!serialized.contains(ENV_SECRET));
        assert!(!serialized.contains(EXTRA_SECRET));
        assert_eq!(server.env_names, ["MCP_TOKEN"]);
        assert_eq!(server.redacted_extra["nested"]["api_token"], "[REDACTED]");

        let duplicate = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("fixture mcp"),
        )
        .unwrap_err();
        assert_eq!(duplicate.code(), ErrorCode::Conflict);

        let global = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: server.id.clone(),
                assigned: true,
                row_version: server.row_version,
            },
        )
        .unwrap();
        let project = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        let inherited = set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                mcp_id: global.id.clone(),
                assigned: true,
                mcp_row_version: global.row_version,
                project_row_version: project,
            },
        )
        .unwrap_err();
        assert_eq!(inherited.code(), ErrorCode::Conflict);
        let inherited_disable = set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                mcp_id: global.id.clone(),
                assigned: false,
                mcp_row_version: global.row_version,
                project_row_version: project,
            },
        )
        .unwrap_err();
        assert_eq!(inherited_disable.code(), ErrorCode::Conflict);

        let options = list_mcp_project_options(
            &fixture.database,
            &McpProjectOptionsInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
            },
        )
        .unwrap();
        assert_eq!(options[0].state, McpProjectSelectionState::Inherited);
        assert!(!options[0].selectable);

        let stale = crate::mcp::set_mcp_enabled(
            &mut fixture.database,
            &redactor,
            &crate::mcp::VersionedMcpInput {
                id: global.id,
                row_version: server.row_version,
            },
            false,
        )
        .unwrap_err();
        assert_eq!(stale.code(), ErrorCode::Conflict);
    }

    #[test]
    fn global_json_and_toml_round_trip_rename_cleanup_and_drift_are_safe() {
        let mut fixture = Fixture::new();
        let claude_path = fixture.home.join(".claude.json");
        let codex_path = fixture.home.join(".codex/config.toml");
        fs::write(
            &claude_path,
            br#"{
  "theme": "dark",
  "mcpServers": {
    "external": {"command": "keep", "unknown": {"value": 1}}
  },
  "unknownTop": {"preserve": true}
}
"#,
        )
        .unwrap();
        fs::write(
            &codex_path,
            r#"# 顶层注释必须保留
model = "fixture-model"

[mcp_servers.external]
command = "keep"
unknown = "preserve"

[plugins.fixture]
enabled = true
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let created = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &http_input("managed-http"),
        )
        .unwrap();
        let claude = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: created.row_version,
            },
        )
        .unwrap();
        let codex = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Codex,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: claude.row_version,
            },
        )
        .unwrap();
        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let write_operations = std::sync::Mutex::new(());

        for tool in [Tool::Claude, Tool::Codex] {
            let input = PreviewMcpSyncInput {
                tool,
                project_id: None,
                exclude_from_git: false,
            };
            let preview = preview_mcp_sync_with_probes(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &input,
                &user_probe,
                &policy,
            )
            .unwrap();
            let serialized = serde_json::to_string(&preview).unwrap();
            for secret in [HEADER_SECRET, ENV_SECRET, EXTRA_SECRET] {
                assert!(!serialized.contains(secret));
            }
            apply_mcp_preview_with_probes(
                &write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &mut redactor,
                &ApplyMcpPreviewInput {
                    preview_id: preview.preview_id,
                    tool,
                    project_id: None,
                },
                &user_probe,
                &policy,
            )
            .unwrap();
        }

        let claude_native: Value =
            serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        assert_eq!(claude_native["theme"], "dark");
        assert_eq!(claude_native["mcpServers"]["external"]["command"], "keep");
        assert_eq!(
            claude_native["mcpServers"]["managed-http"]["headers"]["Authorization"],
            HEADER_SECRET
        );
        let codex_native = fs::read_to_string(&codex_path).unwrap();
        assert!(codex_native.contains("# 顶层注释必须保留"));
        assert!(codex_native.contains("[plugins.fixture]"));
        assert!(codex_native.contains("[mcp_servers.external]"));
        assert!(codex_native.contains(HEADER_SECRET));

        let updated = update_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &UpdateMcpServerInput {
                id: codex.id.clone(),
                name: "renamed-http".to_owned(),
                transport: McpTransport::StreamableHttp,
                command: None,
                args: Vec::new(),
                url: Some("https://mcp.example.test/rpc?tenant=fixture".to_owned()),
                headers: SensitiveMapUpdate::Keep,
                env: SensitiveMapUpdate::Keep,
                extra: SensitiveJsonUpdate::Keep,
                enabled: true,
                row_version: codex.row_version,
            },
        )
        .unwrap();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        let renamed: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        assert!(renamed["mcpServers"].get("managed-http").is_none());
        assert!(renamed["mcpServers"].get("renamed-http").is_some());

        let reassigned = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: updated.id.clone(),
                assigned: false,
                row_version: updated.row_version,
            },
        )
        .unwrap();
        let removal = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: removal.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        let cleaned: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        assert!(cleaned["mcpServers"].get("renamed-http").is_none());
        assert!(cleaned["mcpServers"].get("external").is_some());

        let reenabled = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: reassigned.id.clone(),
                assigned: true,
                row_version: reassigned.row_version,
            },
        )
        .unwrap();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        let mut drifted: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        drifted["mcpServers"]["renamed-http"]["url"] =
            Value::String("https://external.example.test/rpc".to_owned());
        fs::write(&claude_path, serde_json::to_vec_pretty(&drifted).unwrap()).unwrap();
        let _unassigned = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: reenabled.id,
                assigned: false,
                row_version: reenabled.row_version,
            },
        )
        .unwrap();
        let blocked = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(blocked.targets[0].change_kind.as_str(), "conflict");
        assert!(blocked.targets[0]
            .warning_codes
            .contains(&crate::sync::ERROR_MANAGED_ITEM_BASELINE_MISMATCH.to_owned()));
        assert_eq!(
            drifted["mcpServers"]["renamed-http"]["url"],
            "https://external.example.test/rpc"
        );

        let sync_payloads = fixture
            .database
            .connection()
            .prepare("SELECT redacted_diff_json FROM sync_items")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        let journals = read_tree_text(fixture.paths.journals());
        for secret in [HEADER_SECRET, ENV_SECRET, EXTRA_SECRET] {
            assert!(!sync_payloads.contains(secret));
            assert!(!journals.contains(secret));
        }
    }

    #[test]
    fn claude_and_trusted_codex_project_targets_append_without_writing_inherited_items() {
        let mut fixture = Fixture::new();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.{}]\ntrust_level = \"trusted\"\n",
                toml_edit::Key::new(fixture.project.to_string_lossy().as_ref())
            ),
        )
        .unwrap();
        fs::write(
            fixture.project.join(".mcp.json"),
            br#"{"unknownTop":{"keep":true},"mcpServers":{"external":{"command":"keep"}}}"#,
        )
        .unwrap();
        fs::write(
            fixture.project.join(".codex/config.toml"),
            "# 项目注释\n[features]\nfixture = true\n",
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let inherited = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("inherited-global"),
        )
        .unwrap();
        let inherited = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: inherited.id,
                assigned: true,
                row_version: inherited.row_version,
            },
        )
        .unwrap();
        let claude_project = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("claude-project"),
        )
        .unwrap();
        let codex_project = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &http_input("codex-project"),
        )
        .unwrap();

        let mut project_version = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                mcp_id: claude_project.id,
                assigned: true,
                mcp_row_version: claude_project.row_version,
                project_row_version: project_version,
            },
        )
        .unwrap();
        project_version = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Codex,
                mcp_id: codex_project.id,
                assigned: true,
                mcp_row_version: codex_project.row_version,
                project_row_version: project_version,
            },
        )
        .unwrap();

        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let write_operations = std::sync::Mutex::new(());
        for tool in [Tool::Claude, Tool::Codex] {
            let preview = preview_mcp_sync_with_probes(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &PreviewMcpSyncInput {
                    tool,
                    project_id: Some(fixture.project_id.clone()),
                    exclude_from_git: false,
                },
                &user_probe,
                &policy,
            )
            .unwrap();
            assert_ne!(preview.targets[0].change_kind.as_str(), "conflict");
            apply_mcp_preview_with_probes(
                &write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &mut redactor,
                &ApplyMcpPreviewInput {
                    preview_id: preview.preview_id,
                    tool,
                    project_id: Some(fixture.project_id.clone()),
                },
                &user_probe,
                &policy,
            )
            .unwrap();
        }

        let claude_native: Value =
            serde_json::from_slice(&fs::read(fixture.project.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(claude_native["unknownTop"]["keep"], true);
        assert!(claude_native["mcpServers"].get("external").is_some());
        assert!(claude_native["mcpServers"].get("claude-project").is_some());
        assert!(claude_native["mcpServers"]
            .get("inherited-global")
            .is_none());
        let codex_native = fs::read_to_string(fixture.project.join(".codex/config.toml")).unwrap();
        assert!(codex_native.contains("# 项目注释"));
        assert!(codex_native.contains("[features]"));
        assert!(codex_native.contains("[mcp_servers.codex-project]"));
        assert!(codex_native.contains(HEADER_SECRET));
        assert_eq!(inherited.name, "inherited-global");
    }

    #[test]
    fn project_inheritance_external_conflict_untrusted_and_capability_fail_closed() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let global = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("global-name"),
        )
        .unwrap();
        let global = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: global.id,
                assigned: true,
                row_version: global.row_version,
            },
        )
        .unwrap();
        fs::write(
            fixture.project.join(".mcp.json"),
            serde_json::to_vec_pretty(&json!({
                "unknownTop": true,
                "mcpServers": {"global-name": {"command": "external"}}
            }))
            .unwrap(),
        )
        .unwrap();
        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let conflict = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(conflict.targets[0].change_kind.as_str(), "conflict");
        let unchanged: Value =
            serde_json::from_slice(&fs::read(fixture.project.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(
            unchanged["mcpServers"]["global-name"]["command"],
            "external"
        );

        let project_server = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("project-only"),
        )
        .unwrap();
        let project_version = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Codex,
                mcp_id: project_server.id,
                assigned: true,
                mcp_row_version: project_server.row_version,
                project_row_version: project_version,
            },
        )
        .unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            "[projects.\"/somewhere-else\"]\ntrust_level = \"trusted\"\n",
        )
        .unwrap();
        let untrusted = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Codex,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(
            untrusted.targets[0].status,
            crate::domain::SyncStatus::Untrusted
        );

        let custom_root = fixture.home.join("custom-claude");
        fs::create_dir(&custom_root).unwrap();
        let custom_environment = ExplicitEnvironment::new(
            &fixture.home,
            Some(custom_root.clone()),
            None,
            ToolAvailability::all_installed(),
        )
        .unwrap()
        .with_claude_installation_version("fixture-1.0.0")
        .unwrap();
        let unsupported = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &custom_environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap_err();
        assert_eq!(unsupported.code(), ErrorCode::NotFound);

        let verified_path = fixture.home.join("verified-claude-mcp.json");
        let verified_probe =
            VerifiedClaudeUserMcpEvidence::new("fixture-1.0.0", &custom_root, &verified_path)
                .unwrap();
        let supported = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &custom_environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &verified_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(
            supported.targets[0].descriptor.path.as_deref(),
            Some(verified_path.to_string_lossy().as_ref())
        );
        assert_eq!(global.name, "global-name");

        let secret_url = "https://phase5-url-secret@mcp.example.test/rpc";
        let mut invalid = http_input("invalid-url");
        invalid.url = Some(secret_url.to_owned());
        let error = create_mcp_server(&mut fixture.database, &mut redactor, &invalid).unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("phase5-url-secret"));
    }

    #[test]
    fn inherited_only_project_preview_does_not_create_an_empty_native_file() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let inherited = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("inherited-only"),
        )
        .unwrap();
        set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: inherited.id,
                assigned: true,
                row_version: inherited.row_version,
            },
        )
        .unwrap();

        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert!(preview.targets.is_empty());

        apply_mcp_preview_with_probes(
            &std::sync::Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert!(!fixture.project.join(".mcp.json").exists());
    }

    fn read_tree_text(root: &Path) -> String {
        let mut output = String::new();
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    output.push_str(&read_tree_text(&path));
                } else if let Ok(text) = fs::read_to_string(path) {
                    output.push_str(&text);
                }
            }
        }
        output
    }
}
