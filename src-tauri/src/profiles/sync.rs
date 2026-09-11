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
        ArtifactKind::Mcp | ArtifactKind::Skill | ArtifactKind::Hook => {
            return Err(AppError::internal(
                "非 Provider/Prompt 种类不应进入档���同步",
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
    let active = repository::find_active_provider_profile(database, tool)?;
    if active.is_none() && target.baseline.full_hash.is_none() {
        return Err(AppError::not_found("activeProviderProfile", tool.as_str()));
    }
    if let Some(secret) = active.as_ref().and_then(|profile| profile.api_key.as_ref()) {
        redactor.register_secret(secret.clone());
    }
    let desired_projection = active
        .as_ref()
        .map(provider_projection)
        .transpose()?
        .unwrap_or_else(|| Value::Object(Map::new()));
    let ownership = provider_ownership(tool, target.projection.as_ref(), &desired_projection)?;
    let scan = scan_target(tool.adapter(), &descriptor, &ownership);
    let row_versions = active
        .as_ref()
        .map(provider_row_version)
        .transpose()?
        .into_iter()
        .collect();
    Ok(PreparedProfileSync {
        allowed_root: crate::adapters::descriptor_allowed_root(&descriptor)?,
        descriptor,
        ownership,
        baseline: target.baseline,
        scan,
        desired_projection,
        row_versions,
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
    let plan = build_preview_plan(
        prepared.descriptor.scope,
        None,
        vec![PreviewTargetRequest {
            descriptor: prepared.descriptor,
            ownership: prepared.ownership,
            baseline: prepared.baseline,
            scan: prepared.scan,
            baseline_mismatched_items: Vec::new(),
            readopt_available: false,
            desired_projection: prepared.desired_projection,
            row_versions: prepared.row_versions,
            git: prepared.git,
            exclude_from_git: false,
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
            hook_initial_adopt: false,
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
