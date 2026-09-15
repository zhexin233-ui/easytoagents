use std::{fs, path::Path};

use serde_json::{json, Value};
use tempfile::tempdir;

use super::probe::{
    PI_MCP_ADAPTER_MISSING, PI_MCP_ADAPTER_NOT_LOADED, PI_MCP_ADAPTER_VERSION_UNSUPPORTED,
};
use super::{
    detect_mcp_shadowing, inline_api_key_diagnostic, managed_children_symlink_diagnostic,
    normalize_mcp_document, probe::probe_mcp_adapter, probe::PiMcpAdapterProbeInput,
    probe::PiMcpAdapterState, prompt_fallback_present, read_mcp_servers, PiAdapter,
    PiMcpShadowSource, PI_AGENT_DIR_OVERRIDE_UNMAPPED, PI_INSTALLATION_PROBE_UNSUPPORTED,
    PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED, PI_MCP_SHADOWED_BY_PROJECT_PI,
    PI_MCP_SHADOWED_BY_PROJECT_SHARED, PI_PROVIDER_INLINE_API_KEY, PI_SKILL_SYMLINK_BROKEN,
    PI_SKILL_SYMLINK_ESCAPE,
};
use crate::{
    adapters::{
        canonicalize_project_root, CapabilityState, ConservativeClaudeCustomizationPolicyProbe,
        ConservativeClaudeUserMcpProbe, DiscoveryContext, ExplicitEnvironment, ManagedOwnership,
        ObservedDocument, PromptOverrideState, ProviderCodec, SymlinkPolicy, TargetDescriptor,
        TargetFormat, TargetTrustState, ToolAdapter, ToolAvailability, ToolAvailabilityState,
    },
    domain::{ArtifactKind, ProjectRoot, Scope, Tool},
};

fn write_json(path: &Path, value: Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn write_adapter_package(agent_dir: &Path, version: &str) {
    write_json(
        &agent_dir
            .join("npm/node_modules/pi-mcp-adapter")
            .join("package.json"),
        json!({ "name": "pi-mcp-adapter", "version": version }),
    );
}

fn environment(home: &Path, state: ToolAvailabilityState) -> ExplicitEnvironment {
    ExplicitEnvironment::new(
        home,
        None,
        None,
        ToolAvailability::from_states([
            ToolAvailabilityState::Unavailable,
            ToolAvailabilityState::Unavailable,
            ToolAvailabilityState::Unavailable,
            ToolAvailabilityState::Unavailable,
            ToolAvailabilityState::Unavailable,
            state,
        ]),
    )
    .unwrap()
}

struct ContextHost {
    user_probe: ConservativeClaudeUserMcpProbe,
    policy_probe: ConservativeClaudeCustomizationPolicyProbe,
}

impl ContextHost {
    fn new() -> Self {
        Self {
            user_probe: ConservativeClaudeUserMcpProbe,
            policy_probe: ConservativeClaudeCustomizationPolicyProbe,
        }
    }

    fn context<'a>(
        &'a self,
        environment: &'a ExplicitEnvironment,
        project_root: Option<&'a ProjectRoot>,
    ) -> DiscoveryContext<'a> {
        DiscoveryContext {
            environment,
            project_root,
            claude_user_mcp_probe: &self.user_probe,
            claude_customization_policy_probe: &self.policy_probe,
        }
    }
}

fn find(targets: &[TargetDescriptor], kind: ArtifactKind, scope: Scope) -> &TargetDescriptor {
    targets
        .iter()
        .find(|target| target.artifact_kind == kind && target.scope == scope)
        .unwrap_or_else(|| panic!("missing descriptor {kind:?}/{scope:?}"))
}

fn project_root(home: &Path) -> ProjectRoot {
    let path = home.join("project");
    fs::create_dir_all(&path).unwrap();
    canonicalize_project_root(&path).unwrap()
}

fn ready_mcp_adapter(agent_dir: &Path) {
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    write_adapter_package(agent_dir, "2.33.0");
}

#[test]
fn mcp_adapter_probe_reports_missing_when_nothing_is_declared() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: None,
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::Missing);
    assert_eq!(
        probe.state.diagnostic_code(),
        Some("PI_MCP_ADAPTER_MISSING")
    );
    assert!(!probe.declared);
    assert_eq!(probe.version, None);
}

#[test]
fn mcp_adapter_probe_accepts_declared_and_installed_adapter() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    write_adapter_package(&agent_dir, "2.33.0");
    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: None,
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::Ready);
    assert!(probe.state.is_ready());
    assert!(probe.declared);
    assert_eq!(probe.version.as_deref(), Some("2.33.0"));
}

