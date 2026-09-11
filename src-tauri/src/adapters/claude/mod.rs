//! Claude 配置格式、目标矩阵与管理策略发现。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde_json::{json, Map, Value};

use crate::{
    adapters::{
        descriptor_path, path_text, ClaudeCustomizationPolicyProbeInput, ClaudeUserMcpProbeInput,
        ClaudeUserMcpProbeResult, DiscoveryContext, ManagedOwnership, PolicyState, ProviderCodec,
        ProviderCodecDiscovery, ProviderCodecInput, ProviderCodecOptions,
        ProviderCodecProfileInput, SymlinkPolicy, TargetCapability, TargetDescriptor, TargetFormat,
        ToolAdapter,
    },
    domain::{ArtifactKind, Scope, Tool},
    error::AppError,
};

#[derive(Debug, Default)]
pub struct ClaudeAdapter;

impl ToolAdapter for ClaudeAdapter {
    fn tool(&self) -> Tool {
        Tool::Claude
    }

    fn provider_codec(&self) -> Option<&dyn ProviderCodec> {
        Some(self)
    }

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let availability = environment.tool_availability(Tool::Claude);
        let installed = availability.is_installed();
        let tool_capability = match availability {
            crate::adapters::ToolAvailabilityState::Installed => TargetCapability::supported(),
            crate::adapters::ToolAvailabilityState::Unavailable => {
                TargetCapability::tool_not_installed()
            }
            crate::adapters::ToolAvailabilityState::Unsupported => {
                TargetCapability::unsupported("CLAUDE_INSTALLATION_PROBE_UNSUPPORTED")
            }
        };
        let settings_path = environment.claude_config_dir().join("settings.json");
        let customization_policy =
            context
                .claude_customization_policy_probe
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: environment.claude_installation_version(),
                    claude_config_dir: environment.claude_config_dir(),
                    source_path: environment.claude_customization_policy_source_path(),
                    tool_installed: installed,
                });

        let user_mcp_probe = if !installed {
            ClaudeUserMcpProbeResult::ToolNotInstalled
        } else if environment.uses_default_claude_config_dir() {
            ClaudeUserMcpProbeResult::Supported(environment.home().join(".claude.json"))
        } else if environment.claude_installation_version().is_some() {
            context
                .claude_user_mcp_probe
                .probe(&ClaudeUserMcpProbeInput {
                    home: environment.home(),
                    claude_config_dir: environment.claude_config_dir(),
                    uses_default_config_dir: environment.uses_default_claude_config_dir(),
                    installation_version: environment.claude_installation_version(),
                    tool_installed: installed,
                })
        } else {
            ClaudeUserMcpProbeResult::Unsupported("CLAUDE_INSTALLATION_VERSION_UNKNOWN")
        };
        let (user_mcp_path, user_mcp_capability) = match user_mcp_probe {
            ClaudeUserMcpProbeResult::Supported(path) => {
                (Some(path_text(&path)?), TargetCapability::supported())
            }
            ClaudeUserMcpProbeResult::Unsupported(code) => {
                (None, TargetCapability::unsupported(code))
            }
            ClaudeUserMcpProbeResult::ToolNotInstalled => {
                (None, TargetCapability::tool_not_installed())
            }
        };

        let mut targets = vec![
            descriptor(
                ArtifactKind::Provider,
                Scope::Global,
                None,
                Some(path_text(&settings_path)?),
                TargetFormat::Json,
                vec!["env"],
                vec!["env"],
                tool_capability.clone(),
                environment.claude_provider_policy(),
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Prompt,
                Scope::Global,
                None,
                Some(path_text(
                    &environment.claude_config_dir().join("CLAUDE.md"),
                )?),
                TargetFormat::Markdown,
                vec!["$document"],
                vec![],
                tool_capability.clone(),
                PolicyState::Allowed,
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Mcp,
                Scope::Global,
                None,
                user_mcp_path,
                TargetFormat::Json,
                vec!["mcpServers"],
                vec!["mcpServers/*/headers", "mcpServers/*/env"],
                user_mcp_capability,
                customization_policy.mcp,
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Skill,
                Scope::Global,
                None,
                Some(path_text(&environment.claude_config_dir().join("skills"))?),
                TargetFormat::SymlinkDirectory,
                vec!["$children"],
                vec![],
                tool_capability.clone(),
                customization_policy.skill,
                SymlinkPolicy::ManagedChildrenOnly,
            ),
            // Hooks 与 Provider 共享 settings.json（官方 hooks 合同，2026-09-05 核验），
            // 用选择器只接管 `hooks` 子树，保留 `env` 等其他内容。
            // Claude 的 customization policy 只封锁 mcp/skills 自定义文件，
            // 不封锁核心 settings.json 合同，因此这里不做策略门禁。
            descriptor(
                ArtifactKind::Hook,
                Scope::Global,
                None,
                Some(path_text(&settings_path)?),
                TargetFormat::Json,
                vec!["hooks"],
                vec![],
                tool_capability.clone(),
                PolicyState::Allowed,
                SymlinkPolicy::Reject,
            ),
        ];

        if let Some(project_root) = context.project_root {
            let root = Path::new(project_root.as_str());
            targets.extend([
                descriptor(
                    ArtifactKind::Mcp,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(path_text(&root.join(".mcp.json"))?),
                    TargetFormat::Json,
                    vec!["mcpServers"],
                    vec!["mcpServers/*/headers", "mcpServers/*/env"],
                    tool_capability.clone(),
                    customization_policy.mcp,
                    SymlinkPolicy::Reject,
                ),
                descriptor(
                    ArtifactKind::Skill,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(path_text(&root.join(".claude/skills"))?),
                    TargetFormat::SymlinkDirectory,
                    vec!["$children"],
                    vec![],
                    tool_capability.clone(),
                    customization_policy.skill,
                    SymlinkPolicy::ManagedChildrenOnly,
                ),
                descriptor(
                    ArtifactKind::Hook,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(path_text(&root.join(".claude/settings.json"))?),
                    TargetFormat::Json,
                    vec!["hooks"],
                    vec![],
                    tool_capability,
                    PolicyState::Allowed,
                    SymlinkPolicy::Reject,
                ),
            ]);
        }

        crate::adapters::populate_descriptor_allowed_roots(environment, &mut targets)?;
        Ok(targets)
    }
}

