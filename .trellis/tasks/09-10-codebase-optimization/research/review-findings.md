# 全项目审阅结论（2026-09-10）

三路审阅（Rust 同步核心与 DB、Rust 服务层与适配器、React 前端）加工程配置检查的合并结论。所有 file:line 均已在审阅当日核对；实施前请再次确认行号。

## 统计基线

| 指标 | 数据 |
|---|---|
| 规模 | Rust 约 5.5 万行，前端约 2.6 万行 |
| Tauri 命令 | 87 个；仅 `commands/skills.rs:39 import_github_skill` 是 `async fn`；无 `#[tauri::command(async)]`、无 `spawn_blocking` |
| 测试内联占比 | `db/mod.rs` 77%、`mcp/service.rs` 64%、`skills/service.rs` 60%、`native_resources.rs` 48%、`tool_probe.rs` 48%、`profiles/service.rs` 42%、`sync/apply.rs` 32%；`hooks/import.rs` 0%、`mcp/import.rs` 10%；`db/{hooks,mcp,projects,skills,native_resources,mcp_imports,skill_imports}.rs` 与 `commands/*.rs` 无测试 |
| `unwrap()` | 生产区几乎为零（仅 `lib.rs` 启动期） |
| 生产区 `expect(` | `sync/apply.rs` 24 处（全部 `path.parent().expect`）；`sync/mod.rs:313/909`；`mcp/import.rs:52`；`adapters/mod.rs:1864,1876,1968-1976`；`mcp/service.rs:526`、`hooks/service.rs:296` 为 `unreachable!` |
| 吞错 `map_err(\|_\|` | 679 处；无日志框架（`tracing/log` 仅 `skills/github.rs`） |
| `prepare_cached` | db/ 下 28 处 `.prepare(`，0 处 `prepare_cached` |
| migration | 20 个；10 个使用 `PRAGMA writable_schema` 改写 `sqlite_master`（0008/0009/0010/0013/0014/0017/0018/0019/0020） |
| `Tool::` | 715 次 / 27 文件；`match tool` 穷举 16 文件 |
| 子进程 | 生产代码仅 `git/mod.rs:286`、`app/tool_probe.rs:464` |
| 网络 | 仅 `skills/github.rs` async `reqwest::Client`；无 `reqwest::blocking` |
| 前端断言 | 非测试源码 0 处 `as`；`eslint-disable` 仅 12 个测试头 `unbound-method` + `onboarding-wizard.tsx:288` |
| 前端 hooks | 所有页面 `useMemo/useCallback` 为 0；`invalidateQueries` 48 处，其中 `project-detail-page.tsx` 19 处 |
| 前端路由 | `lazy/Suspense` 0 处 |
| 前端测试 | 18 文件 268 用例，约 6 秒 |
| CI | 仅 `release.yml`（手动发布）；没有对 push/PR 执行 `pnpm check` 的工作流 |

## A. 已核实的真实缺陷

