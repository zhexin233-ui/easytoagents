#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::ToolAvailability,
        adapters::VerifiedClaudeCustomizationPolicyEvidence,
        error::ErrorCode,
        mcp::{preview_mcp_sync, PreviewMcpSyncInput},
        projects::RegisterProjectInput,
        sync::{
            delete_snapshots, detect_interrupted_run, ApplyFaultDecision, ApplyFaultEvent,
            ApplyFaultInjector, DeleteSnapshotsInput,
        },
    };
    use rusqlite::params;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;
    use uuid::Uuid;

    struct CrashBeforeTarget;
    impl ApplyFaultInjector for CrashBeforeTarget {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            match event {
                ApplyFaultEvent::BeforeTarget { .. } => ApplyFaultDecision::Crash,
                _ => ApplyFaultDecision::Continue,
            }
        }
    }

    struct FailAfterTarget;
    impl ApplyFaultInjector for FailAfterTarget {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            match event {
                ApplyFaultEvent::AfterTarget { .. } => ApplyFaultDecision::Fail,
                _ => ApplyFaultDecision::Continue,
            }
        }
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        database: Database,
        environment: ExplicitEnvironment,
        paths: AppPaths,
        home: PathBuf,
        write_operations: Mutex<()>,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let home = fs::canonicalize(temporary.path()).unwrap();
            fs::create_dir_all(home.join(".claude")).unwrap();
            fs::create_dir_all(home.join(".codex")).unwrap();
            fs::create_dir_all(home.join(".cursor")).unwrap();
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
            let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
            paths.initialize().unwrap();
            let database = Database::open(&paths).unwrap();
            Self {
                _temporary: temporary,
                database,
                environment,
                paths,
                home,
                write_operations: Mutex::new(()),
            }
        }

        fn register_project_with(
            &mut self,
            files: impl FnOnce(&Path),
        ) -> crate::projects::ProjectDto {
            let root = self.home.join("projects/native");
            fs::create_dir_all(&root).unwrap();
            files(&root);
            crate::projects::register_project(
                &mut self.database,
                &self.environment,
                &RegisterProjectInput {
                    display_name: "原生项目".to_owned(),
                    root_path: root.to_string_lossy().into_owned(),
                },
            )
            .unwrap()
        }
    }

    fn write_skill(dir: &Path, name: &str, body: &str) {
        let skill = dir.join(name);
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Fixture skill\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    fn native_resource_rows(database: &Database) -> Vec<(String, String, i64)> {
        let mut statement = database
            .connection()
            .prepare(
                "SELECT external_key, state, row_version FROM project_native_resources
                 ORDER BY external_key",
            )
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn project_reads_do_not_write_native_resource_rows() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(
                root.join(".mcp.json"),
                br#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#,
            )
            .unwrap();
            write_skill(&root.join(".claude/skills"), "native-dir", "workflow");
        });
        let before = native_resource_rows(&fixture.database);
        assert_eq!(before.len(), 2);

        // 外部改动原生文件后，读路径必须原样返回旧观测，不触发对账写库。
        fs::write(
            fixture.home.join("projects/native/.mcp.json"),
            br#"{"mcpServers":{"native-stdio":{"command":"npx"},"added-later":{"command":"uvx"}}}"#,
        )
        .unwrap();
        let listed =
            crate::projects::list_projects(&mut fixture.database, &fixture.environment).unwrap();
        assert_eq!(listed.len(), 1);
        let fetched =
            crate::projects::get_project(&mut fixture.database, &fixture.environment, &project.id)
                .unwrap();
        assert_eq!(fetched.native_resources.active, 2);
        assert_eq!(native_resource_rows(&fixture.database), before);

        // 显式重扫才对账，新增条目此时出现。
        let rescanned = crate::projects::rescan_project(
            &mut fixture.database,
            &fixture.environment,
            &crate::projects::VersionedProjectInput {
                id: project.id.clone(),
                row_version: fetched.row_version,
            },
        )
        .unwrap();
        assert_eq!(rescanned.native_resources.active, 3);
        assert_eq!(native_resource_rows(&fixture.database).len(), 3);
    }

    #[test]
    fn reconcile_is_atomic_when_a_write_fails_mid_project() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(
                root.join(".mcp.json"),
                br#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#,
            )
            .unwrap();
            write_skill(&root.join(".claude/skills"), "native-dir", "workflow");
        });
        let before = native_resource_rows(&fixture.database);
        assert_eq!(before.len(), 2);

        // 让 Skill 目标（对账顺序靠后）的写入在数据库层失败：外部改动 MCP 文件
        // 使第一个描述符产生真实更新，同时用触发器让 Skill 的观测写入中止。
        fs::write(
            fixture.home.join("projects/native/.mcp.json"),
            br#"{"mcpServers":{"native-stdio":{"command":"npx"},"added-later":{"command":"uvx"}}}"#,
        )
        .unwrap();
        fixture
            .database
            .connection()
            .execute_batch(
                "CREATE TEMP TRIGGER fail_skill_reconcile
                 BEFORE UPDATE ON project_native_resources
                 WHEN NEW.entry_type = 'directory'
                 BEGIN SELECT RAISE(ABORT, 'FIXTURE_FAILURE'); END;",
            )
            .unwrap();
        let error = reconcile_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &project.id,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::DatabaseError);
        // 第一个描述符的新增条目必须随事务回滚，表内无半更新。
        assert_eq!(native_resource_rows(&fixture.database), before);

        fixture
            .database
            .connection()
            .execute_batch("DROP TRIGGER fail_skill_reconcile;")
            .unwrap();
        let summary = reconcile_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &project.id,
        )
        .unwrap();
        assert_eq!(summary.active, 3);
    }

    #[test]
    fn registration_discovers_native_items_without_rewriting() {
        let mut fixture = Fixture::new();
        let before_mcp = br#"{"mcpServers":{"native-stdio":{"command":"npx","env":{"API_KEY":"sk-native-secret"}}}}"#;
        let project = fixture.register_project_with(|root| {
            fs::write(root.join(".mcp.json"), before_mcp).unwrap();
            write_skill(&root.join(".claude/skills"), "native-dir", "workflow");
            let link = root.join(".claude/skills/native-link");
            symlink(root.join(".claude/skills/native-dir"), &link).unwrap();
        });
        assert_eq!(project.native_resources.active, 3);
        let after = fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap();
        assert_eq!(after, before_mcp);
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].display_name, "native-stdio");
        assert!(items[0].can_disable);
        let serialized = serde_json::to_string(&items[0]).unwrap();
        assert!(!serialized.contains("sk-native-secret"));
        assert!(!serialized.contains("npx"));
    }

    #[test]
    fn mcp_disable_restore_preserves_siblings_and_blocks_snapshot_delete() {
        let mut fixture = Fixture::new();
        let secret = "sk-native-secret";
        let project = fixture.register_project_with(|root| {
            fs::write(
                root.join(".mcp.json"),
                format!(
                    r#"{{"theme":"keep","mcpServers":{{"native-stdio":{{"command":"npx","env":{{"API_KEY":"{secret}"}}}},"sibling":{{"command":"keep"}}}}}}"#
                ),
            )
            .unwrap();
        });
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        let native = items
            .into_iter()
            .find(|item| item.display_name == "native-stdio")
            .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: native.id.clone(),
                row_version: native.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let preview_json = serde_json::to_string(&preview).unwrap();
        assert!(!preview_json.contains(secret));
        assert!(preview
            .warning_codes
            .iter()
            .any(|code| code == "PROJECT_NATIVE_RESOURCE_REQUIRES_CONFIRMATION"));
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-stdio")
        .unwrap();
        assert_eq!(disabled.state, ProjectNativeResourceState::Disabled);
        let native_file: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
        )
        .unwrap();
        assert!(native_file["mcpServers"].get("native-stdio").is_none());
        assert_eq!(native_file["mcpServers"]["sibling"]["command"], "keep");
        assert_eq!(native_file["theme"], "keep");
        let snapshot_id = repository::get_by_id(&fixture.database, &disabled.id)
            .unwrap()
            .disabled_snapshot_id
            .unwrap();
        let deletion = delete_snapshots(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id.clone()],
            },
        )
        .unwrap();
        assert_eq!(deletion.deleted_ids.len(), 0);
        assert_eq!(deletion.failures.len(), 1);
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        assert!(!serde_json::to_string(&restore).unwrap().contains(secret));
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            restored["mcpServers"]["native-stdio"]["env"]["API_KEY"],
            secret
        );
        assert_eq!(restored["mcpServers"]["sibling"]["command"], "keep");
        let deletion = delete_snapshots(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id],
            },
        )
        .unwrap();
        assert_eq!(deletion.deleted_ids.len(), 1);
    }

    #[test]
    fn codex_mcp_disable_restore_preserves_toml_comments_and_siblings() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".codex")).unwrap();
            fs::write(
                root.join(".codex/config.toml"),
                "# keep-sibling-comment\n[mcp_servers.sibling]\ncommand = \"keep\"\n\n[mcp_servers.native-toml]\ncommand = \"npx\"\n",
            )
            .unwrap();
        });
        let canonical = fs::canonicalize(fixture.home.join("projects/native")).unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                canonical.display()
            ),
        )
        .unwrap();
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Codex,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        let native = items
            .into_iter()
            .find(|item| item.display_name == "native-toml")
            .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: native.id.clone(),
                row_version: native.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        let after = fs::read_to_string(canonical.join(".codex/config.toml")).unwrap();
        assert!(after.contains("keep-sibling-comment"));
        assert!(after.contains("[mcp_servers.sibling]"));
        assert!(!after.contains("[mcp_servers.native-toml]"));
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Codex,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-toml")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored = fs::read_to_string(canonical.join(".codex/config.toml")).unwrap();
        assert!(restored.contains("[mcp_servers.native-toml]"));
        assert!(restored.contains("[mcp_servers.sibling]"));
        assert!(restored.contains("keep-sibling-comment"));
    }

    #[test]
    fn cursor_mcp_disable_restore_preserves_sibling_servers() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".cursor")).unwrap();
            fs::write(
                root.join(".cursor/mcp.json"),
                r#"{"mcpServers":{"native-cursor":{"command":"npx"},"sibling":{"command":"keep"}}}"#,
            )
            .unwrap();
        });
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Cursor,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        let native = items
            .into_iter()
            .find(|item| item.display_name == "native-cursor")
            .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: native.id.clone(),
                row_version: native.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        let native_file: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.cursor/mcp.json")).unwrap(),
        )
        .unwrap();
        assert!(native_file["mcpServers"].get("native-cursor").is_none());
        assert_eq!(native_file["mcpServers"]["sibling"]["command"], "keep");
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Cursor,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-cursor")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored: Value = serde_json::from_slice(
            &fs::read(fixture.home.join("projects/native/.cursor/mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(restored["mcpServers"]["native-cursor"]["command"], "npx");
        assert_eq!(restored["mcpServers"]["sibling"]["command"], "keep");
    }

    #[test]
    fn skill_directory_and_symlink_disable_restore() {
        let mut fixture = Fixture::new();
        let home = fixture.home.clone();
        let project = fixture.register_project_with(|root| {
            write_skill(&root.join(".claude/skills"), "native-dir", "keep-bytes");
            write_skill(&home, "external-skill", "external");
            symlink(
                home.join("external-skill"),
                root.join(".claude/skills/native-link"),
            )
            .unwrap();
            fs::write(root.join(".claude/skills/.DS_Store"), b"ignore").unwrap();
        });
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        let directory = items
            .iter()
            .find(|item| item.display_name == "native-dir")
            .unwrap()
            .clone();
        assert!(items.iter().any(|item| item.display_name == "native-link"));
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: directory.id.clone(),
                row_version: directory.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        assert!(!fixture
            .home
            .join("projects/native/.claude/skills/native-dir")
            .exists());
        assert!(fixture
            .home
            .join("projects/native/.claude/skills/.DS_Store")
            .exists());
        let disabled = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-dir")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled.id.clone(),
                row_version: disabled.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore.preview_id,
            },
        )
        .unwrap();
        let restored = fixture
            .home
            .join("projects/native/.claude/skills/native-dir/SKILL.md");
        assert!(fs::read_to_string(restored).unwrap().contains("keep-bytes"));

        let link = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-link")
        .unwrap();
        let outside_before = fs::canonicalize(fixture.home.join("external-skill")).unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: link.id.clone(),
                row_version: link.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
        )
        .unwrap();
        assert!(!fixture
            .home
            .join("projects/native/.claude/skills/native-link")
            .exists());
        assert_eq!(
            fs::canonicalize(fixture.home.join("external-skill")).unwrap(),
            outside_before
        );
        assert!(fixture.home.join("external-skill/SKILL.md").exists());
        let disabled_link = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-link")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let restore_link = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: disabled_link.id.clone(),
                row_version: disabled_link.row_version,
                action: ProjectNativeResourceAction::Restore,
            },
        )
        .unwrap();
        apply_project_native_resource_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: restore_link.preview_id,
            },
        )
        .unwrap();
        let restored_link = fixture
            .home
            .join("projects/native/.claude/skills/native-link");
        assert_eq!(
            fs::read_link(&restored_link).unwrap(),
            fixture.home.join("external-skill")
        );
        assert_eq!(
            fs::canonicalize(fixture.home.join("external-skill")).unwrap(),
            outside_before
        );
    }

    #[test]
    fn empty_identity_row_does_not_open_ordinary_mcp_apply() {
        let mut fixture = Fixture::new();
        let before = br#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#;
        let project = fixture.register_project_with(|root| {
            fs::write(root.join(".mcp.json"), before).unwrap();
        });
        let identity = repository::find_project_target_identity(
            &fixture.database,
            &project.id,
            Tool::Claude,
            ArtifactKind::Mcp,
            fixture
                .home
                .join("projects/native/.mcp.json")
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap()
        .expect("登记后应有空 baseline 目标身份行");
        assert!(identity.full_hash.is_none());
        assert!(identity.managed_hash.is_none());
        let mut redactor = SecretRedactor::default();
        let preview = preview_mcp_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewMcpSyncInput {
                tool: Tool::Claude,
                project_id: Some(project.id),
                exclude_from_git: false,
            },
        )
        .unwrap();
        assert!(preview.targets.is_empty());
        assert_eq!(
            fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
            before
        );
    }

    #[test]
    fn centrally_owned_mcp_item_is_hidden_from_native_list() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::write(
                root.join(".mcp.json"),
                r#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#,
            )
            .unwrap();
        });
        let identity = repository::find_project_target_identity(
            &fixture.database,
            &project.id,
            Tool::Claude,
            ArtifactKind::Mcp,
            fixture
                .home
                .join("projects/native/.mcp.json")
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap()
        .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_items(
                    id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash
                 ) VALUES (?1, ?2, 'mcp', ?3, 'native-stdio', ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    identity.target_id,
                    Uuid::new_v4().to_string(),
                    "a".repeat(64)
                ],
            )
            .unwrap();
        let items = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn native_apply_crash_before_target_preserves_bytes_and_blocks_writer() {
        let mut fixture = Fixture::new();
        let before = br#"{"mcpServers":{"native-stdio":{"command":"npx"}}}"#;
        let project = fixture.register_project_with(|root| {
            fs::write(root.join(".mcp.json"), before).unwrap();
        });
        let item = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Mcp,
            },
        )
        .unwrap()
        .remove(0);
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id,
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let error = apply_project_native_resource_preview_with_fault(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id.clone(),
            },
            &CrashBeforeTarget,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        assert_eq!(
            fs::read(fixture.home.join("projects/native/.mcp.json")).unwrap(),
            before
        );
        let recovery = detect_interrupted_run(&fixture.database, &fixture.paths)
            .unwrap()
            .expect("崩溃后必须检测到活动 run");
        assert_eq!(recovery.run_id, preview.preview_id);
        assert!(recovery.journal_available);
    }

    #[test]
    fn native_skill_symlink_failure_rolls_back_without_central_root() {
        let mut fixture = Fixture::new();
        let home = fixture.home.clone();
        let project = fixture.register_project_with(|root| {
            write_skill(&home, "external-skill", "external");
            fs::create_dir_all(root.join(".claude/skills")).unwrap();
            symlink(
                home.join("external-skill"),
                root.join(".claude/skills/native-link"),
            )
            .unwrap();
        });
        let link = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-link")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: link.id,
                row_version: link.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let error = apply_project_native_resource_preview_with_fault(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
            &FailAfterTarget,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        let restored = fixture
            .home
            .join("projects/native/.claude/skills/native-link");
        assert_eq!(
            fs::read_link(&restored).unwrap(),
            fixture.home.join("external-skill")
        );
        assert!(fixture.home.join("external-skill/SKILL.md").exists());
    }

    #[test]
    fn native_skill_directory_failure_rolls_back_tree() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            write_skill(&root.join(".claude/skills"), "native-dir", "keep-bytes");
        });
        let directory = list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project.id,
                tool: Tool::Claude,
                artifact_kind: ArtifactKind::Skill,
            },
        )
        .unwrap()
        .into_iter()
        .find(|item| item.display_name == "native-dir")
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: directory.id,
                row_version: directory.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap();
        let error = apply_project_native_resource_preview_with_fault(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyProjectNativeResourcePreviewInput {
                preview_id: preview.preview_id,
            },
            &FailAfterTarget,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        let restored = fixture
            .home
            .join("projects/native/.claude/skills/native-dir/SKILL.md");
        assert!(fs::read_to_string(restored).unwrap().contains("keep-bytes"));
    }

    #[test]
    fn skill_disable_restore_works_for_codex_and_cursor() {
        for tool in [Tool::Codex, Tool::Cursor] {
            let mut fixture = Fixture::new();
            let relative = match tool {
                Tool::Codex => ".codex/skills",
                Tool::Cursor => ".cursor/skills",
                Tool::Claude => unreachable!(),
                Tool::Zcode => unreachable!(),
                Tool::Opencode => unreachable!(),
            };
            let project = fixture.register_project_with(|root| {
                write_skill(&root.join(relative), "native-dir", "platform-bytes");
            });
            if tool == Tool::Codex {
                let canonical = fs::canonicalize(fixture.home.join("projects/native")).unwrap();
                fs::write(
                    fixture.home.join(".codex/config.toml"),
                    format!(
                        "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                        canonical.display()
                    ),
                )
                .unwrap();
            }
            let items = list_project_native_resources(
                &mut fixture.database,
                &fixture.environment,
                &ProjectNativeResourceQueryInput {
                    project_id: project.id.clone(),
                    tool,
                    artifact_kind: ArtifactKind::Skill,
                },
            )
            .unwrap();
            let directory = items
                .into_iter()
                .find(|item| item.display_name == "native-dir")
                .unwrap_or_else(|| panic!("{tool:?} 应发现项目 Skill"));
            let mut redactor = SecretRedactor::default();
            let preview = preview_project_native_resource_action(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &PreviewProjectNativeResourceActionInput {
                    resource_id: directory.id.clone(),
                    row_version: directory.row_version,
                    action: ProjectNativeResourceAction::Disable,
                },
            )
            .unwrap();
            apply_project_native_resource_preview(
                &fixture.write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &ApplyProjectNativeResourcePreviewInput {
                    preview_id: preview.preview_id,
                },
            )
            .unwrap();
            assert!(!fixture
                .home
                .join("projects/native")
                .join(relative)
                .join("native-dir")
                .exists());
            let disabled = list_project_native_resources(
                &mut fixture.database,
                &fixture.environment,
                &ProjectNativeResourceQueryInput {
                    project_id: project.id.clone(),
                    tool,
                    artifact_kind: ArtifactKind::Skill,
                },
            )
            .unwrap()
            .into_iter()
            .find(|item| item.display_name == "native-dir")
            .unwrap();
            let restore = preview_project_native_resource_action(
                &mut fixture.database,
                &fixture.environment,
                &mut redactor,
                &PreviewProjectNativeResourceActionInput {
                    resource_id: disabled.id,
                    row_version: disabled.row_version,
                    action: ProjectNativeResourceAction::Restore,
                },
            )
            .unwrap();
            apply_project_native_resource_preview(
                &fixture.write_operations,
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &ApplyProjectNativeResourcePreviewInput {
                    preview_id: restore.preview_id,
                },
            )
            .unwrap();
            let restored = fixture
                .home
                .join("projects/native")
                .join(relative)
                .join("native-dir/SKILL.md");
            assert!(
                fs::read_to_string(restored)
                    .unwrap()
                    .contains("platform-bytes"),
                "{tool:?} Skill 恢复应还原正文"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // 项目级 Hooks 只读观测
    // ---------------------------------------------------------------------------

    const CLAUDE_PROJECT_HOOKS: &[u8] = br#"{
        "model": "keep-model",
        "hooks": {
            "SessionStart": [
                {"matcher": "startup", "hooks": [{"type": "command", "command": "echo startup-hook", "timeout": 10}]},
                {"matcher": "clear", "hooks": [{"type": "command", "command": "echo clear-hook"}]},
                {"matcher": "compact", "hooks": [{"type": "command", "command": "echo compact-hook"}]}
            ],
            "PreToolUse": [
                {"matcher": "Task", "hooks": [{"type": "command", "command": "bash .claude/hooks/task.sh", "timeout": 30}]}
            ],
            "UserPromptSubmit": [
                {"hooks": [{"type": "command", "command": "echo prompt-hook"}]}
            ]
        }
    }"#;

    fn list_native_hooks(
        fixture: &mut Fixture,
        project_id: &str,
        tool: Tool,
    ) -> Vec<ProjectNativeResourceDto> {
        list_project_native_resources(
            &mut fixture.database,
            &fixture.environment,
            &ProjectNativeResourceQueryInput {
                project_id: project_id.to_owned(),
                tool,
                artifact_kind: ArtifactKind::Hook,
            },
        )
        .unwrap()
    }

    #[test]
    fn claude_project_hooks_are_listed_read_only() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".claude")).unwrap();
            fs::write(root.join(".claude/settings.json"), CLAUDE_PROJECT_HOOKS).unwrap();
        });
        let items = list_native_hooks(&mut fixture, &project.id, Tool::Claude);
        assert_eq!(items.len(), 5, "5 个 hook 条目都应被只读识别");
        let names = items
            .iter()
            .map(|item| item.display_name.as_str())
            .collect::<Vec<_>>();
        for expected in [
            "SessionStart · startup",
            "SessionStart · clear",
            "SessionStart · compact",
            "PreToolUse · Task",
            "UserPromptSubmit",
        ] {
            assert!(names.contains(&expected), "缺少 {expected}，实际 {names:?}");
        }
        for item in &items {
            assert_eq!(item.entry_type, ProjectNativeEntryType::HookEntry);
            assert_eq!(item.state, ProjectNativeResourceState::Active);
            assert!(!item.can_disable, "Hook 条目不可禁用");
            assert!(!item.can_restore, "Hook 条目不可恢复");
            assert_eq!(item.safe_summary["kind"], "hook");
        }
        let startup = items
            .iter()
            .find(|item| item.display_name == "SessionStart · startup")
            .unwrap();
        assert_eq!(startup.safe_summary["event"], "SessionStart");
        assert_eq!(startup.safe_summary["matcher"], "startup");
        assert_eq!(startup.safe_summary["command"], "echo startup-hook");
        assert_eq!(startup.safe_summary["timeout"], 10);
        let prompt = items
            .iter()
            .find(|item| item.display_name == "UserPromptSubmit")
            .unwrap();
        assert!(prompt.safe_summary.get("matcher").is_none());
        assert_eq!(prompt.safe_summary["command"], "echo prompt-hook");
        // 只读观测：原生文件字节不变。
        assert_eq!(
            fs::read(fixture.home.join("projects/native/.claude/settings.json")).unwrap(),
            CLAUDE_PROJECT_HOOKS
        );
    }

    #[test]
    fn cursor_flat_and_zcode_nested_hooks_are_listed() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".cursor")).unwrap();
            fs::write(
                root.join(".cursor/hooks.json"),
                br#"{"version": 1, "hooks": {
                    "preToolUse": [{"command": "echo cursor-before", "timeout": 15, "matcher": "Bash"}],
                    "sessionStart": [{"command": "echo cursor-start"}]
                }}"#,
            )
            .unwrap();
            fs::create_dir_all(root.join(".zcode")).unwrap();
            fs::write(
                root.join(".zcode/config.json"),
                br#"{"mcp": {"servers": {"keep": {"command": "keep"}}}, "hooks": {"enabled": true, "events": {
                    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "echo zcode-prompt"}]}]
                }}}"#,
            )
            .unwrap();
            fs::create_dir_all(root.join(".codex")).unwrap();
            fs::write(
                root.join(".codex/hooks.json"),
                br#"{"description": "keep", "hooks": {
                    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "echo codex-prompt"}]}]
                }}"#,
            )
            .unwrap();
        });
        let cursor = list_native_hooks(&mut fixture, &project.id, Tool::Cursor);
        assert_eq!(cursor.len(), 2, "Cursor 扁平条目应逐条识别");
        assert!(cursor
            .iter()
            .any(|item| item.display_name == "preToolUse · Bash"
                && item.safe_summary["command"] == "echo cursor-before"
                && item.safe_summary["timeout"] == 15));
        assert!(cursor
            .iter()
            .any(|item| item.display_name == "sessionStart"));
        let zcode = list_native_hooks(&mut fixture, &project.id, Tool::Zcode);
        assert_eq!(zcode.len(), 1, "ZCode events 嵌套条目应被识别");
        assert_eq!(zcode[0].display_name, "UserPromptSubmit");
        assert_eq!(zcode[0].safe_summary["command"], "echo zcode-prompt");
        // Codex 与 Claude 同为 matcher 组格式，但项目 hooks 需要项目信任证据。
        let canonical_root = fs::canonicalize(fixture.home.join("projects/native")).unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                canonical_root.display()
            ),
        )
        .unwrap();
        let codex = list_native_hooks(&mut fixture, &project.id, Tool::Codex);
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].display_name, "UserPromptSubmit");
        assert_eq!(codex[0].safe_summary["command"], "echo codex-prompt");
    }

    #[test]
    fn removed_hook_entry_becomes_missing() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".claude")).unwrap();
            fs::write(root.join(".claude/settings.json"), CLAUDE_PROJECT_HOOKS).unwrap();
        });
        assert!(list_native_hooks(&mut fixture, &project.id, Tool::Claude)
            .iter()
            .all(|item| item.state == ProjectNativeResourceState::Active));
        // 外部删除 PreToolUse 事件后重新列出：该条目 missing，其余仍 active。
        fs::write(
            fixture.home.join("projects/native/.claude/settings.json"),
            br#"{"model": "keep-model", "hooks": {
                "SessionStart": [
                    {"matcher": "startup", "hooks": [{"type": "command", "command": "echo startup-hook", "timeout": 10}]}
                ],
                "UserPromptSubmit": [
                    {"hooks": [{"type": "command", "command": "echo prompt-hook"}]}
                ]
            }}"#,
        )
        .unwrap();
        let items = list_native_hooks(&mut fixture, &project.id, Tool::Claude);
        assert_eq!(items.len(), 5, "missing 行保留在列表中（2 active + 3 missing）");
        for gone in ["PreToolUse · Task", "SessionStart · clear", "SessionStart · compact"] {
            let missing = items
                .iter()
                .find(|item| item.display_name == gone)
                .unwrap_or_else(|| panic!("{gone} 应显示为 missing"));
            assert_eq!(missing.state, ProjectNativeResourceState::Missing);
            assert!(!missing.can_disable && !missing.can_restore);
            assert!(missing
                .diagnostic_codes
                .contains(&"PROJECT_NATIVE_RESOURCE_MISSING".to_owned()));
            assert!(missing.safe_summary.get("command").is_none());
        }
        assert!(items
            .iter()
            .filter(|item| {
                matches!(
                    item.display_name.as_str(),
                    "SessionStart · startup" | "UserPromptSubmit"
                )
            })
            .all(|item| item.state == ProjectNativeResourceState::Active));
    }

    #[test]
    fn centrally_synced_hook_entry_is_hidden_from_native_list() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".claude")).unwrap();
            fs::write(root.join(".claude/settings.json"), br#"{"model": "keep"}"#).unwrap();
        });
        // 登记（对账）先建空 baseline 的 Hook 目标身份行。
        let identity = repository::find_project_target_identity(
            &fixture.database,
            &project.id,
            Tool::Claude,
            ArtifactKind::Hook,
            fixture
                .home
                .join("projects/native/.claude/settings.json")
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap()
        .expect("登记后应有 Hook 目标身份行");
        // 中央 Hook 经项目分配同步写入同一文件。
        let hook = crate::hooks::create_hook(
            &mut fixture.database,
            &fixture.paths,
            &crate::hooks::CreateHookInput {
                name: "central-hook".to_owned(),
                event: crate::domain::HookEvent::PreToolUse,
                matcher: Some("Work".to_owned()),
                command: "echo central-hook".to_owned(),
                timeout_seconds: Some(20),
                enabled: true,
                script_source_path: None,
            },
        )
        .unwrap();
        crate::hooks::set_project_hook_assignment(
            &mut fixture.database,
            &crate::hooks::SetProjectHookAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                hook_id: hook.id.clone(),
                event: crate::domain::HookEvent::PreToolUse,
                assigned: true,
                hook_row_version: hook.row_version,
                project_row_version: project.row_version,
            },
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let plan = crate::hooks::preview_hook_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &crate::hooks::PreviewHookSyncInput {
                tool: Tool::Claude,
                project_id: Some(project.id.clone()),
                exclude_from_git: false,
            },
        )
        .unwrap();
        assert_eq!(plan.targets.len(), 1, "项目 Hook 同步应有恰好一个目标");
        crate::hooks::apply_hook_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &crate::hooks::ApplyHookPreviewInput {
                preview_id: plan.preview_id,
                tool: Tool::Claude,
                project_id: Some(project.id.clone()),
            },
        )
        .unwrap();
        // 身份行共用：中央同步的 managed_items 必须挂在登记时建的同一 target 上。
        let managed = hooks_repository::list_managed_hook_items(&fixture.database, &identity.target_id)
            .unwrap();
        assert_eq!(managed.len(), 1, "中央条目应复用登记时建立的目标身份行");

        // 中央条目不展示为项目原生资源。
        assert!(list_native_hooks(&mut fixture, &project.id, Tool::Claude).is_empty());
        // 同一文件里外部新增的条目仍列出。
        fs::write(
            fixture.home.join("projects/native/.claude/settings.json"),
            br#"{"model": "keep", "hooks": {"PreToolUse": [
                {"matcher": "Work", "hooks": [{"type": "command", "command": "echo central-hook", "timeout": 20}]},
                {"matcher": "Bash", "hooks": [{"type": "command", "command": "echo external-keep"}]}
            ]}}"#,
        )
        .unwrap();
        let items = list_native_hooks(&mut fixture, &project.id, Tool::Claude);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].display_name, "PreToolUse · Bash");
        assert_eq!(items[0].state, ProjectNativeResourceState::Active);
    }

    #[test]
    fn hook_entry_action_preview_is_rejected() {
        let mut fixture = Fixture::new();
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".claude")).unwrap();
            fs::write(root.join(".claude/settings.json"), CLAUDE_PROJECT_HOOKS).unwrap();
        });
        let item = list_native_hooks(&mut fixture, &project.id, Tool::Claude)
            .into_iter()
            .next()
            .expect("应列出 Hook 条目");
        let mut redactor = SecretRedactor::default();
        let error = preview_project_native_resource_action(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            &PreviewProjectNativeResourceActionInput {
                resource_id: item.id.clone(),
                row_version: item.row_version,
                action: ProjectNativeResourceAction::Disable,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(
            error
                .details()
                .and_then(|details| details.get("reason"))
                .and_then(Value::as_str)
                .is_some_and(|reason| reason.contains("Hooks 暂不支持临时禁用与恢复")),
            "实际错误：{error:?}"
        );
    }

    #[test]
    fn hook_command_with_secret_is_redacted_in_summary() {
        let mut fixture = Fixture::new();
        let secret = "sk-fixture-secret-000000";
        let project = fixture.register_project_with(|root| {
            fs::create_dir_all(root.join(".claude")).unwrap();
            fs::write(
                root.join(".claude/settings.json"),
                format!(
                    r#"{{"hooks": {{"UserPromptSubmit": [
                        {{"hooks": [{{"type": "command", "command": "curl -H 'Authorization: Bearer {secret}' https://example.test"}}]}}
                    ]}}}}"#
                ),
            )
            .unwrap();
        });
        let items = list_native_hooks(&mut fixture, &project.id, Tool::Claude);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].safe_summary["commandRedacted"], true);
        assert!(items[0].safe_summary.get("command").is_none());
        let serialized = serde_json::to_string(&items[0]).unwrap();
        assert!(!serialized.contains(secret), "脱敏条目不得泄漏命令原文");
    }
}
