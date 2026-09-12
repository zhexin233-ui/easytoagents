# 修复渠道导入基线冲突与工具切换状态残留

## Goal

修复渠道配置页两个缺陷：

1. 删除某工具全部渠道档案后，再次「检测已有配置」→「确认无写入接管」被
   `CONFLICT：该原生目标已经建立受管基线` 阻断，无法重新导入原生渠道。
2. 在 Claude / Codex / Zcode / OpenCode / Cursor 等工具页签间切换时，
   上一工具的导入预览、错误提示、表单与同步预览对话框等界面状态残留，
   显示页应整体重置。

## Background

- 调研结论见 `research/root-cause.md`。
- Bug 1 根因：`delete_profile_row` 不清理 `managed_targets` 基线，
  `adopt_baseline`（`src-tauri/src/db/profiles.rs:889`）对孤儿基线直接报冲突。
- Bug 2 根因：`src/app/router.tsx` 各工具路由渲染同一组件类型且位置相同，
  React 复用实例导致 `ProviderPanel` 等局部 state 跨工具残留。

## Requirements

### R1 孤儿基线可重新接管（Bug 1）

- `confirm_provider_import` 的事务里已经通过 `reject_existing_profiles` 保证
  该工具无任何中央档案；此时若 `managed_targets` 已有带基线的行（孤儿基线），
  导入必须成功，并把基线刷新为本次观测到的原生内容（full hash、managed hash、
  projection、last_status='in_sync'）。
- 基线行的 `row_version` 会随历史同步递增；导入在确认时重读当前 `row_version`
  并以它作为同事务 UPDATE 的守卫（`updated != 1` 报既有冲突文案，作为
  fail-closed 防御分支），不得依赖固定版本值。
- 不改变「有中央档案时禁止再次导入」的既有约束（`reject_existing_profiles` 保持）。
- 导入成功的用户可见文案保持现状（「已有渠道已无写入接管，原生文件内容保持不变」）。

### R2 工具切换时显示页重置（Bug 2）

- 切换工具路由（如 `/claude` → `/codex` 或 `/zcode`）时，页面所有局部状态
  （导入预览卡片、mutation 错误提示、新增/编辑表单、同步预览对话框）必须重置，
  不得出现上一个工具的数据或文案。
- 不改变同一路由内的既有交互（如表单打开时切换不弹确认等，保持现状语义即可）。

## Acceptance Criteria

- [ ] 后端回归测试：先建立基线（导入或同步）→ 删除该工具全部渠道档案 →
      再次「确认无写入接管」成功返回新档案；`managed_targets` 基线被刷新为
      本次观测值；基线行 row_version 历史递增后重导入仍成功且版本继续递增。
- [ ] 后端既有约束保持：存在任意中央档案时导入仍报既有冲突
      （`首次导入仅在该工具尚无中央档案时可确认`）。
- [ ] 前端回归测试：在 Claude 页签触发导入预览（或 mutation 错误）后切换到
      codex / zcode 页签，断言导入预览卡片、错误提示、表单内容均不存在。
- [ ] `pnpm lint`、`pnpm typecheck`、前端相关测试、`cargo test`（profiles 相关）
      全部通过。
- [ ] 手动验收路径：删除全部 Claude 渠道 → 检测已有配置 → 确认无写入接管 →
      成功提示且渠道列表出现「已导入渠道」；切换到 codex/zcode 页签无残留状态。

## Constraints

- 后端为 Rust + SQLite，遵循 `.trellis/spec/backend/` 规范（本任务不改 schema、
  无需新增迁移）。
- 前端遵循 `.trellis/spec/frontend/` 规范；修复尽量小而聚焦，不重构路由结构。
- 错误文案沿用现有 `CONFLICT：…` 呈现机制，不新增错误码。
