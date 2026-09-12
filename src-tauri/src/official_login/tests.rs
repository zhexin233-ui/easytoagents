#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::{Mutex, MutexGuard},
        thread,
        time::{Duration, Instant},
    };

    use tempfile::tempdir;

    use super::{
        login_url_from_output, parse_claude_status, parse_codex_status, validate_context,
        OfficialLoginContext, OfficialLoginPhase, OfficialLoginRegistry,
    };
    use crate::{domain::Tool, error::ErrorCode, security::SecretRedactor};

    // 这些用例会 fork 带独立进程组的 shell fixture；串行执行避免超时断言互相干扰。
    static PROCESS_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

    fn isolate_process_fixture() -> MutexGuard<'static, ()> {
        PROCESS_FIXTURE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        bin: PathBuf,
        context: OfficialLoginContext,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            let bin = root.join("bin");
            for directory in [&home, &bin, &home.join(".claude"), &home.join(".codex")] {
                fs::create_dir_all(directory).unwrap();
            }
            Self {
                _temporary: temporary,
                context: OfficialLoginContext {
                    search_path: bin.clone().into_os_string(),
                    claude_config_dir: home.join(".claude"),
                    codex_home: home.join(".codex"),
                    home,
                    proxy: Some("http://127.0.0.1:10808".to_owned()),
                },
                bin,
            }
        }

        fn write_tool(&self, name: &str, body: &str) {
            write_executable(&self.bin.join(name), body);
        }

        /// 假 claude：`auth status` 固定输出登录 JSON；`auth login` 执行给定动作。
        fn write_claude(&self, login_action: &str) {
            self.write_tool(
                "claude",
                &format!(
                    r#"case "$1 $2" in
  "auth status") echo '{{"loggedIn":true,"authMethod":"claude.ai","email":"user@example.com"}}'; exit 0;;
  "auth login") echo "Opening browser..."; {login_action};;
  *) echo "unexpected: $*" >&2; exit 2;;
