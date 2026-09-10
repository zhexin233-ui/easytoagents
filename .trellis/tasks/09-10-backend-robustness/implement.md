# 实施计划

- [ ] 加载后端规范（error-handling、logging、database、quality）。
- [ ] D1：引入 parking_lot，替换 `AppState` 锁，删除中毒分支；补 panic 后可用测试。
- [ ] D5：`with_db`/`with_db_and_redactor` 与统一 `state_lock_error`；批量改写命令。
- [ ] D2：`AppError.source` 与构造器；逐模块替换 `map_err(|_|`；确认 specta 导出未变化（`pnpm bindings:check`）。
- [ ] D2：tracing 初始化、日志目录权限、脱敏、`rollback_failed` 诊断；补日志不含 secret 测试。
- [ ] D3：journal 枚举与无通配 match；补序列化一致与旧格式测试。
- [ ] D4：`parent_of` 与全部 `expect`/`unreachable!` 替换。
- [ ] D6：更新 `database-guidelines.md` 迁移策略；更新 `logging-guidelines.md`、`error-handling.md`。
- [ ] `pnpm rust:check`、`pnpm bindings:check`、`pnpm check`。

## 风险文件
`app/mod.rs`、`error.rs`、`lib.rs`、`commands/*.rs`、`sync/apply.rs`、`sync/mod.rs`、`db/mod.rs`、`adapters/mod.rs`、`security/mod.rs`。

## 回滚点
六段独立提交，见 design.md。
