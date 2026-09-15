// Provider 检测与导入：候选枚举、可导入性判定与批量接管。
//
// 本文件由 `service.rs` 用 `include!` 引入，因此不能用模块级 `//!` 文档注释。

use std::collections::BTreeSet;

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

fn discover_native_providers(
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Vec<DiscoveredProvider>, AppError> {
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
        TargetScan::Missing => return Ok(Vec::new()),
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

/// 过渡期的单候选选择已由批量候选取代；保留这段注释是提醒后来者：
/// 不要再引入「只取第一个」的捷径，否则 Pi 的多 provider 文件会再次静默丢条目。
///
/// 检测阶段的候选：适配层的原生事实 + 服务层的可导入性判定。
pub(crate) struct ProviderCandidate {
    pub candidate_id: String,
    pub discovered: DiscoveredProvider,
    pub status: ProviderImportCandidateStatus,
    pub reason: Option<String>,
    pub suggested_name: String,
}

/// Pi 的 `models.json` 是多 provider 文件，因此只有 “该工具尚无中央渠道档案” 不再
/// 是导入的前置条件：存量档案按 `provider_id` 逐候选去重，其余 provider 仍可导入。
/// 其他工具的目标文件只承载一份档案，沿用首次接管守卫。
fn supports_incremental_provider_import(tool: Tool) -> bool {
    tool == Tool::Pi
}

fn provider_candidate_name(discovered: &DiscoveredProvider) -> String {
    discovered
        .suggested_name
        .as_ref()
        .filter(|name| ArtifactName::parse((*name).clone()).is_ok())
        .cloned()
        .unwrap_or_else(|| "已导入渠道".to_owned())
}

/// 已有中央档案占用的原生 provider id（仅 Pi 需要；其他工具由守卫整体拦下）。
fn managed_provider_ids(records: &[ProviderProfileRecord]) -> Result<BTreeSet<String>, AppError> {
    let mut ids = BTreeSet::new();
    for record in records {
        if let Some(provider_id) = parse_stored_provider_config(record)?.provider_id {
            ids.insert(provider_id);
        }
    }
    Ok(ids)
}

fn provider_candidate_status(
    tool: Tool,
    discovered: &DiscoveredProvider,
    managed_ids: &BTreeSet<String>,
) -> (ProviderImportCandidateStatus, Option<String>) {
    if let Some(reason) = discovered.unimportable_reason.clone() {
        return (ProviderImportCandidateStatus::Invalid, Some(reason));
    }
    if discovered
        .provider_id
        .as_deref()
        .is_some_and(|provider_id| managed_ids.contains(provider_id))
    {
        return (ProviderImportCandidateStatus::AlreadyManaged, None);
    }
    let invalid = || {
        (
            ProviderImportCandidateStatus::Invalid,
            Some(crate::adapters::pi::PI_PROVIDER_FIELDS_INVALID.to_owned()),
        )
    };
    let Ok(auth_kind) = discovered_auth_kind(discovered) else {
        return invalid();
    };
    if validate_provider_fields(&ProviderFieldsInput {
        tool,
        auth_kind,
        name: &provider_candidate_name(discovered),
        api_base_url: &discovered.api_base_url,
        api_key: discovered.api_key.as_deref(),
        default_model: &discovered.default_model,
    })
    .is_err()
    {
        return invalid();
    }
    if validate_discovered_provider_config(tool, discovered).is_err() {
        return invalid();
    }
    (ProviderImportCandidateStatus::Importable, None)
}

/// 全部候选（含不可导入的），顺序与适配层返回的原生顺序一致。
fn provider_candidates(
    database: &Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Vec<ProviderCandidate>, AppError> {
    let records = repository::list_provider_profiles(database, tool)?;
    if !supports_incremental_provider_import(tool) && !records.is_empty() {
        return Ok(Vec::new());
    }
    let managed_ids = managed_provider_ids(&records)?;
    let mut candidates = Vec::new();
    for discovered in discover_native_providers(environment, tool)? {
        let (status, reason) = provider_candidate_status(tool, &discovered, &managed_ids);
        candidates.push(ProviderCandidate {
            candidate_id: Uuid::new_v4().to_string(),
            suggested_name: provider_candidate_name(&discovered),
            discovered,
            status,
            reason,
        });
    }
    Ok(candidates)
}

/// 把多个候选的原生投影合成本批次的目标级基线。
///
/// 条目级合并：对象逐键递归，其他类型直接替换。单候选工具只有一个候选，
/// 合并结果就是它自己的投影。
fn merge_json_objects(base: &mut Value, addition: &Value) {
    match (base, addition) {
        (Value::Object(base), Value::Object(addition)) => {
            for (key, value) in addition {
                merge_json_objects(base.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (base, addition) => *base = addition.clone(),
    }
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
