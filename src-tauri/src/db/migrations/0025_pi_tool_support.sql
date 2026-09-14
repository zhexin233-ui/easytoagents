-- Pi Provider/Prompt/MCP/Skills 支持（Hooks / Agents 保持关闭）。
--
-- 放宽范围严格按 design §6：只放行 Pi 真正会保存的 artifact。
--   * MCP 与 Skills 的全局/项目分配及导入预览：tool CHECK 增加 'pi'。
--   * managed_targets：tool CHECK 增加 'pi'，并用
--     `tool != 'pi' OR artifact_kind IN ('provider', 'prompt', 'mcp', 'skill')`
--     显式限定 Pi 的 artifact 集合（hook/agent 继续被 CHECK 拒绝）。
--   * provider_profiles / profile_import_previews：tool CHECK 增加 'pi'。
--   * hook_global_assignments / hook_project_assignments / hook_assignment_events /
--     agents / agent_tool_settings：不触碰（Pi 无 Hooks / Agents 合同）。
--
-- 锚点取自 v24 实时 schema 文本（0022 已在 managed_targets 的 tool CHECK 末尾
-- 追加 ZCode 作用域约束并把 Cursor/OpenCode 白名单扩到含 'agent'，因此不能
-- 沿用 0019 的旧锚点）。改写前先校验旧锚点各命中一次，改写后再校验新锚点，
-- 任一未命中即中止迁移，不推进 schema 版本。

-- 前置校验 1：六张 MCP/Skills 表必须存在。
CREATE TEMP TABLE migration_0025_shared_tables (
    found INTEGER NOT NULL CHECK(found = 6)
);
INSERT INTO migration_0025_shared_tables(found)
    SELECT COUNT(*)
    FROM sqlite_master
    WHERE type = 'table'
      AND name IN (
          'mcp_global_assignments',
          'skill_global_assignments',
          'mcp_project_assignments',
          'skill_project_assignments',
          'mcp_import_previews',
          'skill_import_previews'
      );

-- 前置校验 2：逐表校验旧锚点恰好命中一次。
CREATE TEMP TABLE migration_0025_precheck (
    anchor TEXT PRIMARY KEY,
    matched INTEGER NOT NULL CHECK(matched = 1)
);
INSERT INTO migration_0025_precheck(anchor, matched)
    SELECT 'shared_tool_check:' || name,
           CASE
               WHEN (length(sql) - length(replace(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'')),', ''))) = length('tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'')),')
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table'
      AND name IN (
          'mcp_global_assignments',
          'skill_global_assignments',
          'mcp_project_assignments',
          'skill_project_assignments',
          'mcp_import_previews',
          'skill_import_previews'
      );

INSERT INTO migration_0025_precheck(anchor, matched)
    SELECT 'managed_targets_tool_check',
           CASE
               WHEN instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt'', ''agent''))') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets';

INSERT INTO migration_0025_precheck(anchor, matched)
    SELECT 'managed_targets_zcode_tail',
           CASE
               WHEN instr(sql, 'AND (tool != ''zcode'' OR artifact_kind != ''agent'' OR scope = ''global'')),') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets';

INSERT INTO migration_0025_precheck(anchor, matched)
    SELECT 'provider_profiles_tool_check',
           CASE
               WHEN instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''opencode''))') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'provider_profiles';

INSERT INTO migration_0025_precheck(anchor, matched)
    SELECT 'profile_import_previews_tool_check',
           CASE
               WHEN instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'profile_import_previews';

-- 前置校验 3：Hooks / Agents 表不得已被放宽（防止上游误改后本迁移静默通过）。
CREATE TEMP TABLE migration_0025_closed_tables (
    leaked INTEGER NOT NULL CHECK(leaked = 0)
);
INSERT INTO migration_0025_closed_tables(leaked)
    SELECT COUNT(*)
    FROM sqlite_master
    WHERE type = 'table'
      AND name IN (
          'hook_global_assignments',
          'hook_project_assignments',
          'hook_assignment_events',
          'agents',
          'agent_tool_settings'
      )
      AND instr(sql, '''pi''') > 0;

-- 原地修订 tool CHECK。writable_schema 只在本段开启，随后立刻关闭。
PRAGMA writable_schema = ON;

-- 六张 MCP/Skills 表：tool 枚举增加 'pi'。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'')),',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'', ''pi'')),'
)
WHERE type = 'table'
  AND name IN (
      'mcp_global_assignments',
      'skill_global_assignments',
      'mcp_project_assignments',
      'skill_project_assignments',
      'mcp_import_previews',
      'skill_import_previews'
  )
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'')),') > 0;

-- managed_targets：tool 枚举增加 'pi'（Cursor 白名单前缀保持原样）。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt'', ''agent''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'', ''pi'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt'', ''agent''))'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt'', ''agent''))') > 0;

-- managed_targets：追加 Pi 的 artifact 限制（hook/agent 对 pi 仍然被拒）。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'AND (tool != ''zcode'' OR artifact_kind != ''agent'' OR scope = ''global'')),',
    'AND (tool != ''zcode'' OR artifact_kind != ''agent'' OR scope = ''global'') AND (tool != ''pi'' OR artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill''))),'
)
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, 'AND (tool != ''zcode'' OR artifact_kind != ''agent'' OR scope = ''global'')),') > 0;

