//! Release 启动边界的只读 Claude/Codex/Cursor/ZCode/OpenCode 安装与 Claude 策略探针。

use std::{
    ffi::{CString, OsStr, OsString},
    fs::{self, File},
    io::{self, Read},
    os::fd::{AsRawFd, FromRawFd},
    os::unix::ffi::OsStrExt,
    os::unix::fs::PermissionsExt,
    os::unix::process::CommandExt,
    path::{Component, Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::{
    adapters::{
        ExplicitEnvironment, ToolAvailability, ToolAvailabilityState,
        VerifiedClaudeCustomizationPolicyEvidence, VerifiedClaudeUserMcpEvidence,
    },
    domain::Tool,
    error::AppError,
};

pub const DEFAULT_TOOL_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
pub const CLAUDE_MANAGED_SETTINGS_PATH: &str =
    "/Library/Application Support/ClaudeCode/managed-settings.json";
pub const CLAUDE_MANAGED_SETTINGS_DIRECTORY: &str =
    "/Library/Application Support/ClaudeCode/managed-settings.d";
pub const CURSOR_BUNDLE_ID: &str = "com.todesktop.230313mzl4w4u92";
pub const ZCODE_BUNDLE_ID: &str = "dev.zcode.app";

const MAX_PROCESS_OUTPUT_BYTES: u64 = 1024;
const MAX_POLICY_BYTES: u64 = 64 * 1024;
const MAX_PLIST_BYTES: u64 = 1024 * 1024;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseToolProbeInput {
    home: PathBuf,
    claude_config_dir: Option<PathBuf>,
    codex_home: Option<PathBuf>,
    opencode_config_dir: Option<PathBuf>,
    opencode_config_path: Option<PathBuf>,
    opencode_config_content: Option<String>,
    opencode_disabled: bool,
    search_path: OsString,
    timeout: Duration,
    claude_managed_settings_path: PathBuf,
    claude_managed_settings_directory: PathBuf,
    cursor_app_paths: Vec<PathBuf>,
    zcode_app_paths: Vec<PathBuf>,
}

impl ReleaseToolProbeInput {
    pub fn for_macos_release(
        home: PathBuf,
        claude_config_dir: Option<PathBuf>,
        codex_home: Option<PathBuf>,
        search_path: OsString,
    ) -> Self {
        let search_path = macos_release_search_path(&home, search_path);
        let cursor_app_paths = vec![
            PathBuf::from("/Applications/Cursor.app"),
            home.join("Applications/Cursor.app"),
        ];
        let zcode_app_paths = vec![
            PathBuf::from("/Applications/ZCode.app"),
            home.join("Applications/ZCode.app"),
        ];
        Self {
            home,
            claude_config_dir,
            codex_home,
            opencode_config_dir: None,
            opencode_config_path: None,
            opencode_config_content: None,
            opencode_disabled: false,
            search_path,
            timeout: DEFAULT_TOOL_PROBE_TIMEOUT,
            claude_managed_settings_path: PathBuf::from(CLAUDE_MANAGED_SETTINGS_PATH),
            claude_managed_settings_directory: PathBuf::from(CLAUDE_MANAGED_SETTINGS_DIRECTORY),
            cursor_app_paths,
            zcode_app_paths,
        }
    }

    pub fn with_opencode_config_dir(mut self, path: Option<PathBuf>) -> Self {
        self.opencode_config_dir = path;
        self
    }

    pub fn with_opencode_config_path(mut self, path: Option<PathBuf>) -> Self {
        self.opencode_config_path = path;
        self
    }

    pub fn with_opencode_config_content(mut self, content: Option<String>) -> Self {
        self.opencode_config_content = content;
        self
    }

    pub fn with_opencode_disabled(mut self, disabled: bool) -> Self {
        self.opencode_disabled = disabled;
        self
    }
}

/// PATH 中存在被跳过的不安全条目（相对路径、`.`、不可读目录或同名非文件）；
/// 探测仍在其余安全条目中完成，但用户应知道这些位置没有被搜索。
pub const PROBE_DIAGNOSTIC_SKIPPED_PATH_ENTRIES: &str = "INSTALLATION_PROBE_SKIPPED_PATH_ENTRIES";
/// PATH 为空或没有任何安全的绝对条目，探测无处可搜。
pub const PROBE_DIAGNOSTIC_NO_SAFE_PATH_ENTRIES: &str = "INSTALLATION_PROBE_NO_SAFE_PATH_ENTRIES";
/// 首个命中的候选文件自身不安全（无法解析、非普通文件或不可执行）。
pub const PROBE_DIAGNOSTIC_UNSAFE_CANDIDATE: &str = "INSTALLATION_PROBE_UNSAFE_CANDIDATE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProbeOutcome {
    pub state: ToolAvailabilityState,
    pub version: Option<String>,
    /// 稳定诊断码，解释为什么是这个状态；`None` 表示没有额外可说明的原因。
    pub diagnostic: Option<&'static str>,
}

impl ToolProbeOutcome {
    fn installed(version: String) -> Self {
        Self {
            state: ToolAvailabilityState::Installed,
            version: Some(version),
            diagnostic: None,
        }
    }

    fn unavailable() -> Self {
        Self {
            state: ToolAvailabilityState::Unavailable,
            version: None,
            diagnostic: None,
        }
    }

    fn unsupported() -> Self {
        Self {
            state: ToolAvailabilityState::Unsupported,
            version: None,
            diagnostic: None,
        }
    }

    fn with_diagnostic(mut self, diagnostic: Option<&'static str>) -> Self {
        self.diagnostic = diagnostic;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseToolProbeResult {
    pub environment: ExplicitEnvironment,
    pub claude: ToolProbeOutcome,
    pub codex: ToolProbeOutcome,
    pub cursor: ToolProbeOutcome,
    pub zcode: ToolProbeOutcome,
    pub opencode: ToolProbeOutcome,
}

pub fn probe_release_environment(
    input: &ReleaseToolProbeInput,
) -> Result<ReleaseToolProbeResult, AppError> {
    let path_environment = ExplicitEnvironment::new(
        &input.home,
        input.claude_config_dir.clone(),
        input.codex_home.clone(),
        ToolAvailability::all_unavailable(),
    )?;
    // 五个探测互不依赖，各自持有独立的进程组与超时；并行执行让总耗时
    // 取决于最慢的一个而不是五者之和（单个工具挂住 3 秒也不再拖累其它工具）。
    let (claude, codex, cursor, zcode, opencode) = std::thread::scope(|scope| {
        let claude = scope.spawn(|| probe_tool(ToolBinary::Claude, &path_environment, input));
        let codex = scope.spawn(|| probe_tool(ToolBinary::Codex, &path_environment, input));
        let cursor = scope.spawn(|| probe_cursor(&path_environment, input));
        let zcode = scope.spawn(|| probe_zcode(&path_environment, input));
        let opencode = scope.spawn(|| probe_tool(ToolBinary::Opencode, &path_environment, input));
        (
            join_probe(claude),
            join_probe(codex),
            join_probe(cursor),
            join_probe(zcode),
            join_probe(opencode),
        )
    });
    let availability = ToolAvailability::from_states([
        claude.state,
        codex.state,
        cursor.state,
        zcode.state,
        opencode.state,
    ]);
    let mut environment = ExplicitEnvironment::new(
        path_environment.home(),
        Some(path_environment.claude_config_dir().to_path_buf()),
        Some(path_environment.codex_home().to_path_buf()),
        availability,
    )?;
    if let Some(path) = input.opencode_config_dir.as_ref() {
        environment = environment.with_opencode_config_dir(path)?;
    }
    if let Some(path) = input.opencode_config_path.as_ref() {
        environment = environment.with_opencode_config_path(path)?;
    }
    environment = environment
        .with_opencode_config_content(input.opencode_config_content.clone())
        .with_opencode_disabled(input.opencode_disabled);

    if let Some(version) = claude.version.as_deref() {
        environment = environment.with_claude_installation_version(version)?;
        if environment.uses_default_claude_config_dir() {
            let evidence = VerifiedClaudeUserMcpEvidence::new(
                version,
                environment.claude_config_dir(),
                environment.home().join(".claude.json"),
            )?;
            environment = environment.with_claude_user_mcp_evidence(evidence);
        }
        if let Some(evidence) = probe_claude_policy(
            version,
            environment.claude_config_dir(),
            &input.claude_managed_settings_path,
            &input.claude_managed_settings_directory,
        ) {
            environment = environment.with_claude_customization_policy_evidence(evidence);
        }
    }
    if let Some(version) = codex.version.as_deref() {
        environment = environment.with_codex_installation_version(version)?;
    }
    if let Some(version) = cursor.version.as_deref() {
        environment = environment.with_cursor_installation_version(version)?;
    }
    if let Some(version) = zcode.version.as_deref() {
        environment = environment.with_zcode_installation_version(version)?;
    }
    if let Some(version) = opencode.version.as_deref() {
        environment = environment.with_opencode_installation_version(version)?;
    }
    for (tool, outcome) in [
        (Tool::Claude, &claude),
        (Tool::Codex, &codex),
        (Tool::Cursor, &cursor),
        (Tool::Zcode, &zcode),
        (Tool::Opencode, &opencode),
    ] {
        if let Some(diagnostic) = outcome.diagnostic {
            environment = environment.with_installation_probe_diagnostic(tool, diagnostic);
        }
    }

    Ok(ReleaseToolProbeResult {
        environment,
        claude,
        codex,
        cursor,
        zcode,
        opencode,
    })
}

/// 探测线程 panic 时按"不受支持"处理，而不是让整个启动探测崩溃。
fn join_probe(handle: std::thread::ScopedJoinHandle<'_, ToolProbeOutcome>) -> ToolProbeOutcome {
    handle
        .join()
        .unwrap_or_else(|_| ToolProbeOutcome::unsupported())
}

#[derive(Debug, Clone, Copy)]
enum ToolBinary {
    Claude,
    Codex,
    CursorAgent,
    Opencode,
}

impl ToolBinary {
    const fn executable_name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::CursorAgent => "agent",
            Self::Opencode => "opencode",
        }
    }

    fn parse_version(self, stdout: &[u8], stderr: &[u8]) -> Option<String> {
        if !stderr.is_empty() {
            return None;
        }
        let raw = std::str::from_utf8(stdout).ok()?;
        let output = raw
            .strip_suffix("\r\n")
            .or_else(|| raw.strip_suffix('\n'))
            .unwrap_or(raw);
        if output.is_empty() || output.trim() != output || output.lines().count() != 1 {
            return None;
        }
        let version = match self {
            Self::Claude => output.strip_suffix(" (Claude Code)")?,
            Self::Codex => output.strip_prefix("codex-cli ")?,
            Self::CursorAgent => output
                .strip_prefix("Cursor Agent ")
                .or_else(|| output.strip_prefix("cursor-agent "))
                .or_else(|| output.strip_prefix("agent "))
                .unwrap_or(output),
            Self::Opencode => output.strip_prefix("opencode ").unwrap_or(output),
        };
        match self {
            Self::Claude | Self::Codex | Self::Opencode => valid_semantic_version(version),
            Self::CursorAgent => valid_cursor_version(version),
        }
        .then(|| version.to_owned())
    }
}

fn probe_cursor(
    environment: &ExplicitEnvironment,
    input: &ReleaseToolProbeInput,
) -> ToolProbeOutcome {
    let desktop = probe_desktop_app(&input.cursor_app_paths, CURSOR_BUNDLE_ID);
    if desktop.state == ToolAvailabilityState::Installed {
        return desktop;
    }
    let cli = probe_tool(ToolBinary::CursorAgent, environment, input);
    match (desktop, cli) {
        (_, outcome) if outcome.state == ToolAvailabilityState::Installed => outcome,
        (outcome, _) if outcome.state == ToolAvailabilityState::Unsupported => outcome,
        (_, outcome) if outcome.state == ToolAvailabilityState::Unsupported => outcome,
        _ => ToolProbeOutcome::unavailable(),
    }
}

/// ZCode 只有桌面应用这一条官方安装合同；不存在受支持的 PATH CLI 探针。
fn probe_zcode(
    _environment: &ExplicitEnvironment,
    input: &ReleaseToolProbeInput,
) -> ToolProbeOutcome {
    probe_desktop_app(&input.zcode_app_paths, ZCODE_BUNDLE_ID)
}

fn probe_desktop_app(candidates: &[PathBuf], bundle_id: &str) -> ToolProbeOutcome {
    let mut found_unsafe = false;
    for app_path in candidates {
        match read_desktop_bundle_version(app_path, bundle_id) {
            Ok(Some(version)) => return ToolProbeOutcome::installed(version),
            Ok(None) => {}
            Err(()) => found_unsafe = true,
        }
    }
    if found_unsafe {
        ToolProbeOutcome::unsupported()
    } else {
        ToolProbeOutcome::unavailable()
    }
}

fn read_desktop_bundle_version(app_path: &Path, bundle_id: &str) -> Result<Option<String>, ()> {
    match open_absolute_nofollow(app_path, true) {
        SecureOpen::Missing => return Ok(None),
        SecureOpen::Unsafe => return Err(()),
        SecureOpen::Open(_) => {}
    }
    let plist_path = app_path.join("Contents/Info.plist");
    let file = match open_absolute_nofollow(&plist_path, false) {
        SecureOpen::Open(file) => file,
        SecureOpen::Missing | SecureOpen::Unsafe => return Err(()),
    };
    let metadata = file.metadata().map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > MAX_PLIST_BYTES {
        return Err(());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PLIST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_PLIST_BYTES {
        return Err(());
    }
    let value = plist::Value::from_reader(io::Cursor::new(bytes)).map_err(|_| ())?;
    let dictionary = value.as_dictionary().ok_or(())?;
    if dictionary
        .get("CFBundleIdentifier")
        .and_then(plist::Value::as_string)
        != Some(bundle_id)
    {
        return Err(());
    }
    let version = dictionary
        .get("CFBundleShortVersionString")
        .and_then(plist::Value::as_string)
        .or_else(|| {
            dictionary
                .get("CFBundleVersion")
                .and_then(plist::Value::as_string)
        })
        .filter(|value| valid_cursor_version(value))
        .ok_or(())?;
    Ok(Some(version.to_owned()))
}

fn probe_tool(
    tool: ToolBinary,
    environment: &ExplicitEnvironment,
    input: &ReleaseToolProbeInput,
) -> ToolProbeOutcome {
    let (executable, skipped_entries) =
        match resolve_executable(&input.search_path, tool.executable_name()) {
            ExecutableResolution::Found {
                path,
                skipped_entries,
            } => (path, skipped_entries),
            ExecutableResolution::Unavailable { skipped_entries } => {
                return ToolProbeOutcome::unavailable()
                    .with_diagnostic(skipped_entries_diagnostic(skipped_entries));
            }
            ExecutableResolution::Unsupported { reason } => {
                return ToolProbeOutcome::unsupported().with_diagnostic(Some(reason));
            }
        };
    let outcome = match run_version_command(&executable, environment, input) {
        Ok(output) if output.status.success() => tool
            .parse_version(&output.stdout, &output.stderr)
            .map_or_else(ToolProbeOutcome::unsupported, ToolProbeOutcome::installed),
        Ok(_) | Err(_) => ToolProbeOutcome::unsupported(),
    };
    outcome.with_diagnostic(skipped_entries_diagnostic(skipped_entries))
}

const fn skipped_entries_diagnostic(skipped_entries: usize) -> Option<&'static str> {
    if skipped_entries > 0 {
        Some(PROBE_DIAGNOSTIC_SKIPPED_PATH_ENTRIES)
    } else {
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ExecutableResolution {
    Found {
        path: PathBuf,
        skipped_entries: usize,
    },
    Unavailable {
        skipped_entries: usize,
    },
    Unsupported {
        reason: &'static str,
    },
}

/// 逐条目搜索 PATH。不安全的条目（相对路径、`.`/`..` 分量、不可读目录）和同名
/// 非文件条目只是被跳过并计数，不会让整体探测失败：桌面应用继承的 PATH 里
/// 出现 `.` 或 `./node_modules/.bin` 很常见，不应因此把所有工具判成
/// `Unsupported`。只有首个命中的候选文件自身不安全，或者根本没有任何安全条目
/// 可搜时，才返回 `Unsupported`。
fn resolve_executable(search_path: &OsStr, name: &str) -> ExecutableResolution {
    let mut skipped_entries = 0_usize;
    let mut searched_entries = 0_usize;
    for entry in std::env::split_paths(search_path) {
        if !is_safe_absolute_path(&entry) {
            skipped_entries += 1;
            continue;
        }
        let candidate = entry.join(name);
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                searched_entries += 1;
                continue;
            }
            Err(_) => {
                skipped_entries += 1;
                continue;
            }
        };
        if !(metadata.file_type().is_file() || metadata.file_type().is_symlink()) {
            // 同名目录或特殊文件不是可执行候选；继续搜索后面的条目。
            skipped_entries += 1;
            continue;
        }
        // 首个命中的候选必须自身安全，否则视为不受支持而不是继续找下一个：
        // 用户明确把它放在 PATH 前面，静默越过它会执行意料之外的二进制。
        let canonical = match fs::canonicalize(&candidate) {
            Ok(path) => path,
            Err(_) => {
                return ExecutableResolution::Unsupported {
                    reason: PROBE_DIAGNOSTIC_UNSAFE_CANDIDATE,
                }
            }
        };
        let metadata = match fs::metadata(&canonical) {
            Ok(metadata) => metadata,
            Err(_) => {
                return ExecutableResolution::Unsupported {
                    reason: PROBE_DIAGNOSTIC_UNSAFE_CANDIDATE,
                }
            }
        };
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return ExecutableResolution::Unsupported {
                reason: PROBE_DIAGNOSTIC_UNSAFE_CANDIDATE,
            };
        }
        return ExecutableResolution::Found {
            path: candidate,
            skipped_entries,
        };
    }
    if searched_entries == 0 {
        return ExecutableResolution::Unsupported {
            reason: PROBE_DIAGNOSTIC_NO_SAFE_PATH_ENTRIES,
        };
    }
    ExecutableResolution::Unavailable { skipped_entries }
}

