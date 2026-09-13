# Database Guidelines

> Database patterns and conventions for this project.

---

## Overview

The desktop backend uses bundled SQLite through `rusqlite`. The database is the
structured source of truth; native Claude/Codex/Cursor files are synchronization
targets and must not replace relational constraints with unvalidated JSON.

Prompt is a global-only resource in the current schema. Project Prompt
assignments, project Prompt targets, and PromptFile observations are retired;
project-native observation now covers supported MCP, Skill, and read-only Hook /
Agent-file resources (Hook and Agent project assignment/status remains separate
from this observation table).
Migration 0018 removes historical project Prompt rows and private snapshots
without reading or deleting files under a registered project root.

## Query Patterns

- Enable and verify `foreign_keys=ON` and `journal_mode=WAL` for every
  connection. Setting a PRAGMA without checking its returned/effective value is
  insufficient.
- Use explicit transactions for multi-statement state changes.
- Keep project/global inheritance and parent/child sync relationships enforced
  twice: once in domain validation and once with SQLite constraints/triggers.
- Cross-table triggers must protect both `INSERT` and `UPDATE`; insert-only
  protection can be bypassed by changing a key after creation.
- Use `prepare_cached` for every statement in `src/db/`; SQLite's statement cache
  makes repeated list/lookup calls skip re-parsing.
- Decode a `tool` column with `db::column_tool(row, index)`; it is the single
  `Tool::from_stable_str` entry point for SQLite rows and maps unknown values to
  `rusqlite::Error::InvalidQuery` like the other `*_from_database` helpers. Never
  hand-write a `"claude" => Tool::Claude` match in a row mapper.
- Keep cross-cutting synchronization SQL (`sync_runs`, `sync_items`,
  `snapshots`, `managed_targets`, and active-writer checks) in `src/db/sync.rs`.
  Services and the apply engine should call typed helpers instead of embedding
  those table statements beside filesystem orchestration.
- List endpoints issue a constant number of statements: fetch the records, then one
  aggregated query for the per-record relation (`global_tools_for_all_skills`,
  `global_tools_for_all_mcp`, `global_assignments_for_all_hooks`) and assemble in
  memory. Never call a per-id lookup inside a `map` over records. Row versions for a
  set of ids come from one `WHERE id IN (...)` (`row_versions_by_id`).
  `skills::service::tests::list_skills_issues_a_constant_number_of_sql_statements`
  guards this with the rusqlite trace hook.
- `hash_json` serializes the `serde_json::Value` directly; the crate must keep
  `preserve_order` off so `Map` stays a `BTreeMap` with sorted keys
  (`sync::tests::serde_json_serializes_object_keys_in_sorted_order`).

## Migrations

- Migrations are embedded under `src-tauri/src/db/migrations/` and applied in an
  `IMMEDIATE` transaction together with their `schema_migrations` record.
- Back up the existing database only when at least one compiled migration is
  still pending. Run `PRAGMA wal_checkpoint(TRUNCATE)` first so the copied main
  file is self-contained, then copy the main file plus any still-present
  WAL/SHM. Keep only the newest `STARTUP_BACKUP_RETENTION` (3) `startup-*`
  directories; pruning is best-effort and never blocks startup. Backups carry
  Provider credentials, so unconditional per-launch copies are a leak surface,
  not a safety net.
- `configure_connection` runs once per open. PRAGMAs are connection state and
  survive migration commits; do not re-run it after `run_migrations`.
- Treat migration history as an ordered prefix of the compiled migration list;
  reject unknown, renamed, or out-of-order records.
- Migration tests must use `tempfile` roots and must prove reopening is
  idempotent. Never point a test at a developer database.

### Migration strategy for future schema changes

Published migrations are immutable. Do not edit an existing migration or use
`PRAGMA writable_schema` in a new migration. For a future change that needs to
alter a table shape, use a normal transactional table rebuild with this order:

1. Start the migration's `IMMEDIATE` transaction.
2. Run SQL preconditions in the migration itself and abort on a mismatch.
3. Create the replacement table with the complete current schema and new
   constraints.
