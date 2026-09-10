# 修复后端审阅缺陷

父任务：`09-10-codebase-optimization`。覆盖审阅条目 A4、A5、A6、A9、A10、A11、A12。

## 目标
修复已核实的后端正确性缺陷，不改变任何原生写入合同。

## 需求
- R1 / A4：`tool_probe.rs resolve_executable` 不再因 PATH 中任一不安全条目整体判定 `Unsupported`；跳过不安全条目与同名非文件条目继续搜索，仅当最终命中的候选本身不安全时才返回 `Unsupported`。探测结果附带可读原因（例如被跳过的条目数或不安全候选路径的脱敏形式），并透出到总览页的诊断信息。
- R2 / A5：数据库启动备份只在存在待执行迁移时进行；备份前执行 WAL checkpoint 使单文件可恢复；保留最近 3 份，超出的自动删除；备份目录权限与主库一致。
- R3 / A6：`reconcile_project_native_resources` 在一个 `IMMEDIATE` 事务内完成；从 `project_dto`（读路径）移出，改由 `register_project`、`rescan_project` 与项目资源预览前置步骤显式触发；`list_projects/get_project` 变为纯读。
- R4 / A9：`apply.rs` 以 `descriptor.path` 为键的映射改为不会互相覆盖的键（例如 `(tool, artifact, scope, path)` 或索引），`path == None` 的输入不再冲突。
- R5 / A10：`delete_snapshots` 先在事务内删除记录并提交，再删除文件；文件删除失败记录为可重试的清理项而非悬空行。
- R6 / A11：删除只写不读的 `interrupted_run` 缓存，或让 `get_interrupted_run` 真正读取它；二选一并写明理由。
- R7 / A12：`Database::open` 只调用一次 `configure_connection`。

## 验收条件
- A1：`tool_probe` 单测覆盖 PATH 含 `.`、含 `./node_modules/.bin`、含同名目录三种情况，工具仍被正确探测；候选本身为相对路径时仍 `Unsupported`。
- A2：`db` 单测：无待执行迁移时打开数据库不产生备份；有迁移时产生备份且旧备份被裁剪到 3 份。
- A3：`native_resources` 单测：reconcile 中途 SQL 失败后表内无半更新；`list_projects` 调用后 `project_native_resources` 表无变化。
- A4：A9、A10 各有一条回归测试。
- A5：`pnpm rust:check` 与 `pnpm check` 全绿。

## 范围外
命令异步化（`09-10-async-commands-and-probe`）、性能项（`09-10-backend-perf`）。
