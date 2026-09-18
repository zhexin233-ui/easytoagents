pub fn preview_provider_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    tool: Tool,
) -> Result<PreviewPlan, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Provider)?;
    let prepared = prepare_provider_sync(database, environment, redactor, tool)?;
    persist_prepared_preview(database, prepared, redactor)
}

/// 以当前原生 Provider 内容重新接管目标基线。
///
/// 该操作只刷新 `managed_targets` 的目标级基线，不修改中央生效档案，
/// 也不写入原生配置。调用方随后必须重新生成一份持久化 Preview，旧的
/// 冲突 Preview 仍由 Apply 校验拒绝消费。
pub fn readopt_provider_target(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &ReadoptProviderTargetInput,
) -> Result<ReadoptProviderTargetResultDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Provider)?;
    if input.target_path.trim().is_empty() {
        return Err(AppError::invalid_input(
            "targetPath",
            "Provider 重新接管缺少目标路径",
        ));
    }

    let mut descriptor = descriptor_for(environment, input.tool, ArtifactKind::Provider)?;
    let descriptor_target_path = descriptor_path(&descriptor)?;
    if descriptor_target_path != input.target_path {
        // 目标路径属于持久化 Preview 的身份合同；不能接受旧页面或跨工具
        // 输入，避免把另一份配置的内容写进当前 Provider 基线。
        return Err(AppError::invalid_input(
            "targetPath",
            "Provider 目标路径与当前工具目标不一致",
        ));
    }
    refine_claude_provider_policy(&mut descriptor);
    ensure_tool_is_available(&descriptor)?;

    let target = ensure_profile_target(database, &descriptor)?;
    let desired_projection = provider_sync_intent(database, input.tool)?.desired_projection;
    // Provider 没有 managed_items；ownership 仍必须沿用 Preview 的 codec
    // 口径（基线 + 当前中央意图），否则重新接管后下一份 Preview 的 managed
    // hash 会与本次扫描不一致。
    let ownership =
        provider_ownership(input.tool, target.projection.as_ref(), &desired_projection)?;
    let scan = scan_target(input.tool.adapter(), &descriptor, &ownership);
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_readopt_provider_target").with_source(error)
        })?;
    match &scan {
        TargetScan::Observed(observed) => crate::db::sync::update_readopt_target_baseline(
            &transaction,
            &target.baseline.target_id,
            &observed.full_hash,
            &observed.managed_hash,
            &database_path,
        )?,
        TargetScan::Missing => crate::db::sync::clear_readopt_target_baseline(
            &transaction,
            &target.baseline.target_id,
            &database_path,
        )?,
        // 解析、权限、类型、路径安全与其他不可证明状态必须 fail closed；
        // 尤其不能把一次读取失败当成新的基线。
        _ => {
            return Err(AppError::conflict(
                "readopt",
                "目标当前无法安全读取，请先恢复文件内容或权限后再重新接管",
            ));
        }
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_readopt_provider_target").with_source(error)
    })?;
    Ok(ReadoptProviderTargetResultDto {
        target_path: input.target_path.clone(),
    })
}

pub fn preview_prompt_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &SecretRedactor,
    tool: Tool,
) -> Result<PreviewPlan, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Prompt)?;
    let prepared = prepare_prompt_sync(database, environment, tool)?;
    persist_prepared_preview(database, prepared, redactor)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_profile_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    preview_id: &str,
    tool: Tool,
    artifact_kind: ArtifactKind,
) -> Result<ApplyResult, AppError> {
    ensure_profile_capability(tool, artifact_kind)?;
    if !matches!(artifact_kind, ArtifactKind::Provider | ArtifactKind::Prompt) {
        return Err(AppError::invalid_input(
            "artifactKind",
            "档案预览只能应用 Provider 或 Prompt",
        ));
    }
    let persisted = load_persisted_preview(database, preview_id)?;
    if persisted.items.len() != 1
        || persisted.items[0].envelope.descriptor.tool != tool
        || persisted.items[0].envelope.descriptor.artifact_kind != artifact_kind
        || persisted.scope != Scope::Global
        || persisted.project_id.is_some()
        || persisted.items[0].envelope.descriptor.scope != Scope::Global
    {
        return Err(AppError::stale_preview(preview_id, "profileTarget"));
    }
    let prepared = match artifact_kind {
        ArtifactKind::Provider => prepare_provider_sync(database, environment, redactor, tool)?,
        ArtifactKind::Prompt => prepare_prompt_sync(database, environment, tool)?,
        // 档案同步只服务 Provider/Prompt；MCP/Skill/Hook/Agent 有各自的同步入口。
        ArtifactKind::Mcp | ArtifactKind::Skill | ArtifactKind::Hook | ArtifactKind::Agent => {
            return Err(AppError::internal(
                "非 Provider/Prompt 种类不应进入档案同步",
            ));
        }
    };
    let input = ApplyTargetInput {
        descriptor: prepared.descriptor,
        ownership: prepared.ownership,
        desired_projection: prepared.desired_projection,
        allowed_root: prepared.allowed_root,
        central_skills_root: None,
        delete_target: prepared.delete_target,
        managed_items: Vec::new(),
        remove_managed_item_ids: Vec::new(),
        skill_takeover_entries: Vec::new(),
        project_native_action: None,
    };
    apply_persisted_preview(
        write_operations,
        database,
        paths,
        preview_id,
        &[input],
        &NoApplyFault,
    )
}