- **A1 前端** `src/features/projects/project-detail-page.tsx:1463` `artifactLabel` 缺 `"hook"` 分支；`ArtifactKind`（`bindings/commands.ts:731`）含 `hook`，后端 `projects/service.rs:409` 会产出 `ArtifactKind::Hook`，`isProjectResourceKind`（`:808-816`）放行 hook，卡片渲染 `Claude · `（空）。`tsconfig.json` 未开 `noImplicitReturns`。测试夹具 `project.targets` 无 hook 目标。
- **A2 前端** `src/features/hooks/hooks-page.tsx:99` `useState<Tool>("claude")`，`visibleTools`（`:98`）只影响按钮；`:314` 与 `:594` 仍按 `activeTool` 渲染。关闭 Claude 后仍显示 Claude 状态与导入入口。`project-detail-page.tsx:96-100` 已有夹逼写法。
- **A3 前端** `src/features/prompts/prompts-page.tsx:217` `applyMutation.onSuccess` 缺少其它三页都有的 `refresh()`。
- **A4 后端** `src-tauri/src/app/tool_probe.rs:387-391` `resolve_executable`：PATH 任一条目非安全绝对路径即整体 `Unsupported`；`:399-412` 任一 PATH 目录下同名非文件也 `Unsupported`。含 `.`、`./node_modules/.bin` 的 PATH 常见，导致全部工具 `*_INSTALLATION_PROBE_UNSUPPORTED`，写入被 fail-closed 阻断且无可读原因。
- **A5 后端** `src-tauri/src/db/mod.rs:150-153` 每次打开数据库无条件备份（`:628-658 backup_database_before_migrations`），无任何 prune；备份含 Provider 凭据。
- **A6 后端** `src-tauri/src/projects/native_resources.rs:239-293 reconcile_descriptor` 多条写库无事务，由 `projects/service.rs:307 project_dto` 触发，即 `list_projects`/`get_project` 都写库。
- **A7 前端** `project-detail-page.tsx:244` `window.location.assign("#/projects")`、`tool-profiles-page.tsx:201` `<a href="#/prompts">` 绕过 react-router。
- **A8 前端** `mcp-page.tsx:1047` `SensitiveField` 用 label 文案推断 `id`。
- **A9 后端** `sync/apply.rs:3389` `input.descriptor.path.as_deref().unwrap_or_default()` 作为 `BTreeMap` 键，多个 `path == None` 输入互相覆盖。
- **A10 后端** `apply.rs:4219-4265 delete_snapshots` 先删文件再删行，commit 失败留下悬空行。
- **A11 后端** `commands/overview.rs:55-66,102-106` `interrupted_run` RwLock 只写不读。
- **A12 后端** `db/mod.rs:158,160` `configure_connection` 执行两次。

## B. 主线程阻塞与启动

- **B1** 86/87 命令同步 `fn`，Tauri 2 同步命令在主线程执行。重操作：`projects/service.rs:42-56 list_projects` → `observe_project`（`:348` git 子进程；`:362-366` 五个 Adapter `discover` + `scan_target` 读文件 SHA-256）+ `reconcile`；`skills/service.rs:53-58 list_skills` → `skill_dto:1406` → `library.rs:424 inspect_central_skill` → `digest_tree` 全树哈希；`commands/profiles.rs:205-221 apply_profile_preview`、`commands/overview.rs:87-108 restore_snapshot` 持 `Mutex<Database>` 做全部 fsync。`import_github_skill`（`commands/skills.rs:45-51`）在 `.await` 后于 tokio worker 上做同步文件与 DB 操作。
- **B2** `lib.rs:302-319` setup 里同步调用 `probe_release_environment`；`tool_probe.rs:159-164` 串行 5 个探测，每个 `DEFAULT_TOOL_PROBE_TIMEOUT = 3s`（`:27`）。`AppState.environment` 仅初始化写一次（`app/mod.rs:200`），无刷新命令。
- **B3** `skills/github.rs:565-585 environment_proxy` 直接读进程 env，与 `docs/maintainers/adding-tool-adapter.md` §3 的"显式环境注入"原则冲突。

## C. 性能

