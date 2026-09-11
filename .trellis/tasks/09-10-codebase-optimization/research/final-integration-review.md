# 最终集成审查（2026-09-11）

对 `research/review-findings.md` 的 48 个条目（A1-A12、B1-B3、C1-C7、D1-D7、E1-E15、F1-F4）在当前 `main`（HEAD `aac80b6`）逐条核对。审阅基线提交为 `e236e42`（九个子任务之前的最后一次 journal 提交）。本次只做核实与记录，未修改任何 `src/` 或 `src-tauri/` 代码。

自动化门禁状态（本次未重跑 `pnpm check`，仅重跑前端 vitest 以取统计）：前端 32 个测试文件 / 282 用例 / 6.6 秒；Rust `#[test]` 320 个；`.github/workflows/check.yml` 对 push(main) 与 PR 执行 `pnpm check` + `pnpm bindings:check`。

状态口径：**已解决** = 代码已改且有证据；**部分解决** = 主要目标达成但审阅条目里的某个子点仍在；**按规范约束处理** = PRD 明确只以规范约束；**未解决** = 无。

## 1. 总览表

### A. 已核实的真实缺陷（含回归测试）

| 编号 | 状态 | 证据（当前代码） | 回归测试 |
|---|---|---|---|
| A1 | 已解决 | `src/features/projects/detail/page.tsx:470-478` `ARTIFACT_LABELS: Record<ArtifactKind, string>` 含 `hook: "Hooks"`，缺项即编译错误；`tsconfig.json:19` `noImplicitReturns: true` | `project-detail-page.2.test.tsx:376` 「工具配置状态为 Hook 目标渲染 Hooks 标签而不是空文案」（断言 `Claude · Hooks` 可见且 `/^Claude · $/` 不存在） |
| A2 | 已解决 | `src/features/hooks/hooks-page.tsx:82-88` `activeTool` 在渲染期按 `visibleTools` 夹逼；`:265,:461-:521,:768` 全部按 `activeTool` 渲染 | `hooks-page.test.tsx:174` 「关闭 Claude 后工具视图夹逼到第一个启用工具，不再展示 Claude 状态与入口」 |
| A3 | 已解决 | `src/features/prompts/prompts-page.tsx:185-195` 走 `useSyncPreviewFlow({ invalidate: refresh })`；`src/features/sync/use-sync-preview-flow.ts:73-80` `applyMutation.onSuccess` 固定调用 `options.invalidate()` | `prompts-page.test.tsx:636` 「编辑档案并按工具图标启用，应用时消费提示词 preview」（`:713-731` 断言 Apply 后 `listPromptProfiles` 调用数增加） |
| A4 | 已解决 | `src-tauri/src/app/tool_probe.rs:447` `ExecutableResolution::{Found{skipped_entries}, Unavailable{skipped_entries}, Unsupported{reason}}`；`:114-118` 三个诊断码常量；`:415-435` 逐条跳过不安全 PATH 项而非整体 `Unsupported`；诊断经 `ToolProfileStatusDto.installationProbeDiagnostic` 透出，前端 `tool-profiles-page.tsx:33`、`refresh-environment-button.tsx:90` 展示 | `app/tool_probe_tests.rs:665` `unsafe_path_entries_are_skipped_instead_of_failing_the_whole_probe`；`:720` `unsafe_first_candidate_and_empty_safe_path_still_fail_closed` |
| A5 | 已解决 | `src-tauri/src/db/mod.rs:773` `backup_database_before_migrations` 仅在有待应用迁移时执行；`:314` 先 `PRAGMA wal_checkpoint(TRUNCATE)`；`:167,:328` `prune_startup_backups` 保留 3 份 | `db/tests.rs:582` `startup_backup_only_runs_when_migrations_are_pending`；`:600` `startup_backup_checkpoints_active_wal_and_prunes_to_three_directories` |
| A6 | 已解决 | `src-tauri/src/projects/native_resources.rs:53-56` `reconcile_project_native_resources` 先只读观测再在一个 IMMEDIATE 事务内写；`projects/service.rs:265` `project_dto` 为纯读（`project_native_resource_summary`）；对账只由 `service.rs:117`(register)、`:181`(rescan)、`native_resources.rs:102`(list_project_native_resources) 触发 | `projects/native_resources_tests.rs:126` `project_reads_do_not_write_native_resource_rows`；`:169` `reconcile_is_atomic_when_a_write_fails_mid_project` |
| A7 | 已解决 | `src/` 中 `window.location.assign` 与 `href="#/` 零命中；`detail/page.tsx:3,70,175` `useNavigate`/`<Link to="/projects">`；`tool-profiles-page.tsx:208` `<Link to="/prompts">` | `project-detail-page.2.test.tsx:405` 「项目不可用时"返回项目列表"通过路由跳转而不是改写 location」；`tool-profiles-page.test.tsx:199`（`:211-214` 断言 link `href="/prompts"`） |
| A8 | 已解决 | `src/features/mcp/mcp-page.tsx:953-957` `SensitiveField` 用 `useId()` 关联 `<label htmlFor>` | **无专门回归测试**；`mcp-page-preview.test.tsx:298,309,318,349` 的 `getByLabelText("Env JSON")` 只间接依赖 label 关联。原条目是可访问性关联脆弱性，可测（改文案后 `getByLabelText` 仍能命中）但未补 |
| A9 | 已解决 | `src-tauri/src/sync/apply/core.rs:1116` `inputs_by_target_path`：无路径 → `STALE_PREVIEW`，重复路径 → `INVALID_INPUT`；`apply/` 非测试代码中 `unwrap_or_default()` 仅剩 `restore.rs` 三处 `target_id`（非 map 键） | `sync/apply/tests.rs:1872` `apply_rejects_inputs_without_a_target_path_instead_of_collapsing_them` |
| A10 | 已解决 | `src-tauri/src/db/sync.rs:307,336` 一个事务内写 `retired_snapshot_cleanup` 队列 + 删行并提交，再逐个删文件；`db/mod.rs:173,689` 启动时重试队列。`retired_snapshot_cleanup` 表在已发布迁移 `0018_remove_project_prompts.sql:8` 已存在，**未新增表结构** | `sync/apply/tests.rs:1786` `delete_snapshots_retires_rows_first_and_queues_undeletable_files`；`:1844` `delete_snapshots_leaves_no_cleanup_queue_entry_after_a_successful_removal` |
| A11 | 已解决 | `AppState.interrupted_run` 字段已删除；`src-tauri/src/app/mod.rs:280-282` 注释说明不缓存；`commands/overview.rs:51-55` 每次直接 `detect_interrupted_run` | **不可测试**：删除只写不读的死状态，无可观察行为差异（子任务 implement.md 已记录） |
| A12 | 已解决 | `src-tauri/src/db/mod.rs:162` `configure_connection` 仅一次 | **不可测试**：PRAGMA 幂等，删第二次调用无行为差异（子任务 implement.md 已记录） |

