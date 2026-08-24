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
- The release command boundary must derive tool availability and Claude installation
  version from a real read-only probe, then inject the matching version-bound policy
  evidence into MCP/Skill status, preview, and apply paths. `all_installed` and a
  permanently conservative policy probe are fixture/fail-closed defaults, not proof
  that production discovery wiring is complete.
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
- End-to-end secret audits must serialize the real RPC DTOs and read actual SQLite,
  journal, and snapshot-index carriers. Assert each expected carrier is non-empty before
  asserting that fixture secrets have zero matches, so an empty audit cannot pass.
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

## Scenario: Durable apply and conservative recovery

### 1. Scope / Trigger

- Trigger: any preview-consumption, native-config write, snapshot, journal, rollback,
  interrupted-run detection, restore, or local Git exclude change.

### 2. Signatures

- `apply_persisted_preview(&Mutex<()>, &mut Database, &AppPaths, preview_id,
  &[ApplyTargetInput], &dyn ApplyFaultInjector) -> Result<ApplyResult, AppError>`
  is the only Phase 3 native-target write entry.
- `detect_interrupted_run(&Database, &AppPaths) -> Result<Option<InterruptedRunPlan>, AppError>`
  exposes conservative startup recovery state without mutating targets.
- `preview_restore(&mut Database, &AppPaths, snapshot_id, allowed_root)
  -> Result<RestorePreview, AppError>` binds current state to a one-shot restore preview.
- `restore_snapshot(&Mutex<()>, &mut Database, &AppPaths, restore_preview_id,
  allowed_root, central_root) -> Result<ApplyResult, AppError>` consumes that preview.

### 3. Contracts

- Claim an applicable preview inside one SQLite `IMMEDIATE` transaction. Recheck its
  status, expiry, scope/project, descriptor identity, target path, all bound row
  versions, and the absence of any `applying`, `restoring`, or `rollback_failed`
  writer before changing the claim state.
- Complete every parse/type/permission/path/hash/row-version preflight before the first
  snapshot or external write. Snapshot files and the recovery journal must be durable
  before a target mutation starts; private directories are `0700` and files `0600`.
- Bind every planned mutation to both its preflight `before` fingerprint and its intended
  `after` fingerprint. Recheck the former before snapshot creation, again immediately
  before rename/remove, and recheck database row versions before each target write. If an
  external writer changes the entry after the application rename, preserve that external
  state instead of treating it as the application's rollback fingerprint.
- A file/link replacement uses an unpredictable, application-owned temporary sibling:
  write, flush, `fsync` the temporary entry, rename, durably record `renamed`, then
  `fsync` the parent. Persist the temporary fingerprint before rename and remove it
  during recovery only when path, naming convention, parent, and fingerprint all match.
- Persist `ready_to_finalize_database` before the success transaction. A database
  finalize error is commit-ambiguous and must remain blocked for recovery; failure to
  persist a decorative post-commit journal must never roll back externally committed
  state.
- Roll back changed targets in strict reverse order. A rollback failure preserves all
  journals and snapshots and leaves a blocking `rollback_failed` run.
- Restoring one snapshot from a partial multi-target run does not retire the source run.
  Mark it `rolled_back` only after every journal target's current fingerprint equals
  its recorded `before` fingerprint; otherwise it remains blocking.
- A snapshot of a managed Skill child link or owned `.git/info/exclude` block remains
  attached to the parent `managed_target`, but its path is intentionally different.
  Restore must prove that derived relationship from the persisted descriptor/ownership
  (immediate managed child name or Git-resolved local exclude path); exact main-target
  equality alone is insufficient, while a generic descendant check is unsafe.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Same preview submitted twice or from two processes | At most one claim; loser receives stable conflict |
| Any identity, row-version, hash, path, ancestor, type, parse, or permission mismatch | Stable stale/parse/type/permission/conflict error and zero external writes |
| Crash before rename | Keep durable snapshot/journal; never infer that rename occurred |
| Crash after rename or before/after database finalize | Keep blocking evidence; never guess rollback or overwrite |
| Temporary path now contains another entry | Report conflict and preserve it |
| Rollback or restore fails after any mutation | Reverse recovery where proven safe; preserve all evidence and block later writes |
| Tracked Git path | Warning only; never modify `.gitignore` or the index |
| Confirmed untracked Git path | Idempotently update only the owned `.git/info/exclude` marker block |

