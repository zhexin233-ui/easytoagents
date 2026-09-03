//! 项目原生 Skill / MCP / Prompt 的只读发现、对账与禁用/恢复 Preview。

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde_json::{json, Map, Value};

use super::{
    ApplyProjectNativeResourcePreviewInput, PreviewProjectNativeResourceActionInput,
    ProjectNativeEntryType, ProjectNativeResourceAction, ProjectNativeResourceDto,
    ProjectNativeResourceQueryInput, ProjectNativeResourceState, ProjectNativeResourceSummaryDto,
};
use crate::{
    adapters::{
        canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
        cursor::CursorAdapter, DirectoryEntry, DiscoveryContext, ExplicitEnvironment,
        ManagedOwnership, ObservedDocument, PolicyState, TargetDescriptor, TargetFormat,
        TargetTrustState, ToolAdapter,
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
    for descriptor in supported_project_descriptors(environment, &project_root)? {
        reconcile_descriptor(database, &record.id, &descriptor)?;
    }
    summarize_project(database, &record.id)
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
        .map_err(|_| AppError::not_found("preview", &input.preview_id))?;
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
        delete_target: evidence.action == NativeResourceActionKind::Disable
            && evidence.entry_type == NativeResourceEntryType::PromptFile,
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
                    ArtifactKind::Mcp | ArtifactKind::Skill | ArtifactKind::Prompt
                )
        }));
    }
    Ok(descriptors)
}

fn reconcile_descriptor(
    database: &mut Database,
    project_id: &str,
    descriptor: &TargetDescriptor,
) -> Result<(), AppError> {
    let Some(target_path) = descriptor.path.as_deref() else {
        return Ok(());
    };
    if descriptor.capability.state != crate::adapters::CapabilityState::Supported {
        return Ok(());
    }
    let identity = repository::insert_project_target_identity(
        database,
        project_id,
        descriptor.tool,
        descriptor.artifact_kind,
        target_path,
    )?;
    let observed = match observe_items(database, descriptor, &identity.target_id)? {
        Some(items) => items,
        None => return Ok(()),
    };
    let occupied_keys = observed
        .iter()
        .map(|item| item.external_key.clone())
        .collect::<Vec<_>>();
    for item in &observed {
        let existing =
            repository::find_by_target_key(database, &identity.target_id, &item.external_key)?;
        if item.centrally_owned {
            if existing.is_some() {
                repository::upsert_observed_active(
                    database,
                    &identity.target_id,
                    &item.external_key,
                    item.entry_type.as_str(),
                    &item.item_hash,
                )?;
            }
            continue;
        }
        repository::upsert_observed_active(
            database,
            &identity.target_id,
            &item.external_key,
            item.entry_type.as_str(),
            &item.item_hash,
        )?;
    }
    repository::restore_conflict_when_vacant(database, &identity.target_id, &occupied_keys)?;
    repository::mark_active_missing(database, &identity.target_id, &occupied_keys)?;
    Ok(())
}

fn observe_items(
    database: &Database,
    descriptor: &TargetDescriptor,
    target_id: &str,
) -> Result<Option<Vec<ObservedNativeItem>>, AppError> {
    let adapter = tool_adapter(descriptor.tool);
    match descriptor.artifact_kind {
        ArtifactKind::Mcp => observe_mcp_items(database, adapter, descriptor, target_id),
        ArtifactKind::Skill => observe_skill_items(database, adapter, descriptor, target_id),
        ArtifactKind::Prompt => observe_prompt_item(database, adapter, descriptor, target_id),
        ArtifactKind::Provider => Ok(None),
    }
}

