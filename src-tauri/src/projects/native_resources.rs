//! 项目原生 Skill / MCP 的只读发现、对账与禁用/恢复 Preview。

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::TransactionBehavior;
use serde_json::{json, Map, Value};

use super::{
    ApplyProjectNativeResourcePreviewInput, PreviewProjectNativeResourceActionInput,
    ProjectNativeEntryType, ProjectNativeResourceAction, ProjectNativeResourceDto,
    ProjectNativeResourceQueryInput, ProjectNativeResourceState, ProjectNativeResourceSummaryDto,
};
use crate::{
    adapters::{
        canonicalize_project_root, native_mcp_container, projection_value_at, DirectoryEntry,
        DiscoveryContext, ExplicitEnvironment, ManagedOwnership, ObservedDocument, PolicyState,
        TargetDescriptor, TargetFormat, TargetTrustState, ToolAdapter,
    },
    app::AppPaths,
    db::{
        mcp as mcp_repository, native_resources as repository, projects as project_repository,
        skills as skill_repository, Database,
    },
    domain::{ArtifactKind, ChangeKind, ProjectRoot, Scope, TargetType, Tool},
    error::AppError,
    git::inspect_path,
    mcp::register_native_projection_secrets,
    security::SecretRedactor,
    skills::library as skill_library,
    sync::{
        apply_persisted_preview, build_preview_plan, hash_json, load_persisted_preview,
        persist_preview, scan_target, ApplyFaultInjector, ApplyResult, ApplyTargetInput,
        DatabaseEntityType, DatabaseRowVersion, ManagedTargetBaseline, NativeResourceActionKind,
        NativeResourceEntryType, NoApplyFault, PreviewPlan, PreviewTargetRequest,
        ProjectNativeResourceEvidence, TargetScan,
    },
};

const SKIPPED_SKILL_NAMES: &[&str] = &[".DS_Store", ".system", ".", ".."];

struct ObservedNativeItem {
    external_key: String,
    entry_type: ProjectNativeEntryType,
    item_hash: String,
    centrally_owned: bool,
}

/// 对账项目原生资源观测：整个项目的全部描述符在一个 IMMEDIATE 事务内写入，
/// 中途任一 SQL 失败即整体回滚，不会留下"部分工具已更新、其余仍旧"的半更新表。
/// 原生文件与目录的扫描在事务开始前完成（只读），事务内只做数据库写入。
pub fn reconcile_project_native_resources(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    project_id: &str,
) -> Result<ProjectNativeResourceSummaryDto, AppError> {
    let record = project_repository::get_registered_project(database, project_id)?;
    let project_root = match canonicalize_project_root(Path::new(&record.root_path)) {
        Ok(root) if root.as_str() == record.root_path => root,
        _ => return Ok(ProjectNativeResourceSummaryDto::empty()),
    };
    let mut observations = Vec::new();
    for descriptor in supported_project_descriptors(environment, &project_root)? {
        if let Some(observation) = observe_descriptor(database, &record.id, &descriptor)? {
            observations.push(observation);
        }
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_reconcile_native_resources")
                .with_source(error)
        })?;
    for observation in &observations {
        reconcile_observation(&transaction, &database_path, &record.id, observation)?;
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_reconcile_native_resources").with_source(error)
    })?;
    summarize_project(database, &record.id)
}

/// 当前对账后的项目原生资源汇总，不触碰原生文件也不写库。
pub(super) fn project_native_resource_summary(
    database: &Database,
    project_id: &str,
) -> Result<ProjectNativeResourceSummaryDto, AppError> {
    summarize_project(database, project_id)
}

pub fn list_project_native_resources(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &ProjectNativeResourceQueryInput,
) -> Result<Vec<ProjectNativeResourceDto>, AppError> {
    reconcile_project_native_resources(database, environment, &input.project_id)?;
    let records = repository::list_for_project(
        database,
        &input.project_id,
        Some(input.tool),
        Some(input.artifact_kind),
    )?;
    let mut dtos = Vec::new();
    for record in records {
        if should_hide_centralized(&record, database)? {
            continue;
        }
        dtos.push(to_dto(&record)?);
    }
    Ok(dtos)
}

pub fn preview_project_native_resource_action(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &PreviewProjectNativeResourceActionInput,
) -> Result<PreviewPlan, AppError> {
    let prepared = prepare_native_action(database, environment, redactor, input)?;
    let plan = build_preview_plan(
        Scope::Project,
        Some(prepared.project_id.clone()),
        vec![prepared.request],
        redactor,
    )?;
    if plan
        .targets
        .iter()
        .any(|target| target.change_kind == ChangeKind::Conflict || target.error_code.is_some())
    {
        return Err(AppError::conflict(
            "projectNativeResource",
            "当前原生资源状态不允许该动作",
        ));
    }
    persist_preview(database, &plan)?;
    Ok(plan)
}

pub fn apply_project_native_resource_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    input: &ApplyProjectNativeResourcePreviewInput,
) -> Result<ApplyResult, AppError> {
    apply_project_native_resource_preview_with_fault(
        write_operations,
        database,
        paths,
        environment,
        input,
        &NoApplyFault,
    )
}

