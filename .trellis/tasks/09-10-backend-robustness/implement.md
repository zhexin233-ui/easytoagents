# 实施计划

- [x] 加载后端规范（error-handling、logging、database、quality）。
- [x] D1：统一 `AppState` 锁的中毒恢复路径，删除误导性中毒分支；补 panic 后可用测试。
- [x] D5：实现 `with_db`/`with_db_and_redactor`，并将命令层加锁样板收敛到统一 helper。
- [x] D2：增加 `AppError.source` 与构造器；逐模块替换 `map_err(|_|`；确认 specta 导出未变化（`pnpm bindings:check`）。
- [x] D2：完成 tracing 初始化、日志目录权限、脱敏、`rollback_failed` 诊断；补日志不含 secret 测试。
- [x] D3：完成 journal 枚举与无通配 match；补序列化一致与旧格式测试。
- [x] D4：增加 `parent_of` 并替换生产路径中的 `expect`/`unreachable!`。
- [x] D6：更新 `database-guidelines.md` 迁移策略；更新 `logging-guidelines.md`、`error-handling.md`。
- [x] `pnpm rust:check`、`pnpm bindings:check`、`pnpm check`。

## 风险文件
`app/mod.rs`、`error.rs`、`lib.rs`、`commands/*.rs`、`sync/apply.rs`、`sync/mod.rs`、`db/mod.rs`、`adapters/mod.rs`、`security/mod.rs`。

## 回滚点
六段独立提交，见 design.md。
