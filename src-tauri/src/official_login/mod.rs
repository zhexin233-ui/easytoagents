//! 官方账号登录：把 OAuth 交给官方 CLI（`claude auth login` / `codex login`）完成。
//!
//! 本模块只负责三件事：探测当前登录状态、启动/监视/取消登录子进程、把结果整理成
//! 前端可读的 DTO。凭据始终由 CLI 写入它自己的存储（macOS Keychain / `auth.json`），
//! 本应用不读取、不保存、不转发任何 token。子进程遵循安装探针的安全约束：只在安全
//! PATH 条目里解析可执行文件、清空继承环境后显式注入所需变量、独立进程组、超时后
//! 终止整个进程组、输出有界且脱敏后才进入 DTO。

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex, PoisonError},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::Value;
use specta::Type;

use crate::{
    app::tool_probe::{
        apply_tool_process_environment, resolve_executable, run_command_bounded, set_nonblocking,
        terminate_process_group, CommandFailure, ExecutableResolution,
    },
    domain::Tool,
    error::{AppError, ErrorCode},
    security::SecretRedactor,
};

/// 浏览器授权通常在一两分钟内完成；十分钟后仍未结束视为超时并终止子进程。
pub const DEFAULT_LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// `claude auth status` 需要启动 Node 运行时，比 `--version` 探针慢一些。
pub const STATUS_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const STATUS_OUTPUT_LIMIT_BYTES: u64 = 16 * 1024;
const LOGIN_OUTPUT_KEEP_BYTES: usize = 4096;
const DIAGNOSTIC_MAX_CHARS: usize = 300;
const LOGIN_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OfficialLoginPhase {
    /// 没有进行中或刚结束的登录会话。
    Idle,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OfficialLoginStatusDto {
    pub tool: Tool,
    /// 官方 CLI 已解析到且提供登录子命令；为 `false` 时只能按 `manual_command` 手动登录。
    pub supported: bool,
    pub phase: OfficialLoginPhase,
    /// `None` 表示状态探测不可用（CLI 未安装、超时或输出无法识别）。
    pub logged_in: Option<bool>,
    /// CLI 报告的登录方式，如 `claude.ai`、`console`、`chatgpt`、`api_key`。
    pub auth_method: Option<String>,
    /// CLI 报告的账号标识（邮箱或组织名）；只展示，不落库不写日志。
    pub account: Option<String>,
    /// 最近一次登录子进程或状态探测的脱敏诊断片段。
    pub diagnostic: Option<String>,
    /// 登录子进程打印的授权地址；浏览器没有自动打开时供用户手动访问。
    pub login_url: Option<String>,
    pub manual_command: String,
}

/// 启动子进程所需的显式输入；由 `AppState` 从探针配置与代理设置组装。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialLoginContext {
    pub search_path: OsString,
    pub home: PathBuf,
    pub claude_config_dir: PathBuf,
    pub codex_home: PathBuf,
    /// 启动期捕获的 shell 代理；登录需要访问认证服务器，按 HTTP(S)/ALL_PROXY 三件套注入。
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct LoginCommandSpec {
    login_args: &'static [&'static str],
    status_args: &'static [&'static str],
    manual_command: &'static str,
}

fn command_spec(tool: Tool) -> Result<LoginCommandSpec, AppError> {
    match tool {
        Tool::Claude => Ok(LoginCommandSpec {
            login_args: &["auth", "login", "--claudeai"],
            status_args: &["auth", "status", "--json"],
            manual_command: "claude auth login",
        }),
        Tool::Codex => Ok(LoginCommandSpec {
            login_args: &["login"],
            status_args: &["login", "status"],
            manual_command: "codex login",
        }),
        Tool::Cursor | Tool::Zcode | Tool::Opencode => {
            Err(AppError::invalid_input("tool", "该工具不支持官方账号登录"))
        }
    }
}

fn executable_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
        Tool::Cursor => "agent",
        Tool::Zcode => "zcode",
        Tool::Opencode => "opencode",
    }
}