pub(crate) fn apply_project_native_resource_preview_with_fault(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    input: &ApplyProjectNativeResourcePreviewInput,
    fault: &dyn ApplyFaultInjector,
) -> Result<ApplyResult, AppError> {
    let _ = environment;
    let persisted = load_persisted_preview(database, &input.preview_id)?;
    let run_status: String = database
        .connection()
        .query_row(
            "SELECT status FROM sync_runs WHERE id = ?1",
            [&input.preview_id],
            |row| row.get(0),
        )
        .map_err(|error| AppError::not_found("preview", &input.preview_id).with_source(error))?;
    if run_status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &input.preview_id,
            &run_status,
        ));
    }
    let item = persisted
        .items
        .first()
        .ok_or_else(|| AppError::stale_preview(&input.preview_id, "nativeTarget"))?;
    let evidence = item
        .envelope
        .project_native_action
        .clone()
        .ok_or_else(|| AppError::invalid_input("previewId", "该预览不是项目原生资源动作"))?;
    let record = repository::get_by_id(database, &evidence.resource_id)?;
    validate_action_matrix(
        parse_state(&record.state)?,
        evidence_action(evidence.action),
    )?;
    if record.row_version != i64::from(evidence.resource_row_version)
        || record.target_id != item.target_id
    {
        return Err(AppError::stale_preview(&input.preview_id, &record.id));
    }
    let project = project_repository::get_registered_project(
        database,
        persisted
            .project_id
            .as_deref()
            .ok_or_else(|| AppError::invalid_input("previewId", "项目原生资源预览缺少项目标识"))?,
    )?;
    let project_root = canonicalize_project_root(Path::new(&project.root_path))?;
    if project_root.as_str() != project.root_path {
        return Err(AppError::conflict(
            "projectRoot",
            "登记项目根与当前 canonical 路径不一致",
        ));
    }
    let desired = rebuild_desired_projection(database, &item.envelope.descriptor, &evidence)?;
    let apply_input = ApplyTargetInput {
        descriptor: item.envelope.descriptor.clone(),
        ownership: item.envelope.ownership.clone(),
        desired_projection: desired,
        allowed_root: PathBuf::from(project_root.as_str()),
        central_skills_root: None,
        delete_target: false,
        managed_items: Vec::new(),
        remove_managed_item_ids: Vec::new(),
        skill_takeover_entries: Vec::new(),
        project_native_action: Some(evidence),
    };
    apply_persisted_preview(
        write_operations,
        database,
        paths,
        &input.preview_id,
        &[apply_input],
        fault,
    )
}

pub(super) fn supported_project_descriptors(
    environment: &ExplicitEnvironment,
    project_root: &ProjectRoot,
) -> Result<Vec<TargetDescriptor>, AppError> {
    let context = DiscoveryContext {
        environment,
        project_root: Some(project_root),
        claude_user_mcp_probe: environment.claude_user_mcp_probe(),
        claude_customization_policy_probe: environment.claude_customization_policy_probe(),
    };
    let mut descriptors = Vec::new();
    for adapter in tool_adapters() {
        descriptors.extend(adapter.discover(&context)?.into_iter().filter(|target| {
            target.scope == Scope::Project
                && target.path.is_some()
                && matches!(
                    target.artifact_kind,
                    ArtifactKind::Mcp | ArtifactKind::Skill
                )
        }));
    }
    Ok(descriptors)
}

struct DescriptorObservation {
    tool: Tool,
    artifact_kind: ArtifactKind,
    target_path: String,
    items: Vec<ObservedNativeItem>,
}

/// 只读观测一个描述符；返回 `None` 表示该目标不参与对账（无路径、能力未证明或扫描不可用）。
fn observe_descriptor(
    database: &Database,
    project_id: &str,
    descriptor: &TargetDescriptor,
) -> Result<Option<DescriptorObservation>, AppError> {
    let Some(target_path) = descriptor.path.as_deref() else {
        return Ok(None);
    };
    if descriptor.capability.state != crate::adapters::CapabilityState::Supported {
        return Ok(None);
    }
    // 观测需要已有身份行来读取 managed_items；尚无身份行时按"无中央所有权"观测。
    let target_id = repository::find_project_target_identity(
        database,
        project_id,
        descriptor.tool,
        descriptor.artifact_kind,
        target_path,
    )?
    .map(|identity| identity.target_id)
    .unwrap_or_default();
    let Some(items) = observe_items(database, descriptor, &target_id)? else {
        return Ok(None);
    };
    Ok(Some(DescriptorObservation {
        tool: descriptor.tool,
        artifact_kind: descriptor.artifact_kind,
        target_path: target_path.to_owned(),
        items,
    }))
}

