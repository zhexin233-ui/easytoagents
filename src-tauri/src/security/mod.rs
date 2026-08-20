//! 脱敏与应用私有路径权限边界。

use std::{
    collections::HashSet,
    fmt,
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::AppError;

pub const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
pub const PRIVATE_FILE_MODE: u32 = 0o600;
pub const REDACTED: &str = "[REDACTED]";

/// 日志、journal、RPC 和崩溃上下文可安全持有的 JSON 包装。
///
/// 内部值不可由调用方直接构造，只能经 `SecretRedactor` 生成。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RedactedJson(Value);

impl RedactedJson {
    pub fn as_value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }
}

/// selector 使用点分路径，`*` 匹配一个对象键或数组下标。
#[derive(Clone, Default)]
pub struct SecretRedactor {
    secrets: Vec<String>,
    selectors: Vec<Vec<String>>,
}

impl fmt::Debug for SecretRedactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretRedactor")
            .field("registered_secret_count", &self.secrets.len())
            .field("selector_count", &self.selectors.len())
            .finish()
    }
}

impl SecretRedactor {
    pub fn register_secret(&mut self, secret: impl Into<String>) {
        let secret = secret.into();
        if secret.is_empty() || secret == REDACTED || self.secrets.contains(&secret) {
            return;
        }
        self.secrets.push(secret);
        self.secrets
            .sort_by_key(|candidate| std::cmp::Reverse(candidate.len()));
    }

    pub fn register_selector(&mut self, selector: &str) {
        let segments = selector
            .trim_start_matches('$')
            .trim_start_matches('.')
            .split(['.', '/'])
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if !segments.is_empty() && !self.selectors.contains(&segments) {
            self.selectors.push(segments);
        }
    }

    pub fn redact_text(&self, text: &str) -> String {
        // 先替换登记值，避免 `42`、`null` 等短秘密先被解析成 JSON 标量而绕过脱敏。
        let mut redacted = text.to_owned();
        for secret in &self.secrets {
            redacted = redacted.replace(secret, REDACTED);
        }

        if let Ok(json) = serde_json::from_str::<Value>(&redacted) {
            return serde_json::to_string(&self.redact_json(&json))
                .unwrap_or_else(|_| REDACTED.to_owned());
        }

        if looks_like_secret_value(redacted.trim()) {
            REDACTED.to_owned()
        } else {
            redact_inline_secret_values(&redacted)
        }
    }

    pub fn redact_json(&self, value: &Value) -> Value {
        self.redact_json_at(value, &mut Vec::new(), false)
    }

    pub fn redact_structure(&self, value: &Value) -> RedactedJson {
        RedactedJson(self.redact_json(value))
    }

    fn redact_json_at(&self, value: &Value, path: &mut Vec<String>, forced: bool) -> Value {
        match value {
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .map(|(key, value)| {
                        path.push(key.to_ascii_lowercase());
                        let child_forced = forced
                            || is_sensitive_key(key)
                            || is_sensitive_container(key)
                            || self.selector_matches(path);
                        let value = self.redact_json_at(value, path, child_forced);
                        path.pop();
                        (key.clone(), value)
                    })
                    .collect::<Map<_, _>>(),
            ),
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        path.push(index.to_string());
                        let child_forced = forced || self.selector_matches(path);
                        let value = self.redact_json_at(value, path, child_forced);
                        path.pop();
                        value
                    })
                    .collect(),
            ),
            Value::String(text) if forced => Value::String(REDACTED.to_owned()),
            Value::String(text) => Value::String(self.redact_text(text)),
            Value::Null => Value::Null,
            Value::Bool(_) | Value::Number(_) if forced => Value::String(REDACTED.to_owned()),
            Value::Bool(value) => Value::Bool(*value),
            Value::Number(value) => Value::Number(value.clone()),
        }
    }

    fn selector_matches(&self, path: &[String]) -> bool {
        self.selectors.iter().any(|selector| {
            selector.len() <= path.len()
                && selector
                    .iter()
                    .zip(path)
                    .all(|(expected, actual)| expected == "*" || expected == actual)
        })
    }
}

fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let key = normalize_key(key);
    [
        "authorization",
        "apikey",
        "token",
        "secret",
        "password",
        "cookie",
        "credential",
        "bearer",
    ]
    .iter()
    .any(|marker| key.contains(marker))
}

