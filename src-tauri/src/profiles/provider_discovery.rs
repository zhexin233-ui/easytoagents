type DiscoveredProvider = crate::adapters::ProviderCodecDiscovery;

/// 适配层用稳定字符串表达认证方式；未知值说明 codec 合同不一致，直接失败。
fn discovered_auth_kind(discovered: &DiscoveredProvider) -> Result<ProviderAuthKind, AppError> {
    ProviderAuthKind::from_stable_str(&discovered.auth_kind).ok_or_else(|| {
        AppError::invalid_input("providerOptions", "Provider 发现结果的认证方式无效")
    })
}

fn validate_discovered_provider_config(
    tool: Tool,
    discovered: &DiscoveredProvider,
) -> Result<(), AppError> {
    let options_input = ProviderCodecInput {
        auth_kind: &discovered.auth_kind,
        credential_env_key: Some(discovered.credential_env_key.as_str()),
        extra_env: &discovered.extra_env,
        wire_api: discovered.wire_api.as_deref(),
        zcode_kind: discovered.zcode_kind.as_deref(),
        opencode_npm: discovered.opencode_npm.as_deref(),
        opencode_api: discovered.opencode_api.as_deref(),
    };
    let options = provider_options_from_codec(tool, &options_input)?;
    StoredProviderConfig::from_input(
        tool,
        discovered.provider_id.as_deref().unwrap_or("discovered"),
        options,
        discovered.extra_provider_fields.clone(),
    )?;
    Ok(())
}

fn discover_native_provider(
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Option<DiscoveredProvider>, AppError> {
    let mut descriptor = descriptor_for(environment, tool, ArtifactKind::Provider)?;
    refine_claude_provider_policy(&mut descriptor);
    ensure_tool_is_available(&descriptor)?;
    if descriptor.policy != crate::adapters::PolicyState::Allowed {
        return Err(AppError::policy_blocked(
            "claude",
            descriptor.path.as_deref().unwrap_or("<unsupported>"),
            match descriptor.policy {
                crate::adapters::PolicyState::Blocked => "provider_managed_by_host",
                crate::adapters::PolicyState::Unknown => "provider_policy_unknown",
                crate::adapters::PolicyState::Allowed => {
                    return Err(AppError::internal("allowed 策略不应进入阻断分支"))
                }
            },
        ));
    }
    let codec = tool
        .adapter()
        .provider_codec()
        .ok_or_else(|| cursor_unsupported(ArtifactKind::Provider))?;
    let broad_ownership = codec.discovery_ownership()?;
    let scan = scan_target(tool.adapter(), &descriptor, &broad_ownership);
    let observed = match scan {
        TargetScan::Observed(observed) => observed,
        TargetScan::Missing => return Ok(None),
        TargetScan::ParseError => {
            return Err(AppError::parse(
                &descriptor_path(&descriptor)?,
                descriptor.format.as_str(),
            ));
        }
        _ => return Err(scan_error(&descriptor, &scan)),
    };
    codec.discover(
        &descriptor,
        &observed.managed_projection,
        &observed.full_hash,
    )
}

fn ensure_tool_is_available(descriptor: &TargetDescriptor) -> Result<(), AppError> {
    match descriptor.capability.state {
        crate::adapters::CapabilityState::Supported => Ok(()),
        crate::adapters::CapabilityState::ToolNotInstalled => Err(AppError::not_found(
            "toolInstallation",
            descriptor.tool.as_str(),
        )),
        crate::adapters::CapabilityState::Unsupported => Err(AppError::invalid_input(
            "capability",
            match descriptor.capability.diagnostic_code.as_deref() {
                Some("CURSOR_PROVIDER_UNSUPPORTED") => "CURSOR_PROVIDER_UNSUPPORTED",
                Some("OPENCODE_CONFIG_CONTENT_OVERRIDE") => "OPENCODE_CONFIG_CONTENT_OVERRIDE",
                Some("OPENCODE_DISCOVERY_DISABLED") => "OPENCODE_DISCOVERY_DISABLED",
                Some("OPENCODE_INSTALLATION_PROBE_UNSUPPORTED") => {
                    "OPENCODE_INSTALLATION_PROBE_UNSUPPORTED"
                }
                _ => "工具安装探针未能安全确认版本",
            },
        )),
    }
}
