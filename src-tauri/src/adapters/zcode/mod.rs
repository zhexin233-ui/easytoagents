//! ZCode 官方 Provider/Prompt/MCP/Skills 路径与能力矩阵。
//!
//! 证据来源（2026-09-05 本机核验 + 官方 zcode-configuration-guide）：
//! - Provider：`~/.zcode/v2/config.json` 顶层 `provider` 对象（JSON，含敏感 apiKey）。
//! - Prompt：仅全局 `~/.zcode/AGENTS.md`（Markdown 整文档）。
//! - MCP：`~/.zcode/cli/config.json` 与 `<project>/.zcode/config.json` 的嵌套键
//!   `mcp.servers`（JSON；同一文件还承载 hooks 等非受管内容，必须用选择器只接管 MCP 子树）。
//! - Skills：`~/.zcode/skills` 与 `<project>/.zcode/skills`（目录 + SKILL.md）。

use std::{collections::BTreeSet, path::Path};

use serde_json::{json, Map, Value};

use crate::{
    adapters::{
        descriptor_path, path_text, DiscoveryContext, ManagedOwnership, ProviderCodec,
        ProviderCodecDiscovery, ProviderCodecInput, ProviderCodecOptions,
        ProviderCodecProfileInput, SymlinkPolicy, TargetCapability, TargetDescriptor, TargetFormat,
        ToolAdapter, ToolAvailabilityState,
    },
    domain::{ArtifactKind, Scope, Tool},
    error::AppError,
};

#[derive(Debug, Default)]
pub struct ZcodeAdapter;

impl ToolAdapter for ZcodeAdapter {
    fn tool(&self) -> Tool {
        Tool::Zcode
    }

    fn provider_codec(&self) -> Option<&dyn ProviderCodec> {
        Some(self)
    }

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let supported_capability = match environment.tool_availability(Tool::Zcode) {
            ToolAvailabilityState::Installed => TargetCapability::supported(),
            ToolAvailabilityState::Unavailable => TargetCapability::tool_not_installed(),
            ToolAvailabilityState::Unsupported => {
                TargetCapability::unsupported("ZCODE_INSTALLATION_PROBE_UNSUPPORTED")
            }
        };
        let mut targets = vec![
            // Provider 只在本机全局配置中存在；`models`/`source` 等 ZCode 自管字段
            // 通过 options/name/kind/enabled 子选择器保留，不整项覆盖。
            descriptor(
                ArtifactKind::Provider,
                Scope::Global,
                None,
                Some(path_text(
                    &environment.home().join(".zcode/v2/config.json"),
                )?),
                TargetFormat::Json,
                vec!["provider"],
                vec!["provider/*/options/apiKey"],
                supported_capability.clone(),
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Prompt,
                Scope::Global,
                None,
                Some(path_text(&environment.home().join(".zcode/AGENTS.md"))?),
                TargetFormat::Markdown,
                vec!["$document"],
                vec![],
                supported_capability.clone(),
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Mcp,
                Scope::Global,
                None,
                Some(path_text(
                    &environment.home().join(".zcode/cli/config.json"),
                )?),
                TargetFormat::Json,
                vec!["mcp"],
                vec![
                    "mcp/servers/*/headers",
                    "mcp/servers/*/env",
                    "mcp/servers/*/auth",
                ],
                supported_capability.clone(),
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Skill,
                Scope::Global,
                None,
                Some(path_text(&environment.home().join(".zcode/skills"))?),
                TargetFormat::SymlinkDirectory,
                vec!["$children"],
                vec![],
                supported_capability.clone(),
                SymlinkPolicy::ManagedChildrenOnly,
            ),
            // Hooks：与 MCP 同住 `~/.zcode/cli/config.json`，用选择器只接管
            // `hooks` 子树。官方要求配置文件 hooks 必须 `hooks.enabled: true`
            // 才会运行，由投影层恒写该开关（见 hooks/service.rs）。
            descriptor(
                ArtifactKind::Hook,
                Scope::Global,
                None,
                Some(path_text(
                    &environment.home().join(".zcode/cli/config.json"),
                )?),
                TargetFormat::Json,
                vec!["hooks"],
                vec![],
                supported_capability.clone(),
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
                    Some(path_text(&root.join(".zcode/config.json"))?),
                    TargetFormat::Json,
                    vec!["mcp"],
                    vec![
                        "mcp/servers/*/headers",
                        "mcp/servers/*/env",
                        "mcp/servers/*/auth",
                    ],
                    supported_capability.clone(),
                    SymlinkPolicy::Reject,
                ),
                descriptor(
                    ArtifactKind::Skill,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(path_text(&root.join(".zcode/skills"))?),
                    TargetFormat::SymlinkDirectory,
                    vec!["$children"],
                    vec![],
                    supported_capability.clone(),
                    SymlinkPolicy::ManagedChildrenOnly,
                ),
                descriptor(
                    ArtifactKind::Hook,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(path_text(&root.join(".zcode/config.json"))?),
                    TargetFormat::Json,
                    vec!["hooks"],
                    vec![],
                    supported_capability,
                    SymlinkPolicy::Reject,
                ),
            ]);
        }

        crate::adapters::populate_descriptor_allowed_roots(environment, &mut targets)?;
        Ok(targets)
    }
}