#[test]
fn mcp_adapter_probe_detects_object_form_and_disabled_extensions() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    // 对象形声明等价于字符串声明。
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "packages": [{ "source": "npm:pi-mcp-adapter" }] }),
    );
    write_adapter_package(&agent_dir, "2.33.0");
    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: None,
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::Ready);

    // `pi config` 过滤扩展（`extensions: []`）时即使已安装也不加载。
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "packages": [{ "source": "npm:pi-mcp-adapter", "extensions": [] }] }),
    );
    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: None,
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::NotLoaded);
    assert_eq!(
        probe.state.diagnostic_code(),
        Some("PI_MCP_ADAPTER_NOT_LOADED")
    );
    assert!(probe.declared);
}

#[test]
fn mcp_adapter_probe_rejects_unsupported_version() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    write_adapter_package(&agent_dir, "2.0.0");
    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: None,
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::VersionUnsupported);
    assert_eq!(
        probe.state.diagnostic_code(),
        Some("PI_MCP_ADAPTER_VERSION_UNSUPPORTED")
    );

    // 2.33.1 满足最低支持版本 2.33.0。
    write_adapter_package(&agent_dir, "2.33.1");
    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: None,
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::Ready);
}

#[test]
fn mcp_adapter_probe_project_scope_requires_trust_and_honours_exclusive_mode() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    // 项目新增置 settings.json + .pi/npm 安装；全局完全没有声明。
    write_json(
        &project.join(".pi/settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    write_json(
        &project.join(".pi/npm/node_modules/pi-mcp-adapter/package.json"),
        json!({ "name": "pi-mcp-adapter", "version": "2.33.0" }),
    );

    // 项目未受信任：项目 package 不会加载 → NOT_LOADED。
    let untrusted = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: Some(&project),
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(untrusted.state, PiMcpAdapterState::NotLoaded);

    // 受信任后项目级安装可用。
    let trusted = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: Some(&project),
        project_trusted: true,
        exclusive_mode: false,
    });
    assert_eq!(trusted.state, PiMcpAdapterState::Ready);

    // exclusive 模式只读全局文件：项目声明被忽略，且全局缺失 → MISSING。
    let exclusive = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: Some(&project),
        project_trusted: true,
        exclusive_mode: true,
    });
    assert_eq!(exclusive.state, PiMcpAdapterState::Missing);
}

#[test]
fn global_ready_adapter_is_not_overridden_by_project_trust_or_filtering() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    ready_mcp_adapter(&agent_dir);
    write_json(
        &project.join(".pi/settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    write_json(
        &project.join(".pi/npm/node_modules/pi-mcp-adapter/package.json"),
        json!({ "name": "pi-mcp-adapter", "version": "2.33.0" }),
    );

    let untrusted = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: Some(&project),
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(untrusted.state, PiMcpAdapterState::Ready);
    assert_eq!(untrusted.version.as_deref(), Some("2.33.0"));

    write_json(
        &project.join(".pi/settings.json"),
        json!({ "packages": [{ "source": "npm:pi-mcp-adapter", "extensions": [] }] }),
    );
    let filtered = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: Some(&project),
        project_trusted: true,
        exclusive_mode: false,
    });
    assert_eq!(filtered.state, PiMcpAdapterState::Ready);

    let exclusive = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: Some(&project),
        project_trusted: false,
        exclusive_mode: true,
    });
    assert_eq!(exclusive.state, PiMcpAdapterState::Ready);
}

#[test]
fn global_unsupported_version_does_not_override_project_filtering() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    write_adapter_package(&agent_dir, "2.0.0");
    write_json(
        &project.join(".pi/settings.json"),
        json!({ "packages": [{ "source": "npm:pi-mcp-adapter", "extensions": [] }] }),
    );

    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: Some(&project),
        project_trusted: true,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::NotLoaded);
    assert_eq!(probe.version.as_deref(), Some("2.0.0"));
}

#[test]
fn declared_but_not_installed_adapter_fails_closed() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    let probe = probe_mcp_adapter(&PiMcpAdapterProbeInput {
        pi_agent_dir: &agent_dir,
        project_root: None,
        project_trusted: false,
        exclusive_mode: false,
    });
    assert_eq!(probe.state, PiMcpAdapterState::Missing);
    assert!(probe.declared);
}

#[test]
fn pi_descriptor_capability_reflects_unmapped_override_and_probe_state() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let host = ContextHost::new();

    let installed_home = home.join("installed");
    fs::create_dir_all(&installed_home).unwrap();
    let installed = environment(&installed_home, ToolAvailabilityState::Installed);
    let context = host.context(&installed, None);
    assert!(PiAdapter::capability(&context).state == CapabilityState::Supported);

    let unavailable = environment(&home, ToolAvailabilityState::Unavailable);
    let context = host.context(&unavailable, None);
    assert!(PiAdapter::capability(&context).state == CapabilityState::ToolNotInstalled);

    let unsupported = environment(&home, ToolAvailabilityState::Unsupported);
    let context = host.context(&unsupported, None);
    let capability = PiAdapter::capability(&context);
    assert!(capability.state == CapabilityState::Unsupported);
    assert_eq!(
        capability.diagnostic_code.as_deref(),
        Some(PI_INSTALLATION_PROBE_UNSUPPORTED)
    );

    // `PI_CODING_AGENT_DIR` 不可映射优先于安装状态，且不暴露任何路径。
    let unmapped = unavailable.clone().with_pi_agent_dir_unmapped();
    let context = host.context(&unmapped, None);
    let capability = PiAdapter::capability(&context);
    assert!(capability.state == CapabilityState::Unsupported);
    assert_eq!(
        capability.diagnostic_code.as_deref(),
        Some(PI_AGENT_DIR_OVERRIDE_UNMAPPED)
    );
}