### 5. Good/Base/Bad Cases

- Good: an unchanged persisted preview is claimed once, every target is snapshotted,
  atomically replaced, verified, and finalized in one SQLite success transaction.
- Base: a crash before rename leaves a durable `applying` run, snapshot, and journal;
  startup blocks new writes and returns an evidence-based recovery plan.
- Bad: a path, fingerprint, ancestor, descriptor, or row version changes between
  preflight and rename. Apply returns a stable stale/conflict error and preserves the
  external writer's state.

### 6. Tests Required

- Exercise same/different preview claims through independent SQLite connections.
- Inject crashes immediately before/after rename, on target N, and immediately
  before/after database finalize; assert conservative startup plans and no guessed
  overwrite.
- Cover reverse multi-target rollback, rollback failure, second-snapshot restore,
  stale snapshot row versions, derived Skill/Git snapshots, unknown
  directories/symlinks/temporary entries, and partial source-run restoration that must
  continue blocking.
- Audit preview, error, journal, and RPC serialization for fixture secrets. Keep all
  native Claude/Codex fixtures under explicit temporary roots.
- In end-to-end restore tests, derive `allowed_root` through the production
  `snapshot_restore_context` tool/artifact/scope routing and assert it remains inside the
  explicit temporary HOME, tool root, or canonical project root. Tests must not pass a
  hand-written allowed root directly to Restore.

### 7. Wrong vs Correct

#### Wrong

```rust
// A single early check leaves a TOCTOU window before rename.
verify_preview_once(preview_id)?;
fs::rename(temporary, target)?;
```

#### Correct

```rust
// The engine binds fingerprints and row versions, then revalidates at each mutation boundary.
apply_persisted_preview(
    write_operations,
    database,
    paths,
    preview_id,
    inputs,
    &NoApplyFault,
)?;
```

## Scenario: Provider and global-prompt profiles

### 1. Scope / Trigger

- Trigger: Provider/Prompt CRUD, activation, native import, desired projection,
  profile RPC DTO, or profile-page behavior changes.

### 2. Signatures

- Provider service entry points use typed `ProviderProfileInput`,
  `UpdateProviderProfileInput`, `CopyProviderProfileInput`, and
  `VersionedProfileInput`; list responses are `ProviderProfileDto` and never
  contain an API-key value.
- Prompt service entry points use `PromptProfileInput`,
  `UpdatePromptProfileInput`, and `VersionedProfileInput`; list responses are
  `PromptProfileDto`.
- Native takeover is a two-step contract:
  `discover_*_import(...) -> *ImportPreviewDto`, then
  `confirm_*_import(ConfirmImportInput) -> *ProfileDto`.
- Native synchronization is a separate two-step contract:
  `preview_{provider,prompt}_sync(tool) -> PreviewPlan`, then
  `apply_profile_preview(ApplyProfilePreviewInput) -> ApplyResult`.

### 3. Contracts

- Profile names are unique per tool with `NOCASE` semantics. Update, activation,
  and deletion require the caller's `row_version`; active-row changes and the
  one-active-per-tool invariant are committed in one `IMMEDIATE` transaction.
- Provider and Prompt CRUD change only SQLite central intent. Native discovery is
  read-only, stores a redacted import preview, and rescans the full hash before a
  confirmation atomically adopts the profile and baseline.
- Claude Provider ownership is the union of previous and desired profile-declared
  env keys. Unknown or host-managed policy blocks. Unrelated env and settings remain
  unowned.
- Codex Provider ownership is `model`, `model_provider`, and only the previous/current
  managed provider IDs. Built-in IDs (`openai`, `ollama`, `lmstudio`) and unrelated
  tables remain unowned; an imported custom table preserves supported extension fields.
