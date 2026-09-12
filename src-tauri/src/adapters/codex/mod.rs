//! Codex 配置格式、目标矩阵与项目 trust 发现。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde_json::{Map, Value};

use crate::{
    adapters::{
        descriptor_path, path_text, DiscoveryContext, ManagedOwnership, PromptOverrideState,
        ProviderCodec, ProviderCodecDiscovery, ProviderCodecInput, ProviderCodecOptions,
        ProviderCodecProfileInput, SymlinkPolicy, TargetCapability, TargetDescriptor, TargetFormat,
        TargetTrustState, ToolAdapter, PROVIDER_AUTH_KIND_API_KEY,
        PROVIDER_AUTH_KIND_OFFICIAL_LOGIN,
    },
    domain::{ArtifactKind, Scope, Tool},
    error::AppError,
};

#[derive(Debug, Default)]
pub struct CodexAdapter;

impl ToolAdapter for CodexAdapter {
    fn tool(&self) -> Tool {
        Tool::Codex
    }

    fn provider_codec(&self) -> Option<&dyn ProviderCodec> {
        Some(self)
    }

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let availability = environment.tool_availability(Tool::Codex);
        let installed = availability.is_installed();
        let capability = match availability {
            crate::adapters::ToolAvailabilityState::Installed => TargetCapability::supported(),
            crate::adapters::ToolAvailabilityState::Unavailable => {
                TargetCapability::tool_not_installed()
            }
            crate::adapters::ToolAvailabilityState::Unsupported => {
                TargetCapability::unsupported("CODEX_INSTALLATION_PROBE_UNSUPPORTED")
            }
        };
        let config_path = environment.codex_home().join("config.toml");
        let prompt_override = if installed {
            discover_prompt_override(&environment.codex_home().join("AGENTS.override.md"))
        } else {
            PromptOverrideState::Unknown
        };
        let project_trust = context
            .project_root
            .map_or(TargetTrustState::NotRequired, |root| {
                if installed {
                    discover_project_trust(&config_path, root.as_str())
                } else {
                    TargetTrustState::Unknown
                }
            });

        let mut targets = vec![
            TargetDescriptor::builder(Tool::Codex, ArtifactKind::Provider, Scope::Global)
                .path(Some(path_text(&config_path)?))
                .format(TargetFormat::Toml)
                .managed_selectors(["model", "model_provider", "model_providers"])
                .sensitive_selectors(["model_providers/*"])
                .capability(capability.clone())
                .build(),
            TargetDescriptor::builder(Tool::Codex, ArtifactKind::Prompt, Scope::Global)
                .path(Some(path_text(
                    &environment.codex_home().join("AGENTS.md"),
                )?))
                .format(TargetFormat::Markdown)
                .managed_selectors(["$document"])
                .capability(capability.clone())
                .prompt_override(prompt_override)
                .build(),
            TargetDescriptor::builder(Tool::Codex, ArtifactKind::Mcp, Scope::Global)
                .path(Some(path_text(&config_path)?))
                .format(TargetFormat::Toml)
                .managed_selectors(["mcp_servers"])
                .sensitive_selectors([
                    "mcp_servers/*/http_headers",
                    "mcp_servers/*/env_http_headers",
                    "mcp_servers/*/env",
                ])
                .capability(capability.clone())
                .build(),
            TargetDescriptor::builder(Tool::Codex, ArtifactKind::Skill, Scope::Global)
                .path(Some(path_text(&environment.codex_home().join("skills"))?))
                .format(TargetFormat::SymlinkDirectory)
                .managed_selectors(["$children"])
                .capability(capability.clone())
                .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                .build(),
            // Hooks 使用独立 hooks.json（官方推荐每层只用一种表示，避免与
            // config.toml 内联 [hooks] 混用触发合并警告）；只接管 `hooks`
            // 键，保留用户手写的顶层 description 等内容。
            // 信任审查由 Codex /hooks 完成，这里只呈现既有 trust 语义。
            TargetDescriptor::builder(Tool::Codex, ArtifactKind::Hook, Scope::Global)
                .path(Some(path_text(
                    &environment.codex_home().join("hooks.json"),
                )?))
                .format(TargetFormat::Json)
                .managed_selectors(["hooks"])
                .capability(capability.clone())
                .build(),
            // Agents（子代理）目录（官方 "Subagents" 合同，2026-09-12 核验）：
            // 每个子代理一个 TOML 文件 `<codex_home>/agents/<name>.toml`。
            TargetDescriptor::builder(Tool::Codex, ArtifactKind::Agent, Scope::Global)
                .path(Some(path_text(&environment.codex_home().join("agents"))?))
                .format(TargetFormat::Toml)
                .capability(capability.clone())
                .build(),
        ];

