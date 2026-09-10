# 实施计划

- [ ] 加载后端规范（database、error-handling、quality）。
- [ ] A4：改写 `resolve_executable`，补三类 PATH 单测与诊断码文案；核对 `overview` 与前端诊断码映射。
- [ ] A5：调整 `Database::open` 备份判定，增加 checkpoint 与裁剪，补单测。
- [ ] A6：reconcile 事务化并移出 `project_dto`；核对所有调用点，补"读命令不写库"测试。
- [ ] A9、A10、A11、A12：逐项修改并各补一条回归测试。
- [ ] 运行 `pnpm rust:check`、`pnpm check`、`pnpm bindings:check`。
- [ ] 更新 `.trellis/spec/backend/database-guidelines.md`（备份策略）与 `quality-guidelines.md`（读命令不写库）。

## 风险文件
`app/tool_probe.rs`、`db/mod.rs`、`projects/native_resources.rs`、`projects/service.rs`、`sync/apply.rs`。

## 回滚点
每个编号独立提交，可单独 revert。
