# 实施计划：应用方式全局配置

## 执行顺序

### Step 1：后端设置存储与命令

- [ ] 新增 `src-tauri/src/db/migrations/0007_app_settings.sql`，注册到 `db/mod.rs` `MIGRATIONS`（version 7）。
- [ ] 新增 `src-tauri/src/settings.rs`：`ApplyMode`、`AppSettingsDto`、`UpdateAppSettingsInput`、
      `APPLY_MODE_KEY`、`load_app_settings`、`save_app_settings` 及 `#[cfg(test)]` 测试
      （默认值 / roundtrip / reopen 持久化 / 未知值 DatabaseError）。
- [ ] 新增 `src-tauri/src/commands/settings.rs`：`get_app_settings` / `update_app_settings`，
      锁失败映射 `WRITE_IN_PROGRESS`。
- [ ] `src-tauri/src/lib.rs`：`pub mod settings;`、`.typ::<…>` 三项、`collect_commands!` 追加两条命令。
- [ ] 验证：`cargo fmt --manifest-path src-tauri/Cargo.toml`、
      `cargo test --manifest-path src-tauri/Cargo.toml settings::`、
      `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`。

### Step 2：重新生成绑定

- [ ] `pnpm bindings:generate`，确认 `src/bindings/commands.ts` 出现 `getAppSettings` / `updateAppSettings` /
      `AppSettingsDto` / `ApplyMode`。
- [ ] 验证：`pnpm bindings:check`。

### Step 3：设置 API 与设置页

- [ ] 新增 `src/lib/settings-api.ts`：`settingsKeys`、`appSettingsQueryOptions`、`canAutoApplyPreview`。
- [ ] 新增 `src/features/settings/settings-page.tsx`（应用方式卡片 + 勾选框 + 行为说明 + Profiles 不适用说明）。
- [ ] `src/app/router.tsx` 增加 `/settings` 路由；`src/app/app-shell.tsx` `primaryLinks` 增加「设置」。
- [ ] 验证：`pnpm typecheck`。

### Step 4：四条流程接入直接应用

- [ ] `src/features/mcp/mcp-page.tsx`：读取设置；`previewMutation.onSuccess` 无冲突时自动
      `applyMutation.mutate`；按钮文案按 `directApply` 切换。
- [ ] `src/features/skills/skills-page.tsx`：同上。
- [ ] `src/features/projects/project-detail-page.tsx`：父组件 `handlePreview` 统一决策；
      `AssignmentCard` 增加 `directApply` prop 与按钮文案切换。
- [ ] 冲突回退路径自检：`canAutoApplyPreview` 为 false 时一律 `setOpenPreview`。
- [ ] 验证：`pnpm lint && pnpm typecheck && pnpm test --run`。

### Step 5：全量质量门

- [ ] `pnpm format`（统一格式后）再 `pnpm check`，确保 format:check / lint / typecheck / vitest /
      cargo fmt + clippy + test / bindings check 全绿。

### Step 6：收尾

- [ ] Phase 3.3 spec 更新：若 `app_settings` / 应用方式引入了值得沉淀的新约定，写入
      `.trellis/spec/`（backend 数据库指南补键值表模式或 frontend 状态指南补设置读取模式，按实际需要）。
- [ ] 提交（Phase 3.4）：单提交包含迁移、后端、前端与绑定再生成。

## 回滚点

- Step 1 后端独立可回滚（新增文件 + MIGRATIONS 一行 + lib.rs 注册）。
- Step 3/4 纯前端，回滚不影响数据库。
- 数据库无破坏性变更：任何阶段回滚后遗留 `app_settings` 空表无害。

## 验证命令汇总

```bash
pnpm bindings:generate && pnpm bindings:check
pnpm check
```