impl ProviderCodec for ZcodeAdapter {
    fn discovery_ownership(&self) -> Result<crate::adapters::ManagedOwnership, AppError> {
        Ok(crate::adapters::ManagedOwnership::selectors([["provider"]]))
    }

    fn ownership(
        &self,
        baseline: Option<&Value>,
        desired: &Value,
    ) -> Result<crate::adapters::ManagedOwnership, AppError> {
        let mut selectors = BTreeSet::<Vec<String>>::new();
        for projection in [baseline, Some(desired)].into_iter().flatten() {
            let Some(providers) = projection.get("provider").and_then(Value::as_object) else {
                continue;
            };
            for provider_id in providers.keys() {
                for leaf in ["name", "kind", "options", "enabled"] {
                    selectors.insert(vec![
                        "provider".to_owned(),
                        provider_id.clone(),
                        leaf.to_owned(),
                    ]);
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
            zcode_kind: input.zcode_kind.map(str::to_owned),
            ..ProviderCodecOptions::default()
        })
    }

    fn render(&self, input: &ProviderCodecProfileInput<'_>) -> Result<Value, AppError> {
        let provider_id = input.provider_id.ok_or_else(|| {
            AppError::invalid_input("providerOptions", "ZCode Provider 缺少稳定 provider id")
        })?;
        let mut options = Map::new();
        if let Some(value) = input.api_base_url {
            options.insert("baseURL".to_owned(), Value::String(value.to_owned()));
        }
        if let Some(value) = input.api_key {
            options.insert("apiKey".to_owned(), Value::String(value.to_owned()));
        }
        let entry = json!({
            "name": input.name,
            "kind": input.zcode_kind.unwrap_or("anthropic"),
            "options": Value::Object(options),
            "enabled": true,
        });
        Ok(json!({ "provider": { provider_id: entry } }))
    }

    fn discover(
        &self,
        descriptor: &TargetDescriptor,
        managed_projection: &Value,
        full_hash: &str,
    ) -> Result<Option<ProviderCodecDiscovery>, AppError> {
        const PROVIDER_KINDS: &[&str] = &["anthropic", "openai", "gemini"];
        let entries = managed_projection
            .get("provider")
            .and_then(Value::as_object)
            .ok_or_else(|| AppError::parse("~/.zcode/v2/config.json", "json"))?;
        let enabled: Vec<&String> = entries
            .iter()
            .filter(|(_, value)| value.get("enabled").and_then(Value::as_bool) == Some(true))
            .map(|(key, _)| key)
            .collect();
        let selected = match (enabled.len(), entries.len()) {
            (1, _) => (enabled[0], &entries[enabled[0]]),
            (0, 1) => entries
                .iter()
                .next()
                .ok_or_else(|| AppError::parse("~/.zcode/v2/config.json", "json"))?,
            _ => return Ok(None),
        };
        let (provider_id, entry) = selected;
        if entry
            .get("kind")
            .and_then(Value::as_str)
            .map_or(true, |kind| !PROVIDER_KINDS.contains(&kind))
        {
            return Ok(None);
        }
        let kind = entry
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("anthropic")
            .to_owned();
        let options = entry.get("options").and_then(Value::as_object);
        let api_base_url = options
            .and_then(|options| options.get("baseURL"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if api_base_url.is_empty() {
            return Ok(None);
        }
        let api_key = options
            .and_then(|options| options.get("apiKey"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let default_model = entry
            .get("models")
            .and_then(Value::as_object)
            .map(|models| {
                models
                    .keys()
                    .min_by(|a, b| a.cmp(b))
                    .cloned()
                    .unwrap_or_else(|| "unspecified".to_owned())
            })
            .unwrap_or_else(|| "unspecified".to_owned());
        let mut managed_entry = Map::new();
        for leaf in ["name", "kind", "options", "enabled"] {
            if let Some(value) = entry.get(leaf) {
                managed_entry.insert(leaf.to_owned(), value.clone());
            }
        }
        Ok(Some(ProviderCodecDiscovery {
            target_path: descriptor_path(descriptor)?,
            full_hash: full_hash.to_owned(),
            projection: json!({ "provider": { provider_id: Value::Object(managed_entry) } }),
            api_base_url,
            api_key,
            default_model,
            credential_env_key: "ANTHROPIC_API_KEY".to_owned(),
            extra_env: std::collections::BTreeMap::new(),
            provider_id: Some(provider_id.clone()),
            wire_api: None,
            zcode_kind: Some(kind),
            opencode_npm: None,
            opencode_api: None,
            extra_provider_fields: std::collections::BTreeMap::new(),
            suggested_name: entry
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| Some(provider_id.clone())),
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
    symlink_policy: SymlinkPolicy,
) -> TargetDescriptor {
    TargetDescriptor::builder(Tool::Zcode, artifact_kind, scope)
        .project_root(project_root.clone())
        .allowed_root(project_root)
        .path(path)
        .format(format)
        .managed_selectors(managed_selector_roots)
        .sensitive_selectors(sensitive_selectors)
        .capability(capability)
        .symlink_policy(symlink_policy)
        .build()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::{
        adapters::{
            CapabilityState, ConservativeClaudeCustomizationPolicyProbe,
            ConservativeClaudeUserMcpProbe, DiscoveryContext, ExplicitEnvironment, ToolAdapter,
            ToolAvailability, ToolAvailabilityState,
        },
        domain::{ArtifactKind, ProjectRoot, Scope, Tool},
    };

    use super::ZcodeAdapter;

    #[test]
    fn descriptor_matrix_supports_full_capability_set() {
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

        let targets = ZcodeAdapter.discover(&context).unwrap();
        assert!(targets.iter().all(|target| target.tool == Tool::Zcode));
        let global_provider = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        assert_eq!(global_provider.scope, Scope::Global);
        assert_eq!(
            global_provider.path.as_deref(),
            home.join(".zcode/v2/config.json").to_str()
        );
        assert_eq!(global_provider.managed_selector_roots, ["provider"]);
        assert!(global_provider
            .sensitive_selectors
            .iter()
            .any(|root| root.ends_with("/options/apiKey")));

        let global_mcp = targets
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
            })
            .unwrap();
        assert_eq!(
            global_mcp.path.as_deref(),
            home.join(".zcode/cli/config.json").to_str()
        );
        assert_eq!(global_mcp.managed_selector_roots, ["mcp"]);
        assert!(global_mcp
            .sensitive_selectors
            .iter()
            .any(|root| root.ends_with("/headers")));

        let global_prompt = targets
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Prompt && target.scope == Scope::Global
            })
            .unwrap();
        assert_eq!(
            global_prompt.path.as_deref(),
            home.join(".zcode/AGENTS.md").to_str()
        );

        let project_mcp = targets
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
            })
            .unwrap();
        assert_eq!(
            project_mcp.path.as_deref(),
            project.join(".zcode/config.json").to_str()
        );
        assert!(!targets.iter().any(|target| {
            target.artifact_kind == ArtifactKind::Prompt && target.scope == Scope::Project
        }));
        let project_skill = targets
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Skill && target.scope == Scope::Project
            })
            .unwrap();
        assert_eq!(
            project_skill.path.as_deref(),
            project.join(".zcode/skills").to_str()
        );
        assert!(targets
            .iter()
            .all(|target| target.capability.state == CapabilityState::Supported));
    }

    #[test]
    fn unavailable_tool_fails_closed_to_tool_not_installed() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = ExplicitEnvironment::new(
            &home,
            None,
            None,
            ToolAvailability::from_states([
                ToolAvailabilityState::Installed,
                ToolAvailabilityState::Installed,
                ToolAvailabilityState::Installed,
                ToolAvailabilityState::Unavailable,
                ToolAvailabilityState::Installed,
            ]),
        )
        .unwrap();
        let context = DiscoveryContext {
            environment: &environment,
            project_root: None,
            claude_user_mcp_probe: &ConservativeClaudeUserMcpProbe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };

        let targets = ZcodeAdapter.discover(&context).unwrap();
        assert!(targets
            .iter()
            .all(|target| target.capability.state == CapabilityState::ToolNotInstalled));
    }
}
