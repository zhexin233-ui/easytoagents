# 实施计划

- [x] 加载后端规范；通读 `sync/apply.rs` 的 journal、snapshot、rollback 段与全部故障注入测试。
- [x] C5：删除 `canonical_json`，`hash_json` 直接序列化；补有序测试 `serde_json_serializes_object_keys_in_sorted_order`。
- [x] C3：`AppPaths::initialize` 只在 `AppState` 初始化调用一次（全树审计）；`Database::open` 改为 `ensure_directories`；apply/restore/delete/preview_restore 改用 `audit_run_scope`（journals + 涉及 run 的快照目录）；测试 `startup_runs_exactly_one_full_tree_audit` 与 apply 预算测试断言审计次数。
- [x] C4：`global_tools_for_all_skills/mcp`、`global_assignments_for_all_hooks` 一条聚合查询；三处 `collect_row_versions` 改为 `WHERE id IN` 批量；`db/` 全部 `prepare` → `prepare_cached`（启用 rusqlite `trace` 特性用于 SQL 计数测试）；列表场景 Skill 树摘要走 lstat 指纹缓存（`TREE_DIGEST_CACHE`）。测试：`list_skills_issues_a_constant_number_of_sql_statements`、`list_skills_reuses_tree_digest_until_the_central_tree_changes`。
- [x] C2：`PathState::File` 携带 `StatSignature`；`PendingMutation.before_state` 沿链传递到快照与 rename 前复核（`cheap_state_matches` / `verify_known_path_state`）；临时文件指纹由内存计算；预检移到循环前并用 `PRAGMA data_version` 侦测；测试 `single_write_file_apply_stays_within_io_budget` 断言目标完整读取 ≤ 2。
- [x] C1：journal 改为追加式 JSONL（每阶段一行 + 单次 fsync，首次创建时 fsync 目录），`read_journal/parse_journal` 兼容旧 pretty 单对象格式（含无尾换行时追加补换行）；测试 `legacy_whole_object_journal_is_still_recognized_and_appended_to`；全部故障注入测试通过。
- [x] C6：GitHub 文件下载 6 路并发（`FuturesUnordered` 显式窗口，避免闭包 HRTB 导致 Tauri 命令编译失败）、内存聚合后在 `spawn_blocking` 一次写盘、`Client` 按代理键 `OnceLock` 复用；测试 `files_download_concurrently_and_land_in_one_private_directory`、`a_failed_file_fails_the_whole_download_before_any_file_is_written`。
- [x] `pnpm rust:check`、`pnpm check`。
- [ ] 真实 Tauri 应用做一次多目标 apply 与恢复：无头环境未执行；由 `sync::apply` 全部故障注入测试与 `phase8_e2e` / `hooks_e2e` 集成测试覆盖。
- [x] 更新 `.trellis/spec/backend/quality-guidelines.md`（journal 格式、IO 预算、审计作用域、摘要缓存）、`database-guidelines.md`（聚合查询与 prepare_cached、hash_json 前提）、`skill-import-guidelines.md`（下载并发）。

## 与 PRD 的偏差

- **R1 fsync 上限**：PRD 目标 ≤ 8，实际单个 WriteFile 目标一次 apply 为 16 次（改动前约 36 次）：journal 10 个阶段各 1 次 + journal 目录 1 + 快照根 1 + 快照文件 1 + run 目录 1 + 临时文件 1 + 目标父目录 1。剩余大头是 journal 每阶段各自 durable；去掉任一阶段的 fsync 都会改变崩溃恢复证据，与"崩溃恢复与回滚判定结果与改动前完全一致"冲突，故保留。测试断言上限 16。
- **R4 列表摘要**：采用"(mtime, size) 缓存"方案而非把全树摘要移到 `get_skill`，因为前端列表需要 `CENTRAL_SKILL_CONTENT_CHANGED` 诊断码渲染「同步更改」按钮。缓存键含每个条目的路径/类型/大小/mtime 纳秒/权限/链接目标；Apply 与接管路径始终重新完整校验。
- **A2 读取计数**：只统计 apply 内核自身对目标文件的完整读取（`read_target_bytes`），适配器 `scan_target` 的读取不在内核计数范围。

## 风险文件
`sync/apply.rs`、`sync/mod.rs`、`security/mod.rs`、`db/{mod,skills,mcp,hooks}.rs`、`skills/{service,library,github}.rs`、`app/mod.rs`。

## 回滚点
C5/C3/C4 各自独立；C2 与 C1 在 `sync/apply.rs` 内相互独立可分别 revert。
