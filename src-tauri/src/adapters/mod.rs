//! Claude 与 Codex 原生格式适配层。
//!
//! 路径和工具环境必须由调用方显式注入。本模块不会读取进程的 `HOME`、
//! `CLAUDE_CONFIG_DIR` 或 `CODEX_HOME`，也不执行任何外部写入。

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use specta::Type;
use toml_edit::{Array, DocumentMut, Item, Table, TableLike};

use crate::{
    domain::{ArtifactKind, ProjectRoot, Scope, TargetType, Tool},
    error::AppError,
};

pub mod claude;
pub mod codex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TargetFormat {
    Json,
    Toml,
    Markdown,
    SymlinkDirectory,
}

impl TargetFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Markdown => "markdown",
            Self::SymlinkDirectory => "symlink_directory",
        }
    }

    pub const fn expected_type(self) -> TargetType {
        match self {
            Self::Json | Self::Toml | Self::Markdown => TargetType::File,
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
    pub format: TargetFormat,
    pub managed_selector_roots: Vec<String>,
    pub sensitive_selectors: Vec<String>,
    pub capability: TargetCapability,
    pub policy: PolicyState,
    pub trust: TargetTrustState,
    pub prompt_override: PromptOverrideState,
    pub symlink_policy: SymlinkPolicy,
}

impl TargetDescriptor {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref().map(Path::new)
    }

    fn path_for_error(&self) -> &str {
        self.path.as_deref().unwrap_or("<unsupported>")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolAvailability {
    pub claude: bool,
    pub codex: bool,
}

impl ToolAvailability {
    pub const fn all_installed() -> Self {
        Self {
            claude: true,
            codex: true,
        }
    }
}

/// 显式工具环境。构造时只验证调用方提供的路径，不读取进程环境。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitEnvironment {
    home: PathBuf,
    claude_config_dir: PathBuf,
    codex_home: PathBuf,
    uses_default_claude_config_dir: bool,
    claude_installation_version: Option<String>,
    availability: ToolAvailability,
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
        let claude_config_dir =
            normalize_config_root(&requested_claude_config_dir, "claudeConfigDir")?;
        let codex_home = normalize_config_root(&requested_codex_home, "codexHome")?;

        Ok(Self {
            home,
            claude_config_dir,
            codex_home,
            uses_default_claude_config_dir,
            claude_installation_version: None,
            availability,
        })
    }

    pub fn with_claude_installation_version(
        mut self,
        version: impl Into<String>,
    ) -> Result<Self, AppError> {
        let version = version.into();
        if version.trim().is_empty() {
            return Err(AppError::invalid_input(
                "installationVersion",
                "Claude 安装版本不能为空",
            ));
        }
        self.claude_installation_version = Some(version);
        Ok(self)
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

    pub fn availability(&self) -> ToolAvailability {
        self.availability
    }

    pub fn claude_installation_version(&self) -> Option<&str> {
        self.claude_installation_version.as_deref()
    }

    pub fn uses_default_claude_config_dir(&self) -> bool {
        self.uses_default_claude_config_dir
    }
}

/// 对已存在的登记目录做真实 canonicalization，拒绝文件、根目录和相对路径。
pub fn canonicalize_project_root(path: &Path) -> Result<ProjectRoot, AppError> {
    validate_absolute_normal_path(path, "rootPath")?;
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::not_found("project", &path.to_string_lossy()));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(AppError::permission(
                &path.to_string_lossy(),
                "lstat_project_root",
            ));
        }
        Err(_) => {
            return Err(AppError::invalid_input("rootPath", "项目根无法安全读取"));
        }
    }
    let canonical = fs::canonicalize(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AppError::not_found("project", &path.to_string_lossy()),
        std::io::ErrorKind::PermissionDenied => {
            AppError::permission(&path.to_string_lossy(), "canonicalize_project_root")
        }
        _ => AppError::invalid_input("rootPath", "项目根无法安全规范化"),
    })?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AppError::not_found("project", &path.to_string_lossy()),
        std::io::ErrorKind::PermissionDenied => {
            AppError::permission(&path.to_string_lossy(), "lstat_canonical_project_root")
        }
        _ => AppError::invalid_input("rootPath", "规范化项目根无法安全读取"),
    })?;
    if !metadata.is_dir() {
        return Err(AppError::invalid_input("rootPath", "项目根必须是目录"));
    }
    ProjectRoot::parse(&canonical)
}