fn reconcile_observation(
    transaction: &rusqlite::Transaction<'_>,
    database_path: &str,
    project_id: &str,
    observation: &DescriptorObservation,
) -> Result<(), AppError> {
    let identity = repository::insert_project_target_identity_in(
        transaction,
        database_path,
        project_id,
        observation.tool,
        observation.artifact_kind,
        &observation.target_path,
    )?;
    let occupied_keys = observation
        .items
        .iter()
        .map(|item| item.external_key.clone())
        .collect::<Vec<_>>();
    for item in &observation.items {
        if item.centrally_owned {
            // 中央资源占用同一路径时，旧禁用记录仍需对账为冲突并保留快照。
            // 没有原生记录的中央资源不应被登记为新的原生资源。
            let existing = repository::find_by_target_key_in(
                transaction,
                database_path,
                &identity.target_id,
                &item.external_key,
            )?;
            if existing.is_none() {
                continue;
            }
        }
        repository::upsert_observed_active_in(
            transaction,
            database_path,
            &identity.target_id,
            &item.external_key,
            item.entry_type.as_str(),
            &item.item_hash,
        )?;
    }
    repository::restore_conflict_when_vacant_in(
        transaction,
        database_path,
        &identity.target_id,
        &occupied_keys,
    )?;
    repository::mark_active_missing_in(
        transaction,
        database_path,
        &identity.target_id,
        &occupied_keys,
    )?;
    Ok(())
}

fn observe_items(
    database: &Database,
    descriptor: &TargetDescriptor,
    target_id: &str,
) -> Result<Option<Vec<ObservedNativeItem>>, AppError> {
    let adapter = descriptor.tool.adapter();
    match descriptor.artifact_kind {
        ArtifactKind::Mcp => observe_mcp_items(database, adapter, descriptor, target_id),
        ArtifactKind::Skill => observe_skill_items(database, adapter, descriptor, target_id),
        // Hooks 不参与项目原生资源逐条观测（MVP 范围外）。
        ArtifactKind::Hook | ArtifactKind::Prompt | ArtifactKind::Provider => Ok(None),
    }
}

fn observe_mcp_items(
    database: &Database,
    adapter: &dyn ToolAdapter,
    descriptor: &TargetDescriptor,
    target_id: &str,
) -> Result<Option<Vec<ObservedNativeItem>>, AppError> {
    let container = native_mcp_container(descriptor.tool);
    let ownership = ManagedOwnership::selectors([container
        .iter()
        .copied()
        .map(str::to_owned)
        .collect::<Vec<_>>()]);
    let scan = scan_target(adapter, descriptor, &ownership);
    let TargetScan::Observed(observed) = scan else {
        return Ok(match scan {
            TargetScan::Missing => Some(Vec::new()),
            _ => None,
        });
    };
    let Some(servers) =
        projection_value_at(&observed.managed_projection, container).and_then(Value::as_object)
    else {
        return Ok(Some(Vec::new()));
    };
    let managed = mcp_repository::list_managed_mcp_items(database, target_id)?;
    let managed_by_key = managed
        .into_iter()
        .map(|item| (item.external_key, item.last_applied_item_hash))
        .collect::<BTreeMap<_, _>>();
    let mut items = Vec::new();
    for (name, value) in servers {
        let item_hash = hash_json(value);
        let centrally_owned = managed_by_key.contains_key(name);
        items.push(ObservedNativeItem {
            external_key: name.clone(),
            entry_type: ProjectNativeEntryType::McpEntry,
            item_hash,
            centrally_owned,
        });
    }
    Ok(Some(items))
}

fn observe_skill_items(
    database: &Database,
    adapter: &dyn ToolAdapter,
    descriptor: &TargetDescriptor,
    target_id: &str,
) -> Result<Option<Vec<ObservedNativeItem>>, AppError> {
    let scan = scan_target(
        adapter,
        descriptor,
        &ManagedOwnership::SymlinkNames(Vec::new()),
    );
    let TargetScan::Observed(observed) = scan else {
        return Ok(match scan {
            TargetScan::Missing => Some(Vec::new()),
            _ => None,
        });
    };
    let ObservedDocument::SymlinkDirectory(entries) = observed.document() else {
        return Ok(None);
    };
    let managed = skill_repository::list_managed_skill_items(database, target_id)?;
    let managed_by_key = managed
        .into_iter()
        .map(|item| (item.external_key, item.last_applied_item_hash))
        .collect::<BTreeMap<_, _>>();
    let mut items = Vec::new();
    for (name, entry) in entries {
        if SKIPPED_SKILL_NAMES.contains(&name.as_str()) {
            continue;
        }
        let entry_type = match entry.target_type {
            TargetType::Directory => ProjectNativeEntryType::Directory,
            TargetType::Symlink => ProjectNativeEntryType::Symlink,
            TargetType::File | TargetType::Missing => continue,
        };
        let fallback = serde_json::to_value(entry).unwrap_or(Value::Null);
        let item_hash = skill_entry_item_hash(
            &Path::new(descriptor.path.as_deref().unwrap_or_default()).join(name),
            entry_type,
            &fallback,
        );
        items.push(ObservedNativeItem {
            external_key: name.clone(),
            entry_type,
            item_hash,
            centrally_owned: managed_by_key.contains_key(name),
        });
    }
    Ok(Some(items))
}

struct PreparedNativeAction {
    project_id: String,
    request: PreviewTargetRequest,
}

