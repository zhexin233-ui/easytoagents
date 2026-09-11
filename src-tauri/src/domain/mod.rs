//! 不依赖桌面壳与具体配置文件格式的领域模型。

use std::{collections::HashSet, fmt, path::Path};

use serde::{de, Deserialize, Deserializer, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::error::AppError;

macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($variant:ident => $value:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Type)]
        pub enum $name {
            $(#[serde(rename = $value)] $variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }

            pub fn from_stable_str(value: &str) -> Option<Self> {
                match value {
                    $($value => Some(Self::$variant)),+,
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// 正式支持的原生工具。
    pub enum Tool {
        Claude => "claude",
        Codex => "codex",
        Cursor => "cursor",
        Zcode => "zcode",
        Opencode => "opencode",
    }
}

impl Tool {
    /// 所有受支持工具的稳定注册顺序。
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Cursor,
        Self::Zcode,
        Self::Opencode,
    ];

    /// 返回工具的唯一 Adapter 实例。
    pub fn adapter(self) -> &'static dyn crate::adapters::ToolAdapter {
        crate::adapters::adapter_for(self)
    }
}

/// 主实体统一使用 UUID 文本标识，避免各功能自行发明 ID 规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct EntityId(Uuid);

impl EntityId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        Uuid::parse_str(value).map(Self).map_err(|error| {
            AppError::invalid_input("id", "实体 ID 必须是 UUID").with_source(error)
        })
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

string_enum! {
    /// 资源应用范围。
    pub enum Scope {
        Global => "global",
        Project => "project",
    }
}

string_enum! {
    /// 受管资源种类。
    pub enum ArtifactKind {
        Provider => "provider",
        Prompt => "prompt",
        Mcp => "mcp",
        Skill => "skill",
        Hook => "hook",
    }
}

string_enum! {
    /// Hook 的统一事件名（canonical PascalCase）。各工具的原生键可能不同
    /// （Cursor 为 camelCase），写入原生文件前必须经
    /// [`HookEvent::native_key`] 转换；每工具支持的官方事件集合由
    /// [`HookEvent::supported_for_tool`] 定义，不支持的组合必须 fail closed。
    pub enum HookEvent {
        SessionStart => "SessionStart",
        SessionEnd => "SessionEnd",
        UserPromptSubmit => "UserPromptSubmit",
        PreToolUse => "PreToolUse",
        PermissionRequest => "PermissionRequest",
        PostToolUse => "PostToolUse",
        PostToolUseFailure => "PostToolUseFailure",
        SubagentStart => "SubagentStart",
        SubagentStop => "SubagentStop",
        PreCompact => "PreCompact",
        PostCompact => "PostCompact",
        Stop => "Stop",
        Notification => "Notification",
    }
}

impl HookEvent {
    /// 官方 hooks 文档核验（2026-09-05）定义的每工具可配置事件集合：
    /// - Claude：settings.json `hooks` 键下出现的 10 个配置事件；
    /// - Codex：hooks.json 的 11 个事件；
    /// - ZCode：configuration hooks 的 7 个事件；
    /// - Cursor：与 canonical 语义一一对应的 9 个 camelCase 事件；
    ///   其余（UserPromptSubmit 等）无对应事件，不得猜测映射。
    pub fn supported_for_tool(self, tool: Tool) -> bool {
        match tool {
            Tool::Claude => matches!(
                self,
                Self::SessionStart
                    | Self::SessionEnd
                    | Self::UserPromptSubmit
                    | Self::PreToolUse
                    | Self::PermissionRequest
                    | Self::PostToolUse
                    | Self::Notification
                    | Self::SubagentStop
                    | Self::Stop
                    | Self::PreCompact
            ),
            Tool::Codex => matches!(
                self,
                Self::SessionStart
                    | Self::SessionEnd
                    | Self::UserPromptSubmit
                    | Self::PreToolUse
                    | Self::PermissionRequest
                    | Self::PostToolUse
                    | Self::PreCompact
                    | Self::PostCompact
                    | Self::SubagentStart
                    | Self::SubagentStop
                    | Self::Stop
            ),
            Tool::Zcode => matches!(
                self,
                Self::SessionStart
                    | Self::UserPromptSubmit
                    | Self::PreToolUse
                    | Self::PermissionRequest
                    | Self::PostToolUse
                    | Self::PostToolUseFailure
                    | Self::Stop
            ),
            Tool::Cursor => matches!(
                self,
                Self::SessionStart
                    | Self::SessionEnd
                    | Self::PreToolUse
                    | Self::PostToolUse
                    | Self::PostToolUseFailure
                    | Self::SubagentStart
                    | Self::SubagentStop
                    | Self::PreCompact
                    | Self::Stop
            ),
            // OpenCode hooks are implemented by plugin callbacks rather than the
            // command-only model represented by this enum. Keep the capability
            // explicitly unsupported instead of guessing a native mapping.
            Tool::Opencode => false,
        }
    }

    /// 该事件在目标工具原生配置中的键名（Cursor 官方为 camelCase）。
    pub fn native_key(self, tool: Tool) -> &'static str {
        match tool {
            Tool::Cursor => match self {
                Self::SessionStart => "sessionStart",
                Self::SessionEnd => "sessionEnd",
                Self::PreToolUse => "preToolUse",
                Self::PostToolUse => "postToolUse",
                Self::PostToolUseFailure => "postToolUseFailure",
                Self::SubagentStart => "subagentStart",
                Self::SubagentStop => "subagentStop",
                Self::PreCompact => "preCompact",
                Self::Stop => "stop",
                _ => self.as_str(),
            },
            Tool::Opencode => self.as_str(),
            _ => self.as_str(),
        }
    }
}

