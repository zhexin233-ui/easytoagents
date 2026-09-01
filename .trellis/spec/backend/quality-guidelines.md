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
- Treating a tool enum as capability evidence. Cursor currently supports only global
  and project MCP/Skills; Provider, Prompt, API Key/model, and project Rules must return
  explicit unsupported descriptors with no path and must never reach native reads.
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
  real Claude, Codex, or Cursor configuration.
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

- The Claude/Codex/Cursor × global/project × provider/prompt/MCP/skill descriptor matrix is
  complete and reports capability, policy, trust, and prompt-override state accurately.
  Cursor Provider/Prompt/project Rules entries are always unsupported and pathless;
  only its MCP/Skill entries are assignable.
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
  `CODEX_HOME` affects Codex config, prompt, and user skills targets (Codex user
  Skills resolve from `$CODEX_HOME/skills`, following `CODEX_HOME`).
- Cursor targets are resolved only from explicit HOME/project roots: user/project MCP
  use `.cursor/mcp.json`, and user/project Skills use `.cursor/skills`. Cursor has no
  Provider or Prompt target and no project Rules target.
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
| Cursor Provider, Prompt, API Key/model, or project Rules | unsupported and pathless; zero native reads/writes |
| Duplicate target or contradictory row version | `INVALID_INPUT`; persist nothing |

### 5. Good/Base/Bad Cases

- Good: an unmanaged TOML table changes while managed selectors remain equal;
  comments and unknown tables survive rendering and preview reports a warning.
- Base: a missing, supported target produces a redacted `add` preview.
- Bad: stale capability evidence, prompt override uncertainty, a late symlink,
  or a secret-bearing nested diff never becomes an applicable clean preview.

### 6. Tests Required

- Cover the complete Claude/Codex/Cursor × global/project × artifact descriptor matrix,
  including fail-closed Cursor Provider/Prompt cases.
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
- Snapshot persistence has an explicit `storage_kind`: file bodies use `payload_file`,
  missing/symlink and legacy directory placeholders use `metadata_only`, and an
  explicitly taken-over real Skill directory uses `directory_tree`. Migration defaults
  old files to payload and every other old row to metadata only; a metadata-only
  directory is intentionally not restorable.
- A directory-tree snapshot is a private, no-follow, content-hashed copy rooted at
  `snapshots/<run>/<snapshot>.snapshot.d`. Restore copies it to an owned temporary
  sibling, verifies the complete tree hash, quarantines only a currently proven central
  link, then atomically installs the directory. Verification uses tree hash rather than
  the old directory inode. Delete recursively removes a tree only after path ownership,
  direct-parent layout, type, and full hash all match.
- A takeover mutation uses an owned `.takeover` quarantine in the target's immediate
  parent. Journal the quarantine path/fingerprint, entry type, and directory hash before
  installing the central link. Success, rollback, and explicit snapshot recovery may
  clean it only from this evidence; never follow or remove an external symlink target.

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
| Legacy `metadata_only` directory snapshot | List as non-restorable; reject restore before any native write; allow safe metadata-file deletion |
| `directory_tree` snapshot hash/path/type mismatch | Conflict; preserve snapshot and current target |
| Directory-tree restore over a central managed link | Create a second snapshot, restore and hash-verify the tree, then mark target externally changed |

### 5. Good/Base/Bad Cases

- Good: an unchanged persisted preview is claimed once, every target is snapshotted,
  atomically replaced, verified, and finalized in one SQLite success transaction.
- Base: a crash before rename leaves a durable `applying` run, snapshot, and journal;
  startup blocks new writes and returns an evidence-based recovery plan.
- Bad: a path, fingerprint, ancestor, descriptor, or row version changes between
  preflight and rename. Apply returns a stable stale/conflict error and preserves the
  external writer's state.
- Good: a real Skill directory is snapshotted as a complete tree before takeover and can
  later replace only the proven central link; the restored tree is hash-identical even
  though its inode is new.
- Bad: treat a legacy directory metadata row as recursive backup, compare a restored
  directory to the old inode, or recursively remove a tree without an owned full hash.