### B. 主线程阻塞与启动

| 编号 | 状态 | 证据 |
|---|---|---|
| B1 | 已解决 | `src-tauri/src/commands/*.rs` 88 个 `#[tauri::command(async)]` + 1 个 `async fn`（`import_github_skill`），0 个同步主线程命令；`commands/mod.rs` 测试 `every_command_runs_off_the_main_thread` 扫描全部命令源文件（`command_count >= 80`）作为门禁；`commands/skills.rs:53-68` `import_github_skill` 的文件/DB 段包进 `spawn_blocking`；`AppState::database_handle()` 供阻塞线程使用 |
| B2 | 已解决 | `src-tauri/src/lib.rs:333-349` setup 先 `app.manage(AppState::initialize_probing(...))` 再 `spawn_blocking(probe_environment)`；`app/tool_probe.rs:180-191` 五个探测 `std::thread::scope` 并行；`commands/environment.rs:23` 新增 `refresh_environment` 命令、`:11` `environment-ready` 事件；`app/mod.rs:342-412` 探测未完成时返回 `ErrorCode::EnvironmentProbing`；前端 `src/lib/rpc.ts:51-66` `retryWhileEnvironmentProbing`（`providers.tsx:17` 全局 retry）、`src/lib/tauri-events.ts:15` 监听事件，`app-shell.test.tsx:105` 「收到 environment-ready 事件后重新拉取依赖环境的查询」。Tauri 2.11.5 `app.rs setup()` 在用户 setup 闭包前先按配置建窗口，所以窗口创建不等探测。**注意**：setup 闭包内 `paths.initialize()`（全树审计）与 `Database::open`（迁移 + 可能的备份）仍是同步，在事件循环启动前执行；窗口已建但首帧会等这一段，需要真机确认体感（见第 3 节） |
| B3 | 已解决 | `src-tauri/src/skills/github.rs` 无 `std::env` 读取；`lib.rs:360-380` `environment_proxy()` 只在 setup 读一次并注入 `AppState`；`commands/skills.rs:49` `state.github_proxy()` 显式传给 `download_github_skill`；`github.rs:127` `shared_client(proxy)` 按代理键复用 `Client` |

### C. 性能