#[test]
fn pi_agent_dir_mapping_honours_tilde_and_rejects_relative_paths() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let environment = environment(&home, ToolAvailabilityState::Installed);
    assert_eq!(environment.pi_agent_dir(), home.join(".pi/agent"));
    assert!(!environment.pi_agent_dir_unmapped());

    let expanded = environment
        .clone()
        .with_pi_agent_dir("~/custom-agent")
        .unwrap();
    assert_eq!(expanded.pi_agent_dir(), home.join("custom-agent"));
    assert!(!expanded.pi_agent_dir_unmapped());

    // 相对路径无法可靠重现 → Err（调用方转为 PI_AGENT_DIR_OVERRIDE_UNMAPPED）。
    assert!(environment
        .clone()
        .with_pi_agent_dir("relative/agent")
        .is_err());
}

#[test]
fn descriptor_matrix_matches_the_frozen_six_target_surface() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    fs::create_dir_all(&agent_dir).unwrap();
    ready_mcp_adapter(&agent_dir);
    let project = project_root(&home);

    let environment = environment(&home, ToolAvailabilityState::Installed);
    let host = ContextHost::new();
    let targets =
        ToolAdapter::discover(&PiAdapter, &host.context(&environment, Some(&project))).unwrap();

    // 恰好六个 descriptor，且没有 Hook / Agent。
    assert_eq!(targets.len(), 6);
    assert!(targets.iter().all(|target| target.tool == Tool::Pi));
    assert!(targets.iter().all(|target| !matches!(
        target.artifact_kind,
        ArtifactKind::Hook | ArtifactKind::Agent
    )));

    let provider = find(&targets, ArtifactKind::Provider, Scope::Global);
    assert_eq!(
        provider.path.as_deref(),
        agent_dir.join("models.json").to_str()
    );
    assert_eq!(provider.format, TargetFormat::Json);
    assert_eq!(provider.managed_selector_roots, vec!["providers"]);
    assert_eq!(
        provider.sensitive_selectors,
        vec![
            "providers/*/apiKey",
            "providers/*/headers",
            "providers/*/models/*/headers",
            "providers/*/modelOverrides/*/headers"
        ]
    );
    assert_eq!(provider.symlink_policy, SymlinkPolicy::Reject);
    assert_eq!(provider.trust, TargetTrustState::NotRequired);
    assert_eq!(provider.allowed_root.as_deref(), agent_dir.to_str());
    assert_eq!(provider.capability.state, CapabilityState::Supported);

    let prompt = find(&targets, ArtifactKind::Prompt, Scope::Global);
    assert_eq!(prompt.path.as_deref(), agent_dir.join("AGENTS.md").to_str());
    assert_eq!(prompt.format, TargetFormat::Markdown);
    assert_eq!(prompt.managed_selector_roots, vec!["$document"]);
    assert_eq!(prompt.prompt_override, PromptOverrideState::NotPresent);

    let global_skill = find(&targets, ArtifactKind::Skill, Scope::Global);
    assert_eq!(
        global_skill.path.as_deref(),
        agent_dir.join("skills").to_str()
    );
    assert_eq!(global_skill.format, TargetFormat::SymlinkDirectory);
    assert_eq!(global_skill.managed_selector_roots, vec!["$children"]);
    assert_eq!(
        global_skill.symlink_policy,
        SymlinkPolicy::ManagedChildrenOnly
    );
    assert_eq!(global_skill.allowed_root.as_deref(), agent_dir.to_str());

    let global_mcp = find(&targets, ArtifactKind::Mcp, Scope::Global);
    assert_eq!(
        global_mcp.path.as_deref(),
        agent_dir.join("mcp.json").to_str()
    );
    assert_eq!(global_mcp.format, TargetFormat::Json);
    assert_eq!(global_mcp.managed_selector_roots, vec!["mcpServers"]);
    assert_eq!(
        global_mcp.mcp_container,
        Some(vec!["mcpServers".to_owned()])
    );
    assert_eq!(global_mcp.trust, TargetTrustState::NotRequired);
    assert_eq!(global_mcp.allowed_root.as_deref(), agent_dir.to_str());

    let project_skill = find(&targets, ArtifactKind::Skill, Scope::Project);
    assert_eq!(
        project_skill.path.as_deref(),
        Path::new(project.as_str()).join(".pi/skills").to_str()
    );
    assert_eq!(
        project_skill.allowed_root.as_deref(),
        Some(project.as_str())
    );
    assert_eq!(
        project_skill.symlink_policy,
        SymlinkPolicy::ManagedChildrenOnly
    );

    let project_mcp = find(&targets, ArtifactKind::Mcp, Scope::Project);
    assert_eq!(
        project_mcp.path.as_deref(),
        Path::new(project.as_str()).join(".pi/mcp.json").to_str()
    );
    assert_eq!(project_mcp.allowed_root.as_deref(), Some(project.as_str()));
    // `.pi/mcp.json` 不受 Pi project trust 门禁，如实标注 NotRequired。
    assert_eq!(project_mcp.trust, TargetTrustState::NotRequired);
    assert_eq!(
        project_mcp.mcp_container,
        Some(vec!["mcpServers".to_owned()])
    );
}