fn macos_release_search_path(home: &Path, search_path: OsString) -> OsString {
    let mut entries = if search_path.as_os_str().as_bytes().is_empty() {
        Vec::new()
    } else {
        std::env::split_paths(&search_path).collect::<Vec<_>>()
    };
    append_search_path_once(&mut entries, home.join(".volta").join("bin"));
    std::env::join_paths(entries).unwrap_or(search_path)
}

fn append_search_path_once(entries: &mut Vec<PathBuf>, path: PathBuf) {
    if is_safe_absolute_path(&path) && !entries.iter().any(|entry| entry == &path) {
        entries.push(path);
    }
}

fn is_safe_absolute_path(path: &Path) -> bool {
    path.is_absolute()
        && path != Path::new("/")
        && !path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
}

struct CommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandFailure {
    Spawn,
    MissingPipe,
    Wait,
    Timeout,
    Output,
}

fn run_version_command(
    executable: &Path,
    environment: &ExplicitEnvironment,
    input: &ReleaseToolProbeInput,
) -> Result<CommandOutput, CommandFailure> {
    let mut command = Command::new(executable);
    Command::arg(&mut command, "--version");
    Command::current_dir(&mut command, environment.home());
    Command::env_clear(&mut command);
    CommandExt::process_group(&mut command, 0);
    for (name, value) in [
        (OsStr::new("HOME"), environment.home().as_os_str()),
        (
            OsStr::new("CLAUDE_CONFIG_DIR"),
            environment.claude_config_dir().as_os_str(),
        ),
        (
            OsStr::new("CODEX_HOME"),
            environment.codex_home().as_os_str(),
        ),
        (OsStr::new("PATH"), input.search_path.as_os_str()),
        (OsStr::new("CI"), OsStr::new("1")),
        (OsStr::new("NO_COLOR"), OsStr::new("1")),
        (OsStr::new("TERM"), OsStr::new("dumb")),
        (OsStr::new("DISABLE_AUTOUPDATER"), OsStr::new("1")),
        (
            OsStr::new("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"),
            OsStr::new("1"),
        ),
    ] {
        Command::env(&mut command, name, value);
    }
    Command::stdin(&mut command, Stdio::null());
    Command::stdout(&mut command, Stdio::piped());
    Command::stderr(&mut command, Stdio::piped());
    let mut child = command.spawn().map_err(|_| CommandFailure::Spawn)?;
    let mut stdout = child.stdout.take().ok_or(CommandFailure::MissingPipe)?;
    let mut stderr = child.stderr.take().ok_or(CommandFailure::MissingPipe)?;
    set_nonblocking(&stdout)?;
    set_nonblocking(&stderr)?;
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut status = None;
    let mut group_terminated = false;
    let started = Instant::now();
    loop {
        if let Err(error) = drain_nonblocking(&mut stdout, &mut stdout_bytes, &mut stdout_closed)
            .and_then(|_| drain_nonblocking(&mut stderr, &mut stderr_bytes, &mut stderr_closed))
        {
            terminate_process_group(&mut child);
            let _ = child.wait();
            return Err(error);
        }
        if status.is_none() {
            status = child.try_wait().map_err(|_| CommandFailure::Wait)?;
        }
        if status.is_some() && !group_terminated {
            // 即使主进程已经退出，也终止同组后台后代，避免它们继续持有输出 pipe。
            terminate_process_group(&mut child);
            group_terminated = true;
        }
        if let Some(status) = status.filter(|_| stdout_closed && stderr_closed) {
            return Ok(CommandOutput {
                status,
                stdout: stdout_bytes,
                stderr: stderr_bytes,
            });
        }
        if started.elapsed() >= input.timeout {
            terminate_process_group(&mut child);
            let _ = child.wait();
            return Err(CommandFailure::Timeout);
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

fn set_nonblocking(file: &impl AsRawFd) -> Result<(), CommandFailure> {
    // SAFETY: file 在调用期间持有有效 fd；F_GETFL/F_SETFL 不接管描述符。
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(CommandFailure::Output);
    }
    // SAFETY: fd 与 flags 均来自上一步有效调用，只新增 O_NONBLOCK。
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(CommandFailure::Output);
    }
    Ok(())
}

fn drain_nonblocking(
    reader: &mut impl Read,
    output: &mut Vec<u8>,
    closed: &mut bool,
) -> Result<(), CommandFailure> {
    if *closed {
        return Ok(());
    }
    let mut buffer = [0_u8; 512];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                *closed = true;
                return Ok(());
            }
            Ok(read) => {
                output.extend_from_slice(&buffer[..read]);
                if output.len() as u64 > MAX_PROCESS_OUTPUT_BYTES {
                    return Err(CommandFailure::Output);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(CommandFailure::Output),
        }
    }
}