| 编号 | 状态 | 证据 |
|---|---|---|
| C1 | 已解决（含记录在案的偏差） | journal 改为追加式 JSONL、每阶段单次 fsync（`sync/apply/journal.rs`）；`sync/apply/tests.rs:421` `single_write_file_apply_stays_within_io_budget` 断言 `FSYNC_CALLS <= 16`、`TARGET_READS <= 2`、`AUDIT_TREE_CALLS == 1`。子任务 PRD 目标 ≤ 8，实际 16（改动前约 36）；`backend-perf/implement.md` 记录原因：journal 10 个阶段各自 durable 是崩溃恢复证据，不能再省。规范 `quality-guidelines.md` 已写明 ≤ 16 |
| C2 | 已解决 | `sync/apply/fs_ops.rs:336` `StatSignature`；`plan.rs:178` `PendingMutation.before_state` 沿链传递；`core.rs:861-906` 数据库预检只做一次，循环内用 `PRAGMA data_version`（`validate.rs:379-390`）侦测其它连接提交后才重跑 |
| C3 | 已解决 | `app/mod.rs:276` `paths.initialize()` 只在 `AppState::initialize_internal` 调一次；`Database::open` 只 `ensure_directories`；apply/restore/delete/preview_restore 改用 `audit_run_scope`（`core.rs:721`、`restore.rs:41,125,298,473,492`）；测试 `app/mod.rs:585` `startup_runs_exactly_one_full_tree_audit` |
| C4 | 已解决 | 聚合查询 `db/mcp.rs:204` `global_tools_for_all_mcp`、`db/skills.rs:281` `global_tools_for_all_skills`、`db/hooks.rs:192` `global_assignments_for_all_hooks`；`sync/managed.rs:354-390` `collect_row_versions` 批量 `WHERE id IN`；`db/` 下 `.prepare(` 0 处、`prepare_cached` 28 处；`skills/library/core.rs:486` `TREE_DIGEST_CACHE` 按 lstat 指纹缓存树摘要；测试 `skills/tests.rs:193` `list_skills_issues_a_constant_number_of_sql_statements`、`:230` `list_skills_reuses_tree_digest_until_the_central_tree_changes` |
| C5 | 已解决 | `sync/mod.rs:316-322` `hash_json` 直接序列化，`canonical_json` 已删除；`sync/tests.rs:62` `serde_json_serializes_object_keys_in_sorted_order` 守住 `preserve_order` 未开启的前提 |
| C6 | 已解决 | `skills/github.rs:115` `DOWNLOAD_CONCURRENCY = 6`、`:184` `FuturesUnordered` 并发窗口、`:198` 内存聚合后 `spawn_blocking` 一次写盘、`:127-151` `OnceLock` 复用 `Client`；测试 `github.rs:1047` `files_download_concurrently_and_land_in_one_private_directory`、`:1095` `a_failed_file_fails_the_whole_download_before_any_file_is_written`。`OVERALL_TIMEOUT = 120s`、`MAX_FILES = 4096` 保留（审阅只是记录数值，并非缺陷） |
| C7 | 部分解决 | 已做：`prompts-page.tsx:55-58` `useQueries` + `enabled: enabledTools.has(tool)`；`use-enabled-tools.ts:14` `useMemo`；`router.tsx:6-41` 8 个页面 `lazy`；`dashboard-page.tsx:14,19` `OnboardingWizard`/`SnapshotRestoreDialog` `lazy`；`hooks-page.tsx:798` `useVisibleHookEventGroups` `useMemo`。**未做**：`mcp-page.tsx:97-100` 的表单 state（`form/formOpen/formError`）仍与 981 行的 `McpPage` 同组件，表单每次击键仍触发整页重渲染；`frontend-dedup/implement.md` 写"表单组件拆分"已完成，但代码中未见 MCP 表单独立组件 |

### D. 健壮性与可诊断性

| 编号 | 状态 | 证据 |
|---|---|---|
| D1 | 已解决 | `app/mod.rs:303-321` `database_guard/redactor_read/redactor_write` 一律 `unwrap_or_else(PoisonError::into_inner)`；`commands/*.rs` 中 `state_lock_error` 0 处；测试 `app/mod.rs:551` `a_panic_while_holding_the_database_lock_does_not_poison_later_commands` |
| D2 | 已解决 | `error.rs:159` `source: Option<String>`（`skip_serializing`，不出 RPC 边界）、`:194` `with_source`、`:201` `with_source_redacted`、`:223` `tracing::warn`；`logging.rs` 按日滚动私有日志（0700/0600、7 天保留、`EASYTOAGENTS_LOG`）；`map_err(\|_\|` 从 679 处降到 13 处，剩余全部在 rusqlite 行映射闭包或 `()`-typed 内部探针里；`sync/apply/finalize.rs:315-318` 回滚失败保留 `rollback_error.source()` 诊断 |
| D3 | 已解决 | `sync/apply/core.rs:199` `TargetPhase` 枚举（serde 字符串与历史 journal 一致，未知值落 `Unknown` 保守处理）、`:402` `JournalOperation`；`RunJournal`（`:438-455`）`operation/phase` 均为枚举；`.phase = "...".to_owned()` 0 处；`starts_with("crashed")` 0 处；`journal_reports_crash`（`:743`）与 `mutation_may_have_changed_target`（`:1048`）都走 `TargetPhase::may_have_changed_target()/is_crashed()` |
| D4 | 已解决 | 非测试代码（`mod tests`/`include!("tests.rs")` 之前）扫描结果：`lib.rs:270,274,286`（`export_typescript_bindings`，绑定生成工具路径）与 `lib.rs:352`（`.expect("启动桌面应用失败")`）4 处均为审阅基线已有；`sync/apply/fs_ops.rs:187` `parent_of` 替换了 24 处 `path.parent().expect`；`unreachable!` 0 处。**残留（非新增）**：`mcp/service.rs:756,765,778,787,800,811` `serde_json::to_value(&value.env/headers).unwrap()` 在生产函数 `native_mcp_item` 内，基线 `:853,865,881` 已存在，审阅统计"生产区几乎为零"漏掉了它们。`to_value(BTreeMap<String,String>)` 实际不会失败，但仍建议改为 `?` |
| D5 | 已解决 | `commands/mod.rs:26-51` `with_db`/`with_database_handle`/`with_db_and_redactor` 三个 helper；7 份 `state_lock_error` 全部删除；各命令文件 `.lock()` 0 处 |
| D6 | 按规范约束处理 | `.trellis/spec/backend/database-guidelines.md:66-75` "Published migrations are immutable… Do not edit an existing migration or use `PRAGMA writable_schema` in a new migration… table rebuild" 12 步；迁移目录最后一次改动 `c31eeae 2026-09-08`（早于审阅基线），`git log e236e42..HEAD -- src-tauri/src/db/migrations` 为空 |
| D7 | 已解决 | `src/components/notify.tsx:19` `NotifyProvider` 维护 `notifications[]` 队列 + 单一 `NotifyViewport`；`app-shell.tsx:71` 全局唯一挂载；页面内 `<Notify>` 0 处；`use-notify.ts` 有 Provider 时走 context；`notify.test.tsx` 覆盖；`prompts-page.test.tsx:787` 「直接应用模式下自动同步的预览与 Apply 失败通知按队列堆叠」 |