#[test]
fn global_ready_keeps_both_mcp_descriptors_available_for_untrusted_project_package() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    ready_mcp_adapter(&agent_dir);
    let project = project_root(&home);
    write_json(
        &Path::new(project.as_str()).join(".pi/settings.json"),
        json!({ "packages": ["npm:pi-mcp-adapter"] }),
    );
    write_json(
        &agent_dir.join("trust.json"),
        json!({ project.as_str(): false }),
    );
    let environment = environment(&home, ToolAvailabilityState::Installed);
    let host = ContextHost::new();
    let targets =
        ToolAdapter::discover(&PiAdapter, &host.context(&environment, Some(&project))).unwrap();
    for scope in [Scope::Global, Scope::Project] {
        let descriptor = find(&targets, ArtifactKind::Mcp, scope);
        assert_eq!(descriptor.capability.state, CapabilityState::Supported);
        assert!(descriptor.path.is_some());
    }
}

#[test]
fn no_hook_or_agent_descriptor_is_ever_generated() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    fs::create_dir_all(home.join(".pi/agent")).unwrap();
    let environment = environment(&home, ToolAvailabilityState::Installed);
    let host = ContextHost::new();
    let project = project_root(&home);
    let targets =
        ToolAdapter::discover(&PiAdapter, &host.context(&environment, Some(&project))).unwrap();
    assert!(targets.iter().all(|target| !matches!(
        target.artifact_kind,
        ArtifactKind::Hook | ArtifactKind::Agent
    )));
}

#[test]
fn mcp_descriptors_fail_closed_without_path_when_adapter_is_not_ready() {
    for (setup, expected) in [
        (None, PI_MCP_ADAPTER_MISSING),
        (Some("{}"), PI_MCP_ADAPTER_MISSING),
        (Some("filtered"), PI_MCP_ADAPTER_NOT_LOADED),
        (Some("old"), PI_MCP_ADAPTER_VERSION_UNSUPPORTED),
    ] {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let agent_dir = home.join(".pi/agent");
        match setup {
            None => {
                fs::create_dir_all(&agent_dir).unwrap();
            }
            Some("{}") => {
                write_json(&agent_dir.join("settings.json"), json!({ "packages": [] }));
            }
            Some("filtered") => {
                write_json(
                    &agent_dir.join("settings.json"),
                    json!({ "packages": [{ "source": "npm:pi-mcp-adapter", "extensions": [] }] }),
                );
                write_adapter_package(&agent_dir, "2.33.0");
            }
            Some("old") => {
                write_json(
                    &agent_dir.join("settings.json"),
                    json!({ "packages": ["npm:pi-mcp-adapter"] }),
                );
                write_adapter_package(&agent_dir, "0.1.0");
            }
            _ => unreachable!(),
        }
        let environment = environment(&home, ToolAvailabilityState::Installed);
        let host = ContextHost::new();
        let targets = ToolAdapter::discover(&PiAdapter, &host.context(&environment, None)).unwrap();
        for scope in [Scope::Global, Scope::Project] {
            let mcp = targets
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Mcp && target.scope == scope);
            // 没有项目根时只检查全局；显式项目检查在下方单独覆盖。
            if scope == Scope::Project && mcp.is_none() {
                continue;
            }
            let mcp = mcp.unwrap();
            assert_eq!(mcp.capability.state, CapabilityState::Unsupported);
            assert_eq!(mcp.capability.diagnostic_code.as_deref(), Some(expected));
            assert_eq!(mcp.path, None);
        }

        // Provider / Prompt / Skills 仍保持可用路径（只有 MCP 前置失败）。
        let provider = find(&targets, ArtifactKind::Provider, Scope::Global);
        assert_eq!(provider.capability.state, CapabilityState::Supported);
        assert!(provider.path.is_some());
        let prompt = find(&targets, ArtifactKind::Prompt, Scope::Global);
        assert!(prompt.path.is_some());
        let skill = find(&targets, ArtifactKind::Skill, Scope::Global);
        assert!(skill.path.is_some());
    }
}

