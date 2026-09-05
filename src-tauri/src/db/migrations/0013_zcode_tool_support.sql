-- ZCode 全能力工具接入：Provider、Prompt、MCP、Skills 全部支持。
--
-- 与 0010（Cursor 只放宽 MCP/Skills）不同，ZCode 四类 artifact 都有官方文件合同，
-- 因此除 mcp/skill 分配表外，还放宽 managed_targets（不加 artifact 限制）、
-- provider_profiles、prompt_project_assignments、profile_import_previews 的 tool CHECK。
--
-- 这些表被外键、触发器或持久化预览引用，沿用 0008/0009/0010 已验证的
-- writable_schema 原地修订；每个替换都限定表名与旧锚点。

PRAGMA writable_schema = ON;

UPDATE sqlite_master
SET sql = replace(sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode''))')
WHERE type = 'table' AND name IN (
    'mcp_global_assignments',
    'skill_global_assignments',
    'mcp_project_assignments',
    'skill_project_assignments',
    'mcp_import_previews',
    'skill_import_previews'
) AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor''))') > 0;

-- managed_targets 放宽工具枚举；ZCode 四类 artifact 全部受支持，不追加 Cursor
-- 那样的 artifact 限制，但保留对 Cursor 的既有约束。
UPDATE sqlite_master
SET sql = replace(sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill''))),',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill''))),')
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill''))),') > 0;

-- provider_profiles / prompt_project_assignments / profile_import_previews：
-- ZCode Provider 与 Prompt 走正式合同。三张表的 CHECK 形状相同，但 0010 未放宽，
-- 当前锚点仍是 (''claude'', ''codex'')。
UPDATE sqlite_master
SET sql = replace(sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode''))')
WHERE type = 'table' AND name IN (
    'provider_profiles',
    'prompt_project_assignments',
    'profile_import_previews'
) AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex''))') > 0;

PRAGMA writable_schema = OFF;

-- 提示词档案新增 ZCode 生效位（每工具至多一份生效由部分唯一索引强制，与 0009 同型）。
ALTER TABLE prompt_profiles
    ADD COLUMN is_active_zcode INTEGER NOT NULL DEFAULT 0 CHECK(is_active_zcode IN (0, 1));

CREATE UNIQUE INDEX uq_prompt_profiles_one_active_zcode
    ON prompt_profiles(is_active_zcode) WHERE is_active_zcode = 1;
