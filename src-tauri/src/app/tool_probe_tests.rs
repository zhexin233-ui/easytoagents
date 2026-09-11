#[cfg(test)]
mod tests {
    use std::{
        ffi::{CString, OsString},
        fs,
        os::unix::{
            ffi::OsStrExt,
            fs::{symlink, PermissionsExt},
        },
        path::{Path, PathBuf},
        sync::{Mutex, MutexGuard},
        time::Duration,
    };

    use tempfile::tempdir;

    use crate::{
        adapters::{
            claude::ClaudeAdapter, codex::CodexAdapter, CapabilityState, ClaudeCustomizationPolicy,
            ClaudeCustomizationPolicyProbeInput, DiscoveryContext, PolicyState, ToolAdapter,
            ToolAvailabilityState,
        },
        domain::{ArtifactKind, Scope, Tool},
    };

    use super::{probe_release_environment, ReleaseToolProbeInput, ReleaseToolProbeResult};

    // 这些用例会 fork 带独立进程组、后台后代和 pipe 的 shell fixture；并行运行会让
    // EOF/超时断言彼此干扰。release setup 本身只串行执行一次探针，因此测试也显式
    // 隔离这些进程 fixture，且不通过修改全局 PATH/HOME 来实现隔离。
    static PROCESS_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

    fn isolate_process_fixture() -> MutexGuard<'static, ()> {
        PROCESS_FIXTURE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn version_outputs_are_strictly_parsed() {
        assert_eq!(
            super::ToolBinary::Claude.parse_version(b"2.1.217 (Claude Code)", b""),
            Some("2.1.217".to_owned())
        );
        assert_eq!(
            super::ToolBinary::Codex.parse_version(b"codex-cli 0.114.0", b""),
            Some("0.114.0".to_owned())
        );
        assert_eq!(
            super::ToolBinary::CursorAgent.parse_version(b"Cursor Agent 1.7.54", b""),
            Some("1.7.54".to_owned())
        );
        assert_eq!(
            super::ToolBinary::CursorAgent.parse_version(b"Cursor Agent 1.preview", b""),
            None
        );
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        home: PathBuf,
        bin: PathBuf,
        claude_root: PathBuf,
        codex_root: PathBuf,
        policy: PathBuf,
        policy_directory: PathBuf,
        hold_fifo: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            let bin = root.join("bin");
            let claude_root = home.join(".claude");
            let codex_root = home.join(".codex");
            let policy_root = root.join("official-managed-settings");
            let policy = policy_root.join("managed-settings.json");
            let policy_directory = policy_root.join("managed-settings.d");
            let hold_fifo = root.join("hold-open.fifo");
            for directory in [
                &home,
                &bin,
                &claude_root,
                &codex_root,
                &policy_root,
                &policy_directory,
            ] {
                fs::create_dir_all(directory).unwrap();
            }
            let hold_fifo_path = CString::new(hold_fifo.as_os_str().as_bytes()).unwrap();
            // SAFETY: 路径来自隔离 tempfile 且以 NUL 结尾；fixture 负责其生命周期。
            assert_eq!(unsafe { libc::mkfifo(hold_fifo_path.as_ptr(), 0o600) }, 0);
            Self {
                _temporary: temporary,
                home,
                bin,
                claude_root,
                codex_root,
                policy,
                policy_directory,
                hold_fifo,
            }
        }

        fn write_tool(&self, name: &str, body: &str) {
            write_executable(&self.bin.join(name), body);
        }

        fn write_cursor_app(&self, bundle_id: &str, version: &str) -> PathBuf {
            self.write_desktop_app("Cursor.app", bundle_id, version)
        }

        fn write_zcode_app(&self, bundle_id: &str, version: &str) -> PathBuf {
            self.write_desktop_app("ZCode.app", bundle_id, version)
        }