- **C1 fsync 风暴** `persist_journal` 57 个调用点，每次经 `atomic_replace_file`：`sync_all`（`apply.rs:2799`）→ chmod → `sync_all`（`:2801`）→ rename → `sync_directory`（`:2897`）。单个 WriteFile 目标：5 次 journal × 3 + 目标 2 + 父目录 1 + 快照文件 1 + 快照目录 2（`:2308,2341`）≈ 21 次 F_FULLFSYNC。
- **C2 重复读取** `capture_path_state`（`:2250`）全量读 + SHA-256；一次 WriteFile 调用链 `:2297`、`:597`、`:2425`、`:2773`、`:2848`、`:2574` 共 5-6 次。`:586` 每个 mutation 调 `revalidate_database_preflight`（`:1236-1258`），O(mutations × items) SQL。
- **C3 全树审计** `security/mod.rs:452-503 audit_private_tree` 递归 `set_permissions` + `canonicalize`；启动链 `app/mod.rs:160 paths.initialize()` → `db/mod.rs:152 paths.initialize()` → `app/mod.rs:163 audit_permissions()`；运行时 `apply.rs:407`、`:4117` 每次调用。
- **C4 N+1** `skills/service.rs:53-58` 每条 `inspect_record`（`:1081-1095`）+ `db/skills.rs:295-311 global_tools_for_skill`；`mcp/service.rs:44-52` 每条 `global_tools_for_mcp`；`collect_row_versions`（`mcp:1073-1084`、`hooks:1180-1191`、skills 同）循环内逐条 `get_*`。
- **C5 canonical_json** `sync/mod.rs:317-330` 递归重建对象排序键；`cargo tree -e features -i serde_json` 显示特性为 `[alloc, default, raw_value, std]`，无 `preserve_order`，`Map` 已是 `BTreeMap`。`hash_json`（`:311-315`）、`scan_target`（`:195`）每次 hash 都深拷贝。
- **C6 GitHub 下载** `skills/github.rs:152-154` 逐文件串行 await；`:291-301` async 内阻塞 `write_all + sync_all`；`:118-127` 每次新建 `Client`；`OVERALL_TIMEOUT = 120s`（`:31`）、`MAX_FILES = 4096`（`:25`）。
- **C7 前端** `prompts-page.tsx:65-71` 为 5 个工具无条件 `useQuery(toolProfileStatusQueryOptions)`，不看 `enabledTools`（`:74`）；`use-enabled-tools.ts:13` 每渲染 `new Set`；`mcp-page.tsx:113-116` 表单 state 与整页同组件；`hooks-page.tsx:594-612` 三层循环无 memo；`router.tsx:3-11` 静态 import 全部页面，`OnboardingWizard`(745 行)、`SnapshotRestoreDialog`(353 行) 随 Dashboard 首屏加载。

## D. 健壮性与可诊断性

- **D1 锁中毒** `app/mod.rs:136-140` `std::sync::Mutex/RwLock`；所有命令 `.lock().map_err(|_| state_lock_error())` 映射为 `WriteInProgress`；仅 `tool_probe.rs:865` 处理 `PoisonError`。任一 panic 后每个命令永久失败。
- **D2 错误上下文丢失** 679 处 `map_err(|_|`；`db/mod.rs:157,222,283` 丢 rusqlite 错误；`apply.rs:2803-2805 let _ = error;`；`apply.rs:3654` 丢弃回滚失败原因，journal 只留 `"rollback_failed"`。`AppError::new` 只收 `&'static str`（`error.rs:144`）是正确的脱敏合同，需加不出 RPC 边界的 `source`。
- **D3 journal 字符串状态机** `journal.targets[i].phase = "xxx".to_owned()` 57 处；`apply.rs:722-739 mutation_may_have_changed_target` 靠 12 个字面量决定是否回滚；`:441-447 journal_reports_crash` 用 `starts_with("crashed")`；`RunJournal.phase/operation` 为 `String`（`:202-235`）。
- **D4 生产 expect** 见统计表；同步命令 panic 直接终止进程。
- **D5 命令层样板** `fn state_lock_error()` 在 `commands/{skills:165, hooks:175, profiles:223, mcp:186, projects:107, overview:110, settings:28}` 7 份；每个命令重复三行加锁。
- **D6 migration** `writable_schema` 改写 + `db/mod.rs:308-546 validate_migration_preconditions` 硬编码 v10/v18/v19/v20 SQL 文本锚点，`:296-301` 手动 `schema_version + 1`，`:402-416` 事务内修补 `sqlite_master`。**已发布迁移不可重写**；只能约束后续迁移改用 12 步表重建，并把前置校验写进 `.sql`。
- **D7 前端通知** `use-notify.ts:8` 单槽；`<Notify>` 在 `app-shell.tsx:531`、`mcp-page.tsx:353`、`hooks-page.tsx:327`、`prompts-page.tsx:325`、`skills-page.tsx:254` 各一份，同一 `fixed top-4 right-4 z-[60]`；`mcp-page.tsx:141-149` 的提示被 `:317-321` 立即覆盖。

## E. 结构重复

