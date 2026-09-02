# 统一迁移页面状态提示到 notify

## Goal

将中央 MCP、Skills 和 Prompts 页面中短时的操作成功、无结果和失败反馈统一迁移到仓库既有的 `notify` 通知机制，避免这些反馈以旧式内联文本长期占据页面，同时保持原有业务语义、动态数据和后续操作指引。

## Background

- 现有 `useNotify`/`Notify` 仅在三个中央页面的 direct global-sync 分支中使用；其他短时操作结果仍写入页面本地 `message`/`notice`/`applyMessage` 状态（`src/features/mcp/mcp-page.tsx:104`、`src/features/skills/skills-page.tsx:66`、`src/features/prompts/prompts-page.tsx:71-76`）。
- 已知必迁移文案位于 `src/features/mcp/mcp-page.tsx:286-296`：`已以当前内容重新接管（刷新 ... 个、清理 ... 个条目基线）；正在重新生成预览。`
- 旧规范只将 direct global-sync 结果定义为短时通知，并要求 save/delete/import 继续内联（`.trellis/spec/frontend/quality-guidelines.md:522-540`）；本任务明确扩大统一通知范围，实施后必须同步更新该规范。
- 本任务位于独立 Codex worktree，不并入或接管 `center-list-three-column-edit-actions`。

## Requirements

- **R1 统一机制**：三个中央页面的短时操作反馈必须复用现有页面本地 `useNotify`/`Notify`，不增加新的 toast 库、Context 或全局 store。
- **R2 通知语义**：成功和成功无结果使用 `kind: "success"`/`role="status"`；操作失败使用 `kind: "error"`/`role="alert"`；同一结果不得再于内联区重复呈现。
- **R3 完整审计**：迁移 MCP 的保存、删除、零目标预览、Apply、重新接管、导入反馈；Skills 的删除、分配、零目标预览、Apply、两类导入、接管准备反馈；Prompts 的保存、启用、Apply、删除、无可导入结果和导入成功反馈，以及这三页对应的页面级操作失败。
- **R4 正确时机**：通知必须在业务命令成功且必要的查询失效完成后触发；重新接管/接管准备通知不得改变后续预览时序，预览或 Apply 失败应以新的 error 通知替换前一条 success 通知。
- **R5 信息不损失**：保留创建/复用/刷新/清理/快照数量、工具名称及“仍需预览/Apply”等后续操作指引；不修改持久化预览、自动 Apply 判定、RPC payload、查询失效或对话框流程。
- **R6 上下文反馈保留**：查询 loading/error、页面空状态、持久诊断/阻断说明、表单校验与保存错误、导入对话框内检测/校验错误和按钮 pending 文案继续在所属上下文内呈现。
- **R7 验证与规范**：更新旧内联 DOM 断言，对迁移后的代表性成功/无结果/失败路径断言正确 role、文案和唯一呈现，并同步修订前端 notify 规范。

## In Scope

- `src/features/mcp/mcp-page.tsx` 及其测试。
- `src/features/skills/skills-page.tsx` 及其测试。
- `src/features/prompts/prompts-page.tsx` 及其测试。
- 因范围扩大而已过时的 `.trellis/spec/frontend/quality-guidelines.md` notify 契约。

## Out of Scope

- `src/features/projects/project-detail-page.tsx` 及 Provider/项目详情等未点名页面；即使存在相似接管文案，也不在本次三个中央页面审计范围内。
- 全局 notify 基础设施重设计、通知队列/堆叠/去重、新通知级别或展示时长调整。
- 与短时操作反馈无关的持久页面内容、表单/对话框错误、加载与空状态。
- `center-list-three-column-edit-actions` 的布局、操作按钮、代码或规划文档。

## Acceptance Criteria

- [x] **AC1 (R1-R3)**：MCP 已知“已以当前内容重新接管…”文案由 success `Notify` 呈现，动态刷新/清理数量保留，页面不再渲染对应旧式内联成功容器。
- [x] **AC2 (R1-R3, R5)**：审计表中 MCP、Skills、Prompts 的其余操作成功与成功无结果反馈均且仅以 success `role="status"` 通知出现，原有动态数据和操作指引不丢失。
- [x] **AC3 (R2-R4)**：三页的页面级操作失败均且仅以 error `role="alert"` 通知出现；直接应用、手动预览/Apply 和接管后续失败不会同时留下成功通知与内联错误。
- [x] **AC4 (R4-R6)**：查询、表单、导入对话框、持久诊断与按钮 pending 反馈保持原位置和可访问语义；Skills takeover 仍无条件打开预览，direct 模式也不自动 Apply。
- [x] **AC5 (R4-R5)**：三页的持久化预览 ID、自动 Apply 条件、RPC payload、查询失效和对话框开闭顺序不变；新通知替换旧通知时沿用现有 3,000 ms 计时契约。
- [x] **AC6 (R7)**：三个定向页面测试、`pnpm typecheck`、`pnpm lint` 及完整 `pnpm check` 通过，且前端质量规范已反映扩大后的 notify 范围。

## Technical Notes

- 共享机制位于 `src/components/use-notify.ts:5-24` 和 `src/components/notify.tsx:3-29`；支持 `success | error`，新通知替换旧通知并重启 3,000 ms 计时。
- 完整审计证据见 `research/notification-audit.md`，测试缺口见 `research/test-audit.md`。
