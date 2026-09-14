# 技术设计：修复渠道切换冲突弹窗卡死

前置阅读：`research/root-cause.md`、`.trellis/spec/backend/quality-guidelines.md`、`.trellis/spec/backend/error-handling.md`、`.trellis/spec/frontend/quality-guidelines.md`、`.trellis/spec/frontend/hook-guidelines.md`。

## 1. 边界与核心决策

- 保留 Preview/Apply 的 fail-closed 语义；任何 `CONFLICT` Preview 都不可直接 Apply。
- 为 Provider 增加与 Agent/MCP 等资源一致的显式 readopt 能力。readopt 只刷新当前目标基线，不改中央 profile，也不写原生文件。
- 复用 `ChangePreviewDialog` 与 `useSyncPreviewFlow` 的现有 readopt 扩展点。
- readopt 成功后总是丢弃旧冲突 Preview 并重新预览；是否自动 Apply 继续由原 `directApply` 值决定。

## 2. 后端 RPC 与数据流

### 2.1 DTO 与命令

在 Profiles 模块增加生成绑定类型：

```text
ReadoptProviderTargetInput { tool, targetPath }
ReadoptProviderTargetResultDto { targetPath }
```

新增 `readopt_provider_target` Tauri command，并登记到命令生成/调用表。命令通过 `with_db` 调用 Profiles 服务，使用显式 `environment` 解析目标，禁止读取进程环境。

### 2.2 Provider readopt 服务

服务按以下顺序执行：

1. 校验工具具有 Provider capability，并从 adapter 生成当前全局 Provider descriptor。
2. 要求 descriptor 的规范目标路径与 `input.target_path` 精确一致，拒绝跨目标或过期 UI 输入。
3. 用现有 `ensure_profile_target`/查询结果取得 `managed_targets` 的 id、row version 与 baseline。
4. 以 `ManagedOwnership::WholeDocument` 扫描当前目标。
5. 在 SQLite `IMMEDIATE` 事务中：Observed 时用当前 full/managed hash 更新目标基线；Missing 时清空目标基线；parse、权限、类型或路径安全错误返回稳定冲突错误。
6. 返回 targetPath；不创建 Preview、不修改中央 profile、不执行原生写入。

可抽取一个 Profiles 内部的小 helper，或复用整文档目标的既有 readopt 实现；不得复制目标路径解析、安全扫描或 SQL 更新语义。

### 2.3 Preview 可恢复性

`persist_prepared_preview` 仅在漂移状态确实允许重新接管时设置 `readopt_available`。最低安全条件为目标有路径且漂移评估状态是 `ExternalOwnedChange`；parse error、权限、unsupported、unsafe path 等状态继续不可 readopt。

## 3. 前端续跑流程

`ToolProfilesPage` 向 `useSyncPreviewFlow<ReadoptProviderTargetResultDto>` 提供：

- `readopt(tool, targetPath)`：校验路径存在，调用 `commands.readoptProviderTarget`。
- `messages.readoptFailed`：Provider 专用错误文案。
- `onReadopted`：成功通知后调用 `requestPreview(tool, directApply)`。

同时向 `ChangePreviewDialog` 传入 `readopting` 与 `onReadopt`。弹窗继续满足：冲突时 Apply 禁用；readopt 进行中时恢复按钮禁用；取消与 Escape 可关闭。

续跑状态机：

```text
冲突 Preview -> 显式 readopt -> 关闭旧弹窗/刷新查询 -> 新 Preview
  directApply=true  且新 Preview 可应用 -> 自动 Apply
  directApply=false 且新 Preview可应用 -> 展示新确认弹窗
  新 Preview 仍阻塞 -> 展示新的阻塞状态与允许的恢复入口
```

## 4. 兼容性与安全

- 不改变数据库 schema；只更新既有 `managed_targets` 行。
- 不改变 Preview DTO 形状；只让 Provider 的既有 `readoptAvailable` 字段按真实状态返回。
- 旧前端忽略新增 command，不受影响；新前端只在服务声明可恢复时显示按钮。
- API 密钥、原生文件内容与未脱敏 diff 不进入 RPC 结果、日志或任务记录。
- 非受管字段仍由新的 Preview/Apply 渲染链保留。

## 5. 测试策略

- Rust Profiles 集成测试：先建立已应用基线，外部修改 Claude Provider 受管值，断言冲突且 `readoptAvailable=true`；readopt 后原文件字节不变；重新预览可应用；消费新 Preview 后得到目标渠道内容。
- Rust 失败用例：目标路径不匹配、不可读/解析失败状态拒绝 readopt；旧冲突 Preview 仍不可 Apply。
- React 页面测试：直接模式冲突显示 readopt；点击后调用精确 payload，旧 Preview 不 Apply，新 Preview 自动 Apply且不再显示普通确认。
- React 预览模式测试：readopt 后生成新 Preview，但必须由用户点击 Apply。
- 回归：现有无冲突 direct Apply、冲突 Apply disabled、取消/Escape、错误反馈、bindings check 全部通过。

## 6. 回滚

代码可整体回滚，无数据库迁移。已经 readopt 的目标只更新了应用数据库中的基线；若回滚后需要恢复旧判断，可通过重新导入/重建受管基线或再次显式同步处理，原生文件不会因 readopt 本身改变。