impl ProviderCodec for ClaudeAdapter {
    fn discovery_ownership(&self) -> Result<crate::adapters::ManagedOwnership, AppError> {
        Ok(crate::adapters::ManagedOwnership::selectors([["env"]]))
    }

    fn ownership(
        &self,
        baseline: Option<&Value>,
        desired: &Value,
    ) -> Result<crate::adapters::ManagedOwnership, AppError> {
        let mut selectors = BTreeSet::<Vec<String>>::new();
        for projection in [baseline, Some(desired)].into_iter().flatten() {
            if let Some(env) = projection.get("env").and_then(Value::as_object) {
                selectors.extend(env.keys().map(|key| vec!["env".to_owned(), key.clone()]));
            }
        }
        if selectors.is_empty() {
            return Err(AppError::invalid_input(
                "managedOwnership",
                "Provider 同步没有可证明拥有的字段",
            ));
        }
        Ok(ManagedOwnership::Selectors(selectors.into_iter().collect()))
    }

    fn default_options(
        &self,
        input: &ProviderCodecInput<'_>,
    ) -> Result<ProviderCodecOptions, AppError> {
        Ok(ProviderCodecOptions {
            credential_env_key: input.credential_env_key.map(str::to_owned),
            extra_env: input.extra_env.clone(),
            ..ProviderCodecOptions::default()
        })
    }

