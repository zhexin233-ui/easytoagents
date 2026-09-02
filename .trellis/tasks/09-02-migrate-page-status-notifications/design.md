# 技术设计：三个中央页面统一操作通知

## 边界与原则

1. 复用已有 `useNotify`/`Notify`，不修改共享通知类型、样式、3,000 ms 生命周期或页面本地状态模型。
2. 迁移单元是“操作结果”，而不是文字：在 mutation 的 `onSuccess`/`onError` 内在正确时序调用 `notify`，同时删除对应旧状态、清理语句和内联 DOM。
3. 查询、表单、导入对话框与持久诊断反馈不是短时操作结果，保留原有语义和位置。

## 通知流

### 普通成功操作

1. 执行现有 command。
2. 按当前契约完成 query invalidation/对话框状态更新。
3. `notify({ kind: "success", message })`。
4. 不写入任何与通知文案等价的内联状态。

### 预览与 Apply

- 预览请求的页面本地布尔元数据改名为 `autoApply`，只表达“安全时是否自动 Apply”，不再同时兼任“是否通知”。
- 非空手动预览成功仍打开 `ChangePreviewDialog`，不额外通知；零目标是终止的成功无结果，发出 success notify。
- 手动与自动 Apply 的成功/失败都发出 notify，因此 Apply mutation 不再需要 `notifyResult` 布尔元数据。
- 预览失败总是 error notify。冲突/阻断是成功生成的 plan，继续打开预览对话框，不误报为 error notify。

### 重新接管/接管准备

- MCP：清空旧 preview → readopt 成功 → 失效 MCP query → success notify（包含刷新/清理数）→ 生成新 preview。
- Skills：prepare takeover 成功 → 失效 Skills query → success notify → 无条件打开返回的持久预览。
- 后续 preview/Apply 产生的新通知使用现有“替换并重启计时”契约；不增加队列。

## 页面改动边界

### MCP

- 用 success notify 替换 save/delete/empty-preview/manual-apply/readopt/import 的 `setMessage`。
- 为 enabled/delete/assignment/preview/apply/readopt 失败在各自 mutation 中发 error notify。
- 删除 `message`、导入前 `setMessage(null)`、`previewError`/`applyError`/`operationError` 聚合与绿色/红色内联操作结果 DOM。
- 保留 form error、query error、import dialog error 和 target 诊断。

### Skills

- 用 success notify 替换 delete/preview-confirm assignment/empty-preview/manual-apply/directory-import/global-import/takeover-prepared 的 `setMessage`。
- 为 content-preview/delete/assignment/preview/apply 失败发 error notify；内容预览与删除保留现有语义前缀。
- 删除 `message`、导入前清理、derived operation error 及其内联 DOM。
- 保留 query/content loading、空状态、两个导入对话框内错误和 takeover 强制预览契约。

### Prompts

- 用 success notify 替换 save/preview-confirm assignment/manual-apply/delete/no-import-result/import-success 的 `setNotice`/`setApplyMessage`。
- 为 assignment/delete/discover/confirm-import/preview/apply 失败发 error notify。
- 删除 `notice`、`applyMessage`、discover 前清理、`listMutationError`/`applyError` 及对应内联 DOM。
- 保留 save form error、query error、import preview 对话框、new-session 与 override 诊断。

## 兼容与风险

- **指引可见时长**：保存/删除/导入文案包含后续同步指引，迁移后只显示 3 秒。这是用户要求的统一 notify 行为；通过保留完整文案降低信息损失。
- **连续操作覆盖**：现有 notify 没有队列，快速连续操作会以最新结果替换旧结果；与已有 direct-sync 契约一致。
- **错误上下文**：只迁移页面级 mutation error；表单与导入对话框错误保留，避免错误消失后用户不知道需修正何处。
- **回滚**：三个页面可按文件独立回退；共享 notify 组件不改，不存在数据迁移或后端回滚需求。