/// 状态探测结果；`logged_in == None` 表示无法判断。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ProbedStatus {
    supported: bool,
    logged_in: Option<bool>,
    auth_method: Option<String>,
    account: Option<String>,
    diagnostic: Option<String>,
}

#[derive(Debug)]
struct LoginSessionState {
    phase: OfficialLoginPhase,
    /// 只保留末尾若干字节，足够展示失败原因，不会无限增长。
    output: Vec<u8>,
    cancel_requested: bool,
    process_group: Option<i32>,
}

type SharedSession = Arc<Mutex<LoginSessionState>>;

/// 每个工具至多一个登录会话；会话在进程内存活，重启应用即丢弃。
#[derive(Default)]
pub struct OfficialLoginRegistry {
    sessions: Mutex<HashMap<Tool, SharedSession>>,
    login_timeout: Option<Duration>,
}

impl OfficialLoginRegistry {
    #[cfg(test)]
    pub(crate) fn with_login_timeout(timeout: Duration) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            login_timeout: Some(timeout),
        }
    }

    fn session(&self, tool: Tool) -> Option<SharedSession> {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&tool)
            .cloned()
    }

    /// 探测登录状态并合并会话阶段；登录子进程运行中时不再额外探测，避免并发
    /// 启动同一 CLI。
    pub fn status(
        &self,
        context: &OfficialLoginContext,
        redactor: &SecretRedactor,
        tool: Tool,
    ) -> Result<OfficialLoginStatusDto, AppError> {
        let spec = command_spec(tool)?;
        let (phase, session_diagnostic, login_url) = match self.session(tool) {
            Some(session) => {
                let state = session.lock().unwrap_or_else(PoisonError::into_inner);
                (
                    state.phase,
                    diagnostic_from_output(&state.output, redactor),
                    login_url_from_output(&state.output),
                )
            }
            None => (OfficialLoginPhase::Idle, None, None),
        };
        if phase == OfficialLoginPhase::Running {
            return Ok(OfficialLoginStatusDto {
                tool,
                supported: true,
                phase,
                logged_in: None,
                auth_method: None,
                account: None,
                diagnostic: None,
                login_url,
                manual_command: spec.manual_command.to_owned(),
            });
        }
        let probed = probe_status(context, redactor, tool, spec);
        Ok(OfficialLoginStatusDto {
            tool,
            supported: probed.supported,
            phase,
            logged_in: probed.logged_in,
            auth_method: probed.auth_method,
            account: probed.account,
            // 刚结束的登录会话比状态探测更能解释"为什么没登上"。
            diagnostic: match phase {
                OfficialLoginPhase::Failed
                | OfficialLoginPhase::TimedOut
                | OfficialLoginPhase::Cancelled => session_diagnostic.or(probed.diagnostic),
                OfficialLoginPhase::Idle
                | OfficialLoginPhase::Running
                | OfficialLoginPhase::Succeeded => probed.diagnostic,
            },
            login_url: None,
            manual_command: spec.manual_command.to_owned(),
        })
    }

    /// 启动官方 CLI 的登录流程；CLI 自己打开浏览器并等待回调，本方法立即返回。
    pub fn start(&self, context: &OfficialLoginContext, tool: Tool) -> Result<(), AppError> {
        let spec = command_spec(tool)?;
        let executable = resolve_tool_executable(context, tool)?;
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        if sessions.get(&tool).is_some_and(|session| {
            session.lock().unwrap_or_else(PoisonError::into_inner).phase
                == OfficialLoginPhase::Running
        }) {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "该工具的官方账号登录已在进行中",
                true,
            ));
        }
        let mut command = Command::new(&executable);
        Command::args(&mut command, spec.login_args);
        apply_tool_process_environment(
            &mut command,
            &context.home,
            &context.claude_config_dir,
            &context.codex_home,
            &context.search_path,
        );
        if let Some(proxy) = context.proxy.as_deref() {
            for name in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
                Command::env(&mut command, name, proxy);
            }
        }
        Command::stdin(&mut command, Stdio::null());
        Command::stdout(&mut command, Stdio::piped());
        Command::stderr(&mut command, Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            AppError::io_from(
                &executable.to_string_lossy(),
                "spawn_official_login",
                &error,
            )
        })?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            terminate_process_group(&mut child);
            let _ = child.wait();
            return Err(AppError::internal("登录子进程缺少输出管道"));
        };
        if set_nonblocking(&stdout).is_err() || set_nonblocking(&stderr).is_err() {
            terminate_process_group(&mut child);
            let _ = child.wait();
            return Err(AppError::internal("登录子进程输出管道无法设为非阻塞"));
        }
        let session = Arc::new(Mutex::new(LoginSessionState {
            phase: OfficialLoginPhase::Running,
            output: Vec::new(),
            cancel_requested: false,
            process_group: i32::try_from(child.id()).ok(),
        }));
        sessions.insert(tool, Arc::clone(&session));
        drop(sessions);
        let timeout = self.login_timeout.unwrap_or(DEFAULT_LOGIN_TIMEOUT);
        thread::spawn(move || monitor_login(child, stdout, stderr, session, timeout));
        Ok(())
    }

    /// 终止进行中的登录子进程；返回是否确实有会话被取消。
    pub fn cancel(&self, tool: Tool) -> Result<bool, AppError> {
        command_spec(tool)?;
        let Some(session) = self.session(tool) else {
            return Ok(false);
        };
        Ok(cancel_session(&session))
    }

    /// 应用退出时终止所有进行中的登录子进程：CLI 会一直等待浏览器回调，
    /// 遗留的 `codex login` 还会占住 1455 回调端口。
    pub fn cancel_all(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter(|session| cancel_session(session))
            .count()
    }
}

