#[cfg(test)]
mod tests {
    use super::{
        build_desired_projection, build_hook_ownership, events_root, hook_event_supported,
        native_selector_root, HookRecord, ParsedNativeHook,
    };
    use crate::domain::{HookEvent, Tool};
    use serde_json::json;

    fn record(name: &str, event: HookEvent, matcher: Option<&str>) -> HookRecord {
        HookRecord {
            id: "00000000-0000-4000-8000-000000000001".to_owned(),
            name: name.to_owned(),
            event,
            matcher: matcher.map(str::to_owned),
            command: "bash /fixture/hook.sh".to_owned(),
            timeout_seconds: Some(30),
            enabled: true,
            script_name: None,
            script_hash: None,
            row_version: 1,
        }
    }

    #[test]
    fn event_support_matrix_matches_official_contracts() {
        // Claude / Codex / ZCode / Cursor 的官方可配置事件集合（2026-09-05 核验）。
        assert!(HookEvent::PreToolUse.supported_for_tool(Tool::Claude));
        assert!(HookEvent::Notification.supported_for_tool(Tool::Claude));
        assert!(!HookEvent::PostToolUseFailure.supported_for_tool(Tool::Claude));
        assert!(HookEvent::PostCompact.supported_for_tool(Tool::Codex));
        assert!(!HookEvent::Notification.supported_for_tool(Tool::Codex));
        assert!(HookEvent::PostToolUseFailure.supported_for_tool(Tool::Zcode));
        assert!(!HookEvent::SessionEnd.supported_for_tool(Tool::Zcode));
        assert!(HookEvent::Stop.supported_for_tool(Tool::Cursor));
        assert!(!HookEvent::UserPromptSubmit.supported_for_tool(Tool::Cursor));
        for tool in [Tool::Claude, Tool::Codex, Tool::Zcode, Tool::Cursor] {
            assert!(hook_event_supported(tool, HookEvent::PreToolUse).is_ok());
        }
        assert!(hook_event_supported(Tool::Cursor, HookEvent::UserPromptSubmit).is_err());
        assert_eq!(
            hook_event_supported(Tool::Opencode, HookEvent::PreToolUse)
                .unwrap_err()
                .details()
                .and_then(|details| details.get("reason"))
                .and_then(serde_json::Value::as_str),
            Some("OPENCODE_HOOKS_UNSUPPORTED")
        );
    }

    #[test]
    fn native_keys_and_selector_roots_follow_each_tool_contract() {
        assert_eq!(HookEvent::Stop.native_key(Tool::Claude), "Stop");
        assert_eq!(HookEvent::Stop.native_key(Tool::Cursor), "stop");
        assert_eq!(
            HookEvent::PostToolUseFailure.native_key(Tool::Cursor),
            "postToolUseFailure"
        );
        assert_eq!(native_selector_root(Tool::Claude), &["hooks"][..]);
        assert_eq!(
            native_selector_root(Tool::Cursor),
            &["version", "hooks"][..]
        );
        assert_eq!(events_root(Tool::Zcode), &["hooks", "events"][..]);
    }