fn observe_mcp_items(
    database: &Database,
    adapter: &dyn ToolAdapter,
    descriptor: &TargetDescriptor,
    target_id: &str,
) -> Result<Option<Vec<ObservedNativeItem>>, AppError> {
    let container = native_mcp_container(descriptor.tool);
    let ownership = ManagedOwnership::selectors([[container]]);
    let scan = scan_target(adapter, descriptor, &ownership);
    let TargetScan::Observed(observed) = scan else {
        return Ok(match scan {
            TargetScan::Missing => Some(Vec::new()),
            _ => None,
        });
    };
    let Some(servers) = observed
        .managed_projection
        .get(container)
        .and_then(Value::as_object)
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

fn observe_prompt_item(
    database: &Database,
    adapter: &dyn ToolAdapter,
    descriptor: &TargetDescriptor,
    target_id: &str,
) -> Result<Option<Vec<ObservedNativeItem>>, AppError> {
    let scan = scan_target(adapter, descriptor, &ManagedOwnership::WholeDocument);
    let TargetScan::Observed(observed) = scan else {
        return Ok(match scan {
            TargetScan::Missing => Some(Vec::new()),
            _ => None,
        });
    };
    let path = Path::new(descriptor.path.as_deref().expect("Prompt 目标必须有路径"));
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(Some(Vec::new()));
    };
    let managed = match descriptor.artifact_kind {
        ArtifactKind::Prompt => prompt_target_is_managed(database, target_id)?,
        _ => false,
    };
    Ok(Some(vec![ObservedNativeItem {
        external_key: file_name.to_owned(),
        entry_type: ProjectNativeEntryType::PromptFile,
        item_hash: observed.managed_hash.clone(),
        centrally_owned: managed,
    }]))
}

