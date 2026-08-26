# 后端规范定向上下文

来源：`.trellis/spec/backend/quality-guidelines.md`，规划时基于仓库提交 `43d29c2`。原规范是唯一权威；本文只摘录本任务相关完整段落，避免 32768 字节的单文件注入限制截断规范。实现或检查时若原规范已变化，须重新读取相应原文；不修改全局注入配置。

覆盖：通用质量规则、显式发现与只读预览、Skills 中央库与链接、发布环境/Claude 策略证据。数据库、错误和前端合同由 JSONL 中的独立规范文件补充。

---

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

| Condition                                                         | Preview state                                            |
| ----------------------------------------------------------------- | -------------------------------------------------------- |
| Missing target                                                    | `missing`; add may be planned                            |
| Malformed document or scalar at a managed intermediate path       | `parse_error`; block                                     |
| Permission denied, target-type change, or unsafe symlink ancestor | distinct blocked state                                   |
| Only full hash changes                                            | `external_non_owned_change`; deterministic merge allowed |
| Managed hash changes or hash pair is incomplete                   | conflict; block                                          |
| Unknown Claude policy/capability or Codex trust                   | policy/untrusted/unsupported; block                      |
| Duplicate target or contradictory row version                     | `INVALID_INPUT`; persist nothing                         |

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
  direct local-import symlink roots, escaping/broken/directory/cyclic links, hard links, special files,
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
  synchronization path. `CODEX_HOME/skills` is a separate explicit import source.
  Claude policy and Codex project trust fail closed.
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

| Condition                                                                        | Required result                                                       |
| -------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Unsafe entry/link/hard link/special file/limit breach/source race                | Reject import; clean operation staging; source unchanged              |
| Case-only duplicate name or stale row version                                    | `CONFLICT`; central intent unchanged                                  |
| Central path/type/hash/status drift                                              | Invalid/missing diagnostic; content preview, sync, and delete blocked |
| Assignment or managed-item blocker at delete                                     | Transactional conflict; central directory restored/preserved          |
| Ordinary directory, unknown/external/broken link, or item drift at native target | Conflict; do not overwrite or delete                                  |
| Claude policy unknown/blocked or Codex project untrusted                         | Blocked preview/apply; zero native writes                             |
| Pure project inheritance without collision                                       | Empty target list; no target row or directory creation                |

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

| Condition                                                                      | Required result                                                                           |
| ------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| Executable absent from explicit PATH                                           | `unavailable`; target reports tool not installed                                          |
| Tool exists only under default Volta shim path                                 | Append `HOME/.volta/bin`; validate target; installed only on exact version output         |
| Volta-style symlink shim resolves to shared `volta-shim`                       | Execute the PATH candidate, not the canonical target, so argv0 remains the requested tool |
| Unsafe PATH, non-executable, timeout, non-zero exit, or malformed output       | `unsupported`; zero native writes                                                         |
| Valid Claude/Codex version output                                              | `installed`; exact parsed version stored once                                             |
| Claude version/config root differs from evidence                               | evidence stale; MCP/Skill policy/capability fail closed                                   |
| Default Claude config root                                                     | user MCP remains exact `$HOME/.claude.json`                                               |
| Non-default Claude root without verified target evidence                       | unsupported; never guess a user MCP path                                                  |
| Main policy absent and drop-in absent/empty, or valid object omits the setting | allowed evidence bound to version/root                                                    |
| Explicit valid policy `false`/array/`true`                                     | allowed/per-surface blocked according to the validated value                              |
| Malformed, unreadable, dynamic, symlinked, or multi-file policy source         | unknown; MCP/Skills block                                                                 |

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