impl Drop for OfficialLoginRegistry {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

fn cancel_session(session: &SharedSession) -> bool {
    let mut state = session.lock().unwrap_or_else(PoisonError::into_inner);
    if state.phase != OfficialLoginPhase::Running {
        return false;
    }
    state.cancel_requested = true;
    if let Some(process_group) = state.process_group {
        // SAFETY: 负 pid 只定位由本进程为该登录 child 创建的独立进程组。
        unsafe {
            libc::kill(-process_group, libc::SIGKILL);
        }
    }
    true
}

fn resolve_tool_executable(
    context: &OfficialLoginContext,
    tool: Tool,
) -> Result<PathBuf, AppError> {
    match resolve_executable(&context.search_path, executable_name(tool)) {
        ExecutableResolution::Found { path, .. } => Ok(path),
        ExecutableResolution::Unavailable { .. } => {
            Err(AppError::not_found("toolInstallation", tool.as_str()))
        }
        ExecutableResolution::Unsupported { reason } => {
            Err(AppError::invalid_input("toolInstallation", reason))
        }
    }
}

fn monitor_login(
    mut child: std::process::Child,
    mut stdout: std::process::ChildStdout,
    mut stderr: std::process::ChildStderr,
    session: SharedSession,
    timeout: Duration,
) {
    let started = Instant::now();
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut timed_out = false;
    loop {
        {
            let mut state = session.lock().unwrap_or_else(PoisonError::into_inner);
            drain_tail(&mut stdout, &mut state.output, &mut stdout_closed);
            drain_tail(&mut stderr, &mut state.output, &mut stderr_closed);
        }
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(_) => {
                terminate_process_group(&mut child);
                let _ = child.wait();
                finish_session(&session, OfficialLoginPhase::Failed);
                return;
            }
        };
        if let Some(status) = status {
            // 主进程已退出（pid 已被回收）：先在锁内清掉进程组标识，让并发的
            // `cancel` 不再向可能被复用的 pid 发信号；再终止可能残留的同组后代
            // （浏览器唤起助手等），不等它们释放 pipe；最后做一次读取保留收尾输出。
            let cancelled = {
                let mut state = session.lock().unwrap_or_else(PoisonError::into_inner);
                state.process_group = None;
                state.cancel_requested
            };
            terminate_process_group(&mut child);
            {
                let mut state = session.lock().unwrap_or_else(PoisonError::into_inner);
                drain_tail(&mut stdout, &mut state.output, &mut stdout_closed);
                drain_tail(&mut stderr, &mut state.output, &mut stderr_closed);
            }
            let phase = if cancelled {
                OfficialLoginPhase::Cancelled
            } else if timed_out {
                OfficialLoginPhase::TimedOut
            } else if status.success() {
                OfficialLoginPhase::Succeeded
            } else {
                OfficialLoginPhase::Failed
            };
            finish_session(&session, phase);
            return;
        }
        if !timed_out && started.elapsed() >= timeout {
            timed_out = true;
            terminate_process_group(&mut child);
        }
        thread::sleep(LOGIN_POLL_INTERVAL);
    }
}

