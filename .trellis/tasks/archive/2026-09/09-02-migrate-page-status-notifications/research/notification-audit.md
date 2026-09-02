# 页面操作通知审计

## 既有 notify 契约

- `src/components/notify.tsx:3-29`：`NotifyMessage.kind` 仅有 `success | error`；成功使用 `role="status"`，失败使用 `role="alert"`，两者都为 `aria-atomic="true"`。
- `src/components/use-notify.ts:5-24`：页面本地通知状态，3,000 ms 自动清理；新对象直接替换旧通知并重启计时，没有队列或去重。
- 三个目标页面都已渲染 `<Notify>`：MCP `src/features/mcp/mcp-page.tsx:94-105,317-320`，Skills `src/features/skills/skills-page.tsx:58-67,203-205`，Prompts `src/features/prompts/prompts-page.tsx:63-76,269-271`。
- `.trellis/spec/frontend/quality-guidelines.md:530-559` 只覆盖 direct global-sync 的通知与测试；`:522-527` 仍要求 save/delete/import 保留手动及其后续指引。本任务不改“操作仍为手动”语义，但将对应反馈改为短时 notify，因此规范需同步改写。

## 迁移判定

| 类别 | 判定 | 呈现 |
|---|---|---|
| 操作成功 | 用户触发 mutation 后的一次性结果 | success notify |
| 成功无结果 | 命令成功、但本次无可应用/导入对象 | success notify |
| 页面级操作失败 | mutation 失败且当前没有更合适的表单/对话框容器 | error notify |
| 上下文错误 | 表单字段、表单 RPC、导入对话框扫描/校验 | 保留内联 |
| 持久页面状态 | query loading/error、空列表、诊断码、阻断原因 | 保留内联 |

## MCP 中央页

### 成功/无结果候选

| 流程 | 证据 | 时序要求 |
|---|---|---|
| 保存 | `src/features/mcp/mcp-page.tsx:116-133` | `invalidateMcp()` 后通知，保留 direct/preview-confirm 指引文案 |
| 删除 | `:175-190` | 命令成功并刷新列表后通知 |
| 零目标预览 | `:217-238` | 不再区分内联/notify，两种 apply mode 都使用 success notify，不 Apply |
| Apply | `:263-275` | 手动和 direct 成功均通知，关闭预览后刷新 |
| 重新接管 | `:286-296` | 接管成功、失效查询后发出 success，再触发新预览；后续失败用 error 替换 |
| MCP 导入 | `:794-814` | 关闭导入对话框、失效查询后通知，保留创建/复用数和工具名 |

### 页面级失败候选

- `enabledMutation`、`deleteMutation`、`globalAssignmentMutation`、`previewMutation`、`applyMutation`、`readoptMutation` 当前由 `operationError` 聚合后在 `src/features/mcp/mcp-page.tsx:299-343` 常驻呈现；改为各 mutation `onError` 的 error notify，删除聚合区。
- `saveMutation.error` 仍传入 `FormDialog` (`:630`)；`McpImportDialog` 内的扫描/导入错误仍由对话框呈现。

## Skills 中央页

### 成功/无结果候选

- 删除 `src/features/skills/skills-page.tsx:83-95`。
- preview-confirm 分配 `:103-123`。
- 零目标预览 `:125-140`。
- 手动/direct Apply `:165-186`。
- 本地目录导入 `:572-581`。
- 已有 Skills 复制导入 `:584-605`。
- takeover 准备 `:606-617`；通知后仍必须无条件打开 `ChangePreviewDialog`，禁止 direct auto-Apply。

### 页面级失败候选

- `globalAssignmentMutation`、`previewMutation`、`applyMutation` 当前由 `operationError` 聚合后在 `src/features/skills/skills-page.tsx:188-228` 呈现，改为 error notify。
- `contentMutation.error` 和 `deleteMutation.error` 当前以“内容预览失败/移出中央库失败”在 `:270-284` 常驻呈现；二者都是页面级用户操作，保留前缀后迁移为 error notify。
- `SkillDirectoryImportDialog`/`SkillImportDialog` 内检测、选择、部分失败与刷新失败保留对话框上下文，不在页面层二次通知。

## Prompts 中央页

### 成功/无结果候选

- 保存 `src/features/prompts/prompts-page.tsx:82-105`。
- preview-confirm 全局启用 `:125-151`。
- 手动/direct Apply `:176-203`。
- 删除 `:205-217`。
- 无可导入提示词 `:219-229`。
- 导入成功 `:231-248`。

### 页面级失败候选

- `assignmentMutation`、`deleteMutation`、`discoverMutation`、`confirmImportMutation` 当前由 `listMutationError` 在 `src/features/prompts/prompts-page.tsx:250-257,321-333` 常驻呈现，改为各自 error notify。
- 手动 `applyMutation` 失败当前由 `applyError` 在 `:258-260,280-294` 常驻呈现，与 direct 分支一样改为 error notify。
- `previewMutation` 的手动分支失败也改为 error notify；direct 分支保持当前 error notify。
- `saveMutation.error` 仍位于 `FormDialog` (`:596`)；query error、`newSessionNotice`、Codex override 阻断说明保留。

## 非目标页边界

- `src/features/projects/project-detail-page.tsx:95-113` 存在相似的 MCP 重新接管文案，但用户点名的是 MCP/Skills/Prompts 三个中央页面；项目详情页不在本任务范围。
- 已归档任务 `.trellis/tasks/archive/2026-09/09-01-notify-operation-feedback/` 只覆盖 direct global-sync；本任务在其契约上扩大范围，不重做 notify 基础设施。
- 中央列表布局/图标历史提交与本任务无关，不修改 `center-list-three-column-edit-actions` 范围。

