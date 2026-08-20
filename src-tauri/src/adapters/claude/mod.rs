//! Claude 配置格式、目标矩阵与管理策略发现。

use std::path::Path;

use crate::{
    adapters::{
        ClaudeCustomizationPolicyProbeInput, ClaudeUserMcpProbeInput, ClaudeUserMcpProbeResult,
        DiscoveryContext, PolicyState, PromptOverrideState, SymlinkPolicy, TargetCapability,
        TargetDescriptor, TargetFormat, TargetTrustState, ToolAdapter,
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

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let installed = environment.availability().claude;
        let tool_capability = if installed {
            TargetCapability::supported()
        } else {
            TargetCapability::tool_not_installed()
        };
        let settings_path = environment.claude_config_dir().join("settings.json");
        let customization_policy =
            context
                .claude_customization_policy_probe
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: environment.claude_installation_version(),
                    tool_installed: installed,
                });

        let user_mcp_probe = if !installed {
            ClaudeUserMcpProbeResult::ToolNotInstalled
        } else if environment.uses_default_claude_config_dir() {
            ClaudeUserMcpProbeResult::Supported(environment.home().join(".claude.json"))
        } else if environment.claude_installation_version().is_none() {
            ClaudeUserMcpProbeResult::Unsupported("CLAUDE_INSTALLATION_VERSION_UNKNOWN")
        } else {
            context
                .claude_user_mcp_probe
                .probe(&ClaudeUserMcpProbeInput {
                    home: environment.home(),
                    claude_config_dir: environment.claude_config_dir(),
                    uses_default_config_dir: environment.uses_default_claude_config_dir(),
                    installation_version: environment.claude_installation_version(),
                    tool_installed: installed,
                })
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
                    tool_capability,
                    customization_policy.skill,
                    SymlinkPolicy::ManagedChildrenOnly,
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
    path: Option<String>,
    format: TargetFormat,
    managed_selector_roots: Vec<&str>,
    sensitive_selectors: Vec<&str>,
    capability: TargetCapability,
    policy: PolicyState,
    symlink_policy: SymlinkPolicy,
) -> TargetDescriptor {
    TargetDescriptor {
        tool: Tool::Claude,
        artifact_kind,
        scope,
        project_root,
        path,
        format,
        managed_selector_roots: managed_selector_roots
            .into_iter()
            .map(str::to_owned)
            .collect(),
        sensitive_selectors: sensitive_selectors.into_iter().map(str::to_owned).collect(),
        capability,
        policy,
        trust: TargetTrustState::NotRequired,
        prompt_override: PromptOverrideState::NotApplicable,
        symlink_policy,
    }
}

fn path_text(path: &Path) -> Result<String, AppError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::invalid_input("targetPath", "目标路径必须是 UTF-8"))
}
