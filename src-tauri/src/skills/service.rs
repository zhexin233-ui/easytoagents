//! Skills 中央库服务与持久化 Preview/Apply 编排。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{
    library::{
        cleanup_failed_import, delete_quarantined_skill, finalize_skill_import,
        inspect_central_skill, prepare_github_skill_import, prepare_skill_import,
        quarantine_central_skill, read_central_skill_for_adoption, rename_import_exclusively,
        restore_quarantined_skill, sync_directory, validate_central_skill_directory,
    },
    ApplySkillPreviewInput, DeleteSkillResultDto, ImportSkillInput, PreparedSkillRecord,
    PreviewSkillSyncInput, SetGlobalSkillAssignmentInput, SetProjectSkillAssignmentInput,
    SkillContentPreviewDto, SkillDto, SkillProjectDto, SkillProjectOptionDto,
    SkillProjectOptionsInput, SkillProjectSelectionState, SkillTargetStatusDto,
    VersionedSkillInput,
};
use crate::{
    adapters::{
        canonicalize_project_root, descriptor_allowed_root, find_descriptor,
        ClaudeCustomizationPolicyProbe, DiscoveryContext, ManagedOwnership, TargetDescriptor,
        ASSIGNABLE_SKILL_TOOLS,
    },
    app::AppPaths,
    db::{
        skills::{self as repository, ManagedSkillItemRecord, SkillProjectRecord, SkillRecord},
        Database,
    },
    domain::{ArtifactKind, ProjectRoot, Scope, SkillStatus, Tool},
    error::AppError,
    git::inspect_path,
    security::SecretRedactor,
    sync::managed::{
        collect_row_versions, list_global_target_statuses, project_dto as managed_project_dto,
        SkillManagedArtifact,
    },
    sync::{
        apply_persisted_preview, assess_drift, build_preview_plan, hash_json,
        load_persisted_preview, persist_preview, read_directory_target, safe_row_version,
        scan_target, ApplyResult, ApplyTargetInput, DatabaseEntityType, DatabaseRowVersion,
        ManagedItemApply, ManagedTargetBaseline, NoApplyFault, PreviewPlan, PreviewTargetRequest,
        SkillTakeoverEntry, TargetScan,
    },
};

pub fn list_skills(database: &Database, paths: &AppPaths) -> Result<Vec<SkillDto>, AppError> {
    // 列表只发两条 SQL（记�� + 全部全局分配），逐条组装不再回库。
    let mut global_tools = repository::global_tools_for_all_skills(database)?;
    repository::list_skills(database)?
        .iter()
        .map(|record| {
            skill_dto_with_tools(
                paths,
                record,
                global_tools.remove(&record.id).unwrap_or_default(),
            )
        })
        .collect()
}

pub fn get_skill(database: &Database, paths: &AppPaths, id: &str) -> Result<SkillDto, AppError> {
    let record = repository::get_skill(database, id)?;
    skill_dto(database, paths, &record)
}

pub fn import_skill(
    database: &mut Database,
    paths: &AppPaths,
    input: &ImportSkillInput,
) -> Result<SkillDto, AppError> {
    let mut prepared = prepare_skill_import(paths, Path::new(&input.source_path))?;
    if let Err(error) = finalize_skill_import(paths, &mut prepared) {
        cleanup_failed_import(paths, &prepared)?;
        return Err(error);
    }
    let value = PreparedSkillRecord {
        id: prepared.id.clone(),
        name: prepared.name.clone(),
        source_path: prepared.source_path.clone(),
        central_path: prepared.central_path.clone(),
        content_hash: prepared.content_hash.clone(),
        frontmatter: prepared.frontmatter.clone(),
    };
    let record = match repository::insert_skill(database, &value) {
        Ok(record) => record,
        Err(error) => {
            cleanup_failed_import(paths, &prepared)?;
            return Err(error);
        }
    };
    skill_dto(database, paths, &record)
}

pub fn import_downloaded_github_skill(
    database: &mut Database,
    paths: &AppPaths,
    source: &Path,
    normalized_url: &str,
) -> Result<SkillDto, AppError> {
    let mut prepared = prepare_github_skill_import(paths, source, normalized_url)?;
    if let Err(error) = finalize_skill_import(paths, &mut prepared) {
        cleanup_failed_import(paths, &prepared)?;
        return Err(error);
    }
    let value = PreparedSkillRecord {
        id: prepared.id.clone(),
        name: prepared.name.clone(),
        source_path: prepared.source_path.clone(),
        central_path: prepared.central_path.clone(),
        content_hash: prepared.content_hash.clone(),
        frontmatter: prepared.frontmatter.clone(),
    };
    match repository::insert_skill(database, &value) {
        Ok(record) => skill_dto(database, paths, &record),
        Err(error) => match repository::get_skill(database, &prepared.id) {
            Ok(record)
                if record.name == prepared.name
                    && record.source_path == prepared.source_path
                    && record.central_path == prepared.central_path
                    && record.content_hash == prepared.content_hash =>
            {
                skill_dto(database, paths, &record)
            }
            Err(not_found) if not_found.code() == crate::error::ErrorCode::NotFound => {
                cleanup_failed_import(paths, &prepared)?;
                Err(error)
            }
            _ => Err(error),
        },
    }
}

