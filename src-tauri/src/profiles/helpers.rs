fn descriptor_for(
    environment: &ExplicitEnvironment,
    tool: Tool,
    artifact_kind: ArtifactKind,
) -> Result<TargetDescriptor, AppError> {
    let context = DiscoveryContext {
        environment,
        project_root: None,
        claude_user_mcp_probe: environment.claude_user_mcp_probe(),
        claude_customization_policy_probe: environment.claude_customization_policy_probe(),
    };
    find_descriptor(
        tool,
        &context,
        artifact_kind,
        Scope::Global,
        "targetDescriptor",
        artifact_kind.as_str(),
    )
}

/// 显式运行时证据决定默认状态；原生 settings 中的宿主标记只能把状态收紧。
fn refine_claude_provider_policy(descriptor: &mut TargetDescriptor) {
    if descriptor.tool != Tool::Claude
        || descriptor.artifact_kind != ArtifactKind::Provider
        || descriptor.policy == PolicyState::Blocked
    {
        return;
    }
    let marker_ownership =
        ManagedOwnership::selectors([["env", CLAUDE_PROVIDER_MANAGED_BY_HOST_KEY]]);
    match scan_target(Tool::Claude.adapter(), descriptor, &marker_ownership) {
        TargetScan::Observed(observed) => {
            let marker = observed
                .managed_projection
                .get("env")
                .and_then(Value::as_object)
                .and_then(|env| env.get(CLAUDE_PROVIDER_MANAGED_BY_HOST_KEY));
            match marker {
                Some(Value::String(value)) if !value.is_empty() => {
                    descriptor.policy = PolicyState::Blocked;
                }
                Some(Value::String(_)) | None => {}
                Some(_) => descriptor.policy = PolicyState::Unknown,
            }
        }
        TargetScan::Missing => {}
        TargetScan::ParseError
        | TargetScan::ManagedItemBaselineMismatch
        | TargetScan::PermissionDenied
        | TargetScan::Unavailable
        | TargetScan::TargetTypeChanged(_)
        | TargetScan::Failed => descriptor.policy = PolicyState::Unknown,
    }
}

fn ensure_profile_capability(tool: Tool, artifact_kind: ArtifactKind) -> Result<(), AppError> {
    // Cursor Prompt 已按官方规则文件合同开放（任务 09-06-cursor-prompt-support）；
    // Provider / API Key / 模型仍无官方文件合同，保持拒绝。
    if tool == Tool::Cursor && artifact_kind == ArtifactKind::Provider {
        return Err(cursor_unsupported(artifact_kind));
    }
    Ok(())
}

fn cursor_unsupported(artifact_kind: ArtifactKind) -> AppError {
    AppError::invalid_input(
        "capability",
        match artifact_kind {
            ArtifactKind::Provider => "CURSOR_PROVIDER_UNSUPPORTED",
            ArtifactKind::Prompt | ArtifactKind::Mcp | ArtifactKind::Skill | ArtifactKind::Hook => {
                "Cursor 该能力不受支持"
            }
        },
    )
}

fn descriptor_path(descriptor: &TargetDescriptor) -> Result<String, AppError> {
    crate::adapters::descriptor_path(descriptor)
}

fn scan_error(descriptor: &TargetDescriptor, scan: &TargetScan) -> AppError {
    let path = descriptor.path.as_deref().unwrap_or("<unsupported>");
    match scan {
        TargetScan::PermissionDenied => AppError::permission(path, "scan_profile_target"),
        TargetScan::ParseError => AppError::parse(path, descriptor.format.as_str()),
        TargetScan::Unavailable => AppError::not_found("target", path),
        TargetScan::TargetTypeChanged(_) => {
            AppError::conflict("targetPath", "目标类型与配置格式不一致")
        }
        TargetScan::Failed | TargetScan::ManagedItemBaselineMismatch => {
            AppError::new(ErrorCode::Conflict, "目标无法安全读取", true)
        }
        TargetScan::Missing | TargetScan::Observed(_) => {
            AppError::new(ErrorCode::Conflict, "目标状态不符合导入条件", true)
        }
    }
}