string_enum! {
    /// 原生目标相对最近一次受管基线的状态。
    pub enum SyncStatus {
        InSync => "in_sync",
        ExternalNonOwnedChange => "external_non_owned_change",
        ExternalOwnedChange => "external_owned_change",
        Missing => "missing",
        ParseError => "parse_error",
        PermissionDenied => "permission_denied",
        PolicyBlocked => "policy_blocked",
        Untrusted => "untrusted",
        TargetTypeChanged => "target_type_changed",
        Failed => "failed",
    }
}

string_enum! {
    /// 预览中的单目标变化。
    pub enum ChangeKind {
        Add => "add",
        Update => "update",
        Delete => "delete",
        Unchanged => "unchanged",
        Warning => "warning",
        Conflict => "conflict",
    }
}

string_enum! {
    pub enum SyncRunKind {
        Preview => "preview",
        Apply => "apply",
        Restore => "restore",
    }
}

string_enum! {
    pub enum SyncRunStatus {
        Previewed => "previewed",
        Applying => "applying",
        Restoring => "restoring",
        Succeeded => "succeeded",
        Failed => "failed",
        Stale => "stale",
        RolledBack => "rolled_back",
        RollbackFailed => "rollback_failed",
    }
}

string_enum! {
    pub enum McpTransport {
        Stdio => "stdio",
        StreamableHttp => "streamable_http",
    }
}

string_enum! {
    pub enum TrustStatus {
        Unknown => "unknown",
        Trusted => "trusted",
        Untrusted => "untrusted",
    }
}

string_enum! {
    pub enum SkillStatus {
        Ready => "ready",
        Invalid => "invalid",
        Missing => "missing",
    }
}

/// MCP、Skills 与 Hooks 共用的项目选择状态；各 RPC 模块保留历史别名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ManagedProjectSelectionState {
    Inherited,
    Selected,
    Available,
}

/// 三类受管资源共用的项目 DTO；字段顺序与现有 RPC 合同保持一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManagedProjectDto {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub codex_trust_status: TrustStatus,
    pub row_version: u32,
}

/// 三类受管资源共用的目标状态 DTO；模块别名维持前端类型名称。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManagedTargetStatusDto {
    pub tool: Tool,
    pub project_id: Option<String>,
    pub target_path: Option<String>,
    pub status: SyncStatus,
    pub diagnostic_code: Option<String>,
}

string_enum! {
    pub enum TargetType {
        File => "file",
        Directory => "directory",
        Symlink => "symlink",
        Missing => "missing",
    }
}

/// 领域对象通用的命名合同。数据库会以 `NOCASE` 唯一索引再次验证。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ArtifactName(String);