struct PreparedProfileSync {
    descriptor: TargetDescriptor,
    ownership: ManagedOwnership,
    baseline: ManagedTargetBaseline,
    scan: TargetScan,
    desired_projection: Value,
    row_versions: Vec<DatabaseRowVersion>,
    allowed_root: PathBuf,
    git: Option<GitPathStatus>,
    delete_target: bool,
}

fn prepare_provider_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    tool: Tool,
) -> Result<PreparedProfileSync, AppError> {
    let mut descriptor = descriptor_for(environment, tool, ArtifactKind::Provider)?;
    refine_claude_provider_policy(&mut descriptor);
    ensure_tool_is_available(&descriptor)?;
    let target = ensure_profile_target(database, &descriptor)?;
    let intent = provider_sync_intent(database, tool)?;
    if intent.active.is_none() && target.baseline.full_hash.is_none() {
        return Err(AppError::not_found("activeProviderProfile", tool.as_str()));
    }
    for profile in &intent.profiles {
        if let Some(secret) = profile.api_key.as_ref() {
            redactor.register_secret(secret.clone());
        }
    }
    let ownership =
        provider_ownership(tool, target.projection.as_ref(), &intent.desired_projection)?;
    let scan = scan_target(tool.adapter(), &descriptor, &ownership);
    Ok(PreparedProfileSync {
        allowed_root: crate::adapters::descriptor_allowed_root(&descriptor)?,
        descriptor,
        ownership,
        baseline: target.baseline,
        scan,
        desired_projection: intent.desired_projection,
        row_versions: intent.row_versions,
        git: None,
        delete_target: false,
    })
}

fn prepare_prompt_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<PreparedProfileSync, AppError> {
    let descriptor = descriptor_for(environment, tool, ArtifactKind::Prompt)?;
    ensure_tool_is_available(&descriptor)?;
    let target = ensure_profile_target(database, &descriptor)?;
    let assigned = repository::find_active_prompt_profile(database, tool)?;
    let assigned = match assigned {
        Some(record) => Some(record),
        None if target.baseline.full_hash.is_none() => {
            return Err(AppError::not_found("activePromptProfile", tool.as_str()));
        }
        None => None,
    };
    let desired_projection = Value::String(
        assigned
            .as_ref()
            .map(|profile| profile.body.clone())
            .unwrap_or_default(),
    );
    let row_versions = assigned
        .as_ref()
        .map(prompt_row_version)
        .transpose()?
        .into_iter()
        .collect();
    let scan = scan_target(
        tool.adapter(),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    );
    Ok(PreparedProfileSync {
        allowed_root: crate::adapters::descriptor_allowed_root(&descriptor)?,
        git: None,
        descriptor,
        ownership: ManagedOwnership::WholeDocument,
        baseline: target.baseline,
        scan,
        desired_projection,
        row_versions,
        delete_target: assigned.is_none(),
    })
}

fn persist_prepared_preview(
    database: &mut Database,
    prepared: PreparedProfileSync,
    redactor: &SecretRedactor,
) -> Result<PreviewPlan, AppError> {
    let readopt_available = prepared.descriptor.artifact_kind == ArtifactKind::Provider
        && prepared.baseline.full_hash.is_some()
        && prepared.baseline.managed_hash.is_some()
        && crate::sync::assess_drift(&prepared.descriptor, &prepared.baseline, &prepared.scan)
            .status
            == crate::domain::SyncStatus::ExternalOwnedChange;
    let plan = build_profile_preview_plan(
        prepared.descriptor.scope,
        None,
        vec![PreviewTargetRequest {
            descriptor: prepared.descriptor,
            ownership: prepared.ownership,
            baseline: prepared.baseline,
            scan: prepared.scan,
            baseline_mismatched_items: Vec::new(),
            readopt_available,
            desired_projection: prepared.desired_projection,
            row_versions: prepared.row_versions,
            git: prepared.git,
            exclude_from_git: false,
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
            hook_initial_adopt: false,
            hard_block: None,
        }],
        redactor,
    )?;
    persist_preview(database, &plan)?;
    Ok(plan)
}

struct ManagedProfileTarget {
    baseline: ManagedTargetBaseline,
    projection: Option<Value>,
}

