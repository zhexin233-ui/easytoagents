# 技术设计：应用方式全局配置

## 总体思路

直接应用模式**不新增任何后端写入路径**：前端照常调用 `preview_mcp_sync` / `preview_skill_sync` 生成持久化预览，
随后在「预览无冲突」时自动调用既有的 `apply_*_preview`。后端唯一新增面是设置本身的存储与读写。
这保证了直接应用与手动 Apply 走完全相同的安全链路（stale preview 复核、hash、row_version、快照、journal、单写者）。

## 后端

### 迁移 `0007_app_settings.sql`

```sql
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
```

注册到 `src-tauri/src/db/mod.rs` 的 `MIGRATIONS`（version 7，name `app_settings`）。
键值模型而非单例宽表：后续全局设置可增量加键，不需要再动 schema。

### 设置模块 `src-tauri/src/settings.rs`（新增顶层模块，与 `overview` 同级）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMode {
    PreviewConfirm, // "preview_confirm"（默认）
    Direct,         // "direct"
}
```

- 手写 `as_str()` / `from_stable_str()`（参照 `domain::Tool` 的形态，用于 DB 读写与未知值拒绝）。
- `AppSettingsDto { apply_mode: ApplyMode }`、`UpdateAppSettingsInput { apply_mode: ApplyMode }`，
  均为 `#[serde(rename_all = "camelCase")]`。
- 常量 `APPLY_MODE_KEY: &str = "apply_mode"`。
- `load_app_settings(connection: &Connection) -> Result<AppSettingsDto, AppError>`：
  `SELECT value FROM app_settings WHERE key = ?`；缺行 → `PreviewConfirm`；未知存储值 →
  `AppError`（`ErrorCode::DatabaseError`，静态消息，不带动态值）。
- `save_app_settings(connection: &mut Connection, input: &UpdateAppSettingsInput) -> Result<AppSettingsDto, AppError>`：
  显式事务 upsert（`INSERT ... ON CONFLICT(key) DO UPDATE`），回读并返回 DTO。

### 命令 `src-tauri/src/commands/settings.rs`（新增）

```rust
#[tauri::command]
#[specta::specta]
pub fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, AppError>;

#[tauri::command]
#[specta::specta]
pub fn update_app_settings(state: State<'_, AppState>, input: UpdateAppSettingsInput)
    -> Result<AppSettingsDto, AppError>;
```

- 沿用 `state.database().lock().map_err(|_| state_lock_error())` 模式（局部 `state_lock_error`
  返回 `WRITE_IN_PROGRESS`，与 `commands/overview.rs` 一致）。
- 设置为单用户单实例（`tauri_plugin_single_instance`）下的应用级偏好，不引入 row_version 乐观锁。

### `lib.rs` 注册

- `pub mod settings;`
- `.typ::<settings::ApplyMode>()`、`.typ::<settings::AppSettingsDto>()`、`.typ::<settings::UpdateAppSettingsInput>()`
- `collect_commands!` 追加两条命令；运行 `pnpm bindings:generate` 重新生成 `src/bindings/commands.ts`。

### 后端测试（`settings.rs` 内 `#[cfg(test)]`）

用 `tempdir` + `AppPaths::from_data_root` + `Database::open`（ canonicalized 临时根，禁止触碰真实数据库）：

1. 全新库 `load` 返回默认 `PreviewConfirm`。
2. `save(Direct)` 后 `load` 返回 `Direct`；`save(PreviewConfirm)` 同理（roundtrip 两个方向）。
3. 重新 `Database::open` 后设置仍在（迁移幂等 + 持久化）。
4. 手工写入未知取值后 `load` 返回 `DatabaseError` 稳定错误。

## 前端

### `src/lib/settings-api.ts`（新增）

```ts
export const settingsKeys = { all: ["settings"] as const };

export function appSettingsQueryOptions() {
  return queryOptions({
    queryKey: settingsKeys.all,
    queryFn: async () => unwrapResult(await commands.getAppSettings()),
  });
}

// 与 ChangePreviewDialog 的 Apply 可用条件保持一致：
// 有目标，且每个 target 无 conflict、无 errorCode。
export function canAutoApplyPreview(plan: PreviewPlan): boolean {
  return (
    plan.targets.length > 0 &&
    plan.targets.every(
      (target) => target.changeKind !== "conflict" && target.errorCode === null,
    )
  );
}
```

