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
  or `enable_wal`; never copy raw OS/SQLite messages into RPC details. Use
  `AppError::database_from` / `io_from` (or `serialization_from`) so the
  redacted source is retained for diagnostics without changing the RPC shape.
- Propagate `AppError` with `?`. Startup display/logging may use its `Display`
  implementation, which intentionally omits details.
- Tauri commands return `Result<T, AppError>` and use the shared `with_db` (or
  `with_db_and_redactor`) helper before delegating. `AppState` recovers a
  `PoisonError` from its process-local locks, so a panic while holding a lock
  does not turn the lock into a permanent RPC failure.
- `ENVIRONMENT_PROBING` is raised by `AppState::environment()` while the
  background tool probe is still running. It is recoverable, carries no details,
  and is deliberately **not** part of the `sync_runs.error_code` CHECK list
  because no run can start before the environment exists; never persist it.

```rust
#[tauri::command]
#[specta::specta]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<ProjectDto, AppError> {
    with_db(&state, |database| {
        projects::get_project(database, state.environment()?, &id)
    })
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
- `AppError::source` is `#[serde(skip)]` and never enters RPC or Specta output.
  `with_source` records a redacted diagnostic string and emits a structured
  `warn`; code with an application `SecretRedactor` should use
  `with_source_redacted` so registered credentials are covered too.
- For project-native Apply, inspect `sync_runs.status == "previewed"` **before** the
  disable/restore action matrix. A consumed disable preview would otherwise fail
  `Disabled+Disable` as `INVALID_INPUT` instead of `PREVIEW_ALREADY_CONSUMED`.

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

## Scenario: Poison-safe command diagnostics

### 1. Scope / Trigger

- Trigger: any command or service that acquires `AppState` locks, converts a
  database/file/serialization failure, or writes a rollback diagnostic.

### 2. Signatures

- `with_db(&AppState, FnOnce(&mut Database) -> Result<T, AppError>) -> Result<T, AppError>`
- `with_db_and_redactor(&AppState, FnOnce(&mut Database, &mut SecretRedactor) -> Result<T, AppError>) -> Result<T, AppError>`
- `AppError::with_source_redacted(Display, &SecretRedactor) -> AppError`
- `AppError::{database_from,io_from,serialization_from}` retain the source only
  for diagnostics and journal recovery.

### 3. Contracts

- A poisoned process-local mutex/RwLock is recovered with its inner guard; no
  `state_lock_error` RPC branch is emitted.
- Command responses keep the stable `{ code, message, details?, recoverable,
  action? }` shape. `source` is skipped by Serde and Specta.
- Before storing or logging a source, the shared `SecretRedactor` redacts the
  source and existing allowlisted details; the event is a structured `warn`.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Lock was poisoned by a prior panic | Recover the guard and run the command |
| SQLite, I/O, or serialization failure | Stable error code plus redacted source; no raw text in RPC |
| Registered secret occurs in source or details | Secret is replaced before log/journal persistence |
| Caller adds an unallowlisted detail | Reject/omit it through `with_safe_details` |

### 5. Good/Base/Bad Cases

- Good: a command uses `with_db_and_redactor` and returns
  `AppError::io_from(...).with_source_redacted(...)`.
- Base: startup code without a shared redactor uses `with_source`, which still
  applies conservative default redaction.
- Bad: manually locking in every command, returning a raw `Display` string, or
  serializing `source` as an RPC field.

### 6. Tests Required

- Panic while holding the database lock, then assert a subsequent command and
  redactor access both succeed.
- Serialize an `AppError` with a source and assert `source` is absent while
  stable code/details remain unchanged.
- Register a fake API key, attach it as a source, and assert the captured
  tracing/journal text contains the redaction marker and not the key.

### 7. Wrong vs Correct

#### Wrong

```rust
let database = state.database().lock().map_err(|_| AppError::state_lock_error())?;
Err(AppError::new(ErrorCode::DatabaseError, error.to_string(), false))
```

#### Correct

```rust
with_db_and_redactor(&state, |database, redactor| {
    db::save(database).map_err(|error| {
        AppError::database_from("database", "save", &error)
            .with_source_redacted(error, redactor)
    })
})
```