### 后端
- **E1 Tool 分派** `tool_adapter()/adapter_for()` 7 份：`profiles/service.rs:1829`、`skills/service.rs:805`、`hooks/service.rs:917`、`mcp/service.rs:769`、`projects/service.rs:791`、`native_resources.rs:1065`、`sync/apply.rs:1310`。`allowed_root` 5 份且不一致：`mcp/service.rs:691-700`（Claude → `home()`）、`skills/service.rs:749-759`、`hooks/service.rs:850-859`（Claude → `claude_config_dir()`）、`profiles/service.rs:1844-1859`、`overview/mod.rs:359-385`。`native_mcp_container` 3 份：`mcp/service.rs:785`、`projects/service.rs:735`、`native_resources.rs:1048`。`projection_value_at/json_value_at` 4 份：`mcp:799`、`hooks:1344`、`projects/service.rs:745`、`native_resources.rs:1057`。字符串→Tool 手写 2 份（`overview/mod.rs:339-347`、`native_resources.rs:966-975`）而 `domain/mod.rs:29 string_enum!` 已有 `from_stable_str`。`projects/service.rs:362-366` 逐个点名五个 Adapter，`native_resources.rs:1075` 已有 `tool_adapters()`。`ToolAvailability`（`adapters/mod.rs:224-230`）与 `ExplicitEnvironment`（`:256-274`）每工具一字段。
- **E2 三份同构服务** `safe_row_version` 4 份（`mcp:1331`、`skills:1451`、`hooks:1339`、`profiles:1975`，profiles 文案已分叉 `:1977`）；`project_dto` 3 份（`mcp:1188`、`skills:1430`、`hooks:1296`）及三个同构 DTO/`*ProjectSelectionState`（`mcp/models.rs:133`、`skills/models.rs:96`、`hooks/models.rs:107`）；`readopt_with_scan` `mcp:402-495` 与 `hooks:445-540`；`list_global_*_target_statuses` `mcp:496-545` 与 `hooks:271-311`；`collect_row_versions` 3 份；`descriptor_for` 4 份（`mcp:741`、`skills:778`、`hooks:885`、`profiles:1775`）；`prepare_*_sync` 尾部 40 行（`mcp:685-720`、`skills:735-770`、`hooks:840-870`）。
- **E3 Adapter descriptor 构造** `adapters/claude/mod.rs:196-228` 10 参数 `descriptor()`（`#[allow(clippy::too_many_arguments)]`）+ `path_text()`；`codex/mod.rs:51-62` 11 参数；cursor/zcode/opencode 各一变体。`ToolAdapter` trait（`adapters/mod.rs:918-950`）本身清晰。
- **E4 profiles per-tool** `profiles/service.rs:117,193,457,957,1016,1039,1541,1683,1836,1850` 9 个 `match tool`；`cursor_unsupported` 10 处；`:997-1500` 约 500 行 per-tool discover。
- **E5 限额常量重复** `skills/library.rs:31-36` 与 `skills/github.rs:25-30`。
- **E6 SQL 越层** `Database::connection()/connection_mut()`（`db/mod.rs:180-186`）`pub`；SQL 字面量：`sync/apply.rs` 58、`skills/service.rs` 34、`mcp/service.rs` 29、`overview/mod.rs` 23、`projects/service.rs` 13、`sync/mod.rs` 13、`skills/import.rs` 10、`hooks/service.rs` 8、`settings.rs` 6；`db/skills.rs:143 reject_active_writer` 与 `sync/apply.rs:785 active_writer` 重复语义。
- **E7 大文件职责** `sync/apply.rs`：校验 `835-1310`、规划 `1325-2120`、路径原语 `1923-2282`、原子文件操作 `2755-3320`、journal/快照 `2282-2410`、编排 `396-835,3371-3800`、回滚/恢复 `3795-4055,4385-5264`、快照列表 `4055-4385`、行解析 `5264-5434`、测试 `5434-`。`RenameFaultContext` 5 元组（`:2755-2761`）；`too_many_arguments` 于 `:2629,2902,3021`。

