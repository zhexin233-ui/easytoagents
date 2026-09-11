/// Provider/Prompt 页面与引导服务有正式文件合同的工具；Cursor Provider 仍不
/// 支持，但 Prompt 已按官方规则文件合同接入（任务 09-06-cursor-prompt-support）。
/// MCP 与 Skills 的可分配工具集合。
pub const PROFILE_TOOLS: [Tool; 5] = Tool::ALL;
pub const ASSIGNABLE_MCP_TOOLS: [Tool; 5] = Tool::ALL;
pub const ASSIGNABLE_SKILL_TOOLS: [Tool; 5] = Tool::ALL;
/// Hooks 的可分配工具集合（四工具均有官方 hooks 合同，证据见任务 09-05-add-hooks-management）。
pub const ASSIGNABLE_HOOK_TOOLS: [Tool; 4] = [Tool::Claude, Tool::Codex, Tool::Cursor, Tool::Zcode];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TargetFormat {
    Json,
    /// OpenCode's JSON with comments/trailing commas. The parser keeps the
    /// original source so managed top-level replacements do not discard
    /// unrelated comments and keys.
    Jsonc,
    Toml,
    Markdown,
    /// Cursor 规则文件（`.cursor/rules/*.mdc`）：Markdown + 固定
    /// `alwaysApply: true` frontmatter。渲染时包装、观测时剥离，
    /// 受管投影域与其他工具的纯 Markdown 正文保持同构。
    CursorMdc,
    SymlinkDirectory,
}

