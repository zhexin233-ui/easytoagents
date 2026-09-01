-- 区分快照的持久化载体。历史普通文件拥有 payload，历史目录/链接/missing
-- 只有元数据；directory_tree 由 Skills 显式接管创建并保存完整目录树。
ALTER TABLE snapshots
ADD COLUMN storage_kind TEXT NOT NULL DEFAULT 'metadata_only'
CHECK(storage_kind IN ('payload_file', 'metadata_only', 'directory_tree'));

UPDATE snapshots
SET storage_kind = CASE
    WHEN target_type = 'file' THEN 'payload_file'
    ELSE 'metadata_only'
END;
