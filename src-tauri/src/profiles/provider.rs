/// Provider 原生 projection 由对应 Adapter 的 codec 负责；Profiles 只把中央
/// 记录转换成不泄漏 Profiles 内部 DTO 的最小公共输入。
fn provider_projection(profile: &ProviderProfileRecord) -> Result<Value, AppError> {
    let config = parse_stored_provider_config(profile)?;
    let input = crate::adapters::ProviderCodecProfileInput {
        name: &profile.name,
        auth_kind: config.effective_auth_kind(profile.tool).as_str(),
        api_base_url: profile.api_base_url.as_deref(),
        api_key: profile.api_key.as_deref(),
        default_model: profile.default_model.as_deref(),
        credential_env_key: config.credential_env_key.map(|value| value.as_str()),
        extra_env: &config.extra_env,
        provider_id: config.provider_id.as_deref(),
        wire_api: config.wire_api.as_deref(),
        zcode_kind: config.zcode_kind.as_deref(),
        opencode_npm: config.opencode_npm.as_deref(),
        opencode_api: config.opencode_api.as_deref(),
        extra_provider_fields: &config.extra_provider_fields,
    };
    profile
        .tool
        .adapter()
        .provider_codec()
        .ok_or_else(|| cursor_unsupported(ArtifactKind::Provider))?
        .render(&input)
}

fn provider_ownership(
    tool: Tool,
    baseline: Option<&Value>,
    desired: &Value,
) -> Result<ManagedOwnership, AppError> {
    tool.adapter()
        .provider_codec()
        .ok_or_else(|| cursor_unsupported(ArtifactKind::Provider))?
        .ownership(baseline, desired)
}

/// 目标级同步意图（desired 投影 + 参与写回的渠道 + 行版本）。
struct ProviderSyncIntent {
    /// 当前生效渠道；没有生效渠道但已有基线时允许为空（清空语义）。
    active: Option<ProviderProfileRecord>,
    desired_projection: Value,
    row_versions: Vec<DatabaseRowVersion>,
    /// 本次意图会写回的中央渠道。
    profiles: Vec<ProviderProfileRecord>,
}

/// 单 provider 工具的目标文件只承载一份档案：desired 就是当前生效档案。
/// Pi 的 `models.json` 是多 provider 文件：desired 必须是**全部**中央 Pi 渠道的
/// 并集，否则 Apply 会把未生效渠道的原生条目删空（并让基线校验与写入不一致）。
fn provider_sync_intent(database: &Database, tool: Tool) -> Result<ProviderSyncIntent, AppError> {
    let active = repository::find_active_provider_profile(database, tool)?;
    if !supports_incremental_provider_import(tool) {
        let desired_projection = active
            .as_ref()
            .map(provider_projection)
            .transpose()?
            .unwrap_or_else(|| Value::Object(Map::new()));
        let row_versions = active
            .as_ref()
            .map(provider_row_version)
            .transpose()?
            .into_iter()
            .collect();
        let profiles = active.iter().cloned().collect();
        return Ok(ProviderSyncIntent {
            active,
            desired_projection,
            row_versions,
            profiles,
        });
    }
    let profiles = repository::list_provider_profiles(database, tool)?;
    let mut desired_projection = Value::Object(Map::new());
    let mut row_versions = Vec::with_capacity(profiles.len());
    for profile in &profiles {
        merge_json_objects(&mut desired_projection, &provider_projection(profile)?);
        row_versions.push(provider_row_version(profile)?);
    }
    Ok(ProviderSyncIntent {
        active,
        desired_projection,
        row_versions,
        profiles,
    })
}
