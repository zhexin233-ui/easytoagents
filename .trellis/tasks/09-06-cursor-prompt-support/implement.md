# 执行计划：Cursor 提示词支持

> 每步完成后运行对应验证；Rust 测试命令默认 `cargo test`（pnpm rust:check 为 check 级）。

## A. Rust 后端

- [x] A1 `adapters/mod.rs`：新增 `TargetFormat::CursorMdc`（serde/as_str/expected_type）；
      `parse_document` 剥离 frontmatter；`render_document` 包装 `alwaysApply: true`；
      `project_document` WholeDocument 分支覆盖；`PROFILE_TOOLS` 加入 Cursor。
- [x] A2 `adapters/cursor/mod.rs`：global/project Prompt descriptor（`CursorMdc`、
      `["$document"]`、路径 `rules/easytoagents.mdc`）；更新
      `descriptor_matrix_only_supports_mcp_and_skills` 测试为含 prompt 的矩阵断言。
- [x] A3 `db/migrations/0017_cursor_prompt_support.sql`：三处 CHECK 原地修订 + 新列 + 部分唯一
      索引；在 `db/mod.rs` 迁移列表登记；补充迁移测试（从 v16 升级、锚点唯一命中、
      provider CHECK canary、cursor×prompt managed_targets canary、重开库）。
- [x] A4 `db/profiles.rs`：`is_active_cursor` 读写分支（set_global / deactivate /
      globally_enabled / find_active CASE / 三处守卫移除）；记录结构与 DTO 字段同步。
- [x] A5 `profiles/models.rs` + `profiles/service.rs`：`NewPromptProfileRecord`/
      Record/DTO 增加 `is_active_cursor`；`ensure_profile_capability` 收窄到 Provider；
      `confirm_prompt_import`/`create_prompt_profile` 填位；错误文案更新。
- [x] A6 `overview/mod.rs`：`active_prompt_name` 支持 cursor。
- [x] A7 服务级测试：Cursor 全局启用→应用→漂移→恢复；项目分配/解除；
      `.mdc` 导入剥离 frontmatter；「对该工具全局生效的档案不能分配到项目」守卫对 cursor 生效。
      验证：`cargo test`（src-tauri）。

## B. 绑定与前端

- [x] B1 `pnpm bindings:generate`；确认 `TargetFormat` 新变体进入 `src/bindings/commands.ts`
      且 `pnpm bindings:check` 通过。
- [x] B2 `tool-metadata.ts`：cursor 能力与 PROFILE_TOOLS 更新；`tool-metadata.test.ts` 断言更新。
- [x] B3 `router.tsx` 加 `/cursor`；`tool-profiles-page.tsx` fail-closed 条件改为
      「provider+prompt 均不支持」，Cursor 页面不渲染 ProviderPanel、不发 provider 查询。
- [x] B4 核对 `prompts-page` / `project-detail-page` / `dashboard-page` / `onboarding-wizard`
      在 cursor 加入后的假设与空态；前端相关测试更新。验证：`pnpm typecheck && pnpm test --run`。

## C. 文档与质量门

- [x] C1 `docs/maintainers/adding-tool-adapter.md`：非对称示例段落改为 Cursor Prompt
      Supported；能力矩阵表补 Cursor Prompt 行（官方 URL + 2026-09-06 核验日期）。
- [x] C2 全量质量门：`pnpm format:check`、`pnpm lint`、`pnpm typecheck`、`pnpm test --run`、
      `pnpm bindings:check`、`pnpm rust:check`、`pnpm check`、`git diff --check`。

## D. 收尾

- [ ] D1 trellis-check 全量检查通过后提交（feat: Cursor 支持全局与项目级提示词）。
- [ ] D2 `task.py archive` + journal 记录。

## 回滚点

- A 段任何一步失去官方证据支撑或引入无法收敛的设计冲突 → 停在当前步，回退该步补丁；
  迁移文件一旦应用不做倒迁，代码回滚后 `is_active_cursor` 列保持无害。
