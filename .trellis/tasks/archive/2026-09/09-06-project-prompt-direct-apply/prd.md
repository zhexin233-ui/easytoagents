# 项目级提示词直接应用

## Goal

在设置开启“直接应用”时，项目级提示词的“分配到此项目”操作应与项目级 MCP、Skills 和 Hooks 的行为一致：先提交中央分配意图，再生成持久化项目提示词预览，并在预览安全可自动确认时立即应用到对应工具的原生项目文件。用户不应为了让分配生效再次点击单独的“直接应用项目提示词”按钮。

## Background / Confirmed Facts

- `src/features/projects/project-detail-page.tsx` 的 `ProjectPromptAssignments` 当前只在分配成功后失效查询并提示“项目记忆文件尚未写入”，没有像 MCP/Skills/Hooks 一样根据 `directApply` 自动调用 `previewPromptSync`。
- 页面级 `handlePreview` 已负责直接模式的统一策略：先消费持久化预览；当 `canAutoApplyPreview(plan)` 为真时调用 `applyProfilePreview`，冲突或错误目标回退到预览对话框；空目标只显示无需写入提示。
- 该组件同时服务 Claude、Codex、Cursor、ZCode 等支持项目 Prompt 的工具，因此修复必须覆盖所有支持项目提示词的工具，而不能写 Cursor 专属分支。
- 项目提示词的原生写入仍必须经现有 preview/apply 链路；项目目标继续采用硬拷贝语义，解除分配不删除项目文件。本任务不改变该产品契约。
- 现有项目详情测试已覆盖默认（`preview_confirm`）下的提示词分配→预览→显式 Apply，但没有覆盖直接模式下分配自动预览和 Apply。

## Requirements

1. 在 `applyMode: "direct"` 下，成功分配或解除项目提示词后，先完成必要的查询失效，再自动调用 `previewPromptSync(tool, projectId)`。
2. 自动预览沿用页面现有 `handlePreview` 与 `canAutoApplyPreview` 行为：安全且非空的预览自动调用 `applyProfilePreview`；冲突、错误或其它不可自动应用的预览打开现有预览对话框；空预览不调用 Apply。
3. 在默认 `preview_confirm` 模式下，项目提示词分配行为保持现状：只更新中央意图并提示尚未写入，用户仍可手动生成预览并确认 Apply。
4. 逻辑适用于所有支持项目提示词的工具（Claude、Codex、Cursor、ZCode），不改变工具能力矩阵、目标路径、硬拷贝语义或后端命令契约。
5. 为直接模式新增前端回归测试，断言精确的预览参数、Apply 的 `previewId/tool/artifactKind/projectId`，并验证无预览对话框；同时保留默认模式现有测试以防止隐式应用回归。

## Acceptance Criteria

- [ ] 直接应用模式下，点击任一支持工具的“分配到此项目”后，调用 `previewPromptSync`，并对无冲突非空计划调用 `applyProfilePreview`；不需要第二次点击应用按钮。
- [ ] 直接应用模式下，解除项目提示词分配也经过同一 preview/apply/no-op 流程；项目文件保留语义不变。
- [ ] 直接模式下的冲突或错误预览不会绕过现有对话框安全门；默认模式不触发自动预览或 Apply。
- [ ] 测试覆盖至少一个非 Cursor 工具和 Cursor（组件共享逻辑可通过代表性工具 + 工具切换/参数断言证明），且 `pnpm test` / 定向前端测试通过。
- [ ] 不修改后端提示词目标、数据库迁移、硬拷贝或软链接策略。

## Out of Scope

- 不把项目提示词改为符号链接，也不改变项目文件所有权、漂移覆盖和解除分配保留文件的既有语义。
- 不新增“分配并应用”后端 RPC；复用现有分配、预览和 Apply 命令。
- 不改变项目原生资源禁用/恢复在直接模式下仍需人工确认的例外规则。

## Open Questions

无。用户已确认范围为所有项目级提示词管理，并确认直接应用设置应消除额外的手动应用步骤。
