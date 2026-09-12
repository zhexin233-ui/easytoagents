//! Claude 配置格式、目标矩阵与管理策略发现。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde_json::{json, Map, Value};

use crate::{
    adapters::{
        descriptor_path, path_text, ClaudeCustomizationPolicyProbeInput, ClaudeUserMcpProbeInput,
        ClaudeUserMcpProbeResult, DiscoveryContext, ManagedOwnership, ProviderCodec,
        ProviderCodecDiscovery, ProviderCodecInput, ProviderCodecOptions,
        ProviderCodecProfileInput, SymlinkPolicy, TargetCapability, TargetDescriptor, TargetFormat,
        ToolAdapter, PROVIDER_AUTH_KIND_API_KEY, PROVIDER_AUTH_KIND_OFFICIAL_LOGIN,
    },
    domain::{ArtifactKind, Scope, Tool},
    error::AppError,
    security::env_entry_is_manageable,
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
            TargetDescriptor::builder(Tool::Claude, ArtifactKind::Provider, Scope::Global)
                .path(Some(path_text(&settings_path)?))
                .format(TargetFormat::Json)
                .managed_selectors(["env"])
                .sensitive_selectors(["env"])
                .capability(tool_capability.clone())
                .policy(environment.claude_provider_policy())
                .build(),
            TargetDescriptor::builder(Tool::Claude, ArtifactKind::Prompt, Scope::Global)
                .path(Some(path_text(
                    &environment.claude_config_dir().join("CLAUDE.md"),
                )?))
                .format(TargetFormat::Markdown)
                .managed_selectors(["$document"])
                .capability(tool_capability.clone())
                .build(),
            TargetDescriptor::builder(Tool::Claude, ArtifactKind::Mcp, Scope::Global)
                .path(user_mcp_path)
                .format(TargetFormat::Json)
                .managed_selectors(["mcpServers"])
                .sensitive_selectors(["mcpServers/*/headers", "mcpServers/*/env"])
                .capability(user_mcp_capability)
                .policy(customization_policy.mcp)
                .build(),
            TargetDescriptor::builder(Tool::Claude, ArtifactKind::Skill, Scope::Global)
                .path(Some(path_text(
                    &environment.claude_config_dir().join("skills"),
                )?))
                .format(TargetFormat::SymlinkDirectory)
                .managed_selectors(["$children"])
                .capability(tool_capability.clone())
                .policy(customization_policy.skill)
                .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                .build(),
            // Hooks 与 Provider 共享 settings.json（官方 hooks 合同，2026-09-05 核验），
            // 用选择器只接管 `hooks` 子树，保留 `env` 等其他内容。
            // Claude 的 customization policy 只封锁 mcp/skills 自定义文件，
            // 不封锁核心 settings.json 合同，因此这里不做策略门禁。
            TargetDescriptor::builder(Tool::Claude, ArtifactKind::Hook, Scope::Global)
                .path(Some(path_text(&settings_path)?))
                .format(TargetFormat::Json)
                .managed_selectors(["hooks"])
                .capability(tool_capability.clone())
                .build(),
            // Agents（子代理）目录（官方 "Create custom subagents" 合同，
            // 2026-09-12 核验）：目录 descriptor 指向 `<claude_config_dir>/agents`，
            // 受管文件为目录内的 `<name>.md`。官方 strictPluginOnlyCustomization
            // 会同时封锁本地自定义 agents，因此沿用 skill 的同一策略证据门禁。
            TargetDescriptor::builder(Tool::Claude, ArtifactKind::Agent, Scope::Global)
                .path(Some(path_text(
                    &environment.claude_config_dir().join("agents"),
                )?))
                .format(TargetFormat::Markdown)
                .capability(tool_capability.clone())
                .policy(customization_policy.skill)
                .build(),
        ];

        if let Some(project_root) = context.project_root {
            let root = Path::new(project_root.as_str());
            let project_root = Some(project_root.as_str().to_owned());
            targets.extend([
                TargetDescriptor::builder(Tool::Claude, ArtifactKind::Mcp, Scope::Project)
                    .project_root(project_root.clone())
                    .path(Some(path_text(&root.join(".mcp.json"))?))
                    .format(TargetFormat::Json)
                    .managed_selectors(["mcpServers"])
                    .sensitive_selectors(["mcpServers/*/headers", "mcpServers/*/env"])
                    .capability(tool_capability.clone())
                    .policy(customization_policy.mcp)
                    .build(),
                TargetDescriptor::builder(Tool::Claude, ArtifactKind::Skill, Scope::Project)
                    .project_root(project_root.clone())
                    .path(Some(path_text(&root.join(".claude/skills"))?))
                    .format(TargetFormat::SymlinkDirectory)
                    .managed_selectors(["$children"])
                    .capability(tool_capability.clone())
                    .policy(customization_policy.skill)
                    .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                    .build(),
                TargetDescriptor::builder(Tool::Claude, ArtifactKind::Hook, Scope::Project)
                    .project_root(project_root.clone())
                    .path(Some(path_text(&root.join(".claude/settings.json"))?))
                    .format(TargetFormat::Json)
                    .managed_selectors(["hooks"])
                    .capability(tool_capability.clone())
                    .build(),
                // 项目级子代理目录：`<root>/.claude/agents`；与全局同样受
                // strictPluginOnlyCustomization 策略门禁。
                TargetDescriptor::builder(Tool::Claude, ArtifactKind::Agent, Scope::Project)
                    .project_root(project_root)
                    .path(Some(path_text(&root.join(".claude/agents"))?))
                    .format(TargetFormat::Markdown)
                    .capability(tool_capability)
                    .policy(customization_policy.skill)
                    .build(),
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

    /// 始终拥有四个保留键：切换渠道后不能残留上一份手工写入的接入地址或凭据，
    /// 官方登录渠道也正是靠移除这些键回到 Claude Code 自带的账号登录。
    fn ownership(
        &self,
        baseline: Option<&Value>,
        desired: &Value,
    ) -> Result<crate::adapters::ManagedOwnership, AppError> {
        let mut selectors = CLAUDE_RESERVED_ENV_KEYS
            .iter()
            .map(|key| vec!["env".to_owned(), (*key).to_owned()])
            .collect::<BTreeSet<Vec<String>>>();
        for projection in [baseline, Some(desired)].into_iter().flatten() {
            if let Some(env) = projection.get("env").and_then(Value::as_object) {
                selectors.extend(env.keys().map(|key| vec!["env".to_owned(), key.clone()]));
            }
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
        let mut env = Map::new();
        if input.auth_kind != PROVIDER_AUTH_KIND_OFFICIAL_LOGIN {
            if let Some(value) = input.api_base_url.filter(|value| !value.is_empty()) {
                env.insert(
                    CLAUDE_BASE_URL_KEY.to_owned(),
                    Value::String(value.to_owned()),
                );
            }
            if let (Some(key), Some(value)) = (input.credential_env_key, input.api_key) {
                env.insert(key.to_owned(), Value::String(value.to_owned()));
            }
        }
        if let Some(value) = input.default_model.filter(|value| !value.is_empty()) {
            env.insert(CLAUDE_MODEL_KEY.to_owned(), Value::String(value.to_owned()));
        }
        env.extend(
            input
                .extra_env
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone()))),
        );
        Ok(json!({ "env": env }))
    }

    /// 从 `settings.json` 的 `env` 提取渠道事实。
    ///
    /// - 存在接入地址或凭据键 → API Key 渠道；凭据优先取 `ANTHROPIC_AUTH_TOKEN`。
    /// - 三者都不存在 → 官方登录渠道（只接管额外 env 与可选模型）；env 为空则无可导入项。
    /// - 默认模型只来自 `ANTHROPIC_MODEL`，`ANTHROPIC_DEFAULT_*_MODEL` 作为额外 env 原样保留，
    ///   使导入后的首次同步与磁盘内容一致。
    /// - 额外 env 接管所有字符串值的非保留键；疑似凭据或键名不合法的条目只报告键名，
    ///   既不进入档案也不进入受管基线，保持原样不动。
    fn discover(
        &self,
        descriptor: &TargetDescriptor,
        managed_projection: &Value,
        full_hash: &str,
    ) -> Result<Option<ProviderCodecDiscovery>, AppError> {
        let Some(env) = managed_projection.get("env").and_then(Value::as_object) else {
            return Ok(None);
        };
        let text = |key: &str| {
            env.get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        };
        let api_base_url = text(CLAUDE_BASE_URL_KEY);
        let auth_token = text(CLAUDE_AUTH_TOKEN_KEY);
        let api_key = text(CLAUDE_API_KEY_KEY);
        let default_model = text(CLAUDE_MODEL_KEY).unwrap_or_default();

        let mut extra_env = BTreeMap::new();
        let mut skipped_env_keys = Vec::new();
        for (key, value) in env {
            if CLAUDE_RESERVED_ENV_KEYS.contains(&key.as_str()) {
                continue;
            }
            match value.as_str() {
                Some(value) if env_entry_is_manageable(key, value) => {
                    extra_env.insert(key.clone(), value.to_owned());
                }
                _ => skipped_env_keys.push(key.clone()),
            }
        }

        let (auth_kind, credential_env_key, credential) =
            match (api_base_url.is_some(), auth_token, api_key) {
                (_, Some(token), _) => (
                    PROVIDER_AUTH_KIND_API_KEY,
                    CLAUDE_AUTH_TOKEN_KEY,
                    Some(token),
                ),
                (_, None, Some(key)) => (PROVIDER_AUTH_KIND_API_KEY, CLAUDE_API_KEY_KEY, Some(key)),
                (true, None, None) => (PROVIDER_AUTH_KIND_API_KEY, CLAUDE_API_KEY_KEY, None),
                (false, None, None) => {
                    if extra_env.is_empty() && default_model.is_empty() {
                        return Ok(None);
                    }
                    (PROVIDER_AUTH_KIND_OFFICIAL_LOGIN, CLAUDE_API_KEY_KEY, None)
                }
            };
        let mut projection_env = Map::new();
        for key in CLAUDE_RESERVED_ENV_KEYS {
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
            auth_kind: auth_kind.to_owned(),
            api_base_url: api_base_url.unwrap_or_default(),
            api_key: credential,
            default_model,
            credential_env_key: credential_env_key.to_owned(),
            extra_env,
            skipped_env_keys,
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

pub const CLAUDE_BASE_URL_KEY: &str = "ANTHROPIC_BASE_URL";
pub const CLAUDE_MODEL_KEY: &str = "ANTHROPIC_MODEL";
pub const CLAUDE_API_KEY_KEY: &str = "ANTHROPIC_API_KEY";
pub const CLAUDE_AUTH_TOKEN_KEY: &str = "ANTHROPIC_AUTH_TOKEN";

/// 由专用字段承载、渠道同步始终拥有的 env 键；不能出现在额外 env 中。
pub const CLAUDE_RESERVED_ENV_KEYS: [&str; 4] = [
    CLAUDE_BASE_URL_KEY,
    CLAUDE_API_KEY_KEY,
    CLAUDE_AUTH_TOKEN_KEY,
    CLAUDE_MODEL_KEY,
];
