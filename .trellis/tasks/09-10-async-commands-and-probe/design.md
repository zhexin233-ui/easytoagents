# 技术设计

## 命令异步化策略
采用最小改动方案：为每个触碰状态/IO 的命令加 `#[tauri::command(async)]`，签名保持同步 `fn`，Tauri 会把它调度到线程池。tauri-specta 对 `async` 属性的命令生成同样的 `Promise<Result<...>>` 绑定，因此 `commands.ts` 预期无变化；若生成产物出现差异，在 `implement.md` 记录并核对语义。`import_github_skill` 保持 `async fn`，下载之后的 `prepare/finalize + DB` 段包进 `tauri::async_runtime::spawn_blocking`，`AppState` 通过 `Arc` 或 `tauri::State` 的 `'static` 克隆传入。

`Mutex<Database>` 在线程池中被持有不再阻塞 UI，但仍串行化写入，这是既有语义，保持不变。

## 启动探测
`AppState.environment` 改为 `RwLock<EnvironmentState>`，`enum EnvironmentState { Probing, Ready(ExplicitEnvironment) }`。`setup` 中：
1. 读取环境变量构造 `ReleaseToolProbeInput`（与现在相同）。
2. `app.manage(AppState::initialize_probing(paths)?)`，数据库初始化不依赖探测结果。
3. `tauri::async_runtime::spawn_blocking` 执行 `probe_release_environment`（内部改为 `std::thread::scope` 并行五个探测，各自超时），完成后写入 `Ready` 并 `app.emit("environment-ready", ())`。
4. `refresh_environment` 命令复用第 3 步逻辑，返回新的 `ToolAvailability`。

`AppState::environment()` 在 `Probing` 时返回 `AppError` 稳定码 `ENVIRONMENT_PROBING`；`commands/mod.rs get_app_info` ���新增 `get_environment_state` 让前端判断状态。前端 `app-shell.tsx` 监听 `environment-ready` 事件（`@tauri-apps/api/event`）后 invalidate `dashboard`、`toolProfileStatus`、`settings` 等 key；测试中 mock `listen`。

## 代理注入
`AppState` 增加 `github_proxy: Option<ProxyConfig>`，在 `setup` 中用现有 `environment_proxy()` 逻辑读取一次；`download_github_skill(input, proxy: Option<&ProxyConfig>)`；`import_github_skill` 从 state 取出后传入。测试用参数直接构造。

## 兼容与风险
- 事件监听要在 `AppShell` 挂载时注册并在卸载时取消。
- Tauri 单实例插件的 `show/set_focus` 不受影响。
- 若某命令在主线程有隐式依赖（例如访问 window），需单独审查；预期只有 `get_app_info` 之类纯读命令，可保持同步。

## 回滚
`#[tauri::command(async)]` 属性可批量移除；探测状态机与事件独立于其他改动。
