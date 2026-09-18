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
        ClaudeUserMcpCapabilityProbe, DiscoveryContext, ManagedOwnership, ObservedDocument,
        TargetDescriptor, ASSIGNABLE_MCP_TOOLS,
    },
    app::AppPaths,
    db::{
        mcp::{self as repository, ManagedMcpItemRecord, McpProjectRecord, McpServerRecord},
        mcp_imports as import_repository, Database,
    },
    domain::{ArtifactKind, McpTransport, ProjectRoot, Scope, SyncScopeDto, SyncStatus, Tool},
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
    let dto = mcp_dto(database, &record, redactor)?;
    Ok(with_affected_sync_scopes(dto, Vec::new()))
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
    let scopes = repository::sync_scopes_for_mcp(database, &input.id)?;
    let dto = mcp_dto(database, &record, redactor)?;
    Ok(with_affected_sync_scopes(dto, scopes))
}

pub fn set_mcp_enabled(
    database: &mut Database,
    redactor: &SecretRedactor,
    input: &VersionedMcpInput,
    enabled: bool,
) -> Result<McpServerDto, AppError> {
    let record = repository::set_mcp_enabled(database, &input.id, input.row_version, enabled)?;
    let scopes = repository::sync_scopes_for_mcp(database, &input.id)?;
    let dto = mcp_dto(database, &record, redactor)?;
    Ok(with_affected_sync_scopes(dto, scopes))
}

