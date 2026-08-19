# Logging Guidelines

> How logging is done in this project.

---

## Overview

Phase 1 intentionally has no general-purpose logging pipeline. Any later log,
journal, preview, or crash context must accept only redacted structures from
`SecretRedactor`, never raw domain/configuration payloads.

## Log Levels

Define levels when structured logging is introduced. Until then, do not add
ad-hoc `println!`/`eprintln!` calls for runtime data.

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
