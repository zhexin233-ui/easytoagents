# 执行计划

## 顺序清单

1. [ ] `skills/library.rs`
   - `prepare_skill_import_budgeted`：central 预检移到 name 解析后，`central_path = central_skills()/name`。
   - `finalize_skill_import` / `verify_prepared_import_budgeted` central 分支：`validate_direct_child` 用 `prepared.name`。
   - `inspect_central_skill` 增加 `name: &str` 参数，目录名接受 id 或 name；更新 `quarantine_central_skill` 签名。
   - 新增 `migrate_legacy_central_skill_directories(database, paths)`。
2. [ ] `skills/service.rs`：`inspect_record`、`preview_skill_content`、`delete_skill` 传 `record.name`。
3. [ ] `skills/import.rs`：`private_record` 检查改为 `central_skills()/join(&record.name)`（配合 inspect 双布局，
     该检查仅用于识别「已知中央记录」，需与迁移后布局一致；保留对 legacy 布局的兼容判断）。
4. [ ] `app/mod.rs`：`initialize_internal` 在 `Database::open` 后调用迁移。
5. [ ] 测试：更新/新增（见 design.md 测试计划）。

## 验证命令

```bash
cd src-tauri && cargo fmt --all -- --check && cargo test
```

## 回滚点

- 单一 commit；回滚即 revert。