- API keys never appear in list/status DTOs. Recognizable secret-bearing extension
  env values are rejected from ordinary DTO fields; imported Codex extensions remain
  private and the whole managed provider table is sensitive in previews.
- Prompt bodies are non-blank Markdown written byte-for-byte. Deleting the active
  profile produces a new Phase 3 preview; it never writes from the CRUD command.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Stale update/activate/delete `row_version` | `CONFLICT`; transaction rolls back |
| Case-only duplicate name in the same tool | `CONFLICT` |
| Cross-tool copy | New UUID/Provider ID; validate target fields/options again |
| URL with credentials, query, fragment, or non-HTTP(S) scheme | `INVALID_INPUT` |
| Codex reserved Provider ID or non-`responses` wire API | reject/ignore built-in; never overwrite |
| Claude host evidence unknown, malformed, or present | policy-blocked preview; zero external writes |
| Import target hash changes before confirmation | `STALE_PREVIEW`; preserve target |
| Native target changes after sync preview | `STALE_PREVIEW`/conflict; preserve target |

### 5. Good/Base/Bad Cases

- Good: create an inactive tool-specific profile, activate it with the current
  `row_version`, inspect a redacted persisted preview, and apply it through the
  Phase 3 engine while unrelated native fields remain byte/semantically intact.
- Base: CRUD changes only SQLite central intent and returns a masked DTO; native
  files are unchanged until a separate preview is consumed.
- Bad: a stale activation, host-managed Claude setting, reserved Codex provider,
  credential-bearing URL, secret extension value, or changed import target fails
  closed without an external write.

### 6. Tests Required

- Use only temporary explicit homes/config roots and explicit policy/availability.
- Cover case-insensitive uniqueness, activation/deletion CAS, independent copy,
  stable Codex IDs, Claude old-key cleanup, Codex unknown-table/comment preservation,
  lossless Prompt import, stale import/apply, and active-profile deletion cleanup.
- Search serialized import previews, sync previews, RPC DTOs, `sync_items`, and journals
  for every fixture key/token/header; expected matches are zero.
- Regenerate and check Specta bindings whenever a profile command or DTO changes.

### 7. Wrong vs Correct

#### Wrong

```rust
// CRUD must not write a native file or expose the stored key.
let profile = repository.update_provider(input)?;
fs::write(target, serde_json::to_vec(&profile)?)?;
```

#### Correct

```rust
let profile = update_provider_profile(database, input)?;
let preview = preview_provider_sync(database, context, tool)?;
let result = apply_profile_preview(state, preview.preview_id, tool, ArtifactKind::Provider)?;
```

## Scenario: MCP central intent and inherited project projections

### 1. Scope / Trigger

- Trigger: MCP CRUD, sensitive-field editing, global/project assignment, native MCP
  projection, managed-item cleanup, or MCP preview/apply changes.

### 2. Signatures

- List/detail responses use `McpServerDto`; they expose header/env names and a
  redacted extension projection, never header/env values.
- Sensitive edits use `SensitiveMapUpdate` and `SensitiveJsonUpdate` with explicit
  `keep`, `clear`, or `replace` actions.
- Native synchronization remains two-step:
  `preview_mcp_sync(PreviewMcpSyncInput) -> PreviewPlan`, then
  `apply_mcp_preview(ApplyMcpPreviewInput) -> ApplyResult`.

### 3. Contracts

- MCP names are globally unique with `NOCASE` semantics. CRUD, enable changes, and
  both assignment mutations use optimistic row-version checks and never write native
  files.
- `stdio` requires command and permits only args/env; `streamable_http` requires an
  absolute credential-free HTTP(S) URL and permits only headers. Fragments and
  detectable secrets in ordinary DTO-visible args or URL query pairs are rejected.
- Header/env values and detectable extension secrets are registered with the central
  redactor before preview persistence. RPC, errors, sync items, and journals contain
  no such values.
- A project stores only project-specific assignments. Globally assigned items are
  read-only inherited options and are included in ownership only to detect an external
  same-name project entry. When a project has only inherited items and no collision or
  previous managed item, preview has no target and must create neither a project
  `managed_targets` row nor an empty project configuration file.
