# 执行计划

1. [ ] 迁移 0009 + db/mod.rs 注册 + 迁移测试（种子映射、central 金丝雀、唯一索引）
2. [ ] db/profiles.rs：Record/row/查询/`set_global_prompt_assignment`/`set_prompt_project_assignment` 守卫改造
3. [ ] profiles/service.rs：list/create/set_global_prompt_assignment/删除 set_active/discover 守卫/confirm 自动启用/DTO/project 守卫
4. [ ] commands/profiles.rs + lib.rs 注册 + `pnpm bindings:generate`
5. [ ] overview/onboarding 适配
6. [ ] profile-api + prompts-page/prompt-panel 重建
7. [ ] project-detail-page 适配
8. [ ] 后端测试重写（service/db）；前端测试重写（prompts-page、project-detail、onboarding 如有）
9. [ ] `pnpm check` + `cargo test` 全绿
10. [ ] 运行中的应用目测（dev 热重载）；恢复测试造成的 UI 状态
11. [ ] spec 沉淀（per-tool 启用位迁移模式 + 工具无关档案合同）→ 提交 → 归档 → journal

验证命令：`pnpm check`；`cargo test --manifest-path src-tauri/Cargo.toml`
回滚点：每步独立可编译；迁移失败回滚由事务保证。
