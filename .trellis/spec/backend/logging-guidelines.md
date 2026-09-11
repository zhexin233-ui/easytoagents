# Logging Guidelines

> How logging is done in this project.

---

## Overview

The backend uses `tracing` with a process-wide subscriber installed during
Tauri setup. The subscriber writes daily `app-YYYY-MM-DD.log` files below the
application-private `logs/` directory. The directory is `0700` and each log
file is `0600`; files older than seven days are pruned on startup. A failed
log open is deliberately non-fatal and discards that event.

Any later log, journal, preview, or crash context must accept only redacted
structures from `SecretRedactor`, never raw domain/configuration payloads.

## Log Levels

The default filter is `info`. `EASYTOAGENTS_LOG` accepts the standard
`tracing-subscriber::EnvFilter` syntax and is read when the subscriber is
constructed. Apply/restore phase transitions use `info`; failures enriched by
`AppError::with_source` use `warn`.

## Structured Logging

Use stable codes, paths where allowed, hashes, statuses, and redacted source
text. Keep raw `serde_json::Value` out of logging and journal APIs. Journal
failure records contain only the stable error code, allowlisted operation, and
the source after `SecretRedactor` processing.

## What to Log

Future synchronization logs may include run IDs, target paths, state
transitions, warning/error codes, and hashes after passing the error/detail
allowlist.

## What NOT to Log

Never log API keys, Authorization values, bearer/basic credentials, MCP
header/env values, detected secret extension fields, prompt/config file
fragments, snapshot contents, or raw OS/SQLite errors that may embed them.
Always call `with_source_redacted` when a shared `SecretRedactor` is
available; the fallback `with_source` still applies conservative text
redaction for startup/infrastructure failures.

## Current Error Pattern

Map an external failure to a stable operation and return it instead of printing
the raw error. `Database::open` uses this pattern:

```rust
let mut connection = Connection::open(paths.database()).map_err(|error| {
    AppError::database_from(&paths.database().to_string_lossy(), "open", &error)
})?;
```

## Testing and Review

- Search new backend code for console macros and direct logging calls. New
  events must use structured fields and pass through the redaction contract.
- Extend the `SecretRedactor` serialization tests whenever a new durable or
  diagnostic carrier is introduced.
- Treat a raw error string, native configuration fragment, or secret-bearing
  `serde_json::Value` in diagnostics as a release blocker.