- Rename, disable, unassignment, and deletion remove an old native entry only when its
  `managed_items.last_applied_item_hash` still matches the observed item. Apply binds
  every managed-item row version and updates/removes baselines in its success
  transaction.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Case-only duplicate name or stale row version | `CONFLICT`; central intent unchanged |
| Global item selected again at project scope | Domain and SQLite conflict; no duplicate assignment |
| External project item has an inherited/desired name | Conflict preview; external item unchanged |
| Managed item is missing or its item hash changed | `external_owned_change`; rename/delete/apply blocked |
| Non-default Claude user MCP lacks version-bound evidence | Unsupported/blocked; never guess `$HOME/.claude.json` |
| Codex project trust is unknown or untrusted | Untrusted preview; zero external writes |
| Project has only inherited items and no collision | Empty preview target list; no project file creation |

### 5. Good/Base/Bad Cases

- Good: create a validated central MCP, assign it globally, preview/apply the
  native entry, then rename it while the per-item baseline still matches; only
  the proven old entry is removed and unrelated native content survives.
- Base: CRUD and assignment changes update SQLite intent and return masked DTOs;
  external files remain unchanged until a non-empty persisted preview is applied.
- Bad: an inherited duplicate, external same-name entry, stale row version,
  changed managed item, unknown capability/trust, or detectable secret in an
  ordinary DTO-visible field blocks without creating or deleting a native entry.

### 6. Tests Required

- Use explicit temporary homes/config roots/projects only. Cover JSON/TOML unknown
  field and comment preservation, project inheritance, external same-name conflicts,
  rename/removal, item drift, stale CAS, Claude capability/policy, and Codex trust.
- Audit serialized DTOs, previews, `sync_items`, errors, and journals for every fixture
  header/env/extension secret and any rejected args/query token; expected matches are
  zero.
- Regenerate/check Specta bindings whenever an MCP command or DTO changes.

### 7. Wrong vs Correct

#### Wrong

```rust
// Names alone do not prove ownership and inherited items are not project rows.
native.remove(&server.name);
repository.set_project_assignment(project, tool, server.id, false)?;
```

#### Correct

```rust
validate_managed_item_hash(observed_item, managed_item.last_applied_item_hash)?;
let preview = preview_mcp_sync(database, context, input)?;
let result = apply_mcp_preview(state, preview.preview_id, input.tool, input.project_id)?;
```

## Scenario: Skills central library and symlink projections

### 1. Scope / Trigger

- Trigger: Skill directory import, central-library inspection/deletion, assignment,
  target discovery, symlink preview/apply, or Skill RPC DTO changes.

### 2. Signatures

- `prepare_skill_import(&AppPaths, source) -> Result<PreparedSkillImport, AppError>`
  copies into a private staging directory and computes the stable tree hash.
- `inspect_central_skill(&AppPaths, id, central_path, expected_hash, status,
  include_content) -> Result<CentralSkillInspection, AppError>` proves central state.
- Native synchronization remains two-step:
  `preview_skill_sync(PreviewSkillSyncInput) -> PreviewPlan`, then
  `apply_skill_preview(ApplySkillPreviewInput) -> ApplyResult`.

### 3. Contracts

- Import opens every directory/file relative to an already-open parent descriptor with
  no-follow semantics. The root identity remains stable across copy and verification;
  symlink roots, escaping/broken/directory/cyclic links, hard links, special files,
  unsafe names, excessive depth/count/file size/total size, and concurrent changes fail
  closed. Import errors contain no source content and never modify the source tree.
- `SKILL.md` is bounded UTF-8 with an object YAML frontmatter, validated lowercase
  hyphenated name, non-empty bounded description, non-empty workflow body, and no
  reserved `synced` name. Ordinary DTOs expose the description only; full content is
  returned solely by the explicit content-preview RPC after rehashing the central copy.
- Staging and central directories are `0700`; files are `0600` plus a normalized owner
  executable bit. Executability participates in the deterministic tree hash. Rename is
  atomic and directory metadata is synced. Copy/hash/rename/DB failures remove only the
  proven per-operation staging or central child.
