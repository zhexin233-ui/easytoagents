pub fn get_tool_profile_status(
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<ToolProfileStatusDto, AppError> {
    let mut provider = descriptor_for(environment, tool, ArtifactKind::Provider)?;
    refine_claude_provider_policy(&mut provider);
    let prompt = descriptor_for(environment, tool, ArtifactKind::Prompt)?;
    Ok(ToolProfileStatusDto {
        tool,
        availability: environment.tool_availability(tool),
        installation_version: environment.installation_version(tool).map(str::to_owned),
        installation_probe_diagnostic: environment
            .installation_probe_diagnostic(tool)
            .map(str::to_owned),
        provider_target_path: provider.path.clone(),
        prompt_target_path: prompt.path.clone(),
        provider_capability: provider.capability,
        prompt_capability: prompt.capability,
        prompt_override: prompt.prompt_override,
        provider_policy: provider.policy,
        new_session_notice: NEW_SESSION_NOTICE.to_owned(),
        bearer_token_warning: (tool == Tool::Codex).then(|| CODEX_BEARER_TOKEN_WARNING.to_owned()),
    })
}

pub fn discover_provider_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &SecretRedactor,
    tool: Tool,
) -> Result<Option<ProviderImportPreviewDto>, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Provider)?;
    if !repository::list_provider_profiles(database, tool)?.is_empty() {
        return Ok(None);
    }
    let discovered = match discover_native_provider(environment, tool)? {
        Some(discovered) => discovered,
        None => return Ok(None),
    };
    let suggested_name = discovered
        .suggested_name
        .as_ref()
        .filter(|name| ArtifactName::parse((*name).clone()).is_ok())
        .cloned()
        .unwrap_or_else(|| "已导入渠道".to_owned());
    let auth_kind = discovered_auth_kind(&discovered)?;
    validate_provider_fields(&ProviderFieldsInput {
        tool,
        auth_kind,
        name: &suggested_name,
        api_base_url: &discovered.api_base_url,
        api_key: discovered.api_key.as_deref(),
        default_model: &discovered.default_model,
    })?;
    validate_discovered_provider_config(tool, &discovered)?;
    let preview_id = Uuid::new_v4().to_string();
    let mut target_redactor = redactor.clone();
    if let Some(api_key) = &discovered.api_key {
        target_redactor.register_secret(api_key.clone());
    }
    let redacted_projection = target_redactor
        .redact_structure(&discovered.projection)
        .into_value();
    repository::persist_import_preview(
        database,
        &ImportPreviewRecord {
            id: preview_id.clone(),
            tool,
            artifact_kind: ArtifactKind::Provider,
            target_path: discovered.target_path.clone(),
            observed_full_hash: discovered.full_hash,
            suggested_name: suggested_name.clone(),
            redacted_preview_json: serde_json::to_string(&json!({
                "projection": redacted_projection,
                "apiKeyConfigured": discovered.api_key.is_some(),
                "authKind": auth_kind,
                "skippedEnvKeys": discovered.skipped_env_keys,
            }))
            .map_err(|error| {
                AppError::invalid_input("importPreview", "导入预览无法序列化")
                    .with_source_redacted(error, redactor)
            })?,
            status: "previewed".to_owned(),
        },
    )?;
    Ok(Some(ProviderImportPreviewDto {
        preview_id,
        tool,
        target_path: discovered.target_path,
        suggested_name,
        auth_kind,
        api_base_url: discovered.api_base_url,
        api_key_configured: discovered.api_key.is_some(),
        default_model: discovered.default_model,
        redacted_projection,
        skipped_env_keys: discovered.skipped_env_keys,
    }))
}

pub fn confirm_provider_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: ConfirmImportInput,
) -> Result<ProviderProfileDto, AppError> {
    let preview = repository::get_import_preview(database, &input.preview_id)?;
    if preview.artifact_kind != ArtifactKind::Provider || preview.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &input.preview_id,
            &preview.status,
        ));
    }
    ArtifactName::parse(input.name.clone())?;
    let discovered = discover_native_provider(environment, preview.tool)?
        .ok_or_else(|| AppError::stale_preview(&preview.id, &preview.target_path))?;
    if discovered.target_path != preview.target_path
        || discovered.full_hash != preview.observed_full_hash
    {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let id = Uuid::new_v4().to_string();
    let provider_id = discovered
        .provider_id
        .clone()
        .unwrap_or_else(|| generated_codex_provider_id(&id));
    validate_codex_provider_id(preview.tool, &provider_id)?;
    let auth_kind = discovered_auth_kind(&discovered)?;
    validate_provider_fields(&ProviderFieldsInput {
        tool: preview.tool,
        auth_kind,
        name: &input.name,
        api_base_url: &discovered.api_base_url,
        api_key: discovered.api_key.as_deref(),
        default_model: &discovered.default_model,
    })?;
    let options_input = ProviderCodecInput {
        auth_kind: &discovered.auth_kind,
        credential_env_key: Some(discovered.credential_env_key.as_str()),
        extra_env: &discovered.extra_env,
        wire_api: discovered.wire_api.as_deref(),
        zcode_kind: discovered.zcode_kind.as_deref().or(Some("anthropic")),
        opencode_npm: discovered.opencode_npm.as_deref(),
        opencode_api: discovered.opencode_api.as_deref(),
    };
    let options = provider_options_from_codec(preview.tool, &options_input)?;
    let config = StoredProviderConfig::from_input(
        preview.tool,
        &provider_id,
        options,
        discovered.extra_provider_fields,
    )?;
    if let Some(api_key) = &discovered.api_key {
        redactor.register_secret(api_key.clone());
    }
    let projection_json = serde_json::to_string(&discovered.projection).map_err(|error| {
        AppError::invalid_input("importPreview", "Provider 基线无法序列化")
            .with_source_redacted(error, redactor)
    })?;
    let record = repository::adopt_imported_provider(
        database,
        &preview,
        &NewProviderProfileRecord {
            id,
            tool: preview.tool,
            name: input.name,
            api_base_url: optional_text(&discovered.api_base_url),
            api_key: discovered.api_key,
            default_model: optional_text(&discovered.default_model),
            config_json: serde_json::to_string(&config).map_err(|error| {
                AppError::invalid_input("providerOptions", "导入 Provider 选项无法序列化")
                    .with_source_redacted(error, redactor)
            })?,
            is_active: true,
        },
        &ImportedBaselineRecord {
            target_id: Uuid::new_v4().to_string(),
            target_path: discovered.target_path,
            full_hash: discovered.full_hash,
            managed_hash: hash_json(&discovered.projection),
            projection_json,
        },
    )?;
    provider_dto(&record)
}
