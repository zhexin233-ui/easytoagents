pub fn list_hooks(database: &Database) -> Result<Vec<HookDto>, AppError> {
    // 列表只发两条 SQL（记录 + 全部全局分配），逐条组装不再回库。
    let mut assignments = repository::global_assignments_for_all_hooks(database)?;
    repository::list_hooks(database)?
        .iter()
        .map(|record| {
            hook_dto_with_assignments(record, assignments.remove(&record.id).unwrap_or_default())
        })
        .collect()
}

pub fn get_hook(database: &Database, id: &str) -> Result<HookDto, AppError> {
    let record = repository::get_hook(database, id)?;
    hook_dto(database, &record)
}

pub fn create_hook(
    database: &mut Database,
    paths: &AppPaths,
    input: &CreateHookInput,
) -> Result<HookDto, AppError> {
    let value = validated_definition(
        &input.name,
        input.event,
        input.matcher.as_deref(),
        &input.command,
        input.timeout_seconds,
        input.enabled,
    )?;
    let id = EntityId::new().to_string();
    // 脚本接管通道（导入与手动创建共用）：先把脚本本体写入中央目录，
    // 再插入 DB；DB 失败时回滚已写文件，避免留下无主脚本。
    let adopted = match input.script_source_path.as_deref() {
        Some(source) => Some(adopt_script(paths, &id, source)?),
        None => None,
    };
    let value = match &adopted {
        Some(adopted) => crate::db::hooks::ValidatedHookDefinition {
            command: rewrite_command_with_script(
                &value.command,
                &input.command,
                &adopted.original_path,
                &adopted.central_path,
            )?,
            script_name: Some(adopted.file_name.clone()),
            script_hash: Some(adopted.hash.clone()),
            ..value
        },
        None => value,
    };
    match repository::insert_hook(database, &id, &value) {
        Ok(record) => hook_dto(database, &record),
        Err(error) => {
            if adopted.is_some() {
                let _ = fs::remove_dir_all(paths.central_hooks().join(&id));
            }
            Err(error)
        }
    }
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
    paths: &AppPaths,
    input: &VersionedHookInput,
) -> Result<DeleteHookResultDto, AppError> {
    repository::delete_hook(database, &input.id, input.row_version)?;
    // 外键 RESTRICT 保证删除时不存在任何分配（即无原生引用），
    // 中央脚本目录可安全清理；清理失败仅遗留无主文件，不影响正确性。
    let _ = fs::remove_dir_all(paths.central_hooks().join(&input.id));
    Ok(DeleteHookResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

/// 分配前校验事件在该工具的原生合同中存在；不支持的组合 fail closed。
pub fn hook_event_supported(tool: Tool, event: HookEvent) -> Result<(), AppError> {
    ensure_hooks_supported(tool)?;
    if event.supported_for_tool(tool) {
        Ok(())
    } else {
        Err(AppError::invalid_input(
            "event",
            "该工具的原生 hooks 合同不支持此事件",
        ))
    }
}

fn ensure_hooks_supported(tool: Tool) -> Result<(), AppError> {
    if tool == Tool::Opencode {
        return Err(AppError::invalid_input(
            "capability",
            "OPENCODE_HOOKS_UNSUPPORTED",
        ));
    }
    Ok(())
}

pub fn set_global_hook_assignment(
    database: &mut Database,
    input: &SetGlobalHookAssignmentInput,
) -> Result<HookDto, AppError> {
    ensure_hooks_supported(input.tool)?;
    if input.assigned {
        // 生效事件随分配指定，可不同于中央建议事件；仍按工具 fail-closed。
        hook_event_supported(input.tool, input.event)?;
    }
    let record = repository::set_global_assignment(
        database,
        input.tool,
        &input.hook_id,
        input.event,
        input.assigned,
        input.row_version,
    )?;
    hook_dto(database, &record)
}

pub fn set_project_hook_assignment(
    database: &mut Database,
    input: &SetProjectHookAssignmentInput,
) -> Result<HookDto, AppError> {
    ensure_hooks_supported(input.tool)?;
    if input.assigned {
        hook_event_supported(input.tool, input.event)?;
    }
    let record = repository::set_project_assignment(
        database,
        &input.project_id,
        input.tool,
        &input.hook_id,
        input.event,
        input.assigned,
        input.hook_row_version,
        input.project_row_version,
    )?;
    hook_dto(database, &record)
}

pub fn list_hook_projects(database: &Database) -> Result<Vec<HookProjectDto>, AppError> {
    mcp_repository::list_projects(database)?
        .iter()
        .map(crate::sync::managed::project_dto::<McpProjectRecord>)
        .map(|result| {
            result.map(|value| HookProjectDto {
                id: value.id,
                display_name: value.display_name,
                root_path: value.root_path,
                codex_trust_status: value.codex_trust_status,
                row_version: value.row_version,
            })
        })
        .collect()
}

pub fn list_hook_project_options(
    database: &Database,
    input: &HookProjectOptionsInput,
) -> Result<Vec<HookProjectOptionDto>, AppError> {
    ensure_hooks_supported(input.tool)?;
    mcp_repository::get_project(database, &input.project_id)?;
    let global = repository::list_assigned_hooks(database, input.tool, None)?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    let selected = repository::list_assigned_hooks(database, input.tool, Some(&input.project_id))?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    // state=selected 的选项回填分配行上的生效事件（迁移 0016 起事件随分配）。
    let assigned_events: BTreeMap<String, HookEvent> =
        repository::project_assignment_events(database, &input.project_id, input.tool)?
            .into_iter()
            .collect();
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
                assigned_event: assigned_events.get(&record.id).cloned(),
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
    crate::sync::managed::list_global_target_statuses::<
        crate::sync::managed::HookManagedArtifact,
        _,
        _,
    >(
        database,
        ASSIGNABLE_HOOK_TOOLS,
        |tool| hook_target_descriptor(environment, tool, None),
        |database, tool, descriptor| {
            load_target_status(database, tool, None, descriptor.path.as_deref().unwrap_or_default())
                .map(|status| status.map(|status| (status, None)))
        },
    )
    .map(|statuses| {
        statuses
            .into_iter()
            .map(|value| HookTargetStatusDto {
                tool: value.tool,
                project_id: value.project_id,
                target_path: value.target_path,
                status: value.status,
                diagnostic_code: value.diagnostic_code,
            })
            .collect()
    })
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
                hook_initial_adopt: target.hook_initial_adopt,
            }]
        })
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
    let descriptor = hook_target_descriptor(environment, input.tool, project_root.as_ref())?;
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
    let scan = scan_target(input.tool.adapter(), &descriptor, &ownership);
    let outcome = crate::sync::managed::readopt_with_scan::<
        crate::sync::managed::HookManagedArtifact,
    >(
        database,
        &baseline,
        &existing_items,
        &scan,
        &descriptor,
    )?;
    Ok(ReadoptHookTargetResultDto {
        target_path: outcome.target_path,
        updated_item_count: outcome.updated_item_count,
        removed_item_count: outcome.removed_item_count,
    })
}

