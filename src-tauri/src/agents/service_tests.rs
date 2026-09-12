#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Mutex};

    use tempfile::tempdir;

    use super::{
        apply_agent_preview, create_agent, delete_agent, get_agent, list_agent_project_options,
        list_global_agent_target_statuses, preview_agent_sync, readopt_agent_target,
        set_agent_enabled, set_global_agent_assignment, set_project_agent_assignment, update_agent,
        ApplyAgentPreviewInput, CreateAgentInput, PreviewAgentSyncInput, ReadoptAgentTargetInput,
        SetGlobalAgentAssignmentInput, SetProjectAgentAssignmentInput, UpdateAgentInput,
        VersionedAgentInput,
    };
    use crate::{
        adapters::{ExplicitEnvironment, ToolAvailability},
        app::AppPaths,
        db::Database,
        domain::{ChangeKind, ManagedProjectSelectionState, Scope, SyncStatus, Tool},
        error::ErrorCode,
        security::SecretRedactor,
    };

    /// 注册一个落在隔离根内的真实项目（走正式 register_project 合同）。
    fn register_project_fixture(fixture: &mut Fixture) -> crate::projects::ProjectDto {
        let project_root = fixture.root.join("fixture-project");
        fs::create_dir_all(&project_root).unwrap();
        crate::projects::register_project(
            &mut fixture.database,
            &fixture.environment,
            &crate::projects::RegisterProjectInput {
                display_name: "示例项目".to_owned(),
                root_path: project_root.to_string_lossy().into_owned(),
            },
        )
        .unwrap()
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        home: PathBuf,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
        write_operations: Mutex<()>,
        redactor: SecretRedactor,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            let codex_home = root.join("codex-home");
            // 各工具配置根必须真实存在（Apply 的 allowed_root 校验要求）。
            for directory in [
                &home,
                &codex_home,
                &home.join(".claude"),
                &home.join(".cursor"),
                &home.join(".zcode"),
                &home.join(".config/opencode"),
            ] {
                fs::create_dir_all(directory).unwrap();
            }
            let paths = AppPaths::from_data_root(root.join("app-data")).unwrap();
            let database = Database::open(&paths).unwrap();
            // Claude customization policy 证据：显式 Allowed（绑定安装版本），
            // 否则保守探针会让 Claude agents 全部 policy_blocked。
            let environment = ExplicitEnvironment::new(
                &home,
                None,
                Some(codex_home),
                ToolAvailability::all_installed(),
            )
            .unwrap()
            .with_claude_installation_version("fixture-1.0.0")
            .unwrap()
            .with_claude_customization_policy_evidence(
                crate::adapters::VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
                    "fixture-1.0.0",
                    None,
                )
                .unwrap(),
            );
            Self {
                _temporary: temporary,
                root,
                home,
                paths,
                database,
                environment,
                write_operations: Mutex::new(()),
                redactor: SecretRedactor::default(),
            }
        }

        /// 重新读取中央记录，拿到最新 row_version（每次分配/更新都会 bump）。
        fn current(&self, id: &str) -> crate::agents::AgentDto {
            get_agent(&self.database, id).unwrap()
        }

        fn claude_agents_dir(&self) -> PathBuf {
            self.environment.claude_config_dir().join("agents")
        }

        fn codex_agents_dir(&self) -> PathBuf {
            self.environment.codex_home().join("agents")
        }

        fn cursor_agents_dir(&self) -> PathBuf {
            self.home.join(".cursor/agents")
        }

        fn zcode_agents_dir(&self) -> PathBuf {
            self.home.join(".zcode/agents")
        }

        fn opencode_agents_dir(&self) -> PathBuf {
            self.home.join(".config/opencode/agents")
        }

        fn agent_input(name: &str) -> CreateAgentInput {
            CreateAgentInput {
                name: name.to_owned(),
                description: "评审代码改动".to_owned(),
                prompt: "你是代码评审助手。\n请逐条列出问题。".to_owned(),
                enabled: true,
            }
        }

        fn create(&mut self, name: &str) -> crate::agents::AgentDto {
            create_agent(&mut self.database, &Self::agent_input(name)).unwrap()
        }

        /// 停用 / 启用（自动取最新 row_version）。
        fn set_enabled(&mut self, agent_id: &str, enabled: bool) -> crate::agents::AgentDto {
            let current = self.current(agent_id);
            set_agent_enabled(
                &mut self.database,
                &VersionedAgentInput {
                    id: agent_id.to_owned(),
                    row_version: current.row_version,
                },
                enabled,
            )
            .unwrap()
        }

        fn assign_global(&mut self, agent: &crate::agents::AgentDto, tool: Tool) {
            // 分配会 bump row_version，因此每次先取最新版本再提交。
            let current = self.current(&agent.id);
            set_global_agent_assignment(
                &mut self.database,
                &SetGlobalAgentAssignmentInput {
                    tool,
                    agent_id: current.id.clone(),
                    assigned: true,
                    row_version: current.row_version,
                },
            )
            .unwrap();
        }

        fn preview_global(&mut self, tool: Tool) -> crate::sync::PreviewPlan {
            preview_agent_sync(
                &mut self.database,
                &self.environment,
                &mut self.redactor,
                &PreviewAgentSyncInput {
                    tool,
                    project_id: None,
                    exclude_from_git: false,
                },
            )
            .unwrap()
        }

        fn apply_global(&mut self, plan: &crate::sync::PreviewPlan, tool: Tool) {
            apply_agent_preview(
                &self.write_operations,
                &mut self.database,
                &self.paths,
                &self.environment,
                &ApplyAgentPreviewInput {
                    preview_id: plan.preview_id.clone(),
                    tool,
                    project_id: None,
                },
            )
            .unwrap();
        }
    }

    fn file_text(path: &std::path::Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    // -----------------------------------------------------------------------
    // 中央库 CRUD 与名称校验
    // -----------------------------------------------------------------------

    #[test]
    fn agent_crud_rejects_invalid_names_and_enforces_optimistic_concurrency() {
        let mut fixture = Fixture::new();
        let agent = fixture.create("code-reviewer");

        // 名称不符合交集规则 → 创建被拒。
        for bad in ["Code-Reviewer", "code_reviewer", "-lead", "名前", ""] {
            let mut input = Fixture::agent_input(bad);
            let error = create_agent(&mut fixture.database, &input).unwrap_err();
            assert_eq!(error.code(), ErrorCode::InvalidInput, "{bad}");
            let _ = &mut input;
        }

        // 更新与删除要求最新 row_version。
        let stale = UpdateAgentInput {
            id: agent.id.clone(),
            name: "renamed".to_owned(),
            description: agent.description.clone(),
            prompt: agent.prompt.clone(),
            enabled: true,
            row_version: agent.row_version - 1,
        };
        let error = update_agent(&mut fixture.database, &stale).unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        let updated = update_agent(
            &mut fixture.database,
            &UpdateAgentInput {
                row_version: agent.row_version,
                ..stale
            },
        )
        .unwrap();
        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.row_version, agent.row_version + 1);

        // 停用是显式动作。
        let disabled = set_agent_enabled(
            &mut fixture.database,
            &VersionedAgentInput {
                id: agent.id.clone(),
                row_version: updated.row_version,
            },
            false,
        )
        .unwrap();
        assert!(!disabled.enabled);

        // 删除：有分配时被 RESTRICT 阻止（先分配再删除）。
        let enabled = set_agent_enabled(
            &mut fixture.database,
            &VersionedAgentInput {
                id: agent.id.clone(),
                row_version: disabled.row_version,
            },
            true,
        )
        .unwrap();
        fixture.assign_global(&enabled, Tool::Claude);
        let error = delete_agent(
            &mut fixture.database,
            &VersionedAgentInput {
                id: agent.id.clone(),
                row_version: enabled.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert!(get_agent(&fixture.database, &agent.id).is_ok());
    }

    // -----------------------------------------------------------------------
    // 五工具投影 golden
    // -----------------------------------------------------------------------

    #[test]
    fn projections_render_each_tool_contract_and_are_byte_stable() {
        let mut fixture = Fixture::new();
        let agent = fixture.create("code-reviewer");
        fixture.assign_global(&agent, Tool::Claude);
        fixture.assign_global(&agent, Tool::Codex);
        fixture.assign_global(&agent, Tool::Cursor);
        fixture.assign_global(&agent, Tool::Zcode);
        fixture.assign_global(&agent, Tool::Opencode);

        // Markdown 系：YAML frontmatter（serde_yaml_ng 生成）+ 正文。
        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets.len(), 1);
        fixture.apply_global(&plan, Tool::Claude);
        let claude_file = fixture.claude_agents_dir().join("code-reviewer.md");
        let claude_text = file_text(&claude_file);
        assert_eq!(
            claude_text,
            "---\ndescription: 评审代码改动\nname: code-reviewer\n---\n\n你是代码评审助手。\n请逐条列出问题。\n",
            "Claude 投影必须是确定性的 frontmatter + 正文"
        );
        // 重复 Preview → Apply 幂等：第二次为 Unchanged 且无变化可应用。
        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Unchanged);

        // Cursor / ZCode 与 Claude 同构（各自目录）。
        let plan = fixture.preview_global(Tool::Cursor);
        assert_eq!(plan.targets.len(), 1);
        fixture.apply_global(&plan, Tool::Cursor);
        assert_eq!(
            file_text(&fixture.cursor_agents_dir().join("code-reviewer.md")),
            claude_text
        );
        let plan = fixture.preview_global(Tool::Zcode);
        assert_eq!(plan.targets.len(), 1);
        fixture.apply_global(&plan, Tool::Zcode);
        assert_eq!(
            file_text(&fixture.zcode_agents_dir().join("code-reviewer.md")),
            claude_text
        );

        // OpenCode：不写 name，固定 mode: subagent。
        let plan = fixture.preview_global(Tool::Opencode);
        fixture.apply_global(&plan, Tool::Opencode);
        let opencode_text =
            file_text(&fixture.opencode_agents_dir().join("code-reviewer.md"));
        assert_eq!(
            opencode_text,
            "---\ndescription: 评审代码改动\nmode: subagent\n---\n\n你是代码评审助手。\n请逐条列出问题。\n",
            "OpenCode 投影必须固定 mode: subagent 且不写 name"
        );

        // Codex：TOML 三字段，多行 developer_instructions 由 toml_edit 输出。
        let plan = fixture.preview_global(Tool::Codex);
        fixture.apply_global(&plan, Tool::Codex);
        let codex_text = file_text(&fixture.codex_agents_dir().join("code-reviewer.toml"));
        assert!(
            codex_text.contains("description = \"评审代码改动\""),
            "Codex 投影缺少 description：{codex_text}"
        );
        assert!(codex_text.contains("name = \"code-reviewer\""));
        assert!(
            codex_text.contains("developer_instructions = \"\"\"\n你是代码评审助手。\n请逐条列出问题。\"\"\""),
            "多行 developer_instructions 必须由 toml_edit 多行字面量承载：{codex_text}"
        );

        // 重复渲染字节一致（漂移判定稳定的前提）。
        let plan_again = fixture.preview_global(Tool::Codex);
        assert_eq!(plan_again.targets[0].change_kind, ChangeKind::Unchanged);
        let plan_again = fixture.preview_global(Tool::Opencode);
        assert_eq!(plan_again.targets[0].change_kind, ChangeKind::Unchanged);
    }

    // -----------------------------------------------------------------------
    // 同步链路：停用删除、多目标、状态聚合
    // -----------------------------------------------------------------------

    #[test]
    fn disable_then_apply_removes_managed_file_and_restore_recovers_content() {
        let mut fixture = Fixture::new();
        let agent = fixture.create("legacy-agent");
        fixture.assign_global(&agent, Tool::Claude);

        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Add);
        fixture.apply_global(&plan, Tool::Claude);
        let target_file = fixture.claude_agents_dir().join("legacy-agent.md");
        assert!(target_file.exists());

        // 停用后 Apply 删除受管文件（删除前有快照）。
        fixture.set_enabled(&agent.id, false);
        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Delete);
        fixture.apply_global(&plan, Tool::Claude);
        assert!(!target_file.exists(), "停用后 Apply 必须删除受管文件");
        assert!(
            crate::sync::list_snapshots(&fixture.database).unwrap().len() >= 2,
            "每次 Apply（新增 + 删除）都必须留下快照"
        );

        // 重新启用 → 文件恢复为同一投影。
        fixture.set_enabled(&agent.id, true);
        let plan = fixture.preview_global(Tool::Claude);
        fixture.apply_global(&plan, Tool::Claude);
        assert!(target_file.exists());
    }

    #[test]
    fn unassignment_then_apply_removes_file_without_touching_siblings() {
        let mut fixture = Fixture::new();
        let kept = fixture.create("kept-agent");
        let removed = fixture.create("removed-agent");
        fixture.assign_global(&kept, Tool::Claude);
        fixture.assign_global(&removed, Tool::Claude);
        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets.len(), 2, "两个 agent = 两个文件级目标");
        fixture.apply_global(&plan, Tool::Claude);

        // 取消分配 removed-agent → 仅它的文件被删除。
        let current = fixture.current(&removed.id);
        set_global_agent_assignment(
            &mut fixture.database,
            &SetGlobalAgentAssignmentInput {
                tool: Tool::Claude,
                agent_id: removed.id.clone(),
                assigned: false,
                row_version: current.row_version,
            },
        )
        .unwrap();
        let plan = fixture.preview_global(Tool::Claude);
        // kept-agent 为 Unchanged 目标，removed-agent 为 Delete 目标。
        assert_eq!(plan.targets.len(), 2);
        assert_eq!(
            plan.targets
                .iter()
                .find(|target| target.change_kind == ChangeKind::Delete)
                .map(|target| target.descriptor.path.as_deref()),
            Some(
                fixture
                    .claude_agents_dir()
                    .join("removed-agent.md")
                    .to_str()
            )
        );
        fixture.apply_global(&plan, Tool::Claude);
        assert!(!fixture.claude_agents_dir().join("removed-agent.md").exists());
        assert!(fixture.claude_agents_dir().join("kept-agent.md").exists());
    }

    #[test]
    fn external_drift_blocks_then_readopt_refreshes_baseline() {
        let mut fixture = Fixture::new();
        let agent = fixture.create("code-reviewer");
        fixture.assign_global(&agent, Tool::Claude);
        let plan = fixture.preview_global(Tool::Claude);
        fixture.apply_global(&plan, Tool::Claude);

        // 外部改写受管文件 → external_owned_change，readopt 可用。
        let target_file = fixture.claude_agents_dir().join("code-reviewer.md");
        fs::write(&target_file, "---\ndescription: 被外部改写\nname: code-reviewer\n---\n\n外部内容\n").unwrap();
        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets[0].status, SyncStatus::ExternalOwnedChange);
        assert!(plan.targets[0].readopt_available);

        let readopt = readopt_agent_target(
            &mut fixture.database,
            &fixture.environment,
            &ReadoptAgentTargetInput {
                tool: Tool::Claude,
                project_id: None,
                target_path: target_file.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(
            readopt.target_path,
            target_file.to_string_lossy().into_owned()
        );
        let plan = fixture.preview_global(Tool::Claude);
        // readopt 之后目标基线等于外部内容：状态回到 InSync（基线一致），
        // 但与中央投影不同 → change_kind 为 Update，Apply 会恢复中央内容。
        assert_eq!(plan.targets[0].status, SyncStatus::InSync);
        assert_eq!(plan.targets[0].change_kind, ChangeKind::Update);
    }

    #[test]
    fn global_status_card_aggregates_files_per_tool() {
        let mut fixture = Fixture::new();
        let statuses = list_global_agent_target_statuses(&fixture.database, &fixture.environment)
            .unwrap();
        assert_eq!(statuses.len(), 5);
        for status in &statuses {
            assert_eq!(status.aggregate_status, SyncStatus::Missing);
            assert!(status.files.is_empty());
        }

        // 两个 agent 同步后：Claude 卡片 in_sync 且展开两份文件。
        let first = fixture.create("agent-one");
        let second = fixture.create("agent-two");
        fixture.assign_global(&first, Tool::Claude);
        fixture.assign_global(&second, Tool::Claude);
        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets.len(), 2);
        fixture.apply_global(&plan, Tool::Claude);

        let statuses = list_global_agent_target_statuses(&fixture.database, &fixture.environment)
            .unwrap();
        let claude = statuses.iter().find(|s| s.tool == Tool::Claude).unwrap();
        assert_eq!(claude.aggregate_status, SyncStatus::InSync);
        assert_eq!(claude.files.len(), 2);
        assert!(claude
            .files
            .iter()
            .all(|file| file.status == SyncStatus::InSync));
        // 其他工具仍无受管文件。
        let codex = statuses.iter().find(|s| s.tool == Tool::Codex).unwrap();
        assert_eq!(codex.aggregate_status, SyncStatus::Missing);
    }

    #[test]
    fn global_status_preserves_capability_diagnostic_for_unavailable_tools() {
        let fixture = Fixture::new();
        let unavailable_environment = ExplicitEnvironment::new(
            &fixture.home,
            None,
            Some(fixture.environment.codex_home().to_path_buf()),
            ToolAvailability::all_unavailable(),
        )
        .unwrap();
        let statuses =
            list_global_agent_target_statuses(&fixture.database, &unavailable_environment).unwrap();
        assert_eq!(statuses.len(), 5);
        assert!(statuses.iter().all(|status| {
            status.aggregate_status == SyncStatus::Failed
                && status.diagnostic_code.as_deref() == Some("TOOL_NOT_INSTALLED")
        }));
    }

    // -----------------------------------------------------------------------
    // 项目级：ZCode 拒绝、全局继承与互斥、Codex 信任
    // -----------------------------------------------------------------------

    #[test]
    fn zcode_project_agents_are_rejected_by_service_and_database() {
        let mut fixture = Fixture::new();
        // 服务层作用域门禁。
        let error = crate::agents::agent_scope_supported(Tool::Zcode, Scope::Project).unwrap_err();
        assert_eq!(
            error.details().and_then(|details| details.get("reason")).and_then(serde_json::Value::as_str),
            Some("ZCODE_PROJECT_AGENTS_UNSUPPORTED")
        );

        // 项目详情页选项接口同样拒绝。
        let project = register_project_fixture(&mut fixture);
        let error = list_agent_project_options(
            &fixture.database,
            &crate::agents::AgentProjectOptionsInput {
                project_id: project.id.clone(),
                tool: Tool::Zcode,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);

        // 预览同样拒绝（不产生目标、不读文件系统）。
        let error = preview_agent_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut fixture.redactor,
            &PreviewAgentSyncInput {
                tool: Tool::Zcode,
                project_id: Some(project.id.clone()),
                exclude_from_git: false,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn project_assignments_inherit_global_and_reject_duplicates() {
        let mut fixture = Fixture::new();
        let project = register_project_fixture(&mut fixture);
        let agent = fixture.create("code-reviewer");
        fixture.assign_global(&agent, Tool::Claude);

        // 全局分配在项目内只读继承：选项为 Inherited 且不可再选。
        let options = list_agent_project_options(
            &fixture.database,
            &crate::agents::AgentProjectOptionsInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
            },
        )
        .unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].state, ManagedProjectSelectionState::Inherited);
        assert!(!options[0].selectable);

        // 全局分配中的 agent 不能重复加入项目（互斥触发器 + 领域校验）。
        let error = set_project_agent_assignment(
            &mut fixture.database,
            &SetProjectAgentAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                agent_id: agent.id.clone(),
                assigned: true,
                agent_row_version: agent.row_version,
                project_row_version: project.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);

        // 未全局分配的 agent 可以加入项目，并在项目预览中派生文件目标。
        let project_only = fixture.create("project-only");
        let assigned = set_project_agent_assignment(
            &mut fixture.database,
            &SetProjectAgentAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                agent_id: project_only.id.clone(),
                assigned: true,
                agent_row_version: project_only.row_version,
                project_row_version: project.row_version,
            },
        )
        .unwrap();
        let _ = assigned;
        let plan = preview_agent_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut fixture.redactor,
            &PreviewAgentSyncInput {
                tool: Tool::Claude,
                project_id: Some(project.id.clone()),
                exclude_from_git: false,
            },
        )
        .unwrap();
        // 项目级 desired = 项目自有（project-only）+ 全局继承（code-reviewer）。
        assert_eq!(plan.targets.len(), 2);
        assert_eq!(plan.scope, Scope::Project);
        let project_root = std::path::PathBuf::from(project.root_path.as_str());
        // 目标按名称排序：code-reviewer（全局继承）在前，project-only 在后。
        assert_eq!(
            plan.targets[0].descriptor.path.as_deref(),
            project_root
                .join(".claude/agents/code-reviewer.md")
                .to_str()
        );
        assert_eq!(
            plan.targets[1].descriptor.path.as_deref(),
            project_root
                .join(".claude/agents/project-only.md")
                .to_str()
        );
        apply_agent_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyAgentPreviewInput {
                preview_id: plan.preview_id.clone(),
                tool: Tool::Claude,
                project_id: Some(project.id.clone()),
            },
        )
        .unwrap();
        assert!(project_root
            .join(".claude/agents/project-only.md")
            .exists());
        // 全局继承的 agent（code-reviewer）也一并写入项目目录。
        assert!(project_root
            .join(".claude/agents/code-reviewer.md")
            .exists());
    }

    #[test]
    fn codex_untrusted_project_blocks_preview_apply() {
        let mut fixture = Fixture::new();
        // 写入 untrusted 信任声明（官方 config.toml projects.<root>.trust_level）。
        let project = register_project_fixture(&mut fixture);
        fs::create_dir_all(fixture.environment.codex_home().join("agents")).unwrap();
        fs::write(
            fixture.environment.codex_home().join("config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"untrusted\"\n",
                project.root_path
            ),
        )
        .unwrap();
        let agent = fixture.create("code-reviewer");
        set_project_agent_assignment(
            &mut fixture.database,
            &SetProjectAgentAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Codex,
                agent_id: agent.id.clone(),
                assigned: true,
                agent_row_version: agent.row_version,
                project_row_version: project.row_version,
            },
        )
        .unwrap();
        let plan = preview_agent_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut fixture.redactor,
            &PreviewAgentSyncInput {
                tool: Tool::Codex,
                project_id: Some(project.id.clone()),
                exclude_from_git: false,
            },
        )
        .unwrap();
        assert_eq!(plan.targets[0].status, SyncStatus::Untrusted);
        assert_eq!(
            plan.targets[0].change_kind,
            ChangeKind::Conflict,
            "不受信任的项目目标不可应用"
        );
        let error = apply_agent_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyAgentPreviewInput {
                preview_id: plan.preview_id.clone(),
                tool: Tool::Codex,
                project_id: Some(project.id.clone()),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        // 原生目录没有任何写入。
        assert!(!std::path::PathBuf::from(project.root_path.as_str())
            .join(".codex/agents/code-reviewer.toml")
            .exists());
    }

    // -----------------------------------------------------------------------
    // 空集不建目标
    // -----------------------------------------------------------------------

    #[test]
    fn empty_desired_and_no_targets_produce_neither_targets_nor_run() {
        let mut fixture = Fixture::new();
        let runs_before: i64 = fixture
            .database
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_runs", [], |row| row.get(0))
            .unwrap();
        let plan = fixture.preview_global(Tool::Claude);
        assert!(plan.targets.is_empty(), "空投影不建目标");
        let runs_after: i64 = fixture
            .database
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(runs_after, runs_before, "空预览不应持久化运行");
        let agent = fixture.create("code-reviewer");
        fixture.assign_global(&agent, Tool::Claude);
        let plan = fixture.preview_global(Tool::Claude);
        assert_eq!(plan.targets.len(), 1);
    }

    // -----------------------------------------------------------------------
    // 原生导入：只读发现、fail-closed、dropped_fields、名称冲突
    // -----------------------------------------------------------------------

    mod import_tests {
        use super::{Fixture, Tool};
        use crate::{
            agents::{
                confirm_agent_import, discover_agent_import, AgentImportCandidateDto,
                ConfirmAgentImportInput, CreateAgentInput, DiscoverAgentImportInput,
            },
            error::ErrorCode,
        };
        use std::fs;

        fn write_agent(fixture: &Fixture, name: &str, content: &str) {
            let directory = fixture.claude_agents_dir();
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join(name), content).unwrap();
        }

        fn discover(fixture: &mut Fixture) -> Vec<AgentImportCandidateDto> {
            discover_agent_import(
                &mut fixture.database,
                &fixture.environment,
                &DiscoverAgentImportInput { tool: Tool::Claude },
            )
            .unwrap()
            .candidates
        }

        fn discover_codex(fixture: &mut Fixture) -> Vec<AgentImportCandidateDto> {
            discover_agent_import(
                &mut fixture.database,
                &fixture.environment,
                &DiscoverAgentImportInput { tool: Tool::Codex },
            )
            .unwrap()
            .candidates
        }

        #[test]
        fn discover_lists_candidates_with_dropped_fields_and_name_fallback() {
            let mut fixture = Fixture::new();
            write_agent(
                &fixture,
                "code-reviewer.md",
                "---\nname: code-reviewer\ndescription: 评审\ntools: [Read]\nmodel: inherit\n---\n\n正文第一行\n",
            );
            // 无 frontmatter 的 OpenCode 风格文件：name 取文件名，缺 description。
            write_agent(&fixture, "no-frontmatter.md", "只有正文\n");

            let candidates = discover(&mut fixture);
            assert_eq!(candidates.len(), 2);
            let reviewer = &candidates[0];
            assert!(reviewer.importable);
            assert_eq!(reviewer.name, "code-reviewer");
            assert_eq!(reviewer.description, "评审");
            assert_eq!(reviewer.prompt, "正文第一行");
            assert_eq!(reviewer.dropped_fields, vec!["model", "tools"]);

            let fallback = &candidates[1];
            assert!(!fallback.importable);
            assert_eq!(fallback.name, "no-frontmatter");
            assert_eq!(
                fallback.diagnostic_code.as_deref(),
                Some("AGENT_REQUIRED_FIELD_MISSING")
            );
        }

        #[test]
        fn discover_skips_links_directories_and_foreign_extensions_with_count() {
            let mut fixture = Fixture::new();
            let directory = fixture.claude_agents_dir();
            fs::create_dir_all(&directory).unwrap();
            write_agent(&fixture, "valid.md", "---\ndescription: 描述\n---\n\n正文\n");
            fs::create_dir_all(directory.join("subdir")).unwrap();
            fs::write(directory.join("notes.txt"), "不是 agent 文件").unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(
                directory.join("valid.md"),
                directory.join("linked.md"),
            )
            .unwrap();

            let preview = discover_agent_import(
                &mut fixture.database,
                &fixture.environment,
                &DiscoverAgentImportInput { tool: Tool::Claude },
            )
            .unwrap();
            assert_eq!(preview.candidates.len(), 1);
            assert!(preview.candidates[0].importable);
            assert!(preview
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("已跳过 3 个"));
        }

        #[cfg(unix)]
        #[test]
        fn discover_rejects_symlinked_agents_directory() {
            let mut fixture = Fixture::new();
            let directory = fixture.claude_agents_dir();
            fs::create_dir_all(&directory).unwrap();
            let outside = fixture.root.join("outside-agents");
            fs::create_dir_all(&outside).unwrap();
            fs::write(
                outside.join("outside.md"),
                "---\ndescription: 描述\n---\n\n正文\n",
            )
            .unwrap();
            fs::remove_dir(&directory).unwrap();
            std::os::unix::fs::symlink(&outside, &directory).unwrap();

            let error = discover_agent_import(
                &mut fixture.database,
                &fixture.environment,
                &DiscoverAgentImportInput { tool: Tool::Claude },
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::Conflict);
        }

        #[test]
        fn discover_marks_invalid_frontmatter_and_names_as_not_importable() {
            let mut fixture = Fixture::new();
            write_agent(
                &fixture,
                "broken.md",
                "---\ndescription: [未闭合\n---\n\n正文\n",
            );
            write_agent(
                &fixture,
                "Bad_Name.md",
                "---\nname: Bad_Name\ndescription: 描述\n---\n\n正文\n",
            );
            let candidates = discover(&mut fixture);
            assert_eq!(candidates.len(), 2);
            let by_name = |name: &str| {
                candidates
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .unwrap()
            };
            assert_eq!(
                by_name("broken").diagnostic_code.as_deref(),
                Some("AGENT_FRONTMATTER_INVALID")
            );
            assert_eq!(
                by_name("Bad_Name").diagnostic_code.as_deref(),
                Some("AGENT_NAME_INVALID")
            );
            assert!(candidates.iter().all(|candidate| !candidate.importable));
        }

        #[test]
        fn discover_rejects_nul_in_import_fields() {
            let mut fixture = Fixture::new();
            write_agent(
                &fixture,
                "nul.md",
                "---\ndescription: 描述\n---\n\n正\0文\n",
            );
            let candidates = discover(&mut fixture);
            assert_eq!(candidates.len(), 1);
            assert!(!candidates[0].importable);
            assert_eq!(
                candidates[0].diagnostic_code.as_deref(),
                Some("AGENT_FIELD_INVALID")
            );
        }

        #[test]
        fn codex_import_requires_three_fields_and_drops_extras() {
            let mut fixture = Fixture::new();
            let directory = fixture.codex_agents_dir();
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("complete.toml"),
                "name = \"codex-agent\"\ndescription = \"描述\"\ndeveloper_instructions = \"\"\"\n多行正文\n\"\"\"\nmodel = \"gpt-5\"\n",
            )
            .unwrap();
            fs::write(
                directory.join("missing.toml"),
                "name = \"missing-agent\"\ndescription = \"描述\"\n",
            )
            .unwrap();
            fs::write(
                directory.join("missing-name.toml"),
                "description = \"描述\"\ndeveloper_instructions = \"正文\"\n",
            )
            .unwrap();

            let candidates = discover_codex(&mut fixture);
            assert_eq!(candidates.len(), 3);
            let complete = candidates
                .iter()
                .find(|candidate| candidate.name == "codex-agent")
                .unwrap();
            assert!(complete.importable);
            assert_eq!(complete.prompt, "多行正文");
            assert_eq!(complete.dropped_fields, vec!["model"]);
            // developer_instructions 缺失：解析即失败，候选名保留文件名去扩展名。
            let missing = candidates
                .iter()
                .find(|candidate| candidate.name == "missing")
                .unwrap();
            assert!(!missing.importable);
            assert_eq!(
                missing.diagnostic_code.as_deref(),
                Some("AGENT_REQUIRED_FIELD_MISSING")
            );
            // name 缺失也必须 fail-closed，而不是静默使用文件名作为 Codex name。
            let missing_name = candidates
                .iter()
                .find(|candidate| candidate.name == "missing-name")
                .unwrap();
            assert!(!missing_name.importable);
            assert_eq!(
                missing_name.diagnostic_code.as_deref(),
                Some("AGENT_REQUIRED_FIELD_MISSING")
            );
        }

        #[test]
        fn confirm_import_creates_central_records_without_touching_native_files() {
            let mut fixture = Fixture::new();
            let source = fixture.claude_agents_dir().join("code-reviewer.md");
            fs::create_dir_all(fixture.claude_agents_dir()).unwrap();
            fs::write(
                &source,
                "---\nname: code-reviewer\ndescription: 评审\n---\n\n正文\n",
            )
            .unwrap();
            let candidates = discover(&mut fixture);
            assert_eq!(candidates.len(), 1);
            let candidate = &candidates[0];
            assert!(candidate.importable);

            let result = confirm_agent_import(
                &mut fixture.database,
                &fixture.environment,
                &ConfirmAgentImportInput {
                    tool: Tool::Claude,
                    agents: vec![CreateAgentInput {
                        name: candidate.name.clone(),
                        description: candidate.description.clone(),
                        prompt: candidate.prompt.clone(),
                        enabled: true,
                    }],
                },
            )
            .unwrap();
            assert_eq!(result.created_count, 1);
            // 导入不隐式接管原生文件：源文件原样保留，也没有 managed target 行。
            assert!(source.exists());
            assert_eq!(
                fs::read_to_string(&source).unwrap(),
                "---\nname: code-reviewer\ndescription: 评审\n---\n\n正文\n"
            );
            let agents = crate::agents::list_agents(&fixture.database).unwrap();
            assert_eq!(agents.len(), 1);
            assert_eq!(agents[0].name, "code-reviewer");
            assert!(agents[0].global_assignments.is_empty());

            // 名称冲突：再次导入同名（名称规则只允许小写，冲突即精确同名，
            // 由 agents.name 的 NOCASE 唯一索引拦截）→ CONFLICT。
            let error = confirm_agent_import(
                &mut fixture.database,
                &fixture.environment,
                &ConfirmAgentImportInput {
                    tool: Tool::Claude,
                    agents: vec![CreateAgentInput {
                        name: "code-reviewer".to_owned(),
                        description: candidate.description.clone(),
                        prompt: candidate.prompt.clone(),
                        enabled: true,
                    }],
                },
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::Conflict);
        }
    }
}
