//! OpenCode JSON/JSONC Provider, global Prompt, MCP and Skills targets.
//!
//! OpenCode Hooks are intentionally absent: its plugin callback API is not
//! representable by the command-only Hook model used by this application.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

use crate::{
    adapters::{
        descriptor_path, DiscoveryContext, ManagedOwnership, ProviderCodec, ProviderCodecDiscovery,
        ProviderCodecInput, ProviderCodecOptions, ProviderCodecProfileInput, SymlinkPolicy,
        TargetCapability, TargetDescriptor, TargetFormat, ToolAdapter, ToolAvailabilityState,
    },
    domain::{ArtifactKind, Scope, Tool},
    error::AppError,
};

#[derive(Debug, Default)]
pub struct OpencodeAdapter;

impl ToolAdapter for OpencodeAdapter {
    fn tool(&self) -> Tool {
        Tool::Opencode
    }

    fn provider_codec(&self) -> Option<&dyn ProviderCodec> {
        Some(self)
    }

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let capability = if environment.opencode_config_content().is_some() {
            // Inline config is a higher-priority runtime source. The app cannot
            // represent a file Apply as the complete effective OpenCode config,
            // so every native target fails closed until the override disappears.
            TargetCapability::unsupported("OPENCODE_CONFIG_CONTENT_OVERRIDE")
        } else {
            match (
                environment.opencode_disabled(),
                environment.tool_availability(Tool::Opencode),
            ) {
                (true, _) => TargetCapability::unsupported("OPENCODE_DISCOVERY_DISABLED"),
                (false, ToolAvailabilityState::Installed) => TargetCapability::supported(),
                (false, ToolAvailabilityState::Unavailable) => {
                    TargetCapability::tool_not_installed()
                }
                (false, ToolAvailabilityState::Unsupported) => {
                    TargetCapability::unsupported("OPENCODE_INSTALLATION_PROBE_UNSUPPORTED")
                }
            }
        };
        let config_path = select_config_path(
            environment.opencode_config_path(),
            environment.opencode_config_dir(),
        );
        let config_format = config_path
            .as_ref()
            .map_or(TargetFormat::Json, |path| format_for_path(path));
        let mut targets = vec![
            TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Provider, Scope::Global)
                .path(optional_path_text(config_path.as_deref()))
                .format(config_format)
                .managed_selectors(["model", "provider"])
                .sensitive_selectors(["provider/*/options/apiKey", "provider/*/options/headers"])
                .capability(capability.clone())
                .build(),
            TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Prompt, Scope::Global)
                .path(optional_path_text(Some(
                    &environment.opencode_config_dir().join("AGENTS.md"),
                )))
                .format(TargetFormat::Markdown)
                .managed_selectors(["$document"])
                .capability(capability.clone())
                .build(),
            TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Mcp, Scope::Global)
                .path(optional_path_text(config_path.as_deref()))
                .format(config_format)
                .managed_selectors(["mcp"])
                .sensitive_selectors([
                    "mcp/*/headers",
                    "mcp/*/environment",
                    "mcp/*/oauth/clientSecret",
                ])
                .capability(capability.clone())
                .build(),
            TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Skill, Scope::Global)
                .path(optional_path_text(Some(
                    &environment.opencode_config_dir().join("skills"),
                )))
                .format(TargetFormat::SymlinkDirectory)
                .managed_selectors(["$children"])
                .capability(capability.clone())
                .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                .build(),
            // Agents（子代理）目录（官方 "Agents" 合同，2026-09-12 核验）：
            // `<opencode_config_dir>/agents/<name>.md`；frontmatter 缺省
            // `name` 时以文件名为准，投影层固定写 `mode: subagent`。
            TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Agent, Scope::Global)
                .path(optional_path_text(Some(
                    &environment.opencode_config_dir().join("agents"),
                )))
                .format(TargetFormat::Markdown)
                .capability(capability.clone())
                .build(),
        ];

        if let Some(project_root) = context.project_root {
            let root = PathBuf::from(project_root.as_str());
            let project_config = select_project_config_path(&root);
            let project_format = project_config
                .as_ref()
                .map_or(TargetFormat::Json, |path| format_for_path(path));
            let project_root = Some(project_root.as_str().to_owned());
            targets.extend([
                TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Mcp, Scope::Project)
                    .project_root(project_root.clone())
                    .path(optional_path_text(project_config.as_deref()))
                    .format(project_format)
                    .managed_selectors(["mcp"])
                    .sensitive_selectors([
                        "mcp/*/headers",
                        "mcp/*/environment",
                        "mcp/*/oauth/clientSecret",
                    ])
                    .capability(capability.clone())
                    .build(),
                TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Skill, Scope::Project)
                    .project_root(project_root.clone())
                    .path(optional_path_text(Some(&root.join(".opencode/skills"))))
                    .format(TargetFormat::SymlinkDirectory)
                    .managed_selectors(["$children"])
                    .capability(capability.clone())
                    .symlink_policy(SymlinkPolicy::ManagedChildrenOnly)
                    .build(),
                // 项目级子代理目录：`<root>/.opencode/agents`；沿用与全局
                // 相同的 OpenCode 门禁（config override / disabled）。
                TargetDescriptor::builder(Tool::Opencode, ArtifactKind::Agent, Scope::Project)
                    .project_root(project_root)
                    .path(optional_path_text(Some(&root.join(".opencode/agents"))))
                    .format(TargetFormat::Markdown)
                    .capability(capability)
                    .build(),
            ]);
        }
        crate::adapters::populate_descriptor_allowed_roots(environment, &mut targets)?;
        Ok(targets)
    }
}

