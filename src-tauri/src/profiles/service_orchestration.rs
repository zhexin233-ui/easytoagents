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

/// 把候选原因码换成用户可读的稳定文案。
///
/// 原因码本身只出现在候选 DTO 的 `reason` 字段（UI 用于标不可选及提示），
/// 错误对象只携带静态文案，因为 `AppError::invalid_input` 只接受 `&'static str`。
fn provider_import_reason(code: &str) -> &'static str {
    match code {
        crate::adapters::pi::PI_PROVIDER_ENTRY_INVALID => "原生 provider 条目不是 JSON 对象",
        crate::adapters::pi::PI_PROVIDER_ID_INVALID => "原生 provider id 非法，不能作为写入键",
        crate::adapters::pi::PI_PROVIDER_FIELDS_INVALID => {
            "原生渠道配置不完整或格式不受支持（需要接入地址与 API Key）"
        }
        _ => "原生渠道配置不可导入",
    }
}

/// 单个候选的只读展示 DTO。只投影非敏感字段；`api`/`models` 摘要让用户能
/// 确认“其他内容确实被保留”，凭据值一律不回显。
fn provider_candidate_dto(
    candidate: &ProviderCandidate,
    redactor: &SecretRedactor,
) -> Result<ProviderImportCandidateDto, AppError> {
    let discovered = &candidate.discovered;
    let mut candidate_redactor = redactor.clone();
    if let Some(api_key) = &discovered.api_key {
        candidate_redactor.register_secret(api_key.clone());
    }
    let redacted_projection = candidate_redactor
        .redact_structure(&discovered.projection)
        .into_value();
    let api_format = discovered
        .extra_provider_fields
        .get("api")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| discovered.wire_api.clone())
        .or_else(|| discovered.zcode_kind.clone())
        .or_else(|| discovered.opencode_api.clone());
    let model_count = match discovered.extra_provider_fields.get("models") {
        Some(Value::Array(models)) => models.len(),
        Some(Value::Object(models)) => models.len(),
        _ => 0,
    };
    Ok(ProviderImportCandidateDto {
        candidate_id: candidate.candidate_id.clone(),
        provider_id: discovered.provider_id.clone().unwrap_or_default(),
        suggested_name: candidate.suggested_name.clone(),
        status: candidate.status,
        reason: candidate.reason.clone(),
        // 认证方式无法解析时该候选已是 `invalid`，这里只给出可渲染的默认值；
        // 真正的判定仍由 `provider_candidate_status` 负责。
        auth_kind: discovered_auth_kind(discovered).unwrap_or(ProviderAuthKind::ApiKey),
        default_provider: discovered.is_default_provider,
        api_base_url: discovered.api_base_url.clone(),
        api_key_configured: discovered.api_key.is_some(),
        default_model: discovered.default_model.clone(),
        api_format,
        model_count: u32::try_from(model_count).unwrap_or(u32::MAX),
        redacted_projection,
        skipped_env_keys: discovered.skipped_env_keys.clone(),
    })
}

