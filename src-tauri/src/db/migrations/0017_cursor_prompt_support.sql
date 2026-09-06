-- Cursor 提示词能力开放：全局 `~/.cursor/rules` 与项目 `.cursor/rules/*.mdc`
-- 官方文件合同（2026-09-06 核验，cursor.com/docs/rules 与
-- cursor.com/help/customization/rules）。
--
-- 沿用 0013/0014 已验证的 writable_schema 原地文本修订；每个替换限定表名与
-- 旧锚点（instr > 0 防空改），CHECK 生效性由迁移测试金丝雀验证。
--
-- 注意：provider_profiles 的 tool CHECK 与下面第二组锚点文本相同
-- （0013 同时改写了三张表），因此本迁移的表名清单刻意不含 provider_profiles，
-- Cursor Provider 保持全程拒绝。

PRAGMA writable_schema = ON;

-- managed_targets：Cursor 的 artifact 限制加入 'prompt'
--（当前文本为 0014 改写后的锚点）。
UPDATE sqlite_master
SET sql = replace(sql,
    '(tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook''))',
    '(tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook'', ''prompt''))')
WHERE type = 'table' AND name = 'managed_targets'
  AND instr(sql, '(tool != ''cursor'' OR artifact_kind IN (''mcp'', ''skill'', ''hook''))') > 0;

-- prompt_project_assignments / profile_import_previews：Cursor Prompt 走正式
-- 合同；两表 CHECK 形状相同，精确限定表名避免误伤 provider_profiles。
-- profile_import_previews 额外带 tool×artifact 组合约束：cursor 仅允许 prompt
-- 导入预览，Provider 预览在 DB 层继续保持拒绝（第二道边界）。
UPDATE sqlite_master
SET sql = replace(sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor''))')
WHERE type = 'table' AND name = 'prompt_project_assignments'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode''))') > 0;

UPDATE sqlite_master
SET sql = replace(sql,
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'')),
    artifact_kind TEXT NOT NULL CHECK(artifact_kind IN (''provider'', ''prompt''))',
    'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),
    artifact_kind TEXT NOT NULL CHECK(artifact_kind IN (''provider'', ''prompt''))')
WHERE type = 'table' AND name = 'profile_import_previews'
  AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'')),
    artifact_kind TEXT NOT NULL CHECK(artifact_kind IN (''provider'', ''prompt''))') > 0;

PRAGMA writable_schema = OFF;

-- 提示词档案新增 Cursor 生效位（每工具至多一份生效由部分唯一索引强制，与 0009/0013 同型）。
ALTER TABLE prompt_profiles
    ADD COLUMN is_active_cursor INTEGER NOT NULL DEFAULT 0 CHECK(is_active_cursor IN (0, 1));

CREATE UNIQUE INDEX uq_prompt_profiles_one_active_cursor
    ON prompt_profiles(is_active_cursor) WHERE is_active_cursor = 1;