### E. 结构重复 — 后端

| 编号 | 状态 | 证据 |
|---|---|---|
| E1 | 部分解决 | 已做：`adapters/discovery.rs:343` 唯一 `adapter_for` + `domain/mod.rs:69` `Tool::adapter()`；`:363` 唯一 `native_mcp_container`；`:354` 唯一 `projection_value_at`；`:377` `populate_descriptor_allowed_roots` 把 `allowed_root` 写进描述符，`:407` `descriptor_allowed_root` 统一读取；`ToolAvailability` 改为 `[ToolAvailabilityState; 5]` 按 `Tool` 索引（`:294`），`ExplicitEnvironment.installation_versions: [Option<String>; 5]`；`projects/service.rs:362` 改为迭代 `Tool::ALL`。**残留**：(a) `overview/mod.rs:359-400` `global_allowed_root` 仍保留第二份 tool×artifact → 根目录映射（快照恢复用，与 `discovery.rs:390-401` 映射内容一致但是两份手抄）；(b) 字符串→`Tool` 手写 `match` 基线 9 处、现在仍 9 处（`db/{hooks:261,mcp:662,profiles:824,skills:764,skill_imports:46,mcp_imports:68}.rs`、`overview/mod.rs:365`、`skills/service.rs:1278`、`projects/native_resources.rs:1044`），`Tool::from_stable_str` 在非测试代码里 0 个调用者；`backend-dedup/implement.md` 写"字符串解析统一"但未落地 |
| E2 | 已解决 | `sync/managed.rs`：`safe_row_version`(:336)、`collect_row_versions`(:354)、`readopt_with_scan`(:414)、`list_global_target_statuses`(:497)、`project_dto`(:548) 各一份泛型实现，`McpManagedArtifact/SkillManagedArtifact/HookManagedArtifact` 三个零尺寸标记类型；`mcp/service.rs:413,431,608`、`skills/service.rs:527,763`、`hooks/service_core.rs:230,380,650` 均为薄包装；`domain/mod.rs:381` `ManagedProjectDto` 共享投影，三个 RPC DTO 名保留（避免 Specta 别名折叠改名，implement.md 记录）；`descriptor_for` 仅 `profiles/helpers.rs:1` 一份（mcp/skills/hooks 的 `*_target_descriptor` 是 ~10 行 `DiscoveryContext` 构造 + `find_descriptor`，非重复算法） |
| E3 | 部分解决 | `adapters/discovery.rs:159-260` `TargetDescriptorBuilder` 链式构造器、`build()` 默认 `allowed_root = project_root`、`mcp_container` 自动填充；`path_text` 上移到 `discovery.rs:324`。**残留**：五个 adapter 各自仍保留 10 参数的本地 `fn descriptor(...)` 包装（`claude/mod.rs:349`、`codex/mod.rs:505`、`cursor/mod.rs:158`、`zcode/mod.rs:322`、`opencode/mod.rs:372`），`#[allow(clippy::too_many_arguments)]` 基线 5 处、现在仍 5 处；builder 只是被这些包装内部调用，调用点形状未变 |
| E4 | 已解决 | `adapters/discovery.rs:1175` `ProviderCodec` trait，`claude/codex/zcode/opencode` 四个 adapter 实现；`profiles/service.rs` 1975 行拆为 `helpers/models/prompt/provider/provider_discovery/service_orchestration/service_prelude/sync` 8 个文件（最大 572 行）；`profiles/` 非测试 `match tool` 从 9 处降到 1 处（`models.rs:262` 输入校验）；`cursor_unsupported` 从 10 处降到 7 处且集中在 helpers/provider/prelude |
| E5 | 已解决 | `skills/limits.rs` 唯一限额定义，`github.rs:25` 与 `library` 均引用 |
| E6 | 部分解决（有意限定范围） | `db/mod.rs:192-196` `connection()/connection_mut()` 从 `pub` 收窄为 `pub(crate)`；`db/mod.rs:207` `with_immediate_transaction`；`db/sync.rs` 承接 `sync_runs/sync_items/snapshots/managed_targets/active_writer` SQL，`reject_active_writer` 合一（`db/skills.rs:4,139` 委托 `db::sync`）。**残留**：服务层 SQL 字面量仍有 `overview/mod.rs` 30、`projects/service.rs` 17、`skills/service.rs` 15、`sync/apply/finalize.rs` 6、`hooks/service_native.rs` 6、`mcp/service.rs` 6、`settings.rs` 6；`backend-dedup/implement.md` 明确"资源 CRUD、导入和项目原生观测所需的领域 SQL 仍留在各自模块；本轮 `db/sync.rs` 边界针对跨资源同步引擎"，属有意限定 |
| E7 | 已解决 | `sync/apply.rs` 拆为 `sync/apply/{core 1135, restore 1280, mutation 1027, finalize 713, plan 630, validate 462, fs_ops 375, journal 123, snapshot 113, mod 17}.rs` + `tests.rs 2958`；`RenameFaultContext` 5 元组已消失（改 `ApplyContext`）；大文件内联测试全部外置（`db/mod.rs`、`mcp/service.rs`、`skills/service.rs`、`native_resources.rs`、`tool_probe.rs`、`profiles/*` 的 `#[cfg(test)]` 都只剩 `include!("tests.rs")` 一行）。残留：`mutation.rs` 仍有 4 处 `#[allow(clippy::too_many_arguments)]`（基线 3 处），这是拆分后参数显式化的副作用，非重复 |

