# 将操作结果改为三秒通知

## Goal

让全局直接应用的短暂结果反馈以自动消失的通知呈现，避免成功或失败提示长期占据页面，同时保持预览与原生写入契约不变。

## Background

- 用户明确要求：部分提示应改为 `notify` 形式，并停留三秒。
- 已给出的具体场景是“直接应用全局同步”：点击后，无论成功还是失败，都使用 `notify` 通知。
- 仓库当前没有通用 `notify`、toast 或 notification 组件；这些反馈均由页面本地状态以内联提示长期展示。
- MCP 与 Skills 页面都存在文案完全相同的“直接应用全局同步”入口；Prompts 页面存在“直接应用 <工具> 全局同步”的同类入口。

## Requirements

- R1：MCP、Skills、Prompts 三个中央页面的全局直接应用流程共用统一的 `notify` 展示机制。
- R2：直接应用的预览或 Apply 成功时显示成功通知；预览或 Apply 失败时显示失败通知。
- R3：通知展示三秒后自动消失；新通知可以替换当前通知，并重新开始三秒计时。
- R4：成功通知使用非打断式的可访问状态播报，失败通知保留 `role="alert"` 语义。
- R5：不改变持久化预览、自动 Apply 条件、RPC 参数、成功判定、失败判定、查询失效或刷新逻辑。
- R6：手动预览确认、冲突或阻断回退、零目标说明、表单校验及与直接应用无关的操作提示维持现有行为。

## Acceptance Criteria

- [x] AC1：在 direct apply 模式下，MCP、Skills、Prompts 的无冲突全局同步成功后均显示成功 `notify`，三秒后自动消失。（R1-R4）
- [x] AC2：上述三个入口的预览或 Apply 失败时均显示失败 `notify`，三秒后自动消失。（R1-R4）
- [x] AC3：direct apply 成功或失败结果不再同时以内联持久提示展示。（R1-R3）
- [x] AC4：preview-confirm 模式、冲突回退对话框、零目标说明及其他操作提示保持现有行为。（R5-R6）
- [x] AC5：现有全局同步流程、精确预览 ID/工具参数、查询刷新及相关测试保持通过。（R5）

## Out of Scope

- 修改全局同步业务逻辑或数据内容。
- 批量替换与“直接应用全局同步”无关的导入、表单或页面提示。
- 修改 Provider、项目详情或其他非本次确认的同步入口。

## Technical Notes

- 仓库不存在可复用的 toast/notification 库；实现应增加聚焦单一交互契约的共享组件或 Hook，不引入通用全局状态库。
- 需要使用可控计时器测试成功、失败、替换通知及三秒自动消失，并确保计时器在卸载或替换时清理。
- 已定位的相关实现：`src/features/mcp/mcp-page.tsx`、`src/features/skills/skills-page.tsx`、`src/features/prompts/prompts-page.tsx`。

## Notes

- 本任务按轻量任务规划，预计仅需 `prd.md`。