        if let Some(project_root) = context.project_root {
            let root = Path::new(project_root.as_str());
            let project_root = Some(project_root.as_str().to_owned());
            targets.extend([
                TargetDescriptor::builder(Tool::Codex, ArtifactKind::Mcp, Scope::Project)
                    .project_root(project_root.clone())
                    .path(Some(path_text(&root.join(".codex/config.toml"))?))
                    .format(TargetFormat::Toml)
                    .managed_selectors(["mcp_servers"])
                    .sensitive_selectors([
                        "mcp_servers/*/http_headers",
                        "mcp_servers/*/env_http_headers",
                        "mcp_servers/*/env",
                    ])
                    .capability(capability.clone())
                    .trust(project_trust)
                    .build(),
                TargetDescriptor::builder(Tool::Codex, ArtifactKind::Skill, Scope::Project)
                    .project_root(project_root.clone())
                    .path(Some(path_text(&root.join(".codex/skills"))?))
                    .format(TargetFormat::SymlinkDirectory)
                    .managed_selectors(["$children"])
                    .capability(capability.clone())
                    .trust(project_trust)
                    .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                    .build(),
                // 项目级 hooks 只在项目 `.codex/` 层受信任时加载（官方合同），
                // 与项目 MCP 相同的 trust 语义。
                TargetDescriptor::builder(Tool::Codex, ArtifactKind::Hook, Scope::Project)
                    .project_root(project_root.clone())
                    .path(Some(path_text(&root.join(".codex/hooks.json"))?))
                    .format(TargetFormat::Json)
                    .managed_selectors(["hooks"])
                    .capability(capability.clone())
                    .trust(project_trust)
                    .build(),
                // 项目级子代理目录：`<root>/.codex/agents`，沿用 `.codex`
                // 既有信任层（与项目 MCP/Skills/Hooks 相同）。
                TargetDescriptor::builder(Tool::Codex, ArtifactKind::Agent, Scope::Project)
                    .project_root(project_root)
                    .path(Some(path_text(&root.join(".codex/agents"))?))
                    .format(TargetFormat::Toml)
                    .capability(capability)
                    .trust(project_trust)
                    .build(),
            ]);
        }

        crate::adapters::populate_descriptor_allowed_roots(environment, &mut targets)?;
        Ok(targets)
    }
}

impl ProviderCodec for CodexAdapter {
    fn discovery_ownership(&self) -> Result<crate::adapters::ManagedOwnership, AppError> {
        Ok(crate::adapters::ManagedOwnership::selectors([
            vec!["model"],
            vec!["model_provider"],
            vec!["model_providers"],
        ]))
    }

    fn ownership(
        &self,
        baseline: Option<&Value>,
        desired: &Value,
    ) -> Result<crate::adapters::ManagedOwnership, AppError> {
        let mut selectors = BTreeSet::<Vec<String>>::new();
        for projection in [baseline, Some(desired)].into_iter().flatten() {
            if projection.get("model").is_some() {
                selectors.insert(vec!["model".to_owned()]);
            }
            if projection.get("model_provider").is_some() {
                selectors.insert(vec!["model_provider".to_owned()]);
            }
            if let Some(providers) = projection.get("model_providers").and_then(Value::as_object) {
                selectors.extend(
                    providers
                        .keys()
                        .map(|key| vec!["model_providers".to_owned(), key.clone()]),
                );
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
            wire_api: input.wire_api.map(str::to_owned),
            ..ProviderCodecOptions::default()
        })
    }