fn finish_session(session: &SharedSession, phase: OfficialLoginPhase) {
    let mut state = session.lock().unwrap_or_else(PoisonError::into_inner);
    state.phase = phase;
    state.process_group = None;
}

/// 非阻塞读取并只保留末尾 `LOGIN_OUTPUT_KEEP_BYTES` 字节。
fn drain_tail(reader: &mut impl Read, output: &mut Vec<u8>, closed: &mut bool) {
    if *closed {
        return;
    }
    let mut buffer = [0_u8; 512];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                *closed = true;
                return;
            }
            Ok(read) => {
                output.extend_from_slice(&buffer[..read]);
                if output.len() > LOGIN_OUTPUT_KEEP_BYTES {
                    let drop_count = output.len() - LOGIN_OUTPUT_KEEP_BYTES;
                    output.drain(..drop_count);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => {
                *closed = true;
                return;
            }
        }
    }
}

fn probe_status(
    context: &OfficialLoginContext,
    redactor: &SecretRedactor,
    tool: Tool,
    spec: LoginCommandSpec,
) -> ProbedStatus {
    let executable = match resolve_tool_executable(context, tool) {
        Ok(path) => path,
        Err(error) => {
            return ProbedStatus {
                supported: false,
                diagnostic: Some(error.message().to_owned()),
                ..ProbedStatus::default()
            };
        }
    };
    let mut command = Command::new(&executable);
    Command::args(&mut command, spec.status_args);
    apply_tool_process_environment(
        &mut command,
        &context.home,
        &context.claude_config_dir,
        &context.codex_home,
        &context.search_path,
    );
    Command::env(&mut command, "CI", "1");
    let output = match run_command_bounded(
        &mut command,
        STATUS_PROBE_TIMEOUT,
        STATUS_OUTPUT_LIMIT_BYTES,
    ) {
        Ok(output) => output,
        Err(failure) => {
            return ProbedStatus {
                supported: true,
                diagnostic: Some(command_failure_text(failure).to_owned()),
                ..ProbedStatus::default()
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    match tool {
        Tool::Claude => parse_claude_status(&stdout, &stderr, output.status.success(), redactor),
        Tool::Codex => parse_codex_status(&stdout, &stderr, output.status.success(), redactor),
        Tool::Cursor | Tool::Zcode | Tool::Opencode => ProbedStatus::default(),
    }
}

fn parse_claude_status(
    stdout: &str,
    stderr: &str,
    exit_success: bool,
    redactor: &SecretRedactor,
) -> ProbedStatus {
    let json_start = stdout.find('{');
    let parsed = json_start
        .and_then(|start| serde_json::from_str::<Value>(stdout[start..].trim()).ok())
        .filter(Value::is_object);
    let Some(status) = parsed else {
        if mentions_unknown_command(stderr) || mentions_unknown_command(stdout) {
            return ProbedStatus {
                supported: false,
                diagnostic: Some("当前 Claude Code 版本没有 auth 子命令，请升级后重试".to_owned()),
                ..ProbedStatus::default()
            };
        }
        return ProbedStatus {
            supported: true,
            logged_in: None,
            diagnostic: diagnostic_from_text(&format!("{stdout}\n{stderr}"), redactor),
            ..ProbedStatus::default()
        };
    };
    let string_field = |key: &str| {
        status
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let logged_in = status.get("loggedIn").and_then(Value::as_bool);
    ProbedStatus {
        supported: true,
        // 缺少 loggedIn 字段的旧输出退回到退出码判断。
        logged_in: logged_in.or(Some(exit_success)),
        auth_method: string_field("authMethod"),
        account: string_field("email")
            .or_else(|| string_field("orgName"))
            .or_else(|| string_field("subscriptionType")),
        diagnostic: None,
    }
}

fn parse_codex_status(
    stdout: &str,
    stderr: &str,
    exit_success: bool,
    redactor: &SecretRedactor,
) -> ProbedStatus {
    let text = format!("{stdout}\n{stderr}");
    let lowercase = text.to_ascii_lowercase();
    if mentions_unknown_command(&text) {
        return ProbedStatus {
            supported: false,
            diagnostic: Some(
                "当前 Codex CLI 版本没有 login status 子命令，请升级后重试".to_owned(),
            ),
            ..ProbedStatus::default()
        };
    }
    if lowercase.contains("not logged in") {
        return ProbedStatus {
            supported: true,
            logged_in: Some(false),
            ..ProbedStatus::default()
        };
    }
    if exit_success && lowercase.contains("logged in") {
        let auth_method = if lowercase.contains("chatgpt") {
            Some("chatgpt".to_owned())
        } else if lowercase.contains("api key") {
            Some("api_key".to_owned())
        } else {
            None
        };
        return ProbedStatus {
            supported: true,
            logged_in: Some(true),
            auth_method,
            ..ProbedStatus::default()
        };
    }
    ProbedStatus {
        supported: true,
        logged_in: None,
        diagnostic: diagnostic_from_text(&text, redactor),
        ..ProbedStatus::default()
    }
}

fn mentions_unknown_command(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    lowercase.contains("unknown command")
        || lowercase.contains("unrecognized subcommand")
        || lowercase.contains("unexpected argument")
        || lowercase.contains("invalid subcommand")
}

const fn command_failure_text(failure: CommandFailure) -> &'static str {
    match failure {
        CommandFailure::Spawn => "无法启动官方 CLI 进行状态探测",
        CommandFailure::MissingPipe | CommandFailure::Output => "官方 CLI 的状态输出无法安全读取",
        CommandFailure::Wait => "等待官方 CLI 退出时失败",
        CommandFailure::Timeout => "官方 CLI 状态探测超时",
    }
}

fn diagnostic_from_output(output: &[u8], redactor: &SecretRedactor) -> Option<String> {
    diagnostic_from_text(&String::from_utf8_lossy(output), redactor)
}

/// 从登录子进程输出里取出第一个 https 授权地址（两个 CLI 都会把它打印到 stdout）。
/// 地址只含 PKCE challenge 与 state 等公开参数，不含任何凭据。
fn login_url_from_output(output: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(output);
    let start = text.find("https://")?;
    let url = text[start..]
        .split(|character: char| character.is_whitespace() || matches!(character, '"' | '\''))
        .next()?
        .trim_end_matches(['.', ',', ';', ')', ']']);
    (url.len() > "https://".len() && url.len() <= 2048).then(|| url.to_owned())
}

/// 取输出末尾若干字符，去掉控制字符并脱敏；空输出返回 `None`。
fn diagnostic_from_text(text: &str, redactor: &SecretRedactor) -> Option<String> {
    let cleaned = text
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect::<String>();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tail_start = trimmed
        .char_indices()
        .rev()
        .nth(DIAGNOSTIC_MAX_CHARS.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    let tail = trimmed[tail_start..].trim();
    Some(
        tail.lines()
            .map(|line| redactor.redact_text(line.trim_end()))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// 登录上下文中的目录必须是绝对路径，搜索路径不能为空。
pub fn validate_context(context: &OfficialLoginContext) -> Result<(), AppError> {
    for (name, path) in [
        ("home", context.home.as_path()),
        ("claudeConfigDir", context.claude_config_dir.as_path()),
        ("codexHome", context.codex_home.as_path()),
    ] {
        if !path.is_absolute() {
            return Err(AppError::invalid_input(
                name,
                "官方登录上下文路径必须是绝对路径",
            ));
        }
    }
    if context.search_path.as_os_str() == OsStr::new("") {
        return Err(AppError::invalid_input(
            "searchPath",
            "官方登录需要非空的可执行文件搜索路径",
        ));
    }
    Ok(())
}

#[cfg(test)]
include!("tests.rs");
