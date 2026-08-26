# Error Handling

> How errors are handled in this project.

---

## Overview

Backend and RPC failures use the stable `AppError` contract from
`src-tauri/src/error.rs`. Runtime values never enter the user-facing message;
they may only appear in allowlisted, redacted details.

## Error Types

- `ErrorCode` values are stable serialized strings and must stay synchronized
  with SQLite `error_code` checks and generated TypeScript bindings.
- `AppError.message` is a compile-time static Chinese message.
- `AppError.details` is private and can only be populated through the per-code
  allowlist plus `SecretRedactor`.

## Error Handling Patterns

- Convert I/O and SQLite failures to stable operations such as `open`, `lstat`,
  or `enable_wal`; never copy raw OS/SQLite messages into RPC details.
- Propagate `AppError` with `?`. Startup display/logging may use its `Display`
  implementation, which intentionally omits details.
- Tauri commands return `Result<T, AppError>` and translate a poisoned shared
  state lock to the stable `WRITE_IN_PROGRESS` code before delegating.

```rust
#[tauri::command]
#[specta::specta]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<ProjectDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::get_project(&database, state.environment()?, &id)
}
```

## API Error Responses

The generated RPC shape is `{ code, message, details?, recoverable, action? }`.
Regenerate and check `src/bindings/commands.ts` whenever this contract changes.

## Common Mistakes

- Do not derive or expose an alternate RPC error payload.
- Do not accept arbitrary detail keys or dynamic error messages.
- Registered secrets that look like JSON scalars (for example `42` or `null`)
  must be replaced before JSON parsing so they cannot bypass redaction.
- 不用 `redact_text(value) != value` 作为凭据证据；展示隐藏和 JSON 规范化也会改变文本。
  MCP 普通字段使用 `contains_secret`，具体拒绝原因仅含固定 field/reason，不能包含原生值。
- Do not catch an `AppError` and return an ordinary success value. Preserve its
  stable code, recovery metadata, and redacted details through the RPC boundary.

## Construction Example

Use a static message plus allowlisted detail values. `AppError::invalid_input`
is the reference pattern from `src-tauri/src/error.rs`:

```rust
pub fn invalid_input(field: &'static str, reason: &'static str) -> Self {
    Self::new(ErrorCode::InvalidInput, "输入内容无效", true).with_safe_details([
        ("field", Value::String(field.to_owned())),
        ("reason", Value::String(reason.to_owned())),
    ])
}
```
