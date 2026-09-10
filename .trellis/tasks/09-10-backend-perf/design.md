# 技术设计

## journal append-only
`RunJournal` 文件格式改为 JSONL：首行为 run 头（run_id、preview_id、targets 描述），随后每行为 `{ "target": i, "phase": "...", "at": ts, ... }`。`persist_journal` 变为 `append_journal_phase(target, phase)`：`OpenOptions::append` 写一行后 `sync_data`。读取时逐行解析，非法尾行（崩溃截断）忽略。`journal_reports_crash`、`mutation_may_have_changed_target` 改为基于每个目标的最后 phase。恢复兼容：读到旧格式（单 JSON 对象）时按旧逻辑解析，保证已存在的中断 run 仍可恢复；新格式写入前若发现旧格式 journal，先完成恢复流程。

## 读取传递
`PathState` 在 `create_snapshot` 捕获后作为参数传给 `apply_mutation` → `atomic_replace_file`。rename 前用 `cheap_state_matches(&PathState, symlink_metadata)` 比较文件类型、inode、size、mtime；不匹配则回退到全读校验并按现有冲突逻辑处理。写后校验保持全读。

## 预检
`revalidate_database_preflight` 移到循环前；循环内每次 mutation 前读 `PRAGMA data_version`，与循环前记录值不同则重新执行完整预检。

## 审计
`AppPaths::initialize` 幂等但只在 `AppState::initialize` 调用一次；`Database::open` 不再调用。新增 `audit_run_scope(run_id)` 只遍历 `journals/` 与 `snapshots/<run_id>/`。

## 列表 N+1
`db/skills.rs` 新增 `global_tools_for_all_skills() -> BTreeMap<String, Vec<Tool>>`，mcp/hooks 同理；service 层 `list_*` 先查一次再组装。`inspect_record` 拆为 `inspect_record_shallow`（列表）与 `inspect_record_full`（详情/预览）。`prepare` → `prepare_cached` 全局替换并核对生命周期。

## canonical_json
删除 `sync/mod.rs:317-330`，`hash_json` 用 `serde_json::to_vec`。测试：`serde_json::to_string(&json!({"b":1,"a":2}))` 等于 `{"a":2,"b":1}`。

## GitHub 下载
`futures` 已由 reqwest 传递依赖引入，如需显式加入 `futures-util`。`download` 中 `stream::iter(files).map(|f| self.download_file(f)).buffer_unordered(6).try_collect()`；`download_file` 把内容返回内存（受 `MAX_FILE_BYTES`/`MAX_TOTAL_BYTES` 约束），下载完成后在 `spawn_blocking` 中一次性写盘并 `sync_directory`。`Client` 用 `static CLIENT: OnceLock<Client>`；代理配置来自 `09-10-async-commands-and-probe` 的注入参数，若两任务并行则以参数形式预留。

## 风险
journal 格式变更是最敏感的一项，必须保留旧格式解析并有"旧 journal 中断恢复"测试。