- Skill name uniqueness is `NOCASE`; central path, content hash, frontmatter, status,
  and row versions remain database-bound. Deletion uses CAS and is blocked by global or
  project assignments and any Skill managed item. Before recursive deletion, path/id,
  central-root ownership, record hash, and quarantined hash must all match; unknown or
  changed entries are preserved.
- Target paths are Claude user `<CLAUDE_CONFIG_DIR>/skills`, Claude project
  `<project>/.claude/skills`, Codex user explicit `HOME/.agents/skills`, and Codex
  project `<project>/.agents/skills`. `CODEX_HOME` never changes the Codex user Skills
  path. Claude policy and Codex project trust fail closed.
- A projection owns only named child links. Every desired link target is the canonical
  direct central Skill directory. Ordinary directories/files, broken/external/escaping
  links, unknown siblings, and managed-item drift are never overwritten or deleted.
  Missing parent creation is snapshotted and journaled one directory at a time; rollback
  deletes only the same still-empty directory identity created by that run.
- Global assignments are read-only inherited project options and cannot be duplicated
  by a project assignment. A project with only inherited Skills and no collision or old
  managed baseline returns an empty preview and creates neither target row nor directory.
  Apply consumes the exact persisted preview and rechecks all row versions, hashes,
  descriptor identity, policy/trust, target state, and managed-item baselines.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Unsafe entry/link/hard link/special file/limit breach/source race | Reject import; clean operation staging; source unchanged |
| Case-only duplicate name or stale row version | `CONFLICT`; central intent unchanged |
| Central path/type/hash/status drift | Invalid/missing diagnostic; content preview, sync, and delete blocked |
| Assignment or managed-item blocker at delete | Transactional conflict; central directory restored/preserved |
| Ordinary directory, unknown/external/broken link, or item drift at native target | Conflict; do not overwrite or delete |
| Claude policy unknown/blocked or Codex project untrusted | Blocked preview/apply; zero native writes |
| Pure project inheritance without collision | Empty target list; no target row or directory creation |

### 5. Good/Base/Bad Cases

- Good: import an isolated, descriptor-bound source tree into a private central child,
  assign it globally, preview the canonical child link, and apply it while unrelated
  siblings and source files remain unchanged.
- Base: importing, listing, content preview, assignment, and deletion change only the
  private library or SQLite central intent; native Skill targets remain unchanged until
  a non-empty persisted preview is explicitly applied.
- Bad: a source race, escaping link, hard link, changed central hash, stale row version,
  inherited duplicate, ordinary target directory, unknown link, or untrusted/policy-
  blocked target fails closed without deleting or replacing external state.

### 6. Tests Required

- Use only explicit temporary source/home/config/project/data roots; never read or write
  real Skills or native configuration directories.
- Cover strict frontmatter/body validation, source root replacement, stable executable
  hashing/modes, hard links, special files, escaping/broken/cyclic links, permissions,
  and all import limits and cleanup boundaries.
- Cover the four target paths, `CODEX_HOME` independence, policy/trust, persisted-preview
  stale checks, global inheritance, external same-name entries, managed-item drift and
  deletion blockers, atomic missing-parent rollback, and replaced-directory preservation.
- Search ordinary DTOs, errors, persisted previews, and journals for fixture body and
  private frontmatter markers; expected matches are zero. Regenerate/check bindings
  whenever a Skill command or DTO changes.

### 7. Wrong vs Correct

#### Wrong

```rust
fs::read_dir(source)?;
fs::remove_dir_all(database_central_path)?;
```

#### Correct

```rust
let prepared = prepare_skill_import(paths, source)?;
let inspection = inspect_central_skill(paths, id, central_path, expected_hash, status, false)?;
```

## Scenario: Project registry, live status, dashboard, and onboarding

### 1. Scope / Trigger

- Trigger: project registration/rescan/removal, project status DTOs, dashboard
  aggregation, first-run takeover, or snapshot restore entry-point changes.

### 2. Signatures

- Project mutations use `RegisterProjectInput` or `VersionedProjectInput`; removal
  returns `RemoveProjectResultDto` and never mutates a native project target.
