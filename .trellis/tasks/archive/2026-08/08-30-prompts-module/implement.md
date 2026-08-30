# 实施计划：提示词模块（prompts）

> 按序执行；每步验证通过再前进；各步独立可提交（回滚点）。前置认知见 design.md。

## Step 1. 数据库迁移

- [x] `0008_prompt_project_assignments.sql`：新表 `prompt_project_assignments`（PK (project_id, tool)，FK projects/prompt_profiles，row_version 触发器对齐 `prompt_profiles` 模式）。
- [x] 重建 `managed_targets`：CHECK 放开 project 作用域含 `'prompt'`；12 步重建 + 事务 + 迁移测试（旧行原样保留）。
- [x] `db/profiles.rs` 增项目分配函数族（get/set/list + 基线读写按 scope/project_id）。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml db`

## Step 2. 适配器与服务泛化

- [x] `adapters/claude/mod.rs`：项目级 Prompt 描述符（`<root>/CLAUDE.md`）。
- [x] `adapters/codex/mod.rs`：项目级 Prompt 描述符（`<root>/AGENTS.md`，trust 策略对齐项目级 mcp）。
- [x] `profiles/service.rs`：`PromptSyncTarget`（Global | Project）；`prepare_prompt_sync` 双形态；`persist_prepared_preview` 参数化 scope/project；项目态「本地已修改」→ 覆盖预览项（readopt 通道，参照 mcp）；全局严格语义不动。
- [x] 解除分配语义：删分配行 + 清基线，不删项目文件。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml profiles`

## Step 3. 命令与 bindings

- [x] `preview_prompt_sync` 增 `projectId`；`ApplyProfilePreviewInput` 增 `projectId`；新增 `set_prompt_project_assignment` / `get_prompt_project_assignment`。
- [x] `lib.rs` 注册；`pnpm bindings:generate`；`pnpm bindings:check` 通过。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml`（含 command_smoke、bindings）

## Step 4. Rust 测试

- [x] service 测试：项目分配→应用写根文件、外部修改→本地已修改→覆盖、解除分配保留文件、全局回归不变、迁移断言。
- [x] e2e 追加项目提示词链路；非受管内容不被触碰断言。
- 验证：`cargo fmt --check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`

## Step 5. 前端新模块

- [x] `src/features/prompts/prompts-page.tsx` + 测试：工具页签 + 档案库（新建/编辑/删除/生效/接管导入，UI 自 prompt-panel 迁移）+ 全局同步预览/直接应用 + 目标状态。
- [x] `router.tsx` 加 `/prompts`；`app-shell.tsx` primaryLinks 加「提示词」。
- [x] `profile-api.ts` 增项目维度 query/keys。
- 验证：`pnpm typecheck && pnpm lint`

## Step 6. 前端收纳与项目页签

- [x] `tool-profiles-page.tsx` 移除 PromptPanel 与 prompt 分支；测试同步裁剪。
- [x] `project-detail-page.tsx`：`ProjectResourceView` 加 `"prompt"`；`ProjectPromptAssignments`（选择档案→预览→应用带 projectId；解除分配文案；blocked 闸门）；apply 分支按 artifactKind。
- [x] onboarding / dashboard 调用处适配新命令签名。
- 验证：`pnpm typecheck && pnpm lint && pnpm test --run`

## Step 7. 全量质量门

- [x] `pnpm check`（format:check + lint + typecheck + vitest + rust:check + bindings:check）。

## Step 8. 收尾

- [x] spec 更新（Phase 3.3）：新增/补充「提示词项目级分配」合同（目标路径、硬拷贝语义、漂移覆盖、解除分配保留文件、managed_targets CHECK 变更）。
- [x] 提交（Phase 3.4）。

## 风险文件 / 回滚点

| 文件 | 风险 | 缓解 / 回滚 |
|---|---|---|
| 迁移 0008（managed_targets 重建） | 触及既有基线存储 | 重建断言 + 独立 commit；出问题 revert 迁移提交并恢复 DB 备份（应用已有 database-backups 机制） |
| `profiles/service.rs`（双形态 sync） | 全局路径回归 | 全局分支行为冻结 + 既有测试全保留 |
| `tool-profiles-page.tsx` 瘦身 | 渠道页误伤 | 渠道面板用例全保留 |
| bindings | 合同漂移 | `bindings:check` 门禁 |