pub fn delete_mcp_server(
    database: &mut Database,
    input: &VersionedMcpInput,
) -> Result<DeleteMcpResultDto, AppError> {
    let scopes = repository::sync_scopes_for_mcp(database, &input.id)?;
    repository::delete_mcp_server(database, &input.id, input.row_version)?;
    Ok(DeleteMcpResultDto {
        id: input.id.clone(),
        deleted: true,
        affected_sync_scopes: Some(scopes),
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
    let dto = mcp_dto(database, &record, redactor)?;
    Ok(with_affected_sync_scopes(
        dto,
        vec![SyncScopeDto::global(ArtifactKind::Mcp, input.tool)],
    ))
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
    let dto = mcp_dto(database, &record, redactor)?;
    Ok(with_affected_sync_scopes(
        dto,
        vec![SyncScopeDto::project(
            ArtifactKind::Mcp,
            input.tool,
            input.project_id.clone(),
        )],
    ))
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
                hard_block: target.hard_block,
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

/// 外部变化采纳所需的持久化 Preview 证据。该类型不属于 RPC DTO，调用方必须
/// 从服务端读取的 `PersistedPreviewItem` 填充，不能把脱敏 diff 或客户端猜测
/// 的配置作为采纳输入。
#[derive(Debug, Clone)]
pub struct AdoptMcpNativeInput {
    pub preview_id: String,
    pub tool: Tool,
    pub project_id: Option<String>,
    pub target_id: String,
    pub target_path: String,
    pub status: SyncStatus,
    pub descriptor: TargetDescriptor,
    pub ownership: ManagedOwnership,
    pub observed_full_hash: Option<String>,
    pub observed_managed_hash: Option<String>,
    pub target_row_version: u32,
    pub row_versions: Vec<DatabaseRowVersion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdoptMcpNativeResult {
    pub adopted_item_count: u32,
    pub updated_server_count: u32,
}

const MCP_NATIVE_MATCH_OR_IMPORT_REASON: &str = "MATCH_OR_IMPORT_REQUIRED";

fn mcp_native_match_or_import() -> AppError {
    AppError::invalid_input("mcpAdopt", MCP_NATIVE_MATCH_OR_IMPORT_REASON)
}

/// 采纳一个已经由 ExternalChangePlan 证明为 `ExternalOwnedChange` 的 MCP 目标。
///
/// 只接受严格的 `resource_id` + `external_key` 配对，并要求
/// `parse_native_item` 的结果重新渲染后与当前原生条目完全一致；这样中央记录
/// 与原生文档在采纳后仍共享同一个可观察表示，下一次预览才会是 in-sync。
/// 所有中央写入（MCP、managed item、target baseline）交给同一 IMMEDIATE
/// 事务完成，绝不调用 `readopt_mcp_target`，也不写原生文件。
pub fn adopt_mcp_native(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: AdoptMcpNativeInput,
) -> Result<AdoptMcpNativeResult, AppError> {
    if input.status == SyncStatus::ExternalNonOwnedChange {
        // 新增的原生条目不在 managed ownership 内；即使同一目标还有其它
        // 可采纳条目，也不能在这里静默忽略它。交给应用内匹配/导入流程。
        return Err(mcp_native_match_or_import());
    }
    if input.status != SyncStatus::ExternalOwnedChange {
        return Err(AppError::stale_preview(
            &input.preview_id,
            "mcpNativeStatus",
        ));
    }
    let expected_full_hash = input
        .observed_full_hash
        .as_deref()
        .ok_or_else(|| AppError::stale_preview(&input.preview_id, "mcpNativeFullHash"))?;
    let expected_managed_hash = input
        .observed_managed_hash
        .as_deref()
        .ok_or_else(|| AppError::stale_preview(&input.preview_id, "mcpNativeManagedHash"))?;
    if input.tool != input.descriptor.tool
        || input.descriptor.artifact_kind != ArtifactKind::Mcp
        || input.descriptor.scope
            != if input.project_id.is_some() {
                Scope::Project
            } else {
                Scope::Global
            }
        || input.descriptor.path.as_deref() != Some(input.target_path.as_str())
    {
        return Err(mcp_native_match_or_import());
    }

    let project = input
        .project_id
        .as_deref()
        .map(|id| repository::get_project(database, id))
        .transpose()?;
    let project_root = project
        .as_ref()
        .map(|project| canonical_project(&project.root_path))
        .transpose()?;
    let current_descriptor = mcp_target_descriptor(
        environment,
        input.tool,
        project_root.as_ref(),
        environment.claude_user_mcp_probe(),
        environment.claude_customization_policy_probe(),
    )?;
    if current_descriptor != input.descriptor
        || current_descriptor.path.as_deref() != Some(input.target_path.as_str())
    {
        return Err(AppError::stale_preview(
            &input.preview_id,
            "mcpNativeDescriptor",
        ));
    }

    let observed = match scan_target(input.tool.adapter(), &input.descriptor, &input.ownership) {
        TargetScan::Observed(observed) => observed,
        TargetScan::Missing => {
            return Err(AppError::stale_preview(
                &input.preview_id,
                "mcpNativeTarget",
            ));
        }
        TargetScan::ParseError
        | TargetScan::PermissionDenied
        | TargetScan::TargetTypeChanged(_)
        | TargetScan::Failed
        | TargetScan::Unavailable
        | TargetScan::ManagedItemBaselineMismatch => return Err(mcp_native_match_or_import()),
    };
    if observed.full_hash != expected_full_hash {
        return Err(AppError::stale_preview(
            &input.preview_id,
            "mcpNativeFullHash",
        ));
    }
    if observed.managed_hash != expected_managed_hash {
        return Err(AppError::stale_preview(
            &input.preview_id,
            "mcpNativeManagedHash",
        ));
    }
    let container = native_container(input.tool);
    let native_items = projection_value_at(&observed.managed_projection, container)
        .and_then(Value::as_object)
        .ok_or_else(mcp_native_match_or_import)?;
    // `managed_projection` 只包含 ownership 选中的 selector。再从完整的已解析
    // 文档读取一次 MCP 容器，用于发现“只差大小写”的未受管同名条目；否则
    // `mcpServers/foo` 与 `mcpServers/Foo` 可能被投影成一个条目而被错误采纳。
    let all_native_items = native_items_from_document(observed.document(), input.tool, container)
        .ok_or_else(mcp_native_match_or_import)?;
    let existing_items = repository::list_managed_mcp_items(database, &input.target_id)?;
    if existing_items.is_empty() {
        return Err(mcp_native_match_or_import());
    }
    let managed_names = existing_items
        .iter()
        .map(|item| item.external_key.as_str())
        .collect::<BTreeSet<_>>();
    // 项目目标的 ownership 还会选择全局继承名称；若该名称实际出现在项目
    // 文件中，它与本项目 managed item 的含义发生遮蔽，不能在采纳时默默带入
    // 基线，否则下一次中央投影不会 in-sync。
    if native_items
        .keys()
        .any(|name| !managed_names.contains(name.as_str()))
    {
        return Err(mcp_native_match_or_import());
    }
    let records = repository::list_mcp_servers(database)?;
    let records_by_id = records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let mut used_resources = BTreeSet::new();
    let mut used_external_keys = BTreeSet::new();
    let mut adoptions = Vec::with_capacity(existing_items.len());
    for item in &existing_items {
        if !used_resources.insert(item.resource_id.clone())
            || !used_external_keys.insert(item.external_key.clone())
        {
            return Err(mcp_native_match_or_import());
        }
        let record = records_by_id
            .get(item.resource_id.as_str())
            .copied()
            .ok_or_else(mcp_native_match_or_import)?;
        // resource_id 与 external_key 都是身份合同的一部分；名称重命名、大小写
        // 替换或把另一份中央 MCP 映射进来都不能由采纳动作猜测。
        if record.name != item.external_key || !record.enabled {
            return Err(mcp_native_match_or_import());
        }
        let matching_names = native_items
            .keys()
            .filter(|name| name.eq_ignore_ascii_case(&item.external_key))
            .count();
        let all_matching_names = all_native_items
            .keys()
            .filter(|name| name.eq_ignore_ascii_case(&item.external_key))
            .count();
        if matching_names != 1 || all_matching_names != 1 {
            return Err(mcp_native_match_or_import());
        }
        let raw = native_items
            .get(&item.external_key)
            .ok_or_else(mcp_native_match_or_import)?;
        register_native_projection_secrets(redactor, raw);
        let configuration =
            super::import::parse_native_item(input.tool, &item.external_key, raw, redactor)
                .map_err(|_| mcp_native_match_or_import())?;
        if configuration.name != item.external_key
            || configuration
                .command
                .as_deref()
                .is_some_and(|value| redactor.contains_secret(value))
            || configuration
                .url
                .as_deref()
                .is_some_and(|value| redactor.contains_secret(value))
            || configuration
                .args
                .iter()
                .any(|value| redactor.contains_secret(value))
            || redactor.contains_secret(&configuration.name)
        {
            return Err(mcp_native_match_or_import());
        }
        register_configuration_secrets(redactor, &configuration);
        // 严格 round-trip 是“无损”的可执行判据：显式 default（例如 Claude 的
        // enabled=true）、被适配器规范化的字段和任何未知 transport 都转入
        // 应用内匹配/导入，而不是采纳后制造下一次漂移。
        let rendered = native_mcp_item(input.tool, &configuration)
            .map_err(|_| mcp_native_match_or_import())?;
        if rendered != *raw {
            return Err(mcp_native_match_or_import());
        }
        let expected_item_row_version = safe_row_version(item.row_version)?;
        let expected_resource_row_version = records_by_id
            .get(item.resource_id.as_str())
            .map(|record| safe_row_version(record.row_version))
            .transpose()?
            .ok_or_else(|| AppError::stale_preview(&input.preview_id, "mcpNativeServer"))?;
        adoptions.push(import_repository::NativeMcpAdoptionItem {
            id: item.id.clone(),
            resource_id: item.resource_id.clone(),
            external_key: item.external_key.clone(),
            expected_item_row_version,
            expected_resource_row_version,
            item_hash: hash_json(raw),
            configuration,
        });
    }

    // 计划的 row_versions 包含中央 MCP 记录（包括项目继承项）、managed item 和
    // 项目状态。仓储层会在事务内拒绝缺失或不匹配的证据；这里的轻量预检先给出
    // 稳定的 stale 原因，避免继续解析更多私密字段。
    for adoption in &adoptions {
        if !input.row_versions.iter().any(|row| {
            row.entity_type == crate::sync::DatabaseEntityType::McpServer
                && row.entity_id == adoption.resource_id
                && row.row_version == adoption.expected_resource_row_version
        }) || !input.row_versions.iter().any(|row| {
            row.entity_type == crate::sync::DatabaseEntityType::ManagedItem
                && row.entity_id == adoption.id
                && row.row_version == adoption.expected_item_row_version
        }) {
            return Err(AppError::stale_preview(
                &input.preview_id,
                "mcpNativeRowVersions",
            ));
        }
    }

    let target = import_repository::NativeMcpAdoptionTarget {
        preview_id: input.preview_id.clone(),
        tool: input.tool,
        scope: input.descriptor.scope,
        project_id: input.project_id.clone(),
        target_id: input.target_id,
        target_path: input.target_path,
        target_row_version: input.target_row_version,
        observed_full_hash: observed.full_hash.clone(),
        observed_managed_hash: observed.managed_hash.clone(),
        baseline_projection: observed.managed_projection.clone(),
        expected_row_versions: input.row_versions,
    };
    let descriptor = input.descriptor;
    let ownership = input.ownership;
    let tool = input.tool;
    let preview_id = input.preview_id;
    let expected_full_hash = expected_full_hash.to_owned();
    let validate_source = || match scan_target(tool.adapter(), &descriptor, &ownership) {
        TargetScan::Observed(observed) if observed.full_hash == expected_full_hash => Ok(()),
        TargetScan::Observed(_) => Err(AppError::stale_preview(&preview_id, "mcpNativeFullHash")),
        TargetScan::Missing => Err(AppError::stale_preview(&preview_id, "mcpNativeTarget")),
        _ => Err(mcp_native_match_or_import()),
    };
    let result =
        import_repository::adopt_native_mcp(database, &target, &adoptions, validate_source)?;
    Ok(AdoptMcpNativeResult {
        adopted_item_count: result.adopted_item_count,
        updated_server_count: result.updated_server_count,
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
    /// Pi 专用：全局受管条目被该项目共享文件同名遮蔽时提供的硬阻断诊断码。
    hard_block: Option<String>,
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
    let readopt_available = crate::sync::assess_drift_with_managed_item_mismatches(
        &descriptor,
        &baseline,
        &scan,
        &baseline_mismatched_items,
    )
    .status
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
    // Pi：项目作用域下，本项目实际受管的 MCP 名称（项目自有 + 全局继承）若在
    // `<root>/.mcp.json` 或用户手写的 `<root>/.pi/mcp.json` 中已有同名条目，
    // 则本次写入在该项目下不可信（同名含义冲突），必须硬阻断而不是静默写入。
    // 只读检测，不返回任何条目内容。
    let hard_block = if input.tool == Tool::Pi && scope == Scope::Project {
        match project_root.as_ref() {
            None => None,
            Some(root) => {
                let managed_names = desired_records
                    .iter()
                    .chain(inherited_records.iter())
                    .map(|record| record.name.clone())
                    .collect::<Vec<_>>();
                crate::adapters::pi::detect_mcp_shadowing(
                    Path::new(root.as_str()),
                    &managed_names,
                    Scope::Global,
                )
                .map(|shadowing| shadowing.source.diagnostic_code().to_owned())
            }
        }
    } else {
        None
    };
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
            hard_block,
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

fn native_items_from_document<'a>(
    document: &'a ObservedDocument,
    tool: Tool,
    container: &[&str],
) -> Option<&'a Map<String, Value>> {
    let value = match document {
        ObservedDocument::Json(value)
        | ObservedDocument::Jsonc { value, .. }
        | ObservedDocument::Toml {
            semantic: value, ..
        } => value,
        ObservedDocument::Markdown(_) | ObservedDocument::SymlinkDirectory(_) => return None,
    };
    if tool == Tool::Pi {
        let object = value.as_object()?;
        return object
            .get("mcpServers")
            .or_else(|| object.get("mcp-servers"))
            .and_then(Value::as_object);
    }
    projection_value_at(value, container).and_then(Value::as_object)
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
        // Pi MCP 投影：`mcpServers` 条目级；适配器靠字段存在性推断传输，
        // 因此**不写 `type`**；停用映射为 `disabled: true`。未受管字段
        // （oauth/socket/directTools/lifecycle/bearerToken* 等）来自 `extra`，
        // 已在上方克隆进 object，零求值、零丢弃。
        (Tool::Pi, McpTransport::Stdio) => {
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
            if !value.enabled {
                object.insert("disabled".to_owned(), Value::Bool(true));
            }
        }
        (Tool::Pi, McpTransport::StreamableHttp) => {
            object.insert("url".to_owned(), Value::String(http_url(value)?));
            if !value.headers.is_empty() {
                object.insert("headers".to_owned(), string_map_value(&value.headers)?);
            }
            if !value.enabled {
                object.insert("disabled".to_owned(), Value::Bool(true));
            }
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
    // 条目发生漂移时保留完整的 observed document 与 hash。Apply 必须绑定这份
    // observation，确保第二个写者只能得到 STALE_PREVIEW，而不是被静默覆盖。
    if matches {
        (scan, Vec::new())
    } else {
        (scan, mismatched)
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
        affected_sync_scopes: None,
    })
}

fn with_affected_sync_scopes(mut dto: McpServerDto, scopes: Vec<SyncScopeDto>) -> McpServerDto {
    dto.affected_sync_scopes = Some(crate::domain::stable_sync_scopes(scopes));
    dto
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
