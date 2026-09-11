#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use serde_json::{json, Value};
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::{
        apply_mcp_preview_with_probes, create_mcp_server, get_mcp_server, list_mcp_project_options,
        preview_mcp_sync_with_probes, readopt_mcp_target, set_global_mcp_assignment,
        set_project_mcp_assignment, update_mcp_server,
    };
    use crate::{
        adapters::{
            ConservativeClaudeUserMcpProbe, ExplicitEnvironment, ToolAvailability,
            VerifiedClaudeCustomizationPolicyEvidence, VerifiedClaudeUserMcpEvidence,
        },
        app::AppPaths,
        db::Database,
        domain::{McpTransport, SyncStatus, Tool},
        error::ErrorCode,
        mcp::{
            ApplyMcpPreviewInput, McpProjectOptionsInput, McpProjectSelectionState, McpServerInput,
            PreviewMcpSyncInput, ReadoptMcpTargetInput, SensitiveJsonUpdate, SensitiveMapUpdate,
            SetGlobalMcpAssignmentInput, SetProjectMcpAssignmentInput, UpdateMcpServerInput,
        },
        security::SecretRedactor,
    };

    const HEADER_SECRET: &str = "Bearer phase5-header-secret";
    const ENV_SECRET: &str = "phase5-env-secret";
    const EXTRA_SECRET: &str = "phase5-extra-secret";

    struct Fixture {
        _temporary: TempDir,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
        home: std::path::PathBuf,
        project: std::path::PathBuf,
        project_id: String,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("isolated-home");
            let project = root.join("project");
            fs::create_dir(&home).unwrap();
            fs::create_dir(&project).unwrap();
            fs::create_dir(home.join(".claude")).unwrap();
            fs::create_dir(home.join(".codex")).unwrap();
            fs::create_dir(home.join(".cursor")).unwrap();
            fs::create_dir_all(home.join(".zcode/cli")).unwrap();
            fs::create_dir(project.join(".codex")).unwrap();
            fs::create_dir(project.join(".cursor")).unwrap();
            fs::create_dir(project.join(".zcode")).unwrap();
            let home = fs::canonicalize(home).unwrap();
            let project = fs::canonicalize(project).unwrap();
            let environment =
                ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                    .unwrap()
                    .with_claude_installation_version("fixture-1.0.0")
                    .unwrap()
                    .with_claude_customization_policy_evidence(
                        VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                            "fixture-1.0.0",
                            None,
                        )
                        .unwrap(),
                    );
            let paths = AppPaths::from_data_root(root.join("private/app/data")).unwrap();
            let database = Database::open(&paths).unwrap();
            let project_id = Uuid::new_v4().to_string();
            database
                .connection()
                .execute(
                    "INSERT INTO projects(
                        id, display_name, root_path, is_git_repo, codex_trust_status
                     ) VALUES (?1, '隔离项目', ?2, 0, 'unknown')",
                    rusqlite::params![project_id, project.to_string_lossy()],
                )
                .unwrap();
            Self {
                _temporary: temporary,
                paths,
                database,
                environment,
                home,
                project,
                project_id,
            }
        }

        fn allowed_policy(&self) -> VerifiedClaudeCustomizationPolicyEvidence {
            VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting("fixture-1.0.0", None)
                .unwrap()
        }

        fn environment_with_policy(&self, setting: Option<&Value>) -> ExplicitEnvironment {
            ExplicitEnvironment::new(&self.home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_claude_installation_version("fixture-1.0.0")
                .unwrap()
                .with_claude_customization_policy_evidence(
                    VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                        "fixture-1.0.0",
                        setting,
                    )
                    .unwrap(),
                )
        }

        fn environment_without_policy_evidence(&self) -> ExplicitEnvironment {
            ExplicitEnvironment::new(&self.home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_claude_installation_version("fixture-1.0.0")
                .unwrap()
        }
    }

    fn stdio_input(name: &str) -> McpServerInput {
        McpServerInput {
            name: name.to_owned(),
            transport: McpTransport::Stdio,
            command: Some("npx".to_owned()),
            args: vec!["-y".to_owned(), "fixture-server".to_owned()],
            url: None,
            headers: BTreeMap::new(),
            env: BTreeMap::from([("MCP_TOKEN".to_owned(), ENV_SECRET.to_owned())]),
            extra: json!({
                "startup_timeout_sec": 10,
                "nested": {"api_token": EXTRA_SECRET}
            }),
            enabled: true,
        }
    }

    fn http_input(name: &str) -> McpServerInput {
        McpServerInput {
            name: name.to_owned(),
            transport: McpTransport::StreamableHttp,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/rpc?tenant=fixture".to_owned()),
            headers: BTreeMap::from([("Authorization".to_owned(), HEADER_SECRET.to_owned())]),
            env: BTreeMap::new(),
            extra: json!({"request_timeout_sec": 30}),
            enabled: true,
        }
    }

    #[test]
    fn public_preview_status_and_apply_reuse_environment_policy_evidence() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let created = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("release-evidence"),
        )
        .unwrap();
        set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: created.id,
                assigned: true,
                row_version: created.row_version,
            },
        )
        .unwrap();
        let statuses =
            super::list_global_mcp_target_statuses(&fixture.database, &fixture.environment)
                .unwrap();
        assert_ne!(statuses[0].status, crate::domain::SyncStatus::PolicyBlocked);
        let preview = super::preview_mcp_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
        )
        .unwrap();
        assert_eq!(
            preview.targets[0].descriptor.policy,
            crate::adapters::PolicyState::Allowed
        );
        super::apply_mcp_preview(
            &std::sync::Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
        )
        .unwrap();
        assert!(fixture.home.join(".claude.json").is_file());
    }

    #[test]
    fn global_status_distinguishes_initial_missing_unknown_and_blocked_policy() {
        let fixture = Fixture::new();
        let missing =
            super::list_global_mcp_target_statuses(&fixture.database, &fixture.environment)
                .unwrap();
        assert_eq!(missing[0].tool, Tool::Claude);
        assert_eq!(missing[0].status, SyncStatus::Missing);
        assert_eq!(missing[0].diagnostic_code, None);

        let unknown_environment = fixture.environment_without_policy_evidence();
        let unknown =
            super::list_global_mcp_target_statuses(&fixture.database, &unknown_environment)
                .unwrap();
        assert_eq!(unknown[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            unknown[0].diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_CLAUDE_POLICY_UNKNOWN)
        );

        let blocked_environment = fixture.environment_with_policy(Some(&json!(true)));
        let blocked =
            super::list_global_mcp_target_statuses(&fixture.database, &blocked_environment)
                .unwrap();
        assert_eq!(blocked[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            blocked[0].diagnostic_code.as_deref(),
            Some("CLAUDE_POLICY_BLOCKED")
        );
    }

    #[test]
    fn crud_cas_assignment_and_dto_redaction_are_enforced() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let server = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("Fixture MCP"),
        )
        .unwrap();
        let serialized = serde_json::to_string(&server).unwrap();
        assert!(!serialized.contains(ENV_SECRET));
        assert!(!serialized.contains(EXTRA_SECRET));
        assert_eq!(server.env_names, ["MCP_TOKEN"]);
        assert_eq!(server.redacted_extra["nested"]["api_token"], "[REDACTED]");

        let duplicate = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("fixture mcp"),
        )
        .unwrap_err();
        assert_eq!(duplicate.code(), ErrorCode::Conflict);

        let global = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: server.id.clone(),
                assigned: true,
                row_version: server.row_version,
            },
        )
        .unwrap();
        let project = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        let inherited = set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                mcp_id: global.id.clone(),
                assigned: true,
                mcp_row_version: global.row_version,
                project_row_version: project,
            },
        )
        .unwrap_err();
        assert_eq!(inherited.code(), ErrorCode::Conflict);
        let inherited_disable = set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                mcp_id: global.id.clone(),
                assigned: false,
                mcp_row_version: global.row_version,
                project_row_version: project,
            },
        )
        .unwrap_err();
        assert_eq!(inherited_disable.code(), ErrorCode::Conflict);

        let options = list_mcp_project_options(
            &fixture.database,
            &McpProjectOptionsInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
            },
        )
        .unwrap();
        assert_eq!(options[0].state, McpProjectSelectionState::Inherited);
        assert!(!options[0].selectable);

        let stale = crate::mcp::set_mcp_enabled(
            &mut fixture.database,
            &redactor,
            &crate::mcp::VersionedMcpInput {
                id: global.id,
                row_version: server.row_version,
            },
            false,
        )
        .unwrap_err();
        assert_eq!(stale.code(), ErrorCode::Conflict);
    }

    #[test]
    fn global_json_and_toml_round_trip_rename_cleanup_and_drift_are_safe() {
        let mut fixture = Fixture::new();
        let claude_path = fixture.home.join(".claude.json");
        let codex_path = fixture.home.join(".codex/config.toml");
        let cursor_path = fixture.home.join(".cursor/mcp.json");
        fs::write(
            &claude_path,
            br#"{
  "theme": "dark",
  "mcpServers": {
    "external": {"command": "keep", "unknown": {"value": 1}}
  },
  "unknownTop": {"preserve": true}
}
"#,
        )
        .unwrap();
        fs::write(
            &codex_path,
            r#"# 顶层注释必须保留
model = "fixture-model"

[mcp_servers.external]
command = "keep"
unknown = "preserve"

[plugins.fixture]
enabled = true
"#,
        )
        .unwrap();
        fs::write(
            &cursor_path,
            r#"{"mcpServers":{"external":{"command":"keep","unknown":"preserve"}},"theme":"dark"}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let created = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &http_input("managed-http"),
        )
        .unwrap();
        let claude = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: created.row_version,
            },
        )
        .unwrap();
        let codex = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Codex,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: claude.row_version,
            },
        )
        .unwrap();
        let cursor = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Cursor,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: codex.row_version,
            },
        )
        .unwrap();
        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let write_operations = std::sync::Mutex::new(());

        for tool in [Tool::Claude, Tool::Codex, Tool::Cursor] {
            let input = PreviewMcpSyncInput {
                tool,
                project_id: None,
                exclude_from_git: false,
            };
            let preview = preview_mcp_sync_with_probes(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &input,
                &user_probe,
                &policy,
            )
            .unwrap();
            let serialized = serde_json::to_string(&preview).unwrap();
            for secret in [HEADER_SECRET, ENV_SECRET, EXTRA_SECRET] {
                assert!(!serialized.contains(secret));
            }
            apply_mcp_preview_with_probes(
                &write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &mut redactor,
                &ApplyMcpPreviewInput {
                    preview_id: preview.preview_id,
                    tool,
                    project_id: None,
                },
                &user_probe,
                &policy,
            )
            .unwrap();
        }

        let claude_native: Value =
            serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        assert_eq!(claude_native["theme"], "dark");
        assert_eq!(claude_native["mcpServers"]["external"]["command"], "keep");
        assert_eq!(
            claude_native["mcpServers"]["managed-http"]["headers"]["Authorization"],
            HEADER_SECRET
        );
        let codex_native = fs::read_to_string(&codex_path).unwrap();
        assert!(codex_native.contains("# 顶层注释必须保留"));
        assert!(codex_native.contains("[plugins.fixture]"));
        assert!(codex_native.contains("[mcp_servers.external]"));
        assert!(codex_native.contains(HEADER_SECRET));
        let cursor_native: Value =
            serde_json::from_slice(&fs::read(&cursor_path).unwrap()).unwrap();
        assert_eq!(cursor_native["theme"], "dark");
        assert_eq!(
            cursor_native["mcpServers"]["external"]["unknown"],
            "preserve"
        );
        assert_eq!(
            cursor_native["mcpServers"]["managed-http"]["headers"]["Authorization"],
            HEADER_SECRET
        );

        let updated = update_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &UpdateMcpServerInput {
                id: codex.id.clone(),
                name: "renamed-http".to_owned(),
                transport: McpTransport::StreamableHttp,
                command: None,
                args: Vec::new(),
                url: Some("https://mcp.example.test/rpc?tenant=fixture".to_owned()),
                headers: SensitiveMapUpdate::Keep,
                env: SensitiveMapUpdate::Keep,
                extra: SensitiveJsonUpdate::Keep,
                enabled: true,
                row_version: cursor.row_version,
            },
        )
        .unwrap();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        let renamed: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        assert!(renamed["mcpServers"].get("managed-http").is_none());
        assert!(renamed["mcpServers"].get("renamed-http").is_some());

        let reassigned = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: updated.id.clone(),
                assigned: false,
                row_version: updated.row_version,
            },
        )
        .unwrap();
        let removal = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: removal.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        let cleaned: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        assert!(cleaned["mcpServers"].get("renamed-http").is_none());
        assert!(cleaned["mcpServers"].get("external").is_some());

        let reenabled = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: reassigned.id.clone(),
                assigned: true,
                row_version: reassigned.row_version,
            },
        )
        .unwrap();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        let mut drifted: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        drifted["mcpServers"]["renamed-http"]["url"] =
            Value::String("https://external.example.test/rpc".to_owned());
        fs::write(&claude_path, serde_json::to_vec_pretty(&drifted).unwrap()).unwrap();
        let _unassigned = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: reenabled.id,
                assigned: false,
                row_version: reenabled.row_version,
            },
        )
        .unwrap();
        let blocked = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(blocked.targets[0].change_kind.as_str(), "conflict");
        assert!(blocked.targets[0]
            .warning_codes
            .contains(&crate::sync::ERROR_MANAGED_ITEM_BASELINE_MISMATCH.to_owned()));
        assert_eq!(
            drifted["mcpServers"]["renamed-http"]["url"],
            "https://external.example.test/rpc"
        );

        let sync_payloads = fixture
            .database
            .connection()
            .prepare("SELECT redacted_diff_json FROM sync_items")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        let journals = read_tree_text(fixture.paths.journals());
        for secret in [HEADER_SECRET, ENV_SECRET, EXTRA_SECRET] {
            assert!(!sync_payloads.contains(secret));
            assert!(!journals.contains(secret));
        }
    }

    #[test]
    fn claude_and_trusted_codex_project_targets_append_without_writing_inherited_items() {
        let mut fixture = Fixture::new();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.{}]\ntrust_level = \"trusted\"\n",
                toml_edit::Key::new(fixture.project.to_string_lossy().as_ref())
            ),
        )
        .unwrap();
        fs::write(
            fixture.project.join(".mcp.json"),
            br#"{"unknownTop":{"keep":true},"mcpServers":{"external":{"command":"keep"}}}"#,
        )
        .unwrap();
        fs::write(
            fixture.project.join(".codex/config.toml"),
            "# 项目注释\n[features]\nfixture = true\n",
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let inherited = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("inherited-global"),
        )
        .unwrap();
        let inherited = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: inherited.id,
                assigned: true,
                row_version: inherited.row_version,
            },
        )
        .unwrap();
        let claude_project = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("claude-project"),
        )
        .unwrap();
        let codex_project = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &http_input("codex-project"),
        )
        .unwrap();

        let mut project_version = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                mcp_id: claude_project.id,
                assigned: true,
                mcp_row_version: claude_project.row_version,
                project_row_version: project_version,
            },
        )
        .unwrap();
        project_version = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Codex,
                mcp_id: codex_project.id,
                assigned: true,
                mcp_row_version: codex_project.row_version,
                project_row_version: project_version,
            },
        )
        .unwrap();

        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let write_operations = std::sync::Mutex::new(());
        for tool in [Tool::Claude, Tool::Codex] {
            let preview = preview_mcp_sync_with_probes(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &PreviewMcpSyncInput {
                    tool,
                    project_id: Some(fixture.project_id.clone()),
                    exclude_from_git: false,
                },
                &user_probe,
                &policy,
            )
            .unwrap();
            assert_ne!(preview.targets[0].change_kind.as_str(), "conflict");
            apply_mcp_preview_with_probes(
                &write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &mut redactor,
                &ApplyMcpPreviewInput {
                    preview_id: preview.preview_id,
                    tool,
                    project_id: Some(fixture.project_id.clone()),
                },
                &user_probe,
                &policy,
            )
            .unwrap();
        }

        let claude_native: Value =
            serde_json::from_slice(&fs::read(fixture.project.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(claude_native["unknownTop"]["keep"], true);
        assert!(claude_native["mcpServers"].get("external").is_some());
        assert!(claude_native["mcpServers"].get("claude-project").is_some());
        assert!(claude_native["mcpServers"]
            .get("inherited-global")
            .is_none());
        let codex_native = fs::read_to_string(fixture.project.join(".codex/config.toml")).unwrap();
        assert!(codex_native.contains("# 项目注释"));
        assert!(codex_native.contains("[features]"));
        assert!(codex_native.contains("[mcp_servers.codex-project]"));
        assert!(codex_native.contains(HEADER_SECRET));
        assert_eq!(inherited.name, "inherited-global");
    }

    #[test]
    fn project_inheritance_external_conflict_untrusted_and_capability_fail_closed() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let global = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("global-name"),
        )
        .unwrap();
        let global = set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: global.id,
                assigned: true,
                row_version: global.row_version,
            },
        )
        .unwrap();
        fs::write(
            fixture.project.join(".mcp.json"),
            serde_json::to_vec_pretty(&json!({
                "unknownTop": true,
                "mcpServers": {"global-name": {"command": "external"}}
            }))
            .unwrap(),
        )
        .unwrap();
        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let conflict = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(conflict.targets[0].change_kind.as_str(), "conflict");
        let unchanged: Value =
            serde_json::from_slice(&fs::read(fixture.project.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(
            unchanged["mcpServers"]["global-name"]["command"],
            "external"
        );

        let project_server = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("project-only"),
        )
        .unwrap();
        let project_version = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&fixture.project_id],
                |row| row.get::<_, u32>(0),
            )
            .unwrap();
        set_project_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetProjectMcpAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Codex,
                mcp_id: project_server.id,
                assigned: true,
                mcp_row_version: project_server.row_version,
                project_row_version: project_version,
            },
        )
        .unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            "[projects.\"/somewhere-else\"]\ntrust_level = \"trusted\"\n",
        )
        .unwrap();
        let untrusted = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Codex,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(
            untrusted.targets[0].status,
            crate::domain::SyncStatus::Untrusted
        );

        let custom_root = fixture.home.join("custom-claude");
        fs::create_dir(&custom_root).unwrap();
        let custom_environment = ExplicitEnvironment::new(
            &fixture.home,
            Some(custom_root.clone()),
            None,
            ToolAvailability::all_installed(),
        )
        .unwrap()
        .with_claude_installation_version("fixture-1.0.0")
        .unwrap();
        let unsupported = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &custom_environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap_err();
        assert_eq!(unsupported.code(), ErrorCode::NotFound);

        let verified_path = fixture.home.join("verified-claude-mcp.json");
        let verified_probe =
            VerifiedClaudeUserMcpEvidence::new("fixture-1.0.0", &custom_root, &verified_path)
                .unwrap();
        let supported = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &custom_environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &verified_probe,
            &policy,
        )
        .unwrap();
        assert_eq!(
            supported.targets[0].descriptor.path.as_deref(),
            Some(verified_path.to_string_lossy().as_ref())
        );
        assert_eq!(global.name, "global-name");

        let secret_url = "https://phase5-url-secret@mcp.example.test/rpc";
        let mut invalid = http_input("invalid-url");
        invalid.url = Some(secret_url.to_owned());
        let error = create_mcp_server(&mut fixture.database, &mut redactor, &invalid).unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("phase5-url-secret"));
    }

    #[test]
    fn inherited_only_project_preview_does_not_create_an_empty_native_file() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let inherited = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("inherited-only"),
        )
        .unwrap();
        set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: inherited.id,
                assigned: true,
                row_version: inherited.row_version,
            },
        )
        .unwrap();

        let user_probe = ConservativeClaudeUserMcpProbe;
        let policy = fixture.allowed_policy();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert!(preview.targets.is_empty());

        apply_mcp_preview_with_probes(
            &std::sync::Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
            },
            &user_probe,
            &policy,
        )
        .unwrap();
        assert!(!fixture.project.join(".mcp.json").exists());
        let project_targets: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_targets
                 WHERE artifact_kind = 'mcp' AND scope = 'project' AND project_id = ?1",
                [&fixture.project_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(project_targets, 0, "纯继承项目不应产生无意义 target");
    }

    fn write_import_source(fixture: &Fixture, tool: Tool, items: Value) -> std::path::PathBuf {
        let path = match tool {
            Tool::Claude => fixture.home.join(".claude.json"),
            Tool::Codex => fixture.home.join(".codex/config.toml"),
            Tool::Cursor => fixture.home.join(".cursor/mcp.json"),
            Tool::Zcode => fixture.home.join(".zcode/cli/config.json"),
            Tool::Opencode => fixture.home.join(".config/opencode/opencode.json"),
        };
        let mut document =
            super::native_container(tool)
                .iter()
                .rev()
                .fold(items, |child, segment| {
                    let segment: &str = segment;
                    json!({ segment: child })
                });
        document
            .as_object_mut()
            .unwrap()
            .insert("unrelated".to_owned(), json!({ "keep": "outside" }));
        let text = match tool {
            Tool::Claude => serde_json::to_string_pretty(&document).unwrap(),
            Tool::Codex => toml_edit::ser::to_string(&document).unwrap(),
            Tool::Cursor => serde_json::to_string_pretty(&document).unwrap(),
            Tool::Zcode => serde_json::to_string_pretty(&document).unwrap(),
            Tool::Opencode => serde_json::to_string_pretty(&document).unwrap(),
        };
        fs::write(&path, text).unwrap();
        path
    }

    fn import_item(tool: Tool, input: &McpServerInput) -> Value {
        let configuration = super::ValidatedMcpConfiguration::from_create(input).unwrap();
        let mut item = super::native_mcp_item(tool, &configuration).unwrap();
        // 原生配置通常省略缺省值，首次同步应显示规范化而不是外部同名冲突。
        item.as_object_mut().unwrap().remove("type");
        item.as_object_mut().unwrap().remove("enabled");
        item
    }

    fn import_selection(
        preview: &crate::mcp::McpImportPreviewDto,
        names: &[&str],
    ) -> crate::mcp::ConfirmMcpImportInput {
        crate::mcp::ConfirmMcpImportInput {
            preview_id: preview.preview_id.clone().unwrap(),
            candidate_ids: preview
                .candidates
                .iter()
                .filter(|candidate| names.contains(&candidate.name.as_str()))
                .map(|candidate| candidate.candidate_id.clone())
                .collect(),
        }
    }

    fn import_counts(database: &Database) -> (i64, i64, i64, i64) {
        database.connection().query_row(
            "SELECT (SELECT COUNT(*) FROM mcp_servers), (SELECT COUNT(*) FROM mcp_global_assignments),
             (SELECT COUNT(*) FROM managed_targets WHERE artifact_kind = 'mcp'),
             (SELECT COUNT(*) FROM managed_items WHERE resource_kind = 'mcp')", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap()
    }

    #[test]
    fn mcp_import_selects_extends_and_syncs_without_touching_unselected_entries() {
        use crate::mcp::{confirm_mcp_import, discover_mcp_import, McpImportCandidateStatus};
        for tool in [Tool::Claude, Tool::Codex, Tool::Cursor] {
            let mut fixture = Fixture::new();
            let mut redactor = SecretRedactor::default();
            let disabled = json!({"command": "external", "enabled": false});
            let mut stdio = import_item(tool, &stdio_input("stdio"));
            let mut http = import_item(tool, &http_input("http"));
            if matches!(tool, Tool::Codex | Tool::Cursor) {
                // 保留真实来源常见的显式 type，不能让 helper 抹掉兼容性边界。
                stdio["type"] = json!("stdio");
                http["type"] = json!("http");
            }
            let path = write_import_source(
                &fixture,
                tool,
                json!({
                    "stdio": stdio,
                    "http": http,
                    "disabled": disabled,
                }),
            );
            let before = fs::read(&path).unwrap();
            let first =
                discover_mcp_import(&mut fixture.database, &fixture.environment, &redactor, tool)
                    .unwrap();
            assert_eq!(first.candidates.len(), 3);
            assert_eq!(
                first
                    .candidates
                    .iter()
                    .filter(|item| item.status == McpImportCandidateStatus::Importable)
                    .count(),
                2
            );
            assert_eq!(import_counts(&fixture.database), (0, 0, 0, 0));
            assert_eq!(
                first
                    .candidates
                    .iter()
                    .find(|item| item.name == "disabled")
                    .unwrap()
                    .status,
                McpImportCandidateStatus::Disabled
            );
            let selection = import_selection(&first, &["stdio"]);
            let result = confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &selection,
            )
            .unwrap();
            assert_eq!(result.created_count, 1);
            assert_eq!(import_counts(&fixture.database), (1, 1, 1, 1));
            assert_eq!(fs::read(&path).unwrap(), before);
            let repeat = confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &selection,
            )
            .unwrap_err();
            assert_eq!(repeat.code(), ErrorCode::PreviewAlreadyConsumed);
            let second =
                discover_mcp_import(&mut fixture.database, &fixture.environment, &redactor, tool)
                    .unwrap();
            assert_eq!(
                second
                    .candidates
                    .iter()
                    .find(|item| item.name == "stdio")
                    .unwrap()
                    .status,
                McpImportCandidateStatus::AlreadyManaged
            );
            let selection = import_selection(&second, &["http"]);
            confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &selection,
            )
            .unwrap();
            assert_eq!(import_counts(&fixture.database), (2, 2, 1, 2));
            assert_eq!(fs::read(&path).unwrap(), before);
            let preview = super::preview_mcp_sync(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &PreviewMcpSyncInput {
                    tool,
                    project_id: None,
                    exclude_from_git: false,
                },
            )
            .unwrap();
            assert_eq!(preview.targets.len(), 1);
            assert!(
                preview.targets[0].error_code.is_none(),
                "{:?}",
                preview.targets[0].status
            );
            assert_eq!(fs::read(&path).unwrap(), before);
            let applied = super::apply_mcp_preview(
                &std::sync::Mutex::new(()),
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &mut redactor,
                &ApplyMcpPreviewInput {
                    preview_id: preview.preview_id.clone(),
                    tool,
                    project_id: None,
                },
            )
            .unwrap();
            assert_eq!(applied.applied_targets, 1);
            let contents = fs::read_to_string(&path).unwrap();
            let after: Value = match tool {
                Tool::Claude => serde_json::from_str(&contents).unwrap(),
                Tool::Codex => toml_edit::de::from_str(&contents).unwrap(),
                Tool::Cursor => serde_json::from_str(&contents).unwrap(),
                Tool::Zcode => serde_json::from_str(&contents).unwrap(),
                Tool::Opencode => serde_json::from_str(&contents).unwrap(),
            };
            assert_eq!(
                super::service_projection_get(&after, super::native_container(tool))["disabled"],
                disabled
            );
            assert_eq!(after["unrelated"]["keep"], "outside");
            let records = crate::db::mcp::list_mcp_servers(&fixture.database).unwrap();
            assert!(records
                .iter()
                .any(|record| record.env_json.contains(ENV_SECRET)));
            assert!(records
                .iter()
                .any(|record| record.headers_json.contains(HEADER_SECRET)));
            let central = super::list_mcp_servers(&fixture.database, &redactor).unwrap();
            assert!(central.iter().all(|item| item.global_tools == vec![tool]));
            let mut carriers = vec![
                serde_json::to_string(&first).unwrap(),
                serde_json::to_string(&second).unwrap(),
                serde_json::to_string(&preview).unwrap(),
                serde_json::to_string(&central).unwrap(),
                serde_json::to_string(&result).unwrap(),
                serde_json::to_string(&repeat).unwrap(),
            ];
            for sql in [
                "SELECT context_json || redacted_preview_json FROM mcp_import_previews",
                "SELECT redacted_diff_json FROM sync_items",
            ] {
                let values = fixture
                    .database
                    .connection()
                    .prepare(sql)
                    .unwrap()
                    .query_map([], |row| row.get::<_, String>(0))
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                assert!(!values.is_empty());
                carriers.extend(values);
            }
            let journal = read_tree_text(fixture.paths.journals());
            assert!(!journal.is_empty());
            carriers.push(journal);
            for carrier in carriers {
                assert!(!carrier.is_empty());
                for secret in [ENV_SECRET, HEADER_SECRET, EXTRA_SECRET] {
                    assert!(!carrier.contains(secret));
                }
            }
        }
    }

    #[test]
    fn mcp_import_reuses_identical_cross_tool_records_and_blocks_conflicting_names() {
        use crate::mcp::{
            confirm_mcp_import, discover_mcp_import, McpImportAction, McpImportCandidateStatus,
        };
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        for tool in [Tool::Claude, Tool::Codex, Tool::Cursor, Tool::Zcode] {
            let mut native = import_item(tool, &stdio_input("shared"));
            if tool == Tool::Codex {
                native["type"] = json!("stdio");
            }
            write_import_source(&fixture, tool, json!({"shared": native}));
            let preview =
                discover_mcp_import(&mut fixture.database, &fixture.environment, &redactor, tool)
                    .unwrap();
            assert_eq!(
                preview.candidates[0].action,
                Some(if tool == Tool::Claude {
                    McpImportAction::Create
                } else {
                    McpImportAction::Reuse
                })
            );
            confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &import_selection(&preview, &["shared"]),
            )
            .unwrap();
        }
        assert_eq!(import_counts(&fixture.database), (1, 4, 4, 4));
        let central = super::list_mcp_servers(&fixture.database, &redactor).unwrap();
        assert_eq!(
            central[0].global_tools,
            vec![Tool::Claude, Tool::Codex, Tool::Cursor, Tool::Zcode]
        );
        let mut conflict_fixture = Fixture::new();
        super::create_mcp_server(
            &mut conflict_fixture.database,
            &mut redactor,
            &stdio_input("shared"),
        )
        .unwrap();
        write_import_source(
            &conflict_fixture,
            Tool::Claude,
            json!({"shared": {"command": "different"}, "SHARED": {"command": "npx"}}),
        );
        let preview = discover_mcp_import(
            &mut conflict_fixture.database,
            &conflict_fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(preview.preview_id.is_none());
        assert!(preview
            .candidates
            .iter()
            .all(|item| item.status == McpImportCandidateStatus::NameConflict));
        assert_eq!(import_counts(&conflict_fixture.database), (1, 0, 0, 0));
    }

    #[test]
    fn mcp_environment_classification_keeps_unknown_and_credential_values_protected() {
        for (key, value, credential) in [
            ("NODE_REPL_NODE_PATH", "/opt/fixture/node", false),
            ("PATH", "/opt/fixture/bin:/usr/bin", false),
            ("CODEX_HOME", "/opt/fixture/codex", false),
            ("CI", "true", false),
            ("NODE_REPL_NATIVE_PIPE_CONNECT_TIMEOUT_MS", "42", false),
            ("NODE_REPL_NODE_PATH", "relative-path", true),
            ("NODE_REPL_NODE_PATH", "/opt/../private", true),
            ("NODE_REPL_NODE_PATH", "/opt/token=opaque", true),
            ("PATH", "/usr/bin:relative", true),
            ("CI", "not-a-flag", true),
            (
                "NODE_REPL_NATIVE_PIPE_CONNECT_TIMEOUT_MS",
                "not-a-number",
                true,
            ),
            ("CUSTOM_PATH", "/opt/fixture/node", true),
            ("API_KEY", "/opt/fixture/node", true),
            ("UNKNOWN", "42", true),
        ] {
            let mut redactor = SecretRedactor::default();
            super::register_environment_value(&mut redactor, key, value);
            assert_eq!(redactor.contains_secret(value), credential, "{key}");
            assert_eq!(
                redactor.redact_json(&json!({"env": {key: value}}))["env"][key],
                "[REDACTED]"
            );
        }
        for reverse in [false, true] {
            let mut redactor = SecretRedactor::default();
            let keys = if reverse {
                ["API_KEY", "NODE_REPL_NODE_PATH"]
            } else {
                ["NODE_REPL_NODE_PATH", "API_KEY"]
            };
            for key in keys {
                super::register_environment_value(&mut redactor, key, "/opt/fixture/node");
            }
            assert!(redactor.contains_secret("/opt/fixture/node-wrapper"));
        }
    }

    #[test]
    fn mcp_import_preserves_credential_protection_and_reports_safe_field_reasons() {
        use crate::mcp::{discover_mcp_import, McpImportCandidateStatus as Status};

        let mut fixture = Fixture::new();
        let mut input = stdio_input("central-source");
        input.env =
            BTreeMap::from([("API_KEY".to_owned(), "opaque-central-credential".to_owned())]);
        create_mcp_server(
            &mut fixture.database,
            &mut SecretRedactor::default(),
            &input,
        )
        .unwrap();
        write_import_source(
            &fixture,
            Tool::Codex,
            json!({
                "valid": {"command": "server", "args": ["{ \"mode\": \"safe\" }"]},
                "source": {"enabled": false, "env": {"TOKEN": "42", "UNKNOWN": "/opt/private/value", "API_KEY": "/opt/private/shared"}},
                "runtime_source": {"enabled": false, "env": {"NODE_REPL_NODE_PATH": "/opt/private/shared"}},
                "header_source": {"enabled": false, "http_headers": {"X-Auth": "opaque-header-credential"}},
                "central_copy": {"command": "server", "args": ["opaque-central-credential"]},
                "short_copy": {"command": "/opt/worker42"},
                "unknown_copy": {"command": "/opt/private/value-helper"},
                "shared_copy": {"command": "/opt/private/shared-helper"},
                "header_copy": {"url": "https://example.test/opaque-header-credential"},
                "bad_args": {"command": "server", "args": 17},
                "bad_type": {"command": "server", "type": false},
                "mixed": {"type": "stdio", "command": "server", "url": "https://example.test"},
                "wrong_protocol": {"type": "http", "command": "server"},
                "unsupported": {"command": "server", "type": "sk-fixture-type-credential"},
                "refs": {"url": "https://example.test", "env_http_headers": {"Authorization": "TOKEN_VAR"}},
                "url_query": {"url": "https://example.test?token=opaque-query-credential"},
                "nested_json": {"command": "server", "args": ["{\"outer\": {\"token\": \"opaque-json-credential\"}}"]},
            }),
        );
        // 使用全新 redactor，验证中央凭据恢复和原生拒绝项的凭据登记。
        let preview = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(
            preview
                .candidates
                .iter()
                .filter(|item| item.status == Status::Importable)
                .count(),
            1
        );
        for (name, field, status) in [
            ("central_copy", "args", Status::Invalid),
            ("short_copy", "command", Status::Invalid),
            ("unknown_copy", "command", Status::Invalid),
            ("shared_copy", "command", Status::Invalid),
            ("header_copy", "url", Status::Invalid),
            ("bad_args", "args", Status::Invalid),
            ("bad_type", "type", Status::Invalid),
            ("mixed", "command/url", Status::Invalid),
            ("wrong_protocol", "command/url", Status::Invalid),
            ("unsupported", "type", Status::Unsupported),
            ("refs", "env_http_headers", Status::Unsupported),
            ("url_query", "url", Status::Invalid),
            ("nested_json", "args", Status::Invalid),
        ] {
            let item = preview
                .candidates
                .iter()
                .find(|item| item.name == name)
                .unwrap();
            assert_eq!(item.status, status, "{name}");
            assert!(item.reason.as_ref().unwrap().contains(field), "{name}");
            assert!(item.redacted_projection.is_null(), "{name}");
        }
        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT context_json || redacted_preview_json FROM mcp_import_previews",
                [],
                |row| row.get(0),
            )
            .unwrap();
        for carrier in [serde_json::to_string(&preview).unwrap(), stored] {
            assert!(!carrier.is_empty());
            for credential in [
                "opaque-central-credential",
                "/opt/private/value",
                "/opt/private/shared",
                "opaque-header-credential",
                "sk-fixture-type-credential",
                "opaque-query-credential",
                "opaque-json-credential",
            ] {
                assert!(!carrier.contains(credential));
            }
        }
    }

    #[test]
    fn mcp_import_runtime_values_survive_crud_confirmation_preview_and_rescan() {
        use crate::mcp::{
            confirm_mcp_import, discover_mcp_import, McpImportCandidateStatus as Status,
        };

        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let mut input = stdio_input("runtime");
        input.command = Some("/opt/fixture/node-wrapper".to_owned());
        input.args = vec![
            "1".to_owned(),
            "12345".to_owned(),
            "{ \"mode\": \"safe\" }".to_owned(),
        ];
        input.env = BTreeMap::from([
            (
                "NODE_REPL_NODE_PATH".to_owned(),
                "/opt/fixture/node".to_owned(),
            ),
            ("BROWSER_USE_TINYSKY_ENABLED".to_owned(), "1".to_owned()),
            (
                "NODE_REPL_NATIVE_PIPE_CONNECT_TIMEOUT_MS".to_owned(),
                "12345".to_owned(),
            ),
        ]);
        let central = create_mcp_server(&mut fixture.database, &mut redactor, &input).unwrap();
        update_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &UpdateMcpServerInput {
                id: central.id,
                row_version: central.row_version,
                name: input.name.clone(),
                transport: input.transport,
                command: input.command.clone(),
                args: input.args.clone(),
                url: None,
                headers: SensitiveMapUpdate::Keep,
                env: SensitiveMapUpdate::Keep,
                extra: SensitiveJsonUpdate::Keep,
                enabled: true,
            },
        )
        .unwrap();
        let path = write_import_source(
            &fixture,
            Tool::Codex,
            json!({
                "runtime": import_item(Tool::Codex, &input),
                "neighbor": {"command": "/opt/fixture/node-helper", "args": ["1", "12345"]},
                "http_alias": {"type": "streamable_http", "url": "https://example.test/mcp"},
                "disabled": {"command": "external", "enabled": false},
            }),
        );
        let before = fs::read(&path).unwrap();
        let first = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        assert_eq!(
            first
                .candidates
                .iter()
                .filter(|item| item.status == Status::Importable)
                .count(),
            3
        );
        let runtime = first
            .candidates
            .iter()
            .find(|item| item.name == "runtime")
            .unwrap();
        assert_eq!(runtime.action, Some(crate::mcp::McpImportAction::Reuse));
        assert_eq!(
            runtime.redacted_projection["env"]["NODE_REPL_NODE_PATH"],
            "[REDACTED]"
        );
        confirm_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &import_selection(&first, &["runtime"]),
        )
        .unwrap();
        let sync_input = PreviewMcpSyncInput {
            tool: Tool::Codex,
            project_id: None,
            exclude_from_git: false,
        };
        super::preview_mcp_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &sync_input,
        )
        .unwrap();
        // 再次扫描及新进程的空 redactor 都不能把已导入的运行路径升级为凭据。
        for current in [&redactor, &SecretRedactor::default()] {
            let next = discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                current,
                Tool::Codex,
            )
            .unwrap();
            for item in next.candidates {
                let expected = match item.name.as_str() {
                    "runtime" => Status::AlreadyManaged,
                    "disabled" => Status::Disabled,
                    _ => Status::Importable,
                };
                assert_eq!(item.status, expected, "{}", item.name);
            }
        }
        let next = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        confirm_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &import_selection(&next, &["neighbor", "http_alias"]),
        )
        .unwrap();
        let preview = super::preview_mcp_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &sync_input,
        )
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        let applied = super::apply_mcp_preview(
            &std::sync::Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id.clone(),
                tool: Tool::Codex,
                project_id: None,
            },
        )
        .unwrap();
        assert_eq!(applied.applied_targets, 1);
        let after: Value = toml_edit::de::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            after["mcp_servers"]["runtime"]["env"],
            serde_json::to_value(&input.env).unwrap()
        );
        assert_eq!(after["mcp_servers"]["runtime"]["args"], json!(input.args));
        assert_eq!(after["mcp_servers"]["runtime"]["startup_timeout_sec"], 10);
        assert!(after["mcp_servers"]["http_alias"].get("type").is_none());
        assert_eq!(after["mcp_servers"]["disabled"]["enabled"], false);
        assert_eq!(after["unrelated"]["keep"], "outside");
        let central = super::list_mcp_servers(&fixture.database, &redactor).unwrap();
        for carrier in [
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&preview).unwrap(),
            serde_json::to_string(&central).unwrap(),
            read_tree_text(fixture.paths.journals()),
        ] {
            assert!(!carrier.is_empty());
            assert!(!carrier.contains(EXTRA_SECRET));
        }
    }

    #[test]
    fn mcp_import_compares_private_values_and_respects_project_assignments() {
        use crate::mcp::{discover_mcp_import, McpImportCandidateStatus};

        for project_assigned in [false, true] {
            let mut fixture = Fixture::new();
            let mut redactor = SecretRedactor::default();
            let input = stdio_input("shared");
            let central = create_mcp_server(&mut fixture.database, &mut redactor, &input).unwrap();
            let mut native = import_item(Tool::Claude, &input);
            if project_assigned {
                fixture.database.connection().execute(
                    "INSERT INTO mcp_project_assignments(project_id, tool, mcp_id) VALUES (?1, 'claude', ?2)",
                    rusqlite::params![fixture.project_id, central.id],
                ).unwrap();
            } else {
                native["env"]["MCP_TOKEN"] = json!("different-private-value");
            }
            let path = write_import_source(&fixture, Tool::Claude, json!({"shared": native}));
            let before = fs::read(&path).unwrap();
            let preview = discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &redactor,
                Tool::Claude,
            )
            .unwrap();
            assert!(preview.preview_id.is_none());
            assert_eq!(
                preview.candidates[0].status,
                McpImportCandidateStatus::NameConflict
            );
            if project_assigned {
                assert!(preview.candidates[0]
                    .reason
                    .as_ref()
                    .unwrap()
                    .contains("项目分配"));
            }
            assert_eq!(import_counts(&fixture.database), (1, 0, 0, 0));
            assert_eq!(fs::read(path).unwrap(), before);
            let stored = crate::db::mcp::get_mcp_server(&fixture.database, &central.id).unwrap();
            assert_eq!(
                super::configuration_from_record(&stored, &SecretRedactor::default()).unwrap(),
                super::ValidatedMcpConfiguration::from_create(&input).unwrap()
            );
        }
    }

    #[test]
    fn mcp_import_rejects_unsupported_and_secret_entries_individually() {
        use crate::mcp::{discover_mcp_import, McpImportCandidateStatus};
        let mut fixture = Fixture::new();
        let secret = "Bearer import-rejected-secret";
        write_import_source(
            &fixture,
            Tool::Claude,
            json!({
                "valid": {"command": "npx"}, "disabled": {"command": "npx", "disabled": true},
                "sse": {"type": "sse", "url": "https://example.test"},
                "secret": {"command": "npx", "args": ["--token", secret]},
                "malformed": {"command": "npx", "args": null},
                "refs": {"url": "https://example.test", "env_http_headers": {"Authorization": "API_TOKEN"}},
                "scalar": 42,
            }),
        );
        let result = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            result
                .candidates
                .iter()
                .filter(|item| item.status == McpImportCandidateStatus::Importable)
                .count(),
            1
        );
        assert_eq!(result.candidates.len(), 7);
        assert!(!serde_json::to_string(&result).unwrap().contains(secret));
        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT context_json || redacted_preview_json FROM mcp_import_previews",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!stored.is_empty());
        assert!(!stored.contains(secret));
        assert_eq!(import_counts(&fixture.database), (0, 0, 0, 0));
    }

    #[test]
    fn mcp_import_rejects_stale_file_database_and_invalid_selections() {
        use crate::mcp::{confirm_mcp_import, discover_mcp_import};
        for change_database in [false, true] {
            let mut fixture = Fixture::new();
            let mut redactor = SecretRedactor::default();
            let path = write_import_source(
                &fixture,
                Tool::Claude,
                json!({"sample": {"command": "npx"}}),
            );
            let preview = discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &redactor,
                Tool::Claude,
            )
            .unwrap();
            let selection = import_selection(&preview, &["sample"]);
            for ids in [
                vec![],
                vec![Uuid::new_v4().to_string()],
                vec![selection.candidate_ids[0].clone(); 2],
            ] {
                let input = crate::mcp::ConfirmMcpImportInput {
                    preview_id: selection.preview_id.clone(),
                    candidate_ids: ids,
                };
                let error = confirm_mcp_import(
                    &mut fixture.database,
                    &fixture.environment,
                    &mut redactor,
                    &input,
                )
                .unwrap_err();
                assert_eq!(error.code(), ErrorCode::InvalidInput);
                assert_eq!(import_counts(&fixture.database), (0, 0, 0, 0));
            }
            if change_database {
                super::create_mcp_server(
                    &mut fixture.database,
                    &mut redactor,
                    &stdio_input("unrelated"),
                )
                .unwrap();
            } else {
                let text = fs::read_to_string(&path).unwrap();
                fs::write(&path, format!("{text}\n")).unwrap();
            }
            let before = fs::read(&path).unwrap();
            let error = confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &selection,
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::StalePreview);
            assert_eq!(
                import_counts(&fixture.database),
                (i64::from(change_database), 0, 0, 0)
            );
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn mcp_import_never_refreshes_drifted_existing_baselines() {
        use crate::mcp::{confirm_mcp_import, discover_mcp_import};
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        write_import_source(
            &fixture,
            Tool::Claude,
            json!({"first": {"command": "npx"}, "second": {"command": "uvx"}}),
        );
        let preview = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        confirm_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &import_selection(&preview, &["first"]),
        )
        .unwrap();
        let old_hash: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_managed_hash FROM managed_targets",
                [],
                |row| row.get(0),
            )
            .unwrap();
        write_import_source(
            &fixture,
            Tool::Claude,
            json!({"first": {"command": "changed"}, "second": {"command": "uvx"}}),
        );
        let preview = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        let error = confirm_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &import_selection(&preview, &["second"]),
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(import_counts(&fixture.database), (1, 1, 1, 1));
        let unchanged: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_managed_hash FROM managed_targets",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unchanged, old_hash);
    }

    #[test]
    fn mcp_import_rejects_central_target_and_item_row_version_changes() {
        use crate::mcp::{confirm_mcp_import, discover_mcp_import};

        for sql in [
            "UPDATE mcp_servers SET updated_at = updated_at",
            "UPDATE managed_targets SET updated_at = updated_at",
            "UPDATE managed_items SET updated_at = updated_at",
        ] {
            let mut fixture = Fixture::new();
            let mut redactor = SecretRedactor::default();
            let path = write_import_source(
                &fixture,
                Tool::Claude,
                json!({
                    "first": {"command": "npx"}, "second": {"command": "uvx"}
                }),
            );
            let before = fs::read(&path).unwrap();
            let first = discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &redactor,
                Tool::Claude,
            )
            .unwrap();
            confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &import_selection(&first, &["first"]),
            )
            .unwrap();
            let second = discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &redactor,
                Tool::Claude,
            )
            .unwrap();
            fixture.database.connection().execute(sql, []).unwrap();
            let error = confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &import_selection(&second, &["second"]),
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::StalePreview);
            assert_eq!(import_counts(&fixture.database), (1, 1, 1, 1));
            assert_eq!(fs::read(path).unwrap(), before);
        }
    }

    #[test]
    fn mcp_import_rolls_back_the_entire_batch_when_adoption_fails() {
        use crate::mcp::{confirm_mcp_import, discover_mcp_import};
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let path = write_import_source(
            &fixture,
            Tool::Codex,
            json!({"first": {"command": "npx"}, "second": {"command": "uvx"}}),
        );
        let before = fs::read(&path).unwrap();
        let preview = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap();
        fixture.database.connection().execute_batch("CREATE TRIGGER reject_import_item BEFORE INSERT ON managed_items
            WHEN (SELECT COUNT(*) FROM managed_items) > 0 BEGIN SELECT RAISE(ABORT, 'fixture'); END;").unwrap();
        let error = confirm_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &import_selection(&preview, &["first", "second"]),
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(import_counts(&fixture.database), (0, 0, 0, 0));
        let status: String = fixture
            .database
            .connection()
            .query_row("SELECT status FROM mcp_import_previews", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(status, "previewed");
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn mcp_import_revalidates_source_inside_the_transaction_and_before_commit() {
        use std::cell::Cell;

        use crate::{db::mcp_imports, error::AppError, mcp::discover_mcp_import, sync::hash_json};

        for fail_at in [1, 2] {
            let mut fixture = Fixture::new();
            let input = stdio_input("sample");
            let raw = import_item(Tool::Claude, &input);
            let path = write_import_source(&fixture, Tool::Claude, json!({"sample": raw}));
            let before = fs::read(&path).unwrap();
            let preview = discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &SecretRedactor::default(),
                Tool::Claude,
            )
            .unwrap();
            let record =
                mcp_imports::get_preview(&fixture.database, &preview.preview_id.unwrap()).unwrap();
            let state = mcp_imports::state_fingerprint(
                fixture.database.connection(),
                Tool::Claude,
                &record.target_path,
            )
            .unwrap();
            let calls = Cell::new(0);
            let error = mcp_imports::adopt_import(
                &mut fixture.database,
                &record,
                &state,
                None,
                &json!({"mcpServers": {"sample": raw}}),
                &[mcp_imports::ImportedMcpItem {
                    configuration: super::ValidatedMcpConfiguration::from_create(&input).unwrap(),
                    reuse_id: None,
                    item_hash: hash_json(&raw),
                }],
                || {
                    calls.set(calls.get() + 1);
                    if calls.get() == fail_at {
                        Err(AppError::stale_preview(&record.id, &record.target_path))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::StalePreview);
            assert_eq!(calls.get(), fail_at);
            assert_eq!(import_counts(&fixture.database), (0, 0, 0, 0));
            assert_eq!(
                mcp_imports::get_preview(&fixture.database, &record.id)
                    .unwrap()
                    .status,
                "previewed"
            );
            assert_eq!(fs::read(path).unwrap(), before);
        }
    }

    #[test]
    fn mcp_import_blocks_active_writers_without_consuming_the_token() {
        use crate::mcp::{confirm_mcp_import, discover_mcp_import};

        for status in ["applying", "restoring", "rollback_failed"] {
            let mut fixture = Fixture::new();
            let mut redactor = SecretRedactor::default();
            let path =
                write_import_source(&fixture, Tool::Codex, json!({"sample": {"command": "npx"}}));
            let before = fs::read(&path).unwrap();
            let preview = discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &redactor,
                Tool::Codex,
            )
            .unwrap();
            let selection = import_selection(&preview, &["sample"]);
            let run_id = Uuid::new_v4().to_string();
            fixture.database.connection().execute(
                "INSERT INTO sync_runs(id, kind, status, scope, db_version) VALUES (?1, 'apply', ?2, 'global', 0)",
                rusqlite::params![run_id, status],
            ).unwrap();
            let error = confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &selection,
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::WriteInProgress);
            assert_eq!(import_counts(&fixture.database), (0, 0, 0, 0));
            fixture
                .database
                .connection()
                .execute("DELETE FROM sync_runs WHERE id = ?1", [&run_id])
                .unwrap();
            confirm_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &selection,
            )
            .unwrap();
            assert_eq!(import_counts(&fixture.database), (1, 1, 1, 1));
            assert_eq!(fs::read(path).unwrap(), before);
        }
    }

    #[test]
    fn mcp_import_distinguishes_missing_parse_policy_and_unsafe_paths() {
        use crate::mcp::discover_mcp_import;
        let mut fixture = Fixture::new();
        let redactor = SecretRedactor::default();
        let missing = discover_mcp_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(missing.candidates.is_empty());
        assert!(missing.preview_id.is_none());
        assert!(missing.message.unwrap().contains("未发现"));
        fs::write(fixture.home.join(".claude.json"), "not json").unwrap();
        assert_eq!(
            discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &redactor,
                Tool::Claude
            )
            .unwrap_err()
            .code(),
            ErrorCode::ParseError
        );
        let environment = fixture.environment_without_policy_evidence();
        assert_eq!(
            discover_mcp_import(&mut fixture.database, &environment, &redactor, Tool::Claude)
                .unwrap_err()
                .code(),
            ErrorCode::PolicyBlocked
        );
        fs::remove_file(fixture.home.join(".claude.json")).unwrap();
        std::os::unix::fs::symlink(
            fixture.home.join("missing-target"),
            fixture.home.join(".claude.json"),
        )
        .unwrap();
        assert_eq!(
            discover_mcp_import(
                &mut fixture.database,
                &fixture.environment,
                &redactor,
                Tool::Claude
            )
            .unwrap_err()
            .code(),
            ErrorCode::Conflict
        );
        assert_eq!(import_counts(&fixture.database), (0, 0, 0, 0));
    }

    fn read_tree_text(root: &Path) -> String {
        let mut output = String::new();
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    output.push_str(&read_tree_text(&path));
                } else if let Ok(text) = fs::read_to_string(path) {
                    output.push_str(&text);
                }
            }
        }
        output
    }

    #[test]
    fn readopt_refreshes_baselines_and_unlocks_conflicted_target() {
        let mut fixture = Fixture::new();
        let write_operations = std::sync::Mutex::new(());
        let claude_path = fixture.home.join(".claude.json");
        fs::write(
            &claude_path,
            br#"{"theme":"dark","mcpServers":{"external":{"command":"keep"}}}"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let created = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("managed-a"),
        )
        .unwrap();
        set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: created.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();

        // 外部工具改写受管条目 → 目标冲突，并列出具体不匹配条目。
        fs::write(
            &claude_path,
            br#"{"theme":"dark","mcpServers":{"external":{"command":"keep"},"managed-a":{"command":"hijacked"}}}"#,
        )
        .unwrap();
        let conflicted = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();
        assert_eq!(conflicted.targets[0].change_kind.as_str(), "conflict");
        assert!(conflicted.targets[0].readopt_available);
        assert_eq!(
            conflicted.targets[0].baseline_mismatched_items,
            vec!["managed-a".to_owned()]
        );

        let central_before = get_mcp_server(&fixture.database, &redactor, &created.id).unwrap();
        let result = readopt_mcp_target(
            &mut fixture.database,
            &fixture.environment,
            &ReadoptMcpTargetInput {
                tool: Tool::Claude,
                project_id: None,
            },
        )
        .unwrap();
        assert_eq!(result.updated_item_count, 1);
        assert_eq!(result.removed_item_count, 0);
        let central_after = get_mcp_server(&fixture.database, &redactor, &created.id).unwrap();
        assert_eq!(central_before.row_version, central_after.row_version);

        // 接管后冲突解除，正常同步把中央意图写回。
        let recovered = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();
        assert_ne!(recovered.targets[0].change_kind.as_str(), "conflict");
        assert!(recovered.targets[0].error_code.is_none());
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: recovered.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();
        let native: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
        assert_eq!(native["mcpServers"]["managed-a"]["command"], "npx");
        // 接管不改中央意图。
        let central = get_mcp_server(&fixture.database, &redactor, &created.id).unwrap();
        assert_eq!(central.row_version, created.row_version + 1);
    }

    #[test]
    fn readopt_missing_target_clears_baselines_for_rebuild() {
        let mut fixture = Fixture::new();
        let write_operations = std::sync::Mutex::new(());
        let claude_path = fixture.home.join(".claude.json");
        fs::write(&claude_path, br#"{"mcpServers":{}}"#).unwrap();
        let mut redactor = SecretRedactor::default();
        let created = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("managed-a"),
        )
        .unwrap();
        set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: created.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let preview = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();
        apply_mcp_preview_with_probes(
            &write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();

        fs::remove_file(&claude_path).unwrap();
        let conflicted = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();
        assert_eq!(conflicted.targets[0].change_kind.as_str(), "conflict");
        assert_eq!(
            conflicted.targets[0].baseline_mismatched_items,
            vec!["managed-a".to_owned()]
        );

        let result = readopt_mcp_target(
            &mut fixture.database,
            &fixture.environment,
            &ReadoptMcpTargetInput {
                tool: Tool::Claude,
                project_id: None,
            },
        )
        .unwrap();
        assert_eq!(result.removed_item_count, 1);

        // 基线清空后目标回到「缺失、可创建」状态。
        let recovered = preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();
        assert_eq!(recovered.targets[0].change_kind.as_str(), "add");
        assert!(recovered.targets[0].error_code.is_none());
    }

    #[test]
    fn readopt_refuses_unreadable_targets() {
        let mut fixture = Fixture::new();
        let mut redactor = SecretRedactor::default();
        let created = create_mcp_server(
            &mut fixture.database,
            &mut redactor,
            &stdio_input("managed-a"),
        )
        .unwrap();
        set_global_mcp_assignment(
            &mut fixture.database,
            &redactor,
            &SetGlobalMcpAssignmentInput {
                tool: Tool::Claude,
                mcp_id: created.id.clone(),
                assigned: true,
                row_version: created.row_version,
            },
        )
        .unwrap();
        // 首次预览会创建 managed_targets 行（目标缺失但可创建）。
        let policy = fixture.allowed_policy();
        preview_mcp_sync_with_probes(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            fixture.environment.claude_user_mcp_probe(),
            &policy,
        )
        .unwrap();

        fs::write(fixture.home.join(".claude.json"), b"{not-json").unwrap();
        let error = readopt_mcp_target(
            &mut fixture.database,
            &fixture.environment,
            &ReadoptMcpTargetInput {
                tool: Tool::Claude,
                project_id: None,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
    }
}