#[test]
fn exclusive_mode_only_disables_project_mcp() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    ready_mcp_adapter(&agent_dir);
    let project = project_root(&home);
    let environment =
        environment(&home, ToolAvailabilityState::Installed).with_pi_mcp_exclusive_mode(true);
    let host = ContextHost::new();
    let targets =
        ToolAdapter::discover(&PiAdapter, &host.context(&environment, Some(&project))).unwrap();

    let project_mcp = find(&targets, ArtifactKind::Mcp, Scope::Project);
    assert_eq!(project_mcp.capability.state, CapabilityState::Unsupported);
    assert_eq!(
        project_mcp.capability.diagnostic_code.as_deref(),
        Some(PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED)
    );
    assert_eq!(project_mcp.path, None);

    // 全局 MCP 不受 exclusive 影响（适配器仍读 Pi 自有全局文件）。
    let global_mcp = find(&targets, ArtifactKind::Mcp, Scope::Global);
    assert_eq!(global_mcp.capability.state, CapabilityState::Supported);
    assert!(global_mcp.path.is_some());
}

#[test]
fn unmapped_override_makes_all_six_descriptors_unsupported_and_pathless() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let project = project_root(&home);
    let environment =
        environment(&home, ToolAvailabilityState::Installed).with_pi_agent_dir_unmapped();
    let host = ContextHost::new();
    let targets =
        ToolAdapter::discover(&PiAdapter, &host.context(&environment, Some(&project))).unwrap();
    assert_eq!(targets.len(), 6);
    for target in &targets {
        assert_eq!(target.capability.state, CapabilityState::Unsupported);
        assert_eq!(
            target.capability.diagnostic_code.as_deref(),
            Some(PI_AGENT_DIR_OVERRIDE_UNMAPPED)
        );
        // 不可映射时不得暴露默认路径。
        assert_eq!(target.path, None);
    }
}

#[test]
fn prompt_override_three_states_and_fallback_diagnostics() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let environment = environment(&home, ToolAvailabilityState::Installed);
    let host = ContextHost::new();

    let prompt_override = |environment: &ExplicitEnvironment| {
        let targets = ToolAdapter::discover(&PiAdapter, &host.context(environment, None)).unwrap();
        find(&targets, ArtifactKind::Prompt, Scope::Global).prompt_override
    };

    // NotPresent：文件缺失。
    assert_eq!(
        prompt_override(&environment),
        PromptOverrideState::NotPresent
    );

    // Present：非空文件。
    fs::write(agent_dir.join("AGENTS.override.md"), "override").unwrap();
    assert_eq!(prompt_override(&environment), PromptOverrideState::Present);

    // 空文件等同缺失（Codex 先例）。
    fs::write(agent_dir.join("AGENTS.override.md"), "   \n").unwrap();
    assert_eq!(
        prompt_override(&environment),
        PromptOverrideState::NotPresent
    );

    // Unknown：符号链接无法安全读取。
    fs::remove_file(agent_dir.join("AGENTS.override.md")).unwrap();
    std::os::unix::fs::symlink(
        agent_dir.join("AGENTS.md"),
        agent_dir.join("AGENTS.override.md"),
    )
    .unwrap();
    assert_eq!(prompt_override(&environment), PromptOverrideState::Unknown);

    // 回退文件诊断：AGENTS.md 缺失 + CLAUDE.md 存在。
    let fallback_dir = home.join("fallback-agent");
    fs::create_dir_all(&fallback_dir).unwrap();
    assert!(!prompt_fallback_present(&fallback_dir));
    fs::write(fallback_dir.join("CLAUDE.md"), "user prompt").unwrap();
    assert!(prompt_fallback_present(&fallback_dir));
    fs::write(fallback_dir.join("AGENTS.md"), "managed").unwrap();
    assert!(!prompt_fallback_present(&fallback_dir));
}

#[test]
fn project_trust_states_are_read_only_and_fail_closed() {
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let project = project_root(&home);
    let environment = environment(&home, ToolAvailabilityState::Installed);
    let host = ContextHost::new();

    let trust = |environment: &ExplicitEnvironment| {
        let targets =
            ToolAdapter::discover(&PiAdapter, &host.context(environment, Some(&project))).unwrap();
        find(&targets, ArtifactKind::Skill, Scope::Project).trust
    };

    // 无 trust.json，settings.json 缺省 ask → Unknown（保守，阻断 Apply）。
    assert_eq!(trust(&environment), TargetTrustState::Unknown);

    // defaultProjectTrust=always / never。
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "defaultProjectTrust": "always" }),
    );
    assert_eq!(trust(&environment), TargetTrustState::Trusted);
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "defaultProjectTrust": "never" }),
    );
    assert_eq!(trust(&environment), TargetTrustState::Untrusted);

    // trust.json 最近祖先决定优先于 defaultProjectTrust。
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "defaultProjectTrust": "always" }),
    );
    write_json(
        &agent_dir.join("trust.json"),
        json!({ project.as_str(): false }),
    );
    assert_eq!(trust(&environment), TargetTrustState::Untrusted);
    write_json(
        &agent_dir.join("trust.json"),
        json!({ project.as_str(): true }),
    );
    assert_eq!(trust(&environment), TargetTrustState::Trusted);

    // 祖先目录条目生效。
    write_json(
        &agent_dir.join("trust.json"),
        json!({ home.to_str().unwrap(): true }),
    );
    assert_eq!(trust(&environment), TargetTrustState::Trusted);

    // 损坏的 trust.json → Unknown（不得回退到 defaultProjectTrust=always）。
    fs::write(agent_dir.join("trust.json"), "{ not json").unwrap();
    assert_eq!(trust(&environment), TargetTrustState::Unknown);
}