4. Copy columns explicitly from the old table.
5. Validate copied row counts and required data in SQL.
6. Drop dependent triggers before dropping the old table.
7. Drop the old table only after the replacement is populated.
8. Rename the replacement table to the published table name.
9. Recreate indexes, foreign keys, and cross-table triggers.
10. Run `PRAGMA foreign_key_check` and abort if it returns any row.
11. Run `PRAGMA integrity_check` and require the result `ok`.
12. Record the migration version and commit the transaction.

All preconditions belong in the `.sql` migration, not in a Rust branch that
silently chooses a different path. Existing migrations containing historical
schema-text edits remain untouched; this rule applies to migrations added from
now on.

### Scenario: 0021/0024 table rebuild widening a CHECK (canonical precedent)

`0021_project_native_hook_entries.sql` is the first published migration to use
the 12-step transactional table rebuild, widening
`project_native_resources.entry_type` with `hook_entry`; migration
`0024_project_native_agent_files.sql` repeats the same shape for `agent_file`.
Copy this shape for future CHECK/shape changes instead of `writable_schema`:

- SQL-only preconditions: a TEMP table whose column has `CHECK(x = 1)` aborts
  the whole migration transaction when the INSERT feeding it does not satisfy
  the anchor (`COUNT(*)` over `sqlite_master` matching the exact 0018-era CHECK
  text exactly once). `RAISE()` alone cannot be used outside triggers.
- The replacement table is byte-for-byte the 0012 shape with only the CHECK
  widened; copy all columns explicitly and assert row-count equality through a
  second CHECK-validated TEMP table before dropping the old table.
- Drop dependent triggers first (`DROP TRIGGER IF EXISTS`), including the
  cross-table `trg_snapshots_reject_native_resource_id_update` on `snapshots`
  whose body references the rebuilt table (0016 lesson: `DROP TABLE` re-parses
  the schema). Recreate indexes, both row-version triggers, and the snapshots
  cross-protection trigger verbatim.
- Finish with `pragma_foreign_key_check` / `pragma_integrity_check`
  table-valued functions feeding CHECK-validated TEMP tables, so any violation
  aborts the migration, then drop the TEMP tables.
- Migration tests (`project_native_hook_entries_migration_preserves_rows_and_widens_check`
  and `project_native_agent_files_migration_preserves_rows_and_widens_check`)
  builds a v20 database with `mcp_entry`/`directory`/`symlink` rows including a
  disabled row holding a snapshot, then prove preservation, their new entry-type
  acceptance, `prompt_file` rejection, the row-version bump trigger, the snapshots
  id-update trigger, and idempotent reopen. The Agent-file migration preserves the
  existing MCP/Skill/Hook rows and adds only the `agent_file` CHECK value.

### Scenario: In-place schema-text revision for CHECK-only changes (historical)

历史背景：`managed_targets` 被三张子表外键引用（`managed_items` CASCADE、
`sync_targets`/`snapshots` RESTRICT），且迁移事务内 `PRAGMA foreign_keys=OFF`
是 no-op；早期迁移曾为项目 Prompt 放宽 `artifact_kind` CHECK。该段只说明
已发布迁移的历史实现，当前 v18 已收紧项目 CHECK，不能据此重新开放项目 Prompt。

#### Wrong

```sql
-- 迁移事务内重建被外键引用的表：DROP 的隐式 DELETE 会级联清空
-- managed_items，或被 sync_targets/snapshots 的 RESTRICT 中止。
DROP TABLE managed_targets;
```

#### Correct

```sql
-- 0008: 官方支持的 sqlite_schema 文本原地修订；同迁移内的普通 DDL
-- （CREATE TABLE）会推进 schema cookie。事务内同连接的 schema 缓存
-- 不会自动重解析，由 run_migrations 在每次迁移提交后显式推进
-- PRAGMA schema_version 强制重解析（见 db/mod.rs）。
PRAGMA writable_schema = ON;
UPDATE sqlite_master
SET sql = replace(sql,
    'artifact_kind IN (''mcp'', ''skill''))',
    'artifact_kind IN (''mcp'', ''skill'', ''prompt''))')
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''mcp'', ''skill''))') > 0;
PRAGMA writable_schema = OFF;
CREATE TABLE prompt_project_assignments (...); -- 0008 历史 DDL，触发重解析
```

要点：

