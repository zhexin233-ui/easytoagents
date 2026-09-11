//! 跨层稳定错误合同。

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::security::SecretRedactor;

/// RPC、journal 和同步记录共用的稳定错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
pub enum ErrorCode {
    #[serde(rename = "NOT_FOUND")]
    NotFound,
    #[serde(rename = "INVALID_INPUT")]
    InvalidInput,
    #[serde(rename = "PARSE_ERROR")]
    ParseError,
    #[serde(rename = "PERMISSION_DENIED")]
    PermissionDenied,
    #[serde(rename = "POLICY_BLOCKED")]
    PolicyBlocked,
    #[serde(rename = "UNTRUSTED_PROJECT")]
    UntrustedProject,
    #[serde(rename = "CONFLICT")]
    Conflict,
    #[serde(rename = "STALE_PREVIEW")]
    StalePreview,
    #[serde(rename = "PREVIEW_ALREADY_CONSUMED")]
    PreviewAlreadyConsumed,
    #[serde(rename = "WRITE_IN_PROGRESS")]
    WriteInProgress,
    #[serde(rename = "ATOMIC_WRITE_FAILED")]
    AtomicWriteFailed,
    #[serde(rename = "ROLLBACK_FAILED")]
    RollbackFailed,
    #[serde(rename = "SECRET_REDACTED")]
    SecretRedacted,
    #[serde(rename = "DATABASE_ERROR")]
    DatabaseError,
    #[serde(rename = "MIGRATION_FAILED")]
    MigrationFailed,
    #[serde(rename = "PERMISSION_AUDIT_FAILED")]
    PermissionAuditFailed,
    /// 工具环境仍在后台探测；只出现在命令边界，永不写入 sync_runs.error_code。
    #[serde(rename = "ENVIRONMENT_PROBING")]
    EnvironmentProbing,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::InvalidInput => "INVALID_INPUT",
            Self::ParseError => "PARSE_ERROR",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::PolicyBlocked => "POLICY_BLOCKED",
            Self::UntrustedProject => "UNTRUSTED_PROJECT",
            Self::Conflict => "CONFLICT",
            Self::StalePreview => "STALE_PREVIEW",
            Self::PreviewAlreadyConsumed => "PREVIEW_ALREADY_CONSUMED",
            Self::WriteInProgress => "WRITE_IN_PROGRESS",
            Self::AtomicWriteFailed => "ATOMIC_WRITE_FAILED",
            Self::RollbackFailed => "ROLLBACK_FAILED",
            Self::SecretRedacted => "SECRET_REDACTED",
            Self::DatabaseError => "DATABASE_ERROR",
            Self::MigrationFailed => "MIGRATION_FAILED",
            Self::PermissionAuditFailed => "PERMISSION_AUDIT_FAILED",
            Self::EnvironmentProbing => "ENVIRONMENT_PROBING",
        }
    }

    /// sync_runs / sync_items 的 `error_code` CHECK 只接受首批稳定码；后加的
    /// 命令边界码不能写库，落盘时折叠为语义最近的持久化码。
    pub const fn persisted(self) -> Self {
        match self {
            Self::EnvironmentProbing => Self::AtomicWriteFailed,
            other => other,
        }
    }

    pub fn from_stable_str(value: &str) -> Option<Self> {
        match value {
            "NOT_FOUND" => Some(Self::NotFound),
            "INVALID_INPUT" => Some(Self::InvalidInput),
            "PARSE_ERROR" => Some(Self::ParseError),
            "PERMISSION_DENIED" => Some(Self::PermissionDenied),
            "POLICY_BLOCKED" => Some(Self::PolicyBlocked),
            "UNTRUSTED_PROJECT" => Some(Self::UntrustedProject),
            "CONFLICT" => Some(Self::Conflict),
            "STALE_PREVIEW" => Some(Self::StalePreview),
            "PREVIEW_ALREADY_CONSUMED" => Some(Self::PreviewAlreadyConsumed),
            "WRITE_IN_PROGRESS" => Some(Self::WriteInProgress),
            "ATOMIC_WRITE_FAILED" => Some(Self::AtomicWriteFailed),
            "ROLLBACK_FAILED" => Some(Self::RollbackFailed),
            "SECRET_REDACTED" => Some(Self::SecretRedacted),
            "DATABASE_ERROR" => Some(Self::DatabaseError),
            "MIGRATION_FAILED" => Some(Self::MigrationFailed),
            "PERMISSION_AUDIT_FAILED" => Some(Self::PermissionAuditFailed),
            "ENVIRONMENT_PROBING" => Some(Self::EnvironmentProbing),
            _ => None,
        }
    }

    fn detail_allowlist(self) -> &'static [&'static str] {
        match self {
            Self::NotFound => &["resource", "id", "path"],
            Self::InvalidInput => &["field", "reason"],
            Self::ParseError => &["path", "format", "line", "column"],
            Self::PermissionDenied | Self::PermissionAuditFailed => {
                &["path", "operation", "expectedMode", "actualMode"]
            }
            Self::PolicyBlocked => &["tool", "path", "policy"],
            Self::UntrustedProject => &["tool", "path"],
            Self::Conflict => &["field", "resource", "target", "path", "reason"],
            Self::StalePreview => &["previewId", "target", "path"],
            Self::PreviewAlreadyConsumed => &["previewId", "status"],
            Self::WriteInProgress => &["runId", "status"],
            Self::AtomicWriteFailed => &["path", "operation"],
            Self::RollbackFailed => &["runId", "path", "snapshotId"],
            Self::SecretRedacted => &["field"],
            Self::DatabaseError | Self::MigrationFailed => &["path", "operation", "version"],
            Self::EnvironmentProbing => &[],
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    Rescan,
    ReviewConflict,
    Restore,
    FixPermissions,
}

/// 只有构造函数能写入 details，确保 allowlist 与统一脱敏无法被绕过。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    code: ErrorCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<BTreeMap<String, Value>>,
    recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<RecoveryAction>,
    /// 底层错误的脱敏文本，只进日志与 journal 诊断字段；永不出 RPC 边界，
    /// 也不参与相等比较（同一稳定错误无论底层原因都视为同一错误）。
    #[serde(skip)]
    #[specta(skip)]
    source: Option<String>,
}