### 6. Tests Required

- Exercise same/different preview claims through independent SQLite connections.
- Inject crashes immediately before/after rename, on target N, and immediately
  before/after database finalize; assert conservative startup plans and no guessed
  overwrite.
- Cover reverse multi-target rollback, rollback failure, second-snapshot restore,
  stale snapshot row versions, derived Skill/Git snapshots, unknown
  directories/symlinks/temporary entries, and partial source-run restoration that must
  continue blocking.
- Cover external-link and real-directory takeover, failure/crash on both sides of
  quarantine/link rename, external target preservation, `directory_tree` restore/delete,
  legacy `metadata_only` refusal, and v10→v11 storage-kind migration classification.
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

### Scenario: Project-level prompt assignment (hard copy)

#### 1. Scope / Trigger

- Trigger: prompt project assignment, project-scope prompt preview/apply,
  unassignment, or prompt profile deletion changes.

#### 2. Signatures

- `preview_prompt_sync(tool, projectId: Option<String>) -> PreviewPlan`；
  `apply_profile_preview(ApplyProfilePreviewInput{previewId, tool, artifactKind,
  projectId}) -> ApplyResult`；projectId 为 None 时保持全局语义不变。
- `set_prompt_project_assignment(SetPromptProjectAssignmentInput{projectId,
  tool, promptProfileId: Option, projectRowVersion}) -> PromptProjectAssignmentDto`；
  `get_prompt_project_assignment(projectId, tool)`。
- 目标矩阵补全：Claude 项目 `<root>/CLAUDE.md`、Codex 项目
  `<root>/AGENTS.md`（均为 `TargetFormat::Markdown` + `$document` +
  `WholeDocument`）。全局与项目分配**互不排斥**（与 mcp/skill 的互斥触发器
  有意不同）；每 (项目, 工具) 至多一份（`prompt_project_assignments` PK）。

#### 3. Contracts

- 项目级为**硬拷贝**语义：apply 写普通文件，此后项目文件归项目所有；
  外部修改不构成 stale，而是把观测内容作为本次预览的确认基线（不落库），
  apply 端指纹绑定保证预览与应用间未被再次改动。全局作用域保持外部修改
  必须走接管导入的严格语义。
- 解除分配 = 同事务删除分配行 + 清空基线哈希（行保留，见 database-guidelines），
  项目文件保留、仅停止纳管；重复提交相同分配为 no-op（不 bump 项目版本）；
  分配/解除都会 bump 项目 `row_version`。
- 被项目分配引用的档案禁止删除（`count_prompt_project_assignments` 前置
  校验 + FK RESTRICT 兜底）；跨工具档案分配拒绝。
- 档案分配与同步都只改中央意图；原生写入必须经持久化预览 + 显式 Apply。

#### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| 档案与工具不匹配 | `INVALID_INPUT`；无写入 |
| 项目分配缺失但基线哈希仍存在 | `CONFLICT`（防御分支，正常流程不可达） |
| 解除分配时重复提交 / 相同分配重复提交 | no-op，不 bump 项目版本 |
| 档案仍被项目引用时删除 | `CONFLICT`；档案保留 |
| 项目外部修改 CLAUDE.md/AGENTS.md | 预览可合并（覆盖式重新应用），apply 前再校验指纹 |
| 全局目标外部修改 | stale/conflict；走接管导入（严格语义不变） |

#### 5. Tests Required

- 分配→预览→应用写出项目根文件；外部修改→覆盖；解除分配保留文件且基线
  哈希清空；跨工具拒绝；删除阻塞；全局回归不变（`profiles/service.rs`）。
- 迁移金丝雀：同连接识别修订后的 CHECK（`db/mod.rs`）。

#### 6. Wrong vs Correct

#### Wrong

```rust
// 解除分配删除基线行会破坏快照 RESTRICT 外键；也不允许在 CRUD 内写原生文件。
repository::delete_prompt_project_baseline(database, project_id, tool)?;
fs::remove_file(project_root.join("CLAUDE.md"))?;
```