#[test]
fn mcp_container_alias_is_read_but_never_written() {
    let canonical = json!({ "mcpServers": { "one": { "url": "https://example.test" } } });
    let observed = read_mcp_servers(&canonical).unwrap().unwrap();
    assert!(!observed.alias_used);
    assert_eq!(observed.diagnostic_code(), None);
    assert!(observed.servers.contains_key("one"));

    let aliased = json!({ "mcp-servers": { "two": { "command": "echo" } } });
    let observed = read_mcp_servers(&aliased).unwrap().unwrap();
    assert!(observed.alias_used);
    assert_eq!(
        observed.diagnostic_code(),
        Some("PI_MCP_CONTAINER_ALIAS_DETECTED")
    );
    assert!(observed.servers.contains_key("two"));

    assert!(read_mcp_servers(&json!({ "imports": [] }))
        .unwrap()
        .is_none());

    let both = json!({
        "mcpServers": { "canonical": { "command": "canonical" } },
        "mcp-servers": { "alias": { "command": "ignored" } },
        "future": { "keep": true }
    });
    let observed = read_mcp_servers(&both).unwrap().unwrap();
    assert!(!observed.alias_used);
    assert!(observed.servers.contains_key("canonical"));
    assert!(!observed.servers.contains_key("alias"));
    let (normalized, alias_used) = normalize_mcp_document(&both).unwrap();
    assert!(!alias_used);
    assert!(normalized.get("mcp-servers").is_none());
    assert_eq!(normalized["future"]["keep"], true);
    assert!(normalized["mcpServers"].get("alias").is_none());

    assert!(read_mcp_servers(&json!({ "mcpServers": [] })).is_err());
    assert!(read_mcp_servers(&json!({ "mcp-servers": [] })).is_err());
}

#[test]
fn mcp_alias_projection_and_render_normalize_without_losing_unknown_fields() {
    let adapter = PiAdapter;
    let descriptor = TargetDescriptor::builder(Tool::Pi, ArtifactKind::Mcp, Scope::Global)
        .format(TargetFormat::Json)
        .managed_selectors(["mcpServers"])
        .build();
    let ownership =
        ManagedOwnership::selectors([["mcpServers", "managed"], ["mcpServers", "removed"]]);
    let current = ObservedDocument::Json(json!({
        "mcp-servers": {
            "managed": { "command": "old", "unknown": "preserve" },
            "removed": { "command": "remove" },
            "external": { "command": "keep" }
        },
        "future": { "keep": true }
    }));
    let projected = adapter.project_managed(&current, &ownership).unwrap();
    assert_eq!(projected["mcpServers"]["managed"]["unknown"], "preserve");

    let desired = json!({
        "mcpServers": {
            "managed": { "command": "new", "unknown": "preserve" }
        }
    });
    let rendered =
        ToolAdapter::render(&adapter, &descriptor, Some(&current), &desired, &ownership).unwrap();
    let crate::adapters::RenderedTarget::File(bytes) = rendered;
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value.get("mcp-servers").is_none());
    assert_eq!(value["mcpServers"]["managed"]["command"], "new");
    assert_eq!(value["mcpServers"]["managed"]["unknown"], "preserve");
    assert!(value["mcpServers"].get("removed").is_none());
    assert_eq!(value["mcpServers"]["external"]["command"], "keep");
    assert_eq!(value["future"]["keep"], true);
}

#[test]
fn mcp_shadowing_detection_blocks_only_global_managed_names() {
    let temporary = tempdir().unwrap();
    let project = fs::canonicalize(temporary.path()).unwrap();
    fs::create_dir_all(project.join(".pi")).unwrap();

    // 无遮蔽：DEFAULT 未被任何项目文件定义。
    assert_eq!(
        detect_mcp_shadowing(&project, &["safe".to_owned()], Scope::Global),
        None
    );

    // `<root>/.mcp.json` 同名 → 共享文件遮蔽优先。
    write_json(
        &project.join(".mcp.json"),
        json!({ "mcpServers": { "github": { "command": "shared" } } }),
    );
    let shadowing = detect_mcp_shadowing(&project, &["github".to_owned()], Scope::Global).unwrap();
    assert_eq!(shadowing.source, PiMcpShadowSource::ProjectShared);
    assert_eq!(
        shadowing.source.diagnostic_code(),
        PI_MCP_SHADOWED_BY_PROJECT_SHARED
    );

    // 用户手写 `.pi/mcp.json` 同名 → ProjectPi。
    write_json(
        &project.join(".pi/mcp.json"),
        json!({ "mcpServers": { "github": { "url": "https://pi.test" } } }),
    );
    let shadowing = detect_mcp_shadowing(&project, &["github".to_owned()], Scope::Global).unwrap();
    assert_eq!(shadowing.source, PiMcpShadowSource::ProjectShared);
    write_json(&project.join(".mcp.json"), json!({ "mcpServers": {} }));
    let shadowing = detect_mcp_shadowing(&project, &["github".to_owned()], Scope::Global).unwrap();
    assert_eq!(shadowing.source, PiMcpShadowSource::ProjectPi);
    assert_eq!(
        shadowing.source.diagnostic_code(),
        PI_MCP_SHADOWED_BY_PROJECT_PI
    );

    // 项目目标写入 `.pi/mcp.json`，是合并链最高优先级，永不判为遮蔽。
    assert_eq!(
        detect_mcp_shadowing(&project, &["github".to_owned()], Scope::Project),
        None
    );
}

