-- Cursor MVP 只允许 MCP 与 Skills 进入持久化同步合同。Provider、Prompt、
-- profile_import_previews 与 prompt_project_assignments 的 CHECK 保持不变。
--
-- 这些表被外键、触发器或持久化预览引用，沿用 0008/0009 已验证的
-- writable_schema 原地修订；每个替换都限定表名与旧锚点。

PRAGMA writable_schema = ON;

UPDATE sqlite_master
SET sql = replace(sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor''))')
WHERE type = 'table' AND name IN (
    'mcp_global_assignments',
    'skill_global_assignments',
    'mcp_project_assignments',
    'skill_project_assignments',
    'mcp_import_previews',
    'skill_import_previews'
) AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex''))') > 0;

-- managed_targets 同时存放 Provider/Prompt。除放宽工具枚举外，还把 Cursor
-- 约束在 mcp/skill，避免 unsupported 资源通过底层 SQL 绕过服务层。
UPDATE sqlite_master
SET sql = replace(sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'')),',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill''))),')
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'')),') > 0;

PRAGMA writable_schema = OFF;
