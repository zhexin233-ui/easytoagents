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