pub fn discover_provider_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &SecretRedactor,
    tool: Tool,
) -> Result<ProviderImportPreviewDto, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Provider)?;
    let candidates = provider_candidates(database, environment, tool)?;
    let target_path = match candidates.first() {
        Some(candidate) => candidate.discovered.target_path.clone(),
        None => {
            let mut descriptor = descriptor_for(environment, tool, ArtifactKind::Provider)?;
            refine_claude_provider_policy(&mut descriptor);
            descriptor.path.clone().unwrap_or_default()
        }
    };
    let mut dtos = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        dtos.push(provider_candidate_dto(candidate, redactor)?);
    }
    let importable = dtos
        .iter()
        .any(|dto| dto.status == ProviderImportCandidateStatus::Importable);
    // 没有可导入候选就不签发预览：不产生可消费的证据行。
    let preview_id = importable.then(|| Uuid::new_v4().to_string());
    if let (Some(preview_id), Some(candidate)) = (preview_id.as_ref(), candidates.first()) {
        let context = json!({
            "version": 1,
            "candidates": candidates
                .iter()
                .map(|candidate| json!({
                    "candidateId": candidate.candidate_id,
                    "providerId": candidate.discovered.provider_id,
                    "suggestedName": candidate.suggested_name,
                }))
                .collect::<Vec<_>>(),
        });
        // 该列有 `json_type = 'object'` 约束：候选列表必须包在对象里。
        let redacted_preview_json =
            serde_json::to_string(&json!({ "candidates": dtos })).map_err(|error| {
                AppError::invalid_input("importPreview", "导入预览无法序列化")
                    .with_source_redacted(error, redactor)
            })?;
        crate::db::provider_imports::persist_preview(
            database,
            &ProviderImportPreviewRecord {
                id: preview_id.clone(),
                tool,
                target_path: target_path.clone(),
                observed_full_hash: candidate.discovered.full_hash.clone(),
                context_json: serde_json::to_string(&context).map_err(|error| {
                    AppError::invalid_input("importPreview", "导入证据无法序列化")
                        .with_source_redacted(error, redactor)
                })?,
                redacted_preview_json,
                status: "previewed".to_owned(),
            },
        )?;
    }
    let message = (preview_id.is_none() && !dtos.is_empty())
        .then(|| "该工具的已有渠道都已在中央档案中，无需再次导入。".to_owned());
    Ok(ProviderImportPreviewDto {
        preview_id,
        tool,
        target_path,
        candidates: dtos,
        message,
    })
}