### 前端
- **E8 预览状态机** `mcp-page.tsx:273-349`、`hooks-page.tsx:236-312`、`skills-page.tsx:195-250`、`prompts-page.tsx:188-228` 的 `previewMutation/applyMutation/readoptMutation` 逐行相同；`OpenXxxPreview` 6 份（`tool-profiles-page.tsx:25`、`project-detail-page.tsx:49`、`mcp-page.tsx:67`、`skills-page.tsx:49`、`hooks-page.tsx:66`、`prompts-page.tsx:44`）；`XxxPreviewRequest/ApplyRequest` 8 份；`openImport {tool, requestId}` + `crypto.randomUUID()` 6 处；`saveInFlight = useRef(false)` 9 文件。`src/features/sync/` 仅 `.gitkeep`。
- **E9 项目详情页** 1474 行 12 组件；`ProjectMcpAssignments`(818-935)、`ProjectHookAssignments`(937-1142)、`ProjectSkillAssignments`(1144-1261) 同构；`Promise.all([invalidate...])` 在 `154-158、177-181、857-860、981-982、1183-1186` 五处集合不一致。
- **E10 Dialog 壳** `role="dialog"` 14 文件手写；遮罩 `bg-slate-950/40` ×11 与 `bg-black/40` ×4（`mcp-import-dialog.tsx:50`、`hook-import-dialog.tsx:50`、`hook-assignment-picker-dialog.tsx:64`、`project-hook-picker-dialog.tsx:68`）分叉；`src/components/ui/` 仅 `button.tsx`；`form-dialog.tsx:41-120` 可作基础。
- **E11 小组件** `function Field` 3 份（`mcp-page.tsx:1017`、`hooks-page.tsx:974`、`provider-panel.tsx:643`）；图标切换按钮 3 份（`platform-assignment-button.tsx:27-48`、`hooks-page.tsx:877-913`、`project-detail-page.tsx:545-581`）。
- **E12 主题 token** `styles.css:7-22` 仅中性色；`text-red-700 dark:text-red-300` 26 次、`text-amber-800 dark:text-amber-300` 24 次；amber 提示框全串在 `blocking-state.tsx:19`、`sync-status-badge.tsx:66`、`project-detail-page.tsx:1451`、`change-preview-dialog.tsx:72`、`prompts-page.tsx:486`、`provider-panel.tsx:383`、`onboarding-wizard.tsx:540`。
- **E13 前后端双份元数据** `tool-metadata.ts:90-110` `PROFILE_TOOLS/MCP_TOOLS/SKILL_TOOLS` 三个相同数组；`HOOK_TOOLS`(111-116) 与 `capabilities.hooks` 重复；`hook-events.ts:74-129 hookEventSupportedByTool` 手抄 `domain/mod.rs:135 supported_for_tool`；`bindings/commands.ts:705` 常量段为空。
- **E14 测试基建** 13 个测试文件各自 `new QueryClient(...)`（`app-shell.test.tsx:49`、`dashboard-page.test.tsx:68` 少 `mutations`）、13 个 `renderPage/renderShell/renderWizard`、手工 `vi.mock("@/bindings/commands")` 列表（skills 19、mcp 18、detail 20）、40 行 `PreviewPlan` 夹具 9 份、12 份 `eslint-disable unbound-method` 头；`src/test/setup.ts` 1 行；`skills-page.test.tsx` 2440 行。
- **E15 死代码/归属** `.gitkeep`：`src/hooks/`、`src/features/sync/`、`src/features/providers/` 空目录；`src/features/{projects,mcp,prompts,skills}/.gitkeep` 与真实文件并存。`src/lib/app-info.ts appInfoQueryOptions` 无引用；`profile-api.ts` 同时承载 `unwrapResult/ProfileRpcError/profileErrorText` 与 profiles 查询；`hook-import-dialog.tsx:210 hookImportQueryOptions` 应移到 `hooks-api.ts`。

## F. 工程配置

- **F1** 无 push/PR CI；`package.json` 已有 `check` 脚本（format:check、lint、typecheck、test、rust:check）与 `bindings:check`。
- **F2** `src-tauri/Cargo.toml:5 description`、`tauri.conf.json shortDescription` 仍为"Claude 与 Codex"。
- **F3** `reqwest = "0.11.27"`（hyper 0.14 / rustls 0.21）；`serde_yaml = "0.9"` 已归档。
- **F4** `hooks/import.rs` 318 行 0 测试。
