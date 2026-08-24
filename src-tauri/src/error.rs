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
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    code: ErrorCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<BTreeMap<String, Value>>,
    recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<RecoveryAction>,
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
        }
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
}