    fn render(&self, input: &ProviderCodecProfileInput<'_>) -> Result<Value, AppError> {
        const OPENAI_PROVIDER_ID: &str = "openai";

        let provider_id = input.provider_id.ok_or_else(|| {
            AppError::invalid_input("providerOptions", "Codex Provider 缺少稳定 provider id")
        })?;
        validate_provider_id(provider_id)?;
        let default_model = input.default_model.filter(|value| !value.is_empty());
        // 官方账号登录只写模型选择并显式回到内置 openai provider；登录凭据由
        // Codex 自己的 auth.json 承载，本应用不投影任何 token。
        if input.auth_kind == PROVIDER_AUTH_KIND_OFFICIAL_LOGIN || provider_id == OPENAI_PROVIDER_ID
        {
            let mut root = Map::new();
            if let Some(model) = default_model {
                root.insert("model".to_owned(), Value::String(model.to_owned()));
            }
            root.insert(
                "model_provider".to_owned(),
                Value::String(OPENAI_PROVIDER_ID.to_owned()),
            );
            return Ok(Value::Object(root));
        }

        let mut provider = input
            .extra_provider_fields
            .clone()
            .into_iter()
            .collect::<Map<_, _>>();
        provider.insert("name".to_owned(), Value::String(input.name.to_owned()));
        if let Some(value) = input.api_base_url.filter(|value| !value.is_empty()) {
            provider.insert("base_url".to_owned(), Value::String(value.to_owned()));
        }
        if let Some(value) = input.api_key {
            provider.insert(
                "experimental_bearer_token".to_owned(),
                Value::String(value.to_owned()),
            );
        }
        if let Some(value) = input.wire_api {
            provider.insert("wire_api".to_owned(), Value::String(value.to_owned()));
        }
        let mut root = Map::new();
        if let Some(model) = default_model {
            root.insert("model".to_owned(), Value::String(model.to_owned()));
        }
        root.insert(
            "model_provider".to_owned(),
            Value::String(provider_id.to_owned()),
        );
        root.insert(
            "model_providers".to_owned(),
            Value::Object(Map::from_iter([(
                provider_id.to_owned(),
                Value::Object(provider),
            )])),
        );
        Ok(Value::Object(root))
    }

    fn discover(
        &self,
        descriptor: &TargetDescriptor,
        managed_projection: &Value,
        full_hash: &str,
    ) -> Result<Option<ProviderCodecDiscovery>, AppError> {
        const OPENAI_PROVIDER_ID: &str = "openai";
        const RESERVED_PROVIDER_IDS: &[&str] = &["openai", "ollama", "lmstudio"];
        // `model` 可缺省：Codex 会使用内置默认模型，导入后档案的默认模型保持为空。
        let default_model = managed_projection
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_default()
            .to_owned();
        let provider_id = match managed_projection
            .get("model_provider")
            .and_then(Value::as_str)
        {
            Some(value) if value.trim().is_empty() => return Ok(None),
            Some(value) => value,
            None => OPENAI_PROVIDER_ID,
        };
        if provider_id == OPENAI_PROVIDER_ID {
            return discover_openai_provider(
                descriptor,
                managed_projection,
                full_hash,
                default_model,
            );
        }
        if RESERVED_PROVIDER_IDS.contains(&provider_id) {
            return Ok(None);
        }
        validate_provider_id(provider_id)?;
        let Some(table) = managed_projection
            .get("model_providers")
            .and_then(Value::as_object)
            .and_then(|providers| providers.get(provider_id))
            .and_then(Value::as_object)
        else {
            return Ok(None);
        };
        let api_base_url = table
            .get("base_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if api_base_url.is_empty() {
            return Ok(None);
        }
        let api_key = table
            .get("experimental_bearer_token")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if !api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(AppError::invalid_input(
                "experimentalBearerToken",
                "Codex 首次导入仅支持含直接 bearer token 的 Provider",
            ));
        }
        let wire_api = match table.get("wire_api") {
            None => None,
            Some(Value::String(value)) => Some(value.clone()),
            Some(_) => {
                return Err(AppError::invalid_input(
                    "wireApi",
                    "Codex wire_api 必须是字符串",
                ));
            }
        };
        let suggested_name = match table.get("name") {
            Some(Value::String(value)) if !value.trim().is_empty() => Some(value.clone()),
            _ => {
                return Err(AppError::invalid_input(
                    "providerName",
                    "Codex Provider name 必须是非空字符串",
                ));
            }
        };
        let mut managed_projection = Map::new();
        if !default_model.is_empty() {
            managed_projection.insert("model".to_owned(), Value::String(default_model.clone()));
        }
        managed_projection.insert(
            "model_provider".to_owned(),
            Value::String(provider_id.to_owned()),
        );
        managed_projection.insert(
            "model_providers".to_owned(),
            Value::Object(Map::from_iter([(
                provider_id.to_owned(),
                Value::Object(table.clone()),
            )])),
        );
        let managed_projection = Value::Object(managed_projection);
        let extra_provider_fields = table
            .iter()
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "name" | "base_url" | "experimental_bearer_token" | "wire_api"
                )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Ok(Some(ProviderCodecDiscovery {
            target_path: descriptor_path(descriptor)?,
            full_hash: full_hash.to_owned(),
            projection: managed_projection,
            auth_kind: PROVIDER_AUTH_KIND_API_KEY.to_owned(),
            api_base_url,
            api_key,
            default_model,
            credential_env_key: "ANTHROPIC_API_KEY".to_owned(),
            extra_env: BTreeMap::new(),
            skipped_env_keys: Vec::new(),
            provider_id: Some(provider_id.to_owned()),
            wire_api,
            zcode_kind: None,
            opencode_npm: None,
            opencode_api: None,
            extra_provider_fields,
            suggested_name,
        }))
    }
}

