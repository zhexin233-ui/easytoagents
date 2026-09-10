# 后端去重与大文件拆分

父任务：`09-10-codebase-optimization`。覆盖审阅条目 E1、E2、E3、E4、E5、E6、E7。

## 目标
把 Tool 分派、受管条目流程、descriptor 构造、限额常量各收敛为一份实现，SQL 回到 db 层，并把万行级文件按职责拆分、测试外置。行为与 RPC 合同不变。

## 需求
- R1 / E1：`Tool::ALL` 常量与 `Tool::adapter(self) -> &'static dyn ToolAdapter`；删除 7 份 `tool_adapter/adapter_for`；`allowed_root`、`mcp_container` 成为 `TargetDescriptor` 字段由 Adapter 在 `discover` 时填写，删除 5 份 `allowed_root` 与 3 份 `native_mcp_container`，并统一 Claude 根目录口径（以 `claude_config_dir()` 为准，需确认 `mcp/service.rs` 用 `home()` 是否为有意设计，若是则在 descriptor 中显式区分）；`projection_value_at/json_value_at` 合并为一份；手写字符串→Tool 改用 `from_stable_str`；`projects/service.rs` 逐个点名改为 `tool_adapters()` 迭代；`ToolAvailability`/`ExplicitEnvironment` 改为按 `Tool` 索引的映射。
- R2 / E2：新建 `sync/managed.rs` 定义 `trait ManagedArtifact`，泛型化 `readopt_with_scan`、`collect_row_versions`、`list_global_target_statuses`、`descriptor_for`、`safe_row_version`、`project_dto` 与 `prepare_*_sync` 尾部；`*ProjectSelectionState`、`*ProjectDto`、`*TargetStatusDto` 提升为 `domain` 共享类型，前端绑定保留原类型名（用类型别名或 `#[specta(rename)]`），`src/bindings/commands.ts` 差异需在实施记录中逐条说明。
- R3 / E3：`TargetDescriptor::builder(tool, artifact, scope)` 链式构造器，五个 Adapter 全部改用；`path_text` 上移到 `adapters/mod.rs`。
- R4 / E4：`ToolAdapter` 增加 `fn provider_codec(&self) -> Option<&dyn ProviderCodec>`，把 `profiles/service.rs` 中 per-tool 的 ownership、default_options、discover、render 移入各 adapter 目录；`cursor_unsupported` 只剩 `None` 分支一处。
- R5 / E5：`skills/mod.rs` 提供 `pub(crate) mod limits`，`library.rs` 与 `github.rs` 引用。
- R6 / E6：`Database::connection()/connection_mut()` 收窄为 `pub(crate)`；`sync_runs/sync_items/snapshots/managed_targets` 的 SQL 收进 `db/sync.rs`；`Database::with_immediate_transaction(|tx| ...)`；`active_writer` 与 `reject_active_writer` 合一。
- R7 / E7：`sync/apply.rs` 拆为 `sync/apply/{mod,validate,plan,fs_ops,journal,snapshot,restore}.rs`，`RenameFaultContext` 5 元组改为 `struct ApplyContext`；`db/mod.rs`、`mcp/service.rs`、`skills/service.rs`、`profiles/service.rs`（拆 `provider_discovery.rs`、`prompt.rs`）、`native_resources.rs`、`tool_probe.rs` 的内联测试全部移到 `#[cfg(test)] mod tests;` 外置文件；任一生产源文件不超过 1500 行。

## 验收条件
- A1：`rg "fn tool_adapter|fn adapter_for|fn allowed_root|fn native_mcp_container|fn safe_row_version|fn descriptor_for" src-tauri/src` 每个名称至多一处定义。
- A2：`docs/maintainers/adding-tool-adapter.md` 更新后，新增工具清单不再包含"检索所有 match Tool"，改为列出固定的注册点（`Tool` 枚举、adapter 模块、db CHECK）。
- A3：`pnpm bindings:check` 通过；前端测试全绿。
- A4：`wc -l` 校验任一 `src-tauri/src/**/*.rs` 非测试文件不超过 1500 行。
- A5：`cargo test` 用例数不少于拆分前。
- A6：`pnpm rust:check`、`pnpm check` 全绿。

## 范围外
行为变更、新增能力。必须在 `09-10-backend-perf` 与 `09-10-backend-robustness` 之后执行，以避免在同一文件上冲突。
