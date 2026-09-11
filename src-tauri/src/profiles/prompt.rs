pub fn list_prompt_profiles(database: &Database) -> Result<Vec<PromptProfileDto>, AppError> {
    repository::list_prompt_profiles(database)?
        .iter()
        .map(prompt_dto)
        .collect()
}

pub fn create_prompt_profile(
    database: &mut Database,
    input: PromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    validate_prompt_fields(&input.name, &input.body)?;
    // 新建档案不绑定工具也不自动启用；用图标按工具启用（导入路径除外，见 confirm_prompt_import）。
    prompt_dto(&repository::insert_prompt_profile(
        database,
        &NewPromptProfileRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            body: input.body,
            is_active_claude: false,
            is_active_codex: false,
            is_active_zcode: false,
            is_active_cursor: false,
            is_active_opencode: false,
            imported_from_path: None,
        },
    )?)
}

pub fn update_prompt_profile(
    database: &mut Database,
    input: UpdatePromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    validate_prompt_fields(&input.name, &input.body)?;
    prompt_dto(&repository::update_prompt_profile(
        database,
        &input.id,
        &input.name,
        &input.body,
        i64::from(input.row_version),
    )?)
}

pub fn set_global_prompt_assignment(
    database: &mut Database,
    input: &SetGlobalPromptAssignmentInput,
) -> Result<PromptProfileDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Prompt)?;
    prompt_dto(&repository::set_global_prompt_assignment(
        database,
        input.tool,
        &input.prompt_profile_id,
        input.assigned,
        i64::from(input.row_version),
    )?)
}

pub fn delete_prompt_profile(
    database: &mut Database,
    input: &VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    repository::delete_prompt_profile(database, &input.id, i64::from(input.row_version))?;
    Ok(DeleteProfileResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

pub fn discover_prompt_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Option<PromptImportPreviewDto>, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Prompt)?;
    let descriptor = descriptor_for(environment, tool, ArtifactKind::Prompt)?;
    ensure_tool_is_available(&descriptor)?;
    // 该工具已有生效档案、或档案库已有同源导入（imported_from_path 相同）时不再检测。
    let prompt_path = descriptor_path(&descriptor)?;
    if repository::prompt_import_blocked(database, tool, &prompt_path)? {
        return Ok(None);
    }
    let scan = scan_target(
        tool.adapter(),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    );
    let observed = match scan {
        TargetScan::Observed(observed) => observed,
        TargetScan::Missing => return Ok(None),
        TargetScan::ParseError => {
            return Err(AppError::parse(&descriptor_path(&descriptor)?, "markdown"));
        }
        _ => return Err(scan_error(&descriptor, &scan)),
    };
    let prompt_path = descriptor_path(&descriptor)?;
    let body = observed
        .managed_projection
        .as_str()
        .ok_or_else(|| AppError::parse(&prompt_path, "markdown"))?
        .to_owned();
    if body.trim().is_empty() {
        return Ok(None);
    }
    let preview_id = Uuid::new_v4().to_string();
    let target_path = prompt_path;
    let suggested_name = "已导入提示词".to_owned();
    repository::persist_import_preview(
        database,
        &ImportPreviewRecord {
            id: preview_id.clone(),
            tool,
            artifact_kind: ArtifactKind::Prompt,
            target_path: target_path.clone(),
            observed_full_hash: observed.full_hash.clone(),
            suggested_name: suggested_name.clone(),
            redacted_preview_json: "{}".to_owned(),
            status: "previewed".to_owned(),
        },
    )?;
    Ok(Some(PromptImportPreviewDto {
        preview_id,
        tool,
        target_path,
        suggested_name,
        body,
    }))
}

pub fn confirm_prompt_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: ConfirmImportInput,
) -> Result<PromptProfileDto, AppError> {
    let preview = repository::get_import_preview(database, &input.preview_id)?;
    if preview.artifact_kind != ArtifactKind::Prompt || preview.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &input.preview_id,
            &preview.status,
        ));
    }
    let descriptor = descriptor_for(environment, preview.tool, ArtifactKind::Prompt)?;
    ensure_tool_is_available(&descriptor)?;
    if descriptor_path(&descriptor)? != preview.target_path {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let observed = match scan_target(
        preview.tool.adapter(),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    ) {
        TargetScan::Observed(observed) if observed.full_hash == preview.observed_full_hash => {
            observed
        }
        _ => return Err(AppError::stale_preview(&preview.id, &preview.target_path)),
    };
    let body = observed
        .managed_projection
        .as_str()
        .ok_or_else(|| AppError::parse(&preview.target_path, "markdown"))?
        .to_owned();
    validate_prompt_fields(&input.name, &body)?;
    let record = repository::adopt_imported_prompt(
        database,
        &preview,
        &NewPromptProfileRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            body: body.clone(),
            is_active_claude: preview.tool == Tool::Claude,
            is_active_codex: preview.tool == Tool::Codex,
            is_active_zcode: preview.tool == Tool::Zcode,
            is_active_cursor: preview.tool == Tool::Cursor,
            is_active_opencode: preview.tool == Tool::Opencode,
            imported_from_path: Some(preview.target_path.clone()),
        },
        &ImportedBaselineRecord {
            target_id: Uuid::new_v4().to_string(),
            target_path: preview.target_path.clone(),
            full_hash: observed.full_hash,
            managed_hash: observed.managed_hash,
            projection_json: serde_json::to_string(&Value::String(body)).map_err(|error| {
                AppError::invalid_input("body", "提示词基线无法序列化").with_source(error)
            })?,
        },
    )?;
    prompt_dto(&record)
}