        fn write_desktop_app(&self, name: &str, bundle_id: &str, version: &str) -> PathBuf {
            let app = self.home.join("Applications").join(name);
            fs::create_dir_all(app.join("Contents")).unwrap();
            fs::write(
                app.join("Contents/Info.plist"),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{bundle_id}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
</dict></plist>"#
                ),
            )
            .unwrap();
            app
        }

        fn input(&self) -> ReleaseToolProbeInput {
            ReleaseToolProbeInput {
                home: self.home.clone(),
                claude_config_dir: Some(self.claude_root.clone()),
                codex_home: Some(self.codex_root.clone()),
                opencode_config_dir: None,
                opencode_config_path: None,
                opencode_config_content: None,
                opencode_disabled: false,
                search_path: self.bin.clone().into_os_string(),
                timeout: Duration::from_secs(3),
                claude_managed_settings_path: self.policy.clone(),
                claude_managed_settings_directory: self.policy_directory.clone(),
                cursor_app_paths: vec![self.home.join("Applications/Cursor.app")],
                zcode_app_paths: vec![self.home.join("Applications/ZCode.app")],
            }
        }

        fn pipe_holding_descendant(&self, parent_action: &str) -> String {
            let fifo = self.hold_fifo.to_str().unwrap();
            assert!(!fifo.contains('\''));
            format!(
                "if [ \"$1\" = child ]; then read ignored < '{fifo}'; fi\n\"$0\" child &\n{parent_action}"
            )
        }
    }

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn current_customization_policy(result: &ReleaseToolProbeResult) -> ClaudeCustomizationPolicy {
        result
            .environment
            .claude_customization_policy_probe()
            .probe(&ClaudeCustomizationPolicyProbeInput {
                installation_version: result.environment.claude_installation_version(),
                claude_config_dir: result.environment.claude_config_dir(),
                source_path: result.environment.claude_customization_policy_source_path(),
                tool_installed: result.claude.state == ToolAvailabilityState::Installed,
            })
    }

    fn assert_release_policy_unknown(input: &ReleaseToolProbeInput) {
        let result = probe_release_environment(input).unwrap();
        assert_eq!(
            current_customization_policy(&result),
            ClaudeCustomizationPolicy::unknown()
        );
    }

    #[test]
    fn installed_versions_and_explicit_allowed_policy_are_bound_once() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        fs::write(
            &fixture.policy,
            r#"{"strictPluginOnlyCustomization":false}"#,
        )
        .unwrap();

        let input = fixture.input();
        let environment = crate::adapters::ExplicitEnvironment::new(
            &fixture.home,
            Some(fixture.claude_root.clone()),
            Some(fixture.codex_root.clone()),
            crate::adapters::ToolAvailability::all_unavailable(),
        )
        .unwrap();
        let executable = match super::resolve_executable(&input.search_path, "claude") {
            super::ExecutableResolution::Found { path, .. } => path,
            _ => panic!("fake Claude 可执行文件未被安全解析"),
        };
        let output = super::run_version_command(&executable, &environment, &input).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"2.1.217 (Claude Code)");
        assert!(output.stderr.is_empty());

        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Installed);
        assert_eq!(result.claude.version.as_deref(), Some("2.1.217"));
        assert_eq!(result.codex.state, ToolAvailabilityState::Installed);
        assert_eq!(result.codex.version.as_deref(), Some("0.114.0"));
        assert_eq!(
            result.environment.codex_installation_version(),
            Some("0.114.0")
        );
        assert_eq!(
            result.environment.claude_customization_policy_source_path(),
            Some(fixture.policy.as_path())
        );

        let context = DiscoveryContext {
            environment: &result.environment,
            project_root: None,
            claude_user_mcp_probe: result.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: result
                .environment
                .claude_customization_policy_probe(),
        };
        let targets = ClaudeAdapter.discover(&context).unwrap();
        let mcp = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        let skill = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Skill)
            .unwrap();
        assert_eq!(mcp.capability.state, CapabilityState::Supported);
        assert_eq!(
            mcp.path.as_deref(),
            fixture.home.join(".claude.json").to_str()
        );
        assert_eq!(mcp.policy, PolicyState::Allowed);
        assert_eq!(skill.policy, PolicyState::Allowed);
        assert_eq!(
            result
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.218"),
                    claude_config_dir: result.environment.claude_config_dir(),
                    source_path: result.environment.claude_customization_policy_source_path(),
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            result
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: result.environment.claude_config_dir(),
                    source_path: None,
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            result
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: &fixture.codex_root,
                    source_path: result.environment.claude_customization_policy_source_path(),
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
    }

    #[test]
    fn cursor_desktop_bundle_is_primary_and_does_not_require_agent_cli() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.7.54");
        let marker = fixture.home.join("agent-was-executed");
        let marker_text = marker.to_str().unwrap();
        assert!(!marker_text.contains('\''));
        fixture.write_tool(
            "agent",
            &format!("touch '{marker_text}'\nprintf 'Cursor Agent 9.9.9'"),
        );

        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Installed);
        assert_eq!(result.cursor.version.as_deref(), Some("1.7.54"));
        assert!(!marker.exists(), "Desktop 已确认时不应再执行补充 CLI");
        assert_eq!(
            result.environment.cursor_installation_version(),
            Some("1.7.54")
        );
    }

    #[test]
    fn cursor_agent_is_only_a_fallback_and_invalid_bundle_fails_closed() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("agent", "printf 'Cursor Agent 2.3.4'");
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Installed);
        assert_eq!(result.cursor.version.as_deref(), Some("2.3.4"));

        let invalid = Fixture::new();
        invalid.write_cursor_app("com.example.not-cursor", "1.7.54");
        let result = probe_release_environment(&invalid.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);
        assert!(result.cursor.version.is_none());
    }

    #[test]
    fn zcode_desktop_bundle_is_the_only_installation_evidence() {
        let _process_fixture = isolate_process_fixture();

        let missing = Fixture::new();
        let result = probe_release_environment(&missing.input()).unwrap();
        assert_eq!(result.zcode.state, ToolAvailabilityState::Unavailable);
        assert_eq!(result.environment.zcode_installation_version(), None);

        let installed = Fixture::new();
        installed.write_zcode_app(super::ZCODE_BUNDLE_ID, "3.11.2");
        let result = probe_release_environment(&installed.input()).unwrap();
        assert_eq!(result.zcode.state, ToolAvailabilityState::Installed);
        assert_eq!(result.zcode.version.as_deref(), Some("3.11.2"));
        assert_eq!(
            result.environment.zcode_installation_version(),
            Some("3.11.2")
        );

        let wrong_bundle = Fixture::new();
        wrong_bundle.write_zcode_app("com.example.not-zcode", "3.11.2");
        let result = probe_release_environment(&wrong_bundle.input()).unwrap();
        assert_eq!(result.zcode.state, ToolAvailabilityState::Unsupported);

        let malformed = Fixture::new();
        malformed.write_zcode_app(super::ZCODE_BUNDLE_ID, "3.preview");
        let result = probe_release_environment(&malformed.input()).unwrap();
        assert_eq!(result.zcode.state, ToolAvailabilityState::Unsupported);
    }

    #[test]
    fn cursor_desktop_rejects_malformed_versions_symlinked_apps_and_oversized_plists() {
        let _process_fixture = isolate_process_fixture();

        let malformed = Fixture::new();
        malformed.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.preview");
        let result = probe_release_environment(&malformed.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);

        let linked = Fixture::new();
        let app = linked.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.7.54");
        let real_app = linked.home.join("Applications/Cursor-real.app");
        fs::rename(&app, &real_app).unwrap();
        symlink(&real_app, &app).unwrap();
        let result = probe_release_environment(&linked.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);

        let oversized = Fixture::new();
        let app = oversized.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.7.54");
        fs::write(
            app.join("Contents/Info.plist"),
            vec![b'x'; super::MAX_PLIST_BYTES as usize + 1],
        )
        .unwrap();
        let result = probe_release_environment(&oversized.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);
    }

    #[test]
    fn missing_or_unconfigured_policy_sources_are_allowed_and_evidence_stays_bound() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");

        let empty_directory = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(
            current_customization_policy(&empty_directory),
            ClaudeCustomizationPolicy {
                mcp: PolicyState::Allowed,
                skill: PolicyState::Allowed,
            }
        );
        assert!(empty_directory
            .environment
            .claude_customization_policy_source_path()
            .is_none());

        fs::remove_dir(&fixture.policy_directory).unwrap();
        let missing_directory = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(
            current_customization_policy(&missing_directory),
            ClaudeCustomizationPolicy {
                mcp: PolicyState::Allowed,
                skill: PolicyState::Allowed,
            }
        );
        assert_eq!(
            missing_directory
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.218"),
                    claude_config_dir: missing_directory.environment.claude_config_dir(),
                    source_path: None,
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            missing_directory
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: &fixture.codex_root,
                    source_path: None,
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            missing_directory
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: missing_directory.environment.claude_config_dir(),
                    source_path: Some(&fixture.policy),
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );

        fs::create_dir(&fixture.policy_directory).unwrap();
        fs::write(&fixture.policy, "{}").unwrap();
        let undeclared_setting = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(
            current_customization_policy(&undeclared_setting),
            ClaudeCustomizationPolicy {
                mcp: PolicyState::Allowed,
                skill: PolicyState::Allowed,
            }
        );
        assert_eq!(
            undeclared_setting
                .environment
                .claude_customization_policy_source_path(),
            Some(fixture.policy.as_path())
        );
    }

    #[test]
    fn explicit_policy_values_keep_surface_rules_while_dynamic_or_invalid_values_are_unknown() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");

        for (document, expected) in [
            (
                r#"{"strictPluginOnlyCustomization":false}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Allowed,
                    skill: PolicyState::Allowed,
                },
            ),
            (
                r#"{"strictPluginOnlyCustomization":true}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Blocked,
                    skill: PolicyState::Blocked,
                },
            ),
            (
                r#"{"strictPluginOnlyCustomization":["mcp"]}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Blocked,
                    skill: PolicyState::Allowed,
                },
            ),
            (
                r#"{"strictPluginOnlyCustomization":["skills"]}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Allowed,
                    skill: PolicyState::Blocked,
                },
            ),
        ] {
            fs::write(&fixture.policy, document).unwrap();
            let result = probe_release_environment(&fixture.input()).unwrap();
            assert_eq!(current_customization_policy(&result), expected);
        }

        for document in [
            r#"{"strictPluginOnlyCustomization":"mcp"}"#,
            r#"{"policyHelper":"/usr/local/bin/effective-policy"}"#,
        ] {
            fs::write(&fixture.policy, document).unwrap();
            assert_release_policy_unknown(&fixture.input());
        }
    }

    #[test]
    fn missing_tools_are_unavailable_without_running_host_commands() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unavailable);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unavailable);

        let context = DiscoveryContext {
            environment: &result.environment,
            project_root: None,
            claude_user_mcp_probe: result.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: result
                .environment
                .claude_customization_policy_probe(),
        };
        assert!(ClaudeAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .all(|target| target.capability.state == CapabilityState::ToolNotInstalled));
        assert!(CodexAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .all(|target| target.capability.state == CapabilityState::ToolNotInstalled));
    }

    #[test]
    fn macos_release_path_finds_volta_shims_without_shell_path_setup() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        let volta_bin = fixture.home.join(".volta/bin");
        fs::create_dir_all(&volta_bin).unwrap();
        let shim = fixture.home.join("volta-shim");
        write_executable(
            &shim,
            r#"case "${0##*/}" in
claude) printf '2.1.217 (Claude Code)' ;;
codex) printf 'codex-cli 0.114.0' ;;
*) printf 'direct shim execution is unsupported' >&2; exit 9 ;;
esac"#,
        );
        symlink(&shim, volta_bin.join("claude")).unwrap();
        symlink(&shim, volta_bin.join("codex")).unwrap();

        let input = ReleaseToolProbeInput::for_macos_release(
            fixture.home.clone(),
            Some(fixture.claude_root.clone()),
            Some(fixture.codex_root.clone()),
            OsString::new(),
        );
        assert_eq!(
            std::env::split_paths(&input.search_path).collect::<Vec<_>>(),
            vec![volta_bin]
        );

        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Installed);
        assert_eq!(result.claude.version.as_deref(), Some("2.1.217"));
        assert_eq!(result.codex.state, ToolAvailabilityState::Installed);
        assert_eq!(result.codex.version.as_deref(), Some("0.114.0"));
    }

    #[test]
    fn macos_release_path_keeps_existing_precedence_and_deduplicates_volta() {
        let fixture = Fixture::new();
        let volta_bin = fixture.home.join(".volta/bin");
        let input = ReleaseToolProbeInput::for_macos_release(
            fixture.home.clone(),
            Some(fixture.claude_root.clone()),
            Some(fixture.codex_root.clone()),
            std::env::join_paths([fixture.bin.clone(), volta_bin.clone()]).unwrap(),
        );

        assert_eq!(
            std::env::split_paths(&input.search_path).collect::<Vec<_>>(),
            vec![fixture.bin, volta_bin]
        );
    }

    #[test]
    fn timeout_and_malicious_output_are_unsupported() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", &fixture.pipe_holding_descendant("wait"));
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0\\nforged'");
        fixture.write_tool("agent", &fixture.pipe_holding_descendant("wait"));
        let mut input = fixture.input();
        input.timeout = Duration::from_millis(100);
        let started = std::time::Instant::now();
        let result = probe_release_environment(&input).unwrap();
        assert!(started.elapsed() < Duration::from_millis(800));
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.environment.claude_installation_version(), None);
        assert_eq!(result.environment.codex_installation_version(), None);
        assert_eq!(result.environment.cursor_installation_version(), None);
    }

    #[test]
    fn exited_wrapper_cannot_leave_a_pipe_holding_descendant() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", &fixture.pipe_holding_descendant("exit 0"));
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        let mut input = fixture.input();
        input.timeout = Duration::from_secs(5);

        let started = std::time::Instant::now();
        let result = probe_release_environment(&input).unwrap();

        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Installed);
    }

    #[test]
    fn nonzero_oversized_non_utf8_and_stderr_outputs_fail_closed() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "exit 7");
        fixture.write_tool(
            "codex",
            "i=0; while [ \"$i\" -lt 1100 ]; do printf x; i=$((i + 1)); done",
        );
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unsupported);

        fixture.write_tool("claude", "printf '\\377'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'; printf unexpected >&2");
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unsupported);
    }

    #[test]
    fn unsafe_path_entries_are_skipped_instead_of_failing_the_whole_probe() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");

        // 常见的桌面 PATH：`.`、`./node_modules/.bin` 与相对条目混在安全条目前面。
        let mut input = fixture.input();
        input.search_path = std::env::join_paths([
            PathBuf::from("."),
            PathBuf::from("./node_modules/.bin"),
            PathBuf::from("relative-bin"),
            fixture.bin.clone(),
        ])
        .unwrap();
        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Installed);
        assert_eq!(result.claude.version.as_deref(), Some("2.1.217"));
        assert_eq!(result.codex.state, ToolAvailabilityState::Installed);
        assert_eq!(
            result.claude.diagnostic,
            Some(super::PROBE_DIAGNOSTIC_SKIPPED_PATH_ENTRIES)
        );
        assert_eq!(
            result
                .environment
                .installation_probe_diagnostic(Tool::Claude),
            Some(super::PROBE_DIAGNOSTIC_SKIPPED_PATH_ENTRIES)
        );
        assert_eq!(
            result
                .environment
                .installation_probe_diagnostic(Tool::Cursor),
            None
        );

        // 前置条目下同名的是目录，应跳过它继续在后面的安全条目里找到真实二进制。
        let shadow = fixture.home.join("shadow-bin");
        fs::create_dir_all(shadow.join("claude")).unwrap();
        let mut input = fixture.input();
        input.search_path = std::env::join_paths([shadow.clone(), fixture.bin.clone()]).unwrap();
        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Installed);
        assert_eq!(
            result.claude.diagnostic,
            Some(super::PROBE_DIAGNOSTIC_SKIPPED_PATH_ENTRIES)
        );

        // 完全干净的 PATH 不带诊断。
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Installed);
        assert_eq!(result.claude.diagnostic, None);
    }

    #[test]
    fn unsafe_first_candidate_and_empty_safe_path_still_fail_closed() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");

        // 首个命中的候选是不可执行文件：不能越过它去执行后面的二进制。
        let first = fixture.home.join("first-bin");
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("claude"), "not executable").unwrap();
        fs::set_permissions(first.join("claude"), fs::Permissions::from_mode(0o600)).unwrap();
        let mut input = fixture.input();
        input.search_path = std::env::join_paths([first.clone(), fixture.bin.clone()]).unwrap();
        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(
            result.claude.diagnostic,
            Some(super::PROBE_DIAGNOSTIC_UNSAFE_CANDIDATE)
        );

        // 首个候选是悬空链接同样不受支持。
        let dangling = fixture.home.join("dangling-bin");
        fs::create_dir_all(&dangling).unwrap();
        symlink(fixture.home.join("missing-target"), dangling.join("claude")).unwrap();
        let mut input = fixture.input();
        input.search_path = std::env::join_paths([dangling.clone(), fixture.bin.clone()]).unwrap();
        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(
            result.claude.diagnostic,
            Some(super::PROBE_DIAGNOSTIC_UNSAFE_CANDIDATE)
        );

        // 没有任何安全条目：无处可搜，保持不受支持并说明原因。
        let mut input = fixture.input();
        input.search_path = OsString::from("relative-bin");
        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(
            result.claude.diagnostic,
            Some(super::PROBE_DIAGNOSTIC_NO_SAFE_PATH_ENTRIES)
        );
        let mut input = fixture.input();
        input.search_path = OsString::new();
        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.codex.state, ToolAvailabilityState::Unsupported);

        // 安全条目里根本没有这个工具：不可用而不是不受支持。
        let empty = fixture.home.join("empty-bin");
        fs::create_dir_all(&empty).unwrap();
        let mut input = fixture.input();
        input.search_path = std::env::join_paths([PathBuf::from("."), empty]).unwrap();
        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unavailable);
        assert_eq!(
            result.claude.diagnostic,
            Some(super::PROBE_DIAGNOSTIC_SKIPPED_PATH_ENTRIES)
        );
    }

    #[test]
    fn unsafe_search_path_and_unexpected_argv_never_become_installed() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool(
            "claude",
            "[ \"$#\" -eq 1 ] && [ \"$1\" = --version ] || exit 8\nread ignored && exit 9\nprintf '2.1.217 (Claude Code)'",
        );
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        let valid = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(valid.claude.state, ToolAvailabilityState::Installed);

        let mut unsafe_input = fixture.input();
        unsafe_input.search_path = OsString::from("relative-bin");
        let unsupported = probe_release_environment(&unsafe_input).unwrap();
        assert_eq!(unsupported.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(unsupported.codex.state, ToolAvailabilityState::Unsupported);
    }

    #[test]
    fn policy_source_rejects_symlinked_ancestors_ambiguous_inputs_and_malformed_files() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        let valid_policy = r#"{"strictPluginOnlyCustomization":false}"#;
        fs::write(&fixture.policy, valid_policy).unwrap();

        let mut malformed = fixture.input();
        malformed.claude_managed_settings_path = fixture.policy.with_file_name("other.json");
        fs::write(&malformed.claude_managed_settings_path, valid_policy).unwrap();
        assert_release_policy_unknown(&malformed);

        fs::write(&fixture.policy, b"{invalid").unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::write(&fixture.policy, b"").unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::remove_file(&fixture.policy).unwrap();
        fs::create_dir(&fixture.policy).unwrap();
        assert_release_policy_unknown(&fixture.input());
        fs::remove_dir(&fixture.policy).unwrap();

        fs::write(
            &fixture.policy,
            vec![b'x'; super::MAX_POLICY_BYTES as usize + 1],
        )
        .unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::write(&fixture.policy, valid_policy).unwrap();
        fs::set_permissions(&fixture.policy, fs::Permissions::from_mode(0o000)).unwrap();
        assert_release_policy_unknown(&fixture.input());
        fs::set_permissions(&fixture.policy, fs::Permissions::from_mode(0o600)).unwrap();

        fs::write(&fixture.policy, valid_policy).unwrap();
        fs::write(fixture.policy_directory.join("10-policy.json"), "{}").unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::remove_file(fixture.policy_directory.join("10-policy.json")).unwrap();
        let real_parent = fixture.home.parent().unwrap().join("real-policy-parent");
        let alias_parent = fixture.home.parent().unwrap().join("policy-parent-alias");
        fs::create_dir(&real_parent).unwrap();
        fs::write(real_parent.join("managed-settings.json"), valid_policy).unwrap();
        fs::create_dir(real_parent.join("managed-settings.d")).unwrap();
        symlink(&real_parent, &alias_parent).unwrap();
        let mut symlinked = fixture.input();
        symlinked.claude_managed_settings_path = alias_parent.join("managed-settings.json");
        symlinked.claude_managed_settings_directory = alias_parent.join("managed-settings.d");
        assert_release_policy_unknown(&symlinked);
    }

    #[test]
    fn custom_root_blocks_user_mcp_while_policy_can_be_blocked_or_absent() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        fs::write(
            &fixture.policy,
            r#"{"strictPluginOnlyCustomization":["mcp"]}"#,
        )
        .unwrap();
        let custom_root = fixture.home.join("custom-claude");
        fs::create_dir(&custom_root).unwrap();
        let mut input = fixture.input();
        input.claude_config_dir = Some(custom_root);
        let blocked = probe_release_environment(&input).unwrap();
        let context = DiscoveryContext {
            environment: &blocked.environment,
            project_root: None,
            claude_user_mcp_probe: blocked.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: blocked
                .environment
                .claude_customization_policy_probe(),
        };
        let targets = ClaudeAdapter.discover(&context).unwrap();
        let mcp = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        let skill = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Skill)
            .unwrap();
        assert_eq!(mcp.scope, Scope::Global);
        assert_eq!(mcp.capability.state, CapabilityState::Unsupported);
        assert_eq!(mcp.policy, PolicyState::Blocked);
        assert_eq!(skill.policy, PolicyState::Allowed);

        fs::remove_file(&fixture.policy).unwrap();
        let absent = probe_release_environment(&input).unwrap();
        let context = DiscoveryContext {
            environment: &absent.environment,
            project_root: None,
            claude_user_mcp_probe: absent.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: absent
                .environment
                .claude_customization_policy_probe(),
        };
        assert!(ClaudeAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .filter(|target| {
                matches!(
                    target.artifact_kind,
                    ArtifactKind::Mcp | ArtifactKind::Skill
                )
            })
            .all(|target| target.policy == PolicyState::Allowed));
    }
}