- Project reads return `ProjectDto` after a fresh native scan. Dashboard reads return
  `DashboardSummaryDto`; explicit all-skip completion uses an idempotent
  `complete_onboarding` central-state command.
- Snapshot UI first calls `preview_snapshot_restore`, then consumes its exact preview
  through `restore_snapshot`.

### 3. Contracts

- Canonical project-root identity is unique with `NOCASE` semantics. Re-registering a
  soft-removed root reactivates the existing project identity; an active root is a
  conflict.
- Rescan and ordinary project reads inspect current Git, Codex trust, Claude policy,
  native target type/content, managed-item hashes, and external same-name collisions.
  They never substitute persisted `last_status` for native evidence.
- Project removal is CAS-protected, is blocked by any `applying`, `restoring`, or
  `rollback_failed` run, removes only central assignment intent, and leaves native
  files/links untouched.
- Project assignment updates bump the project row version. Every frontend consumer of
  project, MCP, and Skill project DTOs must refresh together after either assignment.
- Dashboard run kind/status/error values use generated enums, not open strings. Unknown
  database values fail closed. Counts and recovery entries expose only central
  metadata, status, paths, hashes, and stable codes.
- First-run detection is read-only. Import confirmation changes only central intent;
  every native write still needs a persisted preview and exact Apply. An interrupted
  wizard can regenerate previews from active central profiles. Explicit all-skip is
  persisted so the app does not loop forever while both tools remain unmanaged.
- A global snapshot restore derives its allowed root from the exact tool/artifact
  matrix (`HOME`, `CLAUDE_CONFIG_DIR`, or `CODEX_HOME`). A removed-project snapshot is
  not restorable until the project identity is active again.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Missing, non-directory, permission-denied, symlink-escaped, or case-only duplicate root | Stable path/conflict error; no row or native write |
| Re-register a soft-removed canonical root | Reactivate the same identity and rescan current native state |
| Stale project `row_version` | `CONFLICT`; preserve project, assignments, and native targets |
| Any active Apply/Restore/rollback-recovery run during removal | `WRITE_IN_PROGRESS`; remove nothing |
| Native external same-name entry or managed-item drift | Distinct conflict/drift target status; never reuse cached success |
| Unknown recent-run kind/status/error value | Fail closed while building the typed dashboard DTO |
| All tools explicitly skipped in onboarding | Persist completion; create no profile, preview, or native write |
| Global snapshot under overridden roots or snapshot of removed project | Derive exact matrix root; removed project remains blocked |

### 5. Good/Base/Bad Cases

- Good: register a canonical isolated project, scan current Git/policy/trust and native
  targets, select only project additions, review a persisted preview, and restore a
  snapshot through the exact tool/artifact root.
- Base: list/rescan/dashboard/onboarding detection reads current evidence and central
  metadata only; project CRUD and import confirmation do not write native targets.
- Bad: a case alias, stale project version, active writer, external same-name item,
  removed-project snapshot, unknown run enum, or skipped tool is never reported as
  synchronized and never triggers an implicit Apply.

### 6. Tests Required

- Use explicit canonical `tempfile` home, Claude root, Codex root, project root, and
  application-data root. Never read process tool environment or a developer config.
- Cover symlink aliases, `NOCASE` duplicates, soft reactivation, stale removal CAS,
  active-writer removal blocking, native preservation, live managed drift, and
  external same-name conflict before the first preview.
- Cover explicit all-skip persistence, typed recent-run DTOs, custom config-root
  restore routing, removed-project restore blocking, and zero secret values in all
  dashboard/project/recovery serialization.

### 7. Wrong vs Correct

#### Wrong

```rust
let project = repository.find_by_root(input.root_path)?;
let status = project.last_status;
restore_snapshot(snapshot_id, environment.home())?;
```

#### Correct

```rust
let project = register_project(database, environment, &input)?;
let project = rescan_project(database, environment, &VersionedProjectInput {
    id: project.id,
    row_version: project.row_version,
})?;
let context = snapshot_restore_context(database, environment, snapshot_id)?;
```
