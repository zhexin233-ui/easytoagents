// Provider「按原生内容接管」：把漂移的原生条目写回中央档案并同时刷新基线。
//
// 与 `readopt_provider_target` 的区别：重新接管只刷新目标基线，档案保持旧内容，
// 因此下一次 Apply 会把用户手改的原生内容改回去。本命令让「文件怎样就以文件为准」，
// 供用户在同步 Preview 的冲突态里一键收敛。

/// 原生条目与中央档案的配对。
///
/// 有稳定原生 key 时按 key 配对；没有 key 的 codec（Claude）只在唯一配对时
/// 匹配，避免把另一份渠道的内容写进错误的档案。
fn match_native_provider<'a>(
    profiles: &[ProviderProfileRecord],
    native: &'a [DiscoveredProvider],
    provider_id: Option<&str>,
) -> Option<&'a DiscoveredProvider> {
    if let Some(provider_id) = provider_id {
        // 一旦中央档案已有稳定 provider id，就不能因为当前恰好只有一份
        // 档案/原生条目而把不同 id 猜成同一渠道；这类重绑必须走应用内匹配/导入。
        return native
            .iter()
            .find(|entry| entry.provider_id.as_deref() == Some(provider_id));
    }
    if profiles.len() == 1 && native.len() == 1 {
        return native.first();
    }
    None
}

/// 把原生 Provider 内容采纳为中央渠道档案的权威内容。
///
/// 只处理**已经漂移**的渠道：档案投影与原生条目一致的渠道不会被改写，也不会
/// 出现在结果里。采纳 `apiKey` 时与方法导入一致：明文入库、`$ENV` 引用原样保留，
/// 绝不展开或执行。
pub fn adopt_provider_native(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: AdoptProviderNativeInput,
) -> Result<AdoptProviderNativeResultDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Provider)?;
    if input.target_id.trim().is_empty() {
        return Err(AppError::invalid_input(
            "targetId",
            "Provider 接管缺少受管目标身份",
        ));
    }
    if input.target_path.trim().is_empty() {
        return Err(AppError::invalid_input(
            "targetPath",
            "Provider 接管缺少目标路径",
        ));
    }
    let mut descriptor = descriptor_for(environment, input.tool, ArtifactKind::Provider)?;
    if descriptor_path(&descriptor)? != input.target_path {
        // 与重新接管同一身份合同：不接受旧页面或跨工具的目标路径。
        return Err(AppError::invalid_input(
            "targetPath",
            "Provider 目标路径与当前工具目标不一致",
        ));
    }
    refine_claude_provider_policy(&mut descriptor);
    ensure_tool_is_available(&descriptor)?;

    let native = discover_native_providers(environment, input.tool)?;
    if native.is_empty() {
        // 原生文件没有可证明的渠道条目：不猜测、不清空，直接报告无漂移可接管。
        return Ok(AdoptProviderNativeResultDto {
            tool: input.tool,
            adopted: Vec::new(),
            affected_sync_scopes: Some(Vec::new()),
        });
    }
    let observed_full_hash = native
        .first()
        .map(|entry| entry.full_hash.clone())
        .unwrap_or_default();
    if let Some(expected) = input.observed_full_hash.as_deref() {
        if expected != observed_full_hash {
            return Err(AppError::stale_preview(
                "externalChangePlan",
                "Provider 原生目标在计划生成后发生了变化",
            ));
        }
    }
    let profiles = repository::list_provider_profiles(database, input.tool)?;

    let mut adoptions = Vec::new();
    let mut adopted_projection = BTreeMap::new();
    let mut adopted_names = Vec::new();
    for profile in &profiles {
        let config = parse_stored_provider_config(profile)?;
        let Some(entry) = match_native_provider(&profiles, &native, config.provider_id.as_deref())
        else {
            continue;
        };
        if provider_projection(profile)? == entry.projection {
            continue;
        }
        if let Some(reason) = entry.unimportable_reason.as_deref() {
            return Err(AppError::invalid_input(
                "providerAdopt",
                provider_import_reason(reason),
            ));
        }
        let auth_kind = discovered_auth_kind(entry)?;
        validate_provider_fields(&ProviderFieldsInput {
            tool: input.tool,
            auth_kind,
            name: &profile.name,
            api_base_url: &entry.api_base_url,
            api_key: entry.api_key.as_deref(),
            default_model: &entry.default_model,
        })?;
        let options_input = ProviderCodecInput {
            auth_kind: &entry.auth_kind,
            credential_env_key: Some(entry.credential_env_key.as_str()),
            extra_env: &entry.extra_env,
            wire_api: entry.wire_api.as_deref(),
            zcode_kind: entry.zcode_kind.as_deref().or(Some("anthropic")),
            opencode_npm: entry.opencode_npm.as_deref(),
            opencode_api: entry.opencode_api.as_deref(),
        };
        let options = provider_options_from_codec(input.tool, &options_input)?;
        let provider_id = config
            .provider_id
            .clone()
            .unwrap_or_else(|| generated_codex_provider_id(&profile.id));
        let updated = StoredProviderConfig::from_input(
            input.tool,
            &provider_id,
            options,
            entry.extra_provider_fields.clone(),
        )?;
        if let Some(api_key) = &entry.api_key {
            redactor.register_secret(api_key.clone());
        }
        adoptions.push(NativeProviderAdoption {
            id: profile.id.clone(),
            row_version: profile.row_version,
            api_base_url: optional_text(&entry.api_base_url),
            api_key: entry.api_key.clone(),
            default_model: optional_text(&entry.default_model),
            config_json: serde_json::to_string(&updated).map_err(|error| {
                AppError::invalid_input("providerOptions", "接管渠道选项无法序列化")
                    .with_source_redacted(error, redactor)
            })?,
        });
        adopted_projection.insert(profile.id.clone(), entry.projection.clone());
        adopted_names.push(profile.name.clone());
    }
    if adoptions.is_empty() {
        return Ok(AdoptProviderNativeResultDto {
            tool: input.tool,
            adopted: Vec::new(),
            affected_sync_scopes: Some(Vec::new()),
        });
    }

    // 基线取全部中央渠道投影的并集（已接管的用原生投影），与下一次 Preview 的
    // ownership 口径一致；否则刚接管完就会出现新的伪造漂移。
    let mut baseline = Value::Object(Map::new());
    for profile in &profiles {
        match adopted_projection.get(&profile.id) {
            Some(projection) => merge_json_objects(&mut baseline, projection),
            None => merge_json_objects(&mut baseline, &provider_projection(profile)?),
        }
    }
    // 只采纳预览绑定的渠道行版本；缺失或过期都由数据库层拒绝。
    let expected_versions: BTreeMap<String, u32> = input
        .row_versions
        .iter()
        .filter(|row| row.entity_type == DatabaseEntityType::ProviderProfile)
        .map(|row| (row.entity_id.clone(), row.row_version))
        .collect();
    crate::db::provider_imports::adopt_native_providers(
        database,
        crate::db::provider_imports::NativeProviderAdoptionRequest {
            tool: input.tool,
            target_path: &input.target_path,
            observed_full_hash: &observed_full_hash,
            baseline_projection: &baseline,
            adoptions: &adoptions,
            expected_versions: &expected_versions,
            current_run_id: input.preview_id.as_deref(),
            target_id: &input.target_id,
            target_row_version: input.target_row_version,
        },
    )?;
    Ok(AdoptProviderNativeResultDto {
        tool: input.tool,
        adopted: adopted_names,
        affected_sync_scopes: Some(vec![SyncScopeDto::global(
            ArtifactKind::Provider,
            input.tool,
        )]),
    })
}
