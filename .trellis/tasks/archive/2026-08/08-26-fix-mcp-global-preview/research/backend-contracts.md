# MCP 导入相关后端规范摘录

为避免完整规范超过上下文注入上限，仅摘录本任务涉及的通用安全、扫描、原生导入、MCP 和发布探针合同。以下为规划时原文快照；如规范更新，以原文件对应章节为准。

## 来源：.trellis/spec/backend/quality-guidelines.md:1（至 151 行）

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


## 来源：.trellis/spec/backend/quality-guidelines.md:268（至 459 行）

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
- Claude Provider import derives the central `default_model` from `ANTHROPIC_MODEL`
  first, then from the official `ANTHROPIC_DEFAULT_*_MODEL` family, while preserving
  imported default-model family variables as profile-declared env keys.
- Codex Provider ownership is `model`, `model_provider`, and only the previous/current
  managed provider IDs. Built-in IDs (`openai`, `ollama`, `lmstudio`) and unrelated
  tables remain unowned; an imported custom table preserves supported extension fields.
- Codex OAuth import may adopt the built-in `openai` provider when `auth.json` proves
  an OAuth token shape exists. It must not persist, diff, or write `auth.json` contents,
  and the resulting sync projection writes only provider selection/model fields without
  `experimental_bearer_token`.
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
  stable Codex IDs, Claude default-model family import, Claude old-key cleanup,
  Codex OAuth import without token persistence, Codex unknown-table/comment preservation,
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


## 来源：.trellis/spec/backend/quality-guidelines.md:663（至 777 行）

## Scenario: Release-native tool and Claude policy probe

### 1. Scope / Trigger

- Trigger: release setup, tool availability/version detection, Claude user-MCP
  capability, Claude customization-policy evidence, or any public MCP/Skill status,
  preview, and apply entry point.

### 2. Signatures

- `probe_release_environment(&ReleaseToolProbeInput) -> Result<ReleaseToolProbeResult, AppError>`
  is the only release-native probe entry. Its input contains explicit HOME, Claude/Codex
  roots, PATH, timeout, and official Claude managed-settings paths.
- `ExplicitEnvironment` owns the resulting availability states, exact Claude/Codex
  versions, optional `VerifiedClaudeUserMcpEvidence`, and optional
  `VerifiedClaudeCustomizationPolicyEvidence` for the lifetime of `AppState`.
- Public MCP/Skill/Project/Profile services consume
  `environment.claude_user_mcp_probe()` and
  `environment.claude_customization_policy_probe()`; injectable `*_with_probes`
  functions remain fixture boundaries only.

### 3. Contracts

- Resolve `claude` and `codex` only from an explicit absolute PATH. The macOS release
  input may append the default `HOME/.volta/bin` shim directory after the inherited
  PATH because GUI launches often miss shell PATH setup; preserve inherited precedence,
  deduplicate the appended entry, and keep rejecting any unsafe inherited PATH segment.
  Validate a discovered PATH candidate by checking its symlink metadata and executable
  canonical target, but execute the original candidate path so symlink shims such as
  Volta keep the `claude` / `codex` argv0 they require. Run only fixed `--version`,
  null stdin, non-blocking bounded stdout/stderr, a hard deadline, a killed process
  group even after the wrapper parent exits, fixed current directory, cleared inherited
  environment, and an explicit minimal environment. Never invoke a shell or an
  interactive/status command.
- Strictly accept only documented single-line version forms. Missing executable means
  `unavailable`; unsafe PATH, spawn/non-zero/timeout/oversized/non-UTF-8/malformed output
  means `unsupported`; only validated output means `installed` with an exact version.
- The default Claude user MCP path remains the official `$HOME/.claude.json` and cannot
  be redirected by evidence. A non-default Claude root stays unsupported unless evidence
  binds the exact installation version, normalized config root, and verified target path.