fn prepare_native_action(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &PreviewProjectNativeResourceActionInput,
) -> Result<PreparedNativeAction, AppError> {
    let record = repository::get_by_id(database, &input.resource_id)?;
    if record.row_version != i64::from(input.row_version) {
        return Err(AppError::conflict("rowVersion", "原生资源已被其他操作更新"));
    }
    let state = parse_state(&record.state)?;
    validate_action_matrix(state, input.action)?;
    if should_hide_centralized(&record, database)? && state == ProjectNativeResourceState::Active {
        return Err(AppError::conflict(
            "projectNativeResource",
            "该条目已由中央资源托管，不能作为项目原生资源操作",
        ));
    }
    let project = project_repository::get_registered_project(database, &record.project_id)?;
    let project_root = canonicalize_project_root(Path::new(&project.root_path))?;
    if project_root.as_str() != project.root_path {
        return Err(AppError::conflict(
            "projectRoot",
            "登记项目根与当前 canonical 路径不一致",
        ));
    }
    let descriptor = supported_project_descriptors(environment, &project_root)?
        .into_iter()
        .find(|candidate| {
            candidate.tool.as_str() == record.tool
                && candidate.artifact_kind.as_str() == record.artifact_kind
                && candidate.path.as_deref() == Some(record.target_path.as_str())
        })
        .ok_or_else(|| AppError::not_found("nativeTarget", &record.target_path))?;
    validate_descriptor_writable(&descriptor)?;
    let entry_type = ProjectNativeEntryType::from_stable_str(&record.entry_type)
        .ok_or_else(|| AppError::invalid_input("entryType", "原生资源入口类型无效"))?;
    let ownership = native_ownership(
        descriptor.tool,
        descriptor.artifact_kind,
        &record.external_key,
    )?;
    let adapter = descriptor.tool.adapter();
    let scan = scan_target(adapter, &descriptor, &ownership);
    validate_live_occupancy(input.action, entry_type, &scan, &descriptor, &record)?;
    if descriptor.artifact_kind == ArtifactKind::Mcp {
        if let TargetScan::Observed(observed) = &scan {
            let value = projection_value_at(
                &observed.managed_projection,
                native_mcp_container(descriptor.tool),
            )
            .and_then(Value::as_object)
            .and_then(|servers| servers.get(&record.external_key))
            .unwrap_or(&observed.managed_projection);
            register_native_projection_secrets(redactor, value);
        }
    }
    let (desired, evidence) = build_action_projection(
        database,
        &descriptor,
        &record,
        input.action,
        entry_type,
        &scan,
    )?;
    if descriptor.artifact_kind == ArtifactKind::Mcp {
        if let Some(item) = projection_value_at(&desired, native_mcp_container(descriptor.tool))
            .and_then(Value::as_object)
            .and_then(|servers| servers.get(&record.external_key))
        {
            register_native_projection_secrets(redactor, item);
        }
    }
    let git = inspect_path(&project_root, Path::new(&record.target_path)).ok();
    let request = PreviewTargetRequest {
        descriptor,
        ownership,
        baseline: ManagedTargetBaseline {
            target_id: record.target_id.clone(),
            target_row_version: record.target_row_version,
            full_hash: None,
            managed_hash: None,
        },
        scan,
        baseline_mismatched_items: Vec::new(),
        readopt_available: false,
        desired_projection: desired,
        row_versions: vec![DatabaseRowVersion {
            entity_type: DatabaseEntityType::ProjectNativeResource,
            entity_id: record.id.clone(),
            row_version: input.row_version,
        }],
        git,
        exclude_from_git: false,
        skill_takeover_entries: Vec::new(),
        project_native_action: Some(evidence),
        hook_initial_adopt: false,
    };
    Ok(PreparedNativeAction {
        project_id: record.project_id,
        request,
    })
}

