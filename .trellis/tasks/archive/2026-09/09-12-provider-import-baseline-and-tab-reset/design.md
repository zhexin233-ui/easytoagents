# 技术设计：渠道导入基线冲突与工具切换状态残留

前置阅读：`research/root-cause.md`、`.trellis/spec/backend/database-guidelines.md`、
`.trellis/spec/frontend/component-guidelines.md`。

## 修复 1：`adopt_baseline` 允许刷新孤儿基线

### 现状

`src-tauri/src/db/profiles.rs:889` `adopt_baseline`：

```text
match existing {
    Some((id, None, None)) => id,                 // 行存在但无基线 → 复用
    Some((_id, _, _)) => Err(CONFLICT 该原生目标已经建立受管基线),  // ← Bug
    None => INSERT 新行,
}
// 之后 UPDATE ... WHERE id = ?1 AND baseline_full_hash IS NULL AND baseline_managed_hash IS NULL
```

### 目标行为

`adopt_imported_provider` 事务中 `reject_existing_profiles` 已先保证
`provider_profiles` 对该工具为空，因此任何既有基线都是孤儿基线，可安全刷新：

- `Some((id, _full, _managed))` → 复用既有 `target_id`，进入 UPDATE 分支。
- UPDATE 去掉 `baseline_full_hash IS NULL AND baseline_managed_hash IS NULL` 守卫，
  改为 `AND row_version = ?` 事务内一致性守卫；同时更新
  `baseline_full_hash` / `baseline_managed_hash` / `baseline_projection_json` /
  `last_status = 'in_sync'`。row_version 由表触发器在 UPDATE 后自动递增
  （`trg_managed_targets_row_version_bump`），调用方不手动 SET。
- 读行时一并取回当前 `row_version`（`ensure_profile_target`，
  `src-tauri/src/profiles/sync.rs:226-247` 已示范）。注意：SELECT 与 UPDATE
  在同一 IMMEDIATE 事务内，该守卫是 fail-closed 防御分支（同 MCP 导入
  `updated != 1 → stale_preview` 的防御模式），不承担跨事务并发检测。
- `updated != 1` → 返回 `AppError::conflict("import", "原生目标受管基线已经变化")`
  （沿用既有文案）。
- `None` 分支（无行）保持现状：INSERT 新行后再走同一条 UPDATE。
  可将 INSERT 分支统一为「确定 target_id 后统一 UPDATE」的结构，减少分叉。

### 数据一致性论证

- 不删除 `managed_targets` 行（`sync_items.target_id` 为 ON DELETE RESTRICT，
  删行在有同步历史时会失败）；仅覆盖基线字段，外键关系不变。
- provider 目标不使用 `managed_items`（`src-tauri/src/profiles/sync.rs:67`
  `managed_items: Vec::new()`），无需清理条目级基线。
- 旧快照（snapshots 表按 `run_id`/`target_path` 组织）不受影响，仍可从
  快照恢复历史原生内容。
- `last_status` 置为 `in_sync` 与 MCP 导入 extend 行为一致
  （`src-tauri/src/db/mcp_imports.rs:196-199`）。

### 兼容性

- 全新库：无既有行 → INSERT 分支，行为不变。
- 既有库「从未导入/同步过」：行不存在或基线为 NULL → 行为不变。
- 既有库「删光档案后重导入」：原 CONFLICT → 现在成功刷新基线（本任务目标）。
- 无需 schema 迁移。

## 修复 2：工具路由加 `key` 强制重挂载

### 方案

`src/app/router.tsx` 每条工具路由元素补 `key`：

```tsx
{ path: "claude", element: <ToolProfilesPage key="claude" tool="claude" /> },
{ path: "codex",  element: <ToolProfilesPage key="codex" tool="codex" /> },
{ path: "cursor", element: <ToolProfilesPage key="cursor" tool="cursor" /> },
{ path: "zcode",  element: <ToolProfilesPage key="zcode" tool="zcode" /> },
{ path: "opencode", element: <ToolProfilesPage key="opencode" tool="opencode" /> },
```

### 为什么在路由层加 key 而不是只在 `ProviderPanel` 上

- `ToolProfilesPage` 内还持有 `useSyncPreviewFlow` 的 `openPreview`
  （同步预览对话框）与 `applyMutation.error`；只重置 `ProviderPanel`
  无法覆盖这些跨工具残留。
- 整页重挂载后 statusQuery / profilesQuery 按 tool 重新拉取，属正常查询行为。

### 取舍

- 代价：切换工具时页面组件重新挂载（查询有 TanStack Query 缓存，代价小）。
- 备选「useEffect 监听 tool 变化逐个重置 state」被否决：状态点多
  （importPreview / editing / form / formOpen / mutation errors / openPreview），
  易漏且新增状态时又要记得清，key 重挂载是声明式兜底。

## 测试设计

### 后端（`src-tauri/src/profiles/tests.rs` 或 `src-tauri/src/db/profiles_tests.rs`）

新增用例 `provider_import_reattaches_orphaned_managed_baseline_after_profile_delete`：

1. 造库 → 导入 provider（建立基线，managed_targets 基线非 NULL）。
2. 删除该渠道档案（`delete_provider_profile`）。
3. 再次 `discover_provider_import` + `confirm_provider_import` → 断言成功。
4. 断言 `managed_targets` 基线字段等于第二次观测值、`last_status = 'in_sync'`。

新增/扩展用例覆盖 row_version 语义：首次导入建立基线后手动递增
`managed_targets.row_version`（模拟历史同步写入），删除档案再导入 →
断言成功且 row_version 继续递增、`last_status = 'in_sync'`。
保留既有用例：存在档案时导入冲突、全新库导入成功。

### 前端（`src/features/tool-profiles/tool-profiles-page.test.tsx`）

新增用例：Claude 页签触发 `discoverProviderImport` 返回预览（或错误）后，
切换路由到 codex / zcode，断言：

- 导入预览卡片（「发现已有渠道，仅生成了导入预览」）不存在；
- 错误提示（role="alert"）不存在；
- 表单输入值重置。

渲染方式沿用该文件现有的 MemoryRouter/路由切换测试基建（若现有测试
以单路由挂载，则补一个双路由切换的用例；实现时以文件内既有模式为准）。