esac"#
                ),
            );
        }
    }

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn wait_for_phase(
        registry: &OfficialLoginRegistry,
        fixture: &Fixture,
        tool: Tool,
        expected: OfficialLoginPhase,
    ) -> super::OfficialLoginStatusDto {
        let redactor = SecretRedactor::default();
        let started = Instant::now();
        loop {
            let status = registry.status(&fixture.context, &redactor, tool).unwrap();
            if status.phase == expected {
                return status;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "等待阶段 {expected:?} 超时，当前 {:?}",
                status.phase
            );
            thread::sleep(Duration::from_millis(30));
        }
    }

    #[test]
    fn claude_json_and_codex_text_status_outputs_are_parsed() {
        let redactor = SecretRedactor::default();
        assert_eq!(
            login_url_from_output(
                b"Opening browser...\nhttps://claude.ai/oauth/authorize?code=1&state=s.\nPaste code here if prompted >"
            )
            .as_deref(),
            Some("https://claude.ai/oauth/authorize?code=1&state=s")
        );
        assert_eq!(login_url_from_output(b"no url here"), None);
        assert_eq!(login_url_from_output(b"https://"), None);
        let claude = parse_claude_status(
            "{\n  \"loggedIn\": true,\n  \"authMethod\": \"claude.ai\",\n  \"email\": \"user@example.com\",\n  \"subscriptionType\": \"max\"\n}\n",
            "",
            true,
            &redactor,
        );
        assert!(claude.supported);
        assert_eq!(claude.logged_in, Some(true));
        assert_eq!(claude.auth_method.as_deref(), Some("claude.ai"));
        assert_eq!(claude.account.as_deref(), Some("user@example.com"));

        let logged_out = parse_claude_status(r#"{"loggedIn":false}"#, "", false, &redactor);
        assert_eq!(logged_out.logged_in, Some(false));
        assert_eq!(logged_out.account, None);

        let unsupported = parse_claude_status("", "error: unknown command 'auth'", false, &redactor);
        assert!(!unsupported.supported);
        assert_eq!(unsupported.logged_in, None);

        let garbled = parse_claude_status(
            "",
            "TypeError: cannot read token=fixture-secret-value",
            false,
            &redactor,
        );
        assert!(garbled.supported);
        assert_eq!(garbled.logged_in, None);
        let diagnostic = garbled.diagnostic.unwrap();
        assert!(!diagnostic.contains("fixture-secret-value"));
        assert!(diagnostic.contains("TypeError"));

        let chatgpt = parse_codex_status("Logged in using ChatGPT\n", "", true, &redactor);
        assert_eq!(chatgpt.logged_in, Some(true));
        assert_eq!(chatgpt.auth_method.as_deref(), Some("chatgpt"));
        let api_key = parse_codex_status("Logged in using an API key\n", "", true, &redactor);
        assert_eq!(api_key.auth_method.as_deref(), Some("api_key"));
        let logged_out = parse_codex_status("Not logged in\n", "", false, &redactor);
        assert_eq!(logged_out.logged_in, Some(false));
        let unsupported = parse_codex_status(
            "",
            "error: unrecognized subcommand 'status'",
            false,
            &redactor,
        );
        assert!(!unsupported.supported);
        let unknown = parse_codex_status("", "something else happened", false, &redactor);
        assert_eq!(unknown.logged_in, None);
        assert!(unknown.diagnostic.is_some());
    }

    #[test]
    fn login_success_and_failure_are_reported_with_redacted_diagnostics() {
        let _guard = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_claude("exit 0");
        let registry = OfficialLoginRegistry::default();
        let redactor = SecretRedactor::default();
        let idle = registry
            .status(&fixture.context, &redactor, Tool::Claude)
            .unwrap();
        assert_eq!(idle.phase, OfficialLoginPhase::Idle);
        assert!(idle.supported);
        assert_eq!(idle.logged_in, Some(true));
        assert_eq!(idle.manual_command, "claude auth login");

        registry.start(&fixture.context, Tool::Claude).unwrap();
        let done = wait_for_phase(
            &registry,
            &fixture,
            Tool::Claude,
            OfficialLoginPhase::Succeeded,
        );
        assert_eq!(done.logged_in, Some(true));
        assert_eq!(done.account.as_deref(), Some("user@example.com"));

        fixture.write_claude("echo 'Raw mode is not supported token=fixture-login-secret' >&2; exit 1");
        registry.start(&fixture.context, Tool::Claude).unwrap();
        let failed = wait_for_phase(&registry, &fixture, Tool::Claude, OfficialLoginPhase::Failed);
        let diagnostic = failed.diagnostic.unwrap();
        assert!(diagnostic.contains("Raw mode is not supported"));
        assert!(!diagnostic.contains("fixture-login-secret"));
        // 失败会话不影响独立的状态探测结论。
        assert_eq!(failed.logged_in, Some(true));
    }

    #[test]
    fn running_login_can_be_cancelled_and_stalls_time_out() {
        let _guard = isolate_process_fixture();
        let fixture = Fixture::new();
        // 子进程 PATH 只有 fixture 的 bin 目录，休眠命令必须用绝对路径。
        fixture.write_tool(
            "codex",
            r#"if [ "$1" = login ] && [ "$2" = status ]; then echo "Not logged in"; exit 1; fi
echo "Starting local login server on http://localhost:1455."
echo "If your browser did not open, navigate to this URL to authenticate:"
echo ""
echo "https://auth.example.test/oauth/authorize?client_id=app_fixture&state=fixture-state"
/bin/sleep 30"#,
        );
        let registry = OfficialLoginRegistry::default();
        registry.start(&fixture.context, Tool::Codex).unwrap();
        let running = wait_for_login_url(&registry, &fixture, Tool::Codex);
        assert_eq!(running.phase, OfficialLoginPhase::Running);
        assert_eq!(running.logged_in, None);
        assert_eq!(
            running.login_url.as_deref(),
            Some("https://auth.example.test/oauth/authorize?client_id=app_fixture&state=fixture-state")
        );
        // 进行中时再次启动被拒绝。
        assert_eq!(
            registry
                .start(&fixture.context, Tool::Codex)
                .unwrap_err()
                .code(),
            ErrorCode::Conflict
        );
        assert!(registry.cancel(Tool::Codex).unwrap());
        let cancelled = wait_for_phase(
            &registry,
            &fixture,
            Tool::Codex,
            OfficialLoginPhase::Cancelled,
        );
        assert_eq!(cancelled.logged_in, Some(false));
        assert_eq!(cancelled.login_url, None);
        assert!(!registry.cancel(Tool::Codex).unwrap());

        let short = OfficialLoginRegistry::with_login_timeout(Duration::from_millis(200));
        short.start(&fixture.context, Tool::Codex).unwrap();
        let timed_out = wait_for_phase(&short, &fixture, Tool::Codex, OfficialLoginPhase::TimedOut);
        assert_eq!(timed_out.manual_command, "codex login");

        // 退出清理：`cancel_all` 只终止运行中的会话，并让其阶段变为 Cancelled。
        let exiting = OfficialLoginRegistry::default();
        assert_eq!(exiting.cancel_all(), 0);
        exiting.start(&fixture.context, Tool::Codex).unwrap();
        assert_eq!(exiting.cancel_all(), 1);
        wait_for_phase(&exiting, &fixture, Tool::Codex, OfficialLoginPhase::Cancelled);
        assert_eq!(exiting.cancel_all(), 0);
    }

    fn wait_for_login_url(
        registry: &OfficialLoginRegistry,
        fixture: &Fixture,
        tool: Tool,
    ) -> super::OfficialLoginStatusDto {
        let redactor = SecretRedactor::default();
        let started = Instant::now();
        loop {
            let status = registry.status(&fixture.context, &redactor, tool).unwrap();
            if status.login_url.is_some() || status.phase != OfficialLoginPhase::Running {
                return status;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "等待登录地址超时"
            );
            thread::sleep(Duration::from_millis(30));
        }
    }

    #[test]
    fn missing_or_unsupported_tools_fail_closed() {
        let _guard = isolate_process_fixture();
        let fixture = Fixture::new();
        let registry = OfficialLoginRegistry::default();
        let redactor = SecretRedactor::default();
        assert_eq!(
            registry
                .start(&fixture.context, Tool::Claude)
                .unwrap_err()
                .code(),
            ErrorCode::NotFound
        );
        let missing = registry
            .status(&fixture.context, &redactor, Tool::Claude)
            .unwrap();
        assert!(!missing.supported);
        assert_eq!(missing.logged_in, None);
        for tool in [Tool::Cursor, Tool::Zcode, Tool::Opencode] {
            assert_eq!(
                registry
                    .status(&fixture.context, &redactor, tool)
                    .unwrap_err()
                    .code(),
                ErrorCode::InvalidInput
            );
            assert_eq!(
                registry.start(&fixture.context, tool).unwrap_err().code(),
                ErrorCode::InvalidInput
            );
        }

        // 旧版本 CLI 没有 auth 子命令：探测报告不支持，并保留手动命令提示。
        fixture.write_tool(
            "claude",
            "echo \"error: unknown command 'auth'\" >&2; exit 1",
        );
        let unsupported = registry
            .status(&fixture.context, &redactor, Tool::Claude)
            .unwrap();
        assert!(!unsupported.supported);
        assert_eq!(unsupported.manual_command, "claude auth login");

        let mut relative = fixture.context.clone();
        relative.home = PathBuf::from("relative/home");
        assert_eq!(
            validate_context(&relative).unwrap_err().code(),
            ErrorCode::InvalidInput
        );
        let mut empty_path = fixture.context.clone();
        empty_path.search_path = std::ffi::OsString::new();
        assert!(validate_context(&empty_path).is_err());
        assert!(validate_context(&fixture.context).is_ok());
    }
}
