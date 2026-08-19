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

## API Error Responses

The generated RPC shape is `{ code, message, details?, recoverable, action? }`.
Regenerate and check `src/bindings/commands.ts` whenever this contract changes.

## Common Mistakes

- Do not derive or expose an alternate RPC error payload.
- Do not accept arbitrary detail keys or dynamic error messages.
- Registered secrets that look like JSON scalars (for example `42` or `null`)
  must be replaced before JSON parsing so they cannot bypass redaction.