fn is_sensitive_container(key: &str) -> bool {
    matches!(
        normalize_key(key).as_str(),
        "headers" | "env" | "environment"
    )
}

fn looks_like_secret_value(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    lowercase.starts_with("bearer ")
        || (lowercase.starts_with("sk-") && value.len() >= 12)
        || (lowercase.starts_with("api_key=") && value.len() > 8)
        || (lowercase.starts_with("token=") && value.len() > 6)
}

/// 判断普通 RPC 扩展字段是否包含按键名或值形态可识别的秘密。
///
/// 原生配置导入可以把未知字段留在私有数据库中，但把字段作为普通 DTO 返回前，
/// 必须先用同一套检测规则证明它不是已识别秘密。
pub(crate) fn contains_detectable_secret(key: &str, value: &str) -> bool {
    is_sensitive_key(key)
        || looks_like_secret_value(value.trim())
        || redact_inline_secret_values(value) != value
}

fn redact_inline_secret_values(value: &str) -> String {
    let mut redacted = redact_tokens_after_pattern(value, "bearer ", false);
    for marker in [
        "authorization",
        "api_key",
        "api-key",
        "api key",
        "apikey",
        "token",
        "secret",
        "password",
        "cookie",
    ] {
        redacted = redact_tokens_after_pattern(&redacted, marker, true);
    }
    redacted
}

fn redact_tokens_after_pattern(value: &str, pattern: &str, requires_assignment: bool) -> String {
    let lowercase = value.to_ascii_lowercase();
    let mut output = String::with_capacity(value.len());
    let mut copied_until = 0;
    let mut search_from = 0;

    while let Some(relative_start) = lowercase[search_from..].find(pattern) {
        let start = search_from + relative_start;
        let pattern_end = start + pattern.len();
        if requires_assignment
            && ((start > 0
                && value[..start]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric))
                || value[pattern_end..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric))
        {
            search_from = pattern_end;
            continue;
        }

        let mut token_start = pattern_end;
        if requires_assignment {
            token_start += value[token_start..]
                .chars()
                .take_while(|character| character.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            let Some(separator) = value[token_start..].chars().next() else {
                break;
            };
            if !matches!(separator, ':' | '=') {
                search_from = pattern_end;
                continue;
            }
            token_start += separator.len_utf8();
            token_start += value[token_start..]
                .chars()
                .take_while(|character| character.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
        }

        let quote = value[token_start..]
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\''));
        if let Some(quote) = quote {
            token_start += quote.len_utf8();
        }
        for scheme in ["bearer ", "basic "] {
            if lowercase[token_start..].starts_with(scheme) {
                token_start += scheme.len();
                break;
            }
        }
        let token_end = value[token_start..]
            .char_indices()
            .find_map(|(offset, character)| {
                let is_end = if let Some(quote) = quote {
                    character == quote
                } else {
                    character.is_whitespace()
                        || matches!(character, ',' | ';' | '"' | '\'' | ')' | ']' | '}')
                };
                is_end.then_some(token_start + offset)
            })
            .unwrap_or(value.len());

        if token_end == token_start {
            search_from = pattern_end;
            continue;
        }
        output.push_str(&value[copied_until..token_start]);
        output.push_str(REDACTED);
        copied_until = token_end;
        search_from = token_end;
    }

    if copied_until == 0 {
        value.to_owned()
    } else {
        output.push_str(&value[copied_until..]);
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionEntry {
    pub path: PathBuf,
    pub mode: u32,
    pub is_directory: bool,
}

/// 创建或收紧应用拥有的目录。未知 symlink/特殊文件会阻止继续写入。
pub fn ensure_private_directory(path: &Path) -> Result<(), AppError> {
    validate_absolute_private_path(path)?;

    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push(Path::new("/")),
            Component::Normal(segment) => current.push(segment),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(permission_error(path, "reject_relative_component"));
            }
        }

        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(permission_error(&current, "reject_unsafe_ancestor"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|_| permission_error(&current, "create_directory"))?;
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|_| permission_error(&current, "lstat_created_directory"))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(permission_error(
                        &current,
                        "reject_unsafe_created_directory",
                    ));
                }
            }
            Err(_) => return Err(permission_error(&current, "lstat_ancestor")),
        }
    }

    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
        .map_err(|_| permission_error(path, "set_directory_permissions"))
}