### E. 结构重复 — 前端

| 编号 | 状态 | 证据 |
|---|---|---|
| E8 | 部分解决 | `src/features/sync/use-sync-preview-flow.ts` 一份 `previewMutation/applyMutation/readoptMutation` 状态机（含 `mountedRef` 守卫），7 个调用点：`mcp-page.tsx:260`、`skills-page.tsx:189`、`hooks-page.tsx:229`、`prompts-page.tsx:185`、`tool-profiles-page.tsx:43`、`projects/detail/assignments/{mcp:53,skill:51,hook:56}.tsx`；`OpenXxxPreview` 6 份 → `OpenSyncPreview` 1 份 + `OpenProjectPreview`（项目原生资源以 `resourceId+rowVersion+action` 为键，implement.md 记录为有意例外）；`use-import-dialog-state.ts` 统一 `{tool, requestId}` + `randomUUID`；`src/hooks/use-submit-guard.ts` 供 4 处使用。**残留**：仍有 9 处手写 `useRef(false)` 在途守卫（`skill-github-import-dialog.tsx:26`、`skill-directory-import-dialog.tsx:28`、`skill-import-dialog.tsx:68-70`、`skills-page.tsx:68`、`projects/detail/native-resources.tsx:41`、`app-shell.tsx:229,233`）未迁到 `useSubmitGuard` |
| E9 | 已解决 | `src/features/projects/detail/page.tsx` 478 行（≤ 500）；`assignments/{mcp 187, skill 167, hook 260, project-assignments-section 112}.tsx` 同构段抽成 `ProjectAssignmentsSection`；`src/lib/projects-api.ts:40-67` `invalidateProjectScope` 统一失效集合（13 个调用点），`projects/` 下 `Promise.all([invalidate…])` 0 处；`project-detail-page.tsx` 变成 2 行兼容 re-export（仅 `project-detail-page.test-helpers.tsx` 引用）。残留死代码：`detail/legacy-page.tsx`（2 行，全仓库 0 引用）、`detail/assignment-card.tsx`（4 行 re-export，被 3 个 assignments 文件引用，可直接改成引用 `project-assignments-section`） |
| E10 | 已解决 | `src/components/ui/dialog.tsx` 唯一 `role="dialog"` 与唯一遮罩 `bg-slate-950/40`（`:32`）；其它文件 `role="dialog"`、`bg-black/40` 0 处 |
| E11 | 已解决 | `src/components/ui/field.tsx` 唯一 `Field`；`src/components/tool-icon-toggle.tsx` `ToolIconToggle` 被 `platform-assignment-button.tsx`、`hooks-page.tsx`、`projects/detail/page.tsx` 复用；三份本地 `function Field` 已删 |
| E12 | 已解决 | `src/styles.css:19-26` 新增 `destructive/warning/success/info` 语义 token；`src/lib/tone-class.ts` `toneClass(Tone)`；非测试代码 `text-red-700 dark:text-red-300` 等手写配对 0 处 |
| E13 | 已解决 | `src/bindings/commands.ts:727-728` 由后端导出 `TOOL_CAPABILITIES`、`HOOK_EVENT_SUPPORT` 常量（`domain/mod.rs` 元数据）；`src/lib/tool-metadata.ts:24` `capabilitiesFor` 从后端常量派生，`:78-86` `PROFILE_TOOLS/MCP_TOOLS/SKILL_TOOLS/HOOK_TOOLS` 全部 `ALL_TOOLS.filter(capability)`；`hook-events.ts:77` `hookEventSupportedByTool` 查 `HOOK_EVENT_SUPPORT`，手抄表已删；`tool-metadata.test.ts` 做一致性断言 |
| E14 | 已解决 | `src/test/render.tsx` `renderWithProviders`（隔离 QueryClient + MemoryRouter + NotifyProvider），测试文件 `new QueryClient(` 0 处、全部 `.test.tsx` 使用 helper；`src/test/commands-mock.ts` `mockCommands(actual.commands)` 15 个工厂；`src/test/fixtures/{dtos,preview-plan}.ts` `makePreviewPlan/makeSkillPreview/makeMcpPreview/makeTarget` 被 10 个文件使用；`eslint-disable unbound-method` 12 份 → 1 份（`commands-mock.ts:9`，带理由）；`skills-page.test.tsx` 2440 行拆为 5 个文件，`mcp-page` 拆 4、`project-detail-page` 拆 4，最大文件 880 行（`tool-profiles-page.test.tsx`，规范阈值 900）；32 文件 / 282 用例 / 6.6 秒 |
| E15 | 已解决 | `src/` 下 `.gitkeep` 0 个；`src/features/providers/` 已删；`src/hooks/` 现有 `use-submit-guard.ts`、`src/features/sync/` 现有两个 hook；`src/lib/app-info.ts` 已删；`hookImportQueryOptions` 移到 `src/lib/hooks-api.ts:47`；`unwrapResult/profileErrorText/ProfileRpcError` 归 `src/lib/rpc.ts`，`profile-api.ts:7` 仅 re-export 兼容。残留：`detail/legacy-page.tsx` 死文件（同 E9） |

