//! OpenCode JSON/JSONC Provider, global Prompt, MCP and Skills targets.
//!
//! OpenCode Hooks are intentionally absent: its plugin callback API is not
//! representable by the command-only Hook model used by this application.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    adapters::{
        DiscoveryContext, PolicyState, PromptOverrideState, SymlinkPolicy, TargetCapability,
        TargetDescriptor, TargetFormat, TargetTrustState, ToolAdapter, ToolAvailabilityState,
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
            descriptor(
                ArtifactKind::Provider,
                Scope::Global,
                None,
                config_path.clone(),
                config_format,
                vec!["provider"],
                vec!["provider/*/options/apiKey", "provider/*/options/headers"],
                capability.clone(),
            ),
            descriptor(
                ArtifactKind::Prompt,
                Scope::Global,
                None,
                Some(environment.opencode_config_dir().join("AGENTS.md")),
                TargetFormat::Markdown,
                vec!["$document"],
                Vec::new(),
                capability.clone(),
            ),
            descriptor(
                ArtifactKind::Mcp,
                Scope::Global,
                None,
                config_path.clone(),
                config_format,
                vec!["mcp"],
                vec![
                    "mcp/*/headers",
                    "mcp/*/environment",
                    "mcp/*/oauth/clientSecret",
                ],
                capability.clone(),
            ),
            descriptor(
                ArtifactKind::Skill,
                Scope::Global,
                None,
                Some(environment.opencode_config_dir().join("skills")),
                TargetFormat::SymlinkDirectory,
                vec!["$children"],
                Vec::new(),
                capability.clone(),
            ),
        ];

        if let Some(project_root) = context.project_root {
            let root = PathBuf::from(project_root.as_str());
            let project_config = select_project_config_path(&root);
            let project_format = project_config
                .as_ref()
                .map_or(TargetFormat::Json, |path| format_for_path(path));
            targets.extend([
                descriptor(
                    ArtifactKind::Mcp,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    project_config,
                    project_format,
                    vec!["mcp"],
                    vec![
                        "mcp/*/headers",
                        "mcp/*/environment",
                        "mcp/*/oauth/clientSecret",
                    ],
                    capability.clone(),
                ),
                descriptor(
                    ArtifactKind::Skill,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(root.join(".opencode/skills")),
                    TargetFormat::SymlinkDirectory,
                    vec!["$children"],
                    Vec::new(),
                    capability,
                ),
            ]);
        }
        Ok(targets)
    }
}

#[allow(clippy::too_many_arguments)]
fn descriptor(
    artifact_kind: ArtifactKind,
    scope: Scope,
    project_root: Option<String>,
    path: Option<PathBuf>,
    format: TargetFormat,
    managed_selector_roots: Vec<&str>,
    sensitive_selectors: Vec<&str>,
    capability: TargetCapability,
) -> TargetDescriptor {
    TargetDescriptor {
        tool: Tool::Opencode,
        artifact_kind,
        scope,
        project_root,
        path: path.and_then(|path| path.to_str().map(str::to_owned)),
        format,
        managed_selector_roots: managed_selector_roots
            .into_iter()
            .map(str::to_owned)
            .collect(),
        sensitive_selectors: sensitive_selectors.into_iter().map(str::to_owned).collect(),
        capability,
        policy: PolicyState::Allowed,
        trust: TargetTrustState::NotRequired,
        prompt_override: PromptOverrideState::NotApplicable,
        symlink_policy: if artifact_kind == ArtifactKind::Skill {
            SymlinkPolicy::ManagedChildrenOnly
        } else {
            SymlinkPolicy::Reject
        },
    }
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