- UPDATE 的 `WHERE` 必须锚定旧文本（`instr(...) > 0`），幂等且防止空改。
- 迁移事务内**不能**用 INSERT 金丝雀验证新 CHECK（同连接缓存仍旧）；
  早期生效性由迁移测试在 `Database::open` 后的金丝雀插入断言
  （`prompt_project_assignment_migration_rewrites_managed_targets_check`）。
  该测试与 0008 仅是历史证据，不是当前 API 合同。
- 基线行是**永久行**：解除纳管清空 `baseline_full_hash` /
  `baseline_managed_hash` / `baseline_projection_json`（两者同 NULL），
  不删除行——快照 RESTRICT 外键引用行 id，删除会失败；NULL 基线 +
  观测内容即规范中的中性 `PROJECT_TARGET_INITIAL_UNMANAGED` 语义。
- 导入接管（`adopt_baseline`）对**孤儿基线**必须刷新而不是报 CONFLICT：
  导入事务先经 `reject_existing_profiles` / `reject_prompt_import_blocked`
  保证没有任何生效或同源档案引用目标，因此既有基线必然无人引用（删光
  档案后基线残留是常态）。刷新走 `UPDATE ... WHERE id = ? AND row_version = ?`
  并把 `last_status` 置 `in_sync`（同 `mcp_imports` 的 extend 模式）；
  `row_version` 由触发器在 UPDATE 后自动递增，禁止手动 SET。该守卫是
  IMMEDIATE 事务内的 fail-closed 防御分支，不承担跨事务并发检测。
- 同形 CHECK 字符串可能出现在多张表（如 `tool IN ('claude','codex')` 同时
  存在于 `provider_profiles` 与 `prompt_profiles`）：`WHERE` 必须额外限定
  `name = '<目标表>'`，否则 replace 会误伤其他表（0009 先例）。
- 0010 只把 Cursor 放进 MCP/Skill 的全局/项目分配、导入预览，以及
  `managed_targets` 的 `mcp`/`skill` 组合；Provider、Prompt、项目 Rules 与
  Cursor 的任意其他 artifact 组合必须继续由表级 CHECK 拒绝。执行
  `writable_schema` 文本改写前，运行时代码必须验证每张目标表的精确旧锚点；
  缺失或重复锚点应让迁移事务失败，不能静默推进版本。
- 0012 只新增 `project_native_resources`、索引、row-version trigger，以及对
  `snapshots.id` UPDATE 的交叉 RESTRICT。不要改写历史 migration 文本。
  `managed_targets` 在这里只是规范化目标身份：允许插入空 `baseline_*` 且无
  `managed_items` 的行；那不是 ownership。`disabled`/`conflict` 必须带
  `disabled_snapshot_id`；`ON DELETE RESTRICT` 是备份，产品删除入口仍须走
  `snapshot_is_referenced`。
- 当列的语义整体作废而表无法重建时（0009：`prompt_profiles` 被以
  RESTRICT 外键引用），保留旧列 + `ALTER TABLE ADD COLUMN` 新语义列是
  首选：每工具启用位 `is_active_claude`/`is_active_codex` + 各自部分唯一
  索引接替 `uq_prompt_profiles_one_active_per_tool`；旧列清零（`UPDATE`）
  并在迁移注释里标记为遗留，代码路径不得再读。

### Scenario: v18 removal of historical project Prompt state

Migration `0018_remove_project_prompts.sql` is the forward-only compatibility
boundary for databases that passed through the historical project Prompt
feature. It must not rewrite migrations 0001–0017 or infer ownership from a
project file.

### 1. Scope / Trigger

- Trigger: opening a v17 (or older supported-prefix) database after the project
  Prompt capability has been removed, including schema checks, row cleanup, and
  private snapshot cleanup.

### 2. Signatures

- `Database::open(&AppPaths) -> Result<Database, AppError>` applies migration
  0018 in one `IMMEDIATE` transaction, then retries the cleanup queue.
- `retired_snapshot_cleanup(snapshot_id, run_id, snapshot_path, storage_kind,
  content_hash, queued_at)` records retired snapshot metadata before source rows
  are deleted.
- `process_retired_snapshot_cleanup(&Connection, &AppPaths) -> Result<_, _>`
  accepts only queue rows whose path is derived from the private snapshots root.

### 3. Contracts

- Materialize project-scope Prompt target IDs before deleting any parent row.
- Record every related snapshot's `snapshot_id`, `run_id`, `snapshot_path`,
  `storage_kind`, and `content_hash` before deleting native-resource, snapshot,
  or sync-item rows.