impl ProviderCodec for OpencodeAdapter {
    fn discovery_ownership(&self) -> Result<crate::adapters::ManagedOwnership, AppError> {
        Ok(crate::adapters::ManagedOwnership::selectors([
            ["model"],
            ["provider"],
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
            let Some(providers) = projection.get("provider").and_then(Value::as_object) else {
                continue;
            };
            for (provider_id, provider) in providers {
                for leaf in ["npm", "name"] {
                    selectors.insert(vec![
                        "provider".to_owned(),
                        provider_id.clone(),
                        leaf.to_owned(),
                    ]);
                }
                if provider.get("options").is_some() {
                    for leaf in ["baseURL", "apiKey"] {
                        selectors.insert(vec![
                            "provider".to_owned(),
                            provider_id.clone(),
                            "options".to_owned(),
                            leaf.to_owned(),
                        ]);
                    }
                }
                if let Some(models) = provider.get("models").and_then(Value::as_object) {
                    for model_id in models.keys() {
                        selectors.insert(vec![
                            "provider".to_owned(),
                            provider_id.clone(),
                            "models".to_owned(),
                            model_id.clone(),
                        ]);
                    }
                }
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
            opencode_npm: input.opencode_npm.map(str::to_owned),
            opencode_api: input.opencode_api.map(str::to_owned),
            ..ProviderCodecOptions::default()
        })
    }

    fn render(&self, input: &ProviderCodecProfileInput<'_>) -> Result<Value, AppError> {
        let provider_id = input.provider_id.ok_or_else(|| {
            AppError::invalid_input("providerOptions", "OpenCode Provider 缺少稳定 provider id")
        })?;
        let npm = input.opencode_npm.ok_or_else(|| {
            AppError::invalid_input("providerOptions", "OpenCode Provider 缺少 npm SDK")
        })?;
        let mut provider = input
            .extra_provider_fields
            .clone()
            .into_iter()
            .collect::<Map<_, _>>();
        provider.insert("npm".to_owned(), Value::String(npm.to_owned()));
        provider.insert("name".to_owned(), Value::String(input.name.to_owned()));
        let mut options = provider
            .remove("options")
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        if let Some(value) = input.api_base_url {
            options.insert("baseURL".to_owned(), Value::String(value.to_owned()));
        }
        if let Some(value) = input.api_key {
            options.insert("apiKey".to_owned(), Value::String(value.to_owned()));
        } else {
            options.remove("apiKey");
        }
        provider.insert("options".to_owned(), Value::Object(options));
        let model_id = input.default_model.unwrap_or("default");
        let mut models = provider
            .remove("models")
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        models
            .entry(model_id.to_owned())
            .or_insert_with(|| json!({ "name": model_id }));
        provider.insert("models".to_owned(), Value::Object(models));
        let model_ref = format!("{provider_id}/{model_id}");
        let mut root = Map::new();
        root.insert("model".to_owned(), Value::String(model_ref));
        root.insert(
            "provider".to_owned(),
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
        let target_path = descriptor_path(descriptor)?;
        let root = managed_projection
            .as_object()
            .ok_or_else(|| AppError::parse(&target_path, "json"))?;
        let model_ref = root
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some((provider_id, model_id)) = model_ref.split_once('/') else {
            return Ok(None);
        };
        if provider_id.trim().is_empty() || model_id.trim().is_empty() {
            return Ok(None);
        }
        let Some(entry) = root
            .get("provider")
            .and_then(Value::as_object)
            .and_then(|providers| providers.get(provider_id))
            .and_then(Value::as_object)
        else {
            return Ok(None);
        };
        let Some(npm) = entry.get("npm").and_then(Value::as_str) else {
            return Ok(None);
        };
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            return Ok(None);
        };
        if npm.trim().is_empty() || name.trim().is_empty() {
            return Ok(None);
        }
        let Some(options) = entry.get("options").and_then(Value::as_object) else {
            return Ok(None);
        };
        let Some(api_base_url) = options.get("baseURL").and_then(Value::as_str) else {
            return Ok(None);
        };
        if api_base_url.trim().is_empty() {
            return Ok(None);
        }
        let api_key = options
            .get("apiKey")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut managed_entry = Map::new();
        for key in ["npm", "name"] {
            if let Some(value) = entry.get(key) {
                managed_entry.insert(key.to_owned(), value.clone());
            }
        }
        let mut managed_options = Map::new();
        for key in ["baseURL", "apiKey"] {
            if let Some(value) = options.get(key) {
                managed_options.insert(key.to_owned(), value.clone());
            }
        }
        managed_entry.insert("options".to_owned(), Value::Object(managed_options));
        if let Some(models) = entry.get("models").and_then(Value::as_object) {
            if let Some(model) = models.get(model_id) {
                managed_entry.insert(
                    "models".to_owned(),
                    Value::Object(Map::from_iter([(model_id.to_owned(), model.clone())])),
                );
            }
        }
        Ok(Some(ProviderCodecDiscovery {
            target_path,
            full_hash: full_hash.to_owned(),
            projection: json!({
                "model": model_ref,
                "provider": { provider_id: Value::Object(managed_entry) },
            }),
            auth_kind: crate::adapters::PROVIDER_AUTH_KIND_API_KEY.to_owned(),
            api_base_url: api_base_url.to_owned(),
            api_key,
            default_model: model_id.to_owned(),
            credential_env_key: "ANTHROPIC_API_KEY".to_owned(),
            extra_env: BTreeMap::new(),
            skipped_env_keys: Vec::new(),
            provider_id: Some(provider_id.to_owned()),
            wire_api: None,
            zcode_kind: None,
            opencode_npm: Some(npm.to_owned()),
            opencode_api: Some(if npm == "@ai-sdk/openai" {
                "openai".to_owned()
            } else {
                "openai-compatible".to_owned()
            }),
            extra_provider_fields: entry
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "npm" | "name" | "options"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            suggested_name: Some(name.to_owned()),
        }))
    }
}

/// OpenCode 目标可能没有配置文件（`config_path == None`）；非 UTF-8 路径与
/// 缺失路径一样视为"无路径"，与既有描述符语义保持一致。
fn optional_path_text(path: Option<&Path>) -> Option<String> {
    path.and_then(|path| path.to_str().map(str::to_owned))
}

fn select_config_path(explicit: Option<&Path>, config_dir: &Path) -> Option<PathBuf> {
    explicit.map(Path::to_path_buf).or_else(|| {
        let jsonc = config_dir.join("opencode.jsonc");
        let json = config_dir.join("opencode.json");
        if is_regular_candidate(&jsonc) {
            Some(jsonc)
        } else {
            Some(json)
        }
    })
}

fn select_project_config_path(root: &Path) -> Option<PathBuf> {
    [
        root.join(".opencode/opencode.jsonc"),
        root.join(".opencode/opencode.json"),
        root.join("opencode.jsonc"),
        root.join("opencode.json"),
    ]
    .into_iter()
    .find(|path| is_regular_candidate(path))
    .or_else(|| Some(root.join("opencode.json")))
}

fn is_regular_candidate(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn format_for_path(path: &Path) -> TargetFormat {
    if path.extension().and_then(|value| value.to_str()) == Some("jsonc") {
        TargetFormat::Jsonc
    } else {
        TargetFormat::Json
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::{
        adapters::{
            ConservativeClaudeCustomizationPolicyProbe, ConservativeClaudeUserMcpProbe,
            DiscoveryContext, ExplicitEnvironment, TargetFormat, ToolAdapter, ToolAvailability,
        },
        domain::{ArtifactKind, ProjectRoot, Scope, Tool},
    };

    use super::OpencodeAdapter;

    #[test]
    fn discovers_opencode_global_and_project_contract_without_hooks() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let home = root.join("home");
        let project = root.join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&project).unwrap();
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed()).unwrap();
        let project_root = ProjectRoot::parse(&project).unwrap();
        let context = DiscoveryContext {
            environment: &environment,
            project_root: Some(&project_root),
            claude_user_mcp_probe: &ConservativeClaudeUserMcpProbe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let targets = OpencodeAdapter.discover(&context).unwrap();
        assert!(targets.iter().all(|target| target.tool == Tool::Opencode));
        assert!(targets
            .iter()
            .all(|target| target.artifact_kind != ArtifactKind::Hook));
        assert!(targets.iter().any(|target| {
            target.artifact_kind == ArtifactKind::Prompt
                && target.scope == Scope::Global
                && target
                    .path
                    .as_deref()
                    .is_some_and(|path| path.ends_with("AGENTS.md"))
        }));
        assert!(targets.iter().any(|target| {
            target.artifact_kind == ArtifactKind::Skill
                && target.scope == Scope::Project
                && target
                    .path
                    .as_deref()
                    .is_some_and(|path| path.ends_with(".opencode/skills"))
        }));
    }

    #[test]
    fn inline_config_override_blocks_all_file_targets() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_opencode_config_content(Some("{\"model\":\"fixture/model\"}".to_owned()));
        let context = DiscoveryContext {
            environment: &environment,
            project_root: None,
            claude_user_mcp_probe: &ConservativeClaudeUserMcpProbe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let targets = OpencodeAdapter.discover(&context).unwrap();
        assert!(!targets.is_empty());
        assert!(targets.iter().all(|target| {
            target.capability.state == crate::adapters::CapabilityState::Unsupported
                && target.capability.diagnostic_code.as_deref()
                    == Some("OPENCODE_CONFIG_CONTENT_OVERRIDE")
        }));
    }

    #[test]
    fn custom_config_file_uses_its_parent_as_the_narrow_file_boundary() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let custom_path = home.join("custom/opencode.jsonc");
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_opencode_config_path(&custom_path)
                .unwrap();
        assert_eq!(environment.opencode_config_file_root(), home.join("custom"));

        let context = DiscoveryContext {
            environment: &environment,
            project_root: None,
            claude_user_mcp_probe: &ConservativeClaudeUserMcpProbe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let targets = OpencodeAdapter.discover(&context).unwrap();
        let provider = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        assert_eq!(provider.path.as_deref(), custom_path.to_str());
        assert_eq!(provider.format, TargetFormat::Jsonc);
    }
}
