# 实施计划

- [x] 加载后端规范（database、error-handling、quality）。
- [x] A4：改写 `resolve_executable`，补三类 PATH 单测与诊断码文案；核对 `overview` 与前端诊断码映射。
- [x] A5：调整 `Database::open` 备份判定，增加 checkpoint 与裁剪，补单测。
- [x] A6：reconcile 事务化并移出 `project_dto`；核对所有调用点，补"读命令不写库"测试。
- [x] A9、A10、A11、A12：逐项修改并各补一条回归测试（A11/A12 为删代码，无可测行为，见下）。
- [x] 运行 `pnpm rust:check`、`pnpm check`、`pnpm bindings:check`。
- [x] 更新 `.trellis/spec/backend/database-guidelines.md`（备份策略）与 `quality-guidelines.md`（读命令不写库、探针 PATH 逐条目、快照删除顺序）。

## 实施记录

- A4：`ExecutableResolution` 改为 `Found { path, skipped_entries } / Unavailable { skipped_entries } / Unsupported { reason }`；诊断码常量 `INSTALLATION_PROBE_SKIPPED_PATH_ENTRIES / NO_SAFE_PATH_ENTRIES / UNSAFE_CANDIDATE` 通过 `ExplicitEnvironment::with_installation_probe_diagnostic` 进入 `ToolProfileStatusDto.installationProbeDiagnostic`（新字段，绑定已重新生成），工具配置页展示文案。测试：`unsafe_path_entries_are_skipped_instead_of_failing_the_whole_probe`、`unsafe_first_candidate_and_empty_safe_path_still_fail_closed`。
- A5：只在 `0 < applied < MIGRATIONS.len()` 时备份；先 `PRAGMA wal_checkpoint(TRUNCATE)`；`prune_startup_backups` 保留 3 份。测试：`startup_backup_only_runs_when_migrations_are_pending`、`startup_backup_checkpoints_active_wal_and_prunes_to_three_directories`；旧测试改为用 v4 库触发备份。
- A6：`db/native_resources.rs` 写函数改为 `*_in(connection, database_path, ...)`；`reconcile_project_native_resources` 先只读观测全部描述符，再在一个 IMMEDIATE 事务内写入。`project_dto` 改为纯读 `project_native_resource_summary`；`register_project`/`rescan_project` 显式对账。**与设计的偏差**：`preview_project_native_resource_action` 不做前置对账——对账会 bump 每条观测行的 `row_version`，导致调用方传入的版本立刻 `CONFLICT`；预览本身已重新扫描原生目标，Apply 再校验哈希。测试：`project_reads_do_not_write_native_resource_rows`、`reconcile_is_atomic_when_a_write_fails_mid_project`（用 TEMP TRIGGER 注入中途失败）。
- A9：三处 `unwrap_or_default()` 路径键统一为 `inputs_by_target_path`（无路径 → `STALE_PREVIEW unsupportedTarget`，重复路径 → `INVALID_INPUT`）。测试：`apply_rejects_inputs_without_a_target_path_instead_of_collapsing_them`。
- A10：`delete_snapshots` 改为一个事务内写入 `retired_snapshot_cleanup` + 删行并提交，再逐个删文件；删成功出队，删失败留队由 `Database::open` 重试；不再回报 `ATOMIC_WRITE_FAILED`。测试：`delete_snapshots_retires_rows_first_and_queues_undeletable_files`、`delete_snapshots_leaves_no_cleanup_queue_entry_after_a_successful_removal`。
- A11：删除 `AppState.interrupted_run` 及两处只写不读的写入点；`get_interrupted_run`/`restore_snapshot` 每次直接查 `sync_runs`。理由写在 `app/mod.rs` 注释。
- A12：删除迁移后的第二次 `configure_connection`（PRAGMA 是连接级状态）。

## 风险文件
`app/tool_probe.rs`、`db/mod.rs`、`projects/native_resources.rs`、`projects/service.rs`、`sync/apply.rs`。

## 回滚点
每个编号独立提交，可单独 revert。
