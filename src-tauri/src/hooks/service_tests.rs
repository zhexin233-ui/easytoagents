#[cfg(test)]
mod tests {
    use super::{
        build_desired_projection, build_hook_ownership, events_root, hook_event_supported,
        native_selector_root, HookRecord,
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
}
