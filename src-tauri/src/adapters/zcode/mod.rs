//! ZCode 官方 Provider/Prompt/MCP/Skills 路径与能力矩阵。
//!
//! 证据来源（2026-09-05 本机核验 + 官方 zcode-configuration-guide）：
//! - Provider：`~/.zcode/v2/config.json` 顶层 `provider` 对象（JSON，含敏感 apiKey）。
//! - Prompt：`~/.zcode/AGENTS.md` 与 `<project>/AGENTS.md`（Markdown 整文档）。
//! - MCP：`~/.zcode/cli/config.json` 与 `<project>/.zcode/config.json` 的嵌套键
//!   `mcp.servers`（JSON；同一文件还承载 hooks 等非受管内容，必须用选择器只接管 MCP 子树）。
//! - Skills：`~/.zcode/skills` 与 `<project>/.zcode/skills`（目录 + SKILL.md）。

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
pub struct ZcodeAdapter;

impl ToolAdapter for ZcodeAdapter {
    fn tool(&self) -> Tool {
        Tool::Zcode
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
        ];

        if let Some(project_root) = context.project_root {
            let root = Path::new(project_root.as_str());
            targets.extend([
                descriptor(
                    ArtifactKind::Prompt,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    Some(path_text(&root.join("AGENTS.md"))?),
                    TargetFormat::Markdown,
                    vec!["$document"],
                    vec![],
                    supported_capability.clone(),
                    SymlinkPolicy::Reject,
                ),
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
                    supported_capability,
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
    symlink_policy: SymlinkPolicy,
) -> TargetDescriptor {
    TargetDescriptor {
        tool: Tool::Zcode,
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
        let project_prompt = targets
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Prompt && target.scope == Scope::Project
            })
            .unwrap();
        assert_eq!(
            project_prompt.path.as_deref(),
            project.join("AGENTS.md").to_str()
        );
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
            ToolAvailability {
                claude: ToolAvailabilityState::Installed,
                codex: ToolAvailabilityState::Installed,
                cursor: ToolAvailabilityState::Installed,
                zcode: ToolAvailabilityState::Unavailable,
            },
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
