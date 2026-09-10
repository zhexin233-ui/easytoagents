# 实施计划

- [ ] 加载后端规范与代码复用思考指南；确认 perf 与 robustness 任务已归档。
- [ ] R7-a：所有大文件内联测试外置为 `tests.rs`，纯移动，`cargo test` 用例数不变。
- [ ] R1：`Tool::ALL`、`adapter_for` 单例、descriptor 字段化、`ToolAvailability` 映射化、字符串解析统一、`projects/service.rs` 迭代化；`pnpm bindings:generate` 并核对差异。
- [ ] R3：`TargetDescriptor::builder`，五个 adapter 迁移；`path_text` 上移。
- [ ] R2：`sync/managed.rs` 与三个 service 的泛型替换；共享 DTO 提升；绑定核对。
- [ ] R4：`ProviderCodec`；`profiles/service.rs` 改写。
- [ ] R5：`skills::limits`。
- [ ] R6：`connection()` 收窄、`db/sync.rs`、`with_immediate_transaction`、`active_writer` 合一。
- [ ] R7-b：`sync/apply/` 目录化与 `ApplyContext`；`profiles/service.rs` 拆分；行数校验。
- [ ] 更新 `docs/maintainers/adding-tool-adapter.md`、`.trellis/spec/backend/directory-structure.md`、`database-guidelines.md`。
- [ ] `pnpm bindings:check`、`pnpm rust:check`、`pnpm check`。

## 风险文件
`domain/mod.rs`、`adapters/**`、`sync/**`、`db/**`、`mcp/service.rs`、`skills/service.rs`、`hooks/service.rs`、`profiles/service.rs`、`projects/**`、`overview/mod.rs`、`lib.rs`。

## 回滚点
R7-a、R1+R3、R2+R4+R5、R6、R7-b 五段提交。
