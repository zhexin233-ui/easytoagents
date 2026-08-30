-- 提示词档案工具无关化：档案不再绑定单一工具，改为 per-tool 启用位（每工具至多一份生效）。
--
-- 迁移事务内 foreign_keys=ON 且 prompt_project_assignments 以 RESTRICT 外键引用
-- prompt_profiles，父表不可重建；因此沿用 0008 先例：仅使用 ADD COLUMN / DROP INDEX /
-- writable_schema 的 CHECK 文本原地修订，不改动任何行的存储布局。
--
-- 旧 `tool` + `is_active` 列保留为遗留列（tool 放宽 CHECK 后新档案统一写 'central'，
-- is_active 全部清零停用），新语义由 is_active_claude / is_active_codex 承载，
-- 每工具至多一份生效由各自的部分唯一索引强制（与原 uq_prompt_profiles_one_active_per_tool 同型）。

ALTER TABLE prompt_profiles
    ADD COLUMN is_active_claude INTEGER NOT NULL DEFAULT 0 CHECK(is_active_claude IN (0, 1));
ALTER TABLE prompt_profiles
    ADD COLUMN is_active_codex INTEGER NOT NULL DEFAULT 0 CHECK(is_active_codex IN (0, 1));

-- 按旧模型把各工具当前生效档案种子到新启用位。
UPDATE prompt_profiles
SET is_active_claude = CASE WHEN tool = 'claude' AND is_active = 1 THEN 1 ELSE 0 END,
    is_active_codex = CASE WHEN tool = 'codex' AND is_active = 1 THEN 1 ELSE 0 END;

-- 遗留列清零：新语义下 is_active 不再被读取，避免残留歧义数据。
UPDATE prompt_profiles SET is_active = 0;

DROP INDEX uq_prompt_profiles_one_active_per_tool;

CREATE UNIQUE INDEX uq_prompt_profiles_one_active_claude
    ON prompt_profiles(is_active_claude) WHERE is_active_claude = 1;
CREATE UNIQUE INDEX uq_prompt_profiles_one_active_codex
    ON prompt_profiles(is_active_codex) WHERE is_active_codex = 1;

-- 新档案不再绑定工具：统一以 'central' 占位遗留来源列（UNIQUE(tool, name) 由此
-- 退化为新档案名全局唯一）。CHECK 修订沿用 0008 的原地文本替换；必须限定
-- name = 'prompt_profiles'，provider_profiles 等表存在同形 CHECK 不可误伤。
-- 锚点不含右括号：prompt_profiles 的 tool CHECK 后随逗号而非右括号。
PRAGMA writable_schema = ON;
UPDATE sqlite_master
SET sql = replace(
    sql,
    'tool IN (''claude'', ''codex'')',
    'tool IN (''claude'', ''codex'', ''central'')'
)
WHERE type = 'table'
  AND name = 'prompt_profiles'
  AND instr(sql, 'tool IN (''claude'', ''codex'')') > 0;
PRAGMA writable_schema = OFF;
