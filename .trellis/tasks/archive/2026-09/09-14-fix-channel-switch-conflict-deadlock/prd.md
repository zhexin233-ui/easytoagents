# 修复渠道切换冲突弹窗卡死

## Goal

修复渠道切换时因 Claude 原生 Provider 配置已被外部修改而进入冲突预览、但弹窗没有任何冲突处理入口的问题，使用户能够在当前流程中安全恢复并完成切换；同时保持“直接应用”模式跳过普通预览确认的语义。

## Background

- 渠道切换先更新中央 active profile，再生成新的持久化 Provider 预览；因此预览失败或冲突时，中央渠道已经切换，但原生配置尚未写入（`src/features/tool-profiles/provider-panel.tsx:173`、`src-tauri/src/profiles/sync.rs:95`）。
- 当 Claude 原生配置的受管字段相对旧基线发生外部变化时，预览会合法地产生 `changeKind: "conflict"` 和 `errorCode: "CONFLICT"`，Apply 必须保持阻止状态（`src-tauri/src/sync/mod.rs:680`、`src-tauri/src/sync/apply/validate.rs:12`）。
- “直接应用”只对无阻塞目标自动 Apply；冲突会回退到 `ChangePreviewDialog`（`src/features/sync/use-sync-preview-flow.ts:136`）。
- 通用弹窗已支持“以当前内容重新接管”，但 Provider 页面没有接入 readopt，且 Provider 预览始终返回 `readoptAvailable: false`，所以用户只看到“请先重新扫描或处理冲突”，没有对应按钮（`src/components/change-preview-dialog.tsx:102`、`src/features/tool-profiles/tool-profiles-page.tsx:206`、`src-tauri/src/profiles/sync.rs:198`）。

## Requirements

- R1：Provider 冲突预览必须提供至少一个能在当前流程内解除冲突并继续切换的明确操作，不能只显示静态提示。
- R2：不得直接绕过持久化 Preview/Apply 的冲突校验，也不得静默覆盖用户在原生配置中修改的受管字段。
- R3：“直接应用”模式下，无冲突预览继续自动应用且不显示确认弹窗。
- R4：“直接应用”模式下，冲突处理成功后应重新扫描并生成新预览；若新预览可应用，应继续直接应用，避免再次要求普通预览确认。
- R5：预览确认模式下，冲突处理成功后应重新扫描并生成新预览，保留用户对最终可应用预览的确认。
- R6：处理动作必须使用目标路径与最新扫描结果，更新基线但不在 readopt 操作本身写入原生文件；实际写入仍只能消费新生成的持久化预览。
- R7：所有失败状态必须允许关闭弹窗，并以可访问的状态/错误反馈告知用户；不得留下只能刷新页面恢复的 UI 状态。

## Key Decision

- 冲突恢复采用显式“以当前内容重新接管并继续切换”：readopt 只把当前原生文件作为新基线，随后重新生成持久化 Preview；直接应用模式在新 Preview 可应用时自动 Apply。不会把“直接应用”解释为允许静默覆盖外部受管修改。

## Acceptance Criteria

- [ ] AC1：复现“外部修改 Claude 原生 Provider 受管字段后切换渠道”时，冲突弹窗显示可执行的冲突处理入口，Apply 仍保持禁用。
- [ ] AC2：选择冲突处理后，系统以当前文件刷新 Provider 基线、关闭旧冲突预览并生成新的 Preview；旧 Preview 不会被 Apply。
- [ ] AC3：直接应用模式下，新 Preview 无阻塞时自动 Apply，完成原生渠道切换且不再弹出普通确认对话框。
- [ ] AC4：预览确认模式下，新 Preview 无阻塞时显示新的可应用预览，由用户确认后才 Apply。
- [ ] AC5：readopt、重新预览或 Apply 任一步失败时展示明确错误，用户仍可取消并再次发起操作。
- [ ] AC6：现有无冲突直接应用、持久化 Preview 校验、敏感信息脱敏与非受管字段保留行为不回归。

## Out of Scope

- 自动合并两个不同的受管 Provider 值。
- 绕过 `CONFLICT`、`STALE_PREVIEW`、row version 或目标身份校验。
- 修改 Provider 以外资源（MCP、Skills、Hooks、Agents）的冲突处理语义。
- 将中央 active profile 的更新与原生文件 Apply 重构为一个跨数据库/文件系统事务。

## Notes

- 本任务跨越 Rust/Tauri 命令、Provider 同步服务、生成绑定、React hook、对话框接入与两层测试，按复杂任务规划。