impl TargetFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Jsonc => "jsonc",
            Self::Toml => "toml",
            Self::Markdown => "markdown",
            Self::CursorMdc => "cursor_mdc",
            Self::SymlinkDirectory => "symlink_directory",
        }
    }

    pub const fn expected_type(self) -> TargetType {
        match self {
            Self::Json | Self::Jsonc | Self::Toml | Self::Markdown | Self::CursorMdc => {
                TargetType::File
            }
            Self::SymlinkDirectory => TargetType::Directory,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Supported,
    Unsupported,
    ToolNotInstalled,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetCapability {
    pub state: CapabilityState,
    pub diagnostic_code: Option<String>,
}

impl TargetCapability {
    pub fn supported() -> Self {
        Self {
            state: CapabilityState::Supported,
            diagnostic_code: None,
        }
    }

    pub fn unsupported(code: &'static str) -> Self {
        Self {
            state: CapabilityState::Unsupported,
            diagnostic_code: Some(code.to_owned()),
        }
    }

    pub fn tool_not_installed() -> Self {
        Self {
            state: CapabilityState::ToolNotInstalled,
            diagnostic_code: Some("TOOL_NOT_INSTALLED".to_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PolicyState {
    Allowed,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudeCustomizationPolicy {
    pub mcp: PolicyState,
    pub skill: PolicyState,
}

impl ClaudeCustomizationPolicy {
    pub const fn unknown() -> Self {
        Self {
            mcp: PolicyState::Unknown,
            skill: PolicyState::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TargetTrustState {
    NotRequired,
    Trusted,
    Untrusted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PromptOverrideState {
    NotApplicable,
    NotPresent,
    Present,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SymlinkPolicy {
    Reject,
    ManagedChildrenOnly,
}

/// Adapter 对一个原生目标的完整只读合同。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TargetDescriptor {
    pub tool: Tool,
    pub artifact_kind: ArtifactKind,
    pub scope: Scope,
    pub project_root: Option<String>,
    pub path: Option<String>,
    /// 外部写入安全边界；Global 目标由 Adapter 显式提供，Project 目标通常等于 project_root。
    pub allowed_root: Option<String>,
    /// MCP 受管投影所在的原生容器路径。
    pub mcp_container: Option<Vec<String>>,
    pub format: TargetFormat,
    pub managed_selector_roots: Vec<String>,
    pub sensitive_selectors: Vec<String>,
    pub capability: TargetCapability,
    pub policy: PolicyState,
    pub trust: TargetTrustState,
    pub prompt_override: PromptOverrideState,
    pub symlink_policy: SymlinkPolicy,
}

/// `TargetDescriptor` 的统一链式构造器，确保所有 Adapter 使用同一组默认策略。
pub(crate) struct TargetDescriptorBuilder {
    descriptor: TargetDescriptor,
}

impl TargetDescriptorBuilder {
    pub(crate) fn project_root(mut self, project_root: Option<String>) -> Self {
        self.descriptor.project_root = project_root;
        self
    }

    pub(crate) fn path(mut self, path: Option<String>) -> Self {
        self.descriptor.path = path;
        self
    }

    pub(crate) fn allowed_root(mut self, allowed_root: Option<String>) -> Self {
        self.descriptor.allowed_root = allowed_root;
        self
    }

    pub(crate) fn format(mut self, format: TargetFormat) -> Self {
        self.descriptor.format = format;
        self
    }

    pub(crate) fn managed_selectors<I, S>(mut self, selectors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.descriptor.managed_selector_roots = selectors.into_iter().map(Into::into).collect();
        self
    }

    pub(crate) fn sensitive_selectors<I, S>(mut self, selectors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.descriptor.sensitive_selectors = selectors.into_iter().map(Into::into).collect();
        self
    }

    pub(crate) fn capability(mut self, capability: TargetCapability) -> Self {
        self.descriptor.capability = capability;
        self
    }

    pub(crate) fn policy(mut self, policy: PolicyState) -> Self {
        self.descriptor.policy = policy;
        self
    }

    pub(crate) fn trust(mut self, trust: TargetTrustState) -> Self {
        self.descriptor.trust = trust;
        self
    }

    pub(crate) fn prompt_override(mut self, prompt_override: PromptOverrideState) -> Self {
        self.descriptor.prompt_override = prompt_override;
        self
    }

    pub(crate) fn symlink_policy(mut self, symlink_policy: SymlinkPolicy) -> Self {
        self.descriptor.symlink_policy = symlink_policy;
        self
    }

    pub(crate) fn build(self) -> TargetDescriptor {
        let mut descriptor = self.descriptor;
        if descriptor.allowed_root.is_none() {
            descriptor.allowed_root = descriptor.project_root.clone();
        }
        descriptor
    }
}

impl TargetDescriptor {
    pub(crate) fn builder(
        tool: Tool,
        artifact_kind: ArtifactKind,
        scope: Scope,
    ) -> TargetDescriptorBuilder {
        TargetDescriptorBuilder {
            descriptor: Self {
                tool,
                artifact_kind,
                scope,
                project_root: None,
                path: None,
                allowed_root: None,
                mcp_container: (artifact_kind == ArtifactKind::Mcp).then(|| {
                    native_mcp_container(tool)
                        .iter()
                        .map(|segment| (*segment).to_owned())
                        .collect()
                }),
                format: TargetFormat::Json,
                managed_selector_roots: Vec::new(),
                sensitive_selectors: Vec::new(),
                capability: TargetCapability::supported(),
                policy: PolicyState::Allowed,
                trust: TargetTrustState::NotRequired,
                prompt_override: PromptOverrideState::NotApplicable,
                symlink_policy: SymlinkPolicy::Reject,
            },
        }
    }
}

impl TargetDescriptor {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref().map(Path::new)
    }

    fn path_for_error(&self) -> &str {
        self.path.as_deref().unwrap_or("<unsupported>")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ToolAvailabilityState {
    Installed,
    Unavailable,
    Unsupported,
}

impl ToolAvailabilityState {
    pub const fn is_installed(self) -> bool {
        matches!(self, Self::Installed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolAvailability([ToolAvailabilityState; 5]);

impl ToolAvailability {
    pub const fn from_states(states: [ToolAvailabilityState; 5]) -> Self {
        Self(states)
    }

    pub const fn get(self, tool: Tool) -> ToolAvailabilityState {
        self.0[tool_index(tool)]
    }
}

impl Index<Tool> for ToolAvailability {
    type Output = ToolAvailabilityState;

    fn index(&self, tool: Tool) -> &Self::Output {
        &self.0[tool_index(tool)]
    }
}

const fn tool_index(tool: Tool) -> usize {
    match tool {
        Tool::Claude => 0,
        Tool::Codex => 1,
        Tool::Cursor => 2,
        Tool::Zcode => 3,
        Tool::Opencode => 4,
    }
}

pub(crate) fn path_text(path: &Path) -> Result<String, AppError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::invalid_input("targetPath", "目标路径必须是 UTF-8"))
}

pub(crate) fn descriptor_path(descriptor: &TargetDescriptor) -> Result<String, AppError> {
    descriptor
        .path
        .clone()
        .ok_or_else(|| AppError::invalid_input("targetPath", "目标路径不可用"))
}

static CLAUDE_ADAPTER: claude::ClaudeAdapter = claude::ClaudeAdapter;
static CODEX_ADAPTER: codex::CodexAdapter = codex::CodexAdapter;
static CURSOR_ADAPTER: cursor::CursorAdapter = cursor::CursorAdapter;
static ZCODE_ADAPTER: zcode::ZcodeAdapter = zcode::ZcodeAdapter;
static OPENCODE_ADAPTER: opencode::OpencodeAdapter = opencode::OpencodeAdapter;

pub fn adapter_for(tool: Tool) -> &'static dyn ToolAdapter {
    match tool {
        Tool::Claude => &CLAUDE_ADAPTER,
        Tool::Codex => &CODEX_ADAPTER,
        Tool::Cursor => &CURSOR_ADAPTER,
        Tool::Zcode => &ZCODE_ADAPTER,
        Tool::Opencode => &OPENCODE_ADAPTER,
    }
}

/// 从受管投影中按 JSON 路径读取值；所有服务层使用同一条投影语义。
pub(crate) fn projection_value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

/// MCP 原生配置的容器路径。
pub(crate) fn native_mcp_container(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude | Tool::Cursor => &["mcpServers"],
        Tool::Codex => &["mcp_servers"],
        Tool::Zcode => &["mcp", "servers"],
        Tool::Opencode => &["mcp"],
    }
}

/// 为每个 Adapter 发现的目标填充外部写入安全边界。
///
/// Project 目标只能写入项目根；Global 目标使用工具实际的配置根。Claude 的
/// 用户级 MCP 文件例外地位于 home 下，因此显式保留这一边界，避免服务层
/// 再按工具复制一份判断。
pub(crate) fn populate_descriptor_allowed_roots(
    environment: &ExplicitEnvironment,
    descriptors: &mut [TargetDescriptor],
) -> Result<(), AppError> {
    for descriptor in descriptors {
        let root = match descriptor.scope {
            Scope::Project => descriptor
                .project_root
                .as_deref()
                .map(PathBuf::from)
                .ok_or_else(|| {
                    AppError::invalid_input("projectRoot", "项目目标缺少 project_root")
                })?,
            Scope::Global => match (descriptor.tool, descriptor.artifact_kind) {
                (Tool::Claude, ArtifactKind::Mcp) => environment.home().to_path_buf(),
                (Tool::Claude, _) => environment.claude_config_dir().to_path_buf(),
                (Tool::Codex, _) => environment.codex_home().to_path_buf(),
                (Tool::Cursor, _) => environment.home().join(".cursor"),
                (Tool::Zcode, _) => environment.home().join(".zcode"),
                (Tool::Opencode, ArtifactKind::Provider | ArtifactKind::Mcp) => {
                    environment.opencode_config_file_root()
                }
                (Tool::Opencode, _) => environment.opencode_config_dir().to_path_buf(),
            },
        };
        descriptor.allowed_root = Some(path_text(&root)?);
    }
    Ok(())
}

pub(crate) fn descriptor_allowed_root(descriptor: &TargetDescriptor) -> Result<PathBuf, AppError> {
    descriptor
        .allowed_root
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::invalid_input("allowedRoot", "目标缺少写入安全边界"))
}

pub(crate) fn descriptor_mcp_container(
    descriptor: &TargetDescriptor,
) -> Result<Vec<&str>, AppError> {
    descriptor
        .mcp_container
        .as_deref()
        .map(|segments| segments.iter().map(String::as_str).collect())
        .ok_or_else(|| AppError::invalid_input("mcpContainer", "MCP 目标缺少原生容器路径"))
}

pub(crate) fn find_descriptor(
    tool: Tool,
    context: &DiscoveryContext<'_>,
    artifact_kind: ArtifactKind,
    scope: Scope,
    not_found_field: &'static str,
    not_found_value: &str,
) -> Result<TargetDescriptor, AppError> {
    tool.adapter()
        .discover(context)?
        .into_iter()
        .find(|descriptor| descriptor.artifact_kind == artifact_kind && descriptor.scope == scope)
        .ok_or_else(|| AppError::not_found(not_found_field, not_found_value))
}

impl ToolAvailability {
    pub const fn all_installed() -> Self {
        Self([ToolAvailabilityState::Installed; 5])
    }

    pub const fn all_unavailable() -> Self {
        Self([ToolAvailabilityState::Unavailable; 5])
    }
}

/// 显式工具环境。构造时只验证调用方提供的路径，不读取进程环境。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitEnvironment {
    home: PathBuf,
    claude_config_dir: PathBuf,
    codex_home: PathBuf,
    opencode_config_dir: PathBuf,
    opencode_config_path: Option<PathBuf>,
    opencode_config_content: Option<String>,
    opencode_disabled: bool,
    uses_default_claude_config_dir: bool,
    installation_versions: [Option<String>; 5],
    claude_provider_policy: PolicyState,
    availability: ToolAvailability,
    /// 安装探针给出的稳定诊断码（例如 PATH 中有被跳过的不安全条目），供状态 DTO 透出。
    installation_probe_diagnostics: Vec<(Tool, &'static str)>,
    claude_user_mcp_evidence: Option<VerifiedClaudeUserMcpEvidence>,
    claude_customization_policy_evidence: Option<VerifiedClaudeCustomizationPolicyEvidence>,
}

impl ExplicitEnvironment {
    pub fn new(
        home: impl Into<PathBuf>,
        claude_config_dir: Option<PathBuf>,
        codex_home: Option<PathBuf>,
        availability: ToolAvailability,
    ) -> Result<Self, AppError> {
        let home = canonicalize_existing_directory(&home.into(), "home")?;
        let requested_claude_config_dir = claude_config_dir.unwrap_or_else(|| home.join(".claude"));
        let uses_default_claude_config_dir = requested_claude_config_dir == home.join(".claude");
        let requested_codex_home = codex_home.unwrap_or_else(|| home.join(".codex"));
        let requested_opencode_config_dir = home.join(".config/opencode");
        let claude_config_dir =
            normalize_config_root(&requested_claude_config_dir, "claudeConfigDir")?;
        let codex_home = normalize_config_root(&requested_codex_home, "codexHome")?;
        let opencode_config_dir =
            normalize_config_root(&requested_opencode_config_dir, "opencodeConfigDir")?;

        Ok(Self {
            home,
            claude_config_dir,
            codex_home,
            opencode_config_dir,
            opencode_config_path: None,
            opencode_config_content: None,
            opencode_disabled: false,
            uses_default_claude_config_dir,
            installation_versions: [None, None, None, None, None],
            claude_provider_policy: PolicyState::Unknown,
            availability,
            installation_probe_diagnostics: Vec::new(),
            claude_user_mcp_evidence: None,
            claude_customization_policy_evidence: None,
        })
    }

    pub fn with_installation_probe_diagnostic(
        mut self,
        tool: Tool,
        diagnostic: &'static str,
    ) -> Self {
        self.installation_probe_diagnostics
            .retain(|(existing, _)| *existing != tool);
        self.installation_probe_diagnostics.push((tool, diagnostic));
        self
    }

    pub fn installation_probe_diagnostic(&self, tool: Tool) -> Option<&'static str> {
        self.installation_probe_diagnostics
            .iter()
            .find(|(existing, _)| *existing == tool)
            .map(|(_, diagnostic)| *diagnostic)
    }

    fn with_installation_version(
        mut self,
        tool: Tool,
        version: impl Into<String>,
    ) -> Result<Self, AppError> {
        let version = version.into();
        if version.trim().is_empty() {
            let message = match tool {
                Tool::Claude => "Claude 安装版本不能为空",
                Tool::Codex => "Codex 安装版本不能为空",
                Tool::Cursor => "Cursor 安装版本不能为空",
                Tool::Zcode => "ZCode 安装版本不能为空",
                Tool::Opencode => "OpenCode 安装版本不能为空",
            };
            return Err(AppError::invalid_input("installationVersion", message));
        }
        self.installation_versions[tool_index(tool)] = Some(version);
        Ok(self)
    }

    pub fn with_claude_installation_version(
        self,
        version: impl Into<String>,
    ) -> Result<Self, AppError> {
        self.with_installation_version(Tool::Claude, version)
    }

    pub fn with_codex_installation_version(
        self,
        version: impl Into<String>,
    ) -> Result<Self, AppError> {
        self.with_installation_version(Tool::Codex, version)
    }

    pub fn with_cursor_installation_version(
        self,
        version: impl Into<String>,
    ) -> Result<Self, AppError> {
        self.with_installation_version(Tool::Cursor, version)
    }

    pub fn with_zcode_installation_version(
        self,
        version: impl Into<String>,
    ) -> Result<Self, AppError> {
        self.with_installation_version(Tool::Zcode, version)
    }

    /// Inject explicit OpenCode discovery overrides from the host boundary.
    /// Adapters never read `XDG_CONFIG_HOME`, `OPENCODE_CONFIG`, or process
    /// environment state themselves.
    pub fn with_opencode_config_dir(mut self, path: impl Into<PathBuf>) -> Result<Self, AppError> {
        self.opencode_config_dir = normalize_config_root(&path.into(), "opencodeConfigDir")?;
        Ok(self)
    }

    pub fn with_opencode_installation_version(
        self,
        version: impl Into<String>,
    ) -> Result<Self, AppError> {
        self.with_installation_version(Tool::Opencode, version)
    }

    pub fn with_opencode_config_path(mut self, path: impl Into<PathBuf>) -> Result<Self, AppError> {
        let path = path.into();
        self.opencode_config_path = Some(normalize_target_path(&path, "opencodeConfig")?);
        Ok(self)
    }

    pub fn with_opencode_config_content(mut self, content: Option<String>) -> Self {
        self.opencode_config_content = content;
        self
    }

    pub fn with_opencode_disabled(mut self, disabled: bool) -> Self {
        self.opencode_disabled = disabled;
        self
    }

    pub fn with_claude_user_mcp_evidence(
        mut self,
        evidence: VerifiedClaudeUserMcpEvidence,
    ) -> Self {
        self.claude_user_mcp_evidence = Some(evidence);
        self
    }

    pub fn with_claude_customization_policy_evidence(
        mut self,
        evidence: VerifiedClaudeCustomizationPolicyEvidence,
    ) -> Self {
        self.claude_customization_policy_evidence = Some(evidence);
        self
    }

    /// 宿主管理状态必须由运行时边界显式探测；未提供证据时保持 unknown。
    pub fn with_claude_provider_policy(mut self, policy: PolicyState) -> Self {
        self.claude_provider_policy = policy;
        self
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn claude_config_dir(&self) -> &Path {
        &self.claude_config_dir
    }

    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn opencode_config_dir(&self) -> &Path {
        &self.opencode_config_dir
    }

    pub fn opencode_config_path(&self) -> Option<&Path> {
        self.opencode_config_path.as_deref()
    }

    /// A custom `OPENCODE_CONFIG` file is a valid narrow write boundary for
    /// the Provider/MCP projection. Prompts and Skills remain rooted at the
    /// standard config directory and do not follow this single-file override.
    pub fn opencode_config_file_root(&self) -> PathBuf {
        self.opencode_config_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.opencode_config_dir.clone())
    }

    pub fn opencode_config_content(&self) -> Option<&str> {
        self.opencode_config_content.as_deref()
    }

    pub fn opencode_disabled(&self) -> bool {
        self.opencode_disabled
    }

    pub fn availability(&self) -> ToolAvailability {
        self.availability
    }

    pub const fn tool_availability(&self, tool: Tool) -> ToolAvailabilityState {
        self.availability.get(tool)
    }

    pub fn claude_installation_version(&self) -> Option<&str> {
        self.installation_version(Tool::Claude)
    }

    pub fn codex_installation_version(&self) -> Option<&str> {
        self.installation_version(Tool::Codex)
    }

    pub fn cursor_installation_version(&self) -> Option<&str> {
        self.installation_version(Tool::Cursor)
    }

    pub fn zcode_installation_version(&self) -> Option<&str> {
        self.installation_version(Tool::Zcode)
    }

    pub fn opencode_installation_version(&self) -> Option<&str> {
        self.installation_version(Tool::Opencode)
    }

    pub fn installation_version(&self, tool: Tool) -> Option<&str> {
        self.installation_versions[tool_index(tool)].as_deref()
    }

    pub fn claude_provider_policy(&self) -> PolicyState {
        self.claude_provider_policy
    }

    pub fn uses_default_claude_config_dir(&self) -> bool {
        self.uses_default_claude_config_dir
    }

    pub fn claude_user_mcp_probe(&self) -> &dyn ClaudeUserMcpCapabilityProbe {
        match self.claude_user_mcp_evidence.as_ref() {
            Some(evidence) => evidence,
            None => &ConservativeClaudeUserMcpProbe,
        }
    }

    pub fn claude_customization_policy_probe(&self) -> &dyn ClaudeCustomizationPolicyProbe {
        match self.claude_customization_policy_evidence.as_ref() {
            Some(evidence) => evidence,
            None => &ConservativeClaudeCustomizationPolicyProbe,
        }
    }

    pub fn claude_customization_policy_source_path(&self) -> Option<&Path> {
        self.claude_customization_policy_evidence
            .as_ref()
            .and_then(VerifiedClaudeCustomizationPolicyEvidence::source_path)
    }
}

/// 对已存在的登记目录做真实 canonicalization，拒绝文件、根目录和相对路径。
pub fn canonicalize_project_root(path: &Path) -> Result<ProjectRoot, AppError> {
    validate_absolute_normal_path(path, "rootPath")?;
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::not_found("project", &path.to_string_lossy()).with_source(error));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(
                AppError::permission(&path.to_string_lossy(), "lstat_project_root")
                    .with_source(error),
            );
        }
        Err(error) => {
            return Err(
                AppError::invalid_input("rootPath", "项目根无法安全读取").with_source(error)
            );
        }
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        let app_error = match error.kind() {
            std::io::ErrorKind::NotFound => AppError::not_found("project", &path.to_string_lossy()),
            std::io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), "canonicalize_project_root")
            }
            _ => AppError::invalid_input("rootPath", "项目根无法安全规范化"),
        };
        app_error.with_source(error)
    })?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|error| {
        let app_error = match error.kind() {
            std::io::ErrorKind::NotFound => AppError::not_found("project", &path.to_string_lossy()),
            std::io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), "lstat_canonical_project_root")
            }
            _ => AppError::invalid_input("rootPath", "规范化项目根无法安全读取"),
        };
        app_error.with_source(error)
    })?;
    if !metadata.is_dir() {
        return Err(AppError::invalid_input("rootPath", "项目根必须是目录"));
    }
    ProjectRoot::parse(&canonical)
}

