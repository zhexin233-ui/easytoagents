# 后端性能优化

父任务：`09-10-codebase-optimization`。覆盖审阅条目 C1、C2、C3、C4、C5、C6。

## 目标
在不改变同步安全语义（原子替换、崩溃恢复、快照）的前提下，把 apply 的 fsync 与读取次数、启动与列表接口的 IO/SQL 次数降到与数据规模线性且系数小的水平。

## 需求
- R1 / C1：journal 持久化改为 append-only，每个阶段追加一行 JSON 并单次 fsync；恢复逻辑读取最后一条有效记录；单个 WriteFile 目标的 fsync 总次数从约 21 次降到不超过 8 次。崩溃恢复与回滚判定结果与改动前完全一致。
- R2 / C2：`capture_path_state` 的结果沿 apply 调用链传递；一次 mutation 对目标文件的全量读取不超过 2 次（rename 前用 `symlink_metadata` 的 inode/size/mtime 做廉价复核，rename 后一次全读校验）。`revalidate_database_preflight` 在 mutation 循环前执行一次，循环内改用 `PRAGMA data_version` 或等价机制检测并发写入。
- R3 / C3：启动只做一次 `paths.initialize()` 与一次全树审计；apply/restore 只审计 `journals/` 与当前 run 的 `snapshots/<run_id>/`。
- R4 / C4：`list_skills`、`list_mcp_servers`、`list_hooks` 的全局分配用一条聚合查询；`collect_row_versions` 用 `WHERE id IN` 或已加载记录映射；`list_skills` 在列表场景只做 `symlink_metadata` 级检查，全树摘要移到 `get_skill` 或以 `(mtime, size)` 缓存；db 层 `.prepare` 改为 `prepare_cached`。
- R5 / C5：删除 `canonical_json`，`hash_json` 直接序列化；增加一条测试断言 `serde_json` 输出键有序，防止未来打开 `preserve_order`。
- R6 / C6：GitHub 下载并发（`buffer_unordered`，并发度 6）；文件写入放进 `spawn_blocking` 或先聚合到内存再一次性落盘；`Client` 用 `OnceLock` 复用；总超时与限额不变。

## 验收条件
- A1：`sync/apply` 现有测试（含故障注入）全部通过；新增一条测试统计 fault injector 观测到的 `sync_all` 次数，验证 R1 上限。
- A2：新增测试统计单个 WriteFile 的 `fs::read` 次数不超过 2。
- A3：审计函数调用次数在启动与一次 apply 中各有测试断言。
- A4：列表接口测试断言 SQL 语句数量为常数（可用 `rusqlite` trace 钩子或计数包装）。
- A5：`canonical_json` 不存在；序列化有序测试存在。
- A6：GitHub 下载测试覆盖并发与部分失败回滚；`pnpm rust:check`、`pnpm check` 全绿。

## 范围外
错误上下文与日志（`09-10-backend-robustness`）、文件拆分（`09-10-backend-dedup-and-split`）。