### F. 工程配置

| 编号 | 状态 | 证据 |
|---|---|---|
| F1 | 已解决 | `.github/workflows/check.yml`：`on.push.branches: [main]` + `pull_request`，macos-15，步骤 `pnpm check` 与 `pnpm bindings:check`；actions 均 SHA 固定 |
| F2 | 已解决 | `src-tauri/Cargo.toml:4` 与 `tauri.conf.json:40` 已改为"Claude、Codex、Cursor、ZCode 与 OpenCode"。残留（装饰性）：`README.md:55` 截图 `alt` 文案仍是"Claude 与 Codex 配置状态" |
| F3 | 已解决 | `Cargo.toml:22` `reqwest = "0.12"`（`Cargo.lock` 解析 0.12.28，hyper 1.11 / rustls 0.23）；`serde_yaml` → `serde_yaml_ng = "0.10"`（`:25`）。`Cargo.lock` 中另有 `reqwest 0.13.4` 为 `tauri` 自身的传递依赖，与本项目无关 |
| F4 | 已解决 | `src-tauri/src/hooks/import.rs:316` `mod tests`，6 个 `#[test]`（基线 0） |

### 统计

- 已解决：42 条（含 C1 记录在案的偏差、A8 测试为间接覆盖）
- 部分解决：5 条（C7、E1、E3、E6、E8）
- 按规范约束处理：1 条（D6）
- 未解决：0 条

## 2. PRD「跨子任务验收」前五项

| 验收项 | 结论 | 说明 |
|---|---|---|
| A1-A12 每条都有回归测试或"不可测试"说明 | 基本达成 | A1-A7、A9、A10 有具名测试；A11、A12 在子任务 implement.md 写明不可测试并说明理由；**A8 没有专门测试**（只有 `getByLabelText("Env JSON")` 的间接覆盖），建议补一条"修改 label 文案后 `getByLabelText` 仍能定位输入框"的用例 |
| 主窗口在探测完成前可见；apply/restore/list_projects/list_skills 不在主线程做文件 IO | 代码层面达成，需真机确认 | 命令全部 `#[tauri::command(async)]`，`every_command_runs_off_the_main_thread` 测试守门；探测在 `spawn_blocking` 且五路并行。但 setup 闭包内 `AppPaths::initialize`（全树 chmod/canonicalize 审计）与 `Database::open`（迁移、备份、`retired_snapshot_cleanup` 重试）仍同步，窗口已创建但首帧渲染要等这段完成；快照目录很大或需要迁移时体感可能仍有延迟 |
| `tool_adapter/allowed_root/descriptor_for/safe_row_version` 各只剩一份 | 3/4 达成 | `adapter_for`、`descriptor_for`、`safe_row_version` 各一份；`allowed_root` 仍有两份映射：`adapters/discovery.rs:390-401`（写入描述符）与 `overview/mod.rs:359-400 global_allowed_root`（快照恢复按 `(tool, artifact)` 字符串重算）。两份内容目前一致，但新增工具时要改两处 |
| 五个中央页面共用同一预览应用流程 hook；`project-detail-page.tsx` ≤ 500 行 | 达成 | `useSyncPreviewFlow` 覆盖 MCP/Skills/Hooks/Prompts/Provider 五页及项目详情三类分配；`detail/page.tsx` 478 行，`project-detail-page.tsx` 为 2 行 re-export |
| 存在对 push 与 PR 执行 `pnpm check` 的工作流 | 达成 | `.github/workflows/check.yml` |

第六项（真实 Tauri 应用走查）见第 4 节。

## 3. 「跨子任务约束」核对