-- provider_profiles：Provider 支持 Pi。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''opencode''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''opencode'', ''pi''))'
)
WHERE type = 'table' AND name = 'provider_profiles'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''opencode''))') > 0;

-- profile_import_previews：Prompt 导入预览支持 Pi；Cursor 仍仅允许 prompt。
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'', ''opencode'', ''pi'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),'
)
WHERE type = 'table' AND name = 'profile_import_previews'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'', ''opencode'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),') > 0;

PRAGMA writable_schema = OFF;

-- prompt_profiles：每工具至多一份生效提示词（与 0009/0013/0017/0019 同型）。
ALTER TABLE prompt_profiles
    ADD COLUMN is_active_pi INTEGER NOT NULL DEFAULT 0 CHECK(is_active_pi IN (0, 1));

CREATE UNIQUE INDEX uq_prompt_profiles_one_active_pi
    ON prompt_profiles(is_active_pi) WHERE is_active_pi = 1;

-- 后置校验：新锚点必须全部就位，且旧锚点必须全部消失。
CREATE TEMP TABLE migration_0025_postcheck (
    anchor TEXT PRIMARY KEY,
    matched INTEGER NOT NULL CHECK(matched = 1)
);

INSERT INTO migration_0025_postcheck(anchor, matched)
    SELECT 'shared_tool_check:' || name,
           CASE
               WHEN (length(sql) - length(replace(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'', ''pi'')),', ''))) = length('tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'', ''pi'')),')
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table'
      AND name IN (
          'mcp_global_assignments',
          'skill_global_assignments',
          'mcp_project_assignments',
          'skill_project_assignments',
          'mcp_import_previews',
          'skill_import_previews'
      );

INSERT INTO migration_0025_postcheck(anchor, matched)
    SELECT 'managed_targets_new_enum',
           CASE
               WHEN instr(sql, 'CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'', ''pi'') AND (tool != ''cursor''') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets';

INSERT INTO migration_0025_postcheck(anchor, matched)
    SELECT 'managed_targets_pi_artifact_limit',
           CASE
               WHEN instr(sql, 'AND (tool != ''pi'' OR artifact_kind IN (''provider'', ''prompt'', ''mcp'', ''skill'')))') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets';

INSERT INTO migration_0025_postcheck(anchor, matched)
    SELECT 'provider_profiles_new_enum',
           CASE
               WHEN instr(sql, 'CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''opencode'', ''pi''))') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'provider_profiles';

INSERT INTO migration_0025_postcheck(anchor, matched)
    SELECT 'profile_import_previews_new_enum',
           CASE
               WHEN instr(sql, 'CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'', ''opencode'', ''pi'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),') > 0
               THEN 1 ELSE 0
           END
    FROM sqlite_master
    WHERE type = 'table' AND name = 'profile_import_previews';

INSERT INTO migration_0025_postcheck(anchor, matched)
    SELECT 'prompt_profiles_is_active_pi',
           CASE
               WHEN (SELECT COUNT(*) FROM pragma_table_info('prompt_profiles') WHERE name = 'is_active_pi') = 1
               THEN 1 ELSE 0
           END;

INSERT INTO migration_0025_postcheck(anchor, matched)
    SELECT 'prompt_profiles_active_pi_index',
           CASE
               WHEN (SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'uq_prompt_profiles_one_active_pi') = 1
               THEN 1 ELSE 0
           END;

-- 旧锚点不得残留（只扫本迁移放行的 6 张表；agent_global_assignments 等
-- Agents 分配表按 design §6 保持原样，不属于陈旧锚点）。
CREATE TEMP TABLE migration_0025_stale_anchors (
    stale INTEGER NOT NULL CHECK(stale = 0)
);
INSERT INTO migration_0025_stale_anchors(stale)
    SELECT COUNT(*)
    FROM sqlite_master
    WHERE type = 'table'
      AND name IN (
          'mcp_global_assignments',
          'skill_global_assignments',
          'mcp_project_assignments',
          'skill_project_assignments',
          'mcp_import_previews',
          'skill_import_previews'
      )
      AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''cursor'', ''zcode'', ''opencode'')),') > 0;

INSERT INTO migration_0025_stale_anchors(stale)
    SELECT COUNT(*)
    FROM sqlite_master
    WHERE type = 'table' AND name = 'managed_targets'
      AND instr(sql, 'AND (tool != ''zcode'' OR artifact_kind != ''agent'' OR scope = ''global'')),') > 0;

-- 外键与完整性校验。
CREATE TEMP TABLE migration_0025_integrity (
    result TEXT NOT NULL CHECK(result = 'ok')
);
INSERT INTO migration_0025_integrity(result)
    SELECT * FROM pragma_integrity_check;

DROP TABLE migration_0025_shared_tables;
DROP TABLE migration_0025_precheck;
DROP TABLE migration_0025_closed_tables;
DROP TABLE migration_0025_postcheck;
DROP TABLE migration_0025_stale_anchors;
DROP TABLE migration_0025_integrity;
