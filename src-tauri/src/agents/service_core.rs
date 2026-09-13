// ---------------------------------------------------------------------------
// 中央库 CRUD 与分配
// ---------------------------------------------------------------------------

pub fn list_agents(database: &Database) -> Result<Vec<AgentDto>, AppError> {
    // 列表只发三条 SQL（记录、全部全局分配、全部工具覆盖），逐条组装不再回库。
    let mut assignments = repository::global_assignments_for_all_agents(database)?;
    let mut tool_settings = repository::tool_settings_for_all_agents(database)?;
    repository::list_agents(database)?
        .iter()
        .map(|record| {
            agent_dto_with_assignments(
                record,
                assignments.remove(&record.id).unwrap_or_default(),
                tool_settings.remove(&record.id).unwrap_or_default(),
            )
        })
        .collect()
}

pub fn get_agent(database: &Database, id: &str) -> Result<AgentDto, AppError> {
    let record = repository::get_agent(database, id)?;
    agent_dto(database, &record)
}

pub fn create_agent(
    database: &mut Database,
    input: &CreateAgentInput,
) -> Result<AgentDto, AppError> {
    let value = validate_agent_definition(&input.name, &input.description, &input.prompt, input.enabled)?;
    let id = Uuid::new_v4().to_string();
    let record = repository::insert_agent(database, &id, &value)?;
    agent_dto(database, &record)
}

pub fn update_agent(database: &mut Database, input: &UpdateAgentInput) -> Result<AgentDto, AppError> {
    let value = validate_agent_definition(&input.name, &input.description, &input.prompt, input.enabled)?;
    let record = repository::update_agent(database, &input.id, input.row_version, &value)?;
    agent_dto(database, &record)
}

pub fn set_agent_enabled(
    database: &mut Database,
    input: &VersionedAgentInput,
    enabled: bool,
) -> Result<AgentDto, AppError> {
    let record = repository::set_agent_enabled(database, &input.id, input.row_version, enabled)?;
    agent_dto(database, &record)
}