/// 检查已有路径分量，防止尚未创建的私有叶节点经祖先 symlink 逃逸。
pub fn reject_symlink_components(path: &Path) -> Result<(), AppError> {
    validate_absolute_private_path(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push(Path::new("/")),
            Component::Normal(segment) => current.push(segment),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(permission_error(path, "reject_relative_component"));
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(permission_error(&current, "reject_symlink_component"));
            }
            Ok(metadata) if current != path && !metadata.is_dir() => {
                return Err(permission_error(&current, "reject_non_directory_ancestor"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(_) => return Err(permission_error(&current, "lstat_ancestor")),
        }
    }
    Ok(())
}

/// 以 `0600` 新建敏感文件；`create_new` 防止意外覆盖既有内容。
pub fn create_private_file(path: &Path) -> Result<File, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| permission_error(path, "resolve_parent"))?;
    ensure_private_directory(parent)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_FILE_MODE)
        .open(path)
        .map_err(|_| permission_error(path, "create_file"))?;
    file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))
        .map_err(|_| permission_error(path, "set_file_permissions"))?;
    Ok(file)
}

pub fn ensure_private_file(path: &Path) -> Result<(), AppError> {
    reject_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| permission_error(path, "lstat"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(permission_error(path, "validate_file_type"));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_FILE_MODE))
        .map_err(|_| permission_error(path, "set_file_permissions"))
}

/// 审计并修复应用私有树：目录 `0700`、普通文件 `0600`，拒绝链接和特殊文件。
pub fn audit_private_tree(root: &Path) -> Result<Vec<PermissionEntry>, AppError> {
    reject_symlink_components(root)?;
    let mut entries = Vec::new();
    let mut visited = HashSet::new();
    audit_private_tree_inner(root, &mut visited, &mut entries)?;
    Ok(entries)
}

fn audit_private_tree_inner(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
    entries: &mut Vec<PermissionEntry>,
) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| permission_error(path, "lstat"))?;
    if metadata.file_type().is_symlink() {
        return Err(permission_error(path, "reject_symlink"));
    }

    if metadata.is_dir() {
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(|_| permission_error(path, "set_directory_permissions"))?;
        let canonical =
            fs::canonicalize(path).map_err(|_| permission_error(path, "canonicalize"))?;
        if !visited.insert(canonical) {
            return Err(permission_error(path, "reject_directory_cycle"));
        }
        entries.push(PermissionEntry {
            path: path.to_owned(),
            mode: PRIVATE_DIRECTORY_MODE,
            is_directory: true,
        });
        let children = fs::read_dir(path).map_err(|_| permission_error(path, "read_directory"))?;
        for child in children {
            let child = child.map_err(|_| permission_error(path, "read_directory_entry"))?;
            audit_private_tree_inner(&child.path(), visited, entries)?;
        }
        return Ok(());
    }

    if metadata.is_file() {
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_FILE_MODE))
            .map_err(|_| permission_error(path, "set_file_permissions"))?;
        entries.push(PermissionEntry {
            path: path.to_owned(),
            mode: PRIVATE_FILE_MODE,
            is_directory: false,
        });
        return Ok(());
    }

    Err(permission_error(path, "reject_special_file"))
}

pub fn mode(path: &Path) -> io::Result<u32> {
    fs::symlink_metadata(path).map(|metadata| metadata.permissions().mode() & 0o777)
}

fn validate_absolute_private_path(path: &Path) -> Result<(), AppError> {
    if !path.is_absolute() || path == Path::new("/") {
        return Err(permission_error(path, "reject_dangerous_path"));
    }
    Ok(())
}

