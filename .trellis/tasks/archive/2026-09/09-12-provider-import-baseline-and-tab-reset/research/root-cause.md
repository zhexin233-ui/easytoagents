# 根因调研：渠道导入 CONFLICT 与工具切换状态残留

日期：2026-09-12

## Bug 1：删除全部渠道后重新导入报 CONFLICT「该原生目标已经建立受管基线」

### 复现路径

1. 用户删除某个工具（如 Claude）的全部渠道档案。
2. 点击「检测已有配置」→ 生成导入预览。
3. 点击「确认无写入接管」→ 后端报 `CONFLICT：该原生目标已经建立受管基线`。

### 根因链路

- 删除档案：`delete_provider_profile` → `delete_profile_row`
  （`src-tauri/src/db/profiles.rs:1134`）只删 `provider_profiles` 行，
  **不清理** `managed_targets` 中该 (tool, artifact_kind='provider', scope='global',
  target_path) 行上的基线字段（baseline_full_hash / baseline_managed_hash /
  baseline_projection_json）。
- 再次导入：`confirm_provider_import`
  （`src-tauri/src/profiles/service_orchestration.rs:100`）→ `adopt_imported_provider`
  （`src-tauri/src/db/profiles.rs:146`）→ `adopt_baseline`
  （`src-tauri/src/db/profiles.rs:889`）。
- `adopt_baseline` 查到既有 `managed_targets` 行且基线字段非 NULL 时
  （`src-tauri/src/db/profiles.rs:915-919`），直接返回
  `AppError::conflict("import", "该原生目标已经建立受管基线")`。
- 由于 `reject_existing_profiles`（同事务、先于 adopt_baseline 执行）已保证该工具
  此时**没有任何中央档案**，这个既有基线必然是「孤儿基线」——没有任何档案引用它，
  唯一的消费者就是导入流程本身，拒绝重新接管过于严格。

### 佐证：其它 artifact 的既有语义

- MCP 导入（`src-tauri/src/db/mcp_imports.rs:187-221`）：目标行已存在时，
  用 `row_version` 乐观锁守卫**更新**基线（extend），而不是报冲突。
- 同步 Apply / 重新接管（`src-tauri/src/db/sync.rs:394-482`）：
  `update_readopt_target_baseline` / `clear_readopt_target_baseline` 均以
  target_id 直接覆盖基线。
- 只有 provider 导入的 `adopt_baseline` 对孤儿基线报冲突。

### 修复方向（结论）

在 `adopt_baseline` 中允许「孤儿基线刷新」：既有行存在且基线非 NULL 时，
改走 UPDATE 分支覆盖基线（full/managed hash、projection、last_status='in_sync'），
并加 `row_version` 乐观锁守卫（managed_targets 有 row_version 列，
`ensure_profile_target` 在 `src-tauri/src/profiles/sync.rs:226-247` 已示范读取方式）。
原 `Some((_id, _, _))` 冲突分支删除或改造。

不做「删除最后一个档案时清除基线」的方案：`sync_items.target_id` 对
managed_targets 是 `ON DELETE RESTRICT`（有同步历史时删整行会被拒绝），
且改动面大、影响 overview 清理文案（`src/lib/rpc.ts:29`）等语义，收益低。

### 涉及测试

- `src-tauri/src/profiles/tests.rs`：`provider_import_preview_is_persisted_redacted_and_adopts_without_writing`（L482）等导入用例。
- 需新增回归用例：先建立基线（导入或同步一次）→ 删除全部档案 → 再次导入应成功且基线被刷新。

## Bug 2：切换工具（Claude/Codex/Zcode 等）后显示页状态残留

### 根因

- `src/app/router.tsx:57-75`：`/claude`、`/codex`、`/zcode`、`/opencode`、`/cursor`
  五条路由渲染同一组件类型 `<ToolProfilesPage tool=... />`，且位于同一个
  `<Outlet />` 位置。
- React 调和时元素类型相同 → **复用组件实例**，仅切换 props。因此
  `ProviderPanel`（`src/features/tool-profiles/provider-panel.tsx`）的局部 state
  全部跨工具残留：
  - `importPreview`（导入预览卡片，含 previewId）：切到别的工具仍显示，甚至可在
    错误的工具页签下确认导入（后端按 preview.tool 落库，与当前页签不符）；
  - mutation 错误提示（如 Bug 1 的 CONFLICT 文案）跨页签残留；
  - `editing` / `form` / `formOpen`（表单内容跨工具保留）；
  - `useSyncPreviewFlow` 的 `openPreview`（同步预览对话框可跨工具保持打开）。

### 修复方向（结论）

给每条工具路由的元素加 `key`（如 `<ToolProfilesPage key="claude" tool="claude" />`），
使切换工具时整页重挂载，所有局部 state 与对话框状态一并重置。
在 `tool-profiles-page.test.tsx` 增加回归测试：Claude 页签触发错误/导入预览后
切换到 codex/zcode，断言相关状态不再出现。

### 涉及测试

- `src/features/tool-profiles/tool-profiles-page.test.tsx`（现有 1083 行，
  L787 起有「检测已有配置」相关用例可参照）。
