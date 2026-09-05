-- Hooks 全局级与项目级管理：中央 hooks 表 + 四工具分配表，
-- 并把 managed_targets / managed_items 的 artifact CHECK 放宽到 'hook'。
--
-- 四工具 hooks 官方合同核验（2026-09-05，任务 09-05-add-hooks-management）：
-- Claude settings.json `hooks` 键、Codex 独立 hooks.json、Cursor 独立
-- hooks.json（version: 1）、ZCode config.json `hooks` 键。
--
-- managed_targets / managed_items 被 sync_targets、snapshots、外键与触发器
-- 引用，沿用 0008/0010/0013 已验证的 writable_schema 原地修订；每个替换都
-- 限定表名与旧锚点（instr > 0 防空改），CHECK 生效性由迁移测试金丝雀验证。

PRAGMA writable_schema = ON;

-- managed_targets：受管资源种类加入 'hook'（原 0001 锚点，此前未被改写）。
UPDATE sqlite_master
SET sql = replace(sql,
    'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))',
    'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''hook''))')
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))') > 0;

-- managed_targets：项目作用域资源种类加入 'hook'（当前文本为 0008 改写后的锚点）。
UPDATE sqlite_master
SET sql = replace(sql,
    'artifact_kind IN (''mcp'', ''skill'', ''prompt''))',
    'artifact_kind IN (''mcp'', ''skill'', ''prompt'', ''hook''))')
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''prompt''))') > 0;

-- managed_targets：Cursor 的 artifact 限制加入 'hook'
--（当前文本为 0010 引入、0013 改写工具枚举后的锚点；Cursor 官方支持 hooks）。
UPDATE sqlite_master
SET sql = replace(sql,
    '(tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill''))',
    '(tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook''))')
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, '(tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill''))') > 0;

-- managed_items：per-item 基线资源种类加入 'hook'（原 0001 锚点，此前未被改写）。
UPDATE sqlite_master
SET sql = replace(sql,
    'CHECK(resource_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))',
    'CHECK(resource_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''hook''))')
WHERE type = 'table' AND name = 'managed_items'
  AND instr(sql, 'CHECK(resource_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))') > 0;

PRAGMA writable_schema = OFF;

CREATE TABLE hooks (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    name TEXT NOT NULL COLLATE NOCASE UNIQUE
        CHECK(
            length(name) BETWEEN 1 AND 100 AND name = trim(name)
            AND instr(name, char(0)) = 0
            AND name NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || '-' || char(159) || ']*')
        ),
    -- 统一 PascalCase 事件名（canonical）；Cursor 原生 camelCase 由服务层映射。
    event TEXT NOT NULL CHECK(event IN (
        'SessionStart', 'SessionEnd', 'UserPromptSubmit',
        'PreToolUse', 'PermissionRequest', 'PostToolUse', 'PostToolUseFailure',
        'SubagentStart', 'SubagentStop', 'PreCompact', 'PostCompact',
        'Stop', 'Notification'
    )),
    matcher TEXT
        CHECK(matcher IS NULL OR (
            length(matcher) BETWEEN 1 AND 500 AND matcher = trim(matcher)
            AND instr(matcher, char(0)) = 0
        )),
    command TEXT NOT NULL CHECK(
        length(command) BETWEEN 1 AND 4000 AND command = trim(command)
        AND instr(command, char(0)) = 0
    ),
    timeout_seconds INTEGER CHECK(timeout_seconds IS NULL OR timeout_seconds > 0),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1)
);

CREATE TABLE hook_global_assignments (
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode')),
    hook_id TEXT NOT NULL REFERENCES hooks(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(tool, hook_id)
);

CREATE TABLE hook_project_assignments (
    project_id TEXT NOT NULL REFERENCES projects(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode')),
    hook_id TEXT NOT NULL REFERENCES hooks(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(project_id, tool, hook_id)
);

CREATE INDEX idx_hook_project_assignments_tool_hook
    ON hook_project_assignments(tool, hook_id);

-- 全局继承与项目分配互斥（与 mcp/skill 同型，覆盖 INSERT 与 UPDATE）。
CREATE TRIGGER trg_hook_project_assignment_reject_global
BEFORE INSERT ON hook_project_assignments
WHEN EXISTS (
    SELECT 1 FROM hook_global_assignments
    WHERE tool = NEW.tool AND hook_id = NEW.hook_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_hook_global_assignment_reject_project_duplicate
BEFORE INSERT ON hook_global_assignments
WHEN EXISTS (
    SELECT 1 FROM hook_project_assignments
    WHERE tool = NEW.tool AND hook_id = NEW.hook_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TRIGGER trg_hook_project_assignment_update_reject_global
BEFORE UPDATE OF tool, hook_id ON hook_project_assignments
WHEN EXISTS (
    SELECT 1 FROM hook_global_assignments
    WHERE tool = NEW.tool AND hook_id = NEW.hook_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_hook_global_assignment_update_reject_project_duplicate
BEFORE UPDATE OF tool, hook_id ON hook_global_assignments
WHEN EXISTS (
    SELECT 1 FROM hook_project_assignments
    WHERE tool = NEW.tool AND hook_id = NEW.hook_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TRIGGER trg_hooks_row_version_guard
BEFORE UPDATE ON hooks WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_hooks_row_version_bump
AFTER UPDATE ON hooks WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE hooks
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
