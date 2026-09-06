-- Hook 事件随分配：事件从中央记录的固定属性改为分配属性，同一中央 Hook
-- 可在不同工具以不同事件生效（导入时来源事件保留为 hooks.event 建议值）。
--
-- 两张分配表无其他表引用，直接重建并回填（事件取自 hooks.event）。
-- 先显式删除 4 个旧触发器：DROP TABLE 会重解析 schema，彼时另一张表上的
-- 互斥触发器仍引用将删的表，会报 "no such table"（运行器实测）。
DROP TRIGGER IF EXISTS trg_hook_project_assignment_reject_global;
DROP TRIGGER IF EXISTS trg_hook_global_assignment_reject_project_duplicate;
DROP TRIGGER IF EXISTS trg_hook_project_assignment_update_reject_global;
DROP TRIGGER IF EXISTS trg_hook_global_assignment_update_reject_project_duplicate;

CREATE TABLE hook_global_assignments_new (
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode')),
    hook_id TEXT NOT NULL REFERENCES hooks(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    event TEXT NOT NULL CHECK(event IN (
        'SessionStart', 'SessionEnd', 'UserPromptSubmit',
        'PreToolUse', 'PermissionRequest', 'PostToolUse', 'PostToolUseFailure',
        'SubagentStart', 'SubagentStop', 'PreCompact', 'PostCompact',
        'Stop', 'Notification'
    )),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(tool, hook_id)
);

INSERT INTO hook_global_assignments_new(tool, hook_id, event, created_at)
    SELECT assignment.tool, assignment.hook_id, hooks.event, assignment.created_at
    FROM hook_global_assignments AS assignment
    JOIN hooks ON hooks.id = assignment.hook_id;

DROP TABLE hook_global_assignments;
ALTER TABLE hook_global_assignments_new RENAME TO hook_global_assignments;

CREATE TABLE hook_project_assignments_new (
    project_id TEXT NOT NULL REFERENCES projects(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode')),
    hook_id TEXT NOT NULL REFERENCES hooks(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    event TEXT NOT NULL CHECK(event IN (
        'SessionStart', 'SessionEnd', 'UserPromptSubmit',
        'PreToolUse', 'PermissionRequest', 'PostToolUse', 'PostToolUseFailure',
        'SubagentStart', 'SubagentStop', 'PreCompact', 'PostCompact',
        'Stop', 'Notification'
    )),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(project_id, tool, hook_id)
);

INSERT INTO hook_project_assignments_new(project_id, tool, hook_id, event, created_at)
    SELECT assignment.project_id, assignment.tool, assignment.hook_id, hooks.event, assignment.created_at
    FROM hook_project_assignments AS assignment
    JOIN hooks ON hooks.id = assignment.hook_id;

DROP TABLE hook_project_assignments;
ALTER TABLE hook_project_assignments_new RENAME TO hook_project_assignments;

CREATE INDEX idx_hook_project_assignments_tool_hook
    ON hook_project_assignments(tool, hook_id);

-- 全局继承与项目分配互斥（与原语义一致，覆盖 INSERT 与 UPDATE）。
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
