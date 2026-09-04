# Implement: 启用的工具全局设置

按顺序执行；每个验证点通过后再前进。回滚点 = 每个 step 一个 commit 粒度
（实际提交在 Phase 3.4 统一处理，这里以「可独立 revert 的改动集」为准）。

## Step 1: 后端设置字段

- [ ] `src-tauri/src/settings.rs`：
  - `ENABLED_TOOLS_KEY` 常量；`AppSettingsDto` / `UpdateAppSettingsInput`
    增加 `enabled_tools: Vec<Tool>`；默认 `vec![Claude, Codex]`。
  - `load_app_settings` 读 JSON 数组，缺 key → 默认；解析失败 / 未知值 →
    `DatabaseError`。
  - `save_app_settings` 同事务 UPSERT 两个 key。
  - 更新既有测试构造 + 新增：默认、round-trip、重开保持、非法值报错。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml settings`

## Step 2: 绑定再生成

- [ ] `pnpm bindings:generate`，检查 `src/bindings/commands.ts` 中
  `AppSettingsDto` / `UpdateAppSettingsInput` 含 `enabledTools: Tool[]`。
- 验证：`pnpm bindings:check`

## Step 3: 前端共享设施

- [ ] `src/lib/tool-metadata.ts`：`DEFAULT_ENABLED_TOOLS` +
  `filterEnabledTools()`。
- [ ] `src/components/use-enabled-tools.ts`：`useEnabledTools()` hook
  （settings 查询，缺数据回落默认集）。
- 验证：`pnpm typecheck`

## Step 4: 设置对话框

- [ ] 「启用的工具」区块（三个带图标 checkbox + 说明文案），toggle 整包提交
  `{ applyMode, enabledTools }`。
- [ ] `settings-dialog.test.tsx`：默认态渲染、切换提交参数断言。
- 验证：`pnpm test --run settings-dialog`

## Step 5: 渲染点过滤

- [ ] app-shell.tsx（TopBar 过滤 PROFILE_TOOLS）+ app-shell.test 断言。
- [ ] prompts-page.tsx（tools 过滤 + 空集隐藏状态区块）+ 测试 mock/断言。
- [ ] mcp-page.tsx（图标列 + 状态卡过滤）+ 测试。
- [ ] skills-page.tsx（同上）+ 测试。
- [ ] project-detail-page.tsx（visibleTools / activeTool 派生夹逼 +
  project.targets 过滤）+ 测试（含回落断言）。
- [ ] dashboard-page.tsx（data.tools 过滤）+ 测试。
- [ ] onboarding-wizard.tsx（tools 过滤；已有测试补 settings mock 如需）。
- [ ] provider-panel.tsx（目标工具关闭时隐藏复制入口）+ 测试如适用。
- [ ] 所有现存 `getAppSettings` mock 补 `enabledTools` 字段。
- 验证：`pnpm test --run && pnpm lint && pnpm typecheck`

## Step 6: 全量质量门

- [ ] `pnpm check`（format:check / lint / typecheck / vitest / cargo fmt +
  clippy + test）全绿。

## Review gates

- Step 2 后人工核对生成绑定 diff 仅含本字段。
- Step 5 后跑一次 `pnpm dev` 走查：设置开关 → 顶栏/列表/详情即时增减。

## 回滚点

- 任一步失败且无法快速修复：`git checkout -- <files>` 回到上一 step 状态；
  后端 + 绑定 + 前端为同一条功能链，不建议部分回滚上线。