pub fn adopt_skill_content(
    database: &mut Database,
    paths: &AppPaths,
    input: &VersionedSkillInput,
) -> Result<SkillDto, AppError> {
    let record = repository::get_skill(database, &input.id)?;
    if u32::try_from(record.row_version).ok() != Some(input.row_version) {
        return Err(AppError::conflict("rowVersion", "Skill 已被其他操作修改"));
    }
    repository::reject_active_writer(database)?;
    let adopted =
        read_central_skill_for_adoption(paths, &record.id, &record.name, &record.central_path)?;
    if adopted.name != record.name {
        return Err(AppError::conflict(
            "name",
            "frontmatter.name 与记录名称不一致，不能作为内容采纳",
        ));
    }
    if adopted.content_hash == record.content_hash && record.status == SkillStatus::Ready {
        return skill_dto(database, paths, &record);
    }
    let record = repository::adopt_skill_content(
        database,
        &input.id,
        input.row_version,
        &adopted.content_hash,
        &adopted.frontmatter,
    )?;
    skill_dto(database, paths, &record)
}

pub fn preview_skill_content(
    database: &Database,
    paths: &AppPaths,
    id: &str,
) -> Result<SkillContentPreviewDto, AppError> {
    let record = repository::get_skill(database, id)?;
    let inspection = inspect_central_skill(
        paths,
        &record.id,
        &record.name,
        &record.central_path,
        &record.content_hash,
        record.status,
        true,
    )?;
    if inspection.status != SkillStatus::Ready {
        return Err(AppError::conflict(
            "centralSkill",
            "中央 Skill 已缺失或内容变化，不能提供正文预览",
        ));
    }
    Ok(SkillContentPreviewDto {
        id: record.id,
        name: record.name,
        skill_md: inspection.skill_md.unwrap_or_default(),
        files: inspection.files,
        content_hash: record.content_hash,
        row_version: safe_row_version(record.row_version)?,
    })
}