- Remove only project Prompt native-resource rows, their snapshots and sync
  items, empty historical runs, project Prompt targets, and the assignment
  table. Mixed runs and records for global Prompt, MCP, Skill, or Hook targets
  remain intact.
- Tighten the `managed_targets` project `artifact_kind` CHECK to
  `mcp|skill|hook`, and `project_native_resources.entry_type` to
  `mcp_entry|directory|symlink`, using exact `sqlite_schema` anchors. Global
  Prompt profiles, assignments, targets, imports, snapshots, and runs remain
  valid.
- After the transaction commits, remove only a validated regular `payload_file`
  under `AppPaths.snapshots/<run_id>/` with UUID identity and exact
  `<snapshot_id>.snapshot` path. Missing files count as complete; symlinks,
  special files, invalid paths, and non-payload storage kinds stay queued.
- Cleanup never consults `managed_targets.target_path` and never touches a
  registered project's `CLAUDE.md`, `AGENTS.md`, Cursor rules, or any other
  project file. Retired private snapshot deletion is not reversible by the app.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Missing or duplicate old CHECK anchor | Migration aborts with a stable migration error |
| Project Prompt snapshot is referenced by a mixed run | Snapshot/run remain; only retired rows are removed |
| Queue path escapes the private snapshots root | Queue row remains; no filesystem mutation |
| Queue path is symlink/special/non-regular | Queue row remains; no filesystem mutation |
| Queue storage kind is not `payload_file` | Queue row remains for a later retry |
| Snapshot file is already missing | Queue row is marked complete/removed |
| Reopen after successful v18 migration | No duplicate rows or deletes; schema remains v18 |

### 5. Good/Base/Bad Cases

- Good: upgrade an isolated old database, preserve global and mixed-run rows,
  delete only validated private retired payloads, and reopen successfully.
- Base: a missing payload is treated as already cleaned; an unsafe or unsupported
  queue entry remains for a future retry.
- Bad: rebuild a referenced table, derive cleanup from a project target path,
  delete a project file, or silently ignore a missing schema anchor.

### 6. Tests Required

- Seed v17 state containing project Prompt, global Prompt, MCP/Skill/Hook, mixed
  runs, native-resource snapshots, and project files; assert exact row/file
  preservation and removal after `Database::open`.
- Assert the v18 CHECK canaries reject project Prompt targets and `prompt_file`
  entries, while global Prompt inserts still succeed.
- Exercise idempotent reopen, missing/symlink/special/outside queue paths,
  non-payload queue kinds, FK integrity, and unchanged project-file bytes.

### 7. Wrong vs Correct

#### Wrong

```rust
// Do not trust a historical target_path: it may point into a registered project.
fs::remove_file(managed_target.target_path)?;
```

#### Correct

```rust
// Derive and validate the private snapshot path from queue identity only.
let path = paths.snapshots().join(&run_id).join(format!("{snapshot_id}.snapshot"));
validate_private_payload_path(&path, &paths.snapshots(), &snapshot_id)?;
remove_regular_payload_if_present(&path)?;
```

## Naming Conventions

- Tables and columns use `snake_case`; indexes use `uq_` or `idx_`; triggers use
  `trg_`.
- Main records use lowercase UUID text IDs plus `created_at`, `updated_at`, and
  monotonically increasing `row_version`.
- Automatic row-version triggers key off `NEW.row_version = OLD.row_version`,
  not timestamp equality. A caller may explicitly change `updated_at`, and that
  must not bypass the version bump.
- Persisted hashes are lowercase 64-character hexadecimal SHA-256 values.
- JSON columns validate both `json_valid` and their expected top-level type.

## Common Mistakes

- Do not use `Path::exists` for security-sensitive database or sidecar checks;
  it hides broken symlinks. Use `symlink_metadata` and reject links/special
  files.
- Do not rely on a partial unique index alone for global/project inheritance;
  the invariant crosses tables and needs triggers in both directions.
- Do not use `updated_at` equality as the row-version bump guard.

## Scenario: Private SQLite bootstrap and managed-state schema

### 1. Scope / Trigger

- Trigger: any change to `AppPaths`, `Database::open`, an embedded migration,
  a managed entity, assignment, preview run, or snapshot record.

