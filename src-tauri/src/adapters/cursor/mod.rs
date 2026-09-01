//! Cursor 官方 MCP/Skills 路径与能力矩阵。

use std::path::Path;

use crate::{
    adapters::{
        DiscoveryContext, PolicyState, PromptOverrideState, SymlinkPolicy, TargetCapability,
        TargetDescriptor, TargetFormat, TargetTrustState, ToolAdapter, ToolAvailabilityState,
    },
    domain::{ArtifactKind, Scope, Tool},
    error::AppError,
};

#[derive(Debug, Default)]
pub struct CursorAdapter;

impl ToolAdapter for CursorAdapter {
    fn tool(&self) -> Tool {
        Tool::Cursor
    }

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let supported_capability = match environment.tool_availability(Tool::Cursor) {
            ToolAvailabilityState::Installed => TargetCapability::supported(),
            ToolAvailabilityState::Unavailable => TargetCapability::tool_not_installed(),
            ToolAvailabilityState::Unsupported => {
                TargetCapability::unsupported("CURSOR_INSTALLATION_PROBE_UNSUPPORTED")
            }
        };
        let cursor_home = environment.home().join(".cursor");
        let mut targets = vec![
            descriptor(
                ArtifactKind::Provider,
                Scope::Global,
                None,
                None,
                TargetFormat::Json,
                vec![],
                vec![],
                TargetCapability::unsupported("CURSOR_PROVIDER_UNSUPPORTED"),
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Prompt,
                Scope::Global,
                None,
                None,
                TargetFormat::Markdown,
                vec![],
                vec![],
                TargetCapability::unsupported("CURSOR_PROMPT_UNSUPPORTED"),
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Mcp,
                Scope::Global,
                None,
                Some(path_text(&cursor_home.join("mcp.json"))?),
                TargetFormat::Json,
                vec!["mcpServers"],
                vec![
                    "mcpServers/*/headers",
                    "mcpServers/*/env",
                    "mcpServers/*/auth",
                ],
                supported_capability.clone(),
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Skill,
                Scope::Global,
                None,
                Some(path_text(&cursor_home.join("skills"))?),
                TargetFormat::SymlinkDirectory,
                vec!["$children"],
                vec![],
                supported_capability.clone(),
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
                    Some(path_text(&root.join(".cursor/mcp.json"))?),
                    TargetFormat::Json,
                    vec!["mcpServers"],
                    vec![
                        "mcpServers/*/headers",
                        "mcpServers/*/env",
                        "mcpServers/*/auth",
                    ],
                    supported_capability.clone(),
                    SymlinkPolicy::Reject,
                ),
                descriptor(
                    ArtifactKind::Skill,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(path_text(&root.join(".cursor/skills"))?),
                    TargetFormat::SymlinkDirectory,
                    vec!["$children"],
                    vec![],
                    supported_capability,
                    SymlinkPolicy::ManagedChildrenOnly,
                ),
                descriptor(
                    ArtifactKind::Prompt,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    None,
                    TargetFormat::Markdown,
                    vec![],
                    vec![],
                    TargetCapability::unsupported("CURSOR_PROMPT_UNSUPPORTED"),
                    SymlinkPolicy::Reject,
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
    symlink_policy: SymlinkPolicy,
) -> TargetDescriptor {
    TargetDescriptor {
        tool: Tool::Cursor,
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
        policy: PolicyState::Allowed,
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::{
        adapters::{
            CapabilityState, ConservativeClaudeCustomizationPolicyProbe,
            ConservativeClaudeUserMcpProbe, DiscoveryContext, ExplicitEnvironment, ToolAdapter,
            ToolAvailability,
        },
        domain::{ArtifactKind, ProjectRoot, Scope, Tool},
    };

    use super::CursorAdapter;

    #[test]
    fn descriptor_matrix_only_supports_mcp_and_skills() {
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

        let targets = CursorAdapter.discover(&context).unwrap();
        assert!(targets.iter().all(|target| target.tool == Tool::Cursor));
        for target in &targets {
            let supported = matches!(
                target.artifact_kind,
                ArtifactKind::Mcp | ArtifactKind::Skill
            );
            assert_eq!(
                target.capability.state == CapabilityState::Supported,
                supported
            );
            if supported {
                assert!(target.path.is_some());
            } else {
                assert!(target.path.is_none());
            }
        }
        let global_mcp = targets
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
            })
            .unwrap();
        assert_eq!(
            global_mcp.path.as_deref(),
            home.join(".cursor/mcp.json").to_str()
        );
        assert_eq!(global_mcp.managed_selector_roots, ["mcpServers"]);
        assert!(global_mcp
            .sensitive_selectors
            .iter()
            .any(|root| root.ends_with("/auth")));
        let project_skill = targets
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Skill && target.scope == Scope::Project
            })
            .unwrap();
        assert_eq!(
            project_skill.path.as_deref(),
            project.join(".cursor/skills").to_str()
        );
    }
}