    fn render(&self, input: &ProviderCodecProfileInput<'_>) -> Result<Value, AppError> {
        const BASE_URL_KEY: &str = "ANTHROPIC_BASE_URL";
        const MODEL_KEY: &str = "ANTHROPIC_MODEL";

        let mut env = Map::new();
        if let Some(value) = input.api_base_url {
            env.insert(BASE_URL_KEY.to_owned(), Value::String(value.to_owned()));
        }
        if let (Some(key), Some(value)) = (input.credential_env_key, input.api_key) {
            env.insert(key.to_owned(), Value::String(value.to_owned()));
        }
        if let Some(value) = input.default_model {
            env.insert(MODEL_KEY.to_owned(), Value::String(value.to_owned()));
        }
        env.extend(
            input
                .extra_env
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone()))),
        );
        Ok(json!({ "env": env }))
    }

    fn discover(
        &self,
        descriptor: &TargetDescriptor,
        managed_projection: &Value,
        full_hash: &str,
    ) -> Result<Option<ProviderCodecDiscovery>, AppError> {
        const BASE_URL_KEY: &str = "ANTHROPIC_BASE_URL";
        const MODEL_KEY: &str = "ANTHROPIC_MODEL";
        const DEFAULT_MODEL_KEYS: &[&str] = &[
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
        ];
        const API_KEY: &str = "ANTHROPIC_API_KEY";
        const AUTH_TOKEN: &str = "ANTHROPIC_AUTH_TOKEN";

        let Some(env) = managed_projection.get("env").and_then(Value::as_object) else {
            return Ok(None);
        };
        let api_base_url = env
            .get(BASE_URL_KEY)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let default_model = std::iter::once(MODEL_KEY)
            .chain(DEFAULT_MODEL_KEYS.iter().copied())
            .find_map(|key| {
                env.get(key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_owned)
            });
        let Some(default_model) = default_model else {
            return Ok(None);
        };
        if api_base_url.is_empty() {
            return Ok(None);
        }
        let auth_token = env.get(AUTH_TOKEN).and_then(Value::as_str);
        let api_key = env.get(API_KEY).and_then(Value::as_str);
        let (credential_env_key, credential) = if let Some(value) = auth_token {
            (AUTH_TOKEN, Some(value.to_owned()))
        } else {
            (API_KEY, api_key.map(str::to_owned))
        };
        let extra_env = env
            .iter()
            .filter(|(key, value)| {
                key.starts_with("ANTHROPIC_")
                    && !matches!(
                        key.as_str(),
                        BASE_URL_KEY | MODEL_KEY | API_KEY | AUTH_TOKEN
                    )
                    && value.is_string()
            })
            .map(|(key, value)| (key.clone(), value.as_str().unwrap_or_default().to_owned()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut projection_env = Map::new();
        for key in [BASE_URL_KEY, MODEL_KEY, API_KEY, AUTH_TOKEN] {
            if let Some(value) = env.get(key) {
                projection_env.insert(key.to_owned(), value.clone());
            }
        }
        for (key, value) in &extra_env {
            projection_env.insert(key.clone(), Value::String(value.clone()));
        }
        Ok(Some(ProviderCodecDiscovery {
            target_path: descriptor_path(descriptor)?,
            full_hash: full_hash.to_owned(),
            projection: json!({ "env": projection_env }),
            api_base_url,
            api_key: credential,
            default_model,
            credential_env_key: credential_env_key.to_owned(),
            extra_env,
            provider_id: None,
            wire_api: None,
            zcode_kind: None,
            opencode_npm: None,
            opencode_api: None,
            extra_provider_fields: BTreeMap::new(),
            suggested_name: None,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
fn descriptor(
    artifact_kind: ArtifactKind,
    scope: Scope,
    project_root: Option<String>,
    path: Option<String>,
    format: TargetFormat,
    managed_selector_roots: Vec<&str>,
    sensitive_selectors: Vec<&str>,
    capability: TargetCapability,
    policy: PolicyState,
    symlink_policy: SymlinkPolicy,
) -> TargetDescriptor {
    TargetDescriptor::builder(Tool::Claude, artifact_kind, scope)
        .project_root(project_root.clone())
        .allowed_root(project_root)
        .path(path)
        .format(format)
        .managed_selectors(managed_selector_roots)
        .sensitive_selectors(sensitive_selectors)
        .capability(capability)
        .policy(policy)
        .symlink_policy(symlink_policy)
        .build()
}
