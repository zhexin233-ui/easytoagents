-- Provider 导入预览（多候选）：与 mcp_import_previews / skill_import_previews 同型。
--
-- 一个原生 Provider 目标文件（Pi 的 models.json）可以承载多个 provider 条目，旧的
-- profile_import_previews 是「一次检测一份候选」的单预览语义，无法承载「哪些候选」
-- 这类原生身份证据；沿用该表只能从展示字段反推原生 key，违反 MCP 导入规范。
--
--   * context_json：只放身份证据（候选 UUID + 原生 provider id + 名称），绝不放投影或凭据。
--   * redacted_preview_json：只放前端展示用的脱敏 DTO。
--   * 确认阶段重新扫描原生文件并用 provider id 与 context_json 求交，缺失即 stale。
--
-- Cursor 没有 Provider 合同，因此不进 tool 白名单。

CREATE TABLE provider_import_previews (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'zcode', 'opencode', 'pi')),
    target_path TEXT NOT NULL CHECK(
        target_path LIKE '/%' AND target_path != '/' AND instr(target_path, '//') = 0
        AND target_path NOT LIKE '%/../%' AND target_path NOT LIKE '%/./%'
        AND target_path NOT LIKE '%/..' AND target_path NOT LIKE '%/.'
        AND substr(target_path, -1) != '/'
    ),
    observed_full_hash TEXT NOT NULL CHECK(
        length(observed_full_hash) = 64 AND observed_full_hash = lower(observed_full_hash)
        AND observed_full_hash NOT GLOB '*[^0-9a-f]*'
    ),
    context_json TEXT NOT NULL CHECK(json_valid(context_json) AND json_type(context_json) = 'object'),
    redacted_preview_json TEXT NOT NULL CHECK(
        json_valid(redacted_preview_json) AND json_type(redacted_preview_json) = 'object'
    ),
    status TEXT NOT NULL DEFAULT 'previewed' CHECK(status IN ('previewed', 'consumed')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    consumed_at TEXT
);

CREATE INDEX idx_provider_import_previews_status
    ON provider_import_previews(status, created_at);

-- 后置校验：表结构、索引与 tool 白名单必须一次到位。
CREATE TEMP TABLE migration_0026_postcheck (
    ok INTEGER NOT NULL CHECK(ok = 1)
);
INSERT INTO migration_0026_postcheck(ok)
    SELECT CASE
        WHEN (SELECT COUNT(*) FROM pragma_table_info('provider_import_previews')) = 9
         AND (SELECT COUNT(*) FROM sqlite_master
              WHERE type = 'index' AND name = 'idx_provider_import_previews_status') = 1
         AND instr((SELECT sql FROM sqlite_master
                    WHERE type = 'table' AND name = 'provider_import_previews'), '''pi''') > 0
        THEN 1 ELSE 0
    END;
DROP TABLE migration_0026_postcheck;