fn canonicalize_existing_directory(path: &Path, field: &'static str) -> Result<PathBuf, AppError> {
    validate_absolute_normal_path(path, field)?;
    let canonical = fs::canonicalize(path).map_err(|error| {
        let app_error = match error.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::not_found("directory", &path.to_string_lossy())
            }
            std::io::ErrorKind::PermissionDenied => {
                AppError::permission(&path.to_string_lossy(), "canonicalize_directory")
            }
            _ => AppError::invalid_input(field, "目录无法安全规范化"),
        };
        app_error.with_source(error)
    })?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|error| {
        let app_error = match error.kind() {
            std::io::ErrorKind::PermissionDenied => {
                AppError::permission(&canonical.to_string_lossy(), "lstat_canonical_directory")
            }
            _ => AppError::not_found("directory", &canonical.to_string_lossy()),
        };
        app_error.with_source(error)
    })?;
    if !metadata.is_dir() {
        return Err(AppError::invalid_input(field, "路径必须是已存在目录"));
    }
    Ok(canonical)
}

fn normalize_config_root(path: &Path, field: &'static str) -> Result<PathBuf, AppError> {
    validate_absolute_normal_path(path, field)?;
    match fs::symlink_metadata(path) {
        Ok(_) => canonicalize_existing_directory(path, field),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            canonicalize_from_existing_ancestor(path, field)
        }
        Err(error) => Err(
            AppError::permission(&path.to_string_lossy(), "lstat_config_root").with_source(error),
        ),
    }
}