    #[test]
    fn projections_follow_each_tool_shape() {
        // Claude：hooks 键下为事件 → matcher 组。
        let claude = build_desired_projection(
            Tool::Claude,
            &[record("block-rm", HookEvent::PreToolUse, Some("Bash"))],
        )
        .unwrap();
        assert_eq!(
            claude,
            json!({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
                {"type": "command", "command": "bash /fixture/hook.sh", "timeout": 30}
            ]}]}})
        );

        // Codex：独立 hooks.json，投影只含 hooks 键（保留用户 description）。
        let codex = build_desired_projection(
            Tool::Codex,
            &[record("session-notes", HookEvent::SessionStart, None)],
        )
        .unwrap();
        assert_eq!(
            codex,
            json!({"hooks": {"SessionStart": [{"hooks": [
                {"type": "command", "command": "bash /fixture/hook.sh", "timeout": 30}
            ]}]}})
        );

        // ZCode：runner 级 enabled 恒为 true，事件嵌套在 events 键下。
        let zcode = build_desired_projection(
            Tool::Zcode,
            &[record("notify", HookEvent::PostToolUseFailure, None)],
        )
        .unwrap();
        assert_eq!(
            zcode,
            json!({"hooks": {"enabled": true, "events": {"PostToolUseFailure": [{"hooks": [
                {"type": "command", "command": "bash /fixture/hook.sh", "timeout": 30}
            ]}]}}})
        );

        // Cursor：version + hooks 双键、camelCase 事件、扁平条目、matcher 属于条目。
        let cursor = build_desired_projection(
            Tool::Cursor,
            &[record("format", HookEvent::PostToolUse, Some("Write|Edit"))],
        )
        .unwrap();
        assert_eq!(
            cursor,
            json!({"version": 1, "hooks": {"postToolUse": [
                {"command": "bash /fixture/hook.sh", "timeout": 30, "matcher": "Write|Edit"}
            ]}})
        );
    }

    #[test]
    fn unsupported_event_projection_fails_closed() {
        let error = build_desired_projection(
            Tool::Cursor,
            &[record("prompt-submit", HookEvent::UserPromptSubmit, None)],
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn empty_projection_removes_managed_subtree() {
        for tool in [Tool::Claude, Tool::Codex, Tool::Zcode, Tool::Cursor] {
            assert_eq!(
                build_desired_projection(tool, &[]).unwrap(),
                json!({}),
                "空投影应触发 selector 移除"
            );
        }
    }

    #[test]
    fn shell_word_splitting_respects_quotes_and_rejects_unclosed() {
        use super::split_shell_words;
        assert_eq!(
            split_shell_words("bash \"/path with space/x.sh\" --flag"),
            Some(vec![
                "bash".to_owned(),
                "/path with space/x.sh".to_owned(),
                "--flag".to_owned()
            ])
        );
        assert_eq!(
            split_shell_words("bash 'COST=$1' x.sh"),
            Some(vec![
                "bash".to_owned(),
                "COST=$1".to_owned(),
                "x.sh".to_owned()
            ])
        );
        assert_eq!(split_shell_words("bash \"unclosed"), None);
    }

    #[test]
    fn script_adoption_resolution_is_interpreter_and_file_gated() {
        use super::resolve_script_adoption;
        let temporary = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temporary.path()).unwrap();
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".claude/hooks")).unwrap();
        let script = home.join(".claude/hooks/block-rm.sh");
        std::fs::write(&script, b"#!/bin/bash\ntrue\n").unwrap();

        // 解释器 + 可解析文件 → 接管（支持 ~ 展开）。
        let adopted = resolve_script_adoption(
            &format!("bash {} --verbose", script.to_string_lossy()),
            &home,
        )
        .unwrap();
        assert_eq!(adopted.1, script);

        let tilde = resolve_script_adoption("bash ~/.claude/hooks/block-rm.sh", &home).unwrap();
        assert_eq!(tilde.1, script);

        // $HOME 展开同样可接管。
        let dollar =
            resolve_script_adoption("bash \"$HOME/.claude/hooks/block-rm.sh\"", &home).unwrap();
        assert_eq!(dollar.1, script);

        // 非 解释器命令 → 不接管。
        assert!(
            resolve_script_adoption(&format!("{} --run", script.to_string_lossy()), &home)
                .is_none()
        );
        // 解释器但只有项目级变量路径 → 不接管。
        assert!(resolve_script_adoption(
            "bash \"${CLAUDE_PROJECT_DIR}/.claude/hooks/x.sh\"",
            &home
        )
        .is_none());
        // 解释器但文件不存在 → 不接管。
        assert!(resolve_script_adoption("bash /missing/hook.sh", &home).is_none());
        // 纯 inline 命令 → 不接管。
        assert!(resolve_script_adoption("echo hello", &home).is_none());

        // /usr/bin/env 间接层：env + 解释器 + 脚本 → 接管（回归 2026-09-05 反馈）。
        let env_form = resolve_script_adoption(
            &format!("/usr/bin/env python3 {}", script.to_string_lossy()),
            &home,
        )
        .unwrap();
        assert_eq!(env_form.1, script);
        let env_flagged = resolve_script_adoption(
            &format!("/usr/bin/env -S python3 {}", script.to_string_lossy()),
            &home,
        )
        .unwrap();
        assert_eq!(env_flagged.1, script);
        // env 后不是已知解释器 → 不接管。
        assert!(resolve_script_adoption(
            &format!("/usr/bin/env {}", script.to_string_lossy()),
            &home
        )
        .is_none());
        // 带次版本号的解释器（python3.11）→ 接管。
        let versioned =
            resolve_script_adoption(&format!("python3.11 {}", script.to_string_lossy()), &home)
                .unwrap();
        assert_eq!(versioned.1, script);
    }

    #[test]
    fn command_rewrite_replaces_script_token_with_quoted_central_path() {
        use super::rewrite_command_with_script;
        let rewritten = rewrite_command_with_script(
            "irrelevant",
            "bash /origin/hooks/x.sh --verbose",
            std::path::Path::new("/origin/hooks/x.sh"),
            std::path::Path::new("/central/0001/x.sh"),
        )
        .unwrap();
        assert_eq!(rewritten, "bash \"/central/0001/x.sh\" --verbose");
        // 检测后原路径变化 → 冲突而非静默错写。
        assert!(rewrite_command_with_script(
            "irrelevant",
            "bash /changed/x.sh",
            std::path::Path::new("/origin/hooks/x.sh"),
            std::path::Path::new("/central/0001/x.sh"),
        )
        .is_err());
    }

    #[test]
    fn ownership_covers_every_selector_root() {
        for tool in [Tool::Claude, Tool::Codex, Tool::Zcode, Tool::Cursor] {
            let ownership = build_hook_ownership(tool);
            let crate::adapters::ManagedOwnership::Selectors(selectors) = &ownership else {
                panic!("hooks 必须使用选择器 ownership");
            };
            let roots = native_selector_root(tool);
            assert_eq!(selectors.len(), roots.len());
            for (selector, root) in selectors.iter().zip(roots) {
                assert_eq!(selector.len(), 1);
                assert_eq!(&selector[0], root);
            }
        }
    }

    // -----------------------------------------------------------------------
    // ExternalChangePlan 原生 Hook 采纳
    // -----------------------------------------------------------------------

    struct AdoptionFixture {
        _temporary: tempfile::TempDir,
        home: std::path::PathBuf,
        paths: crate::app::AppPaths,
        database: crate::db::Database,
        environment: crate::adapters::ExplicitEnvironment,
        write_operations: std::sync::Mutex<()>,
        redactor: crate::security::SecretRedactor,
    }

    impl AdoptionFixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = std::fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            let codex_home = root.join("codex-home");
            for directory in [
                &home,
                &codex_home,
                &home.join(".claude"),
                &home.join(".cursor"),
                &home.join(".zcode/cli"),
                &home.join(".config/opencode"),
            ] {
                std::fs::create_dir_all(directory).unwrap();
            }
            let paths = crate::app::AppPaths::from_data_root(root.join("app-data")).unwrap();
            let database = crate::db::Database::open(&paths).unwrap();
            let environment = crate::adapters::ExplicitEnvironment::new(
                &home,
                None,
                Some(codex_home),
                crate::adapters::ToolAvailability::all_installed(),
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
                home,
                paths,
                database,
                environment,
                write_operations: std::sync::Mutex::new(()),
                redactor: crate::security::SecretRedactor::default(),
            }
        }

        fn create_assigned_hook(&mut self, command: &str) -> HookRecord {
            let dto = super::create_hook(
                &mut self.database,
                &self.paths,
                &super::CreateHookInput {
                    name: "adopt-hook".to_owned(),
                    event: HookEvent::PreToolUse,
                    matcher: Some("Bash".to_owned()),
                    command: command.to_owned(),
                    timeout_seconds: Some(30),
                    enabled: true,
                    script_source_path: None,
                },
            )
            .unwrap();
            super::set_global_hook_assignment(
                &mut self.database,
                &super::SetGlobalHookAssignmentInput {
                    tool: Tool::Claude,
                    hook_id: dto.id.clone(),
                    event: HookEvent::PreToolUse,
                    assigned: true,
                    row_version: dto.row_version,
                },
            )
            .unwrap();
            crate::db::hooks::get_hook(&self.database, &dto.id).unwrap()
        }

        fn preview(&mut self) -> crate::sync::PreviewPlan {
            super::preview_hook_sync(
                &mut self.database,
                &self.environment,
                &mut self.redactor,
                &super::PreviewHookSyncInput {
                    tool: Tool::Claude,
                    project_id: None,
                    exclude_from_git: false,
                },
            )
            .unwrap()
        }

        fn apply(&mut self, plan: &crate::sync::PreviewPlan) {
            super::apply_hook_preview(
                &self.write_operations,
                &mut self.database,
                &self.paths,
                &self.environment,
                &super::ApplyHookPreviewInput {
                    preview_id: plan.preview_id.clone(),
                    tool: Tool::Claude,
                    project_id: None,
                },
            )
            .unwrap();
        }

        fn settings_path(&self) -> std::path::PathBuf {
            self.environment.claude_config_dir().join("settings.json")
        }

        fn write_native(&self, value: &serde_json::Value) {
            std::fs::write(
                self.settings_path(),
                serde_json::to_vec(value).unwrap(),
            )
            .unwrap();
        }

        fn adopt_input(plan: &crate::sync::PreviewPlan) -> super::AdoptHookNativeInput {
            let target = &plan.targets[0];
            super::AdoptHookNativeInput {
                tool: Tool::Claude,
                project_id: None,
                target_id: target.target_id.clone(),
                target_row_version: target.target_row_version,
                target_path: target.descriptor.path.clone().unwrap(),
                row_versions: target.row_versions.clone(),
                observed_full_hash: target.current_full_hash.clone(),
                observed_managed_hash: target.current_managed_hash.clone(),
            }
        }
    }

    #[test]
    fn adopt_native_hook_updates_central_item_and_baseline_then_is_in_sync() {
        let mut fixture = AdoptionFixture::new();
        let hook = fixture.create_assigned_hook("echo before");
        let initial = fixture.preview();
        fixture.apply(&initial);

        // Selector ownership must preserve an unrelated top-level field while
        // the changed native command/timeout is adopted into the central Hook.
        fixture.write_native(&json!({
            "env": {"KEEP_ME": "unchanged"},
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command", "command": "echo after", "timeout": 45
            }]}]}
        }));
        let plan = fixture.preview();
        assert_eq!(plan.targets[0].status, crate::domain::SyncStatus::ExternalOwnedChange);
        let result = super::adopt_hook_native(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            AdoptionFixture::adopt_input(&plan),
        )
        .unwrap();
        assert_eq!(result.adopted, vec![hook.id.clone()]);

        let adopted = crate::db::hooks::get_hook(&fixture.database, &hook.id).unwrap();
        assert_eq!(adopted.command, "echo after");
        assert_eq!(adopted.timeout_seconds, Some(45));
        let item = crate::db::hooks::list_managed_hook_items(
            &fixture.database,
            &plan.targets[0].target_id,
        )
        .unwrap();
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].last_applied_item_hash.len(), 64);
        let native: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture.settings_path()).unwrap()).unwrap();
        assert_eq!(native["env"]["KEEP_ME"], "unchanged");

        let after = fixture.preview();
        assert_eq!(after.targets[0].status, crate::domain::SyncStatus::InSync);
        assert_eq!(after.targets[0].change_kind, crate::domain::ChangeKind::Unchanged);
    }

    #[test]
    fn adopt_native_hook_does_not_refresh_baseline_for_non_owned_only_change() {
        let mut fixture = AdoptionFixture::new();
        let hook = fixture.create_assigned_hook("echo before");
        let initial = fixture.preview();
        fixture.apply(&initial);
        let baseline_before = crate::sync::load_managed_target_baseline(
            &fixture.database,
            &initial.targets[0].target_id,
        )
        .unwrap();
        let row_version_before = crate::db::hooks::get_hook(&fixture.database, &hook.id)
            .unwrap()
            .row_version;

        fixture.write_native(&json!({
            "env": {"ONLY_NON_OWNED": "changed"},
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command", "command": "echo before", "timeout": 30
            }]}]}
        }));
        let plan = fixture.preview();
        assert_eq!(
            plan.targets[0].status,
            crate::domain::SyncStatus::ExternalNonOwnedChange
        );
        let error = super::adopt_hook_native(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            AdoptionFixture::adopt_input(&plan),
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::Conflict);
        assert_eq!(
            crate::db::hooks::get_hook(&fixture.database, &hook.id)
                .unwrap()
                .row_version,
            row_version_before
        );
        assert_eq!(
            crate::sync::load_managed_target_baseline(&fixture.database, &initial.targets[0].target_id)
                .unwrap(),
            baseline_before
        );
    }

    #[test]
    fn adopt_native_hook_stages_script_and_cleans_staging_after_success() {
        let mut fixture = AdoptionFixture::new();
        let hook = fixture.create_assigned_hook("echo before");
        let initial = fixture.preview();
        fixture.apply(&initial);

        let script = fixture.home.join(".claude/hooks/native.sh");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        let bytes = b"#!/bin/sh\necho native\n";
        std::fs::write(&script, bytes).unwrap();
        fixture.write_native(&json!({
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command",
                "command": format!("bash {}", script.to_string_lossy()),
                "timeout": 31
            }]}]}
        }));
        let plan = fixture.preview();
        let result = super::adopt_hook_native(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            AdoptionFixture::adopt_input(&plan),
        )
        .unwrap();
        assert_eq!(result.adopted, vec![hook.id.clone()]);
        let adopted = crate::db::hooks::get_hook(&fixture.database, &hook.id).unwrap();
        assert_eq!(adopted.command, format!("bash {}", script.to_string_lossy()));
        assert_eq!(adopted.script_name.as_deref(), Some("native.sh"));
        assert_eq!(adopted.script_hash.as_deref(), Some(crate::sync::hash_bytes(bytes).as_str()));
        let central = fixture
            .paths
            .central_hooks()
            .join(&hook.id)
            .join("native.sh");
        assert_eq!(std::fs::read(central).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(fixture.paths.staging()).unwrap().count(), 0);
        let after = fixture.preview();
        assert_eq!(after.targets[0].status, crate::domain::SyncStatus::InSync);
    }

    #[test]
    fn adopt_native_hook_tracks_script_content_change_without_target_json_drift() {
        let mut fixture = AdoptionFixture::new();
        let source = fixture.home.join(".claude/hooks/stable.sh");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"#!/bin/sh\necho one\n").unwrap();
        let created = super::create_hook(
            &mut fixture.database,
            &fixture.paths,
            &super::CreateHookInput {
                name: "script-content-hook".to_owned(),
                event: HookEvent::PreToolUse,
                matcher: Some("Bash".to_owned()),
                command: format!("bash {}", source.to_string_lossy()),
                timeout_seconds: Some(30),
                enabled: true,
                script_source_path: Some(source.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        let assigned = super::set_global_hook_assignment(
            &mut fixture.database,
            &super::SetGlobalHookAssignmentInput {
                tool: Tool::Claude,
                hook_id: created.id.clone(),
                event: HookEvent::PreToolUse,
                assigned: true,
                row_version: created.row_version,
            },
        )
        .unwrap();
        // Keep the command pointing at the native script while retaining the
        // central copy metadata. This is the shape in which a script-only
        // native edit has no JSON target hash delta.
        let current = crate::db::hooks::get_hook(&fixture.database, &assigned.id).unwrap();
        super::update_hook(
            &mut fixture.database,
            &super::UpdateHookInput {
                id: current.id.clone(),
                name: current.name.clone(),
                event: current.event,
                matcher: current.matcher.clone(),
                command: format!("bash {}", source.to_string_lossy()),
                timeout_seconds: current.timeout_seconds,
                enabled: current.enabled,
                row_version: u32::try_from(current.row_version).unwrap(),
            },
        )
        .unwrap();
        let initial = fixture.preview();
        fixture.apply(&initial);

        std::fs::write(&source, b"#!/bin/sh\necho two\n").unwrap();
        let plan = fixture.preview();
        assert_eq!(plan.targets[0].status, crate::domain::SyncStatus::InSync);
        let result = super::adopt_hook_native(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            AdoptionFixture::adopt_input(&plan),
        )
        .unwrap();
        assert_eq!(result.adopted, vec![assigned.id.clone()]);
        let adopted = crate::db::hooks::get_hook(&fixture.database, &assigned.id).unwrap();
        assert_eq!(adopted.script_hash.as_deref(), Some(crate::sync::hash_bytes(
            b"#!/bin/sh\necho two\n",
        )
        .as_str()));
        let central = fixture
            .paths
            .central_hooks()
            .join(&assigned.id)
            .join(adopted.script_name.as_deref().unwrap());
        assert_eq!(std::fs::read(central).unwrap(), b"#!/bin/sh\necho two\n");
    }

    #[test]
    fn adopt_native_hook_rejects_stale_hash_and_row_without_writing() {
        let mut fixture = AdoptionFixture::new();
        let hook = fixture.create_assigned_hook("echo before");
        let initial = fixture.preview();
        fixture.apply(&initial);
        fixture.write_native(&json!({
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command", "command": "echo one", "timeout": 30
            }]}]}
        }));
        let plan = fixture.preview();
        fixture.write_native(&json!({
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command", "command": "echo two", "timeout": 30
            }]}]}
        }));
        let error = super::adopt_hook_native(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            AdoptionFixture::adopt_input(&plan),
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::StalePreview);
        assert_eq!(crate::db::hooks::get_hook(&fixture.database, &hook.id).unwrap().command, "echo before");

        // A fresh observation is still rejected after a central row change;
        // this also proves the managed item and native script staging path are
        // behind the same row-version fence.
        fixture.write_native(&json!({
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command", "command": "echo three", "timeout": 30
            }]}]}
        }));
        let row_plan = fixture.preview();
        let current = crate::db::hooks::get_hook(&fixture.database, &hook.id).unwrap();
        super::update_hook(
            &mut fixture.database,
            &super::UpdateHookInput {
                id: current.id.clone(),
                name: current.name.clone(),
                event: current.event,
                matcher: current.matcher.clone(),
                command: "echo central-concurrent".to_owned(),
                timeout_seconds: current.timeout_seconds,
                enabled: current.enabled,
                row_version: u32::try_from(current.row_version).unwrap(),
            },
        )
        .unwrap();
        let error = super::adopt_hook_native(
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            AdoptionFixture::adopt_input(&row_plan),
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::StalePreview);
        assert_eq!(
            crate::db::hooks::get_hook(&fixture.database, &hook.id)
                .unwrap()
                .command,
            "echo central-concurrent"
        );
    }

    #[test]
    fn adopt_native_hook_database_transaction_rolls_back_after_source_recheck_failure() {
        let mut fixture = AdoptionFixture::new();
        let hook = fixture.create_assigned_hook("echo before");
        let initial = fixture.preview();
        fixture.apply(&initial);
        let baseline_before = crate::sync::load_managed_target_baseline(
            &fixture.database,
            &initial.targets[0].target_id,
        )
        .unwrap();
        fixture.write_native(&json!({
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command", "command": "echo adopted", "timeout": 31
            }]}]}
        }));
        let plan = fixture.preview();
        let target = &plan.targets[0];
        let descriptor = target.descriptor.clone();
        let ownership = build_hook_ownership(Tool::Claude);
        let observed = super::scan_hook_for_adoption(Tool::Claude, &descriptor, &ownership).unwrap();
        let parsed = super::validate_native_hook_projection(
            &observed,
            Tool::Claude,
            fixture.environment.home(),
        )
        .unwrap();
        let item = crate::db::hooks::list_managed_hook_items(
            &fixture.database,
            &target.target_id,
        )
        .unwrap()
        .pop()
        .unwrap();
        let record = crate::db::hooks::get_hook(&fixture.database, &hook.id).unwrap();
        let adopted_record = HookRecord {
            id: record.id.clone(),
            name: record.name.clone(),
            event: parsed[0].event,
            matcher: parsed[0].matcher.clone(),
            command: parsed[0].command.clone(),
            timeout_seconds: parsed[0].timeout_seconds,
            enabled: true,
            script_name: None,
            script_hash: None,
            row_version: record.row_version,
        };
        let hook_update = crate::db::hooks::NativeHookAdoption {
            id: record.id.clone(),
            row_version: u32::try_from(record.row_version).unwrap(),
            event: parsed[0].event,
            matcher: parsed[0].matcher.clone(),
            command: parsed[0].command.clone(),
            timeout_seconds: parsed[0].timeout_seconds,
            script_name: None,
            script_hash: None,
        };
        let item_update = crate::db::hooks::NativeHookItemAdoption {
            id: item.id.clone(),
            target_id: target.target_id.clone(),
            row_version: u32::try_from(item.row_version).unwrap(),
            resource_id: record.id.clone(),
            external_key: super::hook_external_key(&adopted_record),
            last_applied_item_hash: crate::sync::hash_json(&parsed[0].native_entry),
        };
        let target_update = crate::db::hooks::NativeHookTargetAdoption {
            target_id: target.target_id.clone(),
            target_row_version: target.target_row_version,
            target_path: target.descriptor.path.clone().unwrap(),
            tool: Tool::Claude,
            scope: crate::domain::Scope::Global,
            project_id: None,
            observed_full_hash: observed.full_hash.clone(),
            observed_managed_hash: observed.managed_hash.clone(),
            baseline_projection_json: serde_json::to_string(&observed.managed_projection).unwrap(),
            row_versions: target.row_versions.clone(),
        };
        let calls = std::cell::Cell::new(0_u8);
        let error = crate::db::hooks::adopt_native_hooks(
            &mut fixture.database,
            &target_update,
            &[hook_update],
            &[item_update],
            || {
                let call = calls.get();
                calls.set(call + 1);
                if call == 1 {
                    Err(crate::error::AppError::database(
                        "fixture",
                        "simulate_source_changed",
                    ))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::DatabaseError);
        assert_eq!(
            crate::db::hooks::get_hook(&fixture.database, &hook.id)
                .unwrap()
                .command,
            "echo before"
        );
        let restored_item = crate::db::hooks::list_managed_hook_items(
            &fixture.database,
            &target.target_id,
        )
        .unwrap();
        assert_eq!(restored_item[0].last_applied_item_hash, item.last_applied_item_hash);
        assert_eq!(
            crate::sync::load_managed_target_baseline(&fixture.database, &target.target_id)
                .unwrap()
                ,
            baseline_before
        );
    }

    #[test]
    fn native_hook_mapping_and_parser_fail_closed_for_ambiguity_shell_and_secrets() {
        use super::{match_native_hook_rows, parse_native_hook_entry};

        let temporary = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(temporary.path()).unwrap();
        let script = home.join("native.sh");
        std::fs::write(&script, b"echo safe\n").unwrap();
        let parsed = parse_native_hook_entry(
            Tool::Claude,
            HookEvent::PreToolUse,
            "Bash",
            json!({
                "type": "command",
                "command": format!("bash {}", script.to_string_lossy()),
                "timeout": 20
            }),
            &home,
        )
        .unwrap();
        assert_eq!(parsed.timeout_seconds, Some(20));
        assert_eq!(parsed.script_source.as_deref(), Some(script.as_path()));
        assert!(parse_native_hook_entry(
            Tool::Claude,
            HookEvent::PreToolUse,
            "Bash",
            json!({"type": "command", "command": "bash -c 'echo unsafe'"}),
            &home,
        )
        .is_err());
        let secret = "sk-native-secret-value";
        let error = parse_native_hook_entry(
            Tool::Claude,
            HookEvent::PreToolUse,
            "Bash",
            json!({"type": "command", "command": secret}),
            &home,
        )
        .unwrap_err();
        assert!(!error.to_string().contains(secret));
        assert!(parse_native_hook_entry(
            Tool::Claude,
            HookEvent::PreToolUse,
            "sk-native-secret-value",
            json!({"type": "command", "command": "echo safe"}),
            &home,
        )
        .is_err());
        assert!(parse_native_hook_entry(
            Tool::Claude,
            HookEvent::PreToolUse,
            "Bash",
            json!({"type": "command", "command": "echo safe", "cwd": "/tmp"}),
            &home,
        )
        .is_err());

        let make_record = |id: &str| HookRecord {
            id: id.to_owned(),
            name: id.to_owned(),
            event: HookEvent::PreToolUse,
            matcher: Some("Bash".to_owned()),
            command: "echo old".to_owned(),
            timeout_seconds: Some(30),
            enabled: true,
            script_name: None,
            script_hash: None,
            row_version: 1,
        };
        let left = make_record("00000000-0000-4000-8000-000000000101");
        let right = make_record("00000000-0000-4000-8000-000000000102");
        let items = [left.clone(), right.clone()]
            .iter()
            .enumerate()
            .map(|(index, record)| crate::db::hooks::ManagedHookItemRecord {
                id: format!("00000000-0000-4000-8000-00000000010{}", index + 3),
                resource_id: record.id.clone(),
                external_key: super::hook_external_key(record),
                last_applied_item_hash: "a".repeat(64),
                row_version: 1,
            })
            .collect::<Vec<_>>();
        let rows = vec![
            ParsedNativeHook {
                event: HookEvent::PreToolUse,
                matcher: Some("Bash".to_owned()),
                command: "echo changed-a".to_owned(),
                timeout_seconds: Some(30),
                native_entry: json!({"type": "command", "command": "echo changed-a"}),
                script_source: None,
            },
            ParsedNativeHook {
                event: HookEvent::PreToolUse,
                matcher: Some("Bash".to_owned()),
                command: "echo changed-b".to_owned(),
                timeout_seconds: Some(30),
                native_entry: json!({"type": "command", "command": "echo changed-b"}),
                script_source: None,
            },
        ];
        assert!(match_native_hook_rows(&[left, right], &items, &rows).is_err());

        // Arrays have no native id: moving two unchanged entries across event
        // groups must not be interpreted as two ordinary content edits.
        let mut pre = make_record("00000000-0000-4000-8000-000000000103");
        pre.event = HookEvent::PreToolUse;
        pre.matcher = None;
        pre.command = "echo pre".to_owned();
        let mut post = make_record("00000000-0000-4000-8000-000000000104");
        post.event = HookEvent::PostToolUse;
        post.matcher = None;
        post.command = "echo post".to_owned();
        let pre_entry = json!({"type": "command", "command": "echo pre", "timeout": 30});
        let post_entry = json!({"type": "command", "command": "echo post", "timeout": 30});
        let swapped_items = [pre.clone(), post.clone()]
            .iter()
            .enumerate()
            .map(|(index, record)| crate::db::hooks::ManagedHookItemRecord {
                id: format!(
                    "00000000-0000-4000-8000-00000000020{}",
                    index + 1
                ),
                resource_id: record.id.clone(),
                external_key: super::hook_external_key(record),
                last_applied_item_hash: crate::sync::hash_json(if index == 0 {
                    &pre_entry
                } else {
                    &post_entry
                }),
                row_version: 1,
            })
            .collect::<Vec<_>>();
        let swapped_rows = vec![
            ParsedNativeHook {
                event: HookEvent::PostToolUse,
                matcher: None,
                command: "echo pre".to_owned(),
                timeout_seconds: Some(30),
                native_entry: pre_entry,
                script_source: None,
            },
            ParsedNativeHook {
                event: HookEvent::PreToolUse,
                matcher: None,
                command: "echo post".to_owned(),
                timeout_seconds: Some(30),
                native_entry: post_entry,
                script_source: None,
            },
        ];
        assert!(match_native_hook_rows(&[pre, post], &swapped_items, &swapped_rows).is_err());
    }

    #[test]
    fn native_adoption_rejects_noncanonical_hashes_and_cursor_matchers() {
        assert!(super::is_sha256(&"a".repeat(64)));
        assert!(!super::is_sha256(&"A".repeat(64)));

        let temporary = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(temporary.path()).unwrap();
        let error = super::parse_native_hook_entry(
            Tool::Cursor,
            HookEvent::PreToolUse,
            "",
            json!({"command": "echo safe", "matcher": 42}),
            &home,
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::Conflict);
    }
}