impl PartialEq for AppError {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
            && self.message == other.message
            && self.details == other.details
            && self.recoverable == other.recoverable
            && self.action == other.action
    }
}

impl AppError {
    /// 错误消息必须是编译期固定文案；运行时数据只能进入脱敏后的 details。
    pub fn new(code: ErrorCode, message: &'static str, recoverable: bool) -> Self {
        Self {
            code,
            message: message.to_owned(),
            details: None,
            recoverable,
            action: None,
            source: None,
        }
    }

    /// 代码不变量被打破时的稳定错误：替代生产路径上的 panic 断言。
    pub fn internal(reason: &'static str) -> Self {
        Self::new(ErrorCode::AtomicWriteFailed, "应用内部状态异常", false)
            .with_safe_details([("operation", Value::String("internal".to_owned()))])
            .with_source(reason)
    }

    /// 附带底层错误原因：先做脱敏再记录，并以 warn 级别写日志。
    /// 文案仍是编译期固定的 `message`，`source` 只服务于诊断。
    pub fn with_source(self, error: impl fmt::Display) -> Self {
        self.with_source_redacted(error, &SecretRedactor::default())
    }

    /// 使用调用方已经登记了原生配置凭据的脱敏器附加底层原因。
    /// 这条路径供持有 `AppState` 脱敏器的服务使用；`with_source` 保留给启动期和
    /// 没有共享脱敏器的基础设施错误。
    pub fn with_source_redacted(
        mut self,
        error: impl fmt::Display,
        redactor: &SecretRedactor,
    ) -> Self {
        // 构造器通常在拿到共享 redactor 之前用默认 redactor 填充 details；
        // 调用方随后提供更完整的凭据集合时，必须连同既有 details 一并收紧，
        // 否则 source/log 虽安全，RPC 的 path/operation 仍可能泄漏注册值。
        if let Some(details) = self.details.as_mut() {
            for value in details.values_mut() {
                *value = redactor.redact_structure(value).into_value();
            }
        }
        let redacted = redactor.redact_text(&error.to_string());
        let operation = self
            .detail_text("operation")
            .map(|value| redactor.redact_text(value))
            .unwrap_or_default();
        let path = self
            .detail_text("path")
            .map(|value| redactor.redact_text(value))
            .unwrap_or_default();
        tracing::warn!(
            code = %self.code,
            operation = %operation,
            path = %path,
            source = %redacted,
            "{}",
            self.message
        );
        self.source = Some(redacted);
        self
    }

    /// 脱敏后的底层原因；仅供日志与 journal 诊断使用。
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    fn detail_text(&self, key: &str) -> Option<&str> {
        self.details
            .as_ref()
            .and_then(|details| details.get(key))
            .and_then(Value::as_str)
    }

    pub fn invalid_input(field: &'static str, reason: &'static str) -> Self {
        Self::new(ErrorCode::InvalidInput, "输入内容无效", true).with_safe_details([
            ("field", Value::String(field.to_owned())),
            ("reason", Value::String(reason.to_owned())),
        ])
    }

    pub fn conflict(field: &'static str, reason: &'static str) -> Self {
        Self::new(ErrorCode::Conflict, "检测到配置冲突", true)
            .with_action(RecoveryAction::ReviewConflict)
            .with_safe_details([
                ("field", Value::String(field.to_owned())),
                ("reason", Value::String(reason.to_owned())),
            ])
    }