pub fn delete_skill(
    database: &mut Database,
    paths: &AppPaths,
    input: &VersionedSkillInput,
) -> Result<DeleteSkillResultDto, AppError> {
    let record = repository::ensure_skill_deletable(database, &input.id, input.row_version)?;
    let quarantine = quarantine_central_skill(
        paths,
        &record.id,
        &record.name,
        &record.central_path,
        &record.content_hash,
    )?;
    if let Err(error) = repository::delete_skill_record(database, &input.id, input.row_version) {
        if let Some(quarantine) = quarantine.as_deref() {
            restore_quarantined_skill(paths, quarantine, &record.central_path)?;
        }
        return Err(error);
    }
    if let Some(quarantine) = quarantine.as_deref() {
        delete_quarantined_skill(paths, quarantine, &record.content_hash)?;
    }
    Ok(DeleteSkillResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

pub fn set_global_skill_assignment(
    database: &mut Database,
    paths: &AppPaths,
    input: &SetGlobalSkillAssignmentInput,
) -> Result<SkillDto, AppError> {
    let record = repository::set_global_assignment(
        database,
        input.tool,
        &input.skill_id,
        input.assigned,
        input.row_version,
    )?;
    skill_dto(database, paths, &record)
}

pub fn set_project_skill_assignment(
    database: &mut Database,
    paths: &AppPaths,
    input: &SetProjectSkillAssignmentInput,
) -> Result<SkillDto, AppError> {
    let record = repository::set_project_assignment(
        database,
        &input.project_id,
        input.tool,
        &input.skill_id,
        input.assigned,
        input.skill_row_version,
        input.project_row_version,
    )?;
    skill_dto(database, paths, &record)
}

pub fn list_skill_projects(database: &Database) -> Result<Vec<SkillProjectDto>, AppError> {
    repository::list_projects(database)?
        .iter()
        .map(managed_project_dto::<SkillProjectRecord>)
        .map(|result| {
            result.map(|value| SkillProjectDto {
                id: value.id,
                display_name: value.display_name,
                root_path: value.root_path,
                codex_trust_status: value.codex_trust_status,
                row_version: value.row_version,
            })
        })
        .collect()
}

pub fn list_skill_project_options(
    database: &Database,
    paths: &AppPaths,
    input: &SkillProjectOptionsInput,
) -> Result<Vec<SkillProjectOptionDto>, AppError> {
    repository::get_project(database, &input.project_id)?;
    let global = repository::list_assigned_skills(database, input.tool, None)?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    let selected = repository::list_assigned_skills(database, input.tool, Some(&input.project_id))?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    repository::list_skills(database)?
        .into_iter()
        .map(|record| {
            let state = if global.contains(&record.id) {
                SkillProjectSelectionState::Inherited
            } else if selected.contains(&record.id) {
                SkillProjectSelectionState::Selected
            } else {
                SkillProjectSelectionState::Available
            };
            let inspection = inspect_record(paths, &record, false)?;
            Ok(SkillProjectOptionDto {
                skill_id: record.id,
                name: record.name,
                status: inspection.status,
                state,
                selectable: state == SkillProjectSelectionState::Selected
                    || (state == SkillProjectSelectionState::Available
                        && inspection.status == SkillStatus::Ready),
                row_version: safe_row_version(record.row_version)?,
            })
        })
        .collect()
}

pub fn preview_skill_sync(
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &SecretRedactor,
    input: &PreviewSkillSyncInput,
) -> Result<PreviewPlan, AppError> {
    preview_skill_sync_with_policy_probe(
        database,
        paths,
        environment,
        redactor,
        input,
        environment.claude_customization_policy_probe(),
    )
}

pub fn preview_skill_sync_with_policy_probe(
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &SecretRedactor,
    input: &PreviewSkillSyncInput,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<PreviewPlan, AppError> {
    let prepared = prepare_skill_sync(database, paths, environment, input, policy_probe)?;
    let plan =
        build_prepared_skill_preview(redactor, input.exclude_from_git, prepared, Vec::new())?;
    persist_preview(database, &plan)?;
    Ok(plan)
}

pub(crate) fn build_skill_takeover_preview_in_connection(
    connection: &rusqlite::Connection,
    database_path: &str,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &SecretRedactor,
    tool: Tool,
    entries: Vec<SkillTakeoverEntry>,
) -> Result<PreviewPlan, AppError> {
    let input = PreviewSkillSyncInput {
        tool,
        project_id: None,
        exclude_from_git: false,
    };
    let prepared = prepare_skill_sync_in_connection(
        connection,
        database_path,
        paths,
        environment,
        &input,
        environment.claude_customization_policy_probe(),
    )?;
    let target = prepared
        .target
        .as_ref()
        .ok_or_else(|| AppError::conflict("skillTakeover", "接管目标没有可预览变更"))?;
    if target.baseline.full_hash.is_some()
        || target.baseline.managed_hash.is_some()
        || target
            .row_versions
            .iter()
            .any(|row| row.entity_type == DatabaseEntityType::ManagedItem)
    {
        return Err(AppError::conflict(
            "skillTakeover",
            "已有受管基线或条目时不能走首次接管",
        ));
    }
    build_prepared_skill_preview(redactor, false, prepared, entries)
}

fn build_prepared_skill_preview(
    redactor: &SecretRedactor,
    exclude_from_git: bool,
    prepared: PreparedSkillSync,
    skill_takeover_entries: Vec<SkillTakeoverEntry>,
) -> Result<PreviewPlan, AppError> {
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
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: target.desired_projection,
                row_versions: target.row_versions,
                git: target.git,
                exclude_from_git,
                skill_takeover_entries,
                project_native_action: None,
                hook_initial_adopt: false,
            }]
        })
        .unwrap_or_default();
    let plan = build_preview_plan(scope, project_id, requests, redactor)?;
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_skill_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &SecretRedactor,
    input: &ApplySkillPreviewInput,
) -> Result<ApplyResult, AppError> {
    apply_skill_preview_with_policy_probe(
        write_operations,
        database,
        paths,
        environment,
        redactor,
        input,
        environment.claude_customization_policy_probe(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn apply_skill_preview_with_policy_probe(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    _redactor: &SecretRedactor,
    input: &ApplySkillPreviewInput,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<ApplyResult, AppError> {
    let persisted = load_persisted_preview(database, &input.preview_id)?;
    let preview_input = PreviewSkillSyncInput {
        tool: input.tool,
        project_id: input.project_id.clone(),
        exclude_from_git: persisted
            .items
            .first()
            .is_some_and(|item| item.envelope.exclude_from_git),
    };
    let prepared = prepare_skill_sync(database, paths, environment, &preview_input, policy_probe)?;
    if persisted.scope != prepared.scope
        || persisted.project_id != input.project_id
        || persisted.items.iter().any(|item| {
            item.envelope.descriptor.tool != input.tool
                || item.envelope.descriptor.artifact_kind != ArtifactKind::Skill
        })
    {
        return Err(AppError::stale_preview(&input.preview_id, "skillTarget"));
    }
    let apply_inputs = prepared
        .target
        .map(|target| {
            vec![ApplyTargetInput {
                descriptor: target.descriptor,
                ownership: target.ownership,
                desired_projection: target.desired_projection,
                allowed_root: target.allowed_root,
                central_skills_root: Some(paths.central_skills().to_path_buf()),
                delete_target: false,
                managed_items: target.managed_items,
                remove_managed_item_ids: target.remove_managed_item_ids,
                skill_takeover_entries: persisted
                    .items
                    .first()
                    .map(|item| item.envelope.skill_takeover_entries.clone())
                    .unwrap_or_default(),
                project_native_action: None,
            }]
        })
        .unwrap_or_default();
    if persisted.items.len() != apply_inputs.len() {
        return Err(AppError::stale_preview(&input.preview_id, "skillTargets"));
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

pub fn list_global_skill_target_statuses(
    database: &Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
) -> Result<Vec<SkillTargetStatusDto>, AppError> {
    list_global_skill_target_statuses_with_policy_probe(
        database,
        paths,
        environment,
        environment.claude_customization_policy_probe(),
    )
}

pub fn list_global_skill_target_statuses_with_policy_probe(
    database: &Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<Vec<SkillTargetStatusDto>, AppError> {
    list_global_target_statuses::<SkillManagedArtifact, _, _>(
        database,
        ASSIGNABLE_SKILL_TOOLS,
        |tool| skill_target_descriptor(environment, tool, None, policy_probe),
        |database, tool, descriptor| {
            let desired = repository::list_assigned_skills(database, tool, None)?;
            if let Some(inspection) = desired
                .iter()
                .map(|record| inspect_record(paths, record, false))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .find(|inspection| inspection.status != SkillStatus::Ready)
            {
                return Ok(Some((
                    crate::domain::SyncStatus::ExternalOwnedChange,
                    inspection.diagnostic_code.map(str::to_owned),
                )));
            }
            let baseline = find_skill_target_baseline(
                database.connection(),
                &database.path().to_string_lossy(),
                descriptor,
                None,
            )?
            .unwrap_or(ManagedTargetBaseline {
                target_id: String::new(),
                target_row_version: 0,
                full_hash: None,
                managed_hash: None,
            });
            let existing = if baseline.target_id.is_empty() {
                Vec::new()
            } else {
                repository::list_managed_skill_items(database, &baseline.target_id)?
            };
            let ownership = build_skill_ownership(&desired, &[], &existing);
            let scan = verify_managed_item_baselines(
                scan_target(tool.adapter(), descriptor, &ownership),
                &existing,
            );
            let assessment = assess_drift(descriptor, &baseline, &scan);
            let initial_diagnostic = if baseline.full_hash.is_none()
                && baseline.managed_hash.is_none()
                && existing.is_empty()
            {
                match (assessment.status, &scan, desired.is_empty()) {
                    (
                        crate::domain::SyncStatus::ExternalOwnedChange,
                        TargetScan::Observed(_),
                        false,
                    ) => Some("SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED".to_owned()),
                    (crate::domain::SyncStatus::Missing, TargetScan::Missing, false) => {
                        Some("SKILL_TARGET_INITIAL_SYNC_PENDING".to_owned())
                    }
                    (
                        crate::domain::SyncStatus::ExternalNonOwnedChange,
                        TargetScan::Observed(observation),
                        desired_is_empty,
                    ) => match observation.document() {
                        crate::adapters::ObservedDocument::SymlinkDirectory(entries) => Some(
                            if desired_is_empty {
                                if entries.is_empty() {
                                    "SKILL_TARGET_INITIAL_EMPTY"
                                } else {
                                    "SKILL_TARGET_INITIAL_UNMANAGED"
                                }
                            } else {
                                "SKILL_TARGET_INITIAL_SYNC_PENDING"
                            }
                            .to_owned(),
                        ),
                        _ => None,
                    },
                    _ => None,
                }
            } else {
                None
            };
            Ok(Some((
                assessment.status,
                initial_diagnostic.or_else(|| assessment.diagnostic_codes.into_iter().next()),
            )))
        },
    )
    .map(|statuses| {
        statuses
            .into_iter()
            .map(|value| SkillTargetStatusDto {
                tool: value.tool,
                project_id: value.project_id,
                target_path: value.target_path,
                status: value.status,
                diagnostic_code: value.diagnostic_code,
            })
            .collect()
    })
}

struct PreparedSkillSync {
    scope: Scope,
    project: Option<SkillProjectRecord>,
    target: Option<PreparedSkillTarget>,
}

struct PreparedSkillTarget {
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

fn prepare_skill_sync(
    database: &Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &PreviewSkillSyncInput,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<PreparedSkillSync, AppError> {
    prepare_skill_sync_in_connection(
        database.connection(),
        &database.path().to_string_lossy(),
        paths,
        environment,
        input,
        policy_probe,
    )
}

fn prepare_skill_sync_in_connection(
    connection: &rusqlite::Connection,
    database_path: &str,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &PreviewSkillSyncInput,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<PreparedSkillSync, AppError> {
    let project = input
        .project_id
        .as_deref()
        .map(|id| repository::get_project_from_connection(connection, database_path, id))
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
    let descriptor =
        skill_target_descriptor(environment, input.tool, project_root.as_ref(), policy_probe)?;
    let desired_records = repository::list_assigned_skills_from_connection(
        connection,
        database_path,
        input.tool,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    let inherited_records = if scope == Scope::Project {
        repository::list_assigned_skills_from_connection(
            connection,
            database_path,
            input.tool,
            None,
        )?
    } else {
        Vec::new()
    };
    validate_ready_records(
        paths,
        desired_records.iter().chain(inherited_records.iter()),
    )?;
    let existing_baseline = find_skill_target_baseline(
        connection,
        database_path,
        &descriptor,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    if desired_records.is_empty() && inherited_records.is_empty() && existing_baseline.is_none() {
        return Ok(PreparedSkillSync {
            scope,
            project,
            target: None,
        });
    }
    if desired_records.is_empty() && existing_baseline.is_none() {
        let ownership = build_skill_ownership(&[], &inherited_records, &[]);
        let scan = scan_target(input.tool.adapter(), &descriptor, &ownership);
        if inherited_projection_is_absent(&scan) {
            return Ok(PreparedSkillSync {
                scope,
                project,
                target: None,
            });
        }
    }
    let baseline = match existing_baseline {
        Some(baseline) => baseline,
        None => ensure_skill_target(connection, database_path, &descriptor, project.as_ref())?,
    };
    let existing_items = repository::list_managed_skill_items_from_connection(
        connection,
        database_path,
        &baseline.target_id,
    )?;
    if desired_records.is_empty() && inherited_records.is_empty() && existing_items.is_empty() {
        return Ok(PreparedSkillSync {
            scope,
            project,
            target: None,
        });
    }
    let desired_projection = build_desired_projection(&desired_records);
    let ownership = build_skill_ownership(&desired_records, &inherited_records, &existing_items);
    let scan = verify_managed_item_baselines(
        scan_target(input.tool.adapter(), &descriptor, &ownership),
        &existing_items,
    );
    if desired_records.is_empty()
        && existing_items.is_empty()
        && inherited_projection_is_absent(&scan)
    {
        return Ok(PreparedSkillSync {
            scope,
            project,
            target: None,
        });
    }
    let (managed_items, remove_managed_item_ids) =
        build_managed_item_changes(&desired_records, &existing_items)?;
    let row_versions = collect_row_versions::<SkillManagedArtifact>(
        connection,
        database_path,
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
    Ok(PreparedSkillSync {
        scope,
        project,
        target: Some(PreparedSkillTarget {
            descriptor,
            ownership,
            baseline,
            scan,
            desired_projection,
            row_versions,
            git,
            allowed_root,
            managed_items,
            remove_managed_item_ids,
        }),
    })
}

pub(super) fn skill_target_descriptor(
    environment: &crate::adapters::ExplicitEnvironment,
    tool: Tool,
    project_root: Option<&ProjectRoot>,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<TargetDescriptor, AppError> {
    let context = DiscoveryContext {
        environment,
        project_root,
        claude_user_mcp_probe: environment.claude_user_mcp_probe(),
        claude_customization_policy_probe: policy_probe,
    };
    find_descriptor(
        tool,
        &context,
        ArtifactKind::Skill,
        if project_root.is_some() {
            Scope::Project
        } else {
            Scope::Global
        },
        "skillTarget",
        tool.as_str(),
    )
}

fn build_desired_projection(records: &[SkillRecord]) -> Value {
    Value::Object(
        records
            .iter()
            .map(|record| {
                (
                    record.name.clone(),
                    json!({
                        "targetType": "symlink",
                        "linkTarget": record.central_path,
                    }),
                )
            })
            .collect::<Map<_, _>>(),
    )
}

fn build_skill_ownership(
    desired: &[SkillRecord],
    inherited: &[SkillRecord],
    existing: &[ManagedSkillItemRecord],
) -> ManagedOwnership {
    ManagedOwnership::SymlinkNames(
        desired
            .iter()
            .chain(inherited.iter())
            .map(|record| record.name.clone())
            .chain(existing.iter().map(|item| item.external_key.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    )
}

fn inherited_projection_is_absent(scan: &TargetScan) -> bool {
    match scan {
        TargetScan::Missing => true,
        TargetScan::Observed(observed) => observed
            .managed_projection
            .as_object()
            .is_some_and(Map::is_empty),
        TargetScan::ManagedItemBaselineMismatch
        | TargetScan::ParseError
        | TargetScan::PermissionDenied
        | TargetScan::TargetTypeChanged(_)
        | TargetScan::Unavailable
        | TargetScan::Failed => false,
    }
}

fn verify_managed_item_baselines(
    scan: TargetScan,
    existing: &[ManagedSkillItemRecord],
) -> TargetScan {
    if existing.is_empty() {
        return scan;
    }
    let matches = match &scan {
        TargetScan::Observed(observed) => {
            observed
                .managed_projection
                .as_object()
                .is_some_and(|items| {
                    existing.iter().all(|item| {
                        items
                            .get(&item.external_key)
                            .is_some_and(|value| hash_json(value) == item.last_applied_item_hash)
                    })
                })
        }
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
    desired: &[SkillRecord],
    existing: &[ManagedSkillItemRecord],
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
                "同一目标存在重复的 Skill managed item 基线",
            ));
        }
    }
    let mut used = BTreeSet::new();
    let mut updates = Vec::new();
    for record in desired {
        let native = json!({
            "targetType": "symlink",
            "linkTarget": record.central_path,
        });
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
            resource_kind: ArtifactKind::Skill,
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

fn ensure_skill_target(
    connection: &rusqlite::Connection,
    database_path: &str,
    descriptor: &TargetDescriptor,
    project: Option<&SkillProjectRecord>,
) -> Result<ManagedTargetBaseline, AppError> {
    let target_path = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::not_found("skillTarget", descriptor.tool.as_str()))?;
    let project_id = project.map(|project| project.id.as_str());
    let existing = find_skill_target_baseline(connection, database_path, descriptor, project_id)?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO managed_targets(
                id, tool, artifact_kind, scope, project_id, target_path
             ) VALUES (?1, ?2, 'skill', ?3, ?4, ?5)",
            params![
                id,
                descriptor.tool.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
        )
        .map_err(|error| {
            AppError::database(database_path, "insert_skill_managed_target").with_source(error)
        })?;
    find_skill_target_baseline(connection, database_path, descriptor, project_id)?
        .ok_or_else(|| AppError::database(database_path, "load_inserted_skill_managed_target"))
}

fn find_skill_target_baseline(
    connection: &rusqlite::Connection,
    database_path: &str,
    descriptor: &TargetDescriptor,
    project_id: Option<&str>,
) -> Result<Option<ManagedTargetBaseline>, AppError> {
    let Some(target_path) = descriptor.path.as_deref() else {
        return Ok(None);
    };
    connection
        .query_row(
            "SELECT id, row_version, baseline_full_hash, baseline_managed_hash
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = 'skill' AND scope = ?2
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
            AppError::database(database_path, "find_skill_managed_target").with_source(error)
        })
}

fn validate_ready_records<'a>(
    paths: &AppPaths,
    records: impl Iterator<Item = &'a SkillRecord>,
) -> Result<(), AppError> {
    for record in records {
        let inspection = inspect_record(paths, record, false)?;
        if inspection.status != SkillStatus::Ready {
            return Err(AppError::conflict(
                "centralSkill",
                "已分配 Skill 的中央副本缺失或内容变化",
            ));
        }
    }
    Ok(())
}

fn inspect_record(
    paths: &AppPaths,
    record: &SkillRecord,
    include_content: bool,
) -> Result<super::library::CentralSkillInspection, AppError> {
    inspect_central_skill(
        paths,
        &record.id,
        &record.name,
        &record.central_path,
        &record.content_hash,
        record.status,
        include_content,
    )
}

/// 启动时把历史以记录 id 命名的中央目录迁移为 frontmatter.name 命名：
/// 校验通过后原子重命名，同事务更新 `skills.central_path` 与受管链接基线，
/// 并把仍指向旧目录的受管 symlink 原子改写到新位置。
/// 单条记录不满足安全前提时保持 legacy 布局（inspect 兼容两种布局），不阻塞启动。
pub fn migrate_legacy_central_skill_directories(
    database: &mut Database,
    paths: &AppPaths,
) -> Result<(), AppError> {
    for record in repository::list_skills(database)? {
        migrate_legacy_skill_directory(database, paths, &record)?;
    }
    Ok(())
}

fn migrate_legacy_skill_directory(
    database: &mut Database,
    paths: &AppPaths,
    record: &SkillRecord,
) -> Result<(), AppError> {
    let central_root = paths.central_skills();
    let expected = central_root.join(&record.name);
    let old = PathBuf::from(&record.central_path);
    if old == expected {
        return Ok(());
    }
    // 只迁移已知 legacy 布局：中央根直属、以记录 id 命名的目录；其它布局一律不碰。
    if validate_central_skill_directory(&old, central_root, &record.id, &record.name).is_err() {
        return Ok(());
    }
    let old_canonical = fs::canonicalize(&old).ok();
    match fs::symlink_metadata(&old) {
        Ok(metadata) if !metadata.is_symlink() && metadata.is_dir() => {
            // 内容核验通过才改名；漂移或状态异常的记录保持原位，由既有 Invalid 展示处理。
            let inspection = inspect_central_skill(
                paths,
                &record.id,
                &record.name,
                &record.central_path,
                &record.content_hash,
                record.status,
                false,
            );
            match inspection {
                Ok(inspection) if inspection.status == SkillStatus::Ready => {}
                _ => return Ok(()),
            }
            // 目标名被占用等冲突时保持 legacy；绝不覆盖中央根内的未知目录。
            if rename_import_exclusively(&old, &expected).is_err() {
                return Ok(());
            }
            // rename 已原子完成；目录 fsync 只是持久性优化，失败不回滚也不阻塞启动。
            let _ = sync_directory(central_root);
        }
        // 上次迁移可能已完成 rename 但未更新数据库；仅当新位置核验通过才补完记录。
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let inspection = inspect_central_skill(
                paths,
                &record.id,
                &record.name,
                &expected.to_string_lossy(),
                &record.content_hash,
                record.status,
                false,
            );
            match inspection {
                Ok(inspection) if inspection.status == SkillStatus::Ready => {}
                _ => return Ok(()),
            }
        }
        // 符号链接、特殊文件或权限异常：不动未知内容，保持 legacy 可用。
        _ => return Ok(()),
    }
    let rewritten =
        rewrite_managed_skill_links(database, record, &old, old_canonical.as_deref(), &expected);
    persist_migrated_skill_directory(database, &record.id, &expected, &rewritten)
}

/// 把仍指向旧中央目录的受管 symlink 原子改写到新位置；返回被改写的 managed item id。
/// 链接缺失或已指向其它位置时不动作，交给既有 drift 检测与重新 Apply 自愈。
fn rewrite_managed_skill_links(
    database: &Database,
    record: &SkillRecord,
    old: &Path,
    old_canonical: Option<&Path>,
    expected: &Path,
) -> Vec<String> {
    let rows = (|| {
        let mut statement = database
            .connection()
            .prepare(
                "SELECT item.id, target.target_path, item.external_key
                 FROM managed_items AS item
                 JOIN managed_targets AS target ON target.id = item.target_id
                 WHERE item.resource_kind = 'skill' AND item.resource_id = ?1",
            )
            .ok()?;
        let rows = statement
            .query_map([&record.id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok()?
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        Some(rows)
    })();
    let Some(rows) = rows else {
        return Vec::new();
    };
    let mut rewritten = Vec::new();
    for (item_id, target_path, external_key) in rows {
        let link = PathBuf::from(&target_path).join(&external_key);
        let points_at_old = match fs::symlink_metadata(&link) {
            Ok(metadata) if metadata.file_type().is_symlink() => match fs::read_link(&link) {
                Ok(current) if current == old => true,
                Ok(current) => old_canonical.is_some_and(|canonical| current == canonical),
                Err(_) => false,
            },
            _ => false,
        };
        if !points_at_old {
            continue;
        }
        let temporary = link
            .parent()
            .map(|parent| parent.join(format!(".ea-skill-migrate-{}", Uuid::new_v4())));
        let Some(temporary) = temporary else { continue };
        let rewritten_link =
            symlink(expected, &temporary).is_ok() && fs::rename(&temporary, &link).is_ok();
        if !rewritten_link {
            let _ = fs::remove_file(&temporary);
            continue;
        }
        rewritten.push(item_id);
    }
    rewritten
}

fn persist_migrated_skill_directory(
    database: &mut Database,
    skill_id: &str,
    expected: &Path,
    rewritten_item_ids: &[String],
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_skill_directory_migration").with_source(error)
        })?;
    transaction
        .execute(
            "UPDATE skills SET central_path = ?2 WHERE id = ?1",
            params![skill_id, expected.to_string_lossy()],
        )
        .map_err(|error| {
            AppError::database(&database_path, "migrate_skill_central_path").with_source(error)
        })?;
    // 与 build_managed_item_changes 的 native 投影保持同一形状，避免迁移本身制造 managed item 漂移。
    let native = json!({
        "targetType": "symlink",
        "linkTarget": expected.to_string_lossy(),
    });
    let item_hash = hash_json(&native);
    for item_id in rewritten_item_ids {
        transaction
            .execute(
                "UPDATE managed_items SET last_applied_item_hash = ?2
                 WHERE id = ?1 AND resource_kind = 'skill'",
                params![item_id, item_hash],
            )
            .map_err(|error| {
                AppError::database(&database_path, "migrate_skill_managed_item_hash")
                    .with_source(error)
            })?;
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_skill_directory_migration").with_source(error)
    })
}

/// 启动时对 Skills 受管目标做一次基线记账对账。目录迁移会改写受管链接并刷新
/// item 基线，但目标 `managed_targets.baseline_*` 无法在迁移中可靠重算，会留下
/// 「磁盘已与期望一致、仅基线记账过期」的目标——该状态被 assess_drift 判为
/// `external_owned_change` 且不可合并，Preview 会变成 Conflict，用户无法通过 UI 自愈。
/// 因此仅当【全部受管 item 基线与磁盘一致】且【磁盘观察投影等于当前分配的期望投影】时，
/// 按 `scan_target` 同一口径回填基线；其余漂移一律不动，交给显式 Preview/Apply/回滚。
/// 对账是尽力而为的：任何读取或前提不满足都静默跳过。
pub fn reconcile_skill_target_baselines(database: &Database) {
    let Ok(targets) = list_skill_managed_targets(database) else {
        return;
    };
    for (target_id, tool, project_id, target_path) in targets {
        let _ =
            reconcile_skill_target_baseline(database, &target_id, tool, project_id, &target_path);
    }
}

type SkillManagedTargetRow = (String, Tool, Option<String>, String);

fn list_skill_managed_targets(database: &Database) -> Result<Vec<SkillManagedTargetRow>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, tool, project_id, target_path
             FROM managed_targets
             WHERE artifact_kind = 'skill'
             ORDER BY id",
        )
        .map_err(|error| {
            AppError::database(&database_path, "prepare_list_skill_managed_targets")
                .with_source(error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                crate::db::column_tool(row, 1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| {
            AppError::database(&database_path, "query_list_skill_managed_targets")
                .with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&database_path, "decode_list_skill_managed_targets")
                .with_source(error)
        })?;
    Ok(rows)
}

fn reconcile_skill_target_baseline(
    database: &Database,
    target_id: &str,
    tool: Tool,
    project_id: Option<String>,
    target_path: &str,
) -> Result<(), AppError> {
    let items = repository::list_managed_skill_items(database, target_id)?;
    if items.is_empty() {
        return Ok(());
    }
    for item in &items {
        repository::get_skill(database, &item.resource_id)?;
    }
    let desired_records = repository::list_assigned_skills(database, tool, project_id.as_deref())?;
    let inherited_records = if project_id.is_some() {
        repository::list_assigned_skills(database, tool, None)?
    } else {
        Vec::new()
    };
    let ownership = build_skill_ownership(&desired_records, &inherited_records, &items);
    let ManagedOwnership::SymlinkNames(names) = &ownership else {
        return Ok(());
    };
    let Ok((entries, full_hash)) = read_directory_target(Path::new(target_path)) else {
        return Ok(());
    };
    // 与 adapters::project_document 的 SymlinkDirectory/SymlinkNames 分支保持同一形状。
    let mut observed = serde_json::Map::new();
    for name in names {
        if let Some(entry) = entries.get(name) {
            let Ok(value) = serde_json::to_value(entry) else {
                return Ok(());
            };
            observed.insert(name.clone(), value);
        }
    }
    let observed_hash = hash_json(&Value::Object(observed.clone()));
    for item in &items {
        let Some(value) = observed.get(&item.external_key) else {
            return Ok(());
        };
        if hash_json(value) != item.last_applied_item_hash {
            return Ok(());
        }
    }
    let desired = build_desired_projection(&desired_records);
    if hash_json(&desired) != observed_hash {
        return Ok(());
    }
    let desired_text = serde_json::to_string(&desired).map_err(|error| {
        AppError::invalid_input("projection", "期望投影无法序列化").with_source(error)
    })?;
    let database_path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection()
        .execute(
            "UPDATE managed_targets
             SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                 baseline_projection_json = ?4, last_status = 'in_sync'
             WHERE id = ?1
               AND (baseline_full_hash IS NOT ?2 OR baseline_managed_hash IS NOT ?3)",
            params![target_id, full_hash, observed_hash, desired_text],
        )
        .map_err(|error| {
            AppError::database(&database_path, "reconcile_skill_target_baseline").with_source(error)
        })?;
    if updated > 1 {
        return Err(AppError::database(
            &database_path,
            "reconcile_skill_target_baseline",
        ));
    }
    Ok(())
}

fn skill_dto(
    database: &Database,
    paths: &AppPaths,
    record: &SkillRecord,
) -> Result<SkillDto, AppError> {
    skill_dto_with_tools(
        paths,
        record,
        <SkillManagedArtifact as crate::sync::managed::ManagedArtifact>::global_tools(
            database, &record.id,
        )?,
    )
}

fn skill_dto_with_tools(
    paths: &AppPaths,
    record: &SkillRecord,
    global_tools: Vec<Tool>,
) -> Result<SkillDto, AppError> {
    let inspection = inspect_record(paths, record, false)?;
    let frontmatter: Value = serde_json::from_str(&record.frontmatter_json).map_err(|error| {
        AppError::invalid_input("frontmatter", "数据库中的 Skill frontmatter 无效")
            .with_source(error)
    })?;
    let description = frontmatter
        .get("description")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::invalid_input("frontmatter", "数据库中的 Skill description 无效")
        })?;
    Ok(SkillDto {
        id: record.id.clone(),
        name: record.name.clone(),
        source_path: record.source_path.clone(),
        central_path: record.central_path.clone(),
        content_hash: record.content_hash.clone(),
        description: description.to_owned(),
        status: inspection.status,
        diagnostic_code: inspection.diagnostic_code.map(str::to_owned),
        global_tools,
        row_version: crate::sync::managed_record_row_version::<
            crate::sync::managed::SkillManagedArtifact,
        >(record)?,
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

#[cfg(test)]
include!("tests.rs");
