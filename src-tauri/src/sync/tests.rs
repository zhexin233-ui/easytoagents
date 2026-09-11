#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        path::PathBuf,
    };

    use rusqlite::params;
    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        assess_drift, build_preview_plan, load_managed_target_baseline, load_persisted_preview,
        persist_preview, scan_target, DatabaseEntityType, DatabaseRowVersion,
        ManagedTargetBaseline, PreviewTargetRequest, TargetScan, ERROR_EXTERNAL_OWNED_CHANGE,
        ERROR_INCOMPLETE_BASELINE, WARNING_CODEX_PROMPT_OVERRIDE,
        WARNING_EXTERNAL_NON_OWNED_CHANGE,
    };
    use crate::{
        adapters::{
            canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
            cursor::CursorAdapter, ConservativeClaudeCustomizationPolicyProbe,
            ConservativeClaudeUserMcpProbe, DiscoveryContext, ExplicitEnvironment,
            ManagedOwnership, PolicyState, TargetTrustState, ToolAdapter, ToolAvailability,
        },
        app::AppPaths,
        db::Database,
        domain::{ArtifactKind, ChangeKind, Scope, SyncStatus},
        security::SecretRedactor,
    };

    const TARGET_ID: &str = "10000000-0000-4000-8000-000000000001";

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/phase2")
            .join(name)
    }

    fn isolated_environment(home: &std::path::Path) -> ExplicitEnvironment {
        ExplicitEnvironment::new(home, None, None, ToolAvailability::all_installed())
            .unwrap()
            .with_claude_provider_policy(PolicyState::Allowed)
    }

    fn claude_targets(environment: &ExplicitEnvironment) -> Vec<crate::adapters::TargetDescriptor> {
        let probe = ConservativeClaudeUserMcpProbe;
        ClaudeAdapter
            .discover(&DiscoveryContext {
                environment,
                project_root: None,
                claude_user_mcp_probe: &probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
    }

    /// `hash_json` 直接序列化 `Value`，依赖 serde_json 未开启 `preserve_order`
    /// （`Map` 为 `BTreeMap`）。一旦有人打开该特性，这里会首先失败。
    #[test]
    fn serde_json_serializes_object_keys_in_sorted_order() {
        let value = json!({ "b": 1, "a": { "z": true, "m": [3, { "y": 1, "x": 2 }] } });
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"{"a":{"m":[3,{"x":2,"y":1}],"z":true},"b":1}"#
        );
        let mut reordered = serde_json::Map::new();
        reordered.insert("b".to_owned(), json!(1));
        reordered.insert("a".to_owned(), json!(2));
        assert_eq!(
            super::hash_json(&serde_json::Value::Object(reordered)),
            super::hash_json(&json!({"a": 2, "b": 1}))
        );
    }

    #[test]
    fn all_file_fixtures_parse_only_inside_an_explicit_isolated_matrix() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project_path = home.join("project");
        fs::create_dir(&project_path).unwrap();
        let project = canonicalize_project_root(&project_path).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        fs::create_dir_all(environment.codex_home()).unwrap();
        fs::create_dir_all(home.join(".cursor")).unwrap();
        fs::create_dir_all(project_path.join(".codex")).unwrap();
        fs::create_dir_all(project_path.join(".cursor")).unwrap();
        fs::copy(
            fixture("claude-settings.json"),
            environment.claude_config_dir().join("settings.json"),
        )
        .unwrap();
        fs::copy(fixture("claude-user-mcp.json"), home.join(".claude.json")).unwrap();
        fs::copy(
            fixture("claude-project-mcp.json"),
            project_path.join(".mcp.json"),
        )
        .unwrap();
        fs::copy(
            fixture("claude-prompt.md"),
            environment.claude_config_dir().join("CLAUDE.md"),
        )
        .unwrap();
        let codex_config = fs::read_to_string(fixture("codex-config.toml"))
            .unwrap()
            .replace("/fixture/project", project.as_str());
        fs::write(environment.codex_home().join("config.toml"), codex_config).unwrap();
        fs::copy(
            fixture("codex-project-config.toml"),
            project_path.join(".codex/config.toml"),
        )
        .unwrap();
        fs::copy(
            fixture("codex-prompt.md"),
            environment.codex_home().join("AGENTS.md"),
        )
        .unwrap();
        fs::copy(
            fixture("cursor-user-mcp.json"),
            home.join(".cursor/mcp.json"),
        )
        .unwrap();
        fs::copy(
            fixture("cursor-project-mcp.json"),
            project_path.join(".cursor/mcp.json"),
        )
        .unwrap();

        let probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let claude = ClaudeAdapter.discover(&context).unwrap();
        let codex = CodexAdapter.discover(&context).unwrap();
        let cursor = CursorAdapter.discover(&context).unwrap();
        let cases: Vec<(
            &dyn ToolAdapter,
            &crate::adapters::TargetDescriptor,
            ManagedOwnership,
        )> = vec![
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| target.artifact_kind == ArtifactKind::Provider)
                    .unwrap(),
                ManagedOwnership::selectors([vec!["env", "ANTHROPIC_API_KEY"]]),
            ),
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcpServers", "fixture-user"]]),
            ),
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcpServers", "fixture-project"]]),
            ),
            (
                &ClaudeAdapter,
                claude
                    .iter()
                    .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                    .unwrap(),
                ManagedOwnership::WholeDocument,
            ),
            (
                &CodexAdapter,
                codex
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcp_servers", "fixture_user"]]),
            ),
            (
                &CodexAdapter,
                codex
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcp_servers", "fixture_project"]]),
            ),
            (
                &CodexAdapter,
                codex
                    .iter()
                    .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                    .unwrap(),
                ManagedOwnership::WholeDocument,
            ),
            (
                &CursorAdapter,
                cursor
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcpServers", "fixture-user"]]),
            ),
            (
                &CursorAdapter,
                cursor
                    .iter()
                    .find(|target| {
                        target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
                    })
                    .unwrap(),
                ManagedOwnership::selectors([vec!["mcpServers", "fixture-project"]]),
            ),
        ];
        for (adapter, target, ownership) in cases {
            assert!(matches!(
                scan_target(adapter, target, &ownership),
                TargetScan::Observed(_)
            ));
            assert!(target.path().unwrap().starts_with(&home));
        }
    }

    #[test]
    fn scanner_distinguishes_missing_empty_corrupt_permission_and_target_type() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        let targets = claude_targets(&environment);
        let mut settings = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap()
            .clone();
        let prompt = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        assert!(matches!(
            scan_target(&ClaudeAdapter, prompt, &ManagedOwnership::WholeDocument),
            TargetScan::Missing
        ));

        let empty = home.join("empty.json");
        fs::write(&empty, b"").unwrap();
        settings.path = Some(empty.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &settings,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::ParseError
        ));

        let corrupt = home.join("corrupt.json");
        fs::write(&corrupt, b"{broken").unwrap();
        settings.path = Some(corrupt.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &settings,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::ParseError
        ));

        let unreadable = home.join("unreadable.json");
        fs::copy(fixture("claude-settings.json"), &unreadable).unwrap();
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        settings.path = Some(unreadable.to_str().unwrap().to_owned());
        let unreadable_scan = scan_target(
            &ClaudeAdapter,
            &settings,
            &ManagedOwnership::selectors([vec!["env"]]),
        );
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(unreadable_scan, TargetScan::PermissionDenied));

        let directory = home.join("settings-directory");
        fs::create_dir(&directory).unwrap();
        settings.path = Some(directory.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &settings,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::TargetTypeChanged(_)
        ));
    }

    #[test]
    fn scanner_rejects_late_symlink_ancestors_and_invalid_managed_shapes() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let late_claude_root = home.join("late-claude-root");
        let environment = ExplicitEnvironment::new(
            &home,
            Some(late_claude_root.clone()),
            None,
            ToolAvailability::all_installed(),
        )
        .unwrap();
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        let outside = home.join("outside-config");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("settings.json"), "{\"env\":{}}\n").unwrap();
        symlink(&outside, &late_claude_root).unwrap();
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &descriptor,
                &ManagedOwnership::selectors([vec!["env"]])
            ),
            TargetScan::TargetTypeChanged(crate::domain::TargetType::Symlink)
        ));

        let safe_path = home.join("invalid-managed-shape.json");
        fs::write(&safe_path, "{\"env\":\"external scalar\"}\n").unwrap();
        let mut safe_descriptor = descriptor;
        safe_descriptor.path = Some(safe_path.to_str().unwrap().to_owned());
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &safe_descriptor,
                &ManagedOwnership::selectors([vec!["env", "ANTHROPIC_API_KEY"]])
            ),
            TargetScan::ParseError
        ));
        assert!(matches!(
            scan_target(
                &ClaudeAdapter,
                &safe_descriptor,
                &ManagedOwnership::selectors([vec!["permissions"]])
            ),
            TargetScan::Failed
        ));
    }

    #[test]
    fn scanner_rejects_corrupt_toml_and_accepts_empty_markdown() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.codex_home()).unwrap();
        let probe = ConservativeClaudeUserMcpProbe;
        let targets = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &environment,
                project_root: None,
                claude_user_mcp_probe: &probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap();
        let config = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        fs::write(config.path().unwrap(), "[broken\n").unwrap();
        assert!(matches!(
            scan_target(
                &CodexAdapter,
                config,
                &ManagedOwnership::selectors([vec!["model"]])
            ),
            TargetScan::ParseError
        ));

        let prompt = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        fs::write(prompt.path().unwrap(), b"").unwrap();
        assert!(matches!(
            scan_target(&CodexAdapter, prompt, &ManagedOwnership::WholeDocument),
            TargetScan::Observed(_)
        ));
    }

    #[test]
    fn skills_fixture_is_read_as_links_without_following_or_writing_targets() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        let targets = claude_targets(&environment);
        let skills = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Skill)
            .unwrap();
        fs::create_dir_all(skills.path().unwrap()).unwrap();
        let source = fixture("skills/fixture-skill");
        symlink(&source, skills.path().unwrap().join("fixture-skill")).unwrap();
        fs::create_dir(skills.path().unwrap().join("external-directory")).unwrap();

        let scan = scan_target(
            &ClaudeAdapter,
            skills,
            &ManagedOwnership::SymlinkNames(vec!["fixture-skill".to_owned()]),
        );
        let TargetScan::Observed(observed) = scan else {
            panic!("Skills 目录必须被成功读取");
        };
        assert_eq!(
            observed.managed_projection["fixture-skill"]["targetType"],
            "symlink"
        );
        assert_eq!(
            observed.managed_projection["fixture-skill"]["linkTarget"],
            source.to_str().unwrap()
        );
        assert!(source.join("SKILL.md").is_file());
    }

    #[test]
    fn dual_hash_allows_only_non_owned_changes_to_merge() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        fs::copy(fixture("claude-settings.json"), descriptor.path().unwrap()).unwrap();
        let ownership = ManagedOwnership::selectors([
            vec!["env", "ANTHROPIC_BASE_URL"],
            vec!["env", "ANTHROPIC_API_KEY"],
        ]);
        let first = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let TargetScan::Observed(first_observed) = &first else {
            panic!("fixture 必须能解析");
        };
        let baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 1,
            full_hash: Some(first_observed.full_hash.clone()),
            managed_hash: Some(first_observed.managed_hash.clone()),
        };
        assert_eq!(
            assess_drift(&descriptor, &baseline, &first).status,
            SyncStatus::InSync
        );

        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(descriptor.path().unwrap()).unwrap()).unwrap();
        document["permissions"]["allow"] = json!(["Read", "Glob"]);
        fs::write(
            descriptor.path().unwrap(),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        let non_owned = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let non_owned_assessment = assess_drift(&descriptor, &baseline, &non_owned);
        assert_eq!(
            non_owned_assessment.status,
            SyncStatus::ExternalNonOwnedChange
        );
        assert!(non_owned_assessment.can_merge);
        assert!(non_owned_assessment
            .diagnostic_codes
            .contains(&WARNING_EXTERNAL_NON_OWNED_CHANGE.to_owned()));

        document["env"]["ANTHROPIC_BASE_URL"] = json!("https://external.invalid");
        fs::write(
            descriptor.path().unwrap(),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        let owned = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let owned_assessment = assess_drift(&descriptor, &baseline, &owned);
        assert_eq!(owned_assessment.status, SyncStatus::ExternalOwnedChange);
        assert!(!owned_assessment.can_merge);
        assert!(owned_assessment
            .diagnostic_codes
            .contains(&ERROR_EXTERNAL_OWNED_CHANGE.to_owned()));
    }

    #[test]
    fn policy_blocked_and_untrusted_targets_never_merge() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        let mut descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        descriptor.policy = PolicyState::Blocked;
        let baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 1,
            full_hash: None,
            managed_hash: None,
        };
        let blocked = assess_drift(&descriptor, &baseline, &TargetScan::Missing);
        assert_eq!(blocked.status, SyncStatus::PolicyBlocked);
        assert!(!blocked.can_merge);

        descriptor.policy = PolicyState::Allowed;
        descriptor.trust = TargetTrustState::Untrusted;
        let untrusted = assess_drift(&descriptor, &baseline, &TargetScan::Missing);
        assert_eq!(untrusted.status, SyncStatus::Untrusted);
        assert!(!untrusted.can_merge);

        descriptor.trust = TargetTrustState::NotRequired;
        let incomplete_baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 1,
            full_hash: Some("a".repeat(64)),
            managed_hash: None,
        };
        let incomplete = assess_drift(&descriptor, &incomplete_baseline, &TargetScan::Missing);
        assert_eq!(incomplete.status, SyncStatus::ExternalOwnedChange);
        assert!(!incomplete.can_merge);
        assert_eq!(
            incomplete.diagnostic_codes,
            vec![ERROR_INCOMPLETE_BASELINE.to_owned()]
        );
    }

    #[test]
    fn codex_prompt_override_turns_an_unchanged_preview_into_a_warning() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.codex_home()).unwrap();
        fs::write(environment.codex_home().join("AGENTS.md"), "fixture prompt").unwrap();
        fs::write(
            environment.codex_home().join("AGENTS.override.md"),
            "fixture override",
        )
        .unwrap();
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let descriptor = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        let scan = scan_target(&CodexAdapter, &descriptor, &ManagedOwnership::WholeDocument);
        let TargetScan::Observed(observed) = &scan else {
            panic!("Codex prompt fixture 必须可读取");
        };
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                hook_initial_adopt: false,
                descriptor,
                ownership: ManagedOwnership::WholeDocument,
                baseline: ManagedTargetBaseline {
                    target_id: TARGET_ID.to_owned(),
                    target_row_version: 1,
                    full_hash: Some(observed.full_hash.clone()),
                    managed_hash: Some(observed.managed_hash.clone()),
                },
                scan,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: json!("fixture prompt"),
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
            &SecretRedactor::default(),
        )
        .unwrap();
        assert_eq!(plan.targets[0].status, SyncStatus::InSync);
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Warning);
        assert!(plan.targets[0]
            .warning_codes
            .contains(&WARNING_CODEX_PROMPT_OVERRIDE.to_owned()));
    }

    #[test]
    fn preview_rejects_conflicting_versions_for_the_same_database_entity() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        let error = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                hook_initial_adopt: false,
                descriptor,
                ownership: ManagedOwnership::WholeDocument,
                baseline: ManagedTargetBaseline {
                    target_id: TARGET_ID.to_owned(),
                    target_row_version: 1,
                    full_hash: None,
                    managed_hash: None,
                },
                scan: TargetScan::Missing,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: json!("fixture prompt"),
                row_versions: vec![DatabaseRowVersion {
                    entity_type: DatabaseEntityType::ManagedTarget,
                    entity_id: TARGET_ID.to_owned(),
                    row_version: 2,
                }],
                git: None,
                exclude_from_git: false,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
            &SecretRedactor::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn codex_http_header_fixture_is_redacted_by_the_descriptor_selector() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.codex_home()).unwrap();
        fs::copy(
            fixture("codex-config.toml"),
            environment.codex_home().join("config.toml"),
        )
        .unwrap();
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let descriptor = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        let ownership = ManagedOwnership::selectors([vec!["mcp_servers", "fixture_user"]]);
        let scan = scan_target(&CodexAdapter, &descriptor, &ownership);
        let TargetScan::Observed(observed) = &scan else {
            panic!("Codex MCP fixture 必须可读取");
        };
        let desired_projection = observed.managed_projection.clone();
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                hook_initial_adopt: false,
                descriptor,
                ownership,
                baseline: ManagedTargetBaseline {
                    target_id: TARGET_ID.to_owned(),
                    target_row_version: 1,
                    full_hash: Some(observed.full_hash.clone()),
                    managed_hash: Some(observed.managed_hash.clone()),
                },
                scan,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection,
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
            &SecretRedactor::default(),
        )
        .unwrap();
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("fixture-codex-user-mcp-secret"));
    }

    #[test]
    fn preview_persists_row_versions_and_never_serializes_fixture_secrets() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let home = isolated_root.join("home");
        fs::create_dir(&home).unwrap();
        let environment = isolated_environment(&home);
        fs::create_dir_all(environment.claude_config_dir()).unwrap();
        let mut descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        descriptor.managed_selector_roots.push("custom".to_owned());
        descriptor
            .sensitive_selectors
            .push("custom/value".to_owned());
        fs::copy(fixture("claude-settings.json"), descriptor.path().unwrap()).unwrap();
        let ownership = ManagedOwnership::selectors([
            vec!["env", "ANTHROPIC_BASE_URL"],
            vec!["env", "ANTHROPIC_API_KEY"],
        ]);
        let scan = scan_target(&ClaudeAdapter, &descriptor, &ownership);
        let TargetScan::Observed(observed) = &scan else {
            panic!("fixture 必须能解析");
        };
        let baseline = ManagedTargetBaseline {
            target_id: TARGET_ID.to_owned(),
            target_row_version: 7,
            full_hash: Some(observed.full_hash.clone()),
            managed_hash: Some(observed.managed_hash.clone()),
        };
        let fixture_secrets = [
            "fixture-claude-provider-secret",
            "fixture-claude-policy-secret",
            "fixture-claude-user-mcp-secret",
            "fixture-claude-project-mcp-secret",
            "fixture-codex-provider-secret",
            "fixture-codex-user-mcp-secret",
            "fixture-codex-project-mcp-secret",
            "fixture-claude-prompt-secret",
            "fixture-codex-prompt-secret",
            "fixture-preview-next-secret",
        ];
        let mut redactor = SecretRedactor::default();
        for secret in fixture_secrets {
            redactor.register_secret(secret);
        }
        let desired = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://replacement.invalid",
                "ANTHROPIC_API_KEY": "fixture-preview-next-secret"
            },
            "custom": {
                "value": "fixture-selector-only-value"
            }
        });
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                hook_initial_adopt: false,
                descriptor: descriptor.clone(),
                ownership,
                baseline,
                scan,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: desired,
                row_versions: vec![DatabaseRowVersion {
                    entity_type: DatabaseEntityType::ProviderProfile,
                    entity_id: "20000000-0000-4000-8000-000000000002".to_owned(),
                    row_version: 11,
                }],
                git: None,
                exclude_from_git: false,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
            &redactor,
        )
        .unwrap();
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Update);
        let serialized = serde_json::to_string(&plan).unwrap();
        for secret in fixture_secrets {
            assert!(
                !serialized.contains(secret),
                "Preview 泄漏 fixture secret: {secret}"
            );
        }
        assert!(!serialized.contains("fixture-selector-only-value"));

        let app_paths = AppPaths::from_data_root(isolated_root.join("app-data")).unwrap();
        let mut database = Database::open(&app_paths).unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json,
                    last_status, row_version
                 ) VALUES (?1, 'claude', 'provider', 'global', ?2, ?3, ?4, '{}', 'in_sync', 7)",
                params![
                    TARGET_ID,
                    descriptor.path.as_deref().unwrap(),
                    plan.targets[0].current_full_hash,
                    plan.targets[0].current_managed_hash,
                ],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO provider_profiles(id, tool, name, row_version)
                 VALUES (?1, 'claude', 'Fixture Provider', 11)",
                ["20000000-0000-4000-8000-000000000002"],
            )
            .unwrap();
        let stored_baseline = load_managed_target_baseline(&database, TARGET_ID).unwrap();
        assert_eq!(stored_baseline.target_row_version, 7);
        assert_eq!(stored_baseline.full_hash, plan.targets[0].current_full_hash);
        let mut mismatched_plan = plan.clone();
        mismatched_plan.preview_id = "30000000-0000-4000-8000-000000000003".to_owned();
        mismatched_plan.targets[0].descriptor.path = Some(
            home.join("wrong-settings.json")
                .to_str()
                .unwrap()
                .to_owned(),
        );
        let mismatch_error = persist_preview(&mut database, &mismatched_plan).unwrap_err();
        assert_eq!(mismatch_error.code(), crate::error::ErrorCode::InvalidInput);
        persist_preview(&mut database, &plan).unwrap();
        let persisted = load_persisted_preview(&database, &plan.preview_id).unwrap();
        assert_eq!(persisted.db_version, i64::from(plan.db_version));
        assert_eq!(persisted.items[0].envelope.target_row_version, 7);
        assert_eq!(persisted.items[0].envelope.row_versions[0].row_version, 11);
        let persisted_json: String = database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&plan.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        for secret in fixture_secrets {
            assert!(
                !persisted_json.contains(secret),
                "持久化 Preview 泄漏 fixture secret: {secret}"
            );
        }
        assert!(!persisted_json.contains("fixture-selector-only-value"));
    }

    #[test]
    fn preview_persistence_rejects_changed_database_row_versions() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let home = root.join("home");
        fs::create_dir(&home).unwrap();
        let environment = isolated_environment(&home);
        let descriptor = claude_targets(&environment)
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        let paths = AppPaths::from_data_root(root.join("app-data")).unwrap();
        let mut database = Database::open(&paths).unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                 VALUES (?1, 'claude', 'prompt', 'global', ?2)",
                params![TARGET_ID, descriptor.path.as_deref().unwrap()],
            )
            .unwrap();
        let baseline = load_managed_target_baseline(&database, TARGET_ID).unwrap();
        let plan = build_preview_plan(
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                hook_initial_adopt: false,
                descriptor,
                ownership: ManagedOwnership::WholeDocument,
                baseline,
                scan: TargetScan::Missing,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: json!("fixture prompt"),
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
            &SecretRedactor::default(),
        )
        .unwrap();
        database
            .connection()
            .execute(
                "UPDATE managed_targets SET last_status = 'failed' WHERE id = ?1",
                [TARGET_ID],
            )
            .unwrap();

        let error = persist_preview(&mut database, &plan).unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::StalePreview);
        let count: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sync_runs WHERE id = ?1",
                [&plan.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "过期 Preview 不得留下半持久化 run");
    }

    #[test]
    fn project_fixture_is_always_under_explicit_temp_root() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        let canonical = canonicalize_project_root(&project).unwrap();
        assert!(canonical.as_str().starts_with(root.to_str().unwrap()));
    }
}