impl ArtifactName {
    pub fn parse(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if value.trim() != value {
            return Err(AppError::invalid_input("name", "名称首尾不能包含空白"));
        }
        if value.is_empty() || value.chars().count() > 100 {
            return Err(AppError::invalid_input(
                "name",
                "名称长度必须在 1 到 100 个字符之间",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(AppError::invalid_input("name", "名称不能包含控制字符"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn uniqueness_key(&self) -> String {
        self.0.to_ascii_lowercase()
    }
}

impl<'de> Deserialize<'de> for ArtifactName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

/// 项目根必须已经规范化为不包含 `.`/`..` 的绝对路径。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ProjectRoot(String);

impl ProjectRoot {
    pub fn parse(path: &Path) -> Result<Self, AppError> {
        if !path.is_absolute() {
            return Err(AppError::invalid_input("rootPath", "项目根必须是绝对路径"));
        }

        if path == Path::new("/") {
            return Err(AppError::invalid_input(
                "rootPath",
                "项目根不能是文件系统根目录",
            ));
        }

        let text = path
            .to_str()
            .ok_or_else(|| AppError::invalid_input("rootPath", "项目根必须是 UTF-8 路径"))?;
        if text.is_empty() || text.contains('\0') || text.contains("//") {
            return Err(AppError::invalid_input("rootPath", "项目根路径无效"));
        }

        use std::path::Component;
        if path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        }) {
            return Err(AppError::invalid_input(
                "rootPath",
                "项目根必须是不含相对片段的规范路径",
            ));
        }

        if text.len() > 1 && text.ends_with('/') {
            return Err(AppError::invalid_input(
                "rootPath",
                "项目根不能包含多余的末尾分隔符",
            ));
        }

        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ProjectRoot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(Path::new(&value)).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ProviderProfile {
    pub id: EntityId,
    pub tool: Tool,
    pub name: ArtifactName,
    pub api_base_url: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
    pub config_json: serde_json::Value,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PromptProfile {
    pub id: EntityId,
    pub tool: Tool,
    pub name: ArtifactName,
    pub body: String,
    pub is_active: bool,
    pub imported_from_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct McpServer {
    pub id: EntityId,
    pub name: ArtifactName,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: serde_json::Value,
    pub env: serde_json::Value,
    pub extra: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Skill {
    pub id: EntityId,
    pub name: ArtifactName,
    pub source_path: String,
    pub central_path: String,
    pub content_hash: String,
    pub frontmatter: serde_json::Value,
    pub status: SkillStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Project {
    pub id: EntityId,
    pub display_name: ArtifactName,
    pub root_path: ProjectRoot,
    pub is_git_repo: bool,
    pub codex_trust_status: TrustStatus,
    pub last_scanned_at: Option<String>,
}

/// 在进入仓储前验证每工具最多一个 active，并提前给出稳定领域错误。
pub fn validate_single_active_profile(
    profiles: impl IntoIterator<Item = (Tool, bool)>,
) -> Result<(), AppError> {
    let mut active_tools = HashSet::new();
    for (tool, is_active) in profiles {
        if is_active && !active_tools.insert(tool) {
            return Err(AppError::conflict(
                "activeProfile",
                "同一工具同时只能有一个生效档案",
            ));
        }
    }
    Ok(())
}

/// 对即将批量保存的名称做与 SQLite `NOCASE` 索引一致的预校验。
pub fn validate_unique_names<'a>(
    names: impl IntoIterator<Item = (Option<Tool>, &'a ArtifactName)>,
) -> Result<(), AppError> {
    let mut seen = HashSet::new();
    for (tool, name) in names {
        let key = (tool, name.uniqueness_key());
        if !seen.insert(key) {
            return Err(AppError::conflict("name", "名称已存在"));
        }
    }
    Ok(())
}

/// 项目 assignment 不保存已由同一工具全局继承的资源。
pub fn validate_project_assignment(is_globally_assigned: bool) -> Result<(), AppError> {
    if is_globally_assigned {
        return Err(AppError::conflict(
            "assignment",
            "该资源已由全局分配继承，不能重复添加到项目",
        ));
    }
    Ok(())
}

/// 新增全局 assignment 前必须先清理同一工具下已有的项目重复项。
pub fn validate_global_assignment(has_project_assignments: bool) -> Result<(), AppError> {
    if has_project_assignments {
        return Err(AppError::conflict(
            "assignment",
            "该资源仍有项目分配，不能直接创建重复的全局分配",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        validate_global_assignment, validate_project_assignment, validate_single_active_profile,
        validate_unique_names, ArtifactKind, ArtifactName, ChangeKind, EntityId, HookEvent,
        McpTransport, ProjectRoot, Scope, SkillStatus, SyncRunKind, SyncRunStatus, SyncStatus,
        TargetType, Tool, TrustStatus,
    };

    #[test]
    fn stable_enums_serialize_to_contract_values() {
        assert_eq!(
            Tool::ALL.map(Tool::as_str),
            ["claude", "codex", "cursor", "zcode", "opencode"]
        );
        let values = [
            serde_json::to_value(Tool::Claude).unwrap(),
            serde_json::to_value(Tool::Codex).unwrap(),
            serde_json::to_value(Tool::Cursor).unwrap(),
            serde_json::to_value(Tool::Zcode).unwrap(),
            serde_json::to_value(Tool::Opencode).unwrap(),
            serde_json::to_value(Scope::Global).unwrap(),
            serde_json::to_value(Scope::Project).unwrap(),
            serde_json::to_value(ArtifactKind::Provider).unwrap(),
            serde_json::to_value(ArtifactKind::Prompt).unwrap(),
            serde_json::to_value(ArtifactKind::Mcp).unwrap(),
            serde_json::to_value(ArtifactKind::Skill).unwrap(),
            serde_json::to_value(ArtifactKind::Hook).unwrap(),
            serde_json::to_value(SyncStatus::InSync).unwrap(),
            serde_json::to_value(SyncStatus::ExternalNonOwnedChange).unwrap(),
            serde_json::to_value(SyncStatus::ExternalOwnedChange).unwrap(),
            serde_json::to_value(SyncStatus::Missing).unwrap(),
            serde_json::to_value(SyncStatus::ParseError).unwrap(),
            serde_json::to_value(SyncStatus::PermissionDenied).unwrap(),
            serde_json::to_value(SyncStatus::PolicyBlocked).unwrap(),
            serde_json::to_value(SyncStatus::Untrusted).unwrap(),
            serde_json::to_value(SyncStatus::TargetTypeChanged).unwrap(),
            serde_json::to_value(SyncStatus::Failed).unwrap(),
            serde_json::to_value(ChangeKind::Add).unwrap(),
            serde_json::to_value(ChangeKind::Update).unwrap(),
            serde_json::to_value(ChangeKind::Delete).unwrap(),
            serde_json::to_value(ChangeKind::Unchanged).unwrap(),
            serde_json::to_value(ChangeKind::Warning).unwrap(),
            serde_json::to_value(ChangeKind::Conflict).unwrap(),
            serde_json::to_value(SyncRunKind::Preview).unwrap(),
            serde_json::to_value(SyncRunKind::Apply).unwrap(),
            serde_json::to_value(SyncRunKind::Restore).unwrap(),
            serde_json::to_value(SyncRunStatus::Previewed).unwrap(),
            serde_json::to_value(SyncRunStatus::Applying).unwrap(),
            serde_json::to_value(SyncRunStatus::Restoring).unwrap(),
            serde_json::to_value(SyncRunStatus::Succeeded).unwrap(),
            serde_json::to_value(SyncRunStatus::Failed).unwrap(),
            serde_json::to_value(SyncRunStatus::Stale).unwrap(),
            serde_json::to_value(SyncRunStatus::RolledBack).unwrap(),
            serde_json::to_value(SyncRunStatus::RollbackFailed).unwrap(),
            serde_json::to_value(McpTransport::Stdio).unwrap(),
            serde_json::to_value(McpTransport::StreamableHttp).unwrap(),
            serde_json::to_value(TrustStatus::Unknown).unwrap(),
            serde_json::to_value(TrustStatus::Trusted).unwrap(),
            serde_json::to_value(TrustStatus::Untrusted).unwrap(),
            serde_json::to_value(SkillStatus::Ready).unwrap(),
            serde_json::to_value(SkillStatus::Invalid).unwrap(),
            serde_json::to_value(SkillStatus::Missing).unwrap(),
            serde_json::to_value(TargetType::File).unwrap(),
            serde_json::to_value(TargetType::Directory).unwrap(),
            serde_json::to_value(TargetType::Symlink).unwrap(),
            serde_json::to_value(TargetType::Missing).unwrap(),
            serde_json::to_value(HookEvent::SessionStart).unwrap(),
            serde_json::to_value(HookEvent::SessionEnd).unwrap(),
            serde_json::to_value(HookEvent::UserPromptSubmit).unwrap(),
            serde_json::to_value(HookEvent::PreToolUse).unwrap(),
            serde_json::to_value(HookEvent::PermissionRequest).unwrap(),
            serde_json::to_value(HookEvent::PostToolUse).unwrap(),
            serde_json::to_value(HookEvent::PostToolUseFailure).unwrap(),
            serde_json::to_value(HookEvent::SubagentStart).unwrap(),
            serde_json::to_value(HookEvent::SubagentStop).unwrap(),
            serde_json::to_value(HookEvent::PreCompact).unwrap(),
            serde_json::to_value(HookEvent::PostCompact).unwrap(),
            serde_json::to_value(HookEvent::Stop).unwrap(),
            serde_json::to_value(HookEvent::Notification).unwrap(),
        ];
        let expected = [
            "claude",
            "codex",
            "cursor",
            "zcode",
            "opencode",
            "global",
            "project",
            "provider",
            "prompt",
            "mcp",
            "skill",
            "hook",
            "in_sync",
            "external_non_owned_change",
            "external_owned_change",
            "missing",
            "parse_error",
            "permission_denied",
            "policy_blocked",
            "untrusted",
            "target_type_changed",
            "failed",
            "add",
            "update",
            "delete",
            "unchanged",
            "warning",
            "conflict",
            "preview",
            "apply",
            "restore",
            "previewed",
            "applying",
            "restoring",
            "succeeded",
            "failed",
            "stale",
            "rolled_back",
            "rollback_failed",
            "stdio",
            "streamable_http",
            "unknown",
            "trusted",
            "untrusted",
            "ready",
            "invalid",
            "missing",
            "file",
            "directory",
            "symlink",
            "missing",
            "SessionStart",
            "SessionEnd",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "PostToolUse",
            "PostToolUseFailure",
            "SubagentStart",
            "SubagentStop",
            "PreCompact",
            "PostCompact",
            "Stop",
            "Notification",
        ];
        assert_eq!(
            values,
            expected.map(|value| serde_json::Value::String(value.to_owned()))
        );
    }

    #[test]
    fn entity_ids_are_uuid_values() {
        let id = EntityId::new();
        assert_eq!(id.to_string().len(), 36);
        assert!(EntityId::parse(&id.to_string()).is_ok());
        assert!(EntityId::parse("not-a-uuid").is_err());
    }

    #[test]
    fn artifact_name_rejects_ambiguous_values() {
        assert!(ArtifactName::parse("").is_err());
        assert!(ArtifactName::parse(" 带空格").is_err());
        assert!(ArtifactName::parse("带\n换行").is_err());
        assert_eq!(
            ArtifactName::parse("生产渠道").unwrap().as_str(),
            "生产渠道"
        );
    }

    #[test]
    fn project_root_requires_a_normalized_absolute_path() {
        assert!(ProjectRoot::parse(Path::new("relative/project")).is_err());
        assert!(ProjectRoot::parse(Path::new("/")).is_err());
        assert!(ProjectRoot::parse(Path::new("/tmp/../secret")).is_err());
        assert_eq!(
            ProjectRoot::parse(Path::new("/tmp/project"))
                .unwrap()
                .as_str(),
            "/tmp/project"
        );
    }

    #[test]
    fn serialized_value_objects_cannot_bypass_validated_domain_types() {
        assert!(serde_json::from_str::<ArtifactName>("\" leading\"").is_err());
        assert!(serde_json::from_str::<ProjectRoot>("\"/\"").is_err());
        assert_eq!(
            serde_json::from_str::<ArtifactName>("\"有效名称\"")
                .unwrap()
                .as_str(),
            "有效名称"
        );
    }

    #[test]
    fn active_and_unique_invariants_fail_before_database_access() {
        assert!(
            validate_single_active_profile([(Tool::Claude, true), (Tool::Claude, true),]).is_err()
        );
        assert!(
            validate_single_active_profile([(Tool::Claude, true), (Tool::Codex, true),]).is_ok()
        );

        let first = ArtifactName::parse("Same").unwrap();
        let second = ArtifactName::parse("same").unwrap();
        assert!(validate_unique_names([
            (Some(Tool::Claude), &first),
            (Some(Tool::Claude), &second)
        ])
        .is_err());
        assert!(validate_unique_names([
            (Some(Tool::Claude), &first),
            (Some(Tool::Codex), &second)
        ])
        .is_ok());
    }

    #[test]
    fn inherited_assignment_is_rejected_by_domain_rule() {
        assert!(validate_project_assignment(true).is_err());
        assert!(validate_project_assignment(false).is_ok());
        assert!(validate_global_assignment(true).is_err());
        assert!(validate_global_assignment(false).is_ok());
    }
}
