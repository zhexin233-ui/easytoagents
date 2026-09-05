//! Hooks 领域服务、原生投影与持久化 Preview/Apply 编排。
//!
//! 各工具原生合同（2026-09-05 官方文档核验，见任务 09-05-add-hooks-management）：
//! - Claude：`settings.json` 的 `hooks` 键，`{"<Event>": [{matcher?, hooks: [...]}]}`；
//! - Codex：独立 `hooks.json`，顶层 `{"description"?, "hooks": {...同 Claude...}}`；
//! - Cursor：独立 `hooks.json`，`{"version": 1, "hooks": {"<event>": [扁平条目]}}`（camelCase，
//!   matcher 属于条目本身，command 型条目省略 type）；
//! - ZCode：config.json 的 `hooks` 键，`{"enabled"?, "events": {...}}`，
//!   配置文件 hooks 必须 `enabled: true` 才会运行。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{
    models::validate_hook_definition, ApplyHookPreviewInput, CreateHookInput, DeleteHookResultDto,
    HookDto, HookProjectDto, HookProjectOptionDto, HookProjectOptionsInput,
    HookProjectSelectionState, HookTargetStatusDto, PreviewHookSyncInput, ReadoptHookTargetInput,
    ReadoptHookTargetResultDto, SetGlobalHookAssignmentInput, SetProjectHookAssignmentInput,
    UpdateHookInput, VersionedHookInput,
};
use crate::{
    adapters::{
        canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
        cursor::CursorAdapter, zcode::ZcodeAdapter, DiscoveryContext, ManagedOwnership,
        PolicyState, TargetDescriptor, ToolAdapter, ASSIGNABLE_HOOK_TOOLS,
    },
    app::AppPaths,
    db::{
        hooks::{self as repository, HookRecord, ManagedHookItemRecord},
        mcp::{self as mcp_repository, McpProjectRecord},
        Database,
    },
    domain::{ArtifactKind, HookEvent, ProjectRoot, Scope, SyncStatus, Tool},
    error::AppError,
    git::inspect_path,
    security::SecretRedactor,
    sync::{
        apply_persisted_preview, assess_drift, build_preview_plan, hash_json,
        load_managed_target_baseline, load_persisted_preview, persist_preview, scan_target,
        ApplyResult, ApplyTargetInput, DatabaseEntityType, DatabaseRowVersion, ManagedItemApply,
        ManagedTargetBaseline, NoApplyFault, ObservedTarget, PreviewPlan, PreviewTargetRequest,
        TargetScan,
    },
};

// ---------------------------------------------------------------------------
// 中央库 CRUD 与分配
// ---------------------------------------------------------------------------

pub fn list_hooks(database: &Database) -> Result<Vec<HookDto>, AppError> {
    repository::list_hooks(database)?
        .iter()
        .map(|record| hook_dto(database, record))
        .collect()
}

pub fn get_hook(database: &Database, id: &str) -> Result<HookDto, AppError> {
    let record = repository::get_hook(database, id)?;
    hook_dto(database, &record)
}

pub fn create_hook(database: &mut Database, input: &CreateHookInput) -> Result<HookDto, AppError> {
    let value = validated_definition(
        &input.name,
        input.event,
        input.matcher.as_deref(),
        &input.command,
        input.timeout_seconds,
        input.enabled,
    )?;
    let record = repository::insert_hook(database, &value)?;
    hook_dto(database, &record)
}

pub fn update_hook(database: &mut Database, input: &UpdateHookInput) -> Result<HookDto, AppError> {
    let value = validated_definition(
        &input.name,
        input.event,
        input.matcher.as_deref(),
        &input.command,
        input.timeout_seconds,
        input.enabled,
    )?;
    let record = repository::update_hook(database, &input.id, input.row_version, &value)?;
    hook_dto(database, &record)
}

pub fn set_hook_enabled(
    database: &mut Database,
    input: &VersionedHookInput,
    enabled: bool,
) -> Result<HookDto, AppError> {
    let record = repository::set_hook_enabled(database, &input.id, input.row_version, enabled)?;
    hook_dto(database, &record)
}

