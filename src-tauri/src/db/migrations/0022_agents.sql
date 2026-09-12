-- Agents（子代理）全局级与项目级管理：
-- 1. 新建 agents / agent_global_assignments / agent_project_assignments 三张表
--    与四条全局-项目互斥触发器（风格随 0014）；
-- 2. managed_targets 的 artifact CHECK 放宽加入 'agent'（writable_schema 原地
--    修订，沿用 0008/0013/0014/0019 先例——managed_targets 被 managed_items、
--    sync_targets、snapshots 以 RESTRICT 外键引用，不能在迁移事务内重建），
--    并追加 ZCode 作用域约束：ZCode 仅允许全局 agent 目标（官方明示不支持
--    项目级子代理，形成服务层之外的第二道边界）。
--
-- 五处锚点均为 0021 之后的当前文本（2026-09-12 在 v21 开发库逐字核对，每个
-- 锚点恰好命中一次）。每个 UPDATE 前用 TEMP 表 CHECK 前置校验锚点计数，
-- 缺失或多命中即中止迁移（0021 先例），不静默推进版本。

-- 步骤 1：旧锚点必须各精确存在一次，否则中止迁移。
CREATE TEMP TABLE migration_0022_old_anchors (
    anchor TEXT PRIMARY KEY,
    matched INTEGER NOT NULL CHECK(matched = 1)
);
INSERT INTO migration_0022_old_anchors(anchor, matched)
    SELECT 'artifact_kind_full', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''hook''))') > 0
    UNION ALL
    SELECT 'artifact_kind_project', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''hook''))') > 0
    UNION ALL
    SELECT 'artifact_kind_cursor', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt''))') > 0
    UNION ALL
    SELECT 'artifact_kind_opencode', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))') > 0
    UNION ALL
    SELECT 'tool_check_ending', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'')))' ) > 0;

-- 步骤 2：writable_schema 原地修订。先改 tool CHECK 末尾（追加 ZCode 作用域
-- 约束），再放宽各 artifact CHECK——否则 opencode 约束改写会破坏 tool_check
-- 末尾锚点（两者前缀重叠）。
PRAGMA writable_schema = ON;

-- managed_targets：tool CHECK 末尾追加 ZCode 作用域约束
--（agent 仅允许 global；锚点为 0019 之后的 tool CHECK 结尾）。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'')))',
    'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'')) AND (tool != ''zcode'' OR artifact_kind != ''agent'' OR scope = ''global''))'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'')))' ) > 0;

-- managed_targets：受管资源种类加入 'agent'（0014 改写后的锚点）。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''hook''))',
    'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''hook'', ''agent''))'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''hook''))') > 0;

-- managed_targets：项目作用域资源种类加入 'agent'（0018 收紧后的锚点）。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'artifact_kind IN (''mcp'', ''skill'', ''hook''))',
    'artifact_kind IN (''mcp'', ''skill'', ''hook'', ''agent''))'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''hook''))') > 0;

-- managed_targets：Cursor 的 artifact 限制加入 'agent'
--（0014 改写后的锚点；Cursor 官方支持项目级子代理）。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt''))',
    'artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt'', ''agent''))'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt''))') > 0;

-- managed_targets：OpenCode 的 artifact 限制加入 'agent'
--（0019 改写后的锚点；OpenCode 官方支持项目级子代理）。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))',
    'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''agent''))'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))') > 0;

PRAGMA writable_schema = OFF;

-- 步骤 3：修订后文本后置校验（sqlite_master 内容在事务内可见），任何未命中
-- 即中止迁移，防止部分改写后带病推进版本。
CREATE TEMP TABLE migration_0022_new_anchors (
    anchor TEXT PRIMARY KEY,
    matched INTEGER NOT NULL CHECK(matched = 1)
);
INSERT INTO migration_0022_new_anchors(anchor, matched)
    SELECT 'artifact_kind_full', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'CHECK(artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''hook'', ''agent''))') > 0
    UNION ALL
    SELECT 'artifact_kind_project', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''hook'', ''agent''))') > 0
    UNION ALL
    SELECT 'artifact_kind_cursor', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt'', ''agent''))') > 0
    UNION ALL
    SELECT 'artifact_kind_opencode', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, 'artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'', ''agent''))') > 0
    UNION ALL
    SELECT 'zcode_scope', COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
      AND instr(sql, '(tool != ''zcode'' OR artifact_kind != ''agent'' OR scope = ''global'')') > 0;

-- 步骤 4：三张新表（风格随 0014：UUID CHECK、row_version guard/bump 触发器）。
CREATE TABLE agents (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    -- 五工具交集规则 ^[a-z0-9][a-z0-9-]{0,63}$：小写字母/数字开头，
    -- 仅小写字母、数字与连字符，长度 1..=64（名称即文件名，跨工具通用）。
    name TEXT NOT NULL COLLATE NOCASE UNIQUE
        CHECK(
            length(name) BETWEEN 1 AND 64
            AND name NOT GLOB '*[^a-z0-9-]*'
            AND substr(name, 1, 1) NOT GLOB '[^a-z0-9]'
        ),
    description TEXT NOT NULL CHECK(
        length(description) BETWEEN 1 AND 1000
        AND instr(description, char(0)) = 0
    ),
    prompt TEXT NOT NULL CHECK(
        length(prompt) BETWEEN 1 AND 65536
        AND instr(prompt, char(0)) = 0
    ),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1)
);

CREATE TABLE agent_global_assignments (
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode', 'opencode')),
    agent_id TEXT NOT NULL REFERENCES agents(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(tool, agent_id)
);

-- ZCode 在表级即被拒绝（项目级子代理官方明示不支持）。
CREATE TABLE agent_project_assignments (
    project_id TEXT NOT NULL REFERENCES projects(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'opencode')),
    agent_id TEXT NOT NULL REFERENCES agents(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(project_id, tool, agent_id)
);

CREATE INDEX idx_agent_project_assignments_tool_agent
    ON agent_project_assignments(tool, agent_id);

-- 全局继承与项目分配互斥（与 mcp/skill/hook 同型，覆盖 INSERT 与 UPDATE）。
CREATE TRIGGER trg_agent_project_assignment_reject_global
BEFORE INSERT ON agent_project_assignments
WHEN EXISTS (
    SELECT 1 FROM agent_global_assignments
    WHERE tool = NEW.tool AND agent_id = NEW.agent_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_agent_global_assignment_reject_project_duplicate
BEFORE INSERT ON agent_global_assignments
WHEN EXISTS (
    SELECT 1 FROM agent_project_assignments
    WHERE tool = NEW.tool AND agent_id = NEW.agent_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TRIGGER trg_agent_project_assignment_update_reject_global
BEFORE UPDATE OF tool, agent_id ON agent_project_assignments
WHEN EXISTS (
    SELECT 1 FROM agent_global_assignments
    WHERE tool = NEW.tool AND agent_id = NEW.agent_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_agent_global_assignment_update_reject_project_duplicate
BEFORE UPDATE OF tool, agent_id ON agent_global_assignments
WHEN EXISTS (
    SELECT 1 FROM agent_project_assignments
    WHERE tool = NEW.tool AND agent_id = NEW.agent_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TRIGGER trg_agents_row_version_guard
BEFORE UPDATE ON agents WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_agents_row_version_bump
AFTER UPDATE ON agents WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE agents
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

-- 步骤 5：清理临时校验表。
DROP TABLE migration_0022_old_anchors;
DROP TABLE migration_0022_new_anchors;