fn terminate_process_group(child: &mut std::process::Child) {
    if let Ok(process_group) = i32::try_from(child.id()) {
        // SAFETY: 负 pid 只定位由本进程为该 child 创建的独立进程组。
        unsafe {
            libc::kill(-process_group, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn valid_semantic_version(version: &str) -> bool {
    if version.is_empty()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return false;
    }
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn valid_cursor_version(version: &str) -> bool {
    version.len() <= 64 && valid_semantic_version(version)
}

fn probe_claude_policy(
    installation_version: &str,
    claude_config_dir: &Path,
    source_path: &Path,
    source_directory: &Path,
) -> Option<VerifiedClaudeCustomizationPolicyEvidence> {
    validate_official_policy_path_pair(source_path, source_directory)?;
    if managed_settings_directory_has_entries(source_directory)? {
        return None;
    }
    match read_managed_settings(source_path) {
        ManagedSettingsRead::Missing => {
            VerifiedClaudeCustomizationPolicyEvidence::from_official_source(
                installation_version,
                claude_config_dir,
                None,
                None,
            )
            .ok()
        }
        ManagedSettingsRead::Unsafe => None,
        ManagedSettingsRead::Document(document) => {
            let object = document.as_object()?;
            if object.contains_key("policyHelper") {
                return None;
            }
            VerifiedClaudeCustomizationPolicyEvidence::from_official_source(
                installation_version,
                claude_config_dir,
                Some(source_path),
                object.get("strictPluginOnlyCustomization"),
            )
            .ok()
        }
    }
}

fn validate_official_policy_path_pair(source_path: &Path, source_directory: &Path) -> Option<()> {
    if !is_safe_absolute_path(source_path)
        || !is_safe_absolute_path(source_directory)
        || source_path.file_name()? != OsStr::new("managed-settings.json")
        || source_directory.file_name()? != OsStr::new("managed-settings.d")
        || source_path.parent()? != source_directory.parent()?
    {
        return None;
    }
    Some(())
}

enum SecureOpen {
    Open(File),
    Missing,
    Unsafe,
}

fn open_absolute_nofollow(path: &Path, final_directory: bool) -> SecureOpen {
    if !is_safe_absolute_path(path) {
        return SecureOpen::Unsafe;
    }
    let root = match open_root_directory() {
        Some(root) => root,
        None => return SecureOpen::Unsafe,
    };
    let segments = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment),
            Component::RootDir => None,
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => None,
        })
        .collect::<Vec<_>>();
    let mut current = root;
    for (index, segment) in segments.iter().enumerate() {
        let is_last = index + 1 == segments.len();
        let segment = match CString::new(segment.as_bytes()) {
            Ok(segment) => segment,
            Err(_) => return SecureOpen::Unsafe,
        };
        let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        if !is_last || final_directory {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: current fd 有效；segment 是单个 NUL 结尾路径段；返回 fd 立即交给 File。
        let descriptor = unsafe { libc::openat(current.as_raw_fd(), segment.as_ptr(), flags) };
        if descriptor < 0 {
            return if io::Error::last_os_error().kind() == io::ErrorKind::NotFound {
                SecureOpen::Missing
            } else {
                SecureOpen::Unsafe
            };
        }
        // SAFETY: descriptor 是本函数刚取得且尚未被其他所有者接管的有效 fd。
        current = unsafe { File::from_raw_fd(descriptor) };
    }
    SecureOpen::Open(current)
}

fn open_root_directory() -> Option<File> {
    let root: &std::ffi::CStr = c"/";
    // SAFETY: root 是静态合法 C 路径；返回 fd 立即交给 File。
    let descriptor = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    (descriptor >= 0).then(|| {
        // SAFETY: descriptor 已验证非负且尚未被其他所有者接管。
        unsafe { File::from_raw_fd(descriptor) }
    })
}

fn managed_settings_directory_has_entries(path: &Path) -> Option<bool> {
    match open_absolute_nofollow(path, true) {
        SecureOpen::Missing => Some(false),
        SecureOpen::Unsafe => None,
        SecureOpen::Open(directory) => directory_has_entries(&directory),
    }
}

fn directory_has_entries(directory: &File) -> Option<bool> {
    // SAFETY: directory fd 有效；dup 产生独立 fd，fdopendir 成功后由 closedir 接管。
    let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
    if duplicate < 0 {
        return None;
    }
    // SAFETY: duplicate 是有效目录 fd；成功时 ownership 转移给 stream。
    let stream = unsafe { libc::fdopendir(duplicate) };
    if stream.is_null() {
        // SAFETY: fdopendir 失败时 duplicate 仍由调用方负责关闭。
        unsafe {
            libc::close(duplicate);
        }
        return None;
    }
    let mut found = false;
    loop {
        clear_errno();
        // SAFETY: stream 在 closedir 前有效；readdir 返回的指针只在下次调用前读取。
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            if current_errno() != 0 {
                // SAFETY: stream 是 fdopendir 返回且尚未关闭的有效指针。
                unsafe {
                    libc::closedir(stream);
                }
                return None;
            }
            break;
        }
        // SAFETY: d_name 是 readdir 保证以 NUL 结尾的目录项名称。
        let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name != b"." && name != b".." {
            found = true;
            break;
        }
    }
    // SAFETY: stream 是 fdopendir 返回且尚未关闭的有效指针。
    let closed = unsafe { libc::closedir(stream) };
    (closed == 0).then_some(found)
}

#[cfg(target_os = "macos")]
fn errno_pointer() -> *mut libc::c_int {
    // SAFETY: __error 返回当前线程 errno 的有效指针。
    unsafe { libc::__error() }
}

#[cfg(not(target_os = "macos"))]
fn errno_pointer() -> *mut libc::c_int {
    // SAFETY: __errno_location 返回当前线程 errno 的有效指针。
    unsafe { libc::__errno_location() }
}

fn clear_errno() {
    // SAFETY: errno_pointer 指向当前线程可写 errno。
    unsafe { *errno_pointer() = 0 };
}

fn current_errno() -> libc::c_int {
    // SAFETY: errno_pointer 指向当前线程可读 errno。
    unsafe { *errno_pointer() }
}

enum ManagedSettingsRead {
    Document(Value),
    Missing,
    Unsafe,
}

fn read_managed_settings(path: &Path) -> ManagedSettingsRead {
    let mut file = match open_absolute_nofollow(path, false) {
        SecureOpen::Open(file) => file,
        SecureOpen::Missing => return ManagedSettingsRead::Missing,
        SecureOpen::Unsafe => return ManagedSettingsRead::Unsafe,
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return ManagedSettingsRead::Unsafe,
    };
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_POLICY_BYTES {
        return ManagedSettingsRead::Unsafe;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    if file
        .by_ref()
        .take(MAX_POLICY_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return ManagedSettingsRead::Unsafe;
    }
    if bytes.len() as u64 > MAX_POLICY_BYTES || bytes.len() as u64 != metadata.len() {
        return ManagedSettingsRead::Unsafe;
    }
    match serde_json::from_slice(&bytes) {
        Ok(document) => ManagedSettingsRead::Document(document),
        Err(_) => ManagedSettingsRead::Unsafe,
    }
}

#[cfg(test)]
include!("tool_probe_tests.rs");
