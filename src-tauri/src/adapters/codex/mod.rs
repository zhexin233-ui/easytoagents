//! Codex 配置格式、目标矩阵与项目 trust 发现。

use std::{fs, path::Path};

use serde_json::Value;

use crate::{
    adapters::{
        DiscoveryContext, PolicyState, PromptOverrideState, SymlinkPolicy, TargetCapability,
        TargetDescriptor, TargetFormat, TargetTrustState, ToolAdapter,
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

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError> {
        let environment = context.environment;
        let installed = environment.availability().codex;
        let capability = if installed {
            TargetCapability::supported()
        } else {
            TargetCapability::tool_not_installed()
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
            descriptor(
                ArtifactKind::Provider,
                Scope::Global,
                None,
                path_text(&config_path)?,
                TargetFormat::Toml,
                vec!["model", "model_provider", "model_providers"],
                vec!["model_providers/*/experimental_bearer_token"],
                capability.clone(),
                TargetTrustState::NotRequired,
                PromptOverrideState::NotApplicable,
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Prompt,
                Scope::Global,
                None,
                path_text(&environment.codex_home().join("AGENTS.md"))?,
                TargetFormat::Markdown,
                vec!["$document"],
                vec![],
                capability.clone(),
                TargetTrustState::NotRequired,
                prompt_override,
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Mcp,
                Scope::Global,
                None,
                path_text(&config_path)?,
                TargetFormat::Toml,
                vec!["mcp_servers"],
                vec![
                    "mcp_servers/*/http_headers",
                    "mcp_servers/*/env_http_headers",
                    "mcp_servers/*/env",
                ],
                capability.clone(),
                TargetTrustState::NotRequired,
                PromptOverrideState::NotApplicable,
                SymlinkPolicy::Reject,
            ),
            descriptor(
                ArtifactKind::Skill,
                Scope::Global,
                None,
                path_text(&environment.home().join(".agents/skills"))?,
                TargetFormat::SymlinkDirectory,
                vec!["$children"],
                vec![],
                capability.clone(),
                TargetTrustState::NotRequired,
                PromptOverrideState::NotApplicable,
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
                    path_text(&root.join(".codex/config.toml"))?,
                    TargetFormat::Toml,
                    vec!["mcp_servers"],
                    vec![
                        "mcp_servers/*/http_headers",
                        "mcp_servers/*/env_http_headers",
                        "mcp_servers/*/env",
                    ],
                    capability.clone(),
                    project_trust,
                    PromptOverrideState::NotApplicable,
                    SymlinkPolicy::Reject,
                ),
                descriptor(
                    ArtifactKind::Skill,
                    Scope::Project,
                    Some(project_root.as_str().to_owned()),
                    path_text(&root.join(".agents/skills"))?,
                    TargetFormat::SymlinkDirectory,
                    vec!["$children"],
                    vec![],
                    capability,
                    project_trust,
                    PromptOverrideState::NotApplicable,
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
    path: String,
    format: TargetFormat,
    managed_selector_roots: Vec<&str>,
    sensitive_selectors: Vec<&str>,
    capability: TargetCapability,
    trust: TargetTrustState,
    prompt_override: PromptOverrideState,
    symlink_policy: SymlinkPolicy,
) -> TargetDescriptor {
    TargetDescriptor {
        tool: Tool::Codex,
        artifact_kind,
        scope,
        project_root,
        path: Some(path),
        format,
        managed_selector_roots: managed_selector_roots
            .into_iter()
            .map(str::to_owned)
            .collect(),
        sensitive_selectors: sensitive_selectors.into_iter().map(str::to_owned).collect(),
        capability,
        policy: PolicyState::Allowed,
        trust,
        prompt_override,
        symlink_policy,
    }
}

fn discover_prompt_override(path: &Path) -> PromptOverrideState {
    match read_discovery_file(path) {
        DiscoveryFile::Missing => PromptOverrideState::NotPresent,
        DiscoveryFile::File(bytes) if bytes.is_empty() => PromptOverrideState::NotPresent,
        DiscoveryFile::File(_) => PromptOverrideState::Present,
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

fn path_text(path: &Path) -> Result<String, AppError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::invalid_input("targetPath", "目标路径必须是 UTF-8"))
}