#[test]
fn provider_codec_renders_partial_entry_and_preserves_unknown_fields() {
    let adapter = PiAdapter;
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("api".to_owned(), json!("openai-completions"));
    extra.insert("headers".to_owned(), json!({ "X-Trace": "$ENV" }));
    extra.insert(
        "modelOverrides".to_owned(),
        json!({ "m": { "headers": { "A": "!cmd" } } }),
    );
    let extra_env = std::collections::BTreeMap::new();
    let input = crate::adapters::ProviderCodecProfileInput {
        name: "Pi 自定义渠道",
        auth_kind: crate::adapters::PROVIDER_AUTH_KIND_API_KEY,
        api_base_url: Some("https://api.example.test"),
        api_key: Some("!echo secret"),
        default_model: Some("deepseek-v4"),
        credential_env_key: None,
        extra_env: &extra_env,
        provider_id: Some("custom"),
        wire_api: None,
        zcode_kind: None,
        opencode_npm: None,
        opencode_api: None,
        extra_provider_fields: &extra,
    };
    let rendered = ProviderCodec::render(&adapter, &input).unwrap();
    let entry = &rendered["providers"]["custom"];
    assert_eq!(entry["baseUrl"], "https://api.example.test");
    // `!command` 原样保留，不展开不执行。
    assert_eq!(entry["apiKey"], "!echo secret");
    assert_eq!(entry["api"], "openai-completions");
    assert_eq!(entry["headers"]["X-Trace"], "$ENV");
    assert_eq!(entry["modelOverrides"]["m"]["headers"]["A"], "!cmd");
    assert_eq!(entry["models"], json!([{ "id": "deepseek-v4" }]));

    // 无默认模型时不得写 `models`（也不得写空数组）。
    let input = crate::adapters::ProviderCodecProfileInput {
        default_model: None,
        ..input
    };
    let rendered = ProviderCodec::render(&adapter, &input).unwrap();
    assert!(rendered["providers"]["custom"].get("models").is_none());

    // credential_env_key 渲染为 `$KEY` 引用。
    let input = crate::adapters::ProviderCodecProfileInput {
        api_key: None,
        credential_env_key: Some("MY_KEY"),
        ..input
    };
    let rendered = ProviderCodec::render(&adapter, &input).unwrap();
    assert_eq!(rendered["providers"]["custom"]["apiKey"], "$MY_KEY");
}

#[test]
fn provider_render_keeps_existing_builtin_models_when_desired_has_none() {
    let adapter = PiAdapter;
    // 只给 baseUrl 的 desired 投影；ownership 只拥有 desired 中的 key。
    let desired = json!({ "providers": { "p": { "baseUrl": "https://new.test" } } });
    let ownership = adapter.ownership(None, &desired).unwrap();
    let current = ObservedDocument::Json(json!({
        "providers": {
            "p": {
                "baseUrl": "https://old.test",
                "headers": { "Keep": "me" },
                "models": [{ "id": "builtin", "name": "Built-in" }]
            },
            "other": { "baseUrl": "https://other.test" }
        }
    }));
    let descriptor = TargetDescriptor::builder(Tool::Pi, ArtifactKind::Provider, Scope::Global)
        .format(TargetFormat::Json)
        .managed_selectors(["providers"])
        .build();
    let rendered =
        ToolAdapter::render(&adapter, &descriptor, Some(&current), &desired, &ownership).unwrap();
    let crate::adapters::RenderedTarget::File(bytes) = rendered;
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["providers"]["p"]["baseUrl"], "https://new.test");
    // 未受管字段与既有 models 数组必须保留，绝不写成 `models: []`。
    assert_eq!(value["providers"]["p"]["headers"]["Keep"], "me");
    assert_eq!(value["providers"]["p"]["models"][0]["id"], "builtin");
    assert_eq!(value["providers"]["other"]["baseUrl"], "https://other.test");
}