    pub fn not_found(resource: &'static str, path: &str) -> Self {
        Self::new(ErrorCode::NotFound, "未找到目标资源", true)
            .with_action(RecoveryAction::Rescan)
            .with_safe_details([
                ("resource", Value::String(resource.to_owned())),
                ("path", Value::String(path.to_owned())),
            ])
    }

    pub fn parse(path: &str, format: &'static str) -> Self {
        Self::new(ErrorCode::ParseError, "原生配置格式无法解析", true)
            .with_action(RecoveryAction::ReviewConflict)
            .with_safe_details([
                ("path", Value::String(path.to_owned())),
                ("format", Value::String(format.to_owned())),
            ])
    }

    pub fn policy_blocked(tool: &'static str, path: &str, policy: &'static str) -> Self {
        Self::new(ErrorCode::PolicyBlocked, "目标被工具管理策略阻止", true)
            .with_action(RecoveryAction::Rescan)
            .with_safe_details([
                ("tool", Value::String(tool.to_owned())),
                ("path", Value::String(path.to_owned())),
                ("policy", Value::String(policy.to_owned())),
            ])
    }

    pub fn untrusted_project(tool: &'static str, path: &str) -> Self {
        Self::new(ErrorCode::UntrustedProject, "项目尚未被工具信任", true)
            .with_action(RecoveryAction::Rescan)
            .with_safe_details([
                ("tool", Value::String(tool.to_owned())),
                ("path", Value::String(path.to_owned())),
            ])
    }

    pub fn stale_preview(preview_id: &str, target: &str) -> Self {
        Self::new(ErrorCode::StalePreview, "预览依赖的数据库版本已变化", true)
            .with_action(RecoveryAction::Rescan)
            .with_safe_details([
                ("previewId", Value::String(preview_id.to_owned())),
                ("target", Value::String(target.to_owned())),
            ])
    }

    pub fn preview_already_consumed(preview_id: &str, status: &str) -> Self {
        Self::new(ErrorCode::PreviewAlreadyConsumed, "该预览已经被消费", true)
            .with_action(RecoveryAction::Rescan)
            .with_safe_details([
                ("previewId", Value::String(preview_id.to_owned())),
                ("status", Value::String(status.to_owned())),
            ])
    }

    pub fn write_in_progress(run_id: &str, status: &str) -> Self {
        Self::new(ErrorCode::WriteInProgress, "已有写入或恢复正在进行", true)
            .with_action(RecoveryAction::Restore)
            .with_safe_details([
                ("runId", Value::String(run_id.to_owned())),
                ("status", Value::String(status.to_owned())),
            ])
    }

    pub fn atomic_write(path: &str, operation: &str) -> Self {
        Self::new(ErrorCode::AtomicWriteFailed, "原子写入失败", true)
            .with_action(RecoveryAction::Restore)
            .with_safe_details([
                ("path", Value::String(path.to_owned())),
                ("operation", Value::String(operation.to_owned())),
            ])
    }

    pub fn rollback_failed(run_id: &str, path: &str, snapshot_id: &str) -> Self {
        Self::new(ErrorCode::RollbackFailed, "自动回滚失败", true)
            .with_action(RecoveryAction::Restore)
            .with_safe_details([
                ("runId", Value::String(run_id.to_owned())),
                ("path", Value::String(path.to_owned())),
                ("snapshotId", Value::String(snapshot_id.to_owned())),
            ])
    }

    pub fn permission(path: &str, operation: &str) -> Self {
        Self::new(ErrorCode::PermissionDenied, "目标路径权限不足", true)
            .with_action(RecoveryAction::FixPermissions)
            .with_safe_details([
                ("path", Value::String(path.to_owned())),
                ("operation", Value::String(operation.to_owned())),
            ])
    }

    pub fn database(path: &str, operation: &str) -> Self {
        Self::new(ErrorCode::DatabaseError, "本地数据库操作失败", false).with_safe_details([
            ("path", Value::String(path.to_owned())),
            ("operation", Value::String(operation.to_owned())),
        ])
    }

    /// 把 SQLite 原因留在仅供诊断的 `source` 字段；RPC details 仍只有稳定路径和操作。
    pub fn database_from(path: &str, operation: &str, error: &rusqlite::Error) -> Self {
        Self::database(path, operation).with_source(error)
    }

    /// 把文件系统原因留在仅供诊断的 `source` 字段；不把 OS 文案复制进 RPC。
    pub fn io_from(path: &str, operation: &str, error: &std::io::Error) -> Self {
        Self::atomic_write(path, operation).with_source(error)
    }