页面通过 `useQuery(appSettingsQueryOptions())` 读取（遵循「不把 useQuery 包进自定义 hook」的项目约定），
派生 `directApply = settingsQuery.data?.applyMode === "direct"`。设置未加载完成时按默认
（`preview_confirm`）处理，即 `directApply === false`，不会出现意外跳过确认。

### 设置页 `src/features/settings/settings-page.tsx`（新增）

- 路由 `/settings`；`app-shell.tsx` 的 `primaryLinks` 追加 `{ to: "/settings", label: "设置" }`。
- 「应用方式」卡片：勾选框「直接应用（跳过预览确认）」，
  `checked = applyMode === "direct"`，change 时调用
  `commands.updateAppSettings({ applyMode: checked ? "direct" : "preview_confirm" })`，
  成功后 `invalidateQueries({ queryKey: settingsKeys.all })`；pending 期间禁用勾选框。
- 文案说明三点：默认关闭；开启后仅 MCP/Skills 全局同步与项目追加生效；存在冲突或错误时仍会打开预览对话框。
  另说明 Profiles 流程本次不适用，仍需预览确认。

### 页面接入（MCP / Skills / 项目详情）

**`src/features/mcp/mcp-page.tsx`**

- `const settingsQuery = useQuery(appSettingsQueryOptions());`
  `const directApply = settingsQuery.data?.applyMode === "direct";`
- `previewMutation.onSuccess`：空 targets 分支保持不变；非空时

  ```ts
  if (directApply && canAutoApplyPreview(plan)) {
    applyMutation.mutate({ previewId: plan.previewId, tool, projectId: null });
    return;
  }
  setOpenPreview({ plan, tool });
  ```

  （useMutation 的 options 每次渲染都会刷新，闭包读到的是最新 `directApply`。）
- 按钮：`disabled` 条件不变；文案 `directApply ? "直接应用全局同步" : "生成全局预览"`（pending 文案对应切换）。
- `applyMutation.onSuccess` 的既有消息/失效逻辑复用；直接应用时对话框本就未打开，`setOpenPreview(null)` 无副作用。

**`src/features/skills/skills-page.tsx`**：与 MCP 页同构（`previewSkillSync` / `applySkillPreview`）。

**`src/features/projects/project-detail-page.tsx`**

- 父组件统一接管决策：`onPreview(plan)` 回调签名不变，父级 handler

  ```ts
  const handlePreview = (plan: PreviewPlan, tool: Tool, artifactKind: ArtifactKind) => {
    if (directApply && canAutoApplyPreview(plan)) {
      applyMutation.mutate(
        { plan, tool, artifactKind },
        { onSuccess: () => setMessage("项目原生配置已通过持久化预览应用并完成写后验证。") },
      );
      return;
    }
    setOpenPreview({ plan, tool, artifactKind });
  };
  ```

  子组件 `ProjectMcpAssignments` / `ProjectSkillAssignments` 内部的预览 mutation 只负责生成预览并回调，逻辑不变。
- `AssignmentCard` 新增 `directApply: boolean` prop，按钮文案
  `directApply ? "直接应用项目 X 同步" : "预览项目 X 同步"`。

**`src/features/tool-profiles/tool-profiles-page.tsx`**：本次不接入（R5），维持预览确认。

## 数据流（开启直接应用后的项目追加示例）

```
勾选设置 → update_app_settings(app_settings.apply_mode='direct')
项目详情页点击「直接应用项目 MCP 同步」
  → previewMcpSync({tool, projectId, excludeFromGit})   # 既有校验：信任/策略/路径/基线
  → canAutoApplyPreview(plan)?                          # 与对话框 Apply 可用条件一致
      true  → applyMcpPreview({previewId, ...})         # 既有引擎：快照 → 原子写 → journal
      false → setOpenPreview(plan)                      # 弹窗展示冲突，Apply 仍被禁用
  → invalidate project/mcp/skill keys
```

## 兼容与回滚

- Schema 仅新增独立表，无既有表变更；回滚 = 还原代码，遗留 `app_settings` 表无害（迁移为前向追加）。
- 默认值保证升级后行为逐字节一致；开关只在「预览成功且无冲突」分支改变 UI 路径。
- 绑定变更通过 `bindings:generate` + `bindings:check` 保持同步。