fn provider_options_from_codec(
    tool: Tool,
    input: &ProviderCodecInput<'_>,
) -> Result<ProviderOptionsInput, AppError> {
    let codec = tool
        .adapter()
        .provider_codec()
        .ok_or_else(|| cursor_unsupported(ArtifactKind::Provider))?;
    let options = codec.default_options(input)?;
    let credential_env_key =
        match options.credential_env_key.as_deref() {
            None => None,
            Some(value) => Some(ClaudeCredentialEnvKey::from_stable_str(value).ok_or_else(
                || AppError::invalid_input("providerOptions", "Claude 凭据环境变量名无效"),
            )?),
        };
    Ok(ProviderOptionsInput {
        credential_env_key,
        extra_env: options.extra_env,
        wire_api: options.wire_api,
        zcode_kind: options.zcode_kind,
        opencode_npm: options.opencode_npm,
        opencode_api: options.opencode_api,
    })
}

fn provider_dto(record: &ProviderProfileRecord) -> Result<ProviderProfileDto, AppError> {
    let config = parse_stored_provider_config(record)?;
    Ok(ProviderProfileDto {
        id: record.id.clone(),
        tool: record.tool,
        name: record.name.clone(),
        api_base_url: record.api_base_url.clone().unwrap_or_default(),
        api_key_configured: record
            .api_key
            .as_ref()
            .is_some_and(|value| !value.is_empty()),
        default_model: record.default_model.clone().unwrap_or_default(),
        options: config.options_dto(),
        is_active: record.is_active,
        row_version: safe_row_version(record.row_version)?,
    })
}

fn prompt_dto(record: &PromptProfileRecord) -> Result<PromptProfileDto, AppError> {
    let mut global_tools = Vec::new();
    if record.is_active_claude {
        global_tools.push(Tool::Claude);
    }
    if record.is_active_codex {
        global_tools.push(Tool::Codex);
    }
    if record.is_active_zcode {
        global_tools.push(Tool::Zcode);
    }
    if record.is_active_cursor {
        global_tools.push(Tool::Cursor);
    }
    if record.is_active_opencode {
        global_tools.push(Tool::Opencode);
    }
    Ok(PromptProfileDto {
        id: record.id.clone(),
        name: record.name.clone(),
        body: record.body.clone(),
        global_tools,
        imported_from_path: record.imported_from_path.clone(),
        row_version: safe_row_version(record.row_version)?,
    })
}

fn parse_stored_provider_config(
    record: &ProviderProfileRecord,
) -> Result<StoredProviderConfig, AppError> {
    serde_json::from_str(&record.config_json).map_err(|error| {
        AppError::invalid_input("providerOptions", "Provider 中央配置已损坏").with_source(error)
    })
}

fn provider_row_version(profile: &ProviderProfileRecord) -> Result<DatabaseRowVersion, AppError> {
    Ok(DatabaseRowVersion {
        entity_type: DatabaseEntityType::ProviderProfile,
        entity_id: profile.id.clone(),
        row_version: safe_row_version(profile.row_version)?,
    })
}

fn prompt_row_version(profile: &PromptProfileRecord) -> Result<DatabaseRowVersion, AppError> {
    Ok(DatabaseRowVersion {
        entity_type: DatabaseEntityType::PromptProfile,
        entity_id: profile.id.clone(),
        row_version: safe_row_version(profile.row_version)?,
    })
}

fn generated_codex_provider_id(id: &str) -> String {
    format!("easytoagents_{}", id.replace('-', ""))
}

fn discovered_provider_allows_missing_api_key(tool: Tool, discovered: &DiscoveredProvider) -> bool {
    codex_provider_allows_missing_api_key(tool, discovered.provider_id.as_deref())
}

fn codex_provider_allows_missing_api_key(tool: Tool, provider_id: Option<&str>) -> bool {
    tool == Tool::Codex && provider_id == Some(CODEX_OPENAI_PROVIDER_ID)
}

fn validate_codex_provider_id(tool: Tool, provider_id: &str) -> Result<(), AppError> {
    if tool != Tool::Codex {
        return Ok(());
    }
    if provider_id == CODEX_OPENAI_PROVIDER_ID {
        return Ok(());
    }
    validate_codex_custom_provider_id(tool, provider_id)
}

fn validate_codex_custom_provider_id(tool: Tool, provider_id: &str) -> Result<(), AppError> {
    if tool != Tool::Codex {
        return Ok(());
    }
    if provider_id.is_empty()
        || provider_id.len() > 100
        || CODEX_RESERVED_PROVIDER_IDS.contains(&provider_id)
        || !provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(AppError::invalid_input(
            "providerId",
            "Codex provider id 非法或属于不支持接管的内置项",
        ));
    }
    Ok(())
}
