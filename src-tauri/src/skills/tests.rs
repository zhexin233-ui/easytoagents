#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::{symlink, MetadataExt, PermissionsExt},
        path::Path,
        sync::Mutex,
    };

    use serde_json::json;
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::{
        adopt_skill_content, apply_skill_preview_with_policy_probe, delete_skill,
        import_downloaded_github_skill, import_skill, list_skill_project_options,
        preview_skill_content, preview_skill_sync_with_policy_probe, set_global_skill_assignment,
        set_project_skill_assignment,
    };
    use crate::{
        adapters::{
            ExplicitEnvironment, ToolAvailability, VerifiedClaudeCustomizationPolicyEvidence,
        },
        app::AppPaths,
        db::Database,
        domain::{SkillStatus, SyncStatus, Tool},
        error::ErrorCode,
        security::SecretRedactor,
        skills::{
            ApplySkillPreviewInput, ImportSkillInput, PreviewSkillSyncInput,
            SetGlobalSkillAssignmentInput, SetProjectSkillAssignmentInput, SkillDto,
            SkillProjectOptionsInput, SkillProjectSelectionState, VersionedSkillInput,
        },
        sync::{hash_json, list_snapshots, preview_restore, restore_snapshot},
    };

    const CONTENT_MARKER: &str = "phase6-private-content-marker";
    const FRONTMATTER_MARKER: &str = "phase6-private-frontmatter-marker";

    struct Fixture {
        _temporary: TempDir,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
        home: std::path::PathBuf,
        project: std::path::PathBuf,
        project_id: String,
        source_index: usize,
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
            // Cursor 的写入根保持为窄边界 ~/.cursor；fixture 显式模拟已安装
            // Cursor 创建过配置根，但仍让同步管线自行创建 skills 子目录。
            fs::create_dir(home.join(".cursor")).unwrap();
            // ZCode 同理：安装后自带 ~/.zcode 配置根。
            fs::create_dir(home.join(".zcode")).unwrap();
            // OpenCode 全局配置根是 XDG_CONFIG_HOME/opencode（默认 ~/.config/opencode）。
            fs::create_dir_all(home.join(".config/opencode")).unwrap();
            let home = fs::canonicalize(home).unwrap();
            let project = fs::canonicalize(project).unwrap();
            fs::write(
                home.join(".codex/config.toml"),
                format!(
                    "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                    project.to_string_lossy()
                ),
            )
            .unwrap();
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
            let paths = AppPaths::from_data_root(root.join("private/app-data")).unwrap();
            let database = Database::open(&paths).unwrap();
            let project_id = Uuid::new_v4().to_string();
            database
                .connection()
                .execute(
                    "INSERT INTO projects(
                        id, display_name, root_path, is_git_repo, codex_trust_status
                     ) VALUES (?1, '隔离项目', ?2, 0, 'trusted')",
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
                source_index: 0,
            }
        }

        fn source(&mut self, name: &str) -> std::path::PathBuf {
            self.source_index += 1;
            let source = self
                .home
                .parent()
                .unwrap()
                .join(format!("source-{}", self.source_index));
            fs::create_dir(&source).unwrap();
            fs::write(
                source.join("SKILL.md"),
                format!(
                    "---\nname: {name}\ndescription: 隔离测试 Skill\nmetadata:\n  token: {FRONTMATTER_MARKER}\n---\n\n# Skill\n\n{CONTENT_MARKER}\n"
                ),
            )
            .unwrap();
            fs::write(source.join("asset.txt"), "fixture asset").unwrap();
            source
        }

        fn allowed_policy(&self) -> VerifiedClaudeCustomizationPolicyEvidence {
            VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting("fixture-1.0.0", None)
                .unwrap()
        }

        fn blocked_policy(&self) -> VerifiedClaudeCustomizationPolicyEvidence {
            VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                "fixture-1.0.0",
                Some(&json!(true)),
            )
            .unwrap()
        }

        fn environment_with_policy(
            &self,
            setting: Option<&serde_json::Value>,
        ) -> ExplicitEnvironment {
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

        fn import(&mut self, name: &str) -> crate::skills::SkillDto {
            let source = self.source(name);
            import_skill(
                &mut self.database,
                &self.paths,
                &ImportSkillInput {
                    source_path: source.to_string_lossy().into_owned(),
                },
            )
            .unwrap()
        }
    }

    thread_local! {
        static SQL_STATEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn count_sql(_statement: &str) {
        SQL_STATEMENTS.with(|count| count.set(count.get() + 1));
    }

    /// 列表接口的 SQL 语句数不随记录数增长（记录 + 一次聚合的全局分配）。
    #[test]
    fn list_skills_issues_a_constant_number_of_sql_statements() {
        let mut fixture = Fixture::new();
        let first = fixture.import("alpha-skill");
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: first.id.clone(),
                assigned: true,
                row_version: first.row_version,
            },
        )
        .unwrap();
        fixture.database.connection_mut().trace(Some(count_sql));
        SQL_STATEMENTS.with(|count| count.set(0));
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].global_tools, vec![Tool::Claude]);
        let with_one = SQL_STATEMENTS.with(std::cell::Cell::get);
        fixture.database.connection_mut().trace(None);

        for name in ["beta-skill", "gamma-skill", "delta-skill"] {
            fixture.import(name);
        }
        fixture.database.connection_mut().trace(Some(count_sql));
        SQL_STATEMENTS.with(|count| count.set(0));
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(listed.len(), 4);
        let with_four = SQL_STATEMENTS.with(std::cell::Cell::get);
        fixture.database.connection_mut().trace(None);
        assert_eq!(with_four, with_one, "列表 SQL 数量不应随记录数增长");
        assert!(with_one <= 3, "列表 SQL 数量：{with_one}");
    }

    /// 列表场景复用 stat 指纹缓存：目录没变时不再全树读哈希；文件变化后重新摘要并识别漂移。
    #[test]
    fn list_skills_reuses_tree_digest_until_the_central_tree_changes() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("cached-skill");
        // 导入过程已把摘要放进缓存；清掉它，让首次列表必须完整读一次树。
        crate::skills::library::clear_tree_digest_cache();
        crate::skills::library::FULL_DIGEST_CALLS.with(|count| count.set(0));
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(listed[0].status, SkillStatus::Ready);
        let after_first = crate::skills::library::FULL_DIGEST_CALLS.with(std::cell::Cell::get);
        assert!(after_first >= 1);
        super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(
            crate::skills::library::FULL_DIGEST_CALLS.with(std::cell::Cell::get),
            after_first,
            "目录未变化时第二次列表不应重新读全树"
        );

        // 改动中央文件：mtime/size 变化 → 缓存失效 → 重新摘要并报告漂移。
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            std::path::Path::new(&skill.central_path).join("SKILL.md"),
            "---\nname: cached-skill\ndescription: 已改动\n---\n\n# 改动后的正文\n",
        )
        .unwrap();
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert!(crate::skills::library::FULL_DIGEST_CALLS.with(std::cell::Cell::get) > after_first);
        assert_eq!(
            listed[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
    }

    #[test]
    fn github_download_import_persists_url_and_only_creates_central_copy() {
        let mut fixture = Fixture::new();
        let source = fixture.source("github-demo");
        let source_url = "https://github.com/acme/repo/tree/main/skills/github-demo";
        let imported = import_downloaded_github_skill(
            &mut fixture.database,
            &fixture.paths,
            &source,
            source_url,
        )
        .unwrap();
        assert_eq!(imported.source_path, source_url);
        assert_eq!(imported.name, "github-demo");
        assert_eq!(
            fs::read_to_string(Path::new(&imported.central_path).join("asset.txt")).unwrap(),
            "fixture asset"
        );
        for table in [
            "skill_global_assignments",
            "skill_project_assignments",
            "managed_targets",
            "managed_items",
            "sync_runs",
        ] {
            let count: i64 = fixture
                .database
                .connection()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "GitHub 导入不应写入 {table}");
        }
    }

    #[test]
    fn public_preview_status_and_apply_reuse_environment_policy_evidence() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("release-evidence-skill");
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let statuses = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
        )
        .unwrap();
        assert_ne!(statuses[0].status, SyncStatus::PolicyBlocked);
        let preview = super::preview_skill_sync(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &PreviewSkillSyncInput {
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
        super::apply_skill_preview(
            &std::sync::Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
        )
        .unwrap();
        assert!(fixture
            .home
            .join(".claude/skills/release-evidence-skill")
            .is_symlink());
    }

    #[test]
    fn initial_skill_status_requires_empty_baseline_and_no_managed_items() {
        let mut fixture = Fixture::new();
        let target = fixture.home.join(".claude/skills");
        fs::create_dir_all(&target).unwrap();
        let status = |fixture: &Fixture| {
            super::list_global_skill_target_statuses(
                &fixture.database,
                &fixture.paths,
                &fixture.environment,
            )
            .unwrap()
            .remove(0)
        };
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_EMPTY")
        );
        fs::write(target.join(".DS_Store"), "metadata").unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_UNMANAGED")
        );
        let target_id = Uuid::new_v4().to_string();
        fixture.database.connection().execute(
            "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'claude', 'skill', 'global', ?2)",
            rusqlite::params![target_id, target.to_string_lossy()],
        ).unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_UNMANAGED")
        );
        let skill = fixture.import("unassigned-copy");
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_UNMANAGED")
        );
        fixture.database.connection().execute("UPDATE managed_targets SET baseline_full_hash = ?1, baseline_managed_hash = ?2 WHERE id = ?3", rusqlite::params!["a".repeat(64), crate::sync::hash_json(&json!({})), target_id]).unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("EXTERNAL_NON_OWNED_CHANGE")
        );
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = ON")
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_managed_hash = NULL",
                [],
            )
            .unwrap();
        assert_ne!(status(&fixture).status, SyncStatus::ExternalNonOwnedChange);
        fixture
            .database
            .connection()
            .execute("UPDATE managed_targets SET baseline_full_hash = NULL", [])
            .unwrap();
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = OFF")
            .unwrap();
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = ON")
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_full_hash = ?1 WHERE id = ?2",
                rusqlite::params!["c".repeat(64), target_id],
            )
            .unwrap();
        let incomplete_baseline = status(&fixture);
        assert_eq!(incomplete_baseline.status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            incomplete_baseline.diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_INCOMPLETE_BASELINE)
        );
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_full_hash = NULL WHERE id = ?1",
                [&target_id],
            )
            .unwrap();
        fixture
            .database
            .connection()
            .execute_batch("PRAGMA ignore_check_constraints = OFF")
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_items(
                id, target_id, resource_kind, resource_id, external_key,
                last_applied_item_hash
             ) VALUES (?1, ?2, 'skill', ?3, ?4, ?5)",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    target_id,
                    assigned.id,
                    assigned.name,
                    "b".repeat(64),
                ],
            )
            .unwrap();
        let managed_item_drift = status(&fixture);
        assert_eq!(managed_item_drift.status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            managed_item_drift.diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_MANAGED_ITEM_BASELINE_MISMATCH)
        );
        fixture
            .database
            .connection()
            .execute(
                "DELETE FROM managed_items WHERE target_id = ?1",
                [&target_id],
            )
            .unwrap();
        fs::write(Path::new(&skill.central_path).join("asset.txt"), "changed").unwrap();
        assert_eq!(status(&fixture).status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            status(&fixture).diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
    }

    #[test]
    fn first_global_sync_is_pending_then_preserves_existing_entries_for_all_tools() {
        let mut fixture = Fixture::new();
        let claude_target = fixture.home.join(".claude/skills");
        fs::create_dir_all(claude_target.join("external-untouched")).unwrap();
        fs::write(claude_target.join(".DS_Store"), "metadata").unwrap();
        let skill = fixture.import("first-sync-skill");
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Codex,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: assigned.row_version,
            },
        )
        .unwrap();
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Cursor,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: assigned.row_version,
            },
        )
        .unwrap();
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Zcode,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: assigned.row_version,
            },
        )
        .unwrap();
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Opencode,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: assigned.row_version,
            },
        )
        .unwrap();

        let policy = fixture.allowed_policy();
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        let claude = statuses
            .iter()
            .find(|status| status.tool == Tool::Claude)
            .unwrap();
        let codex = statuses
            .iter()
            .find(|status| status.tool == Tool::Codex)
            .unwrap();
        let cursor = statuses
            .iter()
            .find(|status| status.tool == Tool::Cursor)
            .unwrap();
        assert_eq!(claude.status, SyncStatus::ExternalNonOwnedChange);
        assert_eq!(codex.status, SyncStatus::Missing);
        assert_eq!(cursor.status, SyncStatus::Missing);
        assert_eq!(
            claude.diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );
        assert_eq!(
            codex.diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );
        assert_eq!(
            cursor.diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        let redactor = SecretRedactor::default();
        let claude_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let codex_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Codex,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let cursor_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Cursor,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let zcode_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Zcode,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let opencode_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Opencode,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let previewed_statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert!(previewed_statuses.iter().all(|status| {
            status.diagnostic_code.as_deref() == Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        }));

        for (tool, preview) in [
            (Tool::Claude, claude_preview),
            (Tool::Codex, codex_preview),
            (Tool::Cursor, cursor_preview),
            (Tool::Zcode, zcode_preview),
            (Tool::Opencode, opencode_preview),
        ] {
            apply_skill_preview_with_policy_probe(
                &Mutex::new(()),
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &redactor,
                &ApplySkillPreviewInput {
                    preview_id: preview.preview_id,
                    tool,
                    project_id: None,
                },
                &policy,
            )
            .unwrap();
        }

        assert_eq!(
            fs::read(claude_target.join(".DS_Store")).unwrap(),
            b"metadata"
        );
        assert!(claude_target.join("external-untouched").is_dir());
        for link in [
            claude_target.join("first-sync-skill"),
            fixture
                .environment
                .codex_home()
                .join("skills/first-sync-skill"),
            fixture.home.join(".cursor/skills/first-sync-skill"),
            fixture.home.join(".zcode/skills/first-sync-skill"),
            fixture
                .environment
                .opencode_config_dir()
                .join("skills/first-sync-skill"),
        ] {
            assert!(link.is_symlink());
            assert_eq!(
                fs::canonicalize(link).unwrap(),
                Path::new(&skill.central_path)
            );
        }
        let applied_statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert!(applied_statuses
            .iter()
            .all(|status| status.status == SyncStatus::InSync));

        fs::create_dir(claude_target.join("post-apply-external")).unwrap();
        let drifted_statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        let drifted_claude = drifted_statuses
            .iter()
            .find(|status| status.tool == Tool::Claude)
            .unwrap();
        assert_eq!(drifted_claude.status, SyncStatus::ExternalNonOwnedChange);
        assert_eq!(
            drifted_claude.diagnostic_code.as_deref(),
            Some(crate::sync::WARNING_EXTERNAL_NON_OWNED_CHANGE)
        );
    }

    #[test]
    fn global_status_distinguishes_initial_missing_unknown_and_blocked_policy() {
        let fixture = Fixture::new();
        let missing = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
        )
        .unwrap();
        assert_eq!(missing[0].tool, Tool::Claude);
        assert_eq!(missing[0].status, SyncStatus::Missing);
        assert_eq!(missing[0].diagnostic_code, None);

        let unknown_environment = fixture.environment_without_policy_evidence();
        let unknown = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &unknown_environment,
        )
        .unwrap();
        assert_eq!(unknown[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            unknown[0].diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_CLAUDE_POLICY_UNKNOWN)
        );

        let blocked_environment = fixture.environment_with_policy(Some(&json!(true)));
        let blocked = super::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &blocked_environment,
        )
        .unwrap();
        assert_eq!(blocked[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            blocked[0].diagnostic_code.as_deref(),
            Some("CLAUDE_POLICY_BLOCKED")
        );
    }

    #[test]
    fn import_preview_and_assignment_crud_never_write_native_targets() {
        let mut fixture = Fixture::new();
        let source = fixture.source("fixture-skill");
        let before = fs::read(source.join("SKILL.md")).unwrap();
        let skill = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), before);
        assert!(Path::new(&skill.central_path).join("SKILL.md").is_file());
        let ordinary_rpc =
            serde_json::to_string(&super::list_skills(&fixture.database, &fixture.paths).unwrap())
                .unwrap();
        assert!(!ordinary_rpc.contains(CONTENT_MARKER));
        assert!(!ordinary_rpc.contains(FRONTMATTER_MARKER));
        assert!(!fixture.home.join(".claude/skills").exists());
        assert!(!fixture.environment.codex_home().join("skills").exists());

        let preview = preview_skill_content(&fixture.database, &fixture.paths, &skill.id).unwrap();
        assert!(preview.skill_md.contains(CONTENT_MARKER));
        assert_eq!(preview.files, vec!["SKILL.md", "asset.txt"]);

        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        assert!(!fixture.home.join(".claude/skills").exists());
        let delete_error = delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: assigned.id,
                row_version: assigned.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(delete_error.code(), ErrorCode::Conflict);
        assert!(!serde_json::to_string(&delete_error)
            .unwrap()
            .contains(CONTENT_MARKER));
    }

    #[test]
    fn database_failures_clean_staging_and_restore_quarantined_central_directory() {
        let mut fixture = Fixture::new();
        fixture
            .database
            .connection()
            .execute_batch(
                "CREATE TRIGGER fixture_fail_skill_insert
                 BEFORE INSERT ON skills BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
            )
            .unwrap();
        let source = fixture.source("failed-skill");
        assert!(import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .is_err());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
        assert!(fs::read_dir(fixture.paths.central_skills())
            .unwrap()
            .next()
            .is_none());
        fixture
            .database
            .connection()
            .execute_batch("DROP TRIGGER fixture_fail_skill_insert;")
            .unwrap();

        let skill = fixture.import("deletable-skill");
        fixture
            .database
            .connection()
            .execute_batch(
                "CREATE TRIGGER fixture_fail_skill_delete
                 BEFORE DELETE ON skills BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
            )
            .unwrap();
        assert!(delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .is_err());
        assert!(Path::new(&skill.central_path).join("SKILL.md").is_file());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn names_are_nocase_unique_and_central_content_drift_is_reported() {
        let mut fixture = Fixture::new();
        let first = fixture.import("fixture-skill");
        let second_source = fixture.source("fixture-skill");
        let error = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: second_source.to_string_lossy().into_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(
            fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .count(),
            1
        );
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());

        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: first.id.clone(),
                assigned: true,
                row_version: first.row_version,
            },
        )
        .unwrap();

        fs::write(Path::new(&first.central_path).join("asset.txt"), "tampered").unwrap();
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(listed[0].status, crate::domain::SkillStatus::Invalid);
        assert_eq!(
            listed[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &fixture.allowed_policy(),
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            statuses[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
    }

    fn stored_skill_fingerprint(fixture: &Fixture, id: &str) -> (String, String, i64, String) {
        fixture
            .database
            .connection()
            .query_row(
                "SELECT content_hash, frontmatter_json, row_version, status
                 FROM skills WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
    }

    #[test]
    fn adopting_drifted_central_files_restores_ready_without_rewriting_disk_or_symlinks() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        let skill = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        let link = fixture.home.join(".claude/skills/fixture-skill");
        let link_target = fs::read_link(&link).unwrap();
        let link_inode = fs::symlink_metadata(&link).unwrap().ino();
        let skill_md = Path::new(&skill.central_path).join("SKILL.md");
        let asset = Path::new(&skill.central_path).join("asset.txt");
        let edited = format!(
            "---\nname: fixture-skill\ndescription: 采纳后的描述\nmetadata:\n  token: {FRONTMATTER_MARKER}\n---\n\n# Skill\n\nupdated-body\n"
        );
        fs::write(&skill_md, &edited).unwrap();
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(listed[0].status, SkillStatus::Invalid);
        assert_eq!(
            listed[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
        assert_eq!(
            preview_skill_content(&fixture.database, &fixture.paths, &skill.id)
                .unwrap_err()
                .code(),
            ErrorCode::Conflict
        );
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            statuses[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
        let edited_bytes = fs::read(&skill_md).unwrap();
        let asset_bytes = fs::read(&asset).unwrap();

        let adopted = adopt_skill_content(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .unwrap();
        assert_eq!(adopted.status, SkillStatus::Ready);
        assert_eq!(adopted.diagnostic_code, None);
        assert_eq!(adopted.description, "采纳后的描述");
        assert_eq!(adopted.name, skill.name);
        assert_eq!(adopted.central_path, skill.central_path);
        assert_ne!(adopted.content_hash, skill.content_hash);
        assert_eq!(fs::read(&skill_md).unwrap(), edited_bytes);
        assert_eq!(fs::read(&asset).unwrap(), asset_bytes);
        assert_eq!(fs::read_link(&link).unwrap(), link_target);
        assert_eq!(fs::symlink_metadata(&link).unwrap().ino(), link_inode);

        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        assert_eq!(listed[0].status, SkillStatus::Ready);
        assert_eq!(listed[0].diagnostic_code, None);
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::InSync);
        assert_ne!(
            statuses[0].diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
        let content = preview_skill_content(&fixture.database, &fixture.paths, &skill.id).unwrap();
        assert!(content.skill_md.contains("updated-body"));
        preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
    }

    #[test]
    fn adopting_unchanged_ready_content_is_idempotent() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        let before = stored_skill_fingerprint(&fixture, &skill.id);
        let adopted = adopt_skill_content(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .unwrap();
        assert_eq!(adopted.status, SkillStatus::Ready);
        assert_eq!(adopted.content_hash, skill.content_hash);
        assert_eq!(adopted.row_version, skill.row_version);
        assert_eq!(stored_skill_fingerprint(&fixture, &skill.id), before);
    }

    #[test]
    fn adopt_skill_content_rejects_rename_broken_markdown_type_change_stale_version_and_writers() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        let skill_md = Path::new(&skill.central_path).join("SKILL.md");
        let original = fs::read(&skill_md).unwrap();
        let before = stored_skill_fingerprint(&fixture, &skill.id);

        fs::write(
            &skill_md,
            "---\nname: renamed-skill\ndescription: 改名不能采纳\n---\n\n# renamed\n",
        )
        .unwrap();
        let rename_error = adopt_skill_content(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(rename_error.code(), ErrorCode::Conflict);
        assert_eq!(stored_skill_fingerprint(&fixture, &skill.id), before);
        assert_ne!(fs::read(&skill_md).unwrap(), original);

        fs::write(&skill_md, "not-valid-frontmatter\n").unwrap();
        let parse_error = adopt_skill_content(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(parse_error.code(), ErrorCode::InvalidInput);
        assert_eq!(stored_skill_fingerprint(&fixture, &skill.id), before);

        fs::write(&skill_md, &original).unwrap();
        fs::remove_dir_all(&skill.central_path).unwrap();
        fs::write(&skill.central_path, "not-a-directory").unwrap();
        let type_error = adopt_skill_content(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(type_error.code(), ErrorCode::Conflict);
        assert_eq!(stored_skill_fingerprint(&fixture, &skill.id), before);
        assert!(Path::new(&skill.central_path).is_file());

        fs::remove_file(&skill.central_path).unwrap();
        let missing_error = adopt_skill_content(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: skill.id.clone(),
                row_version: skill.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(missing_error.code(), ErrorCode::Conflict);
        assert_eq!(stored_skill_fingerprint(&fixture, &skill.id), before);

        let second = fixture.import("second-skill");
        fs::write(
            Path::new(&second.central_path).join("SKILL.md"),
            "---\nname: second-skill\ndescription: 漂移内容\n---\n\n# second\n",
        )
        .unwrap();
        let stale = adopt_skill_content(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: second.id.clone(),
                row_version: second.row_version + 1,
            },
        )
        .unwrap_err();
        assert_eq!(stale.code(), ErrorCode::Conflict);
        let second_before = stored_skill_fingerprint(&fixture, &second.id);
        assert_eq!(second_before.2, i64::from(second.row_version));
        let listed = super::list_skills(&fixture.database, &fixture.paths)
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == second.id)
            .unwrap();
        assert_eq!(
            listed.diagnostic_code.as_deref(),
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );

        for status in ["applying", "restoring", "rollback_failed"] {
            let run_id = Uuid::new_v4().to_string();
            fixture
                .database
                .connection()
                .execute(
                    "INSERT INTO sync_runs(id, kind, status, scope, db_version)
                     VALUES (?1, 'apply', ?2, 'global', 1)",
                    rusqlite::params![run_id, status],
                )
                .unwrap();
            let writer_error = adopt_skill_content(
                &mut fixture.database,
                &fixture.paths,
                &VersionedSkillInput {
                    id: second.id.clone(),
                    row_version: second.row_version,
                },
            )
            .unwrap_err();
            assert_eq!(writer_error.code(), ErrorCode::WriteInProgress);
            assert_eq!(
                stored_skill_fingerprint(&fixture, &second.id).0,
                second_before.0
            );
            fixture
                .database
                .connection()
                .execute("DELETE FROM sync_runs WHERE id = ?1", [&run_id])
                .unwrap();
        }
    }

    #[test]
    fn global_inheritance_is_read_only_and_project_assignment_cannot_duplicate_it() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        let global = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let options = list_skill_project_options(
            &fixture.database,
            &fixture.paths,
            &SkillProjectOptionsInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
            },
        )
        .unwrap();
        assert_eq!(options[0].state, SkillProjectSelectionState::Inherited);
        assert!(!options[0].selectable);
        let policy = fixture.allowed_policy();
        let inherited_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert!(inherited_preview.targets.is_empty());
        let project_targets: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_targets
                 WHERE artifact_kind = 'skill' AND scope = 'project' AND project_id = ?1",
                [&fixture.project_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(project_targets, 0, "纯继承项目不应产生无意义 target");
        let error = set_project_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetProjectSkillAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: false,
                skill_row_version: global.row_version,
                project_row_version: 1,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
    }

    #[test]
    fn preview_apply_creates_missing_directories_atomically_and_never_leaks_content() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        let skill = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(CONTENT_MARKER));
        assert_eq!(
            preview.targets[0].change_kind,
            crate::domain::ChangeKind::Add
        );
        let result = apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id.clone(),
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        assert_eq!(result.applied_targets, 1);
        assert!(result.snapshot_count >= 2);
        let link = fixture.home.join(".claude/skills/fixture-skill");
        assert!(link.is_symlink());
        assert_eq!(
            fs::metadata(fixture.home.join(".claude/skills"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "Apply 创建的 Skills 目录必须保持私有权限"
        );
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            Path::new(&skill.central_path)
        );
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &policy,
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::InSync);
        let journal = fs::read_to_string(
            fixture
                .paths
                .journals()
                .join(format!("{}.json", preview.preview_id)),
        )
        .unwrap();
        assert!(!journal.contains(CONTENT_MARKER));
        let persisted_preview: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&preview.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!persisted_preview.contains(CONTENT_MARKER));
    }

    #[test]
    fn ordinary_directory_unknown_links_stale_preview_and_policy_block_never_overwrite() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("fixture-skill");
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        fs::create_dir(fixture.home.join(".claude/skills")).unwrap();
        fs::create_dir(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let redactor = SecretRedactor::default();
        let allowed = fixture.allowed_policy();
        let conflict = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &allowed,
        )
        .unwrap();
        assert_eq!(
            conflict.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert!(fixture.home.join(".claude/skills/fixture-skill").is_dir());
        let ordinary_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(ordinary_status[0].status, SyncStatus::ExternalOwnedChange);
        assert_ne!(
            ordinary_status[0].diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        fs::remove_dir(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let outside = fixture.home.parent().unwrap().join("outside-skill");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let unknown = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &allowed,
        )
        .unwrap();
        assert_eq!(
            unknown.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert_eq!(
            fs::canonicalize(fixture.home.join(".claude/skills/fixture-skill")).unwrap(),
            outside
        );
        let unknown_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(unknown_status[0].status, SyncStatus::ExternalOwnedChange);
        assert_ne!(
            unknown_status[0].diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        fs::remove_file(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let missing = fixture.home.join("missing-skill-target");
        symlink(&missing, fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        let broken = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &allowed,
        )
        .unwrap();
        assert_eq!(
            broken.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert!(
            fs::symlink_metadata(fixture.home.join(".claude/skills/fixture-skill"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let broken_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(broken_status[0].status, SyncStatus::ExternalOwnedChange);
        assert_ne!(
            broken_status[0].diagnostic_code.as_deref(),
            Some("SKILL_TARGET_INITIAL_SYNC_PENDING")
        );

        let blocked = fixture.blocked_policy();
        let policy_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &blocked,
        )
        .unwrap();
        assert_eq!(
            policy_preview.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            policy_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        let policy_status = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &blocked,
        )
        .unwrap();
        assert_eq!(policy_status[0].status, SyncStatus::PolicyBlocked);
        assert_eq!(
            policy_status[0].diagnostic_code.as_deref(),
            Some("CLAUDE_POLICY_BLOCKED")
        );

        fs::remove_file(fixture.home.join(".claude/skills/fixture-skill")).unwrap();
        fs::remove_dir(fixture.home.join(".claude/skills")).unwrap();
        fs::write(fixture.home.join(".claude/skills"), "wrong target type").unwrap();
        let changed_type = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        assert_eq!(changed_type[0].status, SyncStatus::TargetTypeChanged);
        assert_eq!(
            changed_type[0].diagnostic_code.as_deref(),
            Some(crate::sync::ERROR_TARGET_TYPE_CHANGED)
        );

        fs::remove_file(fixture.home.join(".claude/skills")).unwrap();
        fs::create_dir(fixture.home.join(".claude/skills")).unwrap();
        fs::set_permissions(
            fixture.home.join(".claude/skills"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        let permission_denied = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &allowed,
        )
        .unwrap();
        fs::set_permissions(
            fixture.home.join(".claude/skills"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert_eq!(permission_denied[0].status, SyncStatus::PermissionDenied);
        assert_eq!(
            permission_denied[0].diagnostic_code.as_deref(),
            Some("TARGET_PERMISSION_DENIED")
        );
    }

    #[test]
    fn project_links_use_official_paths_and_codex_user_skills_follow_codex_home() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("project-skill");
        let assigned = set_project_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetProjectSkillAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                skill_row_version: skill.row_version,
                project_row_version: 1,
            },
        )
        .unwrap();
        let redactor = SecretRedactor::default();
        let policy = fixture.allowed_policy();
        let project_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert_eq!(
            project_preview.targets[0].descriptor.path.as_deref(),
            Some(fixture.project.join(".claude/skills").to_str().unwrap())
        );
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: project_preview.preview_id,
                tool: Tool::Claude,
                project_id: Some(fixture.project_id.clone()),
            },
            &policy,
        )
        .unwrap();
        assert!(fixture
            .project
            .join(".claude/skills/project-skill")
            .is_symlink());

        let unassigned = set_project_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetProjectSkillAssignmentInput {
                project_id: fixture.project_id.clone(),
                tool: Tool::Claude,
                skill_id: assigned.id,
                assigned: false,
                skill_row_version: assigned.row_version,
                project_row_version: 2,
            },
        )
        .unwrap();
        assert!(unassigned.row_version > assigned.row_version);

        let codex_preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Codex,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert!(codex_preview.targets.is_empty());
        let descriptor =
            super::skill_target_descriptor(&fixture.environment, Tool::Codex, None, &policy).unwrap();
        assert_eq!(
            descriptor.path.as_deref(),
            Some(
                fixture
                    .environment
                    .codex_home()
                    .join("skills")
                    .to_str()
                    .unwrap()
            )
        );
        let cursor_global =
            super::skill_target_descriptor(&fixture.environment, Tool::Cursor, None, &policy).unwrap();
        assert_eq!(
            cursor_global.path.as_deref(),
            fixture.home.join(".cursor/skills").to_str()
        );
        let project_root = crate::domain::ProjectRoot::parse(&fixture.project).unwrap();
        let cursor_project = super::skill_target_descriptor(
            &fixture.environment,
            Tool::Cursor,
            Some(&project_root),
            &policy,
        )
        .unwrap();
        assert_eq!(
            cursor_project.path.as_deref(),
            fixture.project.join(".cursor/skills").to_str()
        );
    }

    #[test]
    fn startup_reconciliation_decodes_cursor_managed_skill_targets() {
        let fixture = Fixture::new();
        let target_id = "00000000-0000-4000-8000-000000000391";
        let target_path = fixture.home.join(".cursor/skills");
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path
                 ) VALUES (?1, 'cursor', 'skill', 'global', ?2)",
                rusqlite::params![target_id, target_path.to_string_lossy()],
            )
            .unwrap();

        let targets = super::list_skill_managed_targets(&fixture.database).unwrap();
        assert!(targets.iter().any(|(id, tool, project_id, path)| {
            id == target_id
                && *tool == Tool::Cursor
                && project_id.is_none()
                && path == target_path.to_str().unwrap()
        }));
    }

    #[test]
    fn external_change_after_preview_is_stale_and_unknown_entries_are_preserved() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("stale-skill");
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id,
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let target = fixture.home.join(".claude/skills");
        fs::create_dir(&target).unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        fs::create_dir(target.join("external-untouched")).unwrap();
        let error = apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert!(target.join("external-untouched").is_dir());
        assert!(!target.join("stale-skill").exists());
    }

    #[test]
    fn managed_link_snapshot_restore_is_safe_and_detects_drift() {
        let mut fixture = Fixture::new();
        let source = fixture.source("restorable-skill");
        let source_skill_md = fs::read(source.join("SKILL.md")).unwrap();
        let skill = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let _assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        let link = fixture.home.join(".claude/skills/restorable-skill");
        let link_snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.target_path == link.to_string_lossy())
            .unwrap();
        let allowed_root = fixture.home.join(".claude");
        let restore_preview = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &link_snapshot.snapshot_id,
            &allowed_root,
        )
        .unwrap();
        restore_snapshot(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &restore_preview.preview_id,
            &allowed_root,
            Some(fixture.paths.central_skills()),
        )
        .unwrap();
        assert!(!link.exists());

        // 快照恢复不会擅自篡改 assignment/managed baseline；缺失的受管链接必须按漂移阻断。
        let drift = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        assert_eq!(
            drift.targets[0].change_kind,
            crate::domain::ChangeKind::Conflict
        );
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), source_skill_md);
        assert!(Path::new(&skill.central_path).is_dir());
    }

    #[test]
    fn managed_link_removal_and_central_delete_are_safe() {
        let mut fixture = Fixture::new();
        let source = fixture.source("removable-skill");
        let source_skill_md = fs::read(source.join("SKILL.md")).unwrap();
        let skill = import_skill(
            &mut fixture.database,
            &fixture.paths,
            &ImportSkillInput {
                source_path: source.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let assigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let policy = fixture.allowed_policy();
        let redactor = SecretRedactor::default();
        let preview = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        let link = fixture.home.join(".claude/skills/removable-skill");
        assert!(link.is_symlink());
        let unassigned = set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: assigned.id,
                assigned: false,
                row_version: assigned.row_version,
            },
        )
        .unwrap();
        let blocked_delete = delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: unassigned.id.clone(),
                row_version: unassigned.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(blocked_delete.code(), ErrorCode::Conflict);
        assert!(
            Path::new(&skill.central_path).is_dir(),
            "已应用 managed item 未清理前不得删除中央副本"
        );
        fs::create_dir(fixture.home.join(".claude/skills/external-directory")).unwrap();
        let removal = preview_skill_sync_with_policy_probe(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &PreviewSkillSyncInput {
                tool: Tool::Claude,
                project_id: None,
                exclude_from_git: false,
            },
            &policy,
        )
        .unwrap();
        apply_skill_preview_with_policy_probe(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &redactor,
            &ApplySkillPreviewInput {
                preview_id: removal.preview_id,
                tool: Tool::Claude,
                project_id: None,
            },
            &policy,
        )
        .unwrap();
        assert!(!link.exists());
        assert!(fixture
            .home
            .join(".claude/skills/external-directory")
            .is_dir());
        delete_skill(
            &mut fixture.database,
            &fixture.paths,
            &VersionedSkillInput {
                id: unassigned.id,
                row_version: unassigned.row_version,
            },
        )
        .unwrap();
        assert!(!Path::new(&skill.central_path).exists());
        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), source_skill_md);
    }

    fn downgrade_to_legacy_layout(fixture: &Fixture, skill: &SkillDto) -> std::path::PathBuf {
        let legacy = fixture.paths.central_skills().join(&skill.id);
        fs::rename(Path::new(&skill.central_path), &legacy).unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE skills SET central_path = ?2 WHERE id = ?1",
                rusqlite::params![skill.id, legacy.to_string_lossy()],
            )
            .unwrap();
        legacy
    }

    fn legacy_managed_link(
        fixture: &Fixture,
        skill: &SkillDto,
        legacy: &Path,
    ) -> (String, std::path::PathBuf) {
        let target = fixture.home.join(".claude/skills");
        fs::create_dir_all(&target).unwrap();
        let target_id = Uuid::new_v4().to_string();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                 VALUES (?1, 'claude', 'skill', 'global', ?2)",
                rusqlite::params![target_id, target.to_string_lossy()],
            )
            .unwrap();
        let native = json!({
            "targetType": "symlink",
            "linkTarget": legacy.to_string_lossy(),
        });
        let item_id = Uuid::new_v4().to_string();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_items(
                    id, target_id, resource_kind, resource_id, external_key,
                    last_applied_item_hash
                 ) VALUES (?1, ?2, 'skill', ?3, ?4, ?5)",
                rusqlite::params![item_id, target_id, skill.id, skill.name, hash_json(&native)],
            )
            .unwrap();
        let link = target.join(&skill.name);
        symlink(legacy, &link).unwrap();
        (item_id, link)
    }

    #[test]
    fn legacy_central_directories_migrate_with_database_and_managed_links() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("migration-skill");
        // 分配先于迁移：与真实用户时序一致，迁移时目标已有期望投影可供对账。
        set_global_skill_assignment(
            &mut fixture.database,
            &fixture.paths,
            &SetGlobalSkillAssignmentInput {
                tool: Tool::Claude,
                skill_id: skill.id.clone(),
                assigned: true,
                row_version: skill.row_version,
            },
        )
        .unwrap();
        let legacy = downgrade_to_legacy_layout(&fixture, &skill);
        let (item_id, link) = legacy_managed_link(&fixture, &skill, &legacy);
        // 模拟真实状态：目标 baseline 来自上一次成功 Apply，仍记录旧布局的 hash。
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                     baseline_projection_json = ?4, last_status = 'in_sync' WHERE id = ?1",
                rusqlite::params![
                    fixture
                        .database
                        .connection()
                        .query_row::<String, _, _>(
                            "SELECT id FROM managed_targets LIMIT 1",
                            [],
                            |row| row.get(0),
                        )
                        .unwrap(),
                    "a".repeat(64),
                    "b".repeat(64),
                    r#"{"stale":"projection"}"#,
                ],
            )
            .unwrap();

        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();
        super::reconcile_skill_target_baselines(&fixture.database);

        let expected = fixture.paths.central_skills().join(&skill.name);
        assert!(!legacy.exists());
        assert!(expected.join("SKILL.md").is_file());
        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&skill.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, expected.to_string_lossy());
        assert_eq!(fs::read_link(&link).unwrap(), expected);
        assert_eq!(fs::canonicalize(&link).unwrap(), expected);
        let item_hash: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT last_applied_item_hash FROM managed_items WHERE id = ?1",
                [&item_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            item_hash,
            hash_json(&json!({
                "targetType": "symlink",
                "linkTarget": expected.to_string_lossy(),
            }))
        );
        assert!(
            super::list_skills(&fixture.database, &fixture.paths).unwrap()[0]
                .central_path
                .ends_with(&skill.name)
        );

        // 迁移刷新 item 基线后，对账回填目标 baseline：
        // 不再留下不可合并（Preview 变 Conflict）的受管内容冲突。
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &fixture.allowed_policy(),
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::InSync);
        assert_eq!(statuses[0].diagnostic_code, None);
        let projection_json: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_projection_json FROM managed_targets LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(projection_json.contains("linkTarget"));
        assert!(!projection_json.contains("stale"));

        // 二次启动幂等。
        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();
        super::reconcile_skill_target_baselines(&fixture.database);
        assert!(expected.join("SKILL.md").is_file());
        assert_eq!(fs::read_link(&link).unwrap(), expected);
        let statuses = super::list_global_skill_target_statuses_with_policy_probe(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
            &fixture.allowed_policy(),
        )
        .unwrap();
        assert_eq!(statuses[0].status, SyncStatus::InSync);
    }

    #[test]
    fn drifted_or_occupied_legacy_directories_keep_the_legacy_layout() {
        let mut fixture = Fixture::new();
        let drifted = fixture.import("drifted-skill");
        let drifted_legacy = downgrade_to_legacy_layout(&fixture, &drifted);
        fs::write(
            drifted_legacy.join("SKILL.md"),
            "---\nname: drifted-skill\ndescription: tampered\n---\n\nbody\n",
        )
        .unwrap();

        let blocked = fixture.import("blocked-skill");
        // 先降级为 legacy 布局，再让同名名称化目录被未知目录占用。
        let blocked_legacy = downgrade_to_legacy_layout(&fixture, &blocked);
        let blocked_expected = fixture.paths.central_skills().join("blocked-skill");
        fs::create_dir(&blocked_expected).unwrap();
        let sentinel = blocked_expected.join("sentinel.txt");
        fs::write(&sentinel, "occupy").unwrap();

        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();

        assert!(drifted_legacy.join("SKILL.md").is_file());
        let drifted_stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&drifted.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(drifted_stored, drifted_legacy.to_string_lossy());
        // 漂移记录仍可被中央核验识别为 Invalid，而不是报路径身份错误。
        let listed = super::list_skills(&fixture.database, &fixture.paths).unwrap();
        let drifted_dto = listed.iter().find(|entry| entry.id == drifted.id).unwrap();
        assert_eq!(drifted_dto.status, SkillStatus::Invalid);

        assert!(blocked_legacy.join("SKILL.md").is_file());
        assert!(sentinel.is_file());
        let blocked_stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&blocked.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(blocked_stored, blocked_legacy.to_string_lossy());
    }

    #[test]
    fn interrupted_migration_completes_the_pending_database_update() {
        let mut fixture = Fixture::new();
        let skill = fixture.import("recovered-skill");
        let legacy = downgrade_to_legacy_layout(&fixture, &skill);
        // 模拟迁移在 rename 之后、数据库更新之前崩溃。
        let expected = fixture.paths.central_skills().join("recovered-skill");
        fs::rename(&legacy, &expected).unwrap();

        super::migrate_legacy_central_skill_directories(&mut fixture.database, &fixture.paths)
            .unwrap();

        let stored: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT central_path FROM skills WHERE id = ?1",
                [&skill.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, expected.to_string_lossy());
        assert!(expected.join("SKILL.md").is_file());
    }
}
