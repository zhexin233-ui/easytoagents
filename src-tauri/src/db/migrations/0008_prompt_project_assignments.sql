-- 提示词项目级分配：为每个 (项目, 工具) 提供至多一份提示词档案分配，
-- 并放开 managed_targets 的项目作用域 CHECK 以允许 artifact_kind='prompt' 的项目基线。
--
-- managed_targets 被三张子表外键引用（managed_items CASCADE、sync_targets/snapshots RESTRICT），
-- 且迁移事务内 foreign_keys 不可关闭，因此不重建表；改用官方支持的 schema 文本原地修订：
-- 锚定替换 CHECK 中的 artifact_kind 枚举。对 sqlite_schema 的直接修订不会推进 schema cookie，
-- 执行迁移的连接在提交后仍持有陈旧 schema 缓存；run_migrations 在每次迁移提交后显式推进
-- schema_version 强制重解析（见 db/mod.rs）。CHECK 是否生效由迁移测试以金丝雀插入验证。

PRAGMA writable_schema = ON;
UPDATE sqlite_master
SET sql = replace(
    sql,
    'artifact_kind IN (''mcp'', ''skill''))',
    'artifact_kind IN (''mcp'', ''skill'', ''prompt''))'
)
WHERE type = 'table'
  AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''mcp'', ''skill''))') > 0;
PRAGMA writable_schema = OFF;

CREATE TABLE prompt_project_assignments (
    project_id TEXT NOT NULL REFERENCES projects(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    prompt_profile_id TEXT NOT NULL REFERENCES prompt_profiles(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(project_id, tool)
);

CREATE INDEX idx_prompt_project_assignments_tool_profile
    ON prompt_project_assignments(tool, prompt_profile_id);