pub fn delete_agent(
    database: &mut Database,
    input: &VersionedAgentInput,
) -> Result<DeleteAgentResultDto, AppError> {
    repository::delete_agent(database, &input.id, input.row_version)?;
    Ok(DeleteAgentResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

/// 写入单个工具的特有设置覆盖层。覆盖层与 Agent 共用 row_version，因而
/// 任何覆盖变更都会使已持久化的 Preview 失效；空对象 / null 清除该行。
pub fn set_agent_tool_settings(
    database: &mut Database,
    input: &SetAgentToolSettingsInput,
) -> Result<AgentDto, AppError> {
    let normalized = match input.settings.as_ref() {
        Some(value) => validate_agent_tool_settings(input.tool, value)?,
        None => validate_agent_tool_settings(input.tool, &serde_json::json!({}))?,
    };
    let record = match normalized {
        Some(settings) => {
            let json = serde_json::to_string(settings.value()).map_err(|_| {
                AppError::invalid_input("settings", "AGENT_FIELD_INVALID")
            })?;
            repository::upsert_tool_settings(
                database,
                &input.agent_id,
                input.tool,
                &json,
                input.row_version,
            )?
        }
        None => repository::delete_tool_settings(
            database,
            &input.agent_id,
            input.tool,
            input.row_version,
        )?,
    };
    agent_dto(database, &record)
}

/// 分配 / 预览前的作用域门禁：ZCode 项目级官方明示不支持，服务层在任何
/// 目标派生与文件读取之前拒绝（诊断码随错误 reason 透出）。
pub fn agent_scope_supported(tool: Tool, scope: Scope) -> Result<(), AppError> {
    if tool == Tool::Zcode && scope == Scope::Project {
        return Err(AppError::invalid_input(
            "capability",
            ZCODE_PROJECT_AGENTS_UNSUPPORTED,
        ));
    }
    Ok(())
}

pub fn set_global_agent_assignment(
    database: &mut Database,
    input: &SetGlobalAgentAssignmentInput,
) -> Result<AgentDto, AppError> {
    if !ASSIGNABLE_AGENT_TOOLS.contains(&input.tool) {
        return Err(AppError::invalid_input("tool", "该工具不支持 Agents 管理"));
    }
    let record = repository::set_global_assignment(
        database,
        input.tool,
        &input.agent_id,
        input.assigned,
        input.row_version,
    )?;
    agent_dto(database, &record)
}

pub fn set_project_agent_assignment(
    database: &mut Database,
    input: &SetProjectAgentAssignmentInput,
) -> Result<AgentDto, AppError> {
    agent_scope_supported(input.tool, Scope::Project)?;
    if !PROJECT_AGENT_TOOLS.contains(&input.tool) {
        return Err(AppError::invalid_input("tool", "该工具不支持项目级 Agents"));
    }
    let record = repository::set_project_assignment(
        database,
        &input.project_id,
        input.tool,
        &input.agent_id,
        input.assigned,
        input.agent_row_version,
        input.project_row_version,
    )?;
    agent_dto(database, &record)
}

pub fn list_agent_projects(database: &Database) -> Result<Vec<AgentProjectDto>, AppError> {
    mcp_repository::list_projects(database)?
        .iter()
        .map(crate::sync::managed::project_dto::<McpProjectRecord>)
        .map(|result| {
            result.map(|value| AgentProjectDto {
                id: value.id,
                display_name: value.display_name,
                root_path: value.root_path,
                codex_trust_status: value.codex_trust_status,
                row_version: value.row_version,
            })
        })
        .collect()
}

pub fn list_agent_project_options(
    database: &Database,
    input: &AgentProjectOptionsInput,
) -> Result<Vec<AgentProjectOptionDto>, AppError> {
    agent_scope_supported(input.tool, Scope::Project)?;
    mcp_repository::get_project(database, &input.project_id)?;
    let global = repository::list_assigned_agents(database, input.tool, None)?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    let selected = repository::list_assigned_agents(database, input.tool, Some(&input.project_id))?
        .into_iter()
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    repository::list_agents(database)?
        .into_iter()
        .map(|record| {
            let state = if global.contains(&record.id) {
                crate::domain::ManagedProjectSelectionState::Inherited
            } else if selected.contains(&record.id) {
                crate::domain::ManagedProjectSelectionState::Selected
            } else {
                crate::domain::ManagedProjectSelectionState::Available
            };
            Ok(AgentProjectOptionDto {
                agent_id: record.id,
                name: record.name,
                enabled: record.enabled,
                state,
                selectable: state != crate::domain::ManagedProjectSelectionState::Inherited,
                row_version: safe_row_version(record.row_version)?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 全局目标状态聚合
// ---------------------------------------------------------------------------

/// 状态严重度排序（design §4.3）：failed > parse_error > permission_denied >
/// policy_blocked > untrusted > target_type_changed > external_owned_change >
/// external_non_owned_change > missing > in_sync。数值越小越严重。
fn status_severity(status: SyncStatus) -> u8 {
    match status {
        SyncStatus::Failed => 0,
        SyncStatus::ParseError => 1,
        SyncStatus::PermissionDenied => 2,
        SyncStatus::PolicyBlocked => 3,
        SyncStatus::Untrusted => 4,
        SyncStatus::TargetTypeChanged => 5,
        SyncStatus::ExternalOwnedChange => 6,
        SyncStatus::ExternalNonOwnedChange => 7,
        SyncStatus::Missing => 8,
        SyncStatus::InSync => 9,
    }
}

/// 全局状态卡按工具聚合：所有受管文件 in_sync 才显示 in_sync；任一文件
/// 漂移 / 缺失 / 失败则显示对应最严重状态。无受管文件时为 missing（前端
/// 显示"未同步"）。文件级状态对每个受管文件现场扫描（与项目重扫同口径）。
pub fn list_global_agent_target_statuses(
    database: &Database,
    environment: &crate::adapters::ExplicitEnvironment,
) -> Result<Vec<AgentToolTargetStatusDto>, AppError> {
    ASSIGNABLE_AGENT_TOOLS
        .iter()
        .map(|tool| {
            let context = discovery_context(environment, None);
            let descriptor = find_descriptor(
                *tool,
                &context,
                ArtifactKind::Agent,
                Scope::Global,
                "agentTarget",
                tool.as_str(),
            )?;
            let directory_path = descriptor.path.clone();
            let (aggregate_status, diagnostic_code, files) =
                if descriptor.capability.state != crate::adapters::CapabilityState::Supported {
                    (
                        SyncStatus::Failed,
                        descriptor.capability.diagnostic_code.clone(),
                        Vec::new(),
                    )
                } else if descriptor.policy != crate::adapters::PolicyState::Allowed {
                    // 与 hooks/mcp 的全局状态口径一致：策略未知或封锁时呈现
                    // policy_blocked；claude 之外的工具策略恒为 Allowed。
                    (
                        SyncStatus::PolicyBlocked,
                        Some(str::to_owned(
                            if descriptor.policy == crate::adapters::PolicyState::Unknown {
                                "CLAUDE_POLICY_UNKNOWN"
                            } else {
                                "CLAUDE_POLICY_BLOCKED"
                            },
                        )),
                        Vec::new(),
                    )
                } else {
                    let rows = repository::list_agent_managed_targets(
                        database,
                        *tool,
                        Scope::Global,
                        None,
                    )?;
                    if rows.is_empty() {
                        (SyncStatus::Missing, None, Vec::new())
                    } else {
                        let (status, files) = aggregate_file_statuses(*tool, &descriptor, &rows)?;
                        (status, None, files)
                    }
                };
            Ok(AgentToolTargetStatusDto {
                tool: *tool,
                directory_path,
                aggregate_status,
                diagnostic_code,
                files,
            })
        })
        .collect()
}

/// 聚合单个工具下所有受管 agent 文件的现场状态。
fn aggregate_file_statuses(
    tool: Tool,
    directory_descriptor: &TargetDescriptor,
    rows: &[AgentManagedTargetRecord],
) -> Result<(SyncStatus, Vec<super::AgentFileTargetStatusDto>), AppError> {
    let mut files = Vec::with_capacity(rows.len());
    let mut worst: Option<(u8, SyncStatus)> = None;
    for row in rows {
        let (status, diagnostic_code) = agent_file_status(tool, directory_descriptor, row)?;
        let severity = status_severity(status);
        if worst.is_none() || severity < worst.map(|(current, _)| current).unwrap_or(u8::MAX) {
            worst = Some((severity, status));
        }
        files.push(super::AgentFileTargetStatusDto {
            target_path: row.target_path.clone(),
            status,
            diagnostic_code,
        });
    }
    files.sort_by(|a, b| a.target_path.cmp(&b.target_path));
    let aggregate_status = worst
        .map(|(_, status)| status)
        .unwrap_or(SyncStatus::Missing);
    Ok((aggregate_status, files))
}

/// 对单个受管 agent 文件做现场扫描 + 漂移评估，得到文件级状态。
fn agent_file_status(
    tool: Tool,
    directory_descriptor: &TargetDescriptor,
    row: &AgentManagedTargetRecord,
) -> Result<(SyncStatus, Option<String>), AppError> {
    let file_descriptor = match agent_file_descriptor(directory_descriptor, &row.target_path, tool) {
        Ok(descriptor) => descriptor,
        Err(_) => {
            return Ok((SyncStatus::Failed, Some("TARGET_READ_FAILED".to_owned())));
        }
    };
    let baseline = ManagedTargetBaseline {
        target_id: row.id.clone(),
        target_row_version: row.row_version,
        full_hash: row.baseline_full_hash.clone(),
        managed_hash: row.baseline_managed_hash.clone(),
    };
    let scan = scan_target(
        tool.adapter(),
        &file_descriptor,
        &ManagedOwnership::WholeDocument,
    );
    let assessment = assess_drift(&file_descriptor, &baseline, &scan);
    Ok((
        assessment.status,
        assessment.diagnostic_codes.into_iter().next(),
    ))
}

// ---------------------------------------------------------------------------
// 预览 / Apply / 重新接管
// ---------------------------------------------------------------------------

pub fn preview_agent_sync(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: &PreviewAgentSyncInput,
) -> Result<PreviewPlan, AppError> {
    let prepared = prepare_agents_sync(database, environment, input)?;
    let scope = prepared.scope;
    let project_id = prepared.project.as_ref().map(|project| project.id.clone());
    let requests = prepared
        .targets
        .into_iter()
        .map(|target| PreviewTargetRequest {
            descriptor: target.descriptor,
            ownership: target.ownership,
            baseline: target.baseline,
            scan: target.scan,
            baseline_mismatched_items: Vec::new(),
            readopt_available: target.readopt_available,
            desired_projection: target.desired_projection,
            row_versions: target.row_versions,
            git: target.git,
            exclude_from_git: input.exclude_from_git,
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
            hook_initial_adopt: false,
        })
        .collect();
    let plan = build_preview_plan(scope, project_id, requests, redactor)?;
    // 空 desired 且无既有受管目标时，不创建无意义的空运行。这样「空集不建
    // 目标」语义也延伸到持久化预览，Apply 只能消费真实存在的变更预览。
    if !plan.targets.is_empty() {
        persist_preview(database, &plan)?;
    }
    Ok(plan)
}

pub fn apply_agent_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &ApplyAgentPreviewInput,
) -> Result<ApplyResult, AppError> {
    let persisted = load_persisted_preview(database, &input.preview_id)?;
    let preview_input = PreviewAgentSyncInput {
        tool: input.tool,
        project_id: input.project_id.clone(),
        exclude_from_git: persisted
            .items
            .first()
            .is_some_and(|item| item.envelope.exclude_from_git),
    };
    let prepared = prepare_agents_sync(database, environment, &preview_input)?;
    if persisted.scope != prepared.scope
        || persisted.project_id != input.project_id
        || persisted.items.iter().any(|item| {
            item.envelope.descriptor.tool != input.tool
                || item.envelope.descriptor.artifact_kind != ArtifactKind::Agent
        })
    {
        return Err(AppError::stale_preview(&input.preview_id, "agentTarget"));
    }
    let apply_inputs = prepared
        .targets
        .iter()
        .map(|target| ApplyTargetInput {
            descriptor: target.descriptor.clone(),
            ownership: target.ownership.clone(),
            desired_projection: target.desired_projection.clone(),
            allowed_root: target.allowed_root.clone(),
            central_skills_root: None,
            delete_target: target.delete_target,
            managed_items: Vec::new(),
            remove_managed_item_ids: Vec::new(),
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
        })
        .collect::<Vec<_>>();
    if persisted.items.len() != apply_inputs.len() {
        return Err(AppError::stale_preview(&input.preview_id, "agentTargets"));
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

/// 以当前磁盘内容重新接管一个受管 agent 文件：仅刷新该目标级基线，解除
/// 「外部改写受管内容」类冲突。整文件目标没有 managed items，不改中央
/// 意图，也不写原生文件。
pub fn readopt_agent_target(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &ReadoptAgentTargetInput,
) -> Result<ReadoptAgentTargetResultDto, AppError> {
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
    let descriptor = agent_directory_descriptor(environment, input.tool, project_root.as_ref())?;
    let rows = repository::list_agent_managed_targets(
        database,
        input.tool,
        scope,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    let row = rows
        .iter()
        .find(|row| row.target_path == input.target_path)
        .ok_or_else(|| {
            AppError::not_found("managedTarget", "该目标尚未纳入受管基线，无需重新接管")
        })?;
    let file_descriptor = agent_file_descriptor(&descriptor, &input.target_path, input.tool)?;
    let baseline = ManagedTargetBaseline {
        target_id: row.id.clone(),
        target_row_version: row.row_version,
        full_hash: row.baseline_full_hash.clone(),
        managed_hash: row.baseline_managed_hash.clone(),
    };
    let scan = scan_target(
        input.tool.adapter(),
        &file_descriptor,
        &ManagedOwnership::WholeDocument,
    );
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_readopt_agent_target").with_source(error)
        })?;
    match &scan {
        TargetScan::Observed(observed) => crate::db::sync::update_readopt_target_baseline(
            &transaction,
            &baseline.target_id,
            &observed.full_hash,
            &observed.managed_hash,
            &database_path,
        )?,
        TargetScan::Missing => {
            crate::db::sync::clear_readopt_target_baseline(
                &transaction,
                &baseline.target_id,
                &database_path,
            )?;
        }
        _ => {
            return Err(AppError::conflict(
                "readopt",
                "目标当前无法安全读取，请先恢复文件内容或权限后再重新接管",
            ));
        }
    }
    transaction
        .commit()
        .map_err(|error| {
            AppError::database(&database_path, "commit_readopt_agent_target").with_source(error)
        })?;
    Ok(ReadoptAgentTargetResultDto {
        target_path: input.target_path.clone(),
    })
}

// ---------------------------------------------------------------------------
// 同步准备
// ---------------------------------------------------------------------------

struct PreparedAgentsSync {
    scope: Scope,
    project: Option<McpProjectRecord>,
    targets: Vec<PreparedAgentsTarget>,
}

struct PreparedAgentsTarget {
    descriptor: TargetDescriptor,
    ownership: ManagedOwnership,
    baseline: ManagedTargetBaseline,
    scan: TargetScan,
    readopt_available: bool,
    desired_projection: Value,
    row_versions: Vec<DatabaseRowVersion>,
    git: Option<crate::git::GitPathStatus>,
    allowed_root: PathBuf,
    delete_target: bool,
}

/// 预览 / Apply 共用的同步准备：
/// 1. desired = 已分配且 enabled 的 agent（项目级另加全局继承）；
/// 2. 为每个 desired agent 派生文件级 descriptor 与确定性投影；
/// 3. 既有受管目标中名称不在 desired 的（停用 / 取消分配 / 中央删除）
///    以空投影 + delete_target 进入预览，Apply 走 Mutation::Remove（删除前
///    快照，可恢复）；文件已缺失或不可读的删除候选直接跳过——没有可安全
///    删除的文件，也不应产生空投影写入；
/// 4. 空 desired 且无既有目标 → 不建目标、不建运行。
fn prepare_agents_sync(
    database: &mut Database,
    environment: &crate::adapters::ExplicitEnvironment,
    input: &PreviewAgentSyncInput,
) -> Result<PreparedAgentsSync, AppError> {
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
    agent_scope_supported(input.tool, scope)?;
    let project_root = project
        .as_ref()
        .map(|project| canonical_project(&project.root_path))
        .transpose()?;
    let directory_descriptor =
        agent_directory_descriptor(environment, input.tool, project_root.as_ref())?;

    let mut desired: Vec<AgentRecord> = repository::list_assigned_agents(
        database,
        input.tool,
        project.as_ref().map(|project| project.id.as_str()),
    )?
    .into_iter()
        .filter(|record| record.enabled)
        .collect();
    let all_tool_settings = repository::tool_settings_for_all_agents(database)?;
    if scope == Scope::Project {
        // 全局分配在项目内只读继承；互斥触发器保证同一 agent 不会同时
        // 出现在全局与项目分配中，这里按 id 防御性去重。
        let inherited = repository::list_assigned_agents(database, input.tool, None)?
            .into_iter()
            .filter(|record| record.enabled);
        let mut seen = BTreeSet::new();
        for record in inherited {
            if seen.insert(record.id.clone()) {
                desired.push(record);
            }
        }
        for record in &desired {
            seen.insert(record.id.clone());
        }
    }
    // 目标顺序按名称稳定排序，保证预览目标列表确定。
    desired.sort_by(|a, b| a.name.cmp(&b.name));

    let existing_rows = repository::list_agent_managed_targets(
        database,
        input.tool,
        scope,
        project.as_ref().map(|project| project.id.as_str()),
    )?;
    if desired.is_empty() && existing_rows.is_empty() {
        return Ok(PreparedAgentsSync {
            scope,
            project,
            targets: Vec::new(),
        });
    }
    let mut rows_by_path: BTreeMap<String, AgentManagedTargetRecord> = existing_rows
        .into_iter()
        .map(|row| (row.target_path.clone(), row))
        .collect();

    let extension = agent_file_extension(input.tool);
    let mut targets = Vec::new();
    let mut desired_names = BTreeSet::new();
    for record in &desired {
        desired_names.insert(record.name.clone());
        let file_descriptor = directory_descriptor.for_agent_file(&record.name, extension)?;
        let target_path = file_descriptor
            .path
            .clone()
            .ok_or_else(|| AppError::internal("agent 文件级目标缺少路径"))?;
        let baseline = match rows_by_path.remove(&target_path) {
            Some(row) => row.to_baseline()?,
            None => ensure_agent_target(database, &file_descriptor, project.as_ref())?,
        };
        let projection = build_agent_projection(
            input.tool,
            record,
            all_tool_settings
                .get(&record.id)
                .and_then(|settings| settings.get(&input.tool)),
        )?;
        let scan = scan_target(
            input.tool.adapter(),
            &file_descriptor,
            &ManagedOwnership::WholeDocument,
        );
        let assessment = assess_drift(&file_descriptor, &baseline, &scan);
        // 重新接管只对「外部改写了受管内容」这一类冲突有意义。
        let readopt_available = assessment.status == SyncStatus::ExternalOwnedChange;
        let row_versions =
            agent_row_versions(project.as_ref(), [record].into_iter())?;
        let git = project_root
            .as_ref()
            .zip(file_descriptor.path.as_deref())
            .map(|(root, path)| inspect_path(root, Path::new(path)))
            .transpose()?;
        targets.push(PreparedAgentsTarget {
            allowed_root: descriptor_allowed_root(&file_descriptor)?,
            descriptor: file_descriptor,
            ownership: ManagedOwnership::WholeDocument,
            baseline,
            scan,
            readopt_available,
            desired_projection: projection,
            row_versions,
            git,
            delete_target: false,
        });
    }

    // 删除候选：受管目标行的文件名不在 desired 中。
    for (target_path, row) in rows_by_path {
        let Some(name) = file_stem_of(&target_path) else {
            continue;
        };
        if desired_names.contains(&name) {
            continue;
        }
        let file_descriptor = directory_descriptor.for_agent_file(&name, extension)?;
        let baseline = row.to_baseline()?;
        let scan = scan_target(
            input.tool.adapter(),
            &file_descriptor,
            &ManagedOwnership::WholeDocument,
        );
        // 只在文件当前可读时计划删除；缺失或不可读的文件交给状态聚合呈现。
        if !matches!(scan, TargetScan::Observed(_)) {
            continue;
        }
        let row_versions = agent_row_versions(project.as_ref(), std::iter::empty())?;
        let git = project_root
            .as_ref()
            .zip(file_descriptor.path.as_deref())
            .map(|(root, path)| inspect_path(root, Path::new(path)))
            .transpose()?;
        targets.push(PreparedAgentsTarget {
            allowed_root: descriptor_allowed_root(&file_descriptor)?,
            descriptor: file_descriptor,
            ownership: ManagedOwnership::WholeDocument,
            baseline,
            scan,
            readopt_available: false,
            desired_projection: empty_projection(input.tool),
            row_versions,
            git,
            delete_target: true,
        });
    }

    Ok(PreparedAgentsSync {
        scope,
        project,
        targets,
    })
}