#[test]
fn provider_ownership_tracks_only_declared_entry_keys() {
    let adapter = PiAdapter;
    let baseline = json!({ "providers": { "p": { "baseUrl": "u", "models": [{ "id": "m" }] } } });
    let desired = json!({ "providers": { "p": { "baseUrl": "u", "apiKey": "$KEY" } } });
    let ManagedOwnership::Selectors(selectors) =
        adapter.ownership(Some(&baseline), &desired).unwrap()
    else {
        panic!("expected selectors");
    };
    let selectors = selectors
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(selectors.contains(&vec![
        "providers".to_owned(),
        "p".to_owned(),
        "baseUrl".to_owned()
    ]));
    assert!(selectors.contains(&vec![
        "providers".to_owned(),
        "p".to_owned(),
        "apiKey".to_owned()
    ]));
    // baseline 中的 models 仍被拥有（切换档案时可移除旧受管数组）。
    assert!(selectors.contains(&vec![
        "providers".to_owned(),
        "p".to_owned(),
        "models".to_owned()
    ]));

    assert!(adapter.ownership(None, &json!({})).is_err());
}

#[test]
fn provider_discover_selects_default_provider_and_preserves_extra_fields() {
    let adapter = PiAdapter;
    let temporary = tempdir().unwrap();
    let home = fs::canonicalize(temporary.path()).unwrap();
    let agent_dir = home.join(".pi/agent");
    write_json(
        &agent_dir.join("settings.json"),
        json!({ "defaultProvider": "cc", "defaultModel": "cc/deepseek-v4" }),
    );
    let descriptor = TargetDescriptor::builder(Tool::Pi, ArtifactKind::Provider, Scope::Global)
        .path(Some(
            agent_dir.join("models.json").to_str().unwrap().to_owned(),
        ))
        .format(TargetFormat::Json)
        .managed_selectors(["providers"])
        .build();
    let projection = json!({
        "providers": {
            "cc": {
                "baseUrl": "https://api.example.test",
                "api": "openai-completions",
                "apiKey": "$MY_KEY",
                "headers": { "X": "1" },
                "models": [{ "id": "deepseek-v4" }, { "id": "second" }]
            },
            "other": { "baseUrl": "https://other.test" }
        }
    });
    let discovered = ProviderCodec::discover(&adapter, &descriptor, &projection, "hash")
        .unwrap()
        .unwrap();
    assert_eq!(discovered.provider_id.as_deref(), Some("cc"));
    assert_eq!(discovered.api_base_url, "https://api.example.test");
    assert_eq!(discovered.api_key.as_deref(), Some("$MY_KEY"));
    assert_eq!(discovered.default_model, "deepseek-v4");
    assert_eq!(
        discovered.auth_kind,
        crate::adapters::PROVIDER_AUTH_KIND_API_KEY
    );
    assert_eq!(
        discovered.extra_provider_fields.get("api"),
        Some(&json!("openai-completions"))
    );
    assert_eq!(
        discovered.extra_provider_fields.get("headers"),
        Some(&json!({ "X": "1" }))
    );
    assert!(!discovered.extra_provider_fields.contains_key("baseUrl"));
    assert!(!discovered.extra_provider_fields.contains_key("apiKey"));
    assert!(!discovered.extra_provider_fields.contains_key("models"));

    // 多 provider 且无 defaultProvider → 歧义，fail closed。
    write_json(&agent_dir.join("settings.json"), json!({}));
    assert!(
        ProviderCodec::discover(&adapter, &descriptor, &projection, "hash")
            .unwrap()
            .is_none()
    );
}

#[test]
fn inline_api_key_diagnostic_flags_only_literals() {
    assert_eq!(
        inline_api_key_diagnostic("sk-live-123"),
        Some(PI_PROVIDER_INLINE_API_KEY)
    );
    assert_eq!(inline_api_key_diagnostic("$MY_KEY"), None);
    assert_eq!(inline_api_key_diagnostic("!echo hi"), None);
    assert_eq!(inline_api_key_diagnostic("   "), None);
}

#[test]
fn managed_children_symlink_self_check_reports_broken_and_escape() {
    let temporary = tempdir().unwrap();
    let root = fs::canonicalize(temporary.path()).unwrap();
    let allowed = root.join("allowed");
    let skills = allowed.join("skills");
    fs::create_dir_all(&skills).unwrap();
    let central = allowed.join("central/one");
    fs::create_dir_all(&central).unwrap();
    std::os::unix::fs::symlink(&central, skills.join("one")).unwrap();
    std::os::unix::fs::symlink(allowed.join("missing"), skills.join("broken")).unwrap();
    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, skills.join("escape")).unwrap();

    let check =
        |name: &str| managed_children_symlink_diagnostic(&skills, &[name.to_owned()], &allowed);
    assert_eq!(check("one"), None);
    assert_eq!(check("absent"), None);
    assert_eq!(check("broken"), Some(PI_SKILL_SYMLINK_BROKEN));
    assert_eq!(check("escape"), Some(PI_SKILL_SYMLINK_ESCAPE));
}