pub fn confirm_provider_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: ConfirmProviderImportInput,
) -> Result<ProviderImportResultDto, AppError> {
    if input.items.is_empty() {
        return Err(AppError::invalid_input(
            "items",
            "Provider 导入至少需要一个候选",
        ));
    }
    let preview = crate::db::provider_imports::get_preview(database, &input.preview_id)?;
    if preview.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &preview.id,
            &preview.status,
        ));
    }
    // 候选身份只来自持久化证据：前端只回传不透明 candidateId。
    let context: Value = serde_json::from_str(&preview.context_json)
        .map_err(|_| AppError::stale_preview(&preview.id, &preview.target_path))?;
    let mut evidence = BTreeMap::new();
    for candidate in context
        .get("candidates")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let Some(candidate_id) = candidate.get("candidateId").and_then(Value::as_str) else {
            continue;
        };
        // `providerId` 可以为 null：Claude / Codex 官方登录没有原生 provider key，
        // 此时用 `None` 参与匹配，不能因此丢掉候选身份证据。
        let provider_id = candidate
            .get("providerId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        evidence.insert(candidate_id.to_owned(), provider_id);
    }
    if evidence.is_empty() {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let mut requested = BTreeSet::new();
    let mut names = BTreeSet::new();
    for item in &input.items {
        ArtifactName::parse(item.name.clone())?;
        if !requested.insert(item.candidate_id.clone()) {
            return Err(AppError::invalid_input("items", "候选不能重复选择"));
        }
        if !names.insert(item.name.to_lowercase()) {
            return Err(AppError::conflict(
                "profile",
                "同一批次内的导入名称必须互不相同",
            ));
        }
        if !evidence.contains_key(&item.candidate_id) {
            return Err(AppError::stale_preview(&preview.id, &preview.target_path));
        }
    }
    // 重新扫描原生文件：目标与 hash 任一变化都属过期预览。
    let candidates = provider_candidates(database, environment, preview.tool)?;
    if candidates.is_empty()
        || candidates.iter().any(|candidate| {
            candidate.discovered.target_path != preview.target_path
                || candidate.discovered.full_hash != preview.observed_full_hash
        })
    {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let find_candidate = |provider_id: Option<&str>| {
        candidates
            .iter()
            .find(|candidate| candidate.discovered.provider_id.as_deref() == provider_id)
    };

    let mut records = Vec::with_capacity(input.items.len());
    let mut batch_projection = Value::Object(Map::new());
    // 默认渠道（Pi 的 `settings.json.defaultProvider`）优先成为生效档案；
    // 没有默认渠道时取本次选择的第一个。唯一生效约束由数据库保证。
    let active_index = input
        .items
        .iter()
        .position(|item| {
            evidence.get(&item.candidate_id).is_some_and(|provider_id| {
                find_candidate(provider_id.as_deref())
                    .is_some_and(|candidate| candidate.discovered.is_default_provider)
            })
        })
        .unwrap_or(0);
    for (index, item) in input.items.iter().enumerate() {
        let provider_id = evidence
            .get(&item.candidate_id)
            .ok_or_else(|| AppError::stale_preview(&preview.id, &preview.target_path))?;
        let candidate = find_candidate(provider_id.as_deref())
            .ok_or_else(|| AppError::stale_preview(&preview.id, &preview.target_path))?;
        match candidate.status {
            ProviderImportCandidateStatus::Importable => {}
            ProviderImportCandidateStatus::AlreadyManaged => {
                return Err(AppError::conflict(
                    "import",
                    "该渠道已有中央档案，不能重复导入",
                ));
            }
            ProviderImportCandidateStatus::Invalid => {
                return Err(AppError::invalid_input(
                    "providerImport",
                    provider_import_reason(
                        candidate
                            .reason
                            .as_deref()
                            .unwrap_or(crate::adapters::pi::PI_PROVIDER_FIELDS_INVALID),
                    ),
                ));
            }
        }
        let discovered = &candidate.discovered;
        let id = Uuid::new_v4().to_string();
        let stable_provider_id = discovered
            .provider_id
            .clone()
            .unwrap_or_else(|| generated_codex_provider_id(&id));
        validate_codex_provider_id(preview.tool, &stable_provider_id)?;
        let auth_kind = discovered_auth_kind(discovered)?;
        validate_provider_fields(&ProviderFieldsInput {
            tool: preview.tool,
            auth_kind,
            name: &item.name,
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
            &stable_provider_id,
            options,
            discovered.extra_provider_fields.clone(),
        )?;
        if let Some(api_key) = &discovered.api_key {
            redactor.register_secret(api_key.clone());
        }
        merge_json_objects(&mut batch_projection, &discovered.projection);
        records.push(NewProviderProfileRecord {
            id,
            tool: preview.tool,
            name: item.name.clone(),
            api_base_url: optional_text(&discovered.api_base_url),
            api_key: discovered.api_key.clone(),
            default_model: optional_text(&discovered.default_model),
            config_json: serde_json::to_string(&config).map_err(|error| {
                AppError::invalid_input("providerOptions", "导入 Provider 选项无法序列化")
                    .with_source_redacted(error, redactor)
            })?,
            is_active: index == active_index,
        });
    }

    // 目标级基线必须是「本批次 + 既有受管 provider」的并集，否则分次导入会覆盖
    // 前一批基线，并让未被导入的 provider 被当成「受管但缺失」而删除。
    let mut descriptor = descriptor_for(environment, preview.tool, ArtifactKind::Provider)?;
    refine_claude_provider_policy(&mut descriptor);
    ensure_tool_is_available(&descriptor)?;
    let target = ensure_profile_target(database, &descriptor)?;
    let codec = preview
        .tool
        .adapter()
        .provider_codec()
        .ok_or_else(|| cursor_unsupported(ArtifactKind::Provider))?;
    let baseline_projection =
        codec.merge_import_baseline(target.projection.as_ref(), &batch_projection)?;
    let validate_source = || {
        // 取写锁可能等待其他连接：提交前重新确认原生文件仍是同一份内容。
        let fresh = discover_native_providers(environment, preview.tool)?;
        if fresh.iter().any(|discovered| {
            discovered.target_path == preview.target_path
                && discovered.full_hash == preview.observed_full_hash
        }) {
            Ok(())
        } else {
            Err(AppError::stale_preview(&preview.id, &preview.target_path))
        }
    };
    let imported = crate::db::provider_imports::adopt_imported_providers(
        database,
        &preview,
        &records,
        &baseline_projection,
        validate_source,
    )?;
    Ok(ProviderImportResultDto {
        tool: preview.tool,
        imported_count: u32::try_from(imported.len()).unwrap_or(u32::MAX),
    })
}
