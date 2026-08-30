# Database Guidelines

> Database patterns and conventions for this project.

---

## Overview

The desktop backend uses bundled SQLite through `rusqlite`. The database is the
structured source of truth; native Claude/Codex files are synchronization
targets and must not replace relational constraints with unvalidated JSON.

## Query Patterns

- Enable and verify `foreign_keys=ON` and `journal_mode=WAL` for every
  connection. Setting a PRAGMA without checking its returned/effective value is
  insufficient.
- Use explicit transactions for multi-statement state changes.
- Keep project/global inheritance and parent/child sync relationships enforced
  twice: once in domain validation and once with SQLite constraints/triggers.
- Cross-table triggers must protect both `INSERT` and `UPDATE`; insert-only
  protection can be bypassed by changing a key after creation.

## Migrations

- Migrations are embedded under `src-tauri/src/db/migrations/` and applied in an
  `IMMEDIATE` transaction together with their `schema_migrations` record.
- Back up the existing database and any active WAL/SHM files before migration.
- Treat migration history as an ordered prefix of the compiled migration list;
  reject unknown, renamed, or out-of-order records.
- Migration tests must use `tempfile` roots and must prove reopening is
  idempotent. Never point a test at a developer database.

### Scenario: In-place schema-text revision for CHECK-only changes

背景：`managed_targets` 被三张子表外键引用（`managed_items` CASCADE、
`sync_targets`/`snapshots` RESTRICT），且迁移事务内 `PRAGMA foreign_keys=OFF`
是 no-op；为放开项目作用域 `artifact_kind` CHECK 而重建表会触发隐式
`DELETE` 的级联清空或 RESTRICT 失败。

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
CREATE TABLE prompt_project_assignments (...); -- 普通 DDL 触发重解析
```

要点：

- UPDATE 的 `WHERE` 必须锚定旧文本（`instr(...) > 0`），幂等且防止空改。
- 迁移事务内**不能**用 INSERT 金丝雀验证新 CHECK（同连接缓存仍旧）；
  生效性由迁移测试在 `Database::open` 后的金丝雀插入断言
  （`prompt_project_assignment_migration_rewrites_managed_targets_check`）。
- 基线行是**永久行**：解除纳管清空 `baseline_full_hash` /
  `baseline_managed_hash` / `baseline_projection_json`（两者同 NULL），
  不删除行——快照 RESTRICT 外键引用行 id，删除会失败；NULL 基线 +
  观测内容即规范中的中性 `PROJECT_TARGET_INITIAL_UNMANAGED` 语义。
- 同形 CHECK 字符串可能出现在多张表（如 `tool IN ('claude','codex')` 同时
  存在于 `provider_profiles` 与 `prompt_profiles`）：`WHERE` 必须额外限定
  `name = '<目标表>'`，否则 replace 会误伤其他表（0009 先例）。
- 当列的语义整体作废而表无法重建时（0009：`prompt_profiles` 被以
  RESTRICT 外键引用），保留旧列 + `ALTER TABLE ADD COLUMN` 新语义列是
  首选：每工具启用位 `is_active_claude`/`is_active_codex` + 各自部分唯一
  索引接替 `uq_prompt_profiles_one_active_per_tool`；旧列清零（`UPDATE`）
  并在迁移注释里标记为遗留，代码路径不得再读。

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
  permissions, backs up an existing SQLite/WAL/SHM set, verifies PRAGMAs, and
  applies the ordered migration prefix.
- Main records use UUID text IDs and optimistic `row_version`; previews reference
  exact entity versions rather than an untyped database timestamp.

### 3. Contracts

- Private directories are `0700`; the database, WAL/SHM, backup, journal, and
  snapshot files are `0600`.
- The schema contains provider/prompt/MCP/skill/project entities, four explicit
  assignment tables, managed targets/items, sync runs/items, snapshots, and the
  `app_settings` key-value table for singleton user preferences (no
  `row_version`; unknown stored enum values fail closed with `DATABASE_ERROR`).
- Every stored JSON value validates its expected top-level shape. Every stored
  hash is lowercase SHA-256. Global inheritance is not represented by duplicate
  project assignments.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Relative, root, broad, symlinked, or special private path | `INVALID_INPUT` or `PERMISSION_DENIED`; create nothing outside the root |
| Effective WAL/foreign-key PRAGMA differs | stable database error; startup stops |
| Migration history is renamed, unknown, or out of order | stable migration error; no later migration runs |
| Global/project assignment duplicates on `INSERT` or `UPDATE` | SQLite trigger conflict plus matching domain conflict |
| `row_version` decreases | reject; unchanged version is atomically bumped |

### 5. Good/Base/Bad Cases

- Good: open a canonical `tempfile` root, create the schema, reopen it, and
  observe identical migration history with stricter permissions.
- Base: a new root has no startup backup and starts at the compiled schema version.
- Bad: an ancestor symlink, broken SQLite sidecar link, forged migration row, or
  assignment-key update must fail closed.

### 6. Tests Required

- Assert WAL and foreign keys after open, idempotent reopen, and recoverable
  backup of an active WAL database.
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