fn canonicalize_existing_directory(path: &Path, field: &'static str) -> Result<PathBuf, AppError> {
    validate_absolute_normal_path(path, field)?;
    let canonical = fs::canonicalize(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AppError::not_found("directory", &path.to_string_lossy()),
        std::io::ErrorKind::PermissionDenied => {
            AppError::permission(&path.to_string_lossy(), "canonicalize_directory")
        }
        _ => AppError::invalid_input(field, "目录无法安全规范化"),
    })?;
    let metadata = fs::symlink_metadata(&canonical).map_err(|error| match error.kind() {
        std::io::ErrorKind::PermissionDenied => {
            AppError::permission(&canonical.to_string_lossy(), "lstat_canonical_directory")
        }
        _ => AppError::not_found("directory", &canonical.to_string_lossy()),
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
        Err(_) => Err(AppError::permission(
            &path.to_string_lossy(),
            "lstat_config_root",
        )),
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
            Err(_) => {
                return Err(AppError::permission(
                    &ancestor.to_string_lossy(),
                    "lstat_config_ancestor",
                ));
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
#[derive(Debug, Clone)]
pub struct VerifiedClaudeCustomizationPolicyEvidence {
    installation_version: String,
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
            policy,
        })
    }
}

impl ClaudeCustomizationPolicyProbe for VerifiedClaudeCustomizationPolicyEvidence {
    fn probe(&self, input: &ClaudeCustomizationPolicyProbeInput<'_>) -> ClaudeCustomizationPolicy {
        if input.tool_installed
            && input.installation_version == Some(self.installation_version.as_str())
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
#[derive(Debug, Clone)]
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

pub trait ToolAdapter {
    fn tool(&self) -> Tool;

    fn discover(&self, context: &DiscoveryContext<'_>) -> Result<Vec<TargetDescriptor>, AppError>;

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

fn parse_document(
    target: &TargetDescriptor,
    raw: ObservedRaw,
) -> Result<ObservedDocument, AppError> {
    match (target.format, raw) {
        (TargetFormat::Json, ObservedRaw::File(bytes)) => {
            if bytes.iter().all(u8::is_ascii_whitespace) {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            let value = serde_json::from_slice::<Value>(&bytes)
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            if !value.is_object() {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            Ok(ObservedDocument::Json(value))
        }
        (TargetFormat::Toml, ObservedRaw::File(bytes)) => {
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            if text.trim().is_empty() {
                return Err(AppError::parse(
                    target.path_for_error(),
                    target.format.as_str(),
                ));
            }
            let document = text
                .parse::<DocumentMut>()
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            let semantic = toml_edit::de::from_str::<Value>(text)
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            Ok(ObservedDocument::Toml { document, semantic })
        }
        (TargetFormat::Markdown, ObservedRaw::File(bytes)) => {
            let text = String::from_utf8(bytes)
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            Ok(ObservedDocument::Markdown(text))
        }
        (TargetFormat::SymlinkDirectory, ObservedRaw::Directory(entries)) => {
            Ok(ObservedDocument::SymlinkDirectory(entries))
        }
        _ => Err(AppError::parse(
            target.path_for_error(),
            target.format.as_str(),
        )),
    }
}

fn project_document(
    document: &ObservedDocument,
    ownership: &ManagedOwnership,
) -> Result<Value, AppError> {
    match (document, ownership) {
        (ObservedDocument::Json(value), ManagedOwnership::WholeDocument)
        | (
            ObservedDocument::Toml {
                semantic: value, ..
            },
            ManagedOwnership::WholeDocument,
        ) => Ok(value.clone()),
        (ObservedDocument::Json(value), ManagedOwnership::Selectors(selectors))
        | (
            ObservedDocument::Toml {
                semantic: value, ..
            },
            ManagedOwnership::Selectors(selectors),
        ) => project_selectors(value, selectors),
        (ObservedDocument::Markdown(text), ManagedOwnership::WholeDocument) => {
            Ok(Value::String(text.clone()))
        }
        (ObservedDocument::SymlinkDirectory(entries), ManagedOwnership::SymlinkNames(names)) => {
            let selected = names
                .iter()
                .filter_map(|name| {
                    entries.get(name).map(|entry| {
                        (
                            name.clone(),
                            serde_json::to_value(entry).unwrap_or(Value::Null),
                        )
                    })
                })
                .collect::<Map<_, _>>();
            Ok(Value::Object(selected))
        }
        _ => Err(AppError::invalid_input(
            "managedOwnership",
            "受管选择器与目标格式不匹配",
        )),
    }
}

fn project_selectors(source: &Value, selectors: &[Vec<String>]) -> Result<Value, AppError> {
    let mut output = Value::Object(Map::new());
    for selector in selectors {
        if let Some(value) = get_json_path(source, selector)? {
            set_json_path(&mut output, selector, value.clone())?;
        }
    }
    Ok(output)
}

fn render_document(
    target: &TargetDescriptor,
    current: Option<&ObservedDocument>,
    desired_projection: &Value,
    ownership: &ManagedOwnership,
) -> Result<RenderedTarget, AppError> {
    match (target.format, current, ownership) {
        (TargetFormat::Json, _, ManagedOwnership::WholeDocument) => {
            if !desired_projection.is_object() {
                return Err(AppError::invalid_input(
                    "desiredProjection",
                    "JSON 配置根必须是对象",
                ));
            }
            let mut bytes = serde_json::to_vec_pretty(desired_projection)
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            bytes.push(b'\n');
            Ok(RenderedTarget::File(bytes))
        }
        (TargetFormat::Json, current, ManagedOwnership::Selectors(selectors)) => {
            let mut merged = match current {
                Some(ObservedDocument::Json(value)) => value.clone(),
                None => Value::Object(Map::new()),
                _ => {
                    return Err(AppError::parse(
                        target.path_for_error(),
                        target.format.as_str(),
                    ))
                }
            };
            for selector in selectors {
                match get_json_path(desired_projection, selector)? {
                    Some(value) => set_json_path(&mut merged, selector, value.clone())?,
                    None => remove_json_path(&mut merged, selector),
                }
            }
            let mut bytes = serde_json::to_vec_pretty(&merged)
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            bytes.push(b'\n');
            Ok(RenderedTarget::File(bytes))
        }
        (TargetFormat::Toml, _, ManagedOwnership::WholeDocument) => {
            let document = toml_edit::ser::to_document(desired_projection)
                .map_err(|_| AppError::parse(target.path_for_error(), target.format.as_str()))?;
            Ok(RenderedTarget::File(document.to_string().into_bytes()))
        }
        (TargetFormat::Toml, current, ManagedOwnership::Selectors(selectors)) => {
            let mut document = match current {
                Some(ObservedDocument::Toml { document, .. }) => document.clone(),
                None => DocumentMut::new(),
                _ => {
                    return Err(AppError::parse(
                        target.path_for_error(),
                        target.format.as_str(),
                    ))
                }
            };
            for selector in selectors {
                let value = get_json_path(desired_projection, selector)?;
                set_toml_path(document.as_table_mut(), selector, value)?;
            }
            Ok(RenderedTarget::File(document.to_string().into_bytes()))
        }
        (TargetFormat::Markdown, _, ManagedOwnership::WholeDocument) => {
            let text = desired_projection.as_str().ok_or_else(|| {
                AppError::invalid_input("desiredProjection", "Markdown 投影必须是字符串")
            })?;
            Ok(RenderedTarget::File(text.as_bytes().to_vec()))
        }
        (TargetFormat::SymlinkDirectory, _, _) => Err(AppError::invalid_input(
            "targetFormat",
            "Phase 2 不渲染或写入 Skills 链接",
        )),
        _ => Err(AppError::invalid_input(
            "managedOwnership",
            "受管选择器与目标格式不匹配",
        )),
    }
}

fn get_json_path<'a>(value: &'a Value, path: &[String]) -> Result<Option<&'a Value>, AppError> {
    let mut current = value;
    for segment in path {
        let object = current.as_object().ok_or_else(|| {
            AppError::invalid_input(
                "managedProjection",
                "受管选择器的中间节点必须是对象或 TOML 表",
            )
        })?;
        let Some(next) = object.get(segment) else {
            return Ok(None);
        };
        current = next;
    }
    Ok(Some(current))
}

fn set_json_path(root: &mut Value, path: &[String], value: Value) -> Result<(), AppError> {
    if path.is_empty() {
        *root = value;
        return Ok(());
    }
    let mut current = root;
    for segment in &path[..path.len() - 1] {
        if !current.is_object() {
            return Err(AppError::invalid_input(
                "managedProjection",
                "受管选择器不能覆盖非对象中间节点",
            ));
        }
        current = current
            .as_object_mut()
            .expect("已验证为 JSON 对象")
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if !current.is_object() {
        return Err(AppError::invalid_input(
            "managedProjection",
            "受管选择器不能覆盖非对象中间节点",
        ));
    }
    current
        .as_object_mut()
        .expect("已验证为 JSON 对象")
        .insert(path[path.len() - 1].clone(), value);
    Ok(())
}

fn remove_json_path(root: &mut Value, path: &[String]) {
    if path.is_empty() {
        *root = Value::Null;
        return;
    }
    let mut current = root;
    for segment in &path[..path.len() - 1] {
        let Some(next) = current
            .as_object_mut()
            .and_then(|object| object.get_mut(segment))
        else {
            return;
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(&path[path.len() - 1]);
    }
}

fn set_toml_path(
    table: &mut dyn TableLike,
    path: &[String],
    value: Option<&Value>,
) -> Result<(), AppError> {
    let Some((head, tail)) = path.split_first() else {
        return Err(AppError::invalid_input(
            "managedSelector",
            "TOML selector 不能为空",
        ));
    };
    if tail.is_empty() {
        if let Some(value) = value {
            let key_decor = table.key(head).map(|key| key.leaf_decor().clone());
            let mut replacement = json_to_toml_item(value)?;
            if let Some(current) = table.get(head) {
                preserve_toml_decor(current, &mut replacement);
            }
            table.insert(head, replacement);
            if let (Some(decor), Some(mut key)) = (key_decor, table.key_mut(head)) {
                *key.leaf_decor_mut() = decor;
            }
        } else {
            table.remove(head);
        }
        return Ok(());
    }

    if value.is_none() && !table.contains_key(head) {
        return Ok(());
    }
    match table.get(head) {
        Some(item) if item.as_table_like().is_none() => {
            return Err(AppError::invalid_input(
                "managedProjection",
                "TOML 受管选择器不能覆盖非表中间节点",
            ));
        }
        None => {
            table.insert(head, Item::Table(Table::new()));
        }
        Some(_) => {}
    }
    let child = table
        .get_mut(head)
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| AppError::invalid_input("managedSelector", "TOML 路径不是表"))?;
    set_toml_path(child, tail, value)
}

fn preserve_toml_decor(current: &Item, replacement: &mut Item) {
    match (current, replacement) {
        (Item::Value(current), Item::Value(replacement)) => {
            *replacement.decor_mut() = current.decor().clone();
        }
        (Item::Table(current), Item::Table(replacement)) => {
            *replacement.decor_mut() = current.decor().clone();
        }
        _ => {}
    }
}

fn json_to_toml_item(value: &Value) -> Result<Item, AppError> {
    match value {
        Value::String(value) => Ok(toml_edit::value(value.clone())),
        Value::Bool(value) => Ok(toml_edit::value(*value)),
        Value::Number(value) if value.is_i64() => {
            Ok(toml_edit::value(value.as_i64().expect("已验证为 i64")))
        }
        Value::Number(value) if value.is_u64() => {
            let number = i64::try_from(value.as_u64().expect("已验证为 u64"))
                .map_err(|_| AppError::invalid_input("desiredProjection", "TOML 整数超出范围"))?;
            Ok(toml_edit::value(number))
        }
        Value::Number(value) => Ok(toml_edit::value(
            value.as_f64().expect("JSON 数字必须可转为 f64"),
        )),
        Value::Array(values) => {
            let mut array = Array::new();
            for value in values {
                let item = json_to_toml_item(value)?;
                let scalar = item.into_value().map_err(|_| {
                    AppError::invalid_input("desiredProjection", "TOML 数组只支持标量值")
                })?;
                array.push_formatted(scalar);
            }
            Ok(toml_edit::value(array))
        }
        Value::Object(values) => {
            let mut table = Table::new();
            for (key, value) in values {
                table.insert(key, json_to_toml_item(value)?);
            }
            Ok(Item::Table(table))
        }
        Value::Null => Err(AppError::invalid_input(
            "desiredProjection",
            "TOML 不支持 null",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink, path::PathBuf};

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        canonicalize_project_root, CapabilityState, ConservativeClaudeCustomizationPolicyProbe,
        ConservativeClaudeUserMcpProbe, DiscoveryContext, ExplicitEnvironment, ManagedOwnership,
        ObservedRaw, PolicyState, PromptOverrideState, RenderedTarget, TargetTrustState,
        ToolAdapter, ToolAvailability, VerifiedClaudeCustomizationPolicyEvidence,
        VerifiedClaudeUserMcpEvidence,
    };
    use crate::{
        adapters::{claude::ClaudeAdapter, codex::CodexAdapter},
        domain::{ArtifactKind, Scope},
    };

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/phase2")
            .join(name)
    }

    fn environment(
        home: &std::path::Path,
        claude_root: Option<PathBuf>,
        codex_root: Option<PathBuf>,
    ) -> ExplicitEnvironment {
        ExplicitEnvironment::new(
            home,
            claude_root,
            codex_root,
            ToolAvailability::all_installed(),
        )
        .unwrap()
    }

    #[test]
    fn default_and_override_matrix_never_reads_process_tool_environment() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project = home.join("project");
        fs::create_dir(&project).unwrap();
        let project = canonicalize_project_root(&project).unwrap();
        let default_environment = environment(&home, None, None);
        let conservative_probe = ConservativeClaudeUserMcpProbe;
        let default_context = DiscoveryContext {
            environment: &default_environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &conservative_probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };

        let claude = ClaudeAdapter.discover(&default_context).unwrap();
        let claude_mcp = claude
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
            })
            .unwrap();
        assert_eq!(
            claude_mcp.path.as_deref(),
            Some(home.join(".claude.json").to_str().unwrap())
        );
        assert_eq!(
            claude
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                .unwrap()
                .path
                .as_deref(),
            Some(home.join(".claude/CLAUDE.md").to_str().unwrap())
        );

        let custom_claude = home.join("custom-claude");
        let custom_codex = home.join("custom-codex");
        let custom_environment = environment(
            &home,
            Some(custom_claude.clone()),
            Some(custom_codex.clone()),
        )
        .with_claude_installation_version("fixture-1.0.0")
        .unwrap();
        let custom_context = DiscoveryContext {
            environment: &custom_environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &conservative_probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let claude = ClaudeAdapter.discover(&custom_context).unwrap();
        let unsupported_mcp = claude
            .iter()
            .find(|target| {
                target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Global
            })
            .unwrap();
        assert_eq!(unsupported_mcp.path, None);
        assert_eq!(
            unsupported_mcp.capability.state,
            CapabilityState::Unsupported
        );

        let codex = CodexAdapter.discover(&custom_context).unwrap();
        assert_eq!(
            codex
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Provider)
                .unwrap()
                .path
                .as_deref(),
            Some(custom_codex.join("config.toml").to_str().unwrap())
        );
        assert_eq!(
            codex
                .iter()
                .find(|target| {
                    target.artifact_kind == ArtifactKind::Skill && target.scope == Scope::Global
                })
                .unwrap()
                .path
                .as_deref(),
            Some(home.join(".agents/skills").to_str().unwrap()),
            "Codex 用户 Skills 不能随 CODEX_HOME 迁移"
        );

        let verified_path = home.join("verified/user-mcp.json");
        let evidence =
            VerifiedClaudeUserMcpEvidence::new("fixture-1.0.0", &custom_claude, &verified_path)
                .unwrap();
        let verified_context = DiscoveryContext {
            environment: &custom_environment,
            project_root: None,
            claude_user_mcp_probe: &evidence,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let verified = ClaudeAdapter.discover(&verified_context).unwrap();
        let verified_mcp = verified
            .iter()
            .find(|target| target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        assert_eq!(
            verified_mcp.path.as_deref(),
            Some(verified_path.to_str().unwrap())
        );
        assert_eq!(verified_mcp.capability.state, CapabilityState::Supported);

        let default_with_version = environment(&home, None, None)
            .with_claude_installation_version("fixture-1.0.0")
            .unwrap();
        let invalid_default_evidence = VerifiedClaudeUserMcpEvidence::new(
            "fixture-1.0.0",
            default_with_version.claude_config_dir(),
            home.join("unexpected-default-user-mcp.json"),
        )
        .unwrap();
        let fixed_default = ClaudeAdapter
            .discover(&DiscoveryContext {
                environment: &default_with_version,
                project_root: None,
                claude_user_mcp_probe: &invalid_default_evidence,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap();
        assert_eq!(
            fixed_default
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Mcp)
                .unwrap()
                .path
                .as_deref(),
            Some(home.join(".claude.json").to_str().unwrap()),
            "默认 Claude 配置根的用户 MCP 位置不能被外部证据改写"
        );
    }

    #[test]
    fn project_root_is_canonicalized_and_symlink_alias_is_not_persisted() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let project = home.join("real-project");
        fs::create_dir(&project).unwrap();
        let alias = home.join("project-alias");
        symlink(&project, &alias).unwrap();

        let canonical = canonicalize_project_root(&alias).unwrap();
        assert_eq!(canonical.as_str(), project.to_str().unwrap());

        let loop_one = home.join("loop-one");
        let loop_two = home.join("loop-two");
        symlink(&loop_two, &loop_one).unwrap();
        symlink(&loop_one, &loop_two).unwrap();
        assert_eq!(
            canonicalize_project_root(&loop_one).unwrap_err().code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            canonicalize_project_root(&home.join("missing-project"))
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::NotFound
        );
    }

    #[test]
    fn missing_override_root_is_canonicalized_from_its_existing_symlink_ancestor() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let outside = home.join("real-config-parent");
        fs::create_dir(&outside).unwrap();
        let alias = home.join("config-alias");
        symlink(&outside, &alias).unwrap();
        let environment = environment(&home, Some(alias.join("claude")), None);

        assert_eq!(environment.claude_config_dir(), outside.join("claude"));
    }

    #[test]
    fn unavailable_tools_have_a_distinct_capability_state() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let environment = ExplicitEnvironment::new(
            &home,
            None,
            None,
            ToolAvailability {
                claude: false,
                codex: false,
            },
        )
        .unwrap();
        let probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: None,
            claude_user_mcp_probe: &probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
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
    fn claude_policy_and_codex_trust_are_discovered_from_isolated_fixtures() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let claude_root = home.join("claude-root");
        let codex_root = home.join("codex-root");
        let project_path = home.join("project");
        fs::create_dir_all(&claude_root).unwrap();
        fs::create_dir_all(&codex_root).unwrap();
        fs::create_dir(&project_path).unwrap();
        let codex_fixture = fs::read_to_string(fixture("codex-config.toml"))
            .unwrap()
            .replace("/fixture/project", project_path.to_str().unwrap());
        fs::write(codex_root.join("config.toml"), codex_fixture).unwrap();
        let environment = environment(&home, Some(claude_root), Some(codex_root))
            .with_claude_installation_version("fixture-1.0.0")
            .unwrap();
        let project = canonicalize_project_root(&project_path).unwrap();
        let evidence = VerifiedClaudeUserMcpEvidence::new(
            "fixture-1.0.0",
            environment.claude_config_dir(),
            home.join("verified-user-mcp.json"),
        )
        .unwrap();
        let policy_fixture: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture("claude-policy-blocked.json")).unwrap())
                .unwrap();
        let policy_evidence = VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
            "fixture-1.0.0",
            policy_fixture.get("strictPluginOnlyCustomization"),
        )
        .unwrap();
        let context = DiscoveryContext {
            environment: &environment,
            project_root: Some(&project),
            claude_user_mcp_probe: &evidence,
            claude_customization_policy_probe: &policy_evidence,
        };

