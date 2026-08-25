# Logging Guidelines

> How logging is done in this project.

---

## Overview

The backend currently has no application logging facade, subscriber, or
general-purpose logging pipeline. Business code does not call `println!`,
`eprintln!`, `dbg!`, `log`, or `tracing`. Failures cross the IPC boundary as
stable `AppError` values, while durable synchronization evidence is stored in
the application-owned database, journal, and snapshot index.

Any later log, journal, preview, or crash context must accept only redacted
structures from `SecretRedactor`, never raw domain/configuration payloads.

## Log Levels

Define levels when structured logging is introduced. Until then, do not add
ad-hoc console output or invent project-specific level semantics.

## Structured Logging

Use stable codes, paths where allowed, hashes, statuses, and `RedactedJson`.
Keep raw `serde_json::Value` out of logging and journal APIs.

## What to Log

Future synchronization logs may include run IDs, target paths, state
transitions, warning/error codes, and hashes after passing the error/detail
allowlist.

## What NOT to Log

Never log API keys, Authorization values, bearer/basic credentials, MCP
header/env values, detected secret extension fields, prompt/config file
fragments, snapshot contents, or raw OS/SQLite errors that may embed them.

## Current Error Pattern

Map an external failure to a stable operation and return it instead of printing
the raw error. `Database::open` uses this pattern:

```rust
let mut connection = Connection::open(paths.database())
    .map_err(|_| AppError::database(&paths.database().to_string_lossy(), "open"))?;
```

## Testing and Review

- Search new backend code for console macros and direct `log`/`tracing` calls.
  Their introduction requires a separate logging design, dependency, redaction
  contract, and tests.
- Extend the `SecretRedactor` serialization tests whenever a new durable or
  diagnostic carrier is introduced.
- Treat a raw error string, native configuration fragment, or secret-bearing
  `serde_json::Value` in diagnostics as a release blocker.