    /// 序列化/反序列化失败没有独立的稳定 RPC 错误码，沿用解析错误合同并保留 source。
    pub fn serialization_from(path: &str, format: &'static str, error: impl fmt::Display) -> Self {
        Self::parse(path, format).with_source(error)
    }

    pub fn migration(path: &str, version: i64) -> Self {
        Self::new(ErrorCode::MigrationFailed, "本地数据库迁移失败", false).with_safe_details([
            ("path", Value::String(path.to_owned())),
            ("version", Value::Number(version.into())),
        ])
    }

    pub fn with_action(mut self, action: RecoveryAction) -> Self {
        self.action = Some(action);
        self
    }

    /// 仅接收业务结构，按错误码 allowlist 后再统一脱敏。
    pub fn with_redacted_details<I, K>(mut self, details: I, redactor: &SecretRedactor) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        let allowlist = self.code.detail_allowlist();
        let filtered = details
            .into_iter()
            .filter_map(|(key, value)| {
                let key = key.into();
                allowlist
                    .contains(&key.as_str())
                    .then(|| (key, redactor.redact_structure(&value).into_value()))
            })
            .collect::<BTreeMap<_, _>>();
        self.details = (!filtered.is_empty()).then_some(filtered);
        self
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn details(&self) -> Option<&BTreeMap<String, Value>> {
        self.details.as_ref()
    }

    pub fn recoverable(&self) -> bool {
        self.recoverable
    }

    pub fn action(&self) -> Option<RecoveryAction> {
        self.action
    }

    fn with_safe_details<I, K>(self, details: I) -> Self
    where
        I: IntoIterator<Item = (K, Value)>,
        K: Into<String>,
    {
        self.with_redacted_details(details, &SecretRedactor::default())
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{AppError, ErrorCode, RecoveryAction};
    use crate::security::SecretRedactor;

    #[test]
    fn details_are_allowlisted_and_redacted_before_serialization() {
        let mut redactor = SecretRedactor::default();
        redactor.register_secret("fixture-secret");
        redactor.register_secret("42");
        let error = AppError::new(ErrorCode::InvalidInput, "输入内容无效", true)
            .with_redacted_details(
                [
                    ("field", json!("apiKey")),
                    (
                        "reason",
                        json!({ "message": "值 fixture-secret 不合法", "code": "42" }),
                    ),
                    ("rawPayload", json!({ "token": "fixture-secret" })),
                ],
                &redactor,
            );

        let serialized = serde_json::to_string(&error).unwrap();
        assert!(!serialized.contains("fixture-secret"));
        assert!(!serialized.contains("rawPayload"));
        assert_eq!(
            error.details().unwrap()["reason"]["message"],
            "值 [REDACTED] 不合法"
        );
        assert_eq!(error.details().unwrap()["reason"]["code"], "[REDACTED]");
    }

    #[test]
    fn error_codes_and_recovery_actions_have_stable_serialized_values() {
        let codes = [
            ErrorCode::NotFound,
            ErrorCode::InvalidInput,
            ErrorCode::ParseError,
            ErrorCode::PermissionDenied,
            ErrorCode::PolicyBlocked,
            ErrorCode::UntrustedProject,
            ErrorCode::Conflict,
            ErrorCode::StalePreview,
            ErrorCode::PreviewAlreadyConsumed,
            ErrorCode::WriteInProgress,
            ErrorCode::AtomicWriteFailed,
            ErrorCode::RollbackFailed,
            ErrorCode::SecretRedacted,
            ErrorCode::DatabaseError,
            ErrorCode::MigrationFailed,
            ErrorCode::PermissionAuditFailed,
            ErrorCode::EnvironmentProbing,
        ];
        for code in codes {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                Value::String(code.as_str().to_owned())
            );
            assert_eq!(ErrorCode::from_stable_str(code.as_str()), Some(code));
        }
        assert_eq!(ErrorCode::from_stable_str("UNKNOWN"), None);
        assert_eq!(
            serde_json::to_value([
                RecoveryAction::Rescan,
                RecoveryAction::ReviewConflict,
                RecoveryAction::Restore,
                RecoveryAction::FixPermissions,
            ])
            .unwrap(),
            json!(["rescan", "review_conflict", "restore", "fix_permissions"])
        );
    }

    #[test]
    fn source_is_diagnostic_only_and_uses_the_callers_redactor() {
        let mut redactor = SecretRedactor::default();
        redactor.register_secret("fixture-api-key");
        let error = AppError::database("fixture-api-key.sqlite3", "read_fixture-api-key")
            .with_source_redacted("database rejected fixture-api-key", &redactor);

        assert_eq!(error.source(), Some("database rejected [REDACTED]"));
        let serialized = serde_json::to_value(&error).unwrap();
        assert!(serialized.get("source").is_none());
        assert!(!serialized.to_string().contains("fixture-api-key"));
    }
}
