# Tauri 命令异步化与启动探测并行

父任务：`09-10-codebase-optimization`。覆盖审阅条目 B1、B2、B3。

## 目标
让所有涉及文件系统、子进程、数据库或网络的命令脱离 Tauri 主线程；主窗口在工具探测完成前即可显示；探测结果可刷新；代理配置走显式注入。

## 需求
- R1 / B1：全部 87 个命令中，凡触碰 `Mutex<Database>`、文件系统、子进程或网络的，都不再在主线程执行。允许的实现：`#[tauri::command(async)]` 保持同步签名，或改为 `async fn` 并把阻塞体放进 `tauri::async_runtime::spawn_blocking`。`import_github_skill` 在 `.await` 后的同步文件与数据库操作也放进 `spawn_blocking`。
- R2 / B1：命令的输入输出 DTO、错误码、生成的 TypeScript 绑定签名不变；`pnpm bindings:check` 通过且 `src/bindings/commands.ts` 无 diff（或仅 Promise 语义相关的无害 diff，需在实施记录中说明）。
- R3 / B2：`setup` 不再同步等待五个工具探测；主窗口先以"探测中"的 `ToolAvailability` 状态显示，探测在后台线程并行执行，完成后通过 Tauri 事件通知前端刷新总览与工具状态查询。探测本身改为并行（`std::thread::scope` 或等价），单个工具超时不影响其他工具。
- R4 / B2：新增 `refresh_environment` 命令重新探测并更新 `AppState.environment`；前端在总览页提供"重新检测工具"入口，并在设置对话框中同样可触发。
- R5 / B3：GitHub 代理配置在 `setup` 中一次性从环境读取并存入 `AppState`，`download_github_skill` 通过参数接收代理设置，不再在下载时读进程环境；测试可通过参数注入代理。
- R6：探测未完成期间，依赖 `environment` 的命令返回明确的"探测中"错误码（稳定 `&'static str` 文案），前端对应查询显示 `role="status"` 的等待态而非错误态。

## 验收条件
- A1：`rg "pub fn" src-tauri/src/commands` 命中的每个命令要么带 `#[tauri::command(async)]`，要么为 `async fn`；用一条 Rust 测试或脚本断言列表完整。
- A2：`pnpm bindings:check` 通过；前端测试全绿且无需修改 mock 签名。
- A3：手工验证：将 PATH 指向一个会阻塞 3 秒的假 `claude` 可执行文件，启动应用主窗口在 1 秒内出现，探测完成后总览页状态自动更新。
- A4：`refresh_environment` 有单测；前端有"重新检测"按钮的交互测试。
- A5：`environment_proxy` 不再存在于 `skills/github.rs`；代理注入有单测。
- A6：`pnpm check` 全绿。

## 范围外
数据库锁类型替换（`09-10-backend-robustness`）、fsync 与 IO 次数优化（`09-10-backend-perf`）。
