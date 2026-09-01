//! Release 启动边界的只读 Claude/Codex/Cursor 安装与 Claude 策略探针。

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
    error::AppError,
};

pub const DEFAULT_TOOL_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
pub const CLAUDE_MANAGED_SETTINGS_PATH: &str =
    "/Library/Application Support/ClaudeCode/managed-settings.json";
pub const CLAUDE_MANAGED_SETTINGS_DIRECTORY: &str =
    "/Library/Application Support/ClaudeCode/managed-settings.d";
pub const CURSOR_BUNDLE_ID: &str = "com.todesktop.230313mzl4w4u92";

const MAX_PROCESS_OUTPUT_BYTES: u64 = 1024;
const MAX_POLICY_BYTES: u64 = 64 * 1024;
const MAX_PLIST_BYTES: u64 = 1024 * 1024;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseToolProbeInput {
    home: PathBuf,
    claude_config_dir: Option<PathBuf>,
    codex_home: Option<PathBuf>,
    search_path: OsString,
    timeout: Duration,
    claude_managed_settings_path: PathBuf,
    claude_managed_settings_directory: PathBuf,
    cursor_app_paths: Vec<PathBuf>,
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
        Self {
            home,
            claude_config_dir,
            codex_home,
            search_path,
            timeout: DEFAULT_TOOL_PROBE_TIMEOUT,
            claude_managed_settings_path: PathBuf::from(CLAUDE_MANAGED_SETTINGS_PATH),
            claude_managed_settings_directory: PathBuf::from(CLAUDE_MANAGED_SETTINGS_DIRECTORY),
            cursor_app_paths,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProbeOutcome {
    pub state: ToolAvailabilityState,
    pub version: Option<String>,
}

impl ToolProbeOutcome {
    fn installed(version: String) -> Self {
        Self {
            state: ToolAvailabilityState::Installed,
            version: Some(version),
        }
    }

    fn unavailable() -> Self {
        Self {
            state: ToolAvailabilityState::Unavailable,
            version: None,
        }
    }

    fn unsupported() -> Self {
        Self {
            state: ToolAvailabilityState::Unsupported,
            version: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseToolProbeResult {
    pub environment: ExplicitEnvironment,
    pub claude: ToolProbeOutcome,
    pub codex: ToolProbeOutcome,
    pub cursor: ToolProbeOutcome,
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
    let claude = probe_tool(ToolBinary::Claude, &path_environment, input);
    let codex = probe_tool(ToolBinary::Codex, &path_environment, input);
    let cursor = probe_cursor(&path_environment, input);
    let availability = ToolAvailability {
        claude: claude.state,
        codex: codex.state,
        cursor: cursor.state,
    };
    let mut environment = ExplicitEnvironment::new(
        path_environment.home(),
        Some(path_environment.claude_config_dir().to_path_buf()),
        Some(path_environment.codex_home().to_path_buf()),
        availability,
    )?;

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

    Ok(ReleaseToolProbeResult {
        environment,
        claude,
        codex,
        cursor,
    })
}

#[derive(Debug, Clone, Copy)]
enum ToolBinary {
    Claude,
    Codex,
    CursorAgent,
}

impl ToolBinary {
    const fn executable_name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::CursorAgent => "agent",
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
        };
        match self {
            Self::Claude | Self::Codex => valid_semantic_version(version),
            Self::CursorAgent => valid_cursor_version(version),
        }
        .then(|| version.to_owned())
    }
}

fn probe_cursor(
    environment: &ExplicitEnvironment,
    input: &ReleaseToolProbeInput,
) -> ToolProbeOutcome {
    let desktop = probe_cursor_desktop(&input.cursor_app_paths);
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

fn probe_cursor_desktop(candidates: &[PathBuf]) -> ToolProbeOutcome {
    let mut found_unsafe = false;
    for app_path in candidates {
        match read_cursor_bundle_version(app_path) {
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

fn read_cursor_bundle_version(app_path: &Path) -> Result<Option<String>, ()> {
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
        != Some(CURSOR_BUNDLE_ID)
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
    let executable = match resolve_executable(&input.search_path, tool.executable_name()) {
        ExecutableResolution::Found(path) => path,
        ExecutableResolution::Unavailable => return ToolProbeOutcome::unavailable(),
        ExecutableResolution::Unsupported => return ToolProbeOutcome::unsupported(),
    };
    match run_version_command(&executable, environment, input) {
        Ok(output) if output.status.success() => tool
            .parse_version(&output.stdout, &output.stderr)
            .map_or_else(ToolProbeOutcome::unsupported, ToolProbeOutcome::installed),
        Ok(_) | Err(_) => ToolProbeOutcome::unsupported(),
    }
}

enum ExecutableResolution {
    Found(PathBuf),
    Unavailable,
    Unsupported,
}

fn resolve_executable(search_path: &OsStr, name: &str) -> ExecutableResolution {
    let entries = std::env::split_paths(search_path).collect::<Vec<_>>();
    if entries.is_empty() || entries.iter().any(|entry| !is_safe_absolute_path(entry)) {
        return ExecutableResolution::Unsupported;
    }
    for entry in entries {
        let candidate = entry.join(name);
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return ExecutableResolution::Unsupported,
        };
        if !(metadata.file_type().is_file() || metadata.file_type().is_symlink()) {
            return ExecutableResolution::Unsupported;
        }
        let canonical = match fs::canonicalize(&candidate) {
            Ok(path) => path,
            Err(_) => return ExecutableResolution::Unsupported,
        };
        let metadata = match fs::metadata(&canonical) {
            Ok(metadata) => metadata,
            Err(_) => return ExecutableResolution::Unsupported,
        };
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return ExecutableResolution::Unsupported;
        }
        return ExecutableResolution::Found(candidate);
    }
    ExecutableResolution::Unavailable
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
    let root = CString::new("/").expect("根路径不含 NUL");
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
mod tests {
    use std::{
        ffi::{CString, OsString},
        fs,
        os::unix::{
            ffi::OsStrExt,
            fs::{symlink, PermissionsExt},
        },
        path::{Path, PathBuf},
        sync::{Mutex, MutexGuard},
        time::Duration,
    };

    use tempfile::tempdir;

    use crate::{
        adapters::{
            claude::ClaudeAdapter, codex::CodexAdapter, CapabilityState, ClaudeCustomizationPolicy,
            ClaudeCustomizationPolicyProbeInput, DiscoveryContext, PolicyState, ToolAdapter,
            ToolAvailabilityState,
        },
        domain::{ArtifactKind, Scope},
    };

    use super::{probe_release_environment, ReleaseToolProbeInput, ReleaseToolProbeResult};

    // 这些用例会 fork 带独立进程组、后台后代和 pipe 的 shell fixture；并行运行会让
    // EOF/超时断言彼此干扰。release setup 本身只串行执行一次探针，因此测试也显式
    // 隔离这些进程 fixture，且不通过修改全局 PATH/HOME 来实现隔离。
    static PROCESS_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

    fn isolate_process_fixture() -> MutexGuard<'static, ()> {
        PROCESS_FIXTURE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn version_outputs_are_strictly_parsed() {
        assert_eq!(
            super::ToolBinary::Claude.parse_version(b"2.1.217 (Claude Code)", b""),
            Some("2.1.217".to_owned())
        );
        assert_eq!(
            super::ToolBinary::Codex.parse_version(b"codex-cli 0.114.0", b""),
            Some("0.114.0".to_owned())
        );
        assert_eq!(
            super::ToolBinary::CursorAgent.parse_version(b"Cursor Agent 1.7.54", b""),
            Some("1.7.54".to_owned())
        );
        assert_eq!(
            super::ToolBinary::CursorAgent.parse_version(b"Cursor Agent 1.preview", b""),
            None
        );
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        home: PathBuf,
        bin: PathBuf,
        claude_root: PathBuf,
        codex_root: PathBuf,
        policy: PathBuf,
        policy_directory: PathBuf,
        hold_fifo: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let home = root.join("home");
            let bin = root.join("bin");
            let claude_root = home.join(".claude");
            let codex_root = home.join(".codex");
            let policy_root = root.join("official-managed-settings");
            let policy = policy_root.join("managed-settings.json");
            let policy_directory = policy_root.join("managed-settings.d");
            let hold_fifo = root.join("hold-open.fifo");
            for directory in [
                &home,
                &bin,
                &claude_root,
                &codex_root,
                &policy_root,
                &policy_directory,
            ] {
                fs::create_dir_all(directory).unwrap();
            }
            let hold_fifo_path = CString::new(hold_fifo.as_os_str().as_bytes()).unwrap();
            // SAFETY: 路径来自隔离 tempfile 且以 NUL 结尾；fixture 负责其生命周期。
            assert_eq!(unsafe { libc::mkfifo(hold_fifo_path.as_ptr(), 0o600) }, 0);
            Self {
                _temporary: temporary,
                home,
                bin,
                claude_root,
                codex_root,
                policy,
                policy_directory,
                hold_fifo,
            }
        }

        fn write_tool(&self, name: &str, body: &str) {
            write_executable(&self.bin.join(name), body);
        }

        fn write_cursor_app(&self, bundle_id: &str, version: &str) -> PathBuf {
            let app = self.home.join("Applications/Cursor.app");
            fs::create_dir_all(app.join("Contents")).unwrap();
            fs::write(
                app.join("Contents/Info.plist"),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{bundle_id}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
</dict></plist>"#
                ),
            )
            .unwrap();
            app
        }

        fn input(&self) -> ReleaseToolProbeInput {
            ReleaseToolProbeInput {
                home: self.home.clone(),
                claude_config_dir: Some(self.claude_root.clone()),
                codex_home: Some(self.codex_root.clone()),
                search_path: self.bin.clone().into_os_string(),
                timeout: Duration::from_secs(3),
                claude_managed_settings_path: self.policy.clone(),
                claude_managed_settings_directory: self.policy_directory.clone(),
                cursor_app_paths: vec![self.home.join("Applications/Cursor.app")],
            }
        }

        fn pipe_holding_descendant(&self, parent_action: &str) -> String {
            let fifo = self.hold_fifo.to_str().unwrap();
            assert!(!fifo.contains('\''));
            format!(
                "if [ \"$1\" = child ]; then read ignored < '{fifo}'; fi\n\"$0\" child &\n{parent_action}"
            )
        }
    }

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn current_customization_policy(result: &ReleaseToolProbeResult) -> ClaudeCustomizationPolicy {
        result
            .environment
            .claude_customization_policy_probe()
            .probe(&ClaudeCustomizationPolicyProbeInput {
                installation_version: result.environment.claude_installation_version(),
                claude_config_dir: result.environment.claude_config_dir(),
                source_path: result.environment.claude_customization_policy_source_path(),
                tool_installed: result.claude.state == ToolAvailabilityState::Installed,
            })
    }

    fn assert_release_policy_unknown(input: &ReleaseToolProbeInput) {
        let result = probe_release_environment(input).unwrap();
        assert_eq!(
            current_customization_policy(&result),
            ClaudeCustomizationPolicy::unknown()
        );
    }

    #[test]
    fn installed_versions_and_explicit_allowed_policy_are_bound_once() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        fs::write(
            &fixture.policy,
            r#"{"strictPluginOnlyCustomization":false}"#,
        )
        .unwrap();

        let input = fixture.input();
        let environment = crate::adapters::ExplicitEnvironment::new(
            &fixture.home,
            Some(fixture.claude_root.clone()),
            Some(fixture.codex_root.clone()),
            crate::adapters::ToolAvailability::all_unavailable(),
        )
        .unwrap();
        let executable = match super::resolve_executable(&input.search_path, "claude") {
            super::ExecutableResolution::Found(path) => path,
            _ => panic!("fake Claude 可执行文件未被安全解析"),
        };
        let output = super::run_version_command(&executable, &environment, &input).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"2.1.217 (Claude Code)");
        assert!(output.stderr.is_empty());

        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Installed);
        assert_eq!(result.claude.version.as_deref(), Some("2.1.217"));
        assert_eq!(result.codex.state, ToolAvailabilityState::Installed);
        assert_eq!(result.codex.version.as_deref(), Some("0.114.0"));
        assert_eq!(
            result.environment.codex_installation_version(),
            Some("0.114.0")
        );
        assert_eq!(
            result.environment.claude_customization_policy_source_path(),
            Some(fixture.policy.as_path())
        );

        let context = DiscoveryContext {
            environment: &result.environment,
            project_root: None,
            claude_user_mcp_probe: result.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: result
                .environment
                .claude_customization_policy_probe(),
        };
        let targets = ClaudeAdapter.discover(&context).unwrap();
        let mcp = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        let skill = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Skill)
            .unwrap();
        assert_eq!(mcp.capability.state, CapabilityState::Supported);
        assert_eq!(
            mcp.path.as_deref(),
            fixture.home.join(".claude.json").to_str()
        );
        assert_eq!(mcp.policy, PolicyState::Allowed);
        assert_eq!(skill.policy, PolicyState::Allowed);
        assert_eq!(
            result
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.218"),
                    claude_config_dir: result.environment.claude_config_dir(),
                    source_path: result.environment.claude_customization_policy_source_path(),
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            result
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: result.environment.claude_config_dir(),
                    source_path: None,
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            result
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: &fixture.codex_root,
                    source_path: result.environment.claude_customization_policy_source_path(),
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
    }

    #[test]
    fn cursor_desktop_bundle_is_primary_and_does_not_require_agent_cli() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.7.54");
        let marker = fixture.home.join("agent-was-executed");
        let marker_text = marker.to_str().unwrap();
        assert!(!marker_text.contains('\''));
        fixture.write_tool(
            "agent",
            &format!("touch '{marker_text}'\nprintf 'Cursor Agent 9.9.9'"),
        );

        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Installed);
        assert_eq!(result.cursor.version.as_deref(), Some("1.7.54"));
        assert!(!marker.exists(), "Desktop 已确认时不应再执行补充 CLI");
        assert_eq!(
            result.environment.cursor_installation_version(),
            Some("1.7.54")
        );
    }

    #[test]
    fn cursor_agent_is_only_a_fallback_and_invalid_bundle_fails_closed() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("agent", "printf 'Cursor Agent 2.3.4'");
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Installed);
        assert_eq!(result.cursor.version.as_deref(), Some("2.3.4"));

        let invalid = Fixture::new();
        invalid.write_cursor_app("com.example.not-cursor", "1.7.54");
        let result = probe_release_environment(&invalid.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);
        assert!(result.cursor.version.is_none());
    }

    #[test]
    fn cursor_desktop_rejects_malformed_versions_symlinked_apps_and_oversized_plists() {
        let _process_fixture = isolate_process_fixture();

        let malformed = Fixture::new();
        malformed.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.preview");
        let result = probe_release_environment(&malformed.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);

        let linked = Fixture::new();
        let app = linked.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.7.54");
        let real_app = linked.home.join("Applications/Cursor-real.app");
        fs::rename(&app, &real_app).unwrap();
        symlink(&real_app, &app).unwrap();
        let result = probe_release_environment(&linked.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);

        let oversized = Fixture::new();
        let app = oversized.write_cursor_app(super::CURSOR_BUNDLE_ID, "1.7.54");
        fs::write(
            app.join("Contents/Info.plist"),
            vec![b'x'; super::MAX_PLIST_BYTES as usize + 1],
        )
        .unwrap();
        let result = probe_release_environment(&oversized.input()).unwrap();
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);
    }

    #[test]
    fn missing_or_unconfigured_policy_sources_are_allowed_and_evidence_stays_bound() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");

        let empty_directory = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(
            current_customization_policy(&empty_directory),
            ClaudeCustomizationPolicy {
                mcp: PolicyState::Allowed,
                skill: PolicyState::Allowed,
            }
        );
        assert!(empty_directory
            .environment
            .claude_customization_policy_source_path()
            .is_none());

        fs::remove_dir(&fixture.policy_directory).unwrap();
        let missing_directory = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(
            current_customization_policy(&missing_directory),
            ClaudeCustomizationPolicy {
                mcp: PolicyState::Allowed,
                skill: PolicyState::Allowed,
            }
        );
        assert_eq!(
            missing_directory
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.218"),
                    claude_config_dir: missing_directory.environment.claude_config_dir(),
                    source_path: None,
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            missing_directory
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: &fixture.codex_root,
                    source_path: None,
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );
        assert_eq!(
            missing_directory
                .environment
                .claude_customization_policy_probe()
                .probe(&ClaudeCustomizationPolicyProbeInput {
                    installation_version: Some("2.1.217"),
                    claude_config_dir: missing_directory.environment.claude_config_dir(),
                    source_path: Some(&fixture.policy),
                    tool_installed: true,
                }),
            ClaudeCustomizationPolicy::unknown()
        );

        fs::create_dir(&fixture.policy_directory).unwrap();
        fs::write(&fixture.policy, "{}").unwrap();
        let undeclared_setting = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(
            current_customization_policy(&undeclared_setting),
            ClaudeCustomizationPolicy {
                mcp: PolicyState::Allowed,
                skill: PolicyState::Allowed,
            }
        );
        assert_eq!(
            undeclared_setting
                .environment
                .claude_customization_policy_source_path(),
            Some(fixture.policy.as_path())
        );
    }

    #[test]
    fn explicit_policy_values_keep_surface_rules_while_dynamic_or_invalid_values_are_unknown() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");

        for (document, expected) in [
            (
                r#"{"strictPluginOnlyCustomization":false}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Allowed,
                    skill: PolicyState::Allowed,
                },
            ),
            (
                r#"{"strictPluginOnlyCustomization":true}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Blocked,
                    skill: PolicyState::Blocked,
                },
            ),
            (
                r#"{"strictPluginOnlyCustomization":["mcp"]}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Blocked,
                    skill: PolicyState::Allowed,
                },
            ),
            (
                r#"{"strictPluginOnlyCustomization":["skills"]}"#,
                ClaudeCustomizationPolicy {
                    mcp: PolicyState::Allowed,
                    skill: PolicyState::Blocked,
                },
            ),
        ] {
            fs::write(&fixture.policy, document).unwrap();
            let result = probe_release_environment(&fixture.input()).unwrap();
            assert_eq!(current_customization_policy(&result), expected);
        }

        for document in [
            r#"{"strictPluginOnlyCustomization":"mcp"}"#,
            r#"{"policyHelper":"/usr/local/bin/effective-policy"}"#,
        ] {
            fs::write(&fixture.policy, document).unwrap();
            assert_release_policy_unknown(&fixture.input());
        }
    }

    #[test]
    fn missing_tools_are_unavailable_without_running_host_commands() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unavailable);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unavailable);

        let context = DiscoveryContext {
            environment: &result.environment,
            project_root: None,
            claude_user_mcp_probe: result.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: result
                .environment
                .claude_customization_policy_probe(),
        };
        assert!(ClaudeAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .all(|target| target.capability.state == CapabilityState::ToolNotInstalled));
        assert!(CodexAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .all(|target| target.capability.state == CapabilityState::ToolNotInstalled));
    }

    #[test]
    fn macos_release_path_finds_volta_shims_without_shell_path_setup() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        let volta_bin = fixture.home.join(".volta/bin");
        fs::create_dir_all(&volta_bin).unwrap();
        let shim = fixture.home.join("volta-shim");
        write_executable(
            &shim,
            r#"case "${0##*/}" in
claude) printf '2.1.217 (Claude Code)' ;;
codex) printf 'codex-cli 0.114.0' ;;
*) printf 'direct shim execution is unsupported' >&2; exit 9 ;;
esac"#,
        );
        symlink(&shim, volta_bin.join("claude")).unwrap();
        symlink(&shim, volta_bin.join("codex")).unwrap();

        let input = ReleaseToolProbeInput::for_macos_release(
            fixture.home.clone(),
            Some(fixture.claude_root.clone()),
            Some(fixture.codex_root.clone()),
            OsString::new(),
        );
        assert_eq!(
            std::env::split_paths(&input.search_path).collect::<Vec<_>>(),
            vec![volta_bin]
        );

        let result = probe_release_environment(&input).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Installed);
        assert_eq!(result.claude.version.as_deref(), Some("2.1.217"));
        assert_eq!(result.codex.state, ToolAvailabilityState::Installed);
        assert_eq!(result.codex.version.as_deref(), Some("0.114.0"));
    }

    #[test]
    fn macos_release_path_keeps_existing_precedence_and_deduplicates_volta() {
        let fixture = Fixture::new();
        let volta_bin = fixture.home.join(".volta/bin");
        let input = ReleaseToolProbeInput::for_macos_release(
            fixture.home.clone(),
            Some(fixture.claude_root.clone()),
            Some(fixture.codex_root.clone()),
            std::env::join_paths([fixture.bin.clone(), volta_bin.clone()]).unwrap(),
        );

        assert_eq!(
            std::env::split_paths(&input.search_path).collect::<Vec<_>>(),
            vec![fixture.bin, volta_bin]
        );
    }

    #[test]
    fn timeout_and_malicious_output_are_unsupported() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", &fixture.pipe_holding_descendant("wait"));
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0\\nforged'");
        fixture.write_tool("agent", &fixture.pipe_holding_descendant("wait"));
        let mut input = fixture.input();
        input.timeout = Duration::from_millis(100);
        let started = std::time::Instant::now();
        let result = probe_release_environment(&input).unwrap();
        assert!(started.elapsed() < Duration::from_millis(800));
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.cursor.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.environment.claude_installation_version(), None);
        assert_eq!(result.environment.codex_installation_version(), None);
        assert_eq!(result.environment.cursor_installation_version(), None);
    }

    #[test]
    fn exited_wrapper_cannot_leave_a_pipe_holding_descendant() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", &fixture.pipe_holding_descendant("exit 0"));
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        let mut input = fixture.input();
        input.timeout = Duration::from_secs(5);

        let started = std::time::Instant::now();
        let result = probe_release_environment(&input).unwrap();

        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Installed);
    }

    #[test]
    fn nonzero_oversized_non_utf8_and_stderr_outputs_fail_closed() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "exit 7");
        fixture.write_tool(
            "codex",
            "i=0; while [ \"$i\" -lt 1100 ]; do printf x; i=$((i + 1)); done",
        );
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unsupported);

        fixture.write_tool("claude", "printf '\\377'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'; printf unexpected >&2");
        let result = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(result.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(result.codex.state, ToolAvailabilityState::Unsupported);
    }

    #[test]
    fn unsafe_search_path_and_unexpected_argv_never_become_installed() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool(
            "claude",
            "[ \"$#\" -eq 1 ] && [ \"$1\" = --version ] || exit 8\nread ignored && exit 9\nprintf '2.1.217 (Claude Code)'",
        );
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        let valid = probe_release_environment(&fixture.input()).unwrap();
        assert_eq!(valid.claude.state, ToolAvailabilityState::Installed);

        let mut unsafe_input = fixture.input();
        unsafe_input.search_path = OsString::from("relative-bin");
        let unsupported = probe_release_environment(&unsafe_input).unwrap();
        assert_eq!(unsupported.claude.state, ToolAvailabilityState::Unsupported);
        assert_eq!(unsupported.codex.state, ToolAvailabilityState::Unsupported);
    }

    #[test]
    fn policy_source_rejects_symlinked_ancestors_ambiguous_inputs_and_malformed_files() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        let valid_policy = r#"{"strictPluginOnlyCustomization":false}"#;
        fs::write(&fixture.policy, valid_policy).unwrap();

        let mut malformed = fixture.input();
        malformed.claude_managed_settings_path = fixture.policy.with_file_name("other.json");
        fs::write(&malformed.claude_managed_settings_path, valid_policy).unwrap();
        assert_release_policy_unknown(&malformed);

        fs::write(&fixture.policy, b"{invalid").unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::write(&fixture.policy, b"").unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::remove_file(&fixture.policy).unwrap();
        fs::create_dir(&fixture.policy).unwrap();
        assert_release_policy_unknown(&fixture.input());
        fs::remove_dir(&fixture.policy).unwrap();

        fs::write(
            &fixture.policy,
            vec![b'x'; super::MAX_POLICY_BYTES as usize + 1],
        )
        .unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::write(&fixture.policy, valid_policy).unwrap();
        fs::set_permissions(&fixture.policy, fs::Permissions::from_mode(0o000)).unwrap();
        assert_release_policy_unknown(&fixture.input());
        fs::set_permissions(&fixture.policy, fs::Permissions::from_mode(0o600)).unwrap();

        fs::write(&fixture.policy, valid_policy).unwrap();
        fs::write(fixture.policy_directory.join("10-policy.json"), "{}").unwrap();
        assert_release_policy_unknown(&fixture.input());

        fs::remove_file(fixture.policy_directory.join("10-policy.json")).unwrap();
        let real_parent = fixture.home.parent().unwrap().join("real-policy-parent");
        let alias_parent = fixture.home.parent().unwrap().join("policy-parent-alias");
        fs::create_dir(&real_parent).unwrap();
        fs::write(real_parent.join("managed-settings.json"), valid_policy).unwrap();
        fs::create_dir(real_parent.join("managed-settings.d")).unwrap();
        symlink(&real_parent, &alias_parent).unwrap();
        let mut symlinked = fixture.input();
        symlinked.claude_managed_settings_path = alias_parent.join("managed-settings.json");
        symlinked.claude_managed_settings_directory = alias_parent.join("managed-settings.d");
        assert_release_policy_unknown(&symlinked);
    }

    #[test]
    fn custom_root_blocks_user_mcp_while_policy_can_be_blocked_or_absent() {
        let _process_fixture = isolate_process_fixture();
        let fixture = Fixture::new();
        fixture.write_tool("claude", "printf '2.1.217 (Claude Code)'");
        fixture.write_tool("codex", "printf 'codex-cli 0.114.0'");
        fs::write(
            &fixture.policy,
            r#"{"strictPluginOnlyCustomization":["mcp"]}"#,
        )
        .unwrap();
        let custom_root = fixture.home.join("custom-claude");
        fs::create_dir(&custom_root).unwrap();
        let mut input = fixture.input();
        input.claude_config_dir = Some(custom_root);
        let blocked = probe_release_environment(&input).unwrap();
        let context = DiscoveryContext {
            environment: &blocked.environment,
            project_root: None,
            claude_user_mcp_probe: blocked.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: blocked
                .environment
                .claude_customization_policy_probe(),
        };
        let targets = ClaudeAdapter.discover(&context).unwrap();
        let mcp = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        let skill = targets
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Skill)
            .unwrap();
        assert_eq!(mcp.scope, Scope::Global);
        assert_eq!(mcp.capability.state, CapabilityState::Unsupported);
        assert_eq!(mcp.policy, PolicyState::Blocked);
        assert_eq!(skill.policy, PolicyState::Allowed);

        fs::remove_file(&fixture.policy).unwrap();
        let absent = probe_release_environment(&input).unwrap();
        let context = DiscoveryContext {
            environment: &absent.environment,
            project_root: None,
            claude_user_mcp_probe: absent.environment.claude_user_mcp_probe(),
            claude_customization_policy_probe: absent
                .environment
                .claude_customization_policy_probe(),
        };
        assert!(ClaudeAdapter
            .discover(&context)
            .unwrap()
            .iter()
            .filter(|target| {
                matches!(
                    target.artifact_kind,
                    ArtifactKind::Mcp | ArtifactKind::Skill
                )
            })
            .all(|target| target.policy == PolicyState::Allowed));
    }
}