fn prompt_target_is_managed(database: &Database, target_id: &str) -> Result<bool, AppError> {
    let path = database.path().to_string_lossy();
    let count: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM managed_items WHERE target_id = ?1 AND resource_kind = 'prompt'",
            [target_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::database(&path, "count_prompt_managed_items"))?;
    Ok(count > 0)
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
    let adapter = tool_adapter(descriptor.tool);
    let scan = scan_target(adapter, &descriptor, &ownership);
    validate_live_occupancy(input.action, entry_type, &scan, &descriptor, &record)?;
    if descriptor.artifact_kind == ArtifactKind::Mcp {
        if let TargetScan::Observed(observed) = &scan {
            let value = observed
                .managed_projection
                .get(native_mcp_container(descriptor.tool))
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
        if let Some(item) = desired
            .get(native_mcp_container(descriptor.tool))
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
            let desired = match entry_type {
                ProjectNativeEntryType::PromptFile => json!(""),
                _ => json!({}),
            };
            Ok((
                desired,
                ProjectNativeResourceEvidence {
                    resource_id: record.id.clone(),
                    resource_row_version: u32::try_from(record.row_version).map_err(|_| {
                        AppError::invalid_input("rowVersion", "原生资源版本超出范围")
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
                    resource_row_version: u32::try_from(record.row_version).map_err(|_| {
                        AppError::invalid_input("rowVersion", "原生资源版本超出范围")
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
            let link_target = fs::read_link(&child)
                .map_err(|_| AppError::stale_preview("persisted", &record.id))?;
            Ok((None, Some(link_target.to_string_lossy().into_owned()), None))
        }
        ProjectNativeEntryType::PromptFile => {
            let metadata = fs::symlink_metadata(path)
                .map_err(|_| AppError::stale_preview("persisted", &record.id))?;
            Ok((None, None, Some(metadata.permissions().mode() & 0o7777)))
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
            let bytes = fs::read(&snapshot.snapshot_path)
                .map_err(|_| AppError::not_found("snapshot", &snapshot.snapshot_path))?;
            let document = parse_config_value(descriptor.format, &bytes)?;
            let container = native_mcp_container(descriptor.tool);
            let item = document
                .get(container)
                .and_then(Value::as_object)
                .and_then(|servers| servers.get(&record.external_key))
                .cloned()
                .ok_or_else(|| AppError::conflict("snapshot", "禁用快照中找不到原 MCP 条目"))?;
            let mut servers = Map::new();
            servers.insert(record.external_key.clone(), item);
            let mut root = Map::new();
            root.insert(container.to_owned(), Value::Object(servers));
            Ok(Value::Object(root))
        }
        ProjectNativeEntryType::PromptFile => {
            let bytes = fs::read(&snapshot.snapshot_path)
                .map_err(|_| AppError::not_found("snapshot", &snapshot.snapshot_path))?;
            let text = String::from_utf8(bytes)
                .map_err(|_| AppError::invalid_input("snapshot", "提示词快照不是 UTF-8"))?;
            Ok(Value::String(text))
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
                ProjectNativeEntryType::McpEntry | ProjectNativeEntryType::PromptFile => {
                    unreachable!()
                }
            };
            let mut root = Map::new();
            root.insert(
                record.external_key.clone(),
                serde_json::to_value(entry).map_err(|_| {
                    AppError::invalid_input("desiredProjection", "Skill 入口投影无法序列化")
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
        NativeResourceActionKind::Disable => Ok(match evidence.entry_type {
            NativeResourceEntryType::PromptFile => json!(""),
            _ => json!({}),
        }),
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
                    NativeResourceEntryType::PromptFile => ProjectNativeEntryType::PromptFile,
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
            let container = native_mcp_container(descriptor.tool);
            let value = observed
                .managed_projection
                .get(container)
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
        ProjectNativeEntryType::PromptFile => Ok(observed.managed_hash.clone()),
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
            ProjectNativeEntryType::PromptFile => {
                if matches!(scan, TargetScan::Missing) {
                    Ok(())
                } else {
                    Err(AppError::conflict(
                        "targetPath",
                        "恢复目标已被占用，拒绝覆盖",
                    ))
                }
            }
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
                    Err(_) => Err(AppError::permission(
                        &child.to_string_lossy(),
                        "lstat_native_skill",
                    )),
                }
            }
        },
    }
}

fn projection_key_absent(scan: &TargetScan, descriptor: &TargetDescriptor, key: &str) -> bool {
    match scan {
        TargetScan::Observed(observed) => observed
            .managed_projection
            .get(native_mcp_container(descriptor.tool))
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
        ArtifactKind::Mcp => Ok(ManagedOwnership::selectors([[
            native_mcp_container(tool),
            external_key,
        ]])),
        ArtifactKind::Skill => Ok(ManagedOwnership::SymlinkNames(
            vec![external_key.to_owned()],
        )),
        ArtifactKind::Prompt => Ok(ManagedOwnership::WholeDocument),
        ArtifactKind::Provider => Err(AppError::invalid_input(
            "artifactKind",
            "Provider 不是项目原生资源",
        )),
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
    if record.state == "disabled" || record.state == "conflict" {
        return Ok(false);
    }
    let owned = match record.artifact_kind.as_str() {
        "mcp" => mcp_repository::list_managed_mcp_items(database, &record.target_id)?
            .into_iter()
            .any(|item| item.external_key == record.external_key),
        "skill" => skill_repository::list_managed_skill_items(database, &record.target_id)?
            .into_iter()
            .any(|item| item.external_key == record.external_key),
        "prompt" => prompt_target_is_managed(database, &record.target_id)?,
        _ => false,
    };
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
        row_version: u32::try_from(record.row_version)
            .map_err(|_| AppError::invalid_input("rowVersion", "原生资源版本超出范围"))?,
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
        ArtifactKind::Prompt => json!({ "kind": "prompt" }),
        ArtifactKind::Provider => json!({}),
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
        ProjectNativeEntryType::PromptFile => NativeResourceEntryType::PromptFile,
    }
}

fn entry_type_record(entry_type: NativeResourceEntryType) -> &'static str {
    match entry_type {
        NativeResourceEntryType::McpEntry => "mcp_entry",
        NativeResourceEntryType::Directory => "directory",
        NativeResourceEntryType::Symlink => "symlink",
        NativeResourceEntryType::PromptFile => "prompt_file",
    }
}

fn parse_config_value(format: TargetFormat, bytes: &[u8]) -> Result<Value, AppError> {
    match format {
        TargetFormat::Json => {
            serde_json::from_slice(bytes).map_err(|_| AppError::parse("snapshot", format.as_str()))
        }
        TargetFormat::Toml => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| AppError::parse("snapshot", format.as_str()))?;
            toml_edit::de::from_str(text).map_err(|_| AppError::parse("snapshot", format.as_str()))
        }
        TargetFormat::Markdown | TargetFormat::SymlinkDirectory => Err(AppError::invalid_input(
            "snapshot",
            "该快照格式不是 MCP 配置",
        )),
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
            ProjectNativeEntryType::McpEntry | ProjectNativeEntryType::PromptFile => {
                hash_json(fallback)
            }
        },
        Err(_) => hash_json(fallback),
    }
}

fn native_mcp_container(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude | Tool::Cursor => "mcpServers",
        Tool::Codex => "mcp_servers",
    }
}

fn tool_adapter(tool: Tool) -> &'static dyn ToolAdapter {
    match tool {
        Tool::Claude => &ClaudeAdapter,
        Tool::Codex => &CodexAdapter,
        Tool::Cursor => &CursorAdapter,
    }
}

fn tool_adapters() -> [&'static dyn ToolAdapter; 3] {
    [&ClaudeAdapter, &CodexAdapter, &CursorAdapter]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::ToolAvailability,
        adapters::VerifiedClaudeCustomizationPolicyEvidence,
        error::ErrorCode,
        mcp::{preview_mcp_sync, PreviewMcpSyncInput},
        projects::RegisterProjectInput,
        sync::{
            delete_snapshots, detect_interrupted_run, ApplyFaultDecision, ApplyFaultEvent,
            ApplyFaultInjector, DeleteSnapshotsInput,
        },
    };
    use rusqlite::params;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::tempdir;
    use uuid::Uuid;

    struct CrashBeforeTarget;
    impl ApplyFaultInjector for CrashBeforeTarget {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            match event {
                ApplyFaultEvent::BeforeTarget { .. } => ApplyFaultDecision::Crash,
                _ => ApplyFaultDecision::Continue,
            }
        }
    }

    struct FailAfterTarget;
    impl ApplyFaultInjector for FailAfterTarget {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            match event {
                ApplyFaultEvent::AfterTarget { .. } => ApplyFaultDecision::Fail,
                _ => ApplyFaultDecision::Continue,
            }
        }
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        database: Database,
        environment: ExplicitEnvironment,
        paths: AppPaths,
        home: PathBuf,
        write_operations: Mutex<()>,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let home = fs::canonicalize(temporary.path()).unwrap();
            fs::create_dir_all(home.join(".claude")).unwrap();
            fs::create_dir_all(home.join(".codex")).unwrap();
            fs::create_dir_all(home.join(".cursor")).unwrap();
            let environment =
                ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                    .unwrap()
                    .with_claude_installation_version("fixture-1.0.0")
                    .unwrap()
                    .with_claude_customization_policy_evidence(
                        VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                            "fixture-1.0.0",
                            None,
                        )
                        .unwrap(),
                    );
            let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
            paths.initialize().unwrap();
            let database = Database::open(&paths).unwrap();
            Self {
                _temporary: temporary,
                database,
                environment,
                paths,
                home,
                write_operations: Mutex::new(()),
            }
        }

        fn register_project_with(
            &mut self,
            files: impl FnOnce(&Path),
        ) -> crate::projects::ProjectDto {
            let root = self.home.join("projects/native");
            fs::create_dir_all(&root).unwrap();
            files(&root);
            crate::projects::register_project(
                &mut self.database,
                &self.environment,
                &RegisterProjectInput {
                    display_name: "原生项目".to_owned(),
                    root_path: root.to_string_lossy().into_owned(),
                },
            )
            .unwrap()
        }
    }

    fn write_skill(dir: &Path, name: &str, body: &str) {
        let skill = dir.join(name);
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Fixture skill\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    #[test]
    fn registration_discovers_native_items_without_rewriting() {
        let mut fixture = Fixture::new();
        let before_mcp = br#"{"mcpServers":{"native-stdio":{"command":"npx","env":{"API_KEY":"sk-native-secret"}}}}"#;
        let project = fixture.register_project_with(|root| {
            fs::write(root.join(".mcp.json"), before_mcp).unwrap();
            fs::write(root.join("CLAUDE.md"), "# native prompt\n").unwrap();
            write_skill(&root.join(".claude/skills"), "native-dir", "workflow");
            let link = root.join(".claude/skills/native-link");
            symlink(root.join(".claude/skills/native-dir"), &link).unwrap();
        });
        assert_eq!(project.native_resources.active, 4);
        let after = fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap();
        assert_eq!(after, before_mcp);
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].display_name, "native-stdio");
        assert!(items[0].can_disable);
        let serialized = serde_json::to_string(&items[0]).unwrap();
        assert!(!serialized.contains("sk-native-secret"));
        assert!(!serialized.contains("npx"));
    }

    #[test]
    fn cursor_project_prompt_is_not_discovered() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(root.join("AGENTS.md"), "# codex prompt\n").unwrap();
        });
        let cursor_prompts = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Cursor,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap();
        assert!(cursor_prompts.is_empty());
        let codex_prompts = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Codex,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap();
        assert_eq!(codex_prompts.len(), 1);
        assert_eq!(codex_prompts[0].display_name, "AGENTS.md");
    }

    #[test]
    fn mcp_disable_restore_preserves_siblings_and_blocks_snapshot_delete() {
        let mut fixture = Fixture::new();
        let secret = "sk-native-secret";
        let project = fixture.register_project_with(|root| {
            fs::write(
                root.join(".mcp.json"),
                format!(
                    r#"{{"theme":"keep","mcpServers":{{"native-stdio":{{"command":"npx","env":{{"API_KEY":"{secret}"}}}},"sibling":{{"command":"keep"}}}}}}"#
                ),
            )
            .unwrap();
        });
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        let native = items
            .into_iter()
            .find(|item| item.display_name == "native-stdio")
            .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: native.id.clone(),
                row_version: native.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let preview_json = serde_json::to_string(&preview).unwrap();
        assert!(!preview_json.contains(secret));
        assert!(preview
            .warning_codes
            .iter()
            .any(|code| code == "PROJECT_NATIVE_RESOURCE_REQUIRES_CONFIRMATION"));
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-stdio")
        .unwrap();
        assert_eq!(disabled.state, ProjectNativeResourceState::Disabled);
        let native_file: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
        )
        .unwrap();
        assert!(native_file["mcpServers"].get("native-stdio").is_none());
        assert_eq!(native_file["mcpServers"]["sibling"]["command"], "keep");
        assert_eq!(native_file["theme"], "keep");
        let snapshot_id = repository::get_by_id(&fixture.database, &disabled.id)
            .unwrap()
            .disabled_snapshot_id
            .unwrap();
        let deletion = delete_snapshots(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id.clone()],
            },
        )
        .unwrap();
        assert_eq!(deletion.deleted_ids.len(), 0);
        assert_eq!(deletion.failures.len(), 1);
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        assert!(!serde_json::to_string(&restore).unwrap().contains(secret));
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            restored["mcpServers"]["native-stdio"]["env"]["API_KEY"],
            secret
        );
        assert_eq!(restored["mcpServers"]["sibling"]["command"], "keep");
        let deletion = delete_snapshots(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id],
            },
        )
        .unwrap();
        assert_eq!(deletion.deleted_ids.len(), 1);
    }

    #[test]
    fn codex_mcp_disable_restore_preserves_toml_comments_and_siblings() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".codex")).unwrap();
            fs::write(
                root.join(".codex/config.toml"),
                "# keep-sibling-comment\n[mcp_servers.sibling]\ncommand = \"keep\"\n\n[mcp_servers.native-toml]\ncommand = \"npx\"\n",
            )
            .unwrap();
        });
        let canonical = fs::canonicalize(fixture.home.join("projects/native")).unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                canonical.display()
            ),
        )
        .unwrap();
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Codex,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        let native = items
            .into_iter()
            .find(|item| item.display_name == "native-toml")
            .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: native.id.clone(),
                row_version: native.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        let after = fs::read_to_string(canonical.join(".codex/config.toml")).unwrap();
        assert!(after.contains("keep-sibling-comment"));
        assert!(after.contains("[mcp_servers.sibling]"));
        assert!(!after.contains("[mcp_servers.native-toml]"));
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Codex,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-toml")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored = fs::read_to_string(canonical.join(".codex/config.toml")).unwrap();
        assert!(restored.contains("[mcp_servers.native-toml]"));
        assert!(restored.contains("[mcp_servers.sibling]"));
        assert!(restored.contains("keep-sibling-comment"));
    }

    #[test]
    fn cursor_mcp_disable_restore_preserves_sibling_servers() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".cursor")).unwrap();
            fs::write(
                root.join(".cursor/mcp.json"),
                r#"{"mcpServers":{"native-cursor":{"command":"npx"},"sibling":{"command":"keep"}}}"#,
            )
            .unwrap();
        });
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Cursor,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        let native = items
            .into_iter()
            .find(|item| item.display_name == "native-cursor")
            .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: native.id.clone(),
                row_version: native.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        let native_file: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.cursor/mcp.json")).unwrap(),
        )
        .unwrap();
        assert!(native_file["mcpServers"].get("native-cursor").is_none());
        assert_eq!(native_file["mcpServers"]["sibling"]["command"], "keep");
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Cursor,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-cursor")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.cursor/mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(restored["mcpServers"]["native-cursor"]["command"], "npx");
        assert_eq!(restored["mcpServers"]["sibling"]["command"], "keep");
    }

    #[test]
    fn prompt_disable_restore_keeps_bytes_and_mode() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            let path = root.join("CLAUDE.md");
            fs::write(&path, b"# exact bytes\n").unwrap();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o640);
            fs::set_permissions(&path, permissions).unwrap();
        });
        let item = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id.clone(),
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        assert!(!fixture.home.join("projects/native/CLAUDE.md").exists());
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let path = fixture.home.join("projects/native/CLAUDE.md");
        assert_eq!(fs::read(&path).unwrap(), b"# exact bytes\n");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o640
        );
    }

    #[test]
    fn skill_directory_and_symlink_disable_restore() {
        let mut fixture = Fixture::new();
        let home = fixture.home.clone();
        let project = fixture.register_project_with(|root| {
            write_skill(&root.join(".claude/skills"), "native-dir", "keep-bytes");
            write_skill(&home, "external-skill", "external");
            symlink(
                home.join("external-skill"),
                root.join(".claude/skills/native-link"),
            )
            .unwrap();
            fs::write(root.join(".claude/skills/.DS_Store"), b"ignore").unwrap();
        });
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        let directory = items
            .iter()
            .find(|item| item.display_name == "native-dir")
            .unwrap()
            .clone();
        assert!(items.iter().any(|item| item.display_name == "native-link"));
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: directory.id.clone(),
                row_version: directory.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        assert!(!fixture
            .home
            .join("projects/native/.claude/skills/native-dir")
            .exists());
        assert!(fixture
            .home
            .join("projects/native/.claude/skills/.DS_Store")
            .exists());
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-dir")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored = fixture
            .home
            .join("projects/native/.claude/skills/native-dir/SKILL.md");
        assert!(fs::read_to_string(restored).unwrap().contains("keep-bytes"));

        let link = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-link")
        .unwrap();
        let outside_before = fs::canonicalize(fixture.home.join("external-skill")).unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: link.id.clone(),
                row_version: link.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        assert!(!fixture
            .home
            .join("projects/native/.claude/skills/native-link")
            .exists());
        assert_eq!(
            fs::canonicalize(fixture.home.join("external-skill")).unwrap(),
            outside_before
        );
        assert!(fixture.home.join("external-skill/SKILL.md").exists());
        let disabled_link = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-link")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore_link = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled_link.id.clone(),
                row_version: disabled_link.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore_link.preview_id,
            },
        )
        .unwrap();
        let restored_link = fixture
            .home
            .join("projects/native/.claude/skills/native-link");
        assert_eq!(
            fs::read_link(&restored_link).unwrap(),
            fixture.home.join("external-skill")
        );
        assert_eq!(
            fs::canonicalize(fixture.home.join("external-skill")).unwrap(),
            outside_before
        );
    }

    #[test]
    fn restore_occupancy_and_project_removal_are_blocked() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(root.join("CLAUDE.md"), "# native\n").unwrap();
        });
        let item = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id.clone(),
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        fs::write(
            fixture.home.join("projects/native/CLAUDE.md"),
            "# occupied\n",
        )
        .unwrap();
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        assert_eq!(disabled.state, ProjectNativeResourceState::Conflict);
        let mut redactor = SecretRedactor::default();
        let error = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        let remove = crate::projects::remove_project(
            &mut fixture.database,
            &crate::projects::VersionedProjectInput {
                id: project.id.clone(),
                row_version: project.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(remove.code(), ErrorCode::Conflict);
    }

    #[test]
    fn empty_identity_row_does_not_open_ordinary_mcp_apply() {
        let mut fixture = Fixture::new();
        let before = br#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#;
        let project = fixture.register_project_with(|root| {
            fs::write(root.join(".mcp.json"), before).unwrap();
        });
        let identity = repository::find_project_target_identity(
            &fixture.database,
            &project.id,
            Tool::Claude,
            ArtifactKind::Mcp,
            fixture
                .home
                .join("projects/native/.mcp.json")
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap()
        .expect("登记后应有空 baseline 目标身份行");
        assert!(identity.full_hash.is_none());
        assert!(identity.managed_hash.is_none());
        let mut redactor = SecretRedactor::default();
        let preview = preview_mcp_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: Some(project.id),
                exclude_from_git: false,
            },
        )
        .unwrap();
        assert!(preview.targets.is_empty());
        assert_eq!(
            fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
            before
        );
    }

    #[test]
    fn centrally_owned_mcp_item_is_hidden_from_native_list() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(
                root.join(".mcp.json"),
                r#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#,
            )
            .unwrap();
        });
        let identity = repository::find_project_target_identity(
            &fixture.database,
            &project.id,
            Tool::Claude,
            ArtifactKind::Mcp,
            fixture
                .home
                .join("projects/native/.mcp.json")
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap()
        .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_items(
                    id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash
                 ) VALUES (?1, ?2, 'mcp', ?3, 'native-stdio', ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    identity.target_id,
                    Uuid::new_v4().to_string(),
                    "a".repeat(64)
                ],
            )
            .unwrap();
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn action_matrix_rejects_restore_on_active_and_disable_on_disabled() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(root.join("CLAUDE.md"), "# native\n").unwrap();
        });
        let item = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let restore_active = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id.clone(),
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap_err();
        assert_eq!(restore_active.code(), ErrorCode::InvalidInput);
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id.clone(),
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let disable_again = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id,
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap_err();
        assert_eq!(disable_again.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn consumed_preview_and_active_writer_are_rejected() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(root.join("CLAUDE.md"), "# native\n").unwrap();
        });
        let item = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id.clone(),
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id.clone(),
            },
        )
        .unwrap();
        let consumed = apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap_err();
        assert_eq!(consumed.code(), ErrorCode::PreviewAlreadyConsumed);

        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version)
                 VALUES (?1, 'apply', 'applying', 'project', ?2, 0)",
                params![Uuid::new_v4().to_string(), project.id],
            )
            .unwrap();
        let restored = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.state == ProjectNativeResourceState::Disabled)
        .unwrap();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: restored.id,
                row_version: restored.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        let writer = apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap_err();
        assert_eq!(writer.code(), ErrorCode::WriteInProgress);
    }

    #[test]
    fn native_apply_crash_before_target_preserves_bytes_and_blocks_writer() {
        let mut fixture = Fixture::new();
        let before = br#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#;
        let project = fixture.register_project_with(|root| {
            fs::write(root.join(".mcp.json"), before).unwrap();
        });
        let item = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id,
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let error = apply_project_native_resource_preview_with_fault(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id.clone(),
            },
            &CrashBeforeTarget,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        assert_eq!(
            fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
            before
        );
        let recovery = detect_interrupted_run(&fixture.database, &fixture.paths)
            .unwrap()
            .expect("崩溃后必须检测到活动 run");
        assert_eq!(recovery.run_id, preview.preview_id);
        assert!(recovery.journal_available);
    }

    #[test]
    fn native_skill_symlink_failure_rolls_back_without_central_root() {
        let mut fixture = Fixture::new();
        let home = fixture.home.clone();
        let project = fixture.register_project_with(|root| {
            write_skill(&home, "external-skill", "external");
            fs::create_dir_all(root.join(".claude/skills")).unwrap();
            symlink(
                home.join("external-skill"),
                root.join(".claude/skills/native-link"),
            )
            .unwrap();
        });
        let link = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-link")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: link.id,
                row_version: link.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let error = apply_project_native_resource_preview_with_fault(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
            &FailAfterTarget,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        let restored = fixture
            .home
            .join("projects/native/.claude/skills/native-link");
        assert_eq!(
            fs::read_link(&restored).unwrap(),
            fixture.home.join("external-skill")
        );
        assert!(fixture.home.join("external-skill/SKILL.md").exists());
    }

    #[test]
    fn native_skill_directory_failure_rolls_back_tree() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            write_skill(&root.join(".claude/skills"), "native-dir", "keep-bytes");
        });
        let directory = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-dir")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: directory.id,
                row_version: directory.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let error = apply_project_native_resource_preview_with_fault(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
            &FailAfterTarget,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        let restored = fixture
            .home
            .join("projects/native/.claude/skills/native-dir/SKILL.md");
        assert!(fs::read_to_string(restored).unwrap().contains("keep-bytes"));
    }

    #[test]
    fn skill_disable_restore_works_for_codex_and_cursor() {
        for tool in [Tool::Codex, Tool::Cursor] {
            let mut fixture = Fixture::new();
            let relative = match tool {
                Tool::Codex => ".codex/skills",
                Tool::Cursor => ".cursor/skills",
                Tool::Claude => unreachable!(),
            };
            let project = fixture.register_project_with(|root| {
                write_skill(&root.join(relative), "native-dir", "platform-bytes");
            });
            if tool == Tool::Codex {
                let canonical = fs::canonicalize(fixture.home.join("projects/native")).unwrap();
                fs::write(
                    fixture.home.join(".codex/config.toml"),
                    format!(
                        "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                        canonical.display()
                    ),
                )
                .unwrap();
            }
            let items = list_project_native_resources(
                &mut fixture.database,
                &fixture.environment,
                &ProjectNativeResourceQueryInput {
                    project_id: project.id.clone(),
                    tool,
                    artifact_kind: ArtifactKind::Skill,
                },
            )
            .unwrap();
            let directory = items
                .into_iter()
                .find(|item| item.display_name == "native-dir")
                .unwrap_or_else(|| panic!("{tool:?} 应发现项目 Skill"));
            let mut redactor = SecretRedactor::default();
            let preview = preview_project_native_resource_action(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &PreviewProjectNativeResourceActionInput {
                    resource_id: directory.id.clone(),
                    row_version: directory.row_version,
                    action: ProjectNativeResourceAction::Disable,
                },
            )
            .unwrap();
            apply_project_native_resource_preview(
                &fixture.write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &ApplyProjectNativeResourcePreviewInput {
                    preview_id: preview.preview_id,
                },
            )
            .unwrap();
            assert!(!fixture
                .home
                .join("projects/native")
                .join(relative)
                .join("native-dir")
                .exists());
            let disabled = list_project_native_resources(
                &mut fixture.database,
                &fixture.environment,
                &ProjectNativeResourceQueryInput {
                    project_id: project.id.clone(),
                    tool,
                    artifact_kind: ArtifactKind::Skill,
                },
            )
            .unwrap()
            .into_iter()
            .find(|item| item.display_name == "native-dir")
            .unwrap();
            let restore = preview_project_native_resource_action(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &PreviewProjectNativeResourceActionInput {
                    resource_id: disabled.id,
                    row_version: disabled.row_version,
                    action: ProjectNativeResourceAction::Restore,
                },
            )
            .unwrap();
            apply_project_native_resource_preview(
                &fixture.write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &ApplyProjectNativeResourcePreviewInput {
                    preview_id: restore.preview_id,
                },
            )
            .unwrap();
            let restored = fixture
                .home
                .join("projects/native")
                .join(relative)
                .join("native-dir/SKILL.md");
            assert!(
                fs::read_to_string(restored)
                    .unwrap()
                    .contains("platform-bytes"),
                "{tool:?} Skill 恢复应还原正文"
            );
        }
    }

    #[test]
    fn codex_prompt_disable_restore_keeps_bytes_and_mode() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            let path = root.join("AGENTS.md");
            fs::write(&path, b"# exact agents\n").unwrap();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o640);
            fs::set_permissions(&path, permissions).unwrap();
        });
        let canonical = fs::canonicalize(fixture.home.join("projects/native")).unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                canonical.display()
            ),
        )
        .unwrap();
        let item = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Codex,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id,
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        assert!(!canonical.join("AGENTS.md").exists());
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Codex,
                artifact_kind: ArtifactKind::Prompt,
            },
        )
        .unwrap()
        .remove(0);
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id,
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let path = canonical.join("AGENTS.md");
        assert_eq!(fs::read(&path).unwrap(), b"# exact agents\n");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o640
        );
    }
}
