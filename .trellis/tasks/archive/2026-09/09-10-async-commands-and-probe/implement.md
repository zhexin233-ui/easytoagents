# 实施计划

- [x] 加载后端规范与前端状态管理规范；核对 tauri-specta 对 `#[tauri::command(async)]` 的绑定输出（生成的 TS 与同步命令完全一致，仅新增命令与错误码）。
- [x] 为 `commands/*.rs` 中全部命令加 `#[tauri::command(async)]`（含 `get_app_info`，不设例外，避免维护白名单）。
- [x] `import_github_skill` 的同步段包进 `spawn_blocking`（通过 `AppState::database_handle()` + 克隆 `AppPaths` 移入闭包）。
- [x] `AppState.environment` 改为 `RwLock<Option<Arc<ExplicitEnvironment>>>`；`environment()` 在探测中返回 `ENVIRONMENT_PROBING`。
- [x] `probe_release_environment` 改为 `std::thread::scope` 并行；`setup` 先 `manage` 再后台 `spawn_blocking` 探测，完成后通过 `AppState` 通知回调发 `environment-ready` 事件。
- [x] 新增 `refresh_environment` / `get_environment_state` 命令并注册；`pnpm bindings:generate`。
- [x] 前端：`AppShell` 订阅事件失效依赖环境的查询家族；总览页头部与设置对话框接入 `RefreshEnvironmentButton`；`QueryClient` 默认对 `ENVIRONMENT_PROBING` 重试使查询保持 pending（页面既有 `role="status"` 等待态即可）；补测试。
- [x] 代理注入：`AppState.github_proxy`、`download_github_skill(url, proxy)` 参数化、删除 `skills/github.rs` 的运行时 env 读取（`environment_proxy` 移到 `lib.rs` setup）。
- [x] `commands::tests::every_command_runs_off_the_main_thread` 扫描全部命令源文件。
- [x] `pnpm bindings:check`、`pnpm check`。
- [ ] 真实 Tauri 应用手工验证启动时序与刷新（A3）：**本会话为无头环境，未执行**；验证步骤：把 PATH 指向一个 `sleep 3` 的假 `claude`，启动应用，主窗口应在 1 秒内出现，总览工具卡随 `environment-ready` 自动更新。
- [x] 更新 `.trellis/spec/backend/quality-guidelines.md`（命令线程模型场景）、`error-handling.md`、`directory-structure.md`、`skill-import-guidelines.md` 与 `.trellis/spec/frontend/state-management.md`。

## 与设计的偏差

- 设计里 `refresh_environment` 用 `AppHandle` 发事件；`collect_commands!` 在泛型 `create_command_builder<R>` 内无法引用运行时泛型命令，改为 `AppState` 持有 `environment_ready: Box<dyn Fn(bool) + Send + Sync>` 回调，setup 里捕获 `AppHandle` 构造回调。命令只依赖 `State`，测试用的 `AppState::initialize` 无回调也可用。
- 探测状态 DTO 直接由 `get_environment_state` 返回（`EnvironmentStateDto { probing, tools[] }`），未改动 `get_app_info`。
- 空状态（`needsOnboarding`）总览页不渲染"重新检测工具"，维持"唯一下一步是首次检测"的既有合同；设置对话框始终提供该入口。

## 风险文件
`src-tauri/src/lib.rs`、`app/mod.rs`、`app/tool_probe.rs`、`commands/*.rs`、`skills/github.rs`、`src/app/app-shell.tsx`、`src/features/dashboard/dashboard-page.tsx`、`src/features/settings/settings-dialog.tsx`。

## 回滚点
命令属性、探测状态机、代理注入三段分别提交。