| 约束 | 结论 | 证据 |
|---|---|---|
| 前端非测试源码不新增 `as` 断言 / `any` / `eslint-disable` | 达成（一处测试基建例外） | 非测试、非 `src/bindings/` 源码：`as` 0 处、`any` 0 处。`src/bindings/commands.ts` 含 98 处 `as`（tauri-specta 生成，豁免）。`eslint-disable` 现有 2 处：`onboarding-wizard.tsx:294`（基线 `:288` 同一处，仅行号漂移）与 `src/test/commands-mock.ts:9`（新增，`@typescript-eslint/unbound-method`，用于 `Object.prototype.hasOwnProperty` 安全读取键，带说明；位于测试基建目录，非页面源码）。若严格按"不新增"口径，这一处是测试工具的例外，建议接受或改用 `Object.hasOwn` 消除 |
| 后端生产路径不新增 `unwrap/expect/panic` | 达成 | 对所有非 `tests.rs`/`*_tests.rs` 文件截至 `mod tests`/`include!("tests.rs")` 之前扫描：`lib.rs:270,274,286,352`（基线已有）与 `mcp/service.rs:756-811` 6 处 `serde_json::to_value(...).unwrap()`（基线 `:853,865,881…` 已有）。**无新增**；`mcp/service.rs` 这 6 处是审阅统计遗漏的既有残留，建议后续改 `?` |
| 迁移 0001-0020 未被改写 | 达成 | `src-tauri/src/db/migrations/` 20 个文件；`git log --oneline e236e42..HEAD -- src-tauri/src/db/migrations` 为空；最后一次改动 `c31eeae`（2026-09-08，早于审阅）；`MIGRATIONS` 列表仍 20 项 |
| 不改变原生目标写入合同 | 未发现变化 | 预览/确认/快照/恢复入口签名与错误矩阵未改；`sync::apply::tests` 全部故障注入测试保留；journal 新格式向后兼容旧 pretty JSON（`legacy_whole_object_journal_is_still_recognized_and_appended_to`） |
| 不做表结构变更 | 达成 | A10 使用的 `retired_snapshot_cleanup` 表在 0018 已存在 |
| 每个子任务更新 `.trellis/spec/` | 达成 | 九个实现提交中除测试基建（1 份）外每个提交都含 `.trellis/spec` 或 `docs/` 变更（dbea5c8 2、42d91e2 1、2ea5b71 5、3f2434d 3、7957fdb 6、acfbc28 4、7b792ff 1、a4ff8b7 4） |

## 4. 未解决 / 部分解决项与建议

以下均为非阻塞项，按建议优先级排列。均未在本次审查中修改。

1. **E1 残留：字符串→`Tool` 手写 `match` 9 处未收敛**（`db/{hooks,mcp,profiles,skills,skill_imports,mcp_imports}.rs`、`overview/mod.rs:365`、`skills/service.rs:1278`、`projects/native_resources.rs:1044`）。`domain/mod.rs:31` 的 `Tool::from_stable_str` 已存在但 0 个非测试调用者。新增工具时这 9 处每处都要改，且各自错误类型不同（`rusqlite::Error::InvalidQuery` / `AppError::conflict` / `AppError::invalid_input`）。建议：在 `db/` 加一个 `column_tool(row, idx) -> rusqlite::Result<Tool>` 与在 `overview`/`native_resources` 用 `from_stable_str().ok_or_else(...)`，然后加一条编译期/测试期守门（例如 grep 测试断言 `"claude" => ` 只出现在 `domain/mod.rs`）。
2. **E1 残留：`allowed_root` 两份映射**（`adapters/discovery.rs:390-401` 与 `overview/mod.rs:384-400`）。建议 `global_allowed_root` 直接构造 `TargetDescriptor` 后调用 `populate_descriptor_allowed_roots` 或抽公共 `global_root_for(environment, tool, artifact)`。
3. **E3 残留：五个 adapter 的 10 参数 `descriptor()` 包装**。builder 已就位，但调用点仍走本地包装；建议把各 adapter 的 `discover` 直接用 `TargetDescriptor::builder(...)` 链式调用，删除包装与 5 处 `too_many_arguments`。
4. **E8 残留：9 处手写 `useRef(false)` 在途守卫**未迁 `useSubmitGuard`（`skill-*-import-dialog.tsx`、`skill-import-dialog.tsx`、`skills-page.tsx:68`、`projects/detail/native-resources.tsx:41`、`app-shell.tsx:229,233`）。
5. **C7 残留：`mcp-page.tsx` 表单 state 仍在 981 行的 `McpPage` 内**（`:97-100`）。implement.md 记录为已做但代码未体现。建议抽 `McpFormDialog` 组件持有 `form/formError`，页面只保留 `formOpen` 与提交回调。
6. **A8 缺专门回归测试**。建议在 `mcp-page-list-crud.test.tsx` 增加一条：渲染编辑对话框后 `getByLabelText("Headers JSON")`/`("Env JSON")` 返回的元素 `id` 与 label 的 `htmlFor` 相等，且 `id` 不包含 label 文案。
7. **D4 既有残留：`mcp/service.rs:756-811` 六处 `serde_json::to_value(...).unwrap()`**（非本轮新增，审阅统计遗漏）。`to_value(BTreeMap<String,String>)` 实际不会失败，但按规范应改 `map_err(|e| AppError::internal(...).with_source(e))?`。
8. **死文件**：`src/features/projects/detail/legacy-page.tsx`（0 引用）可删；`detail/assignment-card.tsx` 是 4 行 re-export，3 个调用方可直接 import `project-assignments-section`；`src/features/projects/project-detail-page.tsx` 2 行 shim 仅测试 helper 引用，可让 helper 直接 import `detail/page`。
9. **E6 有意限定**：服务层仍有约 90 处 SQL 字面量（`overview/mod.rs` 30 最多）。implement.md 已声明本轮边界只覆盖同步引擎；如后续继续收敛，`overview/mod.rs` 与 `projects/service.rs` 优先。
10. **F2 装饰性残留**：`README.md:55` 截图 `alt` 仍写"Claude 与 Codex"。
11. **`src/test/commands-mock.ts:9` 新增 `eslint-disable`**：可改用 `Object.hasOwn(actual, key)` 消除，避免与"不新增 eslint-disable"口径冲突。
12. **`list_project_native_resources` 是读命令但会写库**（`native_resources.rs:102` 前置对账）。这是子任务 design.md 明确的选择（保证视图新鲜），且对账已事务化；但与 A6 的"读命令不写库"表述有张力，建议在 `quality-guidelines.md` 里把该命令列为显式例外。