        let claude = ClaudeAdapter.discover(&context).unwrap();
        assert!(claude
            .iter()
            .filter(|target| matches!(
                target.artifact_kind,
                ArtifactKind::Mcp | ArtifactKind::Skill
            ))
            .all(|target| target.policy == PolicyState::Blocked));
        let codex = CodexAdapter.discover(&context).unwrap();
        assert_eq!(
            codex
                .iter()
                .find(|target| {
                    target.artifact_kind == ArtifactKind::Mcp && target.scope == Scope::Project
                })
                .unwrap()
                .trust,
            TargetTrustState::Trusted
        );

        fs::write(
            environment.codex_home().join("config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"untrusted\"\n",
                project.as_str()
            ),
        )
        .unwrap();
        let untrusted = CodexAdapter.discover(&context).unwrap();
        assert_eq!(
            untrusted
                .iter()
                .filter(|target| target.scope == Scope::Project)
                .map(|target| target.trust)
                .collect::<Vec<_>>(),
            vec![TargetTrustState::Untrusted, TargetTrustState::Untrusted]
        );
    }

    #[test]
    fn claude_policy_evidence_distinguishes_mcp_and_skills_and_fails_closed_when_stale() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let current_environment = environment(&home, None, None)
            .with_claude_installation_version("fixture-1.0.0")
            .unwrap();
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let policy = VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(
            "fixture-1.0.0",
            Some(&json!(["skills"])),
        )
        .unwrap();
        let targets = ClaudeAdapter
            .discover(&DiscoveryContext {
                environment: &current_environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &policy,
            })
            .unwrap();
        assert_eq!(
            targets
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Mcp)
                .unwrap()
                .policy,
            PolicyState::Allowed
        );
        assert_eq!(
            targets
                .iter()
                .find(|target| target.artifact_kind == ArtifactKind::Skill)
                .unwrap()
                .policy,
            PolicyState::Blocked
        );

        let upgraded_environment = environment(&home, None, None)
            .with_claude_installation_version("fixture-2.0.0")
            .unwrap();
        assert!(ClaudeAdapter
            .discover(&DiscoveryContext {
                environment: &upgraded_environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &policy,
            })
            .unwrap()
            .iter()
            .filter(|target| matches!(
                target.artifact_kind,
                ArtifactKind::Mcp | ArtifactKind::Skill
            ))
            .all(|target| target.policy == PolicyState::Unknown));
    }

    #[test]
    fn codex_prompt_override_is_reported_without_following_unknown_links() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let codex_root = home.join("codex-root");
        fs::create_dir(&codex_root).unwrap();
        let current_environment = environment(&home, None, Some(codex_root.clone()));
        let user_mcp_probe = ConservativeClaudeUserMcpProbe;
        let discover = || {
            CodexAdapter
                .discover(&DiscoveryContext {
                    environment: &current_environment,
                    project_root: None,
                    claude_user_mcp_probe: &user_mcp_probe,
                    claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
                })
                .unwrap()
                .into_iter()
                .find(|target| target.artifact_kind == ArtifactKind::Prompt)
                .unwrap()
                .prompt_override
        };

        assert_eq!(discover(), PromptOverrideState::NotPresent);
        fs::write(codex_root.join("AGENTS.override.md"), "覆盖提示词").unwrap();
        assert_eq!(discover(), PromptOverrideState::Present);
        fs::write(codex_root.join("AGENTS.override.md"), "").unwrap();
        assert_eq!(discover(), PromptOverrideState::NotPresent);
        fs::remove_file(codex_root.join("AGENTS.override.md")).unwrap();
        let outside = home.join("outside-override.md");
        fs::write(&outside, "未知链接内容").unwrap();
        symlink(&outside, codex_root.join("AGENTS.override.md")).unwrap();
        assert_eq!(discover(), PromptOverrideState::Unknown);

        let late_root = home.join("late-codex-root");
        let late_environment = environment(&home, None, Some(late_root.clone()));
        let outside_root = home.join("outside-codex-root");
        fs::create_dir(&outside_root).unwrap();
        fs::write(outside_root.join("AGENTS.override.md"), "不得读取").unwrap();
        symlink(&outside_root, &late_root).unwrap();
        let late_prompt = CodexAdapter
            .discover(&DiscoveryContext {
                environment: &late_environment,
                project_root: None,
                claude_user_mcp_probe: &user_mcp_probe,
                claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
            })
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Prompt)
            .unwrap();
        assert_eq!(late_prompt.prompt_override, PromptOverrideState::Unknown);
    }

    #[test]
    fn toml_projection_render_preserves_unmanaged_tables_and_comments() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let codex_root = home.join("codex-root");
        fs::create_dir(&codex_root).unwrap();
        let environment = environment(&home, None, Some(codex_root));
        let probe = ConservativeClaudeUserMcpProbe;
        let context = DiscoveryContext {
            environment: &environment,
            project_root: None,
            claude_user_mcp_probe: &probe,
            claude_customization_policy_probe: &ConservativeClaudeCustomizationPolicyProbe,
        };
        let adapter = CodexAdapter;
        let descriptor = adapter
            .discover(&context)
            .unwrap()
            .into_iter()
            .find(|target| target.artifact_kind == ArtifactKind::Provider)
            .unwrap();
        let raw = fs::read(fixture("codex-config.toml")).unwrap();
        let document = adapter.parse(&descriptor, ObservedRaw::File(raw)).unwrap();
        let ownership = ManagedOwnership::selectors([
            vec!["model"],
            vec!["model_provider"],
            vec!["model_providers", "easytoagents_fixture"],
        ]);
        let projection = adapter.project_managed(&document, &ownership).unwrap();
        assert_eq!(projection["model"], "fixture-model");

        let desired = json!({
            "model": "replacement-model",
            "model_provider": "easytoagents_fixture",
            "model_providers": {
                "easytoagents_fixture": {
                    "name": "Replacement",
                    "base_url": "https://replacement.invalid/v1",
                    "experimental_bearer_token": "replacement-secret"
                }
            }
        });
        let RenderedTarget::File(rendered) = adapter
            .render(&descriptor, Some(&document), &desired, &ownership)
            .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();
        assert!(
            rendered.contains("# 必须保留的文件头注释"),
            "渲染结果：{rendered}"
        );
        assert!(rendered.contains("# 此注释和表不属于应用管理范围"));
        assert!(rendered.contains("[plugins]"));
        assert!(rendered.contains("[mcp_servers.fixture_user]"));
        assert!(rendered.contains("replacement-model"));
    }
}