fn build_action_projection(
    database: &Database,
    descriptor: &TargetDescriptor,
    record: &repository::NativeResourceRecord,
    action: ProjectNativeResourceAction,
    entry_type: ProjectNativeEntryType,
    scan: &TargetScan,
) -> Result<(Value, ProjectNativeResourceEvidence), AppError> {
    match action {
        ProjectNativeResourceAction::Disable => {
            let observed_hash = match scan {
                TargetScan::Observed(observed) => {
                    item_hash_from_scan(descriptor, &record.external_key, observed, entry_type)?
                }
                _ => {
                    return Err(AppError::conflict(
                        "projectNativeResource",
                        "禁用时目标条目已不存在",
                    ));
                }
            };
            if record
                .observed_item_hash
                .as_deref()
                .is_some_and(|hash| hash != observed_hash)
            {
                return Err(AppError::stale_preview("persisted", &record.id));
            }
            let (content_hash, link_target, file_mode) =
                disable_evidence_details(descriptor, record, entry_type)?;
            let desired = json!({});
            Ok((
                desired,
                ProjectNativeResourceEvidence {
                    resource_id: record.id.clone(),
                    resource_row_version: u32::try_from(record.row_version).map_err(|error| {
                        AppError::invalid_input("rowVersion", "原生资源版本超出范围")
                            .with_source(error)
                    })?,
                    action: NativeResourceActionKind::Disable,
                    entry_type: evidence_entry_type(entry_type),
                    external_key: record.external_key.clone(),
                    observed_item_hash: observed_hash,
                    expected_fingerprint: None,
                    content_hash,
                    restore_snapshot_id: None,
                    restore_snapshot_path: None,
                    restore_link_target: link_target,
                    restore_file_mode: file_mode,
                },
            ))
        }
        ProjectNativeResourceAction::Restore => {
            let snapshot_id = record.disabled_snapshot_id.as_deref().ok_or_else(|| {
                AppError::conflict("projectNativeResource", "已禁用资源缺少可恢复快照")
            })?;
            let snapshot = repository::get_snapshot(database, snapshot_id)?;
            let desired = restore_desired_projection(descriptor, record, entry_type, &snapshot)?;
            let observed_hash = record.observed_item_hash.clone().ok_or_else(|| {
                AppError::conflict("projectNativeResource", "已禁用资源缺少观察 hash")
            })?;
            Ok((
                desired,
                ProjectNativeResourceEvidence {
                    resource_id: record.id.clone(),
                    resource_row_version: u32::try_from(record.row_version).map_err(|error| {
                        AppError::invalid_input("rowVersion", "原生资源版本超出范围")
                            .with_source(error)
                    })?,
                    action: NativeResourceActionKind::Restore,
                    entry_type: evidence_entry_type(entry_type),
                    external_key: record.external_key.clone(),
                    observed_item_hash: observed_hash,
                    expected_fingerprint: None,
                    content_hash: snapshot.content_hash.clone(),
                    restore_snapshot_id: Some(snapshot.id.clone()),
                    restore_snapshot_path: Some(snapshot.snapshot_path.clone()),
                    restore_link_target: snapshot.link_target.clone(),
                    restore_file_mode: snapshot.file_mode.and_then(|mode| u32::try_from(mode).ok()),
                },
            ))
        }
    }
}

type DisableEvidenceParts = (Option<String>, Option<String>, Option<u32>);

fn disable_evidence_details(
    descriptor: &TargetDescriptor,
    record: &repository::NativeResourceRecord,
    entry_type: ProjectNativeEntryType,
) -> Result<DisableEvidenceParts, AppError> {
    let path = Path::new(&record.target_path);
    match entry_type {
        ProjectNativeEntryType::Directory => {
            let child = path.join(&record.external_key);
            let inspection = skill_library::inspect_skill_takeover_entry(&child)?;
            Ok((Some(inspection.content_hash), None, None))
        }
        ProjectNativeEntryType::Symlink => {
            let child = path.join(&record.external_key);
            let link_target = fs::read_link(&child).map_err(|error| {
                AppError::stale_preview("persisted", &record.id).with_source(error)
            })?;
            Ok((None, Some(link_target.to_string_lossy().into_owned()), None))
        }
        ProjectNativeEntryType::McpEntry => {
            let _ = descriptor;
            Ok((None, None, None))
        }
    }
}

fn restore_desired_projection(
    descriptor: &TargetDescriptor,
    record: &repository::NativeResourceRecord,
    entry_type: ProjectNativeEntryType,
    snapshot: &repository::NativeSnapshotRecord,
) -> Result<Value, AppError> {
    match entry_type {
        ProjectNativeEntryType::McpEntry => {
            let bytes = fs::read(&snapshot.snapshot_path).map_err(|error| {
                AppError::not_found("snapshot", &snapshot.snapshot_path).with_source(error)
            })?;
            let document = parse_config_value(descriptor.format, &bytes)?;
            let container = native_mcp_container(descriptor.tool);
            let item = projection_value_at(&document, container)
                .and_then(Value::as_object)
                .and_then(|servers| servers.get(&record.external_key))
                .cloned()
                .ok_or_else(|| AppError::conflict("snapshot", "禁用快照中找不到原 MCP 条目"))?;
            let servers = Value::Object(Map::from_iter([(record.external_key.clone(), item)]));
            let root = container.iter().rev().fold(servers, |child, segment| {
                Value::Object(Map::from_iter([(segment.to_string(), child)]))
            });
            Ok(root)
        }
        ProjectNativeEntryType::Directory | ProjectNativeEntryType::Symlink => {
            let entry = match entry_type {
                ProjectNativeEntryType::Directory => DirectoryEntry {
                    target_type: TargetType::Directory,
                    link_target: None,
                },
                ProjectNativeEntryType::Symlink => DirectoryEntry {
                    target_type: TargetType::Symlink,
                    link_target: Some(snapshot.link_target.clone().ok_or_else(|| {
                        AppError::conflict("snapshot", "符号链接快照缺少链接目标")
                    })?),
                },
                ProjectNativeEntryType::McpEntry => {
                    return Err(AppError::internal("MCP 条目不应走 Skill 目录恢复投影"));
                }
            };
            let mut root = Map::new();
            root.insert(
                record.external_key.clone(),
                serde_json::to_value(entry).map_err(|error| {
                    AppError::invalid_input("desiredProjection", "Skill 入口投影无法序列化")
                        .with_source(error)
                })?,
            );
            Ok(Value::Object(root))
        }
    }
}

