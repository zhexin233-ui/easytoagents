# 修复 MCP 与 Skills 初始化状态提示

## Goal

让 MCP 与 Skills 页面的全局目标卡片准确区分正常的首次初始化、策略证据待确认和策略明确禁止；普通个人 Claude 安装可生成初始化预览，可疑或受限的企业策略仍保持安全阻断。

## Background

- 本机 Claude 2.1.245 可正常探测，使用默认配置目录；`~/.claude.json` 与 `~/.claude/settings.json` 均为合法 JSON。
- 企业级 `/Library/Application Support/ClaudeCode/managed-settings.json` 与 `managed-settings.d` 均不存在。这是普通未管理安装，不是用户配置错误。
- 当前 `probe_claude_policy` 仅在企业策略文件存在且明确包含 `strictPluginOnlyCustomization` 时生成证据；完全不存在也会退回保守探针，导致 MCP 与 Skills 返回 `CLAUDE_POLICY_UNKNOWN`（`src-tauri/src/app/tool_probe.rs:433`）。
- 全局目标首次没有持久化状态且策略允许时，MCP/Skills 后端返回可预览的 `missing`（`src-tauri/src/mcp/service.rs:347`、`src-tauri/src/skills/service.rs:367`）。
- 两个页面已经共享 `src/lib/global-target-status-ui.ts`，并将 `missing` 显示为“待初始化”；但 Unknown 与明确 Blocked 仍共用红色“策略阻止”徽标。

## Requirements

- R1：MCP 与 Skills 页面必须共享同一套全局目标状态语义、说明和操作规则。
- R2：官方企业策略文件和 drop-in 目录均不存在或目录为空时，视为 `strictPluginOnlyCustomization` 未启用，Claude MCP 与 Skills 策略均为 Allowed。
- R3：合法企业策略 JSON 未声明 `strictPluginOnlyCustomization` 时，按未启用处理；显式布尔值或字符串数组继续按现有逐 surface 规则解析。
- R4：策略文件不可读、格式或字段类型错误、路径不安全、存在动态 `policyHelper` 或 drop-in 多来源时继续返回 Unknown，禁止预览。
- R5：Unknown 显示黄色“策略状态待确认”和用户可读原因，但仍禁用预览；明确 Blocked 保持红色“策略阻止”并禁用预览。
- R6：正常首次状态显示“待初始化”，说明确认预览后才写入目标，并允许生成预览。
- R7：诊断码仅作为辅助信息；`CLAUDE_POLICY_UNKNOWN`、`CLAUDE_POLICY_BLOCKED` 不得再合并或代替用户提示。
- R8：后端探针、MCP/Skills 状态合同及两个页面必须有回归测试。

## Acceptance Criteria

- [x] AC1（R2、R6）：无企业策略文件和有效 drop-in 时，Claude MCP 与 Skills 卡片显示“待初始化”，预览按钮可用。
- [x] AC2（R3）：合法企业策略文件未声明限制时返回 Allowed；显式 `true`/`false`/surface 数组仍产生正确策略。
- [x] AC3（R4、R5）：不可验证的策略证据返回 `CLAUDE_POLICY_UNKNOWN`，两页均显示黄色“策略状态待确认”，按钮禁用且不会调用预览命令。
- [x] AC4（R5、R7）：明确限制返回 `CLAUDE_POLICY_BLOCKED`，两页均显示红色策略禁止提示，按钮禁用。
- [x] AC5（R1、R6）：MCP 与 Skills 对相同状态使用一致标签、说明和可操作性，保留各自按钮文案。
- [x] AC6（R8）：前端定向测试、Rust 定向测试、格式、lint、类型检查及项目质量门通过。

## Out of Scope

- 降低不可读、损坏、动态或多来源策略的 fail-closed 规则。
- 新增后端 `SyncStatus` 枚举值或修改生成绑定。
- 修改项目级目标卡片、同步预览对话框或其它页面的通用状态语义。
- 修改用户的 Claude 配置或创建虚假的企业策略文件。