/// 只读查找 Provider/Prompt 的托管目标；现场状态扫描不得因为目标尚未登记而
/// 写入 `managed_targets`，因此与会创建身份行的 `ensure_profile_target` 分开。
fn find_profile_target(
    database: &Database,
    descriptor: &TargetDescriptor,
) -> Result<Option<ManagedProfileTarget>, AppError> {
    let target_path = descriptor_path(descriptor)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let row = database
        .connection()
        .query_row(
            "SELECT id, row_version, baseline_full_hash, baseline_managed_hash,
                    baseline_projection_json
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = ?2 AND scope = ?3
               AND project_id IS NULL AND target_path = ?4",
            params![
                descriptor.tool.as_str(),
                descriptor.artifact_kind.as_str(),
                descriptor.scope.as_str(),
                target_path,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database_path, "find_profile_managed_target").with_source(error)
        })?;
    let Some(row) = row else {
        return Ok(None);
    };
    let projection = row
        .4
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                AppError::database(&database_path, "parse_profile_managed_baseline")
                    .with_source(error)
            })
        })
        .transpose()?;
    Ok(Some(ManagedProfileTarget {
        baseline: ManagedTargetBaseline {
            target_id: row.0,
            target_row_version: row.1,
            full_hash: row.2,
            managed_hash: row.3,
        },
        projection,
    }))
}

/// Provider/Prompt 全局目标的现场扫描。这里只读数据库与原生文件；执行覆盖或
/// 采纳时必须再请求 `ExternalChangePlan`，以绑定最新 observation/hash/版本。
pub fn list_global_profile_target_statuses(
    database: &Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Vec<ProfileTargetStatusDto>, AppError> {
    let mut statuses = Vec::new();
    for artifact_kind in [ArtifactKind::Provider, ArtifactKind::Prompt] {
        if artifact_kind == ArtifactKind::Provider
            && tool.adapter().provider_codec().is_none()
        {
            continue;
        }
        let mut descriptor = descriptor_for(environment, tool, artifact_kind)?;
        if artifact_kind == ArtifactKind::Provider {
            refine_claude_provider_policy(&mut descriptor);
        }
        let target = find_profile_target(database, &descriptor)?;
        let baseline = target
            .as_ref()
            .map(|target| target.baseline.clone())
            .unwrap_or_else(|| ManagedTargetBaseline {
                target_id: "status-only".to_owned(),
                target_row_version: 0,
                full_hash: None,
                managed_hash: None,
            });
        let ownership = match artifact_kind {
            ArtifactKind::Provider => {
                let intent = provider_sync_intent(database, tool)?;
                // 没有中央 Provider 意图且目标也没有历史 baseline 时，原生文件
                // 尚不属于本应用的受管目标；不要让 codec 因无 selector 而把
                // 状态扫描升级成 RPC 错误。用户仍可从 Provider 导入入口接管。
                if intent.active.is_none() && baseline.full_hash.is_none() {
                    continue;
                }
                provider_ownership(
                    tool,
                    target.as_ref().and_then(|value| value.projection.as_ref()),
                    &intent.desired_projection,
                )?
            }
            ArtifactKind::Prompt => {
                ManagedOwnership::WholeDocument
            }
            _ => unreachable!("profile status only supports Provider/Prompt"),
        };
        let scan = scan_target(tool.adapter(), &descriptor, &ownership);
        let assessment = crate::sync::assess_drift(&descriptor, &baseline, &scan);
        statuses.push(ProfileTargetStatusDto {
            artifact_kind,
            tool,
            target_path: descriptor.path.clone(),
            status: assessment.status,
            diagnostic_code: assessment.diagnostic_codes.first().cloned(),
        });
    }
    Ok(statuses)
}

fn ensure_profile_target(
    database: &mut Database,
    descriptor: &TargetDescriptor,
) -> Result<ManagedProfileTarget, AppError> {
    let target_path = descriptor_path(descriptor)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let existing = database
        .connection()
        .query_row(
            "SELECT id, row_version, baseline_full_hash, baseline_managed_hash,
                    baseline_projection_json
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = ?2 AND scope = ?3
               AND project_id IS NULL AND target_path = ?4",
            params![
                descriptor.tool.as_str(),
                descriptor.artifact_kind.as_str(),
                descriptor.scope.as_str(),
                target_path,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database_path, "find_profile_managed_target").with_source(error)
        })?;
    let row = if let Some(row) = existing {
        row
    } else {
        let id = Uuid::new_v4().to_string();
        database
            .connection_mut()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, project_id, target_path
                 ) VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
                params![
                    id,
                    descriptor.tool.as_str(),
                    descriptor.artifact_kind.as_str(),
                    descriptor.scope.as_str(),
                    target_path,
                ],
            )
            .map_err(|error| {
                AppError::database(&database_path, "insert_profile_managed_target")
                    .with_source(error)
            })?;
        (id, 1, None, None, None)
    };
    let projection = row
        .4
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                AppError::database(&database_path, "parse_profile_managed_baseline")
                    .with_source(error)
            })
        })
        .transpose()?;
    Ok(ManagedProfileTarget {
        baseline: ManagedTargetBaseline {
            target_id: row.0,
            target_row_version: row.1,
            full_hash: row.2,
            managed_hash: row.3,
        },
        projection,
    })
}