fn rebuild_desired_projection(
    database: &Database,
    descriptor: &TargetDescriptor,
    evidence: &ProjectNativeResourceEvidence,
) -> Result<Value, AppError> {
    match evidence.action {
        NativeResourceActionKind::Disable => Ok(json!({})),
        NativeResourceActionKind::Restore => {
            let snapshot_id = evidence.restore_snapshot_id.as_deref().ok_or_else(|| {
                AppError::invalid_input("projectNativeAction", "恢复缺少快照标识")
            })?;
            let snapshot = repository::get_snapshot(database, snapshot_id)?;
            let record = repository::NativeResourceRecord {
                id: evidence.resource_id.clone(),
                target_id: String::new(),
                project_id: String::new(),
                tool: descriptor.tool.as_str().to_owned(),
                artifact_kind: descriptor.artifact_kind.as_str().to_owned(),
                target_path: descriptor.path.clone().unwrap_or_default(),
                target_row_version: 0,
                external_key: evidence.external_key.clone(),
                entry_type: entry_type_record(evidence.entry_type).to_owned(),
                state: "disabled".to_owned(),
                observed_item_hash: Some(evidence.observed_item_hash.clone()),
                disabled_snapshot_id: Some(snapshot.id.clone()),
                disabled_at: None,
                last_seen_at: String::new(),
                row_version: 0,
            };
            restore_desired_projection(
                descriptor,
                &record,
                match evidence.entry_type {
                    NativeResourceEntryType::McpEntry => ProjectNativeEntryType::McpEntry,
                    NativeResourceEntryType::Directory => ProjectNativeEntryType::Directory,
                    NativeResourceEntryType::Symlink => ProjectNativeEntryType::Symlink,
                },
                &snapshot,
            )
        }
    }
}

fn item_hash_from_scan(
    descriptor: &TargetDescriptor,
    external_key: &str,
    observed: &crate::sync::ObservedTarget,
    entry_type: ProjectNativeEntryType,
) -> Result<String, AppError> {
    match entry_type {
        ProjectNativeEntryType::McpEntry => {
            let value = projection_value_at(
                &observed.managed_projection,
                native_mcp_container(descriptor.tool),
            )
            .and_then(Value::as_object)
            .and_then(|servers| servers.get(external_key))
            .ok_or_else(|| AppError::conflict("projectNativeResource", "MCP 条目已不存在"))?;
            Ok(hash_json(value))
        }
        ProjectNativeEntryType::Directory | ProjectNativeEntryType::Symlink => {
            let child =
                Path::new(descriptor.path.as_deref().unwrap_or_default()).join(external_key);
            let value = observed
                .managed_projection
                .get(external_key)
                .ok_or_else(|| AppError::conflict("projectNativeResource", "Skill 入口已不存在"))?;
            Ok(skill_entry_item_hash(&child, entry_type, value))
        }
    }
}

fn validate_live_occupancy(
    action: ProjectNativeResourceAction,
    entry_type: ProjectNativeEntryType,
    scan: &TargetScan,
    descriptor: &TargetDescriptor,
    record: &repository::NativeResourceRecord,
) -> Result<(), AppError> {
    match action {
        ProjectNativeResourceAction::Disable => match scan {
            TargetScan::Observed(_) => Ok(()),
            _ => Err(AppError::conflict(
                "projectNativeResource",
                "只有仍存在的活动原生资源可以禁用",
            )),
        },
        ProjectNativeResourceAction::Restore => match entry_type {
            ProjectNativeEntryType::McpEntry => {
                if matches!(scan, TargetScan::Missing)
                    || projection_key_absent(scan, descriptor, &record.external_key)
                {
                    Ok(())
                } else {
                    Err(AppError::conflict(
                        "targetPath",
                        "恢复目标已被占用，拒绝覆盖",
                    ))
                }
            }
            ProjectNativeEntryType::Directory | ProjectNativeEntryType::Symlink => {
                let child = Path::new(&record.target_path).join(&record.external_key);
                match fs::symlink_metadata(&child) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Ok(_) => Err(AppError::conflict(
                        "targetPath",
                        "恢复目标已被占用，拒绝覆盖",
                    )),
                    Err(error) => Err(AppError::permission(
                        &child.to_string_lossy(),
                        "lstat_native_skill",
                    )
                    .with_source(error)),
                }
            }
        },
    }
}

fn projection_key_absent(scan: &TargetScan, descriptor: &TargetDescriptor, key: &str) -> bool {
    match scan {
        TargetScan::Observed(observed) => projection_value_at(
            &observed.managed_projection,
            native_mcp_container(descriptor.tool),
        )
        .and_then(Value::as_object)
        .map_or(true, |servers| !servers.contains_key(key)),
        TargetScan::Missing => true,
        _ => false,
    }
}

