# 根因与现有机制

## 结论

问题不是旧 Preview 被复用，而是合法冲突缺少 Provider 专用恢复入口：渠道激活先写中央状态，再生成新 Preview；原生 Provider 受管内容相对旧基线发生外部变化时，新 Preview 被标记为 `CONFLICT`。直接应用只能自动消费无阻塞 Preview，因此回退到弹窗；弹窗虽然有通用 readopt UI，Provider 后端与页面都没有接入它。

## 证据

- `src/features/tool-profiles/provider-panel.tsx:173`：`setActiveProviderProfile` 成功后调用 `onPreview`，中央 active profile 已先更新。
- `src/features/tool-profiles/tool-profiles-page.tsx:40`：`applyMode === "direct"` 传给 `useSyncPreviewFlow` 和 ProviderPanel。
- `src/features/sync/use-sync-preview-flow.ts:136`：只有 `canAutoApplyPreview(plan)` 为真时才直接 Apply；否则打开 `ChangePreviewDialog`。
- `src-tauri/src/profiles/sync.rs:95`：Provider 预览重新读取当前 active profile、目标基线和当前原生配置。
- `src-tauri/src/profiles/sync.rs:198`：Provider 的 `PreviewTargetRequest.readopt_available` 被固定为 `false`。
- `src-tauri/src/sync/mod.rs:680`：不可合并的漂移生成 `ChangeKind::Conflict` 和对应错误码。
- `src-tauri/src/sync/apply/validate.rs:12`：持久化 Preview 含冲突或错误码时，Apply 必须拒绝。
- `src/components/change-preview-dialog.tsx:42`：冲突使 Apply 禁用。
- `src/components/change-preview-dialog.tsx:102`：只有 `errorCode`、`readoptAvailable`、`onReadopt` 和目标路径同时存在时才显示“以当前内容重新接管”。
- `src/features/tool-profiles/tool-profiles-page.tsx:206`：Provider 页面没有向弹窗传 `readopting` / `onReadopt`。
- `src/features/tool-profiles/tool-profiles-page.test.tsx:1002`：已有无冲突直接 Apply 测试。
- `src/features/tool-profiles/tool-profiles-page.test.tsx:1030`：已有直接模式冲突回退弹窗、Apply 禁用测试，但没有恢复动作。

## 可复用先例

- `src-tauri/src/agents/service_core.rs:440` 的 `readopt_agent_target`：按精确目标路径重新扫描，Observed 时更新 full/managed baseline，Missing 时清空 baseline，其他不可读状态 fail closed；整个操作不写原生文件。
- `src-tauri/src/sync/managed.rs:414` 的 `readopt_with_scan`：MCP/Skill/Hook 共用的原子基线刷新事务。
- `src/features/agents/agents-page.tsx:265`：向 `useSyncPreviewFlow` 接入 readopt；`onReadopted` 中以原 `directApply` 语义重新请求 Preview。
- `src/components/change-preview-dialog.tsx:109`：通用 readopt 按钮和进行中状态已存在，无需创建第二套冲突弹窗。

## 设计约束

- Provider 使用整文档所有权，没有 `managed_items`；readopt 只需原子更新 `managed_targets` 的目标级 full/managed baseline。
- 恢复后必须生成新 Preview，旧冲突 Preview 永不 Apply。
- Preview/Apply 的目标身份、row version、desired hash 与数据库版本校验保持不变。
- “直接应用”跳过的是正常确认，不是冲突安全门禁。
