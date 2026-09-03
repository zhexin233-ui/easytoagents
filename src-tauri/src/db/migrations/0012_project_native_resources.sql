-- 项目原生资源观察与可恢复禁用状态。managed_targets 仅复用为规范化目标身份，
-- 空 baseline、无 managed_items 不构成 ownership。被引用的禁用快照不可删除。

CREATE TABLE project_native_resources (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    target_id TEXT NOT NULL REFERENCES managed_targets(id) ON UPDATE CASCADE ON DELETE CASCADE,
    external_key TEXT NOT NULL CHECK(length(external_key) > 0 AND instr(external_key, char(0)) = 0),
    entry_type TEXT NOT NULL CHECK(entry_type IN ('mcp_entry', 'directory', 'symlink', 'prompt_file')),
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
