# 技术设计

## 边界与现有数据流

项目详情页已经把所有项目级资源的分配、预览与应用编排在 `ProjectDetailPage`：

1. `ProjectPromptAssignments` 调用 `setPromptProjectAssignment` 更新中央意图。
2. 成功后失效项目、档案及相关资源查询。
3. `previewPromptSync(tool, projectId)` 生成后端持久化预览。
4. 父页面的 `handlePreview` 按 `directApply` 和 `canAutoApplyPreview` 决定直接调用 `applyProfilePreview`，或打开 `ChangePreviewDialog`。

当前缺口仅在第 2 步到第 3 步：提示词组件没有复制 MCP/Skills/Hooks 的 direct 分支。因此本修复只改前端编排，不改变 RPC、数据库或适配器。

## 方案

- 在 `ProjectPromptAssignments` 的 assignment mutation `onSuccess` 中，完成现有 query invalidation 并确认视图仍挂载后：
  - `directApply === true`：调用已经定义在同一组件中的 `previewMutation.mutate()`，让其 `onSuccess` 继续交给父页面 `onPreview`。
  - 否则保留现有“项目记忆文件尚未写入”提示。
- 不在 assignment mutation 中直接调用 Apply；这样仍保证所有原生写入使用持久化 preview ID，并沿用统一冲突/错误安全门。
- 该分支按 `tool` 参数运行，天然覆盖 Claude、Codex、Cursor、ZCode，不增加工具专属条件。
- 自动分配和解除分配共用同一个 mutation，因此两种操作都获得一致行为。空预览由现有 `previewMutation.onSuccess` 处理，不调用 Apply。

## 兼容性与风险

- `previewMutation` 在 mutation 成功回调执行时已完成初始化，和现有 MCP/Skills/Hooks 组件中的闭包用法一致。
- 先等待 invalidation，再生成预览，确保后端读取已提交的分配状态；不会出现旧意图预览。
- direct 模式下冲突/错误计划仍由 `handlePreview` 打开对话框；不改变项目原生资源和 Skills takeover 的人工确认例外。
- 默认模式不触发新的 RPC，现有显式预览/Apply 流程保持不变。

## 测试设计

- 在 `project-detail-page.test.tsx` 增加 direct 模式项目 Prompt 分配测试，使用 Claude fixture 断言 `previewPromptSync("claude", project.id)` 和精确 `applyProfilePreview` 参数，并断言没有 `ChangePreviewDialog`。
- 切换到 Cursor 或使用 Cursor 预览 fixture 断言同一共享组件传递 `tool: "cursor"`，证明没有 Cursor 专属漏路径；若当前测试 fixture 过重，则保留一个共享实现的工具参数回归断言。
- 为冲突计划增加 direct 模式测试，断言预览对话框出现且 `applyProfilePreview` 未调用。
- 保留现有 preview-confirm 测试，断言分配后不自动调用 `previewPromptSync`，防止默认模式隐式应用回归。