fn validate_descriptor_writable(descriptor: &TargetDescriptor) -> Result<(), AppError> {
    let path = descriptor.path.as_deref().unwrap_or("");
    if descriptor.capability.state != crate::adapters::CapabilityState::Supported {
        return Err(AppError::invalid_input(
            "target",
            "当前工具不支持该项目原生目标",
        ));
    }
    if descriptor.policy != PolicyState::Allowed {
        let policy = match descriptor.policy {
            PolicyState::Allowed => "allowed",
            PolicyState::Blocked => "blocked",
            PolicyState::Unknown => "unknown",
        };
        return Err(AppError::policy_blocked(
            descriptor.tool.as_str(),
            path,
            policy,
        ));
    }
    match descriptor.trust {
        TargetTrustState::Untrusted | TargetTrustState::Unknown => {
            Err(AppError::untrusted_project(descriptor.tool.as_str(), path))
        }
        TargetTrustState::Trusted | TargetTrustState::NotRequired => Ok(()),
    }
}

fn native_ownership(
    tool: Tool,
    artifact_kind: ArtifactKind,
    external_key: &str,
) -> Result<ManagedOwnership, AppError> {
    match artifact_kind {
        ArtifactKind::Mcp => Ok(ManagedOwnership::selectors([native_mcp_container(tool)
            .iter()
            .copied()
            .chain(std::iter::once(external_key))
            .collect::<Vec<_>>()])),
        ArtifactKind::Skill => Ok(ManagedOwnership::SymlinkNames(
            vec![external_key.to_owned()],
        )),
        // Hooks 不参与项目原生资源的逐条停用/恢复（MVP 范围外），
        // 与 Provider 一样 fail closed。
        ArtifactKind::Hook | ArtifactKind::Prompt | ArtifactKind::Provider => Err(
            AppError::invalid_input("artifactKind", "该资源类型不是项目原生资源"),
        ),
    }
}

fn validate_action_matrix(
    state: ProjectNativeResourceState,
    action: ProjectNativeResourceAction,
) -> Result<(), AppError> {
    match (state, action) {
        (ProjectNativeResourceState::Active, ProjectNativeResourceAction::Disable)
        | (ProjectNativeResourceState::Disabled, ProjectNativeResourceAction::Restore) => Ok(()),
        (ProjectNativeResourceState::Active, ProjectNativeResourceAction::Restore)
        | (ProjectNativeResourceState::Disabled, ProjectNativeResourceAction::Disable) => Err(
            AppError::invalid_input("action", "当前状态不允许该原生资源动作"),
        ),
        (ProjectNativeResourceState::Missing | ProjectNativeResourceState::Conflict, _) => {
            Err(AppError::conflict(
                "projectNativeResource",
                "资源处于缺失或冲突状态，不能执行该动作",
            ))
        }
    }
}

fn should_hide_centralized(
    record: &repository::NativeResourceRecord,
    database: &Database,
) -> Result<bool, AppError> {
    let owned = match record.artifact_kind.as_str() {
        "mcp" => mcp_repository::list_managed_mcp_items(database, &record.target_id)?
            .into_iter()
            .any(|item| item.external_key == record.external_key),
        "skill" => skill_repository::list_managed_skill_items(database, &record.target_id)?
            .into_iter()
            .any(|item| item.external_key == record.external_key),
        _ => false,
    };
    // 禁用快照代表另一份待恢复的原生内容，不能因中央资源占用路径而隐藏。
    if record.state == "disabled" || record.state == "conflict" {
        return Ok(false);
    }
    Ok(owned)
}

fn summarize_project(
    database: &Database,
    project_id: &str,
) -> Result<ProjectNativeResourceSummaryDto, AppError> {
    let records = repository::list_for_project(database, project_id, None, None)?;
    let mut summary = ProjectNativeResourceSummaryDto::empty();
    for record in records {
        if should_hide_centralized(&record, database)? {
            continue;
        }
        match record.state.as_str() {
            "active" => summary.active += 1,
            "disabled" => summary.disabled += 1,
            "missing" => summary.missing += 1,
            "conflict" => summary.conflict += 1,
            _ => {}
        }
    }
    Ok(summary)
}

fn to_dto(record: &repository::NativeResourceRecord) -> Result<ProjectNativeResourceDto, AppError> {
    let tool = parse_tool(&record.tool)?;
    let artifact_kind = parse_artifact(&record.artifact_kind)?;
    let entry_type = ProjectNativeEntryType::from_stable_str(&record.entry_type)
        .ok_or_else(|| AppError::invalid_input("entryType", "原生资源入口类型无效"))?;
    let state = parse_state(&record.state)?;
    let mut diagnostic_codes = Vec::new();
    match state {
        ProjectNativeResourceState::Missing => {
            diagnostic_codes.push("PROJECT_NATIVE_RESOURCE_MISSING".to_owned());
        }
        ProjectNativeResourceState::Conflict => {
            diagnostic_codes.push("PROJECT_NATIVE_RESOURCE_CONFLICT".to_owned());
        }
        ProjectNativeResourceState::Disabled => {
            diagnostic_codes.push("PROJECT_NATIVE_RESOURCE_DISABLED".to_owned());
        }
        ProjectNativeResourceState::Active => {}
    }
    Ok(ProjectNativeResourceDto {
        id: record.id.clone(),
        project_id: record.project_id.clone(),
        tool,
        artifact_kind,
        display_name: record.external_key.clone(),
        target_path: record.target_path.clone(),
        entry_type,
        state,
        row_version: u32::try_from(record.row_version).map_err(|error| {
            AppError::invalid_input("rowVersion", "原生资源版本超出范围").with_source(error)
        })?,
        can_disable: state == ProjectNativeResourceState::Active,
        can_restore: state == ProjectNativeResourceState::Disabled,
        diagnostic_codes,
        safe_summary: safe_summary(artifact_kind, entry_type),
        disabled_at: record.disabled_at.clone(),
    })
}