fn canonicalize_from_existing_ancestor(
    path: &Path,
    field: &'static str,
) -> Result<PathBuf, AppError> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = ancestor
                    .file_name()
                    .ok_or_else(|| AppError::invalid_input(field, "路径缺少可规范化的祖先目录"))?;
                missing.push(name.to_owned());
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| AppError::invalid_input(field, "路径缺少可规范化的祖先目录"))?;
            }
            Err(error) => {
                return Err(AppError::permission(
                    &ancestor.to_string_lossy(),
                    "lstat_config_ancestor",
                )
                .with_source(error));
            }
        }
    }
    let mut canonical = canonicalize_existing_directory(ancestor, field)?;
    for component in missing.iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

fn normalize_target_path(path: &Path, field: &'static str) -> Result<PathBuf, AppError> {
    validate_absolute_normal_path(path, field)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::invalid_input(field, "目标路径缺少文件名"))?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::invalid_input(field, "目标路径缺少父目录"))?;
    Ok(normalize_config_root(parent, field)?.join(file_name))
}

fn validate_absolute_normal_path(path: &Path, field: &'static str) -> Result<(), AppError> {
    if !path.is_absolute() || path == Path::new("/") {
        return Err(AppError::invalid_input(field, "路径必须是非根绝对路径"));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::CurDir | Component::ParentDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::invalid_input(field, "路径不能包含相对片段"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeUserMcpProbeInput<'a> {
    pub home: &'a Path,
    pub claude_config_dir: &'a Path,
    pub uses_default_config_dir: bool,
    pub installation_version: Option<&'a str>,
    pub tool_installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeUserMcpProbeResult {
    Supported(PathBuf),
    Unsupported(&'static str),
    ToolNotInstalled,
}

/// 非默认 Claude 配置根必须由安装版本探针提供证据，不能从目录名推断。
pub trait ClaudeUserMcpCapabilityProbe {
    fn probe(&self, input: &ClaudeUserMcpProbeInput<'_>) -> ClaudeUserMcpProbeResult;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudeCustomizationPolicyProbeInput<'a> {
    pub installation_version: Option<&'a str>,
    pub claude_config_dir: &'a Path,
    pub source_path: Option<&'a Path>,
    pub tool_installed: bool,
}

/// Claude 的有效管理策略可能来自服务端、MDM 或系统 managed settings。
/// Adapter 不读取用户 settings 猜测策略，必须消费调用方显式提供的探针证据。
pub trait ClaudeCustomizationPolicyProbe {
    fn probe(&self, input: &ClaudeCustomizationPolicyProbeInput<'_>) -> ClaudeCustomizationPolicy;
}

/// 没有有效策略证据时保持 unknown，确保 MCP/Skills 预览 fail closed。
#[derive(Debug, Default)]
pub struct ConservativeClaudeCustomizationPolicyProbe;

impl ClaudeCustomizationPolicyProbe for ConservativeClaudeCustomizationPolicyProbe {
    fn probe(&self, _input: &ClaudeCustomizationPolicyProbeInput<'_>) -> ClaudeCustomizationPolicy {
        ClaudeCustomizationPolicy::unknown()
    }
}

/// 由外部 capability probe 解析出的有效 Claude 管理策略证据。
/// 证据绑定安装版本；升级或缺少版本时自动失效为 unknown。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaudeCustomizationPolicyEvidence {
    installation_version: String,
    claude_config_dir: Option<PathBuf>,
    source_path: Option<PathBuf>,
    policy: ClaudeCustomizationPolicy,
}

impl VerifiedClaudeCustomizationPolicyEvidence {
    pub fn from_effective_setting(
        installation_version: impl Into<String>,
        setting: Option<&Value>,
    ) -> Result<Self, AppError> {
        let installation_version = installation_version.into();
        if installation_version.trim().is_empty() {
            return Err(AppError::invalid_input(
                "installationVersion",
                "策略证据必须绑定安装版本",
            ));
        }
        let policy = match setting {
            None | Some(Value::Bool(false)) => ClaudeCustomizationPolicy {
                mcp: PolicyState::Allowed,
                skill: PolicyState::Allowed,
            },
            Some(Value::Bool(true)) => ClaudeCustomizationPolicy {
                mcp: PolicyState::Blocked,
                skill: PolicyState::Blocked,
            },
            Some(Value::Array(surfaces)) if surfaces.iter().all(Value::is_string) => {
                ClaudeCustomizationPolicy {
                    mcp: if surfaces
                        .iter()
                        .any(|surface| surface.as_str() == Some("mcp"))
                    {
                        PolicyState::Blocked
                    } else {
                        PolicyState::Allowed
                    },
                    skill: if surfaces
                        .iter()
                        .any(|surface| surface.as_str() == Some("skills"))
                    {
                        PolicyState::Blocked
                    } else {
                        PolicyState::Allowed
                    },
                }
            }
            Some(_) => {
                return Err(AppError::invalid_input(
                    "strictPluginOnlyCustomization",
                    "有效策略必须是布尔值或字符串数组",
                ));
            }
        };
        Ok(Self {
            installation_version,
            claude_config_dir: None,
            source_path: None,
            policy,
        })
    }

    pub fn from_official_source(
        installation_version: impl Into<String>,
        claude_config_dir: impl Into<PathBuf>,
        source_path: Option<&Path>,
        setting: Option<&Value>,
    ) -> Result<Self, AppError> {
        if source_path.is_none() && setting.is_some() {
            return Err(AppError::invalid_input(
                "policySourcePath",
                "显式策略值必须绑定已核验的官方来源",
            ));
        }
        let mut evidence = Self::from_effective_setting(installation_version, setting)?;
        evidence.claude_config_dir = Some(normalize_config_root(
            &claude_config_dir.into(),
            "claudeConfigDir",
        )?);
        if let Some(source_path) = source_path {
            let normalized_source = normalize_target_path(source_path, "policySourcePath")?;
            if normalized_source != source_path {
                return Err(AppError::invalid_input(
                    "policySourcePath",
                    "策略来源必须是无链接重定向的规范绝对路径",
                ));
            }
            evidence.source_path = Some(normalized_source);
        }
        Ok(evidence)
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}

impl ClaudeCustomizationPolicyProbe for VerifiedClaudeCustomizationPolicyEvidence {
    fn probe(&self, input: &ClaudeCustomizationPolicyProbeInput<'_>) -> ClaudeCustomizationPolicy {
        if input.tool_installed
            && input.installation_version == Some(self.installation_version.as_str())
            && self
                .claude_config_dir
                .as_deref()
                .map_or(true, |root| root == input.claude_config_dir)
            && self.source_path.as_deref() == input.source_path
        {
            self.policy
        } else {
            ClaudeCustomizationPolicy::unknown()
        }
    }
}

/// 默认保守探针：只接受官方明确的默认 `$HOME/.claude.json`。
#[derive(Debug, Default)]
pub struct ConservativeClaudeUserMcpProbe;

impl ClaudeUserMcpCapabilityProbe for ConservativeClaudeUserMcpProbe {
    fn probe(&self, input: &ClaudeUserMcpProbeInput<'_>) -> ClaudeUserMcpProbeResult {
        if !input.tool_installed {
            return ClaudeUserMcpProbeResult::ToolNotInstalled;
        }
        if input.uses_default_config_dir {
            ClaudeUserMcpProbeResult::Supported(input.home.join(".claude.json"))
        } else {
            ClaudeUserMcpProbeResult::Unsupported("CLAUDE_USER_MCP_LOCATION_UNSUPPORTED")
        }
    }
}

/// 当前安装版本的外部探针可把已核验结果封装为证据；根或版本不匹配时失效。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaudeUserMcpEvidence {
    installation_version: String,
    claude_config_dir: PathBuf,
    user_mcp_path: PathBuf,
}

impl VerifiedClaudeUserMcpEvidence {
    pub fn new(
        installation_version: impl Into<String>,
        claude_config_dir: impl Into<PathBuf>,
        user_mcp_path: impl Into<PathBuf>,
    ) -> Result<Self, AppError> {
        let installation_version = installation_version.into();
        if installation_version.trim().is_empty() {
            return Err(AppError::invalid_input(
                "installationVersion",
                "探针证据必须绑定安装版本",
            ));
        }
        let claude_config_dir =
            normalize_config_root(&claude_config_dir.into(), "claudeConfigDir")?;
        let user_mcp_path = normalize_target_path(&user_mcp_path.into(), "userMcpPath")?;
        Ok(Self {
            installation_version,
            claude_config_dir,
            user_mcp_path,
        })
    }

    pub fn installation_version(&self) -> &str {
        &self.installation_version
    }
}

impl ClaudeUserMcpCapabilityProbe for VerifiedClaudeUserMcpEvidence {
    fn probe(&self, input: &ClaudeUserMcpProbeInput<'_>) -> ClaudeUserMcpProbeResult {
        if !input.tool_installed {
            return ClaudeUserMcpProbeResult::ToolNotInstalled;
        }
        if input.claude_config_dir == self.claude_config_dir
            && input.installation_version == Some(self.installation_version.as_str())
        {
            ClaudeUserMcpProbeResult::Supported(self.user_mcp_path.clone())
        } else {
            ClaudeUserMcpProbeResult::Unsupported("CLAUDE_CAPABILITY_EVIDENCE_STALE")
        }
    }
}

pub struct DiscoveryContext<'a> {
    pub environment: &'a ExplicitEnvironment,
    pub project_root: Option<&'a ProjectRoot>,
    pub claude_user_mcp_probe: &'a dyn ClaudeUserMcpCapabilityProbe,
    pub claude_customization_policy_probe: &'a dyn ClaudeCustomizationPolicyProbe,
}

/// Provider 适配器从发现结果中提取工具专属选项时使用的最小输入。
/// 使用稳定字符串避免适配层依赖 Profiles 的 RPC DTO。
#[derive(Debug, Clone, Copy)]
pub struct ProviderCodecInput<'a> {
    pub credential_env_key: Option<&'a str>,
    pub extra_env: &'a BTreeMap<String, String>,
    pub wire_api: Option<&'a str>,
    pub zcode_kind: Option<&'a str>,
    pub opencode_npm: Option<&'a str>,
    pub opencode_api: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderCodecOptions {
    pub credential_env_key: Option<String>,
    pub extra_env: BTreeMap<String, String>,
    pub wire_api: Option<String>,
    pub zcode_kind: Option<String>,
    pub opencode_npm: Option<String>,
    pub opencode_api: Option<String>,
}

/// Provider 档案投影所需的最小公共输入。
///
/// 该类型只包含稳定的原生字段，不让适配层依赖 Profiles 的数据库记录或
/// RPC DTO。敏感值仍由调用方传入；投影在进入 Preview 前由 Profiles 统一脱敏。
#[derive(Debug, Clone, Copy)]
pub struct ProviderCodecProfileInput<'a> {
    pub name: &'a str,
    pub api_base_url: Option<&'a str>,
    pub api_key: Option<&'a str>,
    pub default_model: Option<&'a str>,
    pub credential_env_key: Option<&'a str>,
    pub extra_env: &'a BTreeMap<String, String>,
    pub provider_id: Option<&'a str>,
    pub wire_api: Option<&'a str>,
    pub zcode_kind: Option<&'a str>,
    pub opencode_npm: Option<&'a str>,
    pub opencode_api: Option<&'a str>,
    pub extra_provider_fields: &'a BTreeMap<String, Value>,
}

/// Adapter 发现的 Provider 事实；字段保持为适配层可理解的稳定值，Profiles
/// 仅负责把它校验、脱敏并持久化为自己的模型。
#[derive(Debug, Clone)]
pub struct ProviderCodecDiscovery {
    pub target_path: String,
    pub full_hash: String,
    pub projection: Value,
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub default_model: String,
    pub credential_env_key: String,
    pub extra_env: BTreeMap<String, String>,
    pub provider_id: Option<String>,
    pub wire_api: Option<String>,
    pub zcode_kind: Option<String>,
    pub opencode_npm: Option<String>,
    pub opencode_api: Option<String>,
    pub extra_provider_fields: BTreeMap<String, Value>,
    pub suggested_name: Option<String>,
}

/// Provider 的工具专属 ownership 与选项编解码合同。
pub trait ProviderCodec: Sync {
    fn discovery_ownership(&self) -> Result<ManagedOwnership, AppError>;

    fn ownership(
        &self,
        baseline: Option<&Value>,
        desired: &Value,
    ) -> Result<ManagedOwnership, AppError>;

    fn default_options(
        &self,
        input: &ProviderCodecInput<'_>,
    ) -> Result<ProviderCodecOptions, AppError>;

    /// 将中央 Provider 档案投影为该工具的原生 managed projection。
    fn render(&self, input: &ProviderCodecProfileInput<'_>) -> Result<Value, AppError>;

    /// 从已经扫描的原生 Provider 目标提取可导入事实。
    ///
    /// 只传入 descriptor、managed projection 与 hash，避免 codec 依赖 sync
    /// 层的内部观察类型；Profiles 仍掌握扫描/策略/持久化编排。
    fn discover(
        &self,
        descriptor: &TargetDescriptor,
        managed_projection: &Value,
        full_hash: &str,
    ) -> Result<Option<ProviderCodecDiscovery>, AppError>;
}

pub trait ToolAdapter {
    fn tool(&self) -> Tool;

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError>;

    fn provider_codec(&self) -> Option<&dyn ProviderCodec> {
        None
    }

    fn parse(
        &self,
        target: &TargetDescriptor,
        raw: ObservedRaw,
    ) -> Result<ObservedDocument, AppError> {
        parse_document(target, raw)
    }

    fn project_managed(
        &self,
        document: &ObservedDocument,
        ownership: &ManagedOwnership,
    ) -> Result<Value, AppError> {
        project_document(document, ownership)
    }

    fn render(
        &self,
        target: &TargetDescriptor,
        current: Option<&ObservedDocument>,
        desired_projection: &Value,
        ownership: &ManagedOwnership,
    ) -> Result<RenderedTarget, AppError> {
        validate_managed_ownership(target, ownership)?;
        if let Some(current) = current {
            project_document(current, ownership)?;
        }
        render_document(target, current, desired_projection, ownership)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub target_type: TargetType,
    pub link_target: Option<String>,
}

pub enum ObservedRaw {
    File(Vec<u8>),
    Directory(BTreeMap<String, DirectoryEntry>),
}

pub enum ObservedDocument {
    Json(Value),
    Jsonc {
        value: Value,
        source: String,
    },
    Toml {
        document: DocumentMut,
        semantic: Value,
    },
    Markdown(String),
    SymlinkDirectory(BTreeMap<String, DirectoryEntry>),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "paths")]
pub enum ManagedOwnership {
    WholeDocument,
    Selectors(Vec<Vec<String>>),
    SymlinkNames(Vec<String>),
}

impl ManagedOwnership {
    pub fn selectors<I, P, S>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Selectors(
            paths
                .into_iter()
                .map(|path| path.into_iter().map(Into::into).collect())
                .collect(),
        )
    }
}

pub fn validate_managed_ownership(
    target: &TargetDescriptor,
    ownership: &ManagedOwnership,
) -> Result<(), AppError> {
    let valid = match ownership {
        ManagedOwnership::WholeDocument => target
            .managed_selector_roots
            .iter()
            .any(|root| root == "$document"),
        ManagedOwnership::SymlinkNames(_) => target
            .managed_selector_roots
            .iter()
            .any(|root| root == "$children"),
        ManagedOwnership::Selectors(selectors) => {
            !selectors.is_empty()
                && selectors.iter().all(|selector| {
                    selector.first().is_some_and(|head| {
                        target
                            .managed_selector_roots
                            .iter()
                            .any(|root| root == head)
                    })
                })
        }
    };
    if valid {
        Ok(())
    } else {
        Err(AppError::invalid_input(
            "managedOwnership",
            "受管选择器超出目标声明的拥有范围",
        ))
    }
}

pub enum RenderedTarget {
    File(Vec<u8>),
}