/// 拍平原生投影为 `(原生事件键, matcher, 条目)` 列表；Cursor 的 matcher
/// 属于条目本身，其余工具属于 matcher 组。供逐条校验、接管与导入解析复用。
pub(crate) fn native_hook_rows(
    observed: &ObservedTarget,
    tool: Tool,
) -> Vec<(String, String, Value)> {
    let mut rows = Vec::new();
    let Some(events) = projection_value_at(&observed.managed_projection, events_root(tool)) else {
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

/// 初始接管判定：基线为空时，观测到的 hooks 子树要么没有任何条目，要么
/// 每个条目都能被中央记录认领（同事件 + matcher + 超时，且命令一致或其
/// 引用脚本的内容哈希等于中央接管脚本的哈希）。认领失败的条目若直接
/// Apply 会被删除，必须保持 Conflict 由用户先导入或显式重新接管。
fn initial_adopt_allowed<'a>(
    desired: impl Iterator<Item = &'a HookRecord>,
    observed: &ObservedTarget,
    tool: Tool,
    home: &Path,
) -> bool {
    let desired: Vec<&HookRecord> = desired.collect();
    let rows = native_hook_rows(observed, tool);
    if rows.is_empty() {
        return true;
    }
    rows.iter().all(|(native_event, matcher, entry)| {
        let command = entry
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let timeout = entry
            .get("timeout")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok());
        desired.iter().any(|record| {
            record.event.native_key(tool) == native_event
                && record.matcher.as_deref().unwrap_or_default() == matcher
                && record.timeout_seconds == timeout
                && (record.command == command
                    || (record.script_hash.is_some()
                        && resolve_script_adoption(command, home)
                            .map(|(_, path)| script_content_hash(&path).ok())
                            .is_some_and(|hash| hash.as_deref() == record.script_hash.as_deref())))
        })
    })
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
pub(crate) fn native_entries(
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
    let descriptor = hook_target_descriptor(environment, input.tool, project_root.as_ref())?;
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
        scan_target(input.tool.adapter(), &descriptor, &ownership),
        input.tool,
        &existing_items,
    );
    // 重新接管只对「外部改写了受管内容」这一类冲突有意义；策略、信任、
    // 解析失败等其他阻塞状态必须走各自的恢复路径。
    let hook_initial_adopt = baseline.full_hash.is_none()
        && baseline.managed_hash.is_none()
        && match &scan {
            TargetScan::Observed(observed) => initial_adopt_allowed(
                desired_records.iter().chain(inherited_records.iter()),
                observed,
                input.tool,
                environment.home(),
            ),
            _ => false,
        };
    let assessment = assess_hooks_drift(&descriptor, &baseline, &scan, input.tool);
    let readopt_available =
        assessment.status == SyncStatus::ExternalOwnedChange && !hook_initial_adopt;
    let (managed_items, remove_managed_item_ids) =
        build_managed_item_changes(input.tool, &desired_records, &existing_items)?;
    let row_versions = crate::sync::managed::collect_row_versions::<
        crate::sync::managed::HookManagedArtifact,
    >(
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

pub(super) fn hook_target_descriptor(
    environment: &crate::adapters::ExplicitEnvironment,
    tool: Tool,
    project_root: Option<&ProjectRoot>,
) -> Result<TargetDescriptor, AppError> {
    if tool == Tool::Opencode {
        return Err(AppError::invalid_input(
            "capability",
            "OPENCODE_HOOKS_UNSUPPORTED",
        ));
    }
    let context = DiscoveryContext {
        environment,
        project_root,
        claude_user_mcp_probe: environment.claude_user_mcp_probe(),
        claude_customization_policy_probe: environment.claude_customization_policy_probe(),
    };
    find_descriptor(
        tool,
        &context,
        ArtifactKind::Hook,
        if project_root.is_some() {
            Scope::Project
        } else {
            Scope::Global
        },
        "hookTarget",
        tool.as_str(),
    )
}

/// 目标声明的受管选择器根；Cursor 额外接管结构性的 `version` 键。
pub(crate) fn native_selector_root(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude | Tool::Codex | Tool::Zcode => &["hooks"],
        Tool::Cursor => &["version", "hooks"],
        Tool::Opencode => &[],
    }
}

/// 事件映射在受管投影内的路径（文档根坐标）。
pub(crate) fn events_root(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude | Tool::Codex | Tool::Cursor => &["hooks"],
        Tool::Zcode => &["hooks", "events"],
        Tool::Opencode => &[],
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
        Tool::Opencode => {
            return Err(AppError::invalid_input(
                "capability",
                "OPENCODE_HOOKS_UNSUPPORTED",
            ))
        }
    };
    Ok(projection)
}