- Customization policy evidence reads only the exact official macOS managed-settings file.
  Its root-to-leaf walk uses descriptor-relative `openat` with no-follow and type checks
  for every ancestor and leaf before bounded JSON parsing. A trustworthily absent main
  file with an absent or empty drop-in directory, or a valid object that omits
  `strictPluginOnlyCustomization`, is explicit Allowed evidence with no source path.
  An explicit valid setting is accepted from the verified source path. Both forms bind
  the exact Claude version and normalized config root. Invalid, unreadable, symlinked,
  dynamic, or multi-source policy remains unknown. Provider host policy remains independent.
- Setup probes once and stores the immutable evidence in `AppState`; commands never reread
  process environment, rerun binaries, or substitute cached success for mismatched evidence.
- Tool profile status serializes the three-state availability and exact validated version.
  UI discovery must distinguish installed, unavailable, and unsupported; unknown or unsafe
  probe results never become an installed state and never trigger native import reads.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Executable absent from explicit PATH | `unavailable`; target reports tool not installed |
| Tool exists only under default Volta shim path | Append `HOME/.volta/bin`; validate target; installed only on exact version output |
| Volta-style symlink shim resolves to shared `volta-shim` | Execute the PATH candidate, not the canonical target, so argv0 remains the requested tool |
| Unsafe PATH, non-executable, timeout, non-zero exit, or malformed output | `unsupported`; zero native writes |
| Valid Claude/Codex version output | `installed`; exact parsed version stored once |
| Claude version/config root differs from evidence | evidence stale; MCP/Skill policy/capability fail closed |
| Default Claude config root | user MCP remains exact `$HOME/.claude.json` |
| Non-default Claude root without verified target evidence | unsupported; never guess a user MCP path |
| Main policy absent and drop-in absent/empty, or valid object omits the setting | allowed evidence bound to version/root |
| Explicit valid policy `false`/array/`true` | allowed/per-surface blocked according to the validated value |
| Malformed, unreadable, dynamic, symlinked, or multi-file policy source | unknown; MCP/Skills block |

### 5. Good/Base/Bad Cases

- Good: release setup validates both exact versions, binds an explicit official policy
  value, stores one environment in `AppState`, and public status/preview/apply reuse it.
- Base: trustworthily absent official policy sources bind Allowed evidence, so a missing
  supported MCP/Skill target can produce an initialization preview without a native write.
- Bad: a hanging wrapper, forged multiline output, changed Claude version, mismatched
  config root, or ambiguous policy source never becomes installed/allowed evidence.

### 6. Tests Required

- Use only `tempfile` homes, Claude/Codex roots, application roots, policy paths, and
  executable directories. Fake `claude`/`codex` files are the only commands tests run.
- Cover missing tools, default Volta shim path discovery, symlink shim argv0 preservation,
  unsafe/malformed/oversized/non-UTF-8 output, non-zero exit, bounded timeout including
  descendants and wrappers that exit before descendants, exact argv and
  null stdin, exact Claude/Codex parsing, non-default Claude root, and policy
  allowed/blocked/unknown.
- Cover absent main files, absent and empty drop-in directories, valid objects with an
  omitted setting, explicit boolean/surface values, wrong official basenames, ancestor
  symlinks, malformed JSON, unreadable files, dynamic helpers, ambiguous drop-ins, and
  evidence source/version/root/target mismatches without reading real host policy.
- Prove a changed version invalidates policy evidence, and prove public MCP/Skill status,
  preview, and apply consume environment evidence rather than conservative defaults.
- Keep the dedicated-user/VM real-install discovery and UI smoke gate manual; tests must
  never inspect developer tools or configuration.

### 7. Wrong vs Correct

#### Wrong

```rust
let environment = ExplicitEnvironment::new(home, claude_root, codex_root,
    ToolAvailability::all_installed())?;
let policy = ConservativeClaudeCustomizationPolicyProbe;
```

#### Correct

```rust
let probe = probe_release_environment(&ReleaseToolProbeInput::for_macos_release(
    home, claude_root, codex_root, explicit_path,
))?;
app.manage(AppState::initialize_with_environment(paths, probe.environment)?);
```
