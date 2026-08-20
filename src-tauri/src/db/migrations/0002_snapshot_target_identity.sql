-- 恢复必须能把子链接和 Git exclude 的快照追溯到原受管目标。
ALTER TABLE snapshots
ADD COLUMN target_id TEXT REFERENCES managed_targets(id) ON UPDATE CASCADE ON DELETE RESTRICT;

CREATE INDEX idx_snapshots_managed_target ON snapshots(target_id, created_at DESC);

-- UUID 与毫秒时间不能表达同一 Preview 内的目标顺序；故障恢复必须保留计划 ordinal。
ALTER TABLE sync_items ADD COLUMN target_order INTEGER NOT NULL DEFAULT 0 CHECK(target_order >= 0);
CREATE INDEX idx_sync_items_run_order ON sync_items(run_id, target_order, id);