### 2. Signatures

- `AppPaths::from_data_root(impl Into<PathBuf>) -> Result<AppPaths, AppError>` accepts an
  explicit private root; it never reads process `HOME` or tool configuration.
- `Database::open(&AppPaths) -> Result<Database, AppError>` tightens private
  permissions, verifies PRAGMAs, backs up an existing SQLite/WAL/SHM set only
  when migrations are pending (after a WAL checkpoint, pruned to 3 copies), and
  applies the ordered migration prefix.
- Main records use UUID text IDs and optimistic `row_version`; previews reference
  exact entity versions rather than an untyped database timestamp.

### 3. Contracts

- Private directories are `0700`; the database, WAL/SHM, backup, journal,
  snapshot, and log files are `0600`.
- The schema contains provider/prompt/MCP/skill/project entities, global profile
  state, MCP/Skill/Hook global and project assignments, managed targets/items,
  `project_native_resources` for MCP/Skill/Hook/Agent-file observations, sync runs/items,
  snapshots, and the `app_settings` key-value table for singleton user
  preferences (no `row_version`; unknown stored enum values fail closed with
  `DATABASE_ERROR`). Prompt profile state and its native targets are global-only.
- Every stored JSON value validates its expected top-level shape. Every stored
  hash is lowercase SHA-256. Global inheritance is not represented by duplicate
  project assignments.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Relative, root, broad, symlinked, or special private path | `INVALID_INPUT` or `PERMISSION_DENIED`; create nothing outside the root |
| Effective WAL/foreign-key PRAGMA differs | stable database error; startup stops |
| Migration history is renamed, unknown, or out of order | stable migration error; no later migration runs |
| Global/project MCP, Skill, or Hook assignment duplicates on `INSERT` or `UPDATE` | SQLite trigger conflict plus matching domain conflict |
| `row_version` decreases | reject; unchanged version is atomically bumped |

### 5. Good/Base/Bad Cases

- Good: open a canonical `tempfile` root, create the schema, reopen it, and
  observe identical migration history with stricter permissions.
- Base: a new root has no startup backup and starts at the compiled schema version;
  reopening an up-to-date database also produces no backup.
- Bad: an ancestor symlink, broken SQLite sidecar link, forged migration row, or
  assignment-key update must fail closed.

### 6. Tests Required

- Assert WAL and foreign keys after open, idempotent reopen, no backup when no
  migration is pending, and a recoverable (checkpointed) backup of an
  active-WAL database that still has pending migrations, pruned to 3 copies.
- Exercise every cross-table assignment trigger through both `INSERT` and `UPDATE`.
- Assert `0700`/`0600`, ancestor-symlink rejection, broken-sidecar rejection,
  row-version monotonicity, UUID/JSON/hash/path checks, and parent-kind checks.
- All filesystem/database tests use canonicalized `tempfile` roots.

### 7. Wrong vs Correct

#### Wrong

```rust
let path = std::env::var("HOME")?;
Connection::open(format!("{path}/Library/Application Support/app.sqlite"))?;
```

#### Correct

```rust
let paths = AppPaths::from_data_root(explicit_isolated_root)?;
let database = Database::open(&paths)?;
```

## Scenario: `project_native_resources` identity and snapshot guards

### 1. Scope / Trigger

- Trigger: any change to migration `0012_project_native_resources.sql`,
  migration `0021_project_native_hook_entries.sql`, migration
  `0024_project_native_agent_files.sql`,
  `db/native_resources.rs`, native-resource CAS, target-identity upsert, snapshot
  FK/RESTRICT, `soft_remove_project`, or `delete_snapshots` reference checks.
- Current project-native observation covers MCP entries, Skill directories or
  links, and (since 0021/0024, read-only) Hook and Agent-file entries. Prompt/Rules files are outside
  this model and are never observed or exposed as disable/restore resources.

### 2. Signatures

- Table `project_native_resources`: UUID `id`, `target_id` → `managed_targets(id)`
  `ON DELETE CASCADE`, `external_key`, `entry_type` in
  `mcp_entry|directory|symlink|hook_entry|agent_file` (0021/0024 widened the CHECK
  by table rebuild), `state` in
  `active|disabled|missing|conflict`, optional SHA-256 `observed_item_hash`,
  `disabled_snapshot_id` → `snapshots(id)` `ON DELETE RESTRICT`, `disabled_at`,
  timestamps, `row_version`. Unique `(target_id, external_key)`.