pub fn delete_hook(
    database: &mut Database,
    input: &VersionedHookInput,
) -> Result<DeleteHookResultDto, AppError> {
    repository::delete_hook(database, &input.id, input.row_version)?;
    Ok(DeleteHookResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

/// 分配前校验事件在该工具的原生合同中存在；不支持的组合 fail closed。
pub fn hook_event_supported(tool: Tool, event: HookEvent) -> Result<(), AppError> {
    if event.supported_for_tool(tool) {
        Ok(())
    } else {
        Err(AppError::invalid_input(
            "event",
            "该工具的原生 hooks 合同不支持此事件",
        ))
    }
}

pub fn set_global_hook_assignment(
    database: &mut Database,
    input: &SetGlobalHookAssignmentInput,
) -> Result<HookDto, AppError> {
    let record = repository::get_hook(database, &input.hook_id)?;
    if input.assigned {
        hook_event_supported(input.tool, record.event)?;
    }
    let record = repository::set_global_assignment(
        database,
        input.tool,
        &input.hook_id,
        input.assigned,
        input.row_version,
    )?;
    hook_dto(database, &record)
}

pub fn set_project_hook_assignment(
    database: &mut Database,
    input: &SetProjectHookAssignmentInput,
) -> Result<HookDto, AppError> {
    let record = repository::get_hook(database, &input.hook_id)?;
    if input.assigned {
        hook_event_supported(input.tool, record.event)?;
    }
    let record = repository::set_project_assignment(
        database,
        &input.project_id,
        input.tool,
        &input.hook_id,
        input.assigned,
        input.hook_row_version,
        input.project_row_version,
    )?;
    hook_dto(database, &record)
}

pub fn list_hook_projects(database: &Database) -> Result<Vec<HookProjectDto>, AppError> {
    mcp_repository::list_projects(database)?
        .iter()
        .map(project_dto)
        .collect()
}

pub fn list_hook_project_options(
    database: &Database,
    input: &HookProjectOptionsInput,
) -> Result<Vec<HookProjectOptionDto>, AppError> {
    mcp_repository::get_project(database, &input.project_id)?;
    let global = repository::list_assigned_hooks(database, input.tool, None)?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    let selected = repository::list_assigned_hooks(database, input.tool, Some(&input.project_id))?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    repository::list_hooks(database)?
        .into_iter()
        .map(|record| {
            let state = if global.contains(&record.id) {
                HookProjectSelectionState::Inherited
            } else if selected.contains(&record.id) {
                HookProjectSelectionState::Selected
            } else {
                HookProjectSelectionState::Available
            };
            Ok(HookProjectOptionDto {
                hook_id: record.id,
                name: record.name,
                event: record.event,
                enabled: record.enabled,
                state,
                selectable: state != HookProjectSelectionState::Inherited,
                row_version: safe_row_version(record.row_version)?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 全局目标状态
// ---------------------------------------------------------------------------

pub fn list_global_hook_target_statuses(
    database: &Database,
    environment: &crate::adapters::ExplicitEnvironment,
) -> Result<Vec<HookTargetStatusDto>, AppError> {
    ASSIGNABLE_HOOK_TOOLS
        .into_iter()
        .map(|tool| {
            let descriptor = descriptor_for(environment, tool, None)?;
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
                } else if descriptor.policy != PolicyState::Allowed {
                    let diagnostic_code = match descriptor.policy {
                        PolicyState::Blocked => "CLAUDE_POLICY_BLOCKED",
                        PolicyState::Unknown => crate::sync::ERROR_CLAUDE_POLICY_UNKNOWN,
                        PolicyState::Allowed => unreachable!("allowed policy was handled above"),
                    };
                    (SyncStatus::PolicyBlocked, Some(diagnostic_code.to_owned()))
                } else {
                    (persisted.unwrap_or(SyncStatus::Missing), None)
                };
            Ok(HookTargetStatusDto {
                tool,
                project_id: None,
                target_path,
                status,
                diagnostic_code,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 预览 / Apply / 重新接管
// ---------------------------------------------------------------------------

pub fn preview_hook_sync(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &PreviewHookSyncInput,
) -> Result<PreviewPlan, AppError> {
    let prepared = prepare_hooks_sync(database, environment, input)?;
    let scope = prepared.scope;
    let project_id = prepared.project.as_ref().map(|project| project.id.clone());
    let requests = prepared
        .target
        .map(
            |target| {
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
                    hook_initial_adopt: target.hook_initial_adopt,
                }]
            },
        )
        .unwrap_or_default();
    let plan = build_preview_plan(scope, project_id, requests, redactor)?;
    persist_preview(database, &plan)?;
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_hook_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &ApplyHookPreviewInput,
) -> Result<ApplyResult, AppError> {
    let persisted = load_persisted_preview(database, &input.preview_id)?;
    let preview_input = PreviewHookSyncInput {
        tool: input.tool,
        project_id: input.project_id.clone(),
        exclude_from_git: persisted
            .items
            .first()
            .is_some_and(|item| item.envelope.exclude_from_git),
    };
    let prepared = prepare_hooks_sync(database, environment, &preview_input)?;
    if persisted.scope != prepared.scope
        || persisted.project_id != input.project_id
        || persisted.items.iter().any(|item| {
            item.envelope.descriptor.tool != input.tool
                || item.envelope.descriptor.artifact_kind != ArtifactKind::Hook
        })
    {
        return Err(AppError::stale_preview(&input.preview_id, "hookTarget"));
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
        return Err(AppError::stale_preview(&input.preview_id, "hookTargets"));
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
/// 「外部改写受管内容」类冲突。不改中央意图，也不写原生文件。
pub fn readopt_hook_target(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &ReadoptHookTargetInput,
) -> Result<ReadoptHookTargetResultDto, AppError> {
    let project = input
        .project_id
        .as_deref()
        .map(|id| mcp_repository::get_project(database, id))
        .transpose()?;
    let project_root = project
        .as_ref()
        .map(|project| canonical_project(&project.root_path))
        .transpose()?;
    let descriptor = descriptor_for(environment, input.tool, project_root.as_ref())?;
    let baseline = find_hook_target_baseline(
        database,
        &descriptor,
        project.as_ref().map(|project| project.id.as_str()),
    )?
    .ok_or_else(|| AppError::not_found("managedTarget", "该目标尚未纳入受管基线，无需重新接管"))?;
    // 基线刷新必须与下一次预览使用同一套 ownership 口径，否则目标级
    // managed hash 会再次判定为外部改写。
    let existing_items = repository::list_managed_hook_items(database, &baseline.target_id)?;
    let ownership = build_hook_ownership(input.tool);
    let scan = scan_target(tool_adapter(input.tool), &descriptor, &ownership);
    readopt_with_scan(
        database,
        &baseline,
        &existing_items,
        &scan,
        input.tool,
        &descriptor,
    )
}

fn readopt_with_scan(
    database: &mut Database,
    baseline: &ManagedTargetBaseline,
    existing_items: &[ManagedHookItemRecord],
    scan: &TargetScan,
    tool: Tool,
    descriptor: &TargetDescriptor,
) -> Result<ReadoptHookTargetResultDto, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_readopt"))?;
    let (updated_items, removed_items) =
        match scan {
            TargetScan::Observed(observed) => {
                transaction
                .execute(
                    "UPDATE managed_targets SET baseline_full_hash = ?2, baseline_managed_hash = ?3
                     WHERE id = ?1",
                    params![baseline.target_id, observed.full_hash, observed.managed_hash],
                )
                .map_err(|_| AppError::database(&database_path, "readopt_target_baseline"))?;
                // 数组型原生条目没有稳定名称键：按外部键携带的 (事件, matcher)
                // 重定位到原生分组，组内未被其他 item 认领的条目即该 item 的
                // 当前内容；分组消失或无可认领条目视为已被外部移除。
                let mut groups = native_group_hashes(observed, events_root(tool));
                let mut updated = 0u32;
                let mut removed = 0u32;
                for item in existing_items {
                    let parts: Vec<&str> = item.external_key.splitn(3, '|').collect();
                    let relocated = if let [event, _identity, matcher] = parts.as_slice() {
                        groups
                            .get_mut(&((*event).to_owned(), (*matcher).to_owned()))
                            .and_then(|entries| {
                                entries
                                    .iter_mut()
                                    .find(|(_, claimed)| !*claimed)
                                    .map(|(hash, claimed)| {
                                        *claimed = true;
                                        hash.clone()
                                    })
                            })
                    } else {
                        None
                    };
                    match relocated {
                        Some(hash) => {
                            transaction
                                .execute(
                                    "UPDATE managed_items SET last_applied_item_hash = ?2
                                     WHERE id = ?1 AND target_id = ?3",
                                    params![item.id, hash, baseline.target_id],
                                )
                                .map_err(|_| {
                                    AppError::database(&database_path, "readopt_item_baseline")
                                })?;
                            updated += 1;
                        }
                        None => {
                            transaction
                                .execute(
                                    "DELETE FROM managed_items WHERE id = ?1 AND target_id = ?2",
                                    params![item.id, baseline.target_id],
                                )
                                .map_err(|_| {
                                    AppError::database(&database_path, "readopt_remove_item")
                                })?;
                            removed += 1;
                        }
                    }
                }
                (updated, removed)
            }
            TargetScan::Missing => {
                transaction
                    .execute(
                        "DELETE FROM managed_items WHERE target_id = ?1",
                        params![baseline.target_id],
                    )
                    .map_err(|_| AppError::database(&database_path, "readopt_clear_items"))?;
                transaction
                    .execute(
                        "UPDATE managed_targets
                     SET baseline_full_hash = NULL, baseline_managed_hash = NULL
                     WHERE id = ?1",
                        params![baseline.target_id],
                    )
                    .map_err(|_| AppError::database(&database_path, "readopt_clear_baseline"))?;
                (0, existing_items.len().min(u32::MAX as usize) as u32)
            }
            _ => {
                return Err(AppError::conflict(
                    "readopt",
                    "目标当前无法安全读取，请先恢复文件内容或权限后再重新接管",
                ));
            }
        };
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_readopt"))?;
    Ok(ReadoptHookTargetResultDto {
        target_path: descriptor.path.clone().unwrap_or_default(),
        updated_item_count: updated_items,
        removed_item_count: removed_items,
    })
}

/// 按原生 `(事件键, matcher)` 分组收集条目内容哈希与认领标记，供重新
/// 接管逐条重定位。
fn native_group_hashes(
    observed: &ObservedTarget,
    events_path: &[&str],
) -> BTreeMap<(String, String), Vec<(String, bool)>> {
    let mut groups: BTreeMap<(String, String), Vec<(String, bool)>> = BTreeMap::new();
    let Some(events) = projection_value_at(&observed.managed_projection, events_path) else {
        return groups;
    };
    for (native_event, group_values) in events.as_object().into_iter().flatten() {
        for group in group_values.as_array().into_iter().flatten() {
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
                    .unwrap_or_default();
                groups
                    .entry((native_event.clone(), matcher.to_owned()))
                    .or_default()
                    .push((hash_json(hook), false));
            }
        }
    }
    groups
}

/// 全量收集原生投影中的条目内容哈希（跨事件分组、跨 matcher）。
pub(crate) fn native_entry_hashes(
    observed: &ObservedTarget,
    events_path: &[&str],
) -> BTreeSet<String> {
    native_entries(observed, events_path)
        .into_values()
        .map(|entry| hash_json(&entry))
        .collect()
}

/// 把原生投影拍平为 `外部键 -> 条目值` 的映射，供逐条校验、重新接管与
/// 导入解析复用。外部键形态：`<原生事件>|<matcher 或 空>|<内容哈希前 16 位>`。
/// Cursor 的 matcher 属于条目本身；其余工具属于 matcher 组。
pub(super) fn native_entries(
    observed: &ObservedTarget,
    events_path: &[&str],
) -> BTreeMap<String, Value> {
    let mut entries = BTreeMap::new();
    let Some(events) = projection_value_at(&observed.managed_projection, events_path) else {
        return entries;
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
                    .unwrap_or_default();
                let hash = hash_json(hook);
                let key = format!("{native_event}|{matcher}|{}", &hash[..hash.len().min(16)]);
                entries.insert(key, hook.clone());
            }
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// 同步准备
// ---------------------------------------------------------------------------

struct PreparedHooksSync {
    scope: Scope,
    project: Option<McpProjectRecord>,
    target: Option<PreparedHooksTarget>,
}

struct PreparedHooksTarget {
    descriptor: TargetDescriptor,
    hook_initial_adopt: bool,
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

/// Hook 专用的漂移评估：基线为空（从未 Apply）且观测到的 hooks 子树
/// 没有任何条目时，这是「初始接管」而非「外部改写受管内容」，允许合并。
/// 原生 hooks 子树已有内容的初始目标保持 Conflict（readopt_available），
/// 由用户先导入或显式重新接管，避免静默覆盖既有钩子。
pub(crate) fn assess_hooks_drift(
    descriptor: &TargetDescriptor,
    baseline: &ManagedTargetBaseline,
    scan: &TargetScan,
    tool: Tool,
) -> crate::sync::DriftAssessment {
    let assessment = assess_drift(descriptor, baseline, scan);
    if assessment.status == SyncStatus::ExternalOwnedChange
        && baseline.full_hash.is_none()
        && baseline.managed_hash.is_none()
    {
        if let TargetScan::Observed(observed) = scan {
            if native_entries(observed, events_root(tool)).is_empty() {
                return crate::sync::DriftAssessment {
                    status: SyncStatus::ExternalNonOwnedChange,
                    can_merge: true,
                    diagnostic_codes: vec![HOOK_TARGET_INITIAL_EMPTY.to_owned()],
                };
            }
        }
    }
    assessment
}

const HOOK_TARGET_INITIAL_EMPTY: &str = "HOOK_TARGET_INITIAL_EMPTY_HOOKS";

fn prepare_hooks_sync(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &PreviewHookSyncInput,
) -> Result<PreparedHooksSync, AppError> {
    let project = input
        .project_id
        .as_deref()
        .map(|id| mcp_repository::get_project(database, id))
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
    let descriptor = descriptor_for(environment, input.tool, project_root.as_ref())?;
    let desired_records = repository::list_assigned_hooks(
        database,
        input.tool,
        project.as_ref().map(|project| project.id.as_str()),
    )?
    .into_iter()
    .filter(|record| record.enabled)
    .collect::<Vec<_>>();
    let inherited_records = if scope == Scope::Project {
        repository::list_assigned_hooks(database, input.tool, None)?
            .into_iter()
            .filter(|record| record.enabled)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let existing_baseline = find_hook_target_baseline(
        database,
        &descriptor,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    if desired_records.is_empty() && inherited_records.is_empty() && existing_baseline.is_none() {
        return Ok(PreparedHooksSync {
            scope,
            project,
            target: None,
        });
    }
    let baseline = match existing_baseline {
        Some(baseline) => baseline,
        None => ensure_hook_target(database, &descriptor, project.as_ref())?,
    };
    let existing_items = repository::list_managed_hook_items(database, &baseline.target_id)?;
    if desired_records.is_empty() && inherited_records.is_empty() && existing_items.is_empty() {
        return Ok(PreparedHooksSync {
            scope,
            project,
            target: None,
        });
    }

    let desired = build_desired_projection(input.tool, &desired_records)?;
    let ownership = build_hook_ownership(input.tool);
    let (scan, baseline_mismatched_items) = verify_hook_item_baselines(
        scan_target(tool_adapter(input.tool), &descriptor, &ownership),
        input.tool,
        &existing_items,
    );
    // 重新接管只对「外部改写了受管内容」这一类冲突有意义；策略、信任、
    // 解析失败等其他阻塞状态必须走各自的恢复路径。
    let assessment = assess_hooks_drift(&descriptor, &baseline, &scan, input.tool);
    let readopt_available = assessment.status == SyncStatus::ExternalOwnedChange;
    let hook_initial_adopt = assessment.diagnostic_codes
        == vec![crate::sync::HOOK_TARGET_INITIAL_EMPTY_HOOKS.to_owned()];
    let (managed_items, remove_managed_item_ids) =
        build_managed_item_changes(input.tool, &desired_records, &existing_items)?;
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
            Tool::Codex => environment.codex_home().to_path_buf(),
            Tool::Cursor => environment.home().join(".cursor"),
            Tool::Zcode => environment.home().join(".zcode"),
        },
        |root| PathBuf::from(root.as_str()),
    );
    Ok(PreparedHooksSync {
        scope,
        project,
        target: Some(PreparedHooksTarget {
            descriptor,
            hook_initial_adopt,
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

// ---------------------------------------------------------------------------
// Descriptor / 投影 / ownership 基础设施
// ---------------------------------------------------------------------------

pub(super) fn descriptor_for(
    environment: &crate::adapters::ExplicitEnvironment,
    tool: Tool,
    project_root: Option<&ProjectRoot>,
) -> Result<TargetDescriptor, AppError> {
    let context = DiscoveryContext {
        environment,
        project_root,
        claude_user_mcp_probe: environment.claude_user_mcp_probe(),
        claude_customization_policy_probe: environment.claude_customization_policy_probe(),
    };
    tool_adapter(tool)
        .discover(&context)?
        .into_iter()
        .find(|descriptor| {
            descriptor.artifact_kind == ArtifactKind::Hook
                && descriptor.scope
                    == if project_root.is_some() {
                        Scope::Project
                    } else {
                        Scope::Global
                    }
        })
        .ok_or_else(|| AppError::not_found("hookTarget", tool.as_str()))
}

pub(super) fn tool_adapter(tool: Tool) -> &'static dyn ToolAdapter {
    static CLAUDE: ClaudeAdapter = ClaudeAdapter;
    static CODEX: CodexAdapter = CodexAdapter;
    static CURSOR: CursorAdapter = CursorAdapter;
    static ZCODE: ZcodeAdapter = ZcodeAdapter;
    match tool {
        Tool::Claude => &CLAUDE,
        Tool::Codex => &CODEX,
        Tool::Cursor => &CURSOR,
        Tool::Zcode => &ZCODE,
    }
}

/// 目标声明的受管选择器根；Cursor 额外接管结构性的 `version` 键。
pub(crate) fn native_selector_root(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude | Tool::Codex | Tool::Zcode => &["hooks"],
        Tool::Cursor => &["version", "hooks"],
    }
}

/// 事件映射在受管投影内的路径（文档根坐标）。
pub(crate) fn events_root(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude | Tool::Codex | Tool::Cursor => &["hooks"],
        Tool::Zcode => &["hooks", "events"],
    }
}

/// Hook 目标的受管 ownership：每个选择器根各一条选择器。数组条目无法按
/// 内容精确选择，未入中央库的原生条目在预览中呈现为删除，由用户显式确认。
pub(crate) fn build_hook_ownership(tool: Tool) -> ManagedOwnership {
    ManagedOwnership::selectors(native_selector_root(tool).iter().map(|root| vec![*root]))
}

/// 各工具的原生投影（文档根坐标）：
/// - claude/codex：`{"hooks": {"<Event>": [组]}}`
/// - zcode：`{"hooks": {"enabled": true, "events": {"<Event>": [组]}}}`
/// - cursor：`{"version": 1, "hooks": {"<event>": [扁平条目]}}`
///
/// 空记录时返回空对象：render 会按 selector 从文档中移除整个受管子树。
pub(super) fn build_desired_projection(
    tool: Tool,
    records: &[HookRecord],
) -> Result<Value, AppError> {
    if records.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let events = build_native_events(tool, records)?;
    let projection = match tool {
        Tool::Claude | Tool::Codex => json!({ "hooks": events }),
        Tool::Zcode => json!({ "hooks": { "enabled": true, "events": events } }),
        Tool::Cursor => json!({ "version": 1, "hooks": events }),
    };
    Ok(projection)
}

/// 统一事件 + matcher 到各工具原生事件映射；同一 `(event, matcher)` 的多条
/// hook 合并进同一 matcher 组（Cursor 为扁平数组，matcher 属于条目），
/// 事件键、组序与组内条目按确定性顺序排列，保证幂等渲染。
fn build_native_events(tool: Tool, records: &[HookRecord]) -> Result<Value, AppError> {
    if tool == Tool::Cursor {
        type NativeHookEntries = BTreeMap<String, Vec<(String, Value)>>;
        let mut events: NativeHookEntries = BTreeMap::new();
        for record in records {
            hook_event_supported(tool, record.event)?;
            let matcher = record.matcher.clone().unwrap_or_default();
            events
                .entry(record.event.native_key(tool).to_owned())
                .or_default()
                .push((matcher, native_hook_entry(tool, record)?));
        }
        let mut result = Map::new();
        for (native_event, mut entries) in events {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            result.insert(
                native_event,
                Value::Array(entries.into_iter().map(|(_, entry)| entry).collect()),
            );
        }
        return Ok(Value::Object(result));
    }
    type NativeHookGroups = BTreeMap<String, BTreeMap<String, Vec<(String, Value)>>>;
    let mut events: NativeHookGroups = BTreeMap::new();
    for record in records {
        hook_event_supported(tool, record.event)?;
        let matcher = record.matcher.clone().unwrap_or_default();
        events
            .entry(record.event.native_key(tool).to_owned())
            .or_default()
            .entry(matcher)
            .or_default()
            .push((record.name.to_lowercase(), native_hook_entry(tool, record)?));
    }
    let mut result = Map::new();
    for (native_event, groups) in events {
        let mut group_list = Vec::new();
        for (matcher, mut entries) in groups {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut group = Map::new();
            if !matcher.is_empty() {
                group.insert("matcher".to_owned(), Value::String(matcher));
            }
            group.insert(
                "hooks".to_owned(),
                Value::Array(entries.into_iter().map(|(_, entry)| entry).collect()),
            );
            group_list.push(Value::Object(group));
        }
        result.insert(native_event, Value::Array(group_list));
    }
    Ok(Value::Object(result))
}

/// 单条中央 Hook 的原生条目投影（与原生文件中的条目同形，用于条目级哈希）。
fn native_hook_entry(tool: Tool, record: &HookRecord) -> Result<Value, AppError> {
    let mut entry = Map::new();
    if tool != Tool::Cursor {
        entry.insert("type".to_owned(), Value::String("command".to_owned()));
    }
    entry.insert("command".to_owned(), Value::String(record.command.clone()));
    if let Some(timeout) = record.timeout_seconds {
        entry.insert("timeout".to_owned(), Value::Number(timeout.into()));
    }
    if tool == Tool::Cursor {
        if let Some(matcher) = record.matcher.as_deref().filter(|value| !value.is_empty()) {
            entry.insert("matcher".to_owned(), Value::String(matcher.to_owned()));
        }
    }
    Ok(Value::Object(entry))
}

/// per-hook managed item 的确定性外部键：`<Event>|<身份哈希前 16 位>|<matcher>`。
/// matcher 含正则元字符（可能包含 `|`），因此固定放在末段，解析用
/// `splitn(3, '|')`；身份哈希覆盖 name/matcher/command/timeout，确保
/// preflight 的 (target, external_key) 唯一性约束不碰撞。
fn hook_external_key(record: &HookRecord) -> String {
    let identity = hash_json(&json!({
        "name": record.name,
        "matcher": record.matcher,
        "command": record.command,
        "timeout": record.timeout_seconds,
    }));
    format!(
        "{}|{}|{}",
        record.event.as_str(),
        &identity[..identity.len().min(16)],
        record.matcher.as_deref().unwrap_or_default()
    )
}

/// 校验受管条目基线：在全部原生条目内容哈希中寻找 item 的 last_applied
/// hash；找不到即漂移。数组无名称键，因此以内容哈希匹配替代 MCP 的名称匹配。
pub(crate) fn verify_hook_item_baselines(
    scan: TargetScan,
    tool: Tool,
    existing: &[ManagedHookItemRecord],
) -> (TargetScan, Vec<String>) {
    if existing.is_empty() {
        return (scan, Vec::new());
    }
    let events_path = events_root(tool);
    let mismatched = match &scan {
        TargetScan::Observed(observed) => {
            let hashes = native_entry_hashes(observed, events_path);
            existing
                .iter()
                .filter(|item| !hashes.contains(item.last_applied_item_hash.as_str()))
                .map(|item| item.external_key.clone())
                .collect::<Vec<_>>()
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
    desired: &[HookRecord],
    existing: &[ManagedHookItemRecord],
) -> Result<(Vec<ManagedItemApply>, Vec<String>), AppError> {
    let mut by_resource = BTreeMap::new();
    let mut by_external_key = BTreeSet::new();
    for item in existing {
        if by_resource
            .insert(item.resource_id.as_str(), item)
            .is_some()
            || !by_external_key.insert(item.external_key.as_str())
        {
            return Err(AppError::conflict(
                "managedItems",
                "同一目标存在重复的 Hook managed item 基线",
            ));
        }
    }
    let mut used = BTreeSet::new();
    let mut updates = Vec::new();
    for record in desired {
        let native = native_hook_entry(tool, record)?;
        let external_key = hook_external_key(record);
        let existing_item = by_resource.get(record.id.as_str()).copied();
        let id = existing_item
            .map(|item| item.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        used.insert(id.clone());
        updates.push(ManagedItemApply {
            id,
            resource_kind: ArtifactKind::Hook,
            resource_id: record.id.clone(),
            external_key,
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
    records: impl Iterator<Item = &'a HookRecord>,
    items: &[ManagedHookItemRecord],
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
            (DatabaseEntityType::Hook, record.id.clone()),
            safe_row_version(record.row_version)?,
        );
    }
    for item in items {
        versions.insert(
            (DatabaseEntityType::ManagedItem, item.id.clone()),
            safe_row_version(item.row_version)?,
        );
        if let Ok(record) = repository::get_hook(database, &item.resource_id) {
            versions.insert(
                (DatabaseEntityType::Hook, record.id),
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

fn ensure_hook_target(
    database: &mut Database,
    descriptor: &TargetDescriptor,
    project: Option<&McpProjectRecord>,
) -> Result<ManagedTargetBaseline, AppError> {
    let target_path = descriptor
        .path
        .as_deref()
        .ok_or_else(|| AppError::not_found("hookTarget", descriptor.tool.as_str()))?;
    let database_path = database.path().to_string_lossy().into_owned();
    let project_id = project.map(|project| project.id.as_str());
    let existing = find_hook_target_baseline(database, descriptor, project_id)?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    database
        .connection_mut()
        .execute(
            "INSERT INTO managed_targets(
                id, tool, artifact_kind, scope, project_id, target_path
             ) VALUES (?1, ?2, 'hook', ?3, ?4, ?5)",
            params![
                id,
                descriptor.tool.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
        )
        .map_err(|_| AppError::database(&database_path, "insert_hook_managed_target"))?;
    load_managed_target_baseline(database, &id)
}

pub(super) fn find_hook_target_baseline(
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
             WHERE tool = ?1 AND artifact_kind = 'hook' AND scope = ?2
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
        .map_err(|_| AppError::database(&database_path, "find_hook_managed_target"))
}

// ---------------------------------------------------------------------------
// DTO 辅助
// ---------------------------------------------------------------------------

fn hook_dto(database: &Database, record: &HookRecord) -> Result<HookDto, AppError> {
    let global_tools = repository::global_tools_for_hook(database, &record.id)?;
    Ok(HookDto {
        id: record.id.clone(),
        name: record.name.clone(),
        event: record.event,
        matcher: record.matcher.clone(),
        command: record.command.clone(),
        timeout_seconds: record.timeout_seconds,
        enabled: record.enabled,
        global_tools,
        row_version: safe_row_version(record.row_version)?,
    })
}

fn project_dto(project: &McpProjectRecord) -> Result<HookProjectDto, AppError> {
    Ok(HookProjectDto {
        id: project.id.clone(),
        display_name: project.display_name.clone(),
        root_path: project.root_path.clone(),
        codex_trust_status: project.codex_trust_status,
        row_version: safe_row_version(project.row_version)?,
    })
}

pub(crate) fn validated_definition(
    name: &str,
    event: HookEvent,
    matcher: Option<&str>,
    command: &str,
    timeout_seconds: Option<i32>,
    enabled: bool,
) -> Result<repository::ValidatedHookDefinition, AppError> {
    let (name, matcher, command, timeout_seconds) =
        validate_hook_definition(name, event, matcher, command, timeout_seconds)?;
    Ok(repository::ValidatedHookDefinition {
        name,
        event,
        matcher,
        command,
        timeout_seconds,
        enabled,
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

pub(super) fn projection_value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
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
             WHERE tool = ?1 AND artifact_kind = 'hook'
               AND ifnull(project_id, '') = ifnull(?2, '') AND target_path = ?3",
            params![tool.as_str(), project_id, target_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| AppError::database(&path, "load_hook_target_status"))?;
    status.map(parse_sync_status).transpose()
}

fn parse_sync_status(value: String) -> Result<SyncStatus, AppError> {
    crate::domain::SyncStatus::from_stable_str(&value)
        .ok_or_else(|| AppError::database("managed_targets", "解析 last_status 失败"))
}

#[cfg(test)]
mod tests {
    use super::{
        build_desired_projection, build_hook_ownership, events_root, hook_event_supported,
        native_selector_root, HookRecord,
    };
    use crate::domain::{HookEvent, Tool};
    use serde_json::json;

    fn record(name: &str, event: HookEvent, matcher: Option<&str>) -> HookRecord {
        HookRecord {
            id: "00000000-0000-4000-8000-000000000001".to_owned(),
            name: name.to_owned(),
            event,
            matcher: matcher.map(str::to_owned),
            command: "bash /fixture/hook.sh".to_owned(),
            timeout_seconds: Some(30),
            enabled: true,
            row_version: 1,
        }
    }

    #[test]
    fn event_support_matrix_matches_official_contracts() {
        // Claude / Codex / ZCode / Cursor 的官方可配置事件集合（2026-09-05 核验）。
        assert!(HookEvent::PreToolUse.supported_for_tool(Tool::Claude));
        assert!(HookEvent::Notification.supported_for_tool(Tool::Claude));
        assert!(!HookEvent::PostToolUseFailure.supported_for_tool(Tool::Claude));
        assert!(HookEvent::PostCompact.supported_for_tool(Tool::Codex));
        assert!(!HookEvent::Notification.supported_for_tool(Tool::Codex));
        assert!(HookEvent::PostToolUseFailure.supported_for_tool(Tool::Zcode));
        assert!(!HookEvent::SessionEnd.supported_for_tool(Tool::Zcode));
        assert!(HookEvent::Stop.supported_for_tool(Tool::Cursor));
        assert!(!HookEvent::UserPromptSubmit.supported_for_tool(Tool::Cursor));
        for tool in [Tool::Claude, Tool::Codex, Tool::Zcode, Tool::Cursor] {
            assert!(hook_event_supported(tool, HookEvent::PreToolUse).is_ok());
        }
        assert!(hook_event_supported(Tool::Cursor, HookEvent::UserPromptSubmit).is_err());
    }

    #[test]
    fn native_keys_and_selector_roots_follow_each_tool_contract() {
        assert_eq!(HookEvent::Stop.native_key(Tool::Claude), "Stop");
        assert_eq!(HookEvent::Stop.native_key(Tool::Cursor), "stop");
        assert_eq!(
            HookEvent::PostToolUseFailure.native_key(Tool::Cursor),
            "postToolUseFailure"
        );
        assert_eq!(native_selector_root(Tool::Claude), &["hooks"][..]);
        assert_eq!(
            native_selector_root(Tool::Cursor),
            &["version", "hooks"][..]
        );
        assert_eq!(events_root(Tool::Zcode), &["hooks", "events"][..]);
    }

    #[test]
    fn projections_follow_each_tool_shape() {
        // Claude：hooks 键下为事件 → matcher 组。
        let claude = build_desired_projection(
            Tool::Claude,
            &[record("block-rm", HookEvent::PreToolUse, Some("Bash"))],
        )
        .unwrap();
        assert_eq!(
            claude,
            json!({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
                {"type": "command", "command": "bash /fixture/hook.sh", "timeout": 30}
            ]}]}})
        );

        // Codex：独立 hooks.json，投影只含 hooks 键（保留用户 description）。
        let codex = build_desired_projection(
            Tool::Codex,
            &[record("session-notes", HookEvent::SessionStart, None)],
        )
        .unwrap();
        assert_eq!(
            codex,
            json!({"hooks": {"SessionStart": [{"hooks": [
                {"type": "command", "command": "bash /fixture/hook.sh", "timeout": 30}
            ]}]}})
        );

        // ZCode：runner 级 enabled 恒为 true，事件嵌套在 events 键下。
        let zcode = build_desired_projection(
            Tool::Zcode,
            &[record("notify", HookEvent::PostToolUseFailure, None)],
        )
        .unwrap();
        assert_eq!(
            zcode,
            json!({"hooks": {"enabled": true, "events": {"PostToolUseFailure": [{"hooks": [
                {"type": "command", "command": "bash /fixture/hook.sh", "timeout": 30}
            ]}]}}})
        );

        // Cursor：version + hooks 双键、camelCase 事件、扁平条目、matcher 属于条目。
        let cursor = build_desired_projection(
            Tool::Cursor,
            &[record("format", HookEvent::PostToolUse, Some("Write|Edit"))],
        )
        .unwrap();
        assert_eq!(
            cursor,
            json!({"version": 1, "hooks": {"postToolUse": [
                {"command": "bash /fixture/hook.sh", "timeout": 30, "matcher": "Write|Edit"}
            ]}})
        );
    }

    #[test]
    fn unsupported_event_projection_fails_closed() {
        let error = build_desired_projection(
            Tool::Cursor,
            &[record("prompt-submit", HookEvent::UserPromptSubmit, None)],
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn empty_projection_removes_managed_subtree() {
        for tool in [Tool::Claude, Tool::Codex, Tool::Zcode, Tool::Cursor] {
            assert_eq!(
                build_desired_projection(tool, &[]).unwrap(),
                json!({}),
                "空投影应触发 selector 移除"
            );
        }
    }

    #[test]
    fn ownership_covers_every_selector_root() {
        for tool in [Tool::Claude, Tool::Codex, Tool::Zcode, Tool::Cursor] {
            let ownership = build_hook_ownership(tool);
            let crate::adapters::ManagedOwnership::Selectors(selectors) = &ownership else {
                panic!("hooks 必须使用选择器 ownership");
            };
            let roots = native_selector_root(tool);
            assert_eq!(selectors.len(), roots.len());
            for (selector, root) in selectors.iter().zip(roots) {
                assert_eq!(selector.len(), 1);
                assert_eq!(&selector[0], root);
            }
        }
    }
}