**新的回归**：未发现。所有残留项均为既有代码或子任务范围内有记录的未完成子点，没有本轮改动引入的行为退化。

## 5. 需要人工验证项

以下需要在真实 Tauri 应用（隔离 fixture HOME / `CLAUDE_CONFIG_DIR` / `CODEX_HOME`）中验证；两个子任务（`backend-perf`、`frontend-dedup-and-split`）的 implement.md 都把真机走查标为未执行。

1. **完整链路走查**（PRD 验收第六项）：导入（目录 + GitHub）→ 分配（全局 + 项目）→ 预览 → 应用 → 快照恢复，五个中央页面各走一次，确认共享 `useSyncPreviewFlow` 后 `preview_confirm` 与 `direct` 两种应用模式行为一致，通知按队列堆叠不互相覆盖（D7）。
2. **启动体感（B2）**：冷启动观察主窗口是否在工具探测完成前出现；探测中依赖环境的查询是否显示"探测中"并在 `environment-ready` 事件后自动刷新；点击"刷新环境"按钮能否重新探测。特别在快照目录较大（几百个 run）或有待应用迁移时观察首帧延迟（setup 内的全树审计与 `Database::open` 仍同步）。
3. **PATH 诊断（A4）**：在 shell 中让 `PATH` 含 `.` 或相对路径后启动应用，确认工具仍能被探测到，且工具配置页显示 `INSTALLATION_PROBE_SKIPPED_PATH_ENTRIES` 对应文案而非整体不可用。
4. **备份裁剪（A5）**：用旧版本数据库（`schema_version < 20`）启动一次，确认 `database_backups/` 只在迁移时新增且最多保留 3 份；再次启动不再新增备份。
5. **快照删除队列（A10）**：把某个快照文件设为只读目录后删除该快照，确认 UI 不报 `ATOMIC_WRITE_FAILED`、行已删除，恢复权限后重启应用文件被清理。
6. **journal 兼容（C1）**：用改动前版本产生的 pretty JSON journal（可从旧备份目录取）放入 `journals/`，启动后确认中断运行检测与恢复流程读取正常。
7. **GitHub 导入（B3/C6）**：在设置了 `HTTPS_PROXY` 的 shell 与未设置的 shell 下各导入一次多文件 Skill，确认代理生效、并发下载完成且失败时不留半写目录。
8. **Hook 目标标签（A1）**：给某项目配置 Claude Hooks 后打开项目详情"工具配置状态"，确认显示 `Claude · Hooks`。

## 6. 后续修复（2026-09-11，同日集成收尾）

第 4 节 1-12 条在同一工作日由集成收尾提交处理完毕，代码状态以该提交为准：

| 第 4 节编号 | 处理结果 |
|---|---|
| 1（E1 字符串→Tool） | `db::column_tool` 唯一入口，9 处手写 match 全部删除；`domain::tests::tool_literal_dispatch_only_lives_in_domain` 守门。`db/hooks.rs` 读侧额外用 `tool_capabilities()` 保持"OpenCode 不支持 Hooks"的 fail-closed |
| 2（E1 allowed_root） | `adapters::global_root_for` 唯一映射，`overview::global_allowed_root` 仅保留能力门禁后委托 |
| 3（E3 descriptor 包装） | 五个 adapter 直接用 `TargetDescriptor::builder`，`too_many_arguments` 在 `adapters/` 归零；40 组描述符 dump 前后逐字节一致 |
| 4（E8 useRef 守卫） | 9 处迁到 `useSubmitGuard`；`skill-import-dialog.tsx` 两个一次性闩语义不同，保留并注释 |
| 5（C7 MCP 表单） | 拆出 `mcp-form.ts` 与 `mcp-form-dialog.tsx`，`mcp-page.tsx` 981 → 615 行，弹窗按草稿 id 重新挂载 |
| 6（A8 测试） | `mcp-page-list-crud.test.tsx` 新增 label/`htmlFor` 关联用例 |
| 7（D4 unwrap） | `mcp/service.rs` 六处改为 `string_map_value(...)?` |
| 8（死文件） | `legacy-page.tsx`、`assignment-card.tsx`、`project-detail-page.tsx` shim 已删除 |
| 9（E6） | 维持有意限定，不在本轮处理 |
| 10（README alt） | 已更新 |
| 11（eslint-disable） | 改用 `Object.hasOwn`，非测试源码仅剩基线 `onboarding-wizard.tsx` 一处 |
| 12（读命令对账例外） | 已写入 `quality-guidelines.md` |

更新后统计：已解决 47 条、按规范约束处理 1 条（D6）；E6 按子任务记录保留为有意限定范围。第 5 节真机验证项仍待人工执行。