- `insert_project_target_identity(...)` inserts only
  `id, tool, artifact_kind, scope='project', project_id, target_path` when no
  row exists; baselines stay NULL. Valid project artifact kinds are `mcp`,
  `skill`, `hook`, and `agent`; Prompt is not a project target. Agent identity
  rows use the containing directory path; individual `agent_file` rows use the
  filename as `external_key` and remain read-only.
- `snapshot_is_referenced(connection, snapshot_id, database_path) -> bool`
- `count_blocking_native_resources(tx, project_id, database_path) -> u32`
  counts `disabled` + `conflict` for that project.
- Compiled schema version is `24` (`src-tauri/src/app/mod.rs` assertion).

### 3. Contracts

- CHECK: `active`/`missing` must have NULL snapshot and `disabled_at`;
  `disabled`/`conflict` must have both. Row-version triggers follow the shared
  decrease-abort / same-version bump pattern.
- `trg_snapshots_reject_native_resource_id_update` refuses `UPDATE snapshots SET id`
  while a native row references the old id.
- Identity upsert does not write `baseline_full_hash`, `baseline_managed_hash`,
  `baseline_projection_json`, or `managed_items`.
- Project scans may create empty-baseline identity rows only for supported MCP,
  Skill, Hook, or Agent-directory targets. Such a row is observation scaffolding,
  not ownership; it must not create an empty project file or widen ordinary Apply.
  Agents synchronization must first exclude the Agent directory identity path with
  `agent_file_descriptor`; only exact file paths are sync targets.
- Product deletion of snapshots still goes through `delete_snapshots` +
  `snapshot_is_referenced`. Project removal calls
  `count_blocking_native_resources` inside the same IMMEDIATE transaction as
  assignment deletes.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| `disabled` row without snapshot | SQLite CHECK abort |
| Delete snapshot row still referenced | SQLite RESTRICT plus command-level `CONFLICT` |
| Decrease `row_version` | `ROW_VERSION_MUST_INCREASE` |
| Project Prompt target or PromptFile entry supplied after v18 | reject/fail closed; no native read or write |
| Remove project while native `disabled`/`conflict` > 0 | domain `CONFLICT`; transaction rolls back |
| Reopen after 0018 | idempotent; schema version stays 18 |

### 5. Good/Base/Bad Cases

- Good: register or rescan an old project, lazily insert active MCP/Skill/Hook/
  Agent-file observations, and leave all project Prompt/Rules files untouched.
- Base: an empty-baseline identity row exists after registration; ordinary Apply
  still has no target until a supported assignment exists.
- Bad: create a Prompt project target, infer ownership from an empty baseline,
  rebuild `managed_targets`, or delete snapshots through a helper that skips
  `delete_snapshots`.

### 6. Tests Required

- Old-database upgrade, idempotent reopen, CHECK/unique/CAS failures, and the
  v18 canaries that reject project Prompt targets and `prompt_file` entries.
- Empty identity is not treated as ownership and does not create project files;
  project scans do not read Prompt/Rules files.
- Agent directory identity rows do not enter Agents deletion candidates; with no
  assignments the Agent preview has zero targets, while direct top-level files
  remain byte-for-byte unchanged. Applied Agent file targets hide by exact path
  only after a non-empty baseline is present.
- Referenced snapshot survives `delete_snapshots`; `soft_remove_project` refuses
  while supported native rows are disabled or conflicted.

### 7. Wrong vs Correct

#### Wrong

```sql
-- 改历史 migration 或在新 API 里单独删 snapshots，绕过 delete_snapshots。
DROP TABLE snapshots;
```

#### Correct

```sql
-- 0012: 新表 + RESTRICT。删除入口仍调用 snapshot_is_referenced。
CREATE TABLE project_native_resources (
    ...
    disabled_snapshot_id TEXT REFERENCES snapshots(id) ON DELETE RESTRICT,
    CHECK (
        (state IN ('active', 'missing') AND disabled_snapshot_id IS NULL)
        OR (state IN ('disabled', 'conflict') AND disabled_snapshot_id IS NOT NULL)
    )
);
```
