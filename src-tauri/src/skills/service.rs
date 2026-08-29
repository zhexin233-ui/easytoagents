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
        inspect_central_skill, prepare_skill_import, quarantine_central_skill,
        rename_import_exclusively, restore_quarantined_skill, sync_directory,
        validate_central_skill_directory,
    },
    ApplySkillPreviewInput, DeleteSkillResultDto, ImportSkillInput, PreparedSkillRecord,
    PreviewSkillSyncInput, SetGlobalSkillAssignmentInput, SetProjectSkillAssignmentInput,
    SkillContentPreviewDto, SkillDto, SkillProjectDto, SkillProjectOptionDto,
    SkillProjectOptionsInput, SkillProjectSelectionState, SkillTargetStatusDto,
    VersionedSkillInput,
};
use crate::{
    adapters::{
        canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
        ClaudeCustomizationPolicyProbe, DiscoveryContext, ManagedOwnership, TargetDescriptor,
        ToolAdapter,
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
    sync::{
        apply_persisted_preview, assess_drift, build_preview_plan, canonical_json, hash_json,
        load_managed_target_baseline, load_persisted_preview, persist_preview,
        read_directory_target, scan_target, ApplyResult, ApplyTargetInput, DatabaseEntityType,
        DatabaseRowVersion, ManagedItemApply, ManagedTargetBaseline, NoApplyFault, PreviewPlan,
        PreviewTargetRequest, TargetScan,
    },
};

pub fn list_skills(database: &Database, paths: &AppPaths) -> Result<Vec<SkillDto>, AppError> {
    repository::list_skills(database)?
        .iter()
        .map(|record| skill_dto(database, paths, record))
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
        .map(project_dto)
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
    [Tool::Claude, Tool::Codex]
        .into_iter()
        .map(|tool| {
            let descriptor = descriptor_for(environment, tool, None, policy_probe)?;
            let target_path = descriptor.path.clone();
            let desired = repository::list_assigned_skills(database, tool, None)?;
            if let Some(inspection) = desired
                .iter()
                .map(|record| inspect_record(paths, record, false))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .find(|inspection| inspection.status != SkillStatus::Ready)
            {
                return Ok(SkillTargetStatusDto {
                    tool,
                    project_id: None,
                    target_path,
                    status: crate::domain::SyncStatus::ExternalOwnedChange,
                    diagnostic_code: inspection.diagnostic_code.map(str::to_owned),
                });
            }
            let baseline = find_skill_target_baseline(database, &descriptor, None)?.unwrap_or(
                ManagedTargetBaseline {
                    target_id: String::new(),
                    target_row_version: 0,
                    full_hash: None,
                    managed_hash: None,
                },
            );
            let existing = if baseline.target_id.is_empty() {
                Vec::new()
            } else {
                repository::list_managed_skill_items(database, &baseline.target_id)?
            };
            let ownership = build_skill_ownership(&desired, &[], &existing);
            let scan = verify_managed_item_baselines(
                scan_target(tool_adapter(tool), &descriptor, &ownership),
                &existing,
            );
            let assessment = assess_drift(&descriptor, &baseline, &scan);
            let initial_diagnostic = if baseline.full_hash.is_none()
                && baseline.managed_hash.is_none()
                && existing.is_empty()
                && assessment.can_merge
            {
                match (assessment.status, &scan, desired.is_empty()) {
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
            Ok(SkillTargetStatusDto {
                tool,
                project_id: None,
                target_path,
                status: assessment.status,
                diagnostic_code: initial_diagnostic
                    .or_else(|| assessment.diagnostic_codes.into_iter().next()),
            })
        })
        .collect()
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
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &PreviewSkillSyncInput,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<PreparedSkillSync, AppError> {
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
    let descriptor = descriptor_for(environment, input.tool, project_root.as_ref(), policy_probe)?;
    let desired_records = repository::list_assigned_skills(
        database,
        input.tool,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    let inherited_records = if scope == Scope::Project {
        repository::list_assigned_skills(database, input.tool, None)?
    } else {
        Vec::new()
    };
    validate_ready_records(
        paths,
        desired_records.iter().chain(inherited_records.iter()),
    )?;
    let existing_baseline = find_skill_target_baseline(
        database,
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
        let scan = scan_target(tool_adapter(input.tool), &descriptor, &ownership);
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
        None => ensure_skill_target(database, &descriptor, project.as_ref())?,
    };
    let existing_items = repository::list_managed_skill_items(database, &baseline.target_id)?;
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
        scan_target(tool_adapter(input.tool), &descriptor, &ownership),
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
            Tool::Claude => environment.claude_config_dir().to_path_buf(),
            Tool::Codex => environment.home().to_path_buf(),
        },
        |root| PathBuf::from(root.as_str()),
    );
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

pub(super) fn descriptor_for(
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
    tool_adapter(tool)
        .discover(&context)?
        .into_iter()
        .find(|descriptor| {
            descriptor.artifact_kind == ArtifactKind::Skill
                && descriptor.scope
                    == if project_root.is_some() {
                        Scope::Project
                    } else {
                        Scope::Global
                    }
        })
        .ok_or_else(|| AppError::not_found("skillTarget", tool.as_str()))
}

fn tool_adapter(tool: Tool) -> &'static dyn ToolAdapter {
    static CLAUDE: ClaudeAdapter = ClaudeAdapter;
    static CODEX: CodexAdapter = CodexAdapter;
    match tool {
        Tool::Claude => &CLAUDE,
        Tool::Codex => &CODEX,
    }
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

fn collect_row_versions<'a>(
    database: &Database,
    project: Option<&SkillProjectRecord>,
    records: impl Iterator<Item = &'a SkillRecord>,
    items: &[ManagedSkillItemRecord],
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
            (DatabaseEntityType::Skill, record.id.clone()),
            safe_row_version(record.row_version)?,
        );
    }
    for item in items {
        versions.insert(
            (DatabaseEntityType::ManagedItem, item.id.clone()),
            safe_row_version(item.row_version)?,
        );
        if let Ok(record) = repository::get_skill(database, &item.resource_id) {
            versions.insert(
                (DatabaseEntityType::Skill, record.id),
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

fn ensure_skill_target(
    database: &mut Database,
    descriptor: &TargetDescriptor,
    project: Option<&SkillProjectRecord>,
) -> Result<ManagedTargetBaseline, AppError> {
    let target_path = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::not_found("skillTarget", descriptor.tool.as_str()))?;
    let project_id = project.map(|project| project.id.as_str());
    let existing = find_skill_target_baseline(database, descriptor, project_id)?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let id = Uuid::new_v4().to_string();
    database
        .connection_mut()
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
        .map_err(|_| AppError::database(&database_path, "insert_skill_managed_target"))?;
    load_managed_target_baseline(database, &id)
}

fn find_skill_target_baseline(
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
        .map_err(|_| AppError::database(&database_path, "find_skill_managed_target"))
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
        .map_err(|_| AppError::database(&database_path, "begin_skill_directory_migration"))?;
    transaction
        .execute(
            "UPDATE skills SET central_path = ?2 WHERE id = ?1",
            params![skill_id, expected.to_string_lossy()],
        )
        .map_err(|_| AppError::database(&database_path, "migrate_skill_central_path"))?;
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
            .map_err(|_| AppError::database(&database_path, "migrate_skill_managed_item_hash"))?;
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_skill_directory_migration"))
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
        .map_err(|_| AppError::database(&database_path, "prepare_list_skill_managed_targets"))?;
    let rows = statement
        .query_map([], |row| {
            let tool = match row.get::<_, String>(1)?.as_str() {
                "claude" => Tool::Claude,
                "codex" => Tool::Codex,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            Ok((
                row.get::<_, String>(0)?,
                tool,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|_| AppError::database(&database_path, "query_list_skill_managed_targets"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&database_path, "decode_list_skill_managed_targets"))?;
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
    let desired = canonical_json(&build_desired_projection(&desired_records));
    if hash_json(&desired) != observed_hash {
        return Ok(());
    }
    let desired_text = serde_json::to_string(&desired)
        .map_err(|_| AppError::invalid_input("projection", "期望投影无法序列化"))?;
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
        .map_err(|_| AppError::database(&database_path, "reconcile_skill_target_baseline"))?;
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
    let inspection = inspect_record(paths, record, false)?;
    let frontmatter: Value = serde_json::from_str(&record.frontmatter_json)
        .map_err(|_| AppError::invalid_input("frontmatter", "数据库中的 Skill frontmatter 无效"))?;
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
        global_tools: repository::global_tools_for_skill(database, &record.id)?,
        row_version: safe_row_version(record.row_version)?,
    })
}

fn project_dto(project: &SkillProjectRecord) -> Result<SkillProjectDto, AppError> {
    Ok(SkillProjectDto {
        id: project.id.clone(),
        display_name: project.display_name.clone(),
        root_path: project.root_path.clone(),
        codex_trust_status: project.codex_trust_status,
        row_version: safe_row_version(project.row_version)?,
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

fn safe_row_version(value: i64) -> Result<u32, AppError> {
    u32::try_from(value)
        .map_err(|_| AppError::invalid_input("rowVersion", "数据库 row_version 超出 RPC 范围"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        path::Path,
        sync::Mutex,
    };

    use serde_json::json;
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::{
        apply_skill_preview_with_policy_probe, delete_skill, import_skill,
        list_skill_project_options, preview_skill_content, preview_skill_sync_with_policy_probe,
        set_global_skill_assignment, set_project_skill_assignment,
    };
    use crate::{
        adapters::{
            ExplicitEnvironment, ToolAvailability, VerifiedClaudeCustomizationPolicyEvidence,
        },
        app::AppPaths,
        db::Database,
        domain::{SkillStatus, SyncStatus, Tool},
        error::ErrorCode,
        security::SecretRedactor,
        skills::{
            ApplySkillPreviewInput, ImportSkillInput, PreviewSkillSyncInput,
            SetGlobalSkillAssignmentInput, SetProjectSkillAssignmentInput, SkillDto,
            SkillProjectOptionsInput, SkillProjectSelectionState, VersionedSkillInput,
        },
        sync::{hash_json, list_snapshots, preview_restore, restore_snapshot},
    };

    const CONTENT_MARKER: &str = "phase6-private-content-marker";
    const FRONTMATTER_MARKER: &str = "phase6-private-frontmatter-marker";

    struct Fixture {
        _temporary: TempDir,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
        home: std::path::PathBuf,
        project: std::path::PathBuf,
        project_id: String,
        source_index: usize,
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
            let home = fs::canonicalize(home).unwrap();
            let project = fs::canonicalize(project).unwrap();
            fs::write(
                home.join(".codex/config.toml"),
                format!(
                    "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                    project.to_string_lossy()
                ),
            )
            .unwrap();
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
            let paths = AppPaths::from_data_root(root.join("private/app-data")).unwrap();
            let database = Database::open(&paths).unwrap();
            let project_id = Uuid::new_v4().to_string();
            database
                .connection()
                .execute(
                    "INSERT INTO projects(
                        id, display_name, root_path, is_git_repo, codex_trust_status
                     ) VALUES (?1, '隔离项目', ?2, 0, 'trusted')",
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
                source_index: 0,
            }
        }

        fn source(&mut self, name: &str) -> std::path::PathBuf {
            self.source_index += 1;
            let source = self
                .home
                .parent()
                .unwrap()
                .join(format!("source-{}", self.source_index));
            fs::create_dir(&source).unwrap();
            fs::write(
                source.join("SKILL.md"),
                format!(
                    "---\nname: {name}\ndescription: 隔离测试 Skill\nmetadata:\n  token: {FRONTMATTER_MARKER}\n---\n\n# Skill\n\n{CONTENT_MARKER}\n"
                ),
            )
            .unwrap();
            fs::write(source.join("asset.txt"), "fixture asset").unwrap();
            source
        }

        fn allowed_policy(&self) -> VerifiedClaudeCustomizationPolicyEvidence {
            VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting("fixture-1.0.0", None)
                .unwrap()
        }

        fn blocked_policy(&self) -> VerifiedClaudeCustomizationPolicyEvidence {
            VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                "fixture-1.0.0",
                Some(&json!(true)),
            )
            .unwrap()
        }

        fn environment_with_policy(
            &self,
            setting: Option<&serde_json::Value>,
        ) -> ExplicitEnvironment {
            ExplicitEnvironment::new(&self.home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_claude_installation_version("fixture-1.0.0")
                .unwrap()
                .with_claude_customization_policy_evidence(
                    VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                        "fixture-1.0.0",
                        setting,
                    )
                    .unwrap(),
                )
        }

        fn environment_without_policy_evidence(&self) -> ExplicitEnvironment {
            ExplicitEnvironment::new(&self.home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_claude_installation_version("fixture-1.0.0")
                .unwrap()
        }

        fn import(&mut self, name: &str) -> crate::skills::SkillDto {
            let source = self.source(name);
            import_skill(
                &mut self.database,
                &self.paths,
                &ImportSkillInput {
                    source_path: source.to_string_lossy().into_owned(),
                },
            )
            .unwrap()
        }
    }

    #[test]
    fn public_preview_status_and_apply_reuse_environment_policy_evidence() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("release-evidence-skill");
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let statuses = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
        )
        .unwrap();
        assert_ne!(statuses[0].status, SyncStatus::PolicyBlocked);
        let preview = super::preview_skill_sync(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
        )
        .unwrap();
        assert_eq!(
            preview.targets[0].descriptor.policy,
            crate::adapters::PolicyState::Allowed
        );
        super::apply_skill_preview(
            &std::sync::Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
        )
        .unwrap();
        assert!(fixture
            .home
            .join(".claude/skills/release-evidence-skill")
            .is_symlink());
    }

    #[test]
    fn initial_skill_status_requires_empty_baseline_and_no_managed_items() {
        let mut fixture = Fixture::new();
        let target = fixture.home.join(".claude/skills");
        fs::create_dir_all(&target).unwrap();
        let status = |fixture: &Fixture| {
            super::list_global_skill_target_statuses(
                &fixture.database,
                &fixture.paths,
                &fixture.environment,
            )
            .unwrap()
            .remove(0)
        };
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_EMPTY")
        );
        fs::write(target.join(".DS_Store"), "metadata").unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_UNMANAGED")
        );
        let target_id = Uuid::new_v4().to_string();
        fixture.database.connection().execute(
            "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'claude', 'skill', 'global', ?2)",
            rusqlite::params![target_id, target.to_string_lossy()],
        ).unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_UNMANAGED")
        );
        let skill = fixture.import("unassigned-copy");
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_UNMANAGED")
        );
        fixture.database.connection().execute("UPDATE managed_targets SET baseline_full_hash = ?1, baseline_managed_hash = ?2 WHERE id = ?3", rusqlite::params!["a".repeat(64), crate::sync::hash_json(&json!({})), target_id]).unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("EXTERNAL_NON_OWNED_CHANGE")
        );
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = ON")
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_managed_hash = NULL",
                [],
            )
            .unwrap();
        assert_ne!(status(&fixture).status, SyncStatus::ExternalNonOwnedChange);
        fixture
            .database
            .connection()
            .execute("UPDATE managed_targets SET baseline_full_hash = NULL", [])
            .unwrap();
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = OFF")
            .unwrap();
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = ON")
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_full_hash = ?1 WHERE id = ?2",
                rusqlite::params!["c".repeat(64), target_id],
            )
            .unwrap();
        let incomplete_baseline = status(&fixture);
        assert_eq!(incomplete_baseline.status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            incomplete_baseline.diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_INCOMPLETE_BASELINE)
        );
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_full_hash = NULL WHERE id = ?1",
                [&target_id],
            )
            .unwrap();
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = OFF")
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_items(
                id, target_id, resource_kind, resource_id, external_key,
                last_applied_item_hash
             ) VALUES (?1, ?2, 'skill', ?3, ?4, ?5)",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    target_id,
                    assigned.id,
                    assigned.name,
                    "b".repeat(64),
                ],
            )
            .unwrap();
        let managed_item_drift = status(&fixture);
        assert_eq!(managed_item_drift.status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            managed_item_drift.diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_MANAGED_ITEM_BASELINE_MISMATCH)
        );
        fixture
            .database
            .connection()
            .execute(
                "DELETE FROM managed_items WHERE target_id = ?1",
                [&target_id],
            )
            .unwrap();
        fs::write(Path::new(&skill.central_path).join("asset.txt"), "changed").unwrap();
        assert_eq!(status(&fixture).status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
    }

    #[test]
    fn first_global_sync_is_pending_then_preserves_existing_entries_for_both_tools() {
        let mut fixture = Fixture::new();
        let claude_target = fixture.home.join(".claude/skills");
        fs::create_dir_all(claude_target.join("external-untouched")).unwrap();
        fs::write(claude_target.join(".DS_Store"), "metadata").unwrap();
        let skill = fixture.import("first-sync-skill");
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Codex,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: assigned.row_version,
            },
        )
        .unwrap();

        let policy = fixture.allowed_policy();
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        let claude = statuses
            .iter()
            .find(|status| status.tool == Tool::Claude)
            .unwrap();
        let codex = statuses
            .iter()
            .find(|status| status.tool == Tool::Codex)
            .unwrap();
        assert_eq!(claude.status, SyncStatus::ExternalNonOwnedChange);
        assert_eq!(codex.status, SyncStatus::Missing);
        assert_eq!(
            claude.diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );
        assert_eq!(
            codex.diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        let redactor = SecretRedactor::default();
        let claude_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let codex_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Codex,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let previewed_statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert!(previewed_statuses.iter().all(|status| {
            status.diagnostic_code.as_deref() == Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        }));

        for (tool, preview) in [(Tool::Claude, claude_preview), (Tool::Codex, codex_preview)] {
            apply_skill_preview_with_policy_probe(
                &Mutex::new(()),
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &redactor,
                &ApplySkillPreviewInput {
                    preview_id: preview.preview_id,
                    tool,
                    project_id: None,
                },
                &policy,
            )
            .unwrap();
        }

        assert_eq!(
            fs::read(claude_target.join(".DS_Store")).unwrap(),
            b"metadata"
        );
        assert!(claude_target.join("external-untouched").is_dir());
        for link in [
            claude_target.join("first-sync-skill"),
            fixture.home.join(".agents/skills/first-sync-skill"),
        ] {
            assert!(link.is_symlink());
            assert_eq!(
                fs::canonicalize(link).unwrap(),
                Path::new(&skill.central_path)
            );
        }
        let applied_statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert!(applied_statuses
            .iter()
            .all(|status| status.status == SyncStatus::InSync));

        fs::create_dir(claude_target.join("post-apply-external")).unwrap();
        let drifted_statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        let drifted_claude = drifted_statuses
            .iter()
            .find(|status| status.tool == Tool::Claude)
            .unwrap();
        assert_eq!(drifted_claude.status, SyncStatus::ExternalNonOwnedChange);
        assert_eq!(
            drifted_claude.diagnostic_code.as_deref(),
            Some(crate::sync::WARNING_EXTERNAL_NON_OWNED_CHANGE)
        );
    }

    #[test]
    fn global_status_distinguishes_initial_missing_unknown_and_blocked_policy() {
        let fixture = Fixture::new();
        let missing = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
        )
        .unwrap();
        assert_eq!(missing[0].tool, Tool::Claude);
        assert_eq!(missing[0].status, SyncStatus::Missing);
        assert_eq!(missing[0].diagnostic_code, None);

        let unknown_environment = fixture.environment_without_policy_evidence();
        let unknown = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &unknown_environment,
        )
        .unwrap();
        assert_eq!(unknown[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            unknown[0].diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_CLAUDE_POLICY_UNKNOWN)
        );

        let blocked_environment = fixture.environment_with_policy(Some(&json!(true)));
        let blocked = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &blocked_environment,
        )
        .unwrap();
        assert_eq!(blocked[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            blocked[0].diagnostic_code.as_deref(),
            Some("CLAUDE_POLICY_BLOCKED")
        );
    }

    #[test]
    fn import_preview_and_assignment_crud_never_write_native_targets() {
        let mut fixture = Fixture::new();
        let source = fixture.source("fixture-skill");
        let before = fs::read(source.join("SKILL.md")).unwrap();
        let skill = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), before);
        assert!(Path::new(&skill.central_path).join("SKILL.md").is_file());
        let ordinary_rpc =
            serde_json::to_string(&super::list_skills(&fixture.database, &fixture.paths).unwrap())
                .unwrap();
        assert!(!ordinary_rpc.contains(CONTENT_MARKER));
        assert!(!ordinary_rpc.contains(FRONTMATTER_MARKER));
        assert!(!fixture.home.join(".claude/skills").exists());
        assert!(!fixture.home.join(".agents/skills").exists());

        let preview = preview_skill_content(&fixture.database, &fixture.paths, &skill.id).unwrap();
        assert!(preview.skill_md.contains(CONTENT_MARKER));
        assert_eq!(preview.files, vec!["SKILL.md", "asset.txt"]);

        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        assert!(!fixture.home.join(".claude/skills").exists());
        let delete_error = delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: assigned.id,
                row_version: assigned.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(delete_error.code(), ErrorCode::Conflict);
        assert!(!serde_json::to_string(&delete_error)
            .unwrap()
            .contains(CONTENT_MARKER));
    }

    #[test]
    fn database_failures_clean_staging_and_restore_quarantined_central_directory() {
        let mut fixture = Fixture::new();
        fixture
            .database
            .connection()
            .execute_batch(
                "CREATE TRIGGER fixture_fail_skill_insert
                 BEFORE INSERT ON skills BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
            )
            .unwrap();
        let source = fixture.source("failed-skill");
        assert!(import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .is_err());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
        assert!(fs::read_dir(fixture.paths.central_skills())
            .unwrap()
            .next()
            .is_none());
        fixture
            .database
            .connection()
            .execute_batch("DROP TRIGGER fixture_fail_skill_insert;")
            .unwrap();

        let skill = fixture.import("deletable-skill");
        fixture
            .database
            .connection()
            .execute_batch(
                "CREATE TRIGGER fixture_fail_skill_delete
                 BEFORE DELETE ON skills BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
            )
            .unwrap();
        assert!(delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .is_err());
        assert!(Path::new(&skill.central_path).join("SKILL.md").is_file());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn names_are_nocase_unique_and_central_content_drift_is_reported() {
        let mut fixture = Fixture::new();
        let first = fixture.import("fixture-skill");
        let second_source = fixture.source("fixture-skill");
        let error = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: second_source.to_string_lossy().into_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(
            fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .count(),
            1
        );
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());

        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: first.id.clone(),
                assigned: true,
                row_version: first.row_version,
            },
        )
        .unwrap();

        fs::write(Path::new(&first.central_path).join("asset.txt"), "tampered").unwrap();
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(listed[0].status, crate::domain::SkillStatus::Invalid);
        assert_eq!(
            listed[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &fixture.allowed_policy(),
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            statuses[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
    }

    #[test]
    fn global_inheritance_is_read_only_and_project_assignment_cannot_duplicate_it() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        let global = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let options = list_skill_project_options(
            &fixture.database,
            &fixture.paths,
            &SkillProjectOptionsInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
            },
        )
        .unwrap();
        assert_eq!(options[0].state, SkillProjectSelectionState::Inherited);
        assert!(!options[0].selectable);
        let policy = fixture.allowed_policy();
        let inherited_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert!(inherited_preview.targets.is_empty());
        let project_targets: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_targets
                 WHERE artifact_kind = 'skill' AND scope = 'project' AND project_id = ?1",
                [&fixture.project_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(project_targets, 0, "纯继承项目不应产生无意义 target");
        let error = set_project_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetProjectSkillAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: false,
                skill_row_version: global.row_version,
                project_row_version: 1,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
    }

    #[test]
    fn preview_apply_creates_missing_directories_atomically_and_never_leaks_content() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        let skill = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(CONTENT_MARKER));
        assert_eq!(
            preview.targets[0].change_kind,
            crate::domain::ChangeKind::Add
        );
        let result = apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id.clone(),
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        assert_eq!(result.applied_targets, 1);
        assert!(result.snapshot_count >= 2);
        let link = fixture.home.join(".claude/skills/fixture-skill");
        assert!(link.is_symlink());
        assert_eq!(
            fs::metadata(fixture.home.join(".claude/skills"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "Apply 创建的 Skills 目录必须保持私有权限"
        );
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            Path::new(&skill.central_path)
        );
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::InSync);
        let journal = fs::read_to_string(
            fixture
                .paths
                .journals()
                .join(format!("{}.json", preview.preview_id)),
        )
        .unwrap();
        assert!(!journal.contains(CONTENT_MARKER));
        let persisted_preview: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&preview.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!persisted_preview.contains(CONTENT_MARKER));
    }

    #[test]
    fn ordinary_directory_unknown_links_stale_preview_and_policy_block_never_overwrite() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        fs::create_dir(fixture.home.join(".claude/skills")).unwrap();
        fs::create_dir(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let redactor = SecretRedactor::default();
        let allowed = fixture.allowed_policy();
        let conflict = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &allowed,
        )
        .unwrap();
        assert_eq!(
            conflict.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert!(fixture.home.join(".claude/skills/fixture-skill").is_dir());
        let ordinary_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(ordinary_status[0].status, SyncStatus::ExternalOwnedChange);
        assert_ne!(
            ordinary_status[0].diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        fs::remove_dir(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let outside = fixture.home.parent().unwrap().join("outside-skill");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let unknown = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &allowed,
        )
        .unwrap();
        assert_eq!(
            unknown.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert_eq!(
            fs::canonicalize(fixture.home.join(".claude/skills/fixture-skill")).unwrap(),
            outside
        );
        let unknown_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(unknown_status[0].status, SyncStatus::ExternalOwnedChange);
        assert_ne!(
            unknown_status[0].diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        fs::remove_file(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let missing = fixture.home.join("missing-skill-target");
        symlink(&missing, fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let broken = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &allowed,
        )
        .unwrap();
        assert_eq!(
            broken.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert!(
            fs::symlink_metadata(fixture.home.join(".claude/skills/fixture-skill"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let broken_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(broken_status[0].status, SyncStatus::ExternalOwnedChange);
        assert_ne!(
            broken_status[0].diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        let blocked = fixture.blocked_policy();
        let policy_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &blocked,
        )
        .unwrap();
        assert_eq!(
            policy_preview.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            policy_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        let policy_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &blocked,
        )
        .unwrap();
        assert_eq!(policy_status[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            policy_status[0].diagnostic_code.as_deref(),
            Some("CLAUDE_POLICY_BLOCKED")
        );

        fs::remove_file(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        fs::remove_dir(fixture.home.join(".claude/skills")).unwrap();
        fs::write(fixture.home.join(".claude/skills"), "wrong target type").unwrap();
        let changed_type = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(changed_type[0].status, SyncStatus::TargetTypeChanged);
        assert_eq!(
            changed_type[0].diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_TARGET_TYPE_CHANGED)
        );

        fs::remove_file(fixture.home.join(".claude/skills")).unwrap();
        fs::create_dir(fixture.home.join(".claude/skills")).unwrap();
        fs::set_permissions(
            fixture.home.join(".claude/skills"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        let permission_denied = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        fs::set_permissions(
            fixture.home.join(".claude/skills"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert_eq!(permission_denied[0].status, SyncStatus::PermissionDenied);
        assert_eq!(
            permission_denied[0].diagnostic_code.as_deref(),
            Some("TARGET_PERMISSION_DENIED")
        );
    }

    #[test]
    fn project_links_use_official_paths_and_codex_user_skills_ignore_codex_home() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("project-skill");
        let assigned = set_project_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetProjectSkillAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                skill_row_version: skill.row_version,
                project_row_version: 1,
            },
        )
        .unwrap();
        let redactor = SecretRedactor::default();
        let policy = fixture.allowed_policy();
        let project_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert_eq!(
            project_preview.targets[0].descriptor.path.as_deref(),
            Some(fixture.project.join(".claude/skills").to_str().unwrap())
        );
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: project_preview.preview_id,
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
            },
            &policy,
        )
        .unwrap();
        assert!(fixture
            .project
            .join(".claude/skills/project-skill")
            .is_symlink());

        let unassigned = set_project_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetProjectSkillAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                skill_id: assigned.id,
                assigned: false,
                skill_row_version: assigned.row_version,
                project_row_version: 2,
            },
        )
        .unwrap();
        assert!(unassigned.row_version > assigned.row_version);

        let codex_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Codex,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert!(codex_preview.targets.is_empty());
        let descriptor =
            super::descriptor_for(&fixture.environment, Tool::Codex, None, &policy).unwrap();
        assert_eq!(
            descriptor.path.as_deref(),
            Some(fixture.home.join(".agents/skills").to_str().unwrap())
        );
    }

    #[test]
    fn external_change_after_preview_is_stale_and_unknown_entries_are_preserved() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("stale-skill");
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let target = fixture.home.join(".claude/skills");
        fs::create_dir(&target).unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        fs::create_dir(target.join("external-untouched")).unwrap();
        let error = apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert!(target.join("external-untouched").is_dir());
        assert!(!target.join("stale-skill").exists());
    }

    #[test]
    fn managed_link_snapshot_restore_is_safe_and_detects_drift() {
        let mut fixture = Fixture::new();
        let source = fixture.source("restorable-skill");
        let source_skill_md = fs::read(source.join("SKILL.md")).unwrap();
        let skill = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let _assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        let link = fixture.home.join(".claude/skills/restorable-skill");
        let link_snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.target_path == link.to_string_lossy())
            .unwrap();
        let allowed_root = fixture.home.join(".claude");
        let restore_preview = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &link_snapshot.snapshot_id,
            &allowed_root,
        )
        .unwrap();
        restore_snapshot(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &restore_preview.preview_id,
            &allowed_root,
            Some(fixture.paths.central_skills()),
        )
        .unwrap();
        assert!(!link.exists());

        // 快照恢复不会擅自篡改 assignment/managed baseline；缺失的受管链接必须按漂移阻断。
        let drift = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert_eq!(
            drift.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), source_skill_md);
        assert!(Path::new(&skill.central_path).is_dir());
    }

    #[test]
    fn managed_link_removal_and_central_delete_are_safe() {
        let mut fixture = Fixture::new();
        let source = fixture.source("removable-skill");
        let source_skill_md = fs::read(source.join("SKILL.md")).unwrap();
        let skill = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        let link = fixture.home.join(".claude/skills/removable-skill");
        assert!(link.is_symlink());
        let unassigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: assigned.id,
                assigned: false,
                row_version: assigned.row_version,
            },
        )
        .unwrap();
        let blocked_delete = delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: unassigned.id.clone(),
                row_version: unassigned.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(blocked_delete.code(), ErrorCode::Conflict);
        assert!(
            Path::new(&skill.central_path).is_dir(),
            "已应用 managed item 未清理前不得删除中央副本"
        );
        fs::create_dir(fixture.home.join(".claude/skills/external-directory")).unwrap();
        let removal = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: removal.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        assert!(!link.exists());
        assert!(fixture
            .home
            .join(".claude/skills/external-directory")
            .is_dir());
        delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: unassigned.id,
                row_version: unassigned.row_version,
            },
        )
        .unwrap();
        assert!(!Path::new(&skill.central_path).exists());
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), source_skill_md);
    }

    fn downgrade_to_legacy_layout(fixture: &Fixture, skill: &SkillDto) -> std::path::PathBuf {
        let legacy = fixture.paths.central_skills().join(&skill.id);
        fs::rename(Path::new(&skill.central_path), &legacy).unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE skills SET central_path = ?2 WHERE id = ?1",
                rusqlite::params![skill.id, legacy.to_string_lossy()],
            )
            .unwrap();
        legacy
    }

    fn legacy_managed_link(
        fixture: &Fixture,
        skill: &SkillDto,
        legacy: &Path,
    ) -> (String, std::path::PathBuf) {
        let target = fixture.home.join(".claude/skills");
        fs::create_dir_all(&target).unwrap();
        let target_id = Uuid::new_v4().to_string();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                 VALUES (?1, 'claude', 'skill', 'global', ?2)",
                rusqlite::params![target_id, target.to_string_lossy()],
            )
            .unwrap();
        let native = json!({
            "targetType": "symlink",
            "linkTarget": legacy.to_string_lossy(),
        });
        let item_id = Uuid::new_v4().to_string();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_items(
                    id, target_id, resource_kind, resource_id, external_key,
                    last_applied_item_hash
                 ) VALUES (?1, ?2, 'skill', ?3, ?4, ?5)",
                rusqlite::params![item_id, target_id, skill.id, skill.name, hash_json(&native)],
            )
            .unwrap();
        let link = target.join(&skill.name);
        symlink(legacy, &link).unwrap();
        (item_id, link)
    }

    #[test]
    fn legacy_central_directories_migrate_with_database_and_managed_links() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("migration-skill");
        // 分配先于迁移：与真实用户时序一致，迁移时目标已有期望投影可供对账。
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let legacy = downgrade_to_legacy_layout(&fixture, &skill);
        let (item_id, link) = legacy_managed_link(&fixture, &skill, &legacy);
        // 模拟真实状态：目标 baseline 来自上一次成功 Apply，仍记录旧布局的 hash。
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                     baseline_projection_json = ?4, last_status = 'in_sync' WHERE id = ?1",
                rusqlite::params![
                    fixture
                        .database
                        .connection()
                        .query_row::<String, _, _>(
                            "SELECT id FROM managed_targets LIMIT 1",
                            [],
                            |row| row.get(0),
                        )
                        .unwrap(),
                    "a".repeat(64),
                    "b".repeat(64),
                    r#"{"stale":"projection"}"#,
                ],
            )
            .unwrap();

        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();
        super::reconcile_skill_target_baselines(&fixture.database);

        let expected = fixture.paths.central_skills().join(&skill.name);
        assert!(!legacy.exists());
        assert!(expected.join("SKILL.md").is_file());
        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&skill.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, expected.to_string_lossy());
        assert_eq!(fs::read_link(&link).unwrap(), expected);
        assert_eq!(fs::canonicalize(&link).unwrap(), expected);
        let item_hash: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT last_applied_item_hash FROM managed_items WHERE id = ?1",
                [&item_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            item_hash,
            hash_json(&json!({
                "targetType": "symlink",
                "linkTarget": expected.to_string_lossy(),
            }))
        );
        assert!(
            super::list_skills(&fixture.database, &fixture.paths).unwrap()[0]
                .central_path
                .ends_with(&skill.name)
        );

        // 迁移刷新 item 基线后，对账回填目标 baseline：
        // 不再留下不可合并（Preview 变 Conflict）的受管内容冲突。
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &fixture.allowed_policy(),
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::InSync);
        assert_eq!(statuses[0].diagnostic_code, None);
        let projection_json: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_projection_json FROM managed_targets LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(projection_json.contains("linkTarget"));
        assert!(!projection_json.contains("stale"));

        // 二次启动幂等。
        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();
        super::reconcile_skill_target_baselines(&fixture.database);
        assert!(expected.join("SKILL.md").is_file());
        assert_eq!(fs::read_link(&link).unwrap(), expected);
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &fixture.allowed_policy(),
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::InSync);
    }

    #[test]
    fn drifted_or_occupied_legacy_directories_keep_the_legacy_layout() {
        let mut fixture = Fixture::new();
        let drifted = fixture.import("drifted-skill");
        let drifted_legacy = downgrade_to_legacy_layout(&fixture, &drifted);
        fs::write(
            drifted_legacy.join("SKILL.md"),
            "---\nname: drifted-skill\ndescription: tampered\n---\n\nbody\n",
        )
        .unwrap();

        let blocked = fixture.import("blocked-skill");
        // 先降级为 legacy 布局，再让同名名称化目录被未知目录占用。
        let blocked_legacy = downgrade_to_legacy_layout(&fixture, &blocked);
        let blocked_expected = fixture.paths.central_skills().join("blocked-skill");
        fs::create_dir(&blocked_expected).unwrap();
        let sentinel = blocked_expected.join("sentinel.txt");
        fs::write(&sentinel, "occupy").unwrap();

        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();

        assert!(drifted_legacy.join("SKILL.md").is_file());
        let drifted_stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&drifted.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(drifted_stored, drifted_legacy.to_string_lossy());
        // 漂移记录仍可被中央核验识别为 Invalid，而不是报路径身份错误。
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        let drifted_dto = listed.iter().find(|entry| entry.id == drifted.id).unwrap();
        assert_eq!(drifted_dto.status, SkillStatus::Invalid);

        assert!(blocked_legacy.join("SKILL.md").is_file());
        assert!(sentinel.is_file());
        let blocked_stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&blocked.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(blocked_stored, blocked_legacy.to_string_lossy());
    }

    #[test]
    fn interrupted_migration_completes_the_pending_database_update() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("recovered-skill");
        let legacy = downgrade_to_legacy_layout(&fixture, &skill);
        // 模拟迁移在 rename 之后、数据库更新之前崩溃。
        let expected = fixture.paths.central_skills().join("recovered-skill");
        fs::rename(&legacy, &expected).unwrap();

        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();

        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&skill.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, expected.to_string_lossy());
        assert!(expected.join("SKILL.md").is_file());
    }
}