fn permission_error(path: &Path, operation: &str) -> AppError {
    AppError::permission(&path.to_string_lossy(), operation)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
    };

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        audit_private_tree, ensure_private_directory, mode, SecretRedactor, PRIVATE_DIRECTORY_MODE,
        PRIVATE_FILE_MODE, REDACTED,
    };

    #[test]
    fn redactor_covers_registered_values_sensitive_keys_containers_and_selectors() {
        let mut redactor = SecretRedactor::default();
        redactor.register_secret("fixture-registered-secret");
        redactor.register_selector("extensions.*.privateValue");
        let input = json!({
            "apiKey": "key-value",
            "headers": { "X-Custom": "header-secret", "Authorization": "Bearer auth-secret" },
            "env": { "SAFE_NAME": "env-secret" },
            "extensions": [{ "privateValue": "selector-secret", "public": "kept" }],
            "nested": { "note": "contains fixture-registered-secret", "public": true }
        });

        let redacted = redactor.redact_json(&input);
        let serialized = serde_json::to_string(&redacted).unwrap();
        for secret in [
            "key-value",
            "header-secret",
            "auth-secret",
            "env-secret",
            "selector-secret",
            "fixture-registered-secret",
        ] {
            assert!(!serialized.contains(secret), "泄漏了测试秘密：{secret}");
        }
        assert_eq!(redacted["apiKey"], REDACTED);
        assert_eq!(redacted["nested"]["public"], true);
        assert_eq!(redacted["extensions"][0]["public"], "kept");

        let safe_structure = redactor.redact_structure(&input);
        assert!(!serde_json::to_string(&safe_structure)
            .unwrap()
            .contains("fixture-registered-secret"));
    }

    #[test]
    fn redactor_handles_nested_json_text_and_inline_authorization() {
        let redactor = SecretRedactor::default();
        let json_text = r#"{"outer":{"token":"never-log-this","safe":"ok"}}"#;
        let redacted = redactor.redact_text(json_text);
        assert!(!redacted.contains("never-log-this"));
        assert!(redacted.contains("ok"));

        let authorization = redactor.redact_text(
            "Authorization: Bearer first-secret; proxy=Bearer second-secret token=third-secret X-Authorization=Basic fourth-secret",
        );
        assert!(!authorization.contains("first-secret"));
        assert!(!authorization.contains("second-secret"));
        assert!(!authorization.contains("third-secret"));
        assert!(!authorization.contains("fourth-secret"));
    }

    #[test]
    fn redactor_handles_json_shaped_short_and_unicode_registered_secrets() {
        let mut redactor = SecretRedactor::default();
        for secret in ["42", "null", "密钥"] {
            redactor.register_secret(secret);
        }

        for value in ["42", "null", "前缀密钥后缀"] {
            assert!(!redactor.redact_text(value).contains(value));
        }
        let nested = json!({ "extra": { "value": "42", "说明": "密钥" } });
        let serialized = serde_json::to_string(&redactor.redact_json(&nested)).unwrap();
        assert!(!serialized.contains("42"));
        assert!(!serialized.contains("密钥"));
    }

    #[test]
    fn permission_audit_repairs_modes_inside_an_isolated_directory() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let root = isolated_root.join("Application Support/EasyToAgents");
        ensure_private_directory(&root).unwrap();
        let nested = root.join("snapshots/run-1");
        fs::create_dir_all(&nested).unwrap();
        let sensitive = nested.join("snapshot.bin");
        fs::write(&sensitive, b"fixture").unwrap();
        fs::set_permissions(&nested, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&sensitive, fs::Permissions::from_mode(0o644)).unwrap();

        audit_private_tree(&root).unwrap();

        assert_eq!(mode(&root).unwrap(), PRIVATE_DIRECTORY_MODE);
        assert_eq!(mode(&nested).unwrap(), PRIVATE_DIRECTORY_MODE);
        assert_eq!(mode(&sensitive).unwrap(), PRIVATE_FILE_MODE);
    }

    #[test]
    fn permission_audit_rejects_unknown_symlinks() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let root = isolated_root.join("private");
        ensure_private_directory(&root).unwrap();
        let outside = isolated_root.join("outside");
        fs::write(&outside, b"fixture").unwrap();
        symlink(&outside, root.join("unknown-link")).unwrap();

        assert!(audit_private_tree(&root).is_err());
    }

    #[test]
    fn private_directory_creation_rejects_a_symlinked_ancestor() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let outside = isolated_root.join("outside");
        fs::create_dir(&outside).unwrap();
        let linked_parent = isolated_root.join("linked-parent");
        symlink(&outside, &linked_parent).unwrap();

        assert!(ensure_private_directory(&linked_parent.join("private")).is_err());
        assert!(!outside.join("private").exists());
    }
}