#### Correct

```rust
// 解除分配：同事务删分配行 + 清空基线哈希，文件保留。
repository::set_prompt_project_assignment(database, project_id, tool, None, expected)?;
// 写入必须经预览 + Apply。
let plan = preview_prompt_sync(database, environment, redactor, tool, Some(project_id))?;
apply_profile_preview(state, plan.preview_id, tool, ArtifactKind::Prompt, Some(project_id))?;
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
- Header/env 展示值与可识别扩展凭据在预览持久化前统一登记。已识别运行值只用于
  展示隐藏，真实凭据额外参与普通字段检测；未知环境值仍保守作为凭据。
  env/header 专用字段不返回原值，RPC、错误、同步记录和 journal 不泄漏凭据。
  具体分类、同值优先级及生命周期要求见 [Native MCP Import](./mcp-import-guidelines.md)。
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

全局检测与批量复制还须遵守 [Skills 导入合同](./skill-import-guidelines.md)，其中显式入口链接与普通本地导入根链接有不同边界，复制不接管原安装。

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
  `<project>/.claude/skills`, Codex user `$CODEX_HOME/skills`, and Codex
  project `<project>/.codex/skills`, Cursor user `$HOME/.cursor/skills`, and Cursor
  project `<project>/.cursor/skills`. Codex user Skills follow `CODEX_HOME` to
  mirror where Codex itself reads skills (its bundled `.system` tree lives in
  `$CODEX_HOME/skills/.system`). `HOME/.agents/skills` is an import-only
  source, never a synchronization target. Claude policy and Codex project
  trust fail closed.
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
- Cover all six target paths, `CODEX_HOME` following for Codex skills (custom
  CODEX_HOME must move the Codex user Skills target), policy/trust, persisted-preview
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
- A project target whose scan is observed with an empty managed projection, both
  baseline hashes absent, and no existing managed items carries the neutral
  diagnostic `PROJECT_TARGET_INITIAL_UNMANAGED` instead of
  `EXTERNAL_NON_OWNED_CHANGE`; the `SyncStatus` value stays
  `external_non_owned_change` and `can_merge` stays `true`. The frontend renders it
  as a muted "未纳管" badge with an explanatory line, not as a drift warning. This is
  the project-target counterpart of the Skills `SKILL_TARGET_INITIAL_*` presentation
  diagnostics; it must not change the shared drift algorithm or preview/apply
  conflict handling.
- Registration observes targets before the project row exists, so the managed
  assessment (including the neutral initial diagnostic) only applies after the row is
  persisted; registration-time observation falls back to the unmanaged scan path.
  Assertions on managed-target diagnostics must read a post-registration rescan or
  project read.
- Dashboard run kind/status/error values use generated enums, not open strings. Unknown
  database values fail closed. Counts and recovery entries expose only central
  metadata, status, paths, hashes, and stable codes.
- First-run detection is read-only. Import confirmation changes only central intent;
  every native write still needs a persisted preview and exact Apply. An interrupted
  wizard can regenerate previews from active central profiles. Explicit all-skip is
  persisted so the app does not loop forever while both tools remain unmanaged.
- A global snapshot restore derives its allowed root from the exact tool/artifact
  matrix (`HOME`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, or Cursor's
  `$HOME/.cursor`). Cursor accepts only MCP/Skill snapshots; Provider/Prompt restore
  remains rejected. A removed-project snapshot is not restorable until the project
  identity is active again.

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
- Cover the neutral `PROJECT_TARGET_INITIAL_UNMANAGED` diagnostic for an observed
  external-only project target with global inheritance and no baseline, and its muted
  frontend rendering without the raw diagnostic line.
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

## Scenario: Release-native tool and Claude policy probe

### 1. Scope / Trigger

- Trigger: release setup, tool availability/version detection, Claude user-MCP
  capability, Claude customization-policy evidence, or any public MCP/Skill status,
  preview, and apply entry point.

### 2. Signatures

- `probe_release_environment(&ReleaseToolProbeInput) -> Result<ReleaseToolProbeResult, AppError>`
  is the only release-native probe entry. Its input contains explicit HOME, Claude/Codex
  roots, PATH, timeout, official Claude managed-settings paths, and the bounded Cursor
  Desktop candidate list (`/Applications/Cursor.app` then
  `$HOME/Applications/Cursor.app`).
- `ExplicitEnvironment` owns the resulting availability states, exact Claude/Codex/Cursor
  versions, optional `VerifiedClaudeUserMcpEvidence`, and optional
  `VerifiedClaudeCustomizationPolicyEvidence` for the lifetime of `AppState`.
- Public MCP/Skill/Project/Profile services consume
  `environment.claude_user_mcp_probe()` and
  `environment.claude_customization_policy_probe()`; injectable `*_with_probes`
  functions remain fixture boundaries only.

### 3. Contracts

- Resolve `claude` and `codex` only from an explicit absolute PATH. Probe Cursor Desktop
  first from the explicit application candidate list: use no-follow opens and bounded
  `Contents/Info.plist` parsing, require a strict semantic version, and accept only a
  regular non-symlink bundle. Only when Desktop is absent may the probe fall back to
  an explicit-PATH `agent --version`; a valid Desktop result is authoritative.
  The macOS release input may append the default `HOME/.volta/bin` shim directory after the inherited
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
| Valid Cursor Desktop bundle/version | `installed`; do not execute Cursor Agent fallback |
| Cursor Desktop absent and exact Cursor Agent version output | `installed`; exact parsed version stored once |
| Cursor bundle symlink, oversized/malformed plist, or malformed agent output without another trustworthy probe | `unsupported`; zero native reads/writes |
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

---

## Scenario: Baseline re-adoption for externally rewritten managed targets

### 1. Scope / Trigger

- Trigger: any change to `verify_managed_item_baselines`, `readopt_mcp_target`,
  `PreviewTargetRequest.readopt_available` / `baseline_mismatched_items`, or the
  preview conflict classification for managed-item targets.

### 2. Signatures

- `verify_managed_item_baselines` returns `(TargetScan, Vec<String>)`; the vec
  lists external keys whose on-disk hash diverges from
  `managed_items.last_applied_item_hash` (or that vanished from disk; a missing
  target file lists every managed key).
- `readopt_mcp_target(database, environment, input)` refreshes BOTH baseline
  levels: `managed_targets.baseline_full_hash/baseline_managed_hash` and every
  `managed_items.last_applied_item_hash`.

### 3. Contracts

- Re-adoption only mutates baseline rows. Central records, assignments, and
  native files must not change; row_version bumps come from the existing
  triggers and invalidate older persisted previews naturally.
- Ownership used to rescan must be built from the current central intent with
  the same construction as `prepare_mcp_sync`, otherwise the next preview
  immediately re-flags the target as externally changed.
- `TargetScan::Missing` clears item rows and nulls both target hashes (the
  schema requires both NULL or both set); the next preview returns to the
  mergeable "missing, create" state.
- Parse/permission/target-type/failed scans refuse re-adoption with a stable
  conflict error; capability/policy/trust blocks never set
  `readopt_available`.
- The command takes the `write_operations` mutex so an in-flight apply cannot
  interleave with baseline rewrites.

### 4. Tests Required

- Conflict preview carries the mismatched external keys and
  `readopt_available=true`; after re-adoption the next preview is mergeable and
  apply rewrites central intent.
- Missing-file re-adoption returns to the mergeable missing state.
- Unreadable targets refuse with a stable error and central rows are untouched.

---

## Scenario: Snapshot batch deletion

### 1. Scope / Trigger

- Trigger: any change to `delete_snapshots`, the snapshot storage-path
  validation helper, or anything that mutates the `snapshots` table or removes
  files under `<snapshots_root>/<run_id>/`.
- Snapshots are the crash-recovery safety net; deletion is the only mutation
  of the snapshots domain besides creation, so it must stay fail-closed.

### 2. Signatures

- `delete_snapshots(write_operations: &Mutex<()>, database: &mut Database,
  paths: &AppPaths, input: &DeleteSnapshotsInput) -> Result<DeleteSnapshotsResultDto, AppError>`
  lives in `sync/apply.rs` next to `list_snapshots`; re-exported via
  `sync/mod.rs`; command wrapper in `commands/overview.rs` locks
  `state.database()` and passes `state.write_operations()` exactly like
  `restore_snapshot`.
- `validate_snapshot_storage_path(paths, run_id, snapshot_id, snapshot_path)`
  is shared with `load_snapshot_record`; never re-implement an ad-hoc path
  check at a new call site.

### 3. Contracts

- Input is `snapshot_ids: Vec<String>` (deduped in place); empty input is a
  no-op returning empty results, not an error. Single delete, multi-select
  delete, and delete-all share this one command.
- Per-item flow: pre-check (existence, active-run blocker, storage-path
  validation) for the WHOLE batch BEFORE any file removal, then
  `fs::remove_file` (a missing file counts as already deleted and succeeds),
  then ONE `IMMEDIATE` transaction deleting the rows whose files were removed.
- Active-run blocker: a snapshot whose `run_id` has status
  `applying` / `restoring` / `rollback_failed` must be refused; those journals
  reference the snapshot for crash recovery.
- Deletion order is file-first, DB-second. A DB commit failure after file
  removal leaves rows pointing at missing files, which fail closed on restore
  (mirrored risk of create's file-first/insert-second); never invert the order
  so a live row never keeps a removed file.
- Infrastructure failures (lock unavailable, permission audit, DB commit) fail
  the whole command with `Err(AppError)`; per-item problems never do.
- Pending restore previews (kind='restore', status='previewed') are NOT
  blockers; executing one against a deleted snapshot fails closed with
  NOT_FOUND inside `load_snapshot_record`. Do not add JSON-envelope scans.

### 4. Validation & Error Matrix

- Unknown id (or empty batch entry) -> per-item `NOT_FOUND`.
- Snapshot's run status in `applying`/`restoring`/`rollback_failed` ->
  per-item `CONFLICT`; file and row untouched.
- `snapshot_path` mismatching `<snapshots_root>/<run_id>/<snapshot_id>.snapshot`
  or parent canonicalizing outside the snapshots root -> per-item `CONFLICT`;
  the file must NOT be removed (anti-impersonation guard).
- `fs::remove_file` failure -> per-item `ATOMIC_WRITE_FAILED`
  (`AppError::atomic_write(path, "remove_snapshot")`).
- Lock/audit/DB commit failure -> command-level `WRITE_IN_PROGRESS` /
  `PERMISSION_DENIED` / `DATABASE_ERROR`.

### 5. Good/Base/Bad Cases

- Good: mixed batch where blocked and free snapshots coexist; free ones are
  deleted, blocked ones reported with their codes.
- Base: single-id batch (single delete is just batch of one); empty batch.
- Bad: any branch that deletes a DB row whose file removal failed, or that
  removes a file whose path failed validation.

### 6. Tests Required

- Batch success removes rows and files; missing-file self-heal counts as
  deleted.
- All three active statuses reject (file + row preserved) while an unaffected
  item in the same batch still deletes.
- Unknown id in a mixed batch is `NOT_FOUND` without blocking the others.
- Impersonated storage path is refused and the real file survives.
- Duplicate ids delete once; empty input is a no-op.

### 7. Wrong vs Correct

#### Wrong

```rust
// Per-item interleave: check item, remove file, delete row, next item.
// A mid-batch DB failure leaves earlier items half-deleted.
for id in ids {
    let row = load_row(id)?;
    check_blockers(&row)?;
    fs::remove_file(&row.snapshot_path)?;
    delete_row(id)?; // one transaction per item
}
```

#### Correct

```rust
// Pre-check the whole batch, then remove files, then one transaction.
let plan = plan_deletions(database, paths, &ids)?; // per-item pre-checks
remove_files(&plan)?;                              // per-item results
delete_rows(database, &plan.removable_ids)?;       // single IMMEDIATE tx
```