fn safe_summary(artifact_kind: ArtifactKind, entry_type: ProjectNativeEntryType) -> Value {
    match artifact_kind {
        ArtifactKind::Mcp => json!({ "kind": "mcp" }),
        ArtifactKind::Skill => json!({ "entryType": entry_type.as_str() }),
        ArtifactKind::Prompt | ArtifactKind::Hook | ArtifactKind::Provider => json!({}),
    }
}

fn parse_state(value: &str) -> Result<ProjectNativeResourceState, AppError> {
    ProjectNativeResourceState::from_stable_str(value)
        .ok_or_else(|| AppError::invalid_input("state", "原生资源状态无效"))
}

fn parse_tool(value: &str) -> Result<Tool, AppError> {
    match value {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        "cursor" => Ok(Tool::Cursor),
        "zcode" => Ok(Tool::Zcode),
        "opencode" => Ok(Tool::Opencode),
        _ => Err(AppError::invalid_input("tool", "工具类型无效")),
    }
}

fn parse_artifact(value: &str) -> Result<ArtifactKind, AppError> {
    match value {
        "mcp" => Ok(ArtifactKind::Mcp),
        "skill" => Ok(ArtifactKind::Skill),
        "prompt" => Ok(ArtifactKind::Prompt),
        "provider" => Ok(ArtifactKind::Provider),
        _ => Err(AppError::invalid_input("artifactKind", "资源类型无效")),
    }
}

fn evidence_action(action: NativeResourceActionKind) -> ProjectNativeResourceAction {
    match action {
        NativeResourceActionKind::Disable => ProjectNativeResourceAction::Disable,
        NativeResourceActionKind::Restore => ProjectNativeResourceAction::Restore,
    }
}

fn evidence_entry_type(entry_type: ProjectNativeEntryType) -> NativeResourceEntryType {
    match entry_type {
        ProjectNativeEntryType::McpEntry => NativeResourceEntryType::McpEntry,
        ProjectNativeEntryType::Directory => NativeResourceEntryType::Directory,
        ProjectNativeEntryType::Symlink => NativeResourceEntryType::Symlink,
    }
}

fn entry_type_record(entry_type: NativeResourceEntryType) -> &'static str {
    match entry_type {
        NativeResourceEntryType::McpEntry => "mcp_entry",
        NativeResourceEntryType::Directory => "directory",
        NativeResourceEntryType::Symlink => "symlink",
    }
}

fn parse_config_value(format: TargetFormat, bytes: &[u8]) -> Result<Value, AppError> {
    match format {
        TargetFormat::Json => serde_json::from_slice(bytes)
            .map_err(|error| AppError::parse("snapshot", format.as_str()).with_source(error)),
        TargetFormat::Jsonc => {
            let text = std::str::from_utf8(bytes)
                .map_err(|error| AppError::parse("snapshot", format.as_str()).with_source(error))?;
            crate::adapters::parse_jsonc(text)
                .map_err(|error| AppError::parse("snapshot", format.as_str()).with_source(error))
        }
        TargetFormat::Toml => {
            let text = std::str::from_utf8(bytes)
                .map_err(|error| AppError::parse("snapshot", format.as_str()).with_source(error))?;
            toml_edit::de::from_str(text)
                .map_err(|error| AppError::parse("snapshot", format.as_str()).with_source(error))
        }
        TargetFormat::Markdown | TargetFormat::CursorMdc | TargetFormat::SymlinkDirectory => Err(
            AppError::invalid_input("snapshot", "该快照格式不是 MCP 配置"),
        ),
    }
}

fn skill_entry_item_hash(
    path: &Path,
    entry_type: ProjectNativeEntryType,
    fallback: &Value,
) -> String {
    match skill_library::inspect_skill_takeover_entry(path) {
        Ok(inspection) => match entry_type {
            ProjectNativeEntryType::Directory => inspection.content_hash,
            ProjectNativeEntryType::Symlink => inspection.fingerprint,
            ProjectNativeEntryType::McpEntry => hash_json(fallback),
        },
        Err(_) => hash_json(fallback),
    }
}

/// MCP 条目在原生文件中的容器路径；ZCode 是官方定义的嵌套键 `mcp.servers`。
fn tool_adapters() -> [&'static dyn ToolAdapter; 5] {
    Tool::ALL.map(|tool| tool.adapter())
}

#[cfg(test)]
include!("native_resources_tests.rs");
