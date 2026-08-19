# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

Backend integrations use explicit inputs and fail-closed evidence. Discovery, scanning,
preview generation, and Git inspection are read-only operations; they must be safe to
run against hostile paths and configuration contents.

---

## Forbidden Patterns

- Reading `HOME`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, or equivalent process state from
  adapters. Resolve every target from an explicit discovery context.
- Guessing Claude MCP targets for a non-default config root, or treating stale policy
  and trust evidence as allowed.
- Following a symlinked target or a symlink introduced in an ancestor after discovery.
- Treating an incomplete full/managed hash pair as a mergeable baseline.
- Logging or persisting raw native configuration, raw diffs, tokens, environment
  values, headers, or other secret-bearing values.
- Running Git inspection without argument termination, a timeout, sanitized repository
  redirection variables, and disabled optional hooks/caches.

---

## Required Patterns

- Canonicalize registered project roots and use `lstat`-style checks for target paths
  and their ancestors before opening a target.
- Bind config-root capability evidence to the exact root and tool version, and bind
  managed-policy evidence to the exact installation version; unknown or mismatched
  evidence must fail closed.
- Keep managed-selector ownership in `TargetDescriptor`; validate that adapter scans
  and renders cannot escape those roots.
- Canonicalize map ordering before hashing. Persist full and managed hashes together so
  unmanaged-only drift can merge while managed drift blocks.
- Redact each document-relative before/after projection before adding a diff envelope.
- Persist preview target identity and all participating row versions, then validate them
  before a preview can be consumed.

---

## Testing Requirements

- Use isolated temporary homes, config roots, and project roots. Tests must never access
  real Claude or Codex configuration.
- Cover missing, empty, malformed, permission-denied, symlink, target-type-change, and
  scalar-at-managed-path cases for every supported format.
- Audit serialized preview/RPC/error/journal output against fixture secrets.
- Verify Git inspection neither executes repository hooks nor modifies `.gitignore`,
  `.git/info/exclude`, the index, or worktree files.

---

## Code Review Checklist

- The Claude/Codex × global/project × provider/prompt/MCP/skill descriptor matrix is
  complete and reports capability, policy, trust, and prompt-override state accurately.
- Sensitive selectors match the current native field names and are document-relative.
- JSON/TOML rendering preserves unmanaged fields, tables, and comments; Markdown and
  skill-link handling do not follow unmanaged links.
- Preview remains pure read/compute/persist and exposes no apply, snapshot, restore, or
  external-write entry point.

## Scenario: Explicit discovery and read-only preview

### 1. Scope / Trigger

- Trigger: any target-matrix, adapter, native-format parser, managed projection,
  drift assessment, Git inspection, or preview persistence change.

### 2. Signatures

- `ExplicitEnvironment::new(home, claude_config_dir, codex_home, availability)`
  receives every environment path explicitly.
- `ToolAdapter::discover(&DiscoveryContext) -> Result<Vec<TargetDescriptor>, AppError>`
  produces descriptors; `scan_target(adapter, descriptor, ownership)` is read-only.
- `build_preview_plan(scope, project_id, requests, redactor) -> Result<PreviewPlan, AppError>`
  computes redacted changes; `persist_preview(&mut Database, &PreviewPlan)` writes
  only application-owned SQLite preview rows.

### 3. Contracts

- `CLAUDE_CONFIG_DIR` affects Claude settings, prompt, and skills targets;
  `CODEX_HOME` affects Codex config and prompt; Codex user skills always resolve
  from explicit `HOME/.agents/skills`.
- Non-default Claude user MCP requires version-bound capability evidence. Codex
  project MCP/skills require trusted evidence. Unknown evidence blocks.
- A preview binds descriptor identity, full/managed hashes, managed target
  `row_version`, all participating entity versions, scope/project, warnings,
  and a redacted before/after projection.

### 4. Validation & Error Matrix

| Condition | Preview state |
| --- | --- |
| Missing target | `missing`; add may be planned |
| Malformed document or scalar at a managed intermediate path | `parse_error`; block |
| Permission denied, target-type change, or unsafe symlink ancestor | distinct blocked state |
| Only full hash changes | `external_non_owned_change`; deterministic merge allowed |
| Managed hash changes or hash pair is incomplete | conflict; block |
| Unknown Claude policy/capability or Codex trust | policy/untrusted/unsupported; block |
| Duplicate target or contradictory row version | `INVALID_INPUT`; persist nothing |

### 5. Good/Base/Bad Cases

- Good: an unmanaged TOML table changes while managed selectors remain equal;
  comments and unknown tables survive rendering and preview reports a warning.
- Base: a missing, supported target produces a redacted `add` preview.
- Bad: stale capability evidence, prompt override uncertainty, a late symlink,
  or a secret-bearing nested diff never becomes an applicable clean preview.

### 6. Tests Required

- Cover the complete Claude/Codex × global/project × artifact descriptor matrix.
- Use isolated homes/config roots/projects for missing, empty, malformed,
  permission, symlink, trust, policy, override, and drift fixtures.
- Search serialized preview rows, RPC DTOs, errors, and journals for every fixture
  secret; expected matches are zero.
- Verify Git inspection has a timeout, passes `--`, sanitizes Git redirection
  variables, disables optional hooks/caches, and changes no repository file.
- Regenerate/check Specta bindings whenever a descriptor, error, or preview DTO changes.

### 7. Wrong vs Correct

#### Wrong

```rust
let home = std::env::var("HOME")?;
let preview = json!({ "before": raw, "after": desired });
```

#### Correct

```rust
let environment = ExplicitEnvironment::new(home, claude_root, codex_root, availability)?;
let preview = build_preview_plan(scope, project_id, requests, &redactor)?;
```
