# 技术设计

## A4 探测器
`resolve_executable(search_path, name)` 改为逐条目评估：不安全条目或 `join(name)` 为非文件时 `continue` 并计数；命中第一个普通文件后再校验其路径安全性，失败即 `Unsupported { reason }`。`ExecutableResolution` 增加 `skipped_entries: usize` 与 `reason: Option<&'static str>`，由 `ToolProbeOutcome` 带到 `ToolAvailabilityState` 的诊断字段（复用现有诊断码机制，新增 `*_INSTALLATION_PROBE_SKIPPED_PATH_ENTRIES` 一类的稳定码，文案为 `&'static str`）。前端总览页已渲染诊断码，仅需补充文案映射。

## A5 备份
`Database::open` 顺序改为：`prepare_database_file` → 打开连接 → 读取 `applied_migrations` → 若 `applied < MIGRATIONS.len()` 则 `PRAGMA wal_checkpoint(TRUNCATE)` 后调用 `backup_database_before_migrations` → 迁移。备份目录裁剪：按文件名时间戳排序，保留最新 3 个 `startup-*` 目录，其余 `remove_dir_all`；裁剪失败只记录不阻断启动。

## A6 reconcile 事务
`reconcile_project_native_resources(database, project, observed)` 内部用 `transaction_with_behavior(Immediate)` 包住全部 `reconcile_descriptor`；从 `project_dto` 删除调用，`register_project`、`rescan_project` 在观察完成后显式调用；`list_project_native_resources` 与 `preview_project_native_resource_action` 前置调用一次以保证视图新鲜。文档化"读命令不写库"。

## A9 / A10 / A11 / A12
- A9：`apply_claimed_preview` 的输入索引键改为 `(ArtifactKind, Scope, Option<PathBuf>, tool)` 元组或直接按 preview target 顺序索引。
- A10：`delete_snapshots` 事务内 `DELETE` → commit → 逐个删文件；删失败的路径写入 `retired_snapshot_cleanup`（已有机制）。
- A11：优先删除 `AppState.interrupted_run` 及其 setter；若 `get_interrupted_run` 需要缓存则改为读缓存优先。
- A12：删除第二次 `configure_connection`，若迁移后需重设 WAL 则单独调用最小 PRAGMA。

## 兼容与回滚
所有改动不涉及表结构与原生文件格式；每项可独立 revert。
