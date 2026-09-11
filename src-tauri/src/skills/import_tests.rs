#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{ToolAvailability, VerifiedClaudeCustomizationPolicyEvidence};
    use std::{
        fs,
        os::unix::fs::{symlink, MetadataExt, PermissionsExt},
        sync::{Arc, Barrier, Mutex},
    };

    const BODY: &str = "PRIVATE_WORKFLOW_BODY_47281";
    const PRIVATE: &str = "PRIVATE_FRONTMATTER_49271";

    struct Fixture {
        _temporary: tempfile::TempDir,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            fs::create_dir(&home).unwrap();
            let paths = AppPaths::from_data_root(root.join("data")).unwrap();
            let database = Database::open(&paths).unwrap();
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
            Self {
                _temporary: temporary,
                paths,
                database,
                environment,
                root,
            }
        }

        fn skill(&self, path: &Path, name: &str) {
            fs::create_dir_all(path).unwrap();
            fs::write(
                path.join("SKILL.md"),
                format!(
                    "---\nname: {name}\ndescription: 测试技能\nprivate: {PRIVATE}\n---\n{BODY}\n"
                ),
            )
            .unwrap();
            fs::write(path.join("asset.sh"), "exit 0\n").unwrap();
            fs::set_permissions(path.join("asset.sh"), fs::Permissions::from_mode(0o751)).unwrap();
        }

        fn preview(&self, tool: Tool) -> SkillImportPreviewDto {
            discover_skill_import(&self.database, &self.paths, &self.environment, tool).unwrap()
        }

        fn input(preview: &SkillImportPreviewDto) -> ConfirmSkillImportInput {
            ConfirmSkillImportInput {
                preview_id: preview.preview_id.clone().unwrap(),
                candidate_ids: preview
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.status == CandidateStatus::Importable)
                    .map(|candidate| candidate.candidate_id.clone())
                    .collect(),
            }
        }

        fn confirm(
            &mut self,
            input: &ConfirmSkillImportInput,
        ) -> Result<SkillImportResultDto, AppError> {
            confirm_skill_import(&mut self.database, &self.paths, &self.environment, input)
        }

        fn assert_no_partial(&self) {
            assert!(skills::list_skills(&self.database).unwrap().is_empty());
            assert_eq!(
                fs::read_dir(self.paths.central_skills()).unwrap().count(),
                0
            );
            assert_eq!(fs::read_dir(self.paths.staging()).unwrap().count(), 0);
        }

        fn metadata_counts(&self) -> Vec<i64> {
            [
                "skill_global_assignments",
                "skill_project_assignments",
                "managed_targets",
                "managed_items",
                "sync_runs",
                "sync_items",
                "snapshots",
            ]
            .into_iter()
            .map(|table| {
                self.database
                    .connection()
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap()
            })
            .collect()
        }
    }

    #[test]
    fn compatibility_links_are_readonly_deduplicated_and_selected_without_adoption() {
        let mut fixture = Fixture::new();
        let source = fixture.root.join("manager/skill-one");
        fixture.skill(&source, "skill-one");
        let compat = fixture.environment.codex_home().join("skills");
        fs::create_dir_all(&compat).unwrap();
        symlink(&source, compat.join("first-alias")).unwrap();
        symlink("first-alias", compat.join("second-alias")).unwrap();
        fixture.skill(&compat.join("other"), "other");
        fs::write(compat.join(".DS_Store"), "metadata").unwrap();
        let before = fs::metadata(source.join("asset.sh")).unwrap();
        let metadata = fixture.metadata_counts();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.sources[0].status, SourceStatus::Ready);
        assert_eq!(preview.sources[1].status, SourceStatus::Missing);
        assert_eq!(preview.candidates.len(), 2);
        let candidate = preview
            .candidates
            .iter()
            .find(|candidate| candidate.name == "skill-one")
            .unwrap();
        assert_eq!(candidate.source_paths.len(), 2);
        fixture.assert_no_partial();
        let carriers = [
            serde_json::to_string(&preview).unwrap(),
            fixture
                .database
                .connection()
                .query_row(
                    "SELECT context_json || redacted_preview_json FROM skill_import_previews",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        ];
        for carrier in carriers {
            assert!(!carrier.is_empty());
            assert!(!carrier.contains(BODY));
            assert!(!carrier.contains(PRIVATE));
        }
        let input = ConfirmSkillImportInput {
            preview_id: preview.preview_id.unwrap(),
            candidate_ids: vec![candidate.candidate_id.clone()],
        };
        assert_eq!(fixture.confirm(&input).unwrap().created_count, 1);
        let records = skills::list_skills(&fixture.database).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_path, source.to_string_lossy());
        assert_eq!(fixture.metadata_counts(), metadata);
        assert_eq!(fs::read_link(compat.join("first-alias")).unwrap(), source);
        let after = fs::metadata(source.join("asset.sh")).unwrap();
        assert_eq!((before.ino(), before.mode()), (after.ino(), after.mode()));
        assert_eq!(
            fs::read_to_string(source.join("asset.sh")).unwrap(),
            "exit 0\n"
        );
        assert!(!fixture.environment.home().join(".agents/skills").exists());
        assert_eq!(
            fixture.confirm(&input).unwrap_err().code(),
            ErrorCode::PreviewAlreadyConsumed
        );
        let repeated = fixture.preview(Tool::Codex);
        assert_eq!(
            repeated
                .candidates
                .iter()
                .find(|candidate| candidate.name == "skill-one")
                .unwrap()
                .status,
            CandidateStatus::AlreadyImported
        );
    }

    #[test]
    fn cursor_uses_dedicated_and_agents_import_sources_without_assigning() {
        let mut fixture = Fixture::new();
        let cursor_source = fixture.environment.home().join(".cursor/skills/cursor-one");
        let agents_source = fixture.environment.home().join(".agents/skills/agents-one");
        fixture.skill(&cursor_source, "cursor-one");
        fixture.skill(&agents_source, "agents-one");
        let metadata = fixture.metadata_counts();

        let preview = fixture.preview(Tool::Cursor);
        assert_eq!(preview.sources.len(), 2);
        assert_eq!(preview.sources[0].kind, SourceKind::CursorHome);
        assert_eq!(preview.sources[1].kind, SourceKind::CursorAgents);
        assert_eq!(preview.candidates.len(), 2);
        let input = Fixture::input(&preview);
        let result = fixture.confirm(&input).unwrap();
        assert_eq!(result.tool, Tool::Cursor);
        assert_eq!(result.created_count, 2);
        assert_eq!(fixture.metadata_counts(), metadata);
        assert!(cursor_source.join("SKILL.md").is_file());
        assert!(agents_source.join("SKILL.md").is_file());
    }

    #[test]
    fn exact_external_link_requires_takeover_preview_before_apply() {
        for tool in [Tool::Cursor, Tool::Opencode] {
            verify_external_link_takeover(tool);
        }
    }

    fn verify_external_link_takeover(tool: Tool) {
        let mut fixture = Fixture::new();
        let external = fixture.root.join("external/one");
        fixture.skill(&external, "one");
        let agents_root = match tool {
            Tool::Opencode => fixture.environment.opencode_config_dir().join("skills"),
            _ => fixture.environment.home().join(".agents/skills"),
        };
        fs::create_dir_all(&agents_root).unwrap();
        symlink(&external, agents_root.join("one")).unwrap();
        let import_preview = fixture.preview(tool);
        let import_input = Fixture::input(&import_preview);
        assert_eq!(fixture.confirm(&import_input).unwrap().created_count, 1);
        fs::remove_file(agents_root.join("one")).unwrap();

        let cursor_root = match tool {
            Tool::Opencode => fixture.environment.opencode_config_dir().join("skills"),
            _ => fixture.environment.home().join(".cursor/skills"),
        };
        fs::create_dir_all(&cursor_root).unwrap();
        let cursor_entry = cursor_root.join("one");
        symlink(&external, &cursor_entry).unwrap();
        let preview = fixture.preview(tool);
        let candidate = preview
            .candidates
            .iter()
            .find(|candidate| candidate.name == "one")
            .unwrap();
        assert_eq!(candidate.status, CandidateStatus::AlreadyImported);
        assert!(candidate.takeover_eligible);
        assert_eq!(
            candidate.takeover_entry_type,
            Some(SkillTakeoverEntryType::ExternalSymlink)
        );
        let takeover_input = PrepareSkillTakeoverInput {
            preview_id: preview.preview_id.clone().unwrap(),
            candidate_ids: vec![candidate.candidate_id.clone()],
        };
        assert_eq!(
            fixture
                .confirm(&ConfirmSkillImportInput {
                    preview_id: takeover_input.preview_id.clone(),
                    candidate_ids: takeover_input.candidate_ids.clone(),
                })
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        let external_before = fs::metadata(external.join("SKILL.md")).unwrap();
        let takeover = prepare_skill_takeover(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &takeover_input,
        )
        .unwrap();
        assert!(!takeover
            .plan
            .warning_codes
            .iter()
            .any(|code| code == crate::sync::ERROR_EXTERNAL_OWNED_CHANGE));
        assert_eq!(takeover.assigned_count, 1);
        assert_eq!(takeover.reused_count, 0);
        assert!(takeover
            .plan
            .warning_codes
            .iter()
            .any(|code| code == crate::sync::WARNING_SKILL_TAKEOVER_CONFIRMATION));
        assert_eq!(fs::read_link(&cursor_entry).unwrap(), external);

        let statuses = service::list_global_skill_target_statuses(
            &fixture.database,
            &fixture.paths,
            &fixture.environment,
        )
        .unwrap();
        assert_eq!(
            statuses
                .iter()
                .find(|status| status.tool == tool)
                .unwrap()
                .diagnostic_code
                .as_deref(),
            Some("SKILL_TARGET_INITIAL_TAKEOVER_REQUIRED")
        );

        service::apply_skill_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &crate::skills::ApplySkillPreviewInput {
                preview_id: takeover.plan.preview_id,
                tool,
                project_id: None,
            },
        )
        .unwrap();
        let central = skills::list_skills(&fixture.database).unwrap()[0]
            .central_path
            .clone();
        assert_eq!(
            fs::canonicalize(&cursor_entry).unwrap(),
            PathBuf::from(central)
        );
        let external_after = fs::metadata(external.join("SKILL.md")).unwrap();
        assert_eq!(external_before.ino(), external_after.ino());
        assert_eq!(
            fs::read_to_string(external.join("asset.sh")).unwrap(),
            "exit 0\n"
        );
    }

    #[test]
    fn takeover_prepare_failure_rolls_back_assignment_target_preview_and_token() {
        let mut fixture = Fixture::new();
        let external = fixture.root.join("external/one");
        fixture.skill(&external, "one");
        let agents_root = fixture.environment.home().join(".agents/skills");
        fs::create_dir_all(&agents_root).unwrap();
        symlink(&external, agents_root.join("one")).unwrap();
        let input = Fixture::input(&fixture.preview(Tool::Cursor));
        fixture.confirm(&input).unwrap();
        fs::remove_file(agents_root.join("one")).unwrap();
        let cursor_root = fixture.environment.home().join(".cursor/skills");
        fs::create_dir_all(&cursor_root).unwrap();
        symlink(&external, cursor_root.join("one")).unwrap();
        let preview = fixture.preview(Tool::Cursor);
        let candidate = preview
            .candidates
            .iter()
            .find(|candidate| candidate.takeover_eligible)
            .unwrap();
        let skill = skills::list_skills(&fixture.database).unwrap()[0].clone();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'cursor', 'skill', 'global', ?2, ?3, ?3, '{}')",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    cursor_root.to_string_lossy(),
                    "b".repeat(64),
                ],
            )
            .unwrap();

        let error = prepare_skill_takeover(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &SecretRedactor::default(),
            &PrepareSkillTakeoverInput {
                preview_id: preview.preview_id.clone().unwrap(),
                candidate_ids: vec![candidate.candidate_id.clone()],
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert!(skills::global_tools_for_skill(&fixture.database, &skill.id)
            .unwrap()
            .is_empty());
        assert_eq!(
            repository::get_preview(
                fixture.database.connection(),
                preview.preview_id.as_deref().unwrap(),
            )
            .unwrap()
            .status,
            "previewed"
        );
        assert_eq!(
            fixture
                .database
                .connection()
                .query_row("SELECT COUNT(*) FROM sync_runs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            skills::get_skill(&fixture.database, &skill.id)
                .unwrap()
                .row_version,
            skill.row_version
        );
        assert_eq!(fs::read_link(cursor_root.join("one")).unwrap(), external);
    }

    #[test]
    fn builtin_collections_aliases_and_symlinked_collection_aliases_are_never_read() {
        for tool in [Tool::Codex, Tool::Claude] {
            let fixture = Fixture::new();
            let compat = fixture.environment.codex_home().join("skills");
            let source = match tool {
                Tool::Codex => compat.clone(),
                Tool::Claude => fixture.environment.claude_config_dir().join("skills"),
                Tool::Cursor => fixture.environment.home().join(".cursor/skills"),
                Tool::Zcode => fixture.environment.home().join(".zcode/skills"),
                Tool::Opencode => fixture.environment.opencode_config_dir().join("skills"),
            };
            fixture.skill(&compat.join(".system/builtin"), "builtin");
            fs::create_dir_all(&source).unwrap();
            symlink(compat.join(".system/builtin"), source.join("builtin-alias")).unwrap();
            let preview = fixture.preview(tool);
            let source_status = preview
                .sources
                .iter()
                .find(|entry| entry.path == source.to_string_lossy())
                .unwrap();
            assert!(preview.candidates.is_empty(), "{tool:?}");
            assert_eq!(source_status.status, SourceStatus::Empty);
            assert_eq!(
                source_status.diagnostic_code.as_deref(),
                Some("SKILL_IMPORT_BUILTIN_EXCLUDED")
            );
            assert!(preview.preview_id.is_none());
            // 集合本身没有 SKILL.md，仍须解析其真实目录并排除跨工具别名。
            let actual = fixture.root.join("bundled");
            fs::rename(compat.join(".system"), &actual).unwrap();
            symlink(&actual, compat.join(".system")).unwrap();
            symlink(actual.join("builtin"), source.join("resolved-alias")).unwrap();
            assert!(fixture.preview(tool).candidates.is_empty(), "{tool:?}");
        }
    }

    #[test]
    fn builtin_aliases_created_during_confirmation_reject_the_whole_batch() {
        for tool in [Tool::Codex, Tool::Claude] {
            for changed_at in ["before", "copy", "sql"] {
                let mut fixture = Fixture::new();
                let compat = fixture.environment.codex_home().join("skills");
                let source = match tool {
                    Tool::Codex => compat.clone(),
                    Tool::Claude => fixture.environment.claude_config_dir().join("skills"),
                    Tool::Cursor => fixture.environment.home().join(".cursor/skills"),
                    Tool::Zcode => fixture.environment.home().join(".zcode/skills"),
                    Tool::Opencode => fixture.environment.opencode_config_dir().join("skills"),
                };
                let actual = fixture.root.join("external");
                fixture.skill(&actual.join("one"), "one");
                fs::create_dir_all(&source).unwrap();
                fs::create_dir_all(&compat).unwrap();
                symlink(actual.join("one"), source.join("one")).unwrap();
                let input = Fixture::input(&fixture.preview(tool));
                if changed_at == "before" {
                    symlink(&actual, compat.join(".system")).unwrap();
                }
                let result = confirm_with_fault(
                    &mut fixture.database,
                    &fixture.paths,
                    &fixture.environment,
                    &input,
                    &|stage, _| {
                        if stage == changed_at {
                            symlink(&actual, compat.join(".system")).unwrap();
                        }
                        Ok(())
                    },
                );
                assert!(result.is_err(), "{tool:?} / {changed_at}");
                fixture.assert_no_partial();
                assert_eq!(
                    repository::get_preview(fixture.database.connection(), &input.preview_id)
                        .unwrap()
                        .status,
                    "previewed"
                );
            }
        }
    }

    #[test]
    fn custom_roots_same_content_conflicts_invalid_links_and_private_paths() {
        let mut fixture = Fixture::new();
        let custom = fixture.root.join("custom-codex");
        fixture.environment = ExplicitEnvironment::new(
            fixture.environment.home(),
            None,
            Some(custom.clone()),
            ToolAvailability::all_installed(),
        )
        .unwrap();
        let agents = fixture.environment.home().join(".agents/skills");
        fixture.skill(&agents.join("one"), "same");
        fixture.skill(&custom.join("skills/two"), "same");
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates.len(), 1);
        assert_eq!(preview.candidates[0].source_paths.len(), 2);
        fs::write(custom.join("skills/two/asset.sh"), "different\n").unwrap();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates.len(), 2);
        assert!(preview
            .candidates
            .iter()
            .all(|candidate| candidate.status == CandidateStatus::NameConflict));
        assert!(preview.preview_id.is_none());
        symlink("missing", agents.join("broken")).unwrap();
        symlink("cycle", agents.join("cycle")).unwrap();
        symlink(fixture.paths.staging(), agents.join("private")).unwrap();
        fixture.skill(&agents.join("escape"), "escape");
        symlink("../../outside", agents.join("escape/link")).unwrap();
        let preview = fixture.preview(Tool::Codex);
        let broken = preview
            .candidates
            .iter()
            .find(|candidate| candidate.name == "broken")
            .unwrap();
        assert!(broken
            .reason
            .as_deref()
            .unwrap()
            .contains("来源软链接的目标已不存在"));
        assert!(!broken.takeover_eligible);
        assert_eq!(
            preview
                .candidates
                .iter()
                .filter(|candidate| candidate.status == CandidateStatus::Invalid)
                .count(),
            4
        );
        fixture.assert_no_partial();
    }

    #[test]
    fn stale_source_content_link_identity_and_central_versions_reject_confirmation() {
        for change in ["content", "link", "central"] {
            let mut fixture = Fixture::new();
            let source = fixture.root.join("managed-external");
            fixture.skill(&source, "one");
            let root = fixture.environment.codex_home().join("skills");
            fs::create_dir_all(&root).unwrap();
            symlink(&source, root.join("alias")).unwrap();
            let input = Fixture::input(&fixture.preview(Tool::Codex));
            match change {
                "content" => fs::write(source.join("asset.sh"), "changed").unwrap(),
                "link" => {
                    let other = fixture.root.join("other");
                    fixture.skill(&other, "one");
                    fs::remove_file(root.join("alias")).unwrap();
                    symlink(&other, root.join("alias")).unwrap();
                }
                _ => {
                    fixture.database.connection().execute("UPDATE skill_import_previews SET context_json = json_set(context_json, '$.central_state', 'different')", []).unwrap();
                }
            }
            assert!(fixture.confirm(&input).is_err(), "{change}");
            fixture.assert_no_partial();
        }
    }

    #[test]
    fn invalid_selection_is_rejected_and_batch_faults_roll_back_every_item() {
        for stage in ["copy", "rename", "sql", "commit"] {
            let mut fixture = Fixture::new();
            let root = fixture.environment.codex_home().join("skills");
            fixture.skill(&root.join("one"), "one");
            fixture.skill(&root.join("two"), "two");
            let input = Fixture::input(&fixture.preview(Tool::Codex));
            for ids in [
                vec![],
                vec![input.candidate_ids[0].clone(); 2],
                vec![Uuid::new_v4().to_string()],
            ] {
                assert!(fixture
                    .confirm(&ConfirmSkillImportInput {
                        preview_id: input.preview_id.clone(),
                        candidate_ids: ids
                    })
                    .is_err());
            }
            let result = confirm_with_fault(
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &input,
                &|boundary, index| {
                    if boundary == stage && (index == 1 || boundary == "commit") {
                        Err(AppError::atomic_write("fixture", "injected_batch_failure"))
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.is_err(), "{stage}");
            fixture.assert_no_partial();
            assert_eq!(
                repository::get_preview(fixture.database.connection(), &input.preview_id)
                    .unwrap()
                    .status,
                "previewed"
            );
            assert_eq!(fixture.metadata_counts(), vec![0; 7]);
            assert_eq!(fixture.confirm(&input).unwrap().created_count, 2);
        }
    }

    #[test]
    fn uncertain_commit_is_verified_without_deleting_committed_copies() {
        let mut fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        let result = confirm_with_fault(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &input,
            &|stage, _| {
                if stage == "after_commit" {
                    Err(AppError::database("fixture", "uncertain_commit"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert_eq!(result.created_count, 1);
        assert_eq!(
            crate::skills::list_skills(&fixture.database, &fixture.paths).unwrap()[0].status,
            SkillStatus::Ready
        );
    }

    #[test]
    fn independent_connections_only_consume_the_token_once() {
        let fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        let barrier = Arc::new(Barrier::new(2));
        let connections = [
            Database::open(&fixture.paths).unwrap(),
            Database::open(&fixture.paths).unwrap(),
        ];
        let handles = connections
            .into_iter()
            .map(|mut database| {
                let paths = fixture.paths.clone();
                let environment = fixture.environment.clone();
                let input = input.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    confirm_skill_import(&mut database, &paths, &environment, &input)
                })
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(skills::list_skills(&fixture.database).unwrap().len(), 1);
        assert_eq!(
            fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .count(),
            1
        );
        assert_eq!(fs::read_dir(fixture.paths.staging()).unwrap().count(), 0);
    }

    #[test]
    fn policy_blocked_and_unavailable_sources_do_not_hide_other_valid_sources() {
        let mut fixture = Fixture::new();
        let claude = fixture.environment.claude_config_dir().join("skills");
        fixture.skill(&claude.join("one"), "one");
        fixture.environment = ExplicitEnvironment::new(
            fixture.environment.home(),
            None,
            None,
            ToolAvailability::all_installed(),
        )
        .unwrap();
        let blocked = fixture.preview(Tool::Claude);
        assert!(blocked.candidates.is_empty());
        assert_eq!(blocked.sources[0].status, SourceStatus::Unavailable);
        let agents_parent = fixture.environment.home().join(".agents");
        fs::create_dir(&agents_parent).unwrap();
        symlink(&claude, agents_parent.join("skills")).unwrap();
        fixture.skill(&fixture.environment.codex_home().join("skills/two"), "two");
        let preview = fixture.preview(Tool::Codex);
        // codex_home/skills 优先且可用；.agents/skills 不可用不隐藏其余来源。
        assert_eq!(preview.sources[1].status, SourceStatus::Unavailable);
        assert_eq!(preview.candidates[0].name, "two");
    }
    #[test]
    fn detection_and_confirmation_limits_are_explicit_and_never_stage_during_detection() {
        let fixture = Fixture::new();
        let root = fixture.environment.codex_home().join("skills");
        for index in 0..=MAX_CANDIDATE_ENTRIES {
            fs::create_dir_all(root.join(format!("entry-{index:03}"))).unwrap();
        }
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates.len(), MAX_CANDIDATE_ENTRIES);
        assert_eq!(
            preview.sources[0].diagnostic_code.as_deref(),
            Some("SKILL_IMPORT_SCAN_LIMIT")
        );
        assert_eq!(preview.sources[0].status, SourceStatus::Unavailable);
        fixture.assert_no_partial();
        let one = root.join("entry-000");
        fixture.skill(&one, "one");
        let evidence = library::resolve_skill_source(&root, &one).unwrap();
        assert!(library::inspect_skill_source(&evidence, &Cell::new(1)).is_err());
        fixture.assert_no_partial();
    }

    #[test]
    fn writer_and_central_changes_block_import_and_do_not_consume_the_token() {
        let mut fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        let run = Uuid::new_v4().to_string();
        fixture.database.connection().execute("INSERT INTO sync_runs(id, kind, status, scope, db_version) VALUES (?1, 'apply', 'applying', 'global', 1)", [&run]).unwrap();
        assert_eq!(
            fixture.confirm(&input).unwrap_err().code(),
            ErrorCode::WriteInProgress
        );
        fixture.assert_no_partial();
        fixture
            .database
            .connection()
            .execute("DELETE FROM sync_runs WHERE id = ?1", [&run])
            .unwrap();
        let other = fixture.root.join("other-source");
        fixture.skill(&other, "other");
        crate::skills::import_skill(
            &mut fixture.database,
            &fixture.paths,
            &crate::skills::ImportSkillInput {
                source_path: other.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(
            fixture.confirm(&input).unwrap_err().code(),
            ErrorCode::StalePreview
        );
        assert_eq!(skills::list_skills(&fixture.database).unwrap().len(), 1);
        assert_eq!(
            repository::get_preview(fixture.database.connection(), &input.preview_id)
                .unwrap()
                .status,
            "previewed"
        );
    }

    #[test]
    fn changes_during_copy_and_uncertain_rollback_preserve_whole_batch_boundary() {
        for mode in ["source", "uncertain_rollback"] {
            let mut fixture = Fixture::new();
            let root = fixture.environment.codex_home().join("skills");
            fixture.skill(&root.join("one"), "one");
            fixture.skill(&root.join("two"), "two");
            let input = Fixture::input(&fixture.preview(Tool::Codex));
            let result = confirm_with_fault(
                &mut fixture.database,
                &fixture.paths,
                &fixture.environment,
                &input,
                &|stage, index| {
                    if mode == "source" && stage == "copy" && index == 1 {
                        fs::write(root.join("one/asset.sh"), "changed").unwrap();
                    }
                    if mode == "uncertain_rollback" && stage == "uncertain_rollback" {
                        return Err(AppError::database("fixture", "uncertain_rollback"));
                    }
                    Ok(())
                },
            );
            assert!(result.is_err());
            fixture.assert_no_partial();
        }
    }

    #[test]
    fn known_central_links_reuse_across_tools_but_case_only_names_conflict() {
        let mut fixture = Fixture::new();
        fixture.skill(&fixture.environment.codex_home().join("skills/one"), "one");
        let input = Fixture::input(&fixture.preview(Tool::Codex));
        fixture.confirm(&input).unwrap();
        let central = skills::list_skills(&fixture.database).unwrap().remove(0);
        let claude = fixture.environment.claude_config_dir().join("skills");
        fs::create_dir_all(&claude).unwrap();
        symlink(&central.central_path, claude.join("central-alias")).unwrap();
        let preview = fixture.preview(Tool::Claude);
        assert_eq!(
            preview.candidates[0].status,
            CandidateStatus::AlreadyImported
        );
        assert_eq!(
            preview.candidates[0].existing_skill_id.as_deref(),
            Some(central.id.as_str())
        );
        assert!(preview.preview_id.is_none());
        fixture
            .database
            .connection()
            .execute(
                "UPDATE skills SET name = 'ONE' WHERE id = ?1",
                [&central.id],
            )
            .unwrap();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(preview.candidates[0].status, CandidateStatus::NameConflict);
        assert!(preview.preview_id.is_none());
    }
    #[test]
    fn broad_links_and_new_builtin_aliases_cannot_be_confirmed() {
        let mut fixture = Fixture::new();
        let root = fixture.environment.codex_home().join("skills");
        fixture.skill(&root.join("one"), "one");
        symlink(fixture.environment.home(), root.join("broad-home")).unwrap();
        let preview = fixture.preview(Tool::Codex);
        assert_eq!(
            preview
                .candidates
                .iter()
                .find(|candidate| candidate.name == "broad-home")
                .unwrap()
                .status,
            CandidateStatus::Invalid
        );
        let input = Fixture::input(&preview);
        symlink(root.join("one"), root.join(".system")).unwrap();
        assert!(fixture.confirm(&input).is_err());
        fixture.assert_no_partial();
    }
}
