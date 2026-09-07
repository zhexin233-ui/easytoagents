-- OpenCode Provider/Prompt/MCP/Skills 支持。Hooks 保持四工具合同，不能借由
-- 数据库枚举放宽而绕过 command-only capability 边界。

PRAGMA writable_schema = ON;

-- MCP 与 Skills 的全局/项目分配及导入预览。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode''))'
)
WHERE type = 'table' AND name IN (
    'mcp_global_assignments',
    'skill_global_assignments',
    'mcp_project_assignments',
    'skill_project_assignments',
    'mcp_import_previews',
    'skill_import_previews'
) AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode''))') > 0;

-- managed_targets 允许 OpenCode 的 Provider/Prompt/MCP/Skills；Cursor 的
-- 既有 artifact 限制及 Hook 的四工具约束均保留。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt''))),',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt'')) AND (tool != ''opencode'' OR artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))),'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt''))),') > 0;

-- Provider 与 Prompt 导入预览。Cursor 仍仅允许 Prompt；OpenCode 两者均可。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),'
)
WHERE type = 'table' AND name = 'profile_import_previews'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),') > 0;

-- Provider 档案本身不开放 Cursor，但开放 OpenCode。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''opencode''))'
)
WHERE type = 'table' AND name = 'provider_profiles'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode''))') > 0;

PRAGMA writable_schema = OFF;

ALTER TABLE prompt_profiles
    ADD COLUMN is_active_opencode INTEGER NOT NULL DEFAULT 0 CHECK(is_active_opencode IN (0, 1));

CREATE UNIQUE INDEX uq_prompt_profiles_one_active_opencode
    ON prompt_profiles(is_active_opencode) WHERE is_active_opencode = 1;
