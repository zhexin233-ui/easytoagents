-- 放宽 project_native_resources.entry_type CHECK 以允许 'hook_entry'：
-- 项目级 Hooks（Claude settings.json / Codex hooks.json / Cursor hooks.json /
-- ZCode config.json）按条目只读登记为项目原生资源。
--
-- 该表没有被任何表以外键引用（只引用 managed_targets 与 snapshots），按
-- database-guidelines 的 12 步表重建执行：本文件内先校验 0018 之后的精确
-- CHECK 锚点，再建新表、显式复制、行数校验、删触发器、换表、重建索引与
-- 触发器，最后做 foreign_key_check 与 integrity_check。禁止 writable_schema。

-- 步骤 2（前置校验）：旧 CHECK 锚点必须精确存在一次，否则中止迁移。
CREATE TEMP TABLE migration_0021_entry_type_anchor (
    matched INTEGER NOT NULL CHECK(matched = 1)
);
INSERT INTO migration_0021_entry_type_anchor(matched)
    SELECT COUNT(*)
    FROM sqlite_master
    WHERE type = 'table'
      AND name = 'project_native_resources'
      AND sql IS NOT NULL
      AND instr(sql, 'entry_type IN (''mcp_entry'', ''directory'', ''symlink'')') > 0;

-- 步骤 3：新表完整复制 0012 的列与约束，仅 entry_type CHECK 增补 'hook_entry'。
CREATE TABLE project_native_resources_new (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    target_id TEXT NOT NULL REFERENCES managed_targets(id) ON UPDATE CASCADE ON DELETE CASCADE,
    external_key TEXT NOT NULL CHECK(length(external_key) > 0 AND instr(external_key, char(0)) = 0),
    entry_type TEXT NOT NULL CHECK(entry_type IN ('mcp_entry', 'directory', 'symlink', 'hook_entry')),
    state TEXT NOT NULL CHECK(state IN ('active', 'disabled', 'missing', 'conflict')),
    observed_item_hash TEXT CHECK(
        observed_item_hash IS NULL OR (
            length(observed_item_hash) = 64 AND observed_item_hash = lower(observed_item_hash)
            AND observed_item_hash NOT GLOB '*[^0-9a-f]*'
        )
    ),
    disabled_snapshot_id TEXT REFERENCES snapshots(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    disabled_at TEXT,
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    UNIQUE(target_id, external_key),
    CHECK(
        (state IN ('active', 'missing') AND disabled_snapshot_id IS NULL AND disabled_at IS NULL)
        OR
        (state IN ('disabled', 'conflict') AND disabled_snapshot_id IS NOT NULL AND disabled_at IS NOT NULL)
    )
);

-- 步骤 4（显式列复制）与步骤 5（行数校验）。
INSERT INTO project_native_resources_new(
    id, target_id, external_key, entry_type, state, observed_item_hash,
    disabled_snapshot_id, disabled_at, last_seen_at, created_at, updated_at, row_version
)
SELECT id, target_id, external_key, entry_type, state, observed_item_hash,
       disabled_snapshot_id, disabled_at, last_seen_at, created_at, updated_at, row_version
FROM project_native_resources;

CREATE TEMP TABLE migration_0021_row_count (
    diff INTEGER NOT NULL CHECK(diff = 0)
);
INSERT INTO migration_0021_row_count(diff)
    SELECT (SELECT COUNT(*) FROM project_native_resources)
         - (SELECT COUNT(*) FROM project_native_resources_new);

-- 步骤 6：先删依赖触发器（0016 教训：DROP TABLE 会重解析 schema，此时
-- 仍引用旧表的触发器会报 "no such table"），再删旧表并重命名新表。
DROP TRIGGER IF EXISTS trg_project_native_resources_row_version_guard;
DROP TRIGGER IF EXISTS trg_project_native_resources_row_version_bump;
DROP TRIGGER IF EXISTS trg_snapshots_reject_native_resource_id_update;
DROP TABLE project_native_resources;
ALTER TABLE project_native_resources_new RENAME TO project_native_resources;

-- 步骤 7：重建索引、row_version 触发器与 snapshots 交叉保护触发器
-- （SQL 与 0012 逐字一致）。
CREATE INDEX idx_project_native_resources_target
    ON project_native_resources(target_id, state, external_key);
CREATE INDEX idx_project_native_resources_snapshot
    ON project_native_resources(disabled_snapshot_id);
CREATE INDEX idx_project_native_resources_state
    ON project_native_resources(state);

CREATE TRIGGER trg_project_native_resources_row_version_guard
BEFORE UPDATE ON project_native_resources WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;

CREATE TRIGGER trg_project_native_resources_row_version_bump
AFTER UPDATE ON project_native_resources WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE project_native_resources
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

-- 交叉保护：快照 UPDATE 改 id 时仍须拒绝被原生禁用记录引用的行。
CREATE TRIGGER trg_snapshots_reject_native_resource_id_update
BEFORE UPDATE OF id ON snapshots
WHEN EXISTS (
    SELECT 1 FROM project_native_resources
    WHERE disabled_snapshot_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'SNAPSHOT_REFERENCED_BY_NATIVE_RESOURCE');
END;

-- 步骤 8：外键与完整性校验（有违例即中止迁移），并清理临时校验表。
CREATE TEMP TABLE migration_0021_fk_check (
    violations INTEGER NOT NULL CHECK(violations = 0)
);
INSERT INTO migration_0021_fk_check(violations)
    SELECT COUNT(*) FROM pragma_foreign_key_check;

CREATE TEMP TABLE migration_0021_integrity_check (
    result TEXT NOT NULL CHECK(result = 'ok')
);
INSERT INTO migration_0021_integrity_check(result)
    SELECT * FROM pragma_integrity_check;

DROP TABLE migration_0021_entry_type_anchor;
DROP TABLE migration_0021_row_count;
DROP TABLE migration_0021_fk_check;
DROP TABLE migration_0021_integrity_check;
