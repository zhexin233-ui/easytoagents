# 实施计划

- [ ] 加载后端规范；通读 `sync/apply.rs` 的 journal、snapshot、rollback 段与全部故障注入测试。
- [ ] C5：删除 `canonical_json`，补有序测试（最小改动先行验证测试基线）。
- [ ] C3：审计调用去重，补调用次数断言。
- [ ] C4：聚合查询、`inspect_record` 拆分、`prepare_cached`，补 SQL 计数测试。
- [ ] C2：`PathState` 传递与廉价复核；预检一次化 + `data_version`；补读取次数测试。
- [ ] C1：journal JSONL 格式、旧格式兼容、fsync 计数测试、旧 journal 恢复测试。
- [ ] C6：GitHub 并发下载与写盘，补测试。
- [ ] `pnpm rust:check`、`pnpm check`；真实 Tauri 应用做一次多目标 apply 与恢复。
- [ ] 更新 `.trellis/spec/backend/quality-guidelines.md`（journal 格式与 fsync 预算）、`database-guidelines.md`（聚合查询与 prepare_cached）。

## 风险文件
`sync/apply.rs`、`sync/mod.rs`、`security/mod.rs`、`db/{mod,skills,mcp,hooks}.rs`、`skills/{service,library,github}.rs`、`app/mod.rs`。

## 回滚点
C5/C3/C4 各自独立提交；C2 与 C1 分别提交，C1 必须最后合入。
