-- 移除项目级 Prompt 的持久化状态。
--
-- 该迁移只清理中央数据库中的历史项目 Prompt 状态。项目目录中的
-- CLAUDE.md、AGENTS.md 和 Cursor 规则文件从来不是迁移目标，不能从
-- managed_targets.target_path 删除。历史 Prompt 快照属于应用私有数据，
-- 先登记到可重试的退休队列，事务提交后再由 Database::open 安全清理。

CREATE TABLE retired_snapshot_cleanup (
    snapshot_id TEXT PRIMARY KEY CHECK(
        length(snapshot_id) = 36 AND snapshot_id = lower(snapshot_id)
        AND substr(snapshot_id, 9, 1) = '-' AND substr(snapshot_id, 14, 1) = '-'
        AND substr(snapshot_id, 19, 1) = '-' AND substr(snapshot_id, 24, 1) = '-'
        AND length(replace(snapshot_id, '-', '')) = 32
        AND replace(snapshot_id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    run_id TEXT NOT NULL CHECK(
        length(run_id) = 36 AND run_id = lower(run_id)
        AND substr(run_id, 9, 1) = '-' AND substr(run_id, 14, 1) = '-'
        AND substr(run_id, 19, 1) = '-' AND substr(run_id, 24, 1) = '-'
        AND length(replace(run_id, '-', '')) = 32
        AND replace(run_id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    snapshot_path TEXT NOT NULL CHECK(
        snapshot_path LIKE '/%' AND snapshot_path != '/' AND instr(snapshot_path, '//') = 0
        AND snapshot_path NOT LIKE '%/../%' AND snapshot_path NOT LIKE '%/./%'
        AND snapshot_path NOT LIKE '%/..' AND snapshot_path NOT LIKE '%/.'
        AND substr(snapshot_path, -1) != '/'
    ),
    -- 旧数据库可能保留 metadata_only/directory_tree 的历史记录；它们也
    -- 必须先进入退休队列，避免一条异常的存储分类阻断整库迁移。物理
    -- 清理阶段只会接受 payload_file，并对路径和文件类型再次校验。
    storage_kind TEXT NOT NULL CHECK(
        storage_kind IN ('payload_file', 'metadata_only', 'directory_tree')
    ),
    content_hash TEXT CHECK(
        content_hash IS NULL OR (
            length(content_hash) = 64 AND content_hash = lower(content_hash)
            AND content_hash NOT GLOB '*[^0-9a-f]*'
        )
    ),
    retired_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- 先物化身份，避免删除父行后失去清理范围。临时表随迁移连接结束，
-- 不会成为应用数据库的公开状态。
CREATE TEMP TABLE retired_project_prompt_targets (
    id TEXT PRIMARY KEY
);
INSERT INTO retired_project_prompt_targets(id)
    SELECT id
    FROM managed_targets
    WHERE scope = 'project' AND artifact_kind = 'prompt';

CREATE TEMP TABLE retired_prompt_runs (
    id TEXT PRIMARY KEY
);
INSERT INTO retired_prompt_runs(id)
    SELECT DISTINCT run_id
    FROM snapshots
    WHERE target_id IN (SELECT id FROM retired_project_prompt_targets);
INSERT OR IGNORE INTO retired_prompt_runs(id)
    SELECT DISTINCT run_id
    FROM sync_items
    WHERE target_id IN (SELECT id FROM retired_project_prompt_targets);

-- 退休队列必须在删除 snapshots 前记录完整的身份和存储信息。
INSERT INTO retired_snapshot_cleanup(snapshot_id, run_id, snapshot_path, storage_kind, content_hash)
    SELECT snapshot.id, snapshot.run_id, snapshot.snapshot_path,
           snapshot.storage_kind, snapshot.content_hash
    FROM snapshots AS snapshot
    WHERE snapshot.target_id IN (SELECT id FROM retired_project_prompt_targets)
       OR snapshot.id IN (
           SELECT native.disabled_snapshot_id
           FROM project_native_resources AS native
           WHERE native.target_id IN (SELECT id FROM retired_project_prompt_targets)
             AND native.disabled_snapshot_id IS NOT NULL
       );

-- 先解除 PromptFile 的 snapshots RESTRICT 引用，再删除对应历史快照和同步项。
DELETE FROM project_native_resources
WHERE target_id IN (SELECT id FROM retired_project_prompt_targets);

DELETE FROM snapshots
WHERE id IN (SELECT snapshot_id FROM retired_snapshot_cleanup);

DELETE FROM sync_items
WHERE target_id IN (SELECT id FROM retired_project_prompt_targets);

-- 混合 run 仍可能包含 MCP/Skill/Hook/Provider 的 item 或 snapshot，只有完全
-- 没有任何子记录的项目 Prompt run 才能被清理。
DELETE FROM sync_runs
WHERE id IN (SELECT id FROM retired_prompt_runs)
  AND NOT EXISTS (SELECT 1 FROM sync_items WHERE sync_items.run_id = sync_runs.id)
  AND NOT EXISTS (SELECT 1 FROM snapshots WHERE snapshots.run_id = sync_runs.id);

-- managed_items 由 ON DELETE CASCADE 清理；全局 Prompt target 不在临时身份表中，
-- 因而继续保留。
DELETE FROM managed_targets
WHERE id IN (SELECT id FROM retired_project_prompt_targets);

DROP TABLE prompt_project_assignments;

-- managed_targets 被多张表以 RESTRICT 外键引用，不能 DROP/重建。只收紧项目
-- artifact CHECK 的 sqlite_schema 文本，保持既有表和外键身份不变。
PRAGMA writable_schema = ON;
UPDATE sqlite_master
SET sql = replace(
    sql,
    'artifact_kind IN (''mcp'', ''skill'', ''prompt'', ''hook''))',
    'artifact_kind IN (''mcp'', ''skill'', ''hook''))'
)
WHERE type = 'table'
  AND name = 'managed_targets'
  AND instr(sql, 'artifact_kind IN (''mcp'', ''skill'', ''prompt'', ''hook''))') > 0;

UPDATE sqlite_master
SET sql = replace(
    sql,
    'entry_type IN (''mcp_entry'', ''directory'', ''symlink'', ''prompt_file'')',
    'entry_type IN (''mcp_entry'', ''directory'', ''symlink'')'
)
WHERE type = 'table'
  AND name = 'project_native_resources'
  AND instr(sql, 'entry_type IN (''mcp_entry'', ''directory'', ''symlink'', ''prompt_file'')') > 0;
PRAGMA writable_schema = OFF;
