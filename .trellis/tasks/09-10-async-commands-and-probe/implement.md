# 实施计划

- [ ] 加载后端规范与前端状态管理规范；核对 tauri-specta 对 `#[tauri::command(async)]` 的绑定输出。
- [ ] 为 `commands/*.rs` 中全部 IO/状态命令加 `#[tauri::command(async)]`；纯读命令记录例外理由。
- [ ] `import_github_skill` 的同步段包进 `spawn_blocking`。
- [ ] `AppState.environment` 改为 `RwLock<EnvironmentState>`；`environment()` 在探测中返回 `ENVIRONMENT_PROBING`。
- [ ] `probe_release_environment` 改为并行；`setup` 改为后台探测 + `environment-ready` 事件。
- [ ] 新增 `refresh_environment` 命令并注册到 `collect_commands!`；`pnpm bindings:generate`。
- [ ] 前端：监听事件刷新查询；总览与设置增加"重新检测工具"；探测中状态渲染 `role="status"`；补测试。
- [ ] 代理注入：`AppState.github_proxy`、`download_github_skill` 参数化、删除运行时 env 读取，补单测。
- [ ] 写一条测试断言 `commands/` 下没有裸同步 IO 命令。
- [ ] `pnpm bindings:check`、`pnpm check`；真实 Tauri 应用手工验证启动时序与刷新。
- [ ] 更新 `.trellis/spec/backend/quality-guidelines.md`（命令线程模型）与 `directory-structure.md`（状态机）。

## 风险文件
`src-tauri/src/lib.rs`、`app/mod.rs`、`app/tool_probe.rs`、`commands/*.rs`、`skills/github.rs`、`src/app/app-shell.tsx`、`src/features/dashboard/dashboard-page.tsx`、`src/features/settings/settings-dialog.tsx`。

## 回滚点
命令属性、探测状态机、代理注入三段分别提交。