fn validate_provider_id(provider_id: &str) -> Result<(), AppError> {
    if provider_id == "openai" {
        return Ok(());
    }
    if provider_id.is_empty()
        || provider_id.len() > 100
        || ["openai", "ollama", "lmstudio"].contains(&provider_id)
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

fn discover_openai_provider(
    descriptor: &TargetDescriptor,
    projection: &Value,
    full_hash: &str,
    default_model: String,
) -> Result<Option<ProviderCodecDiscovery>, AppError> {
    let Some(config_path) = descriptor.path.as_deref() else {
        return Err(AppError::invalid_input("targetPath", "目标路径不可用"));
    };
    let Some(codex_home) = Path::new(config_path).parent() else {
        return Ok(None);
    };
    let Ok(content) = fs::read_to_string(codex_home.join("auth.json")) else {
        return Ok(None);
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return Ok(None);
    };
    let has_tokens = root
        .get("tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| {
            ["access_token", "refresh_token", "id_token"]
                .iter()
                .any(|key| {
                    tokens
                        .get(*key)
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty())
                })
        });
    if !has_tokens {
        return Ok(None);
    }
    let mut managed_projection = Map::new();
    if !default_model.is_empty() {
        managed_projection.insert("model".to_owned(), Value::String(default_model.clone()));
    }
    if let Some(value) = projection.get("model_provider") {
        managed_projection.insert("model_provider".to_owned(), value.clone());
    }
    Ok(Some(ProviderCodecDiscovery {
        target_path: descriptor_path(descriptor)?,
        full_hash: full_hash.to_owned(),
        projection: Value::Object(managed_projection),
        auth_kind: PROVIDER_AUTH_KIND_OFFICIAL_LOGIN.to_owned(),
        api_base_url: String::new(),
        api_key: None,
        default_model,
        credential_env_key: "ANTHROPIC_API_KEY".to_owned(),
        extra_env: BTreeMap::new(),
        skipped_env_keys: Vec::new(),
        provider_id: Some("openai".to_owned()),
        wire_api: None,
        zcode_kind: None,
        opencode_npm: None,
        opencode_api: None,
        extra_provider_fields: BTreeMap::new(),
        suggested_name: Some("Codex 官方账号登录".to_owned()),
    }))
}

fn discover_prompt_override(path: &Path) -> PromptOverrideState {
    match read_discovery_file(path) {
        DiscoveryFile::Missing => PromptOverrideState::NotPresent,
        DiscoveryFile::File(bytes) => match std::str::from_utf8(&bytes) {
            Ok(text) if text.trim().is_empty() => PromptOverrideState::NotPresent,
            Ok(_) => PromptOverrideState::Present,
            Err(_) => PromptOverrideState::Unknown,
        },
        DiscoveryFile::Unavailable => PromptOverrideState::Unknown,
    }
}

fn discover_project_trust(config_path: &Path, project_root: &str) -> TargetTrustState {
    let bytes = match read_discovery_file(config_path) {
        DiscoveryFile::File(bytes) => bytes,
        DiscoveryFile::Missing | DiscoveryFile::Unavailable => return TargetTrustState::Unknown,
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return TargetTrustState::Unknown;
    };
    let Ok(config) = toml_edit::de::from_str::<Value>(text) else {
        return TargetTrustState::Unknown;
    };
    match config
        .get("projects")
        .and_then(Value::as_object)
        .and_then(|projects| projects.get(project_root))
        .and_then(Value::as_object)
        .and_then(|project| project.get("trust_level"))
        .and_then(Value::as_str)
    {
        Some("trusted") => TargetTrustState::Trusted,
        Some("untrusted") => TargetTrustState::Untrusted,
        _ => TargetTrustState::Unknown,
    }
}

enum DiscoveryFile {
    Missing,
    File(Vec<u8>),
    Unavailable,
}

fn read_discovery_file(path: &Path) -> DiscoveryFile {
    let Some(parent) = path.parent() else {
        return DiscoveryFile::Unavailable;
    };
    let canonical_parent = match fs::canonicalize(parent) {
        Ok(parent) => parent,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DiscoveryFile::Missing;
        }
        Err(_) => return DiscoveryFile::Unavailable,
    };
    if canonical_parent != parent {
        return DiscoveryFile::Unavailable;
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DiscoveryFile::Missing;
        }
        Err(_) => return DiscoveryFile::Unavailable,
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return DiscoveryFile::Unavailable;
    }
    fs::read(path)
        .map(DiscoveryFile::File)
        .unwrap_or(DiscoveryFile::Unavailable)
}
