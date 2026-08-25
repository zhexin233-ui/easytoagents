# 初始化状态调查摘要

## 本机结论

- `claude` 位于 `/Users/zhexin/.volta/bin/claude`，版本为 `2.1.245 (Claude Code)`。
- 未设置 `CLAUDE_CONFIG_DIR`，因此使用默认配置根。
- `/Users/zhexin/.claude.json` 与 `/Users/zhexin/.claude/settings.json` 存在且通过 `jq empty`，没有发现用户配置格式问题。
- `/Library/Application Support/ClaudeCode/managed-settings.json` 与 `managed-settings.d` 均不存在。

## 根因链路

1. `probe_release_environment` 仅在 `probe_claude_policy` 返回证据时安装验证策略探针（`src-tauri/src/app/tool_probe.rs:103-145`）。
2. `probe_claude_policy` 当前要求官方文件可读、对象合法、无 `policyHelper` 且明确包含 `strictPluginOnlyCustomization`；任何一步没有值都返回 `None`（`src-tauri/src/app/tool_probe.rs:433-457`）。
3. 无证据时 `ExplicitEnvironment` 使用 `ConservativeClaudeCustomizationPolicyProbe`，无条件返回 Unknown（`src-tauri/src/adapters/mod.rs:345-356`、`533-540`）。
4. MCP 与 Skills 将 Unknown 映射为 `policy_blocked + CLAUDE_POLICY_UNKNOWN`，以保持 fail closed（`src-tauri/src/mcp/service.rs:347-362`、`src-tauri/src/sync/mod.rs:311-350`）。

## 已批准的新合同

- 官方主文件不存在且 drop-in 不存在或为空：Allowed。
- 合法对象未声明 `strictPluginOnlyCustomization`：Allowed。
- 显式合法值：保持现有逐 surface Allowed/Blocked 解析。
- 不可读、损坏、字段类型无效、路径不安全、`policyHelper`、非空 drop-in：Unknown。
- Unknown 前端使用黄色“策略状态待确认”且禁用预览；Blocked 使用红色“策略阻止”；Missing 使用黄色“待初始化”且允许预览。

## 现有可复用结构

- `VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting` 已把 `None` 解析为 MCP/Skills Allowed（`src-tauri/src/adapters/mod.rs:553-591`）。
- `VerifiedClaudeCustomizationPolicyEvidence::probe` 已验证安装版本、配置根与 source path 一致性（`src-tauri/src/adapters/mod.rs:637-650`）。
- `read_managed_settings` 当前使用 `Option<Value>` 合并了可信缺失与不安全/无效读取，需拆分结果类型（`src-tauri/src/app/tool_probe.rs:590` 附近）。
- MCP 与 Skills 页面已共享 `src/lib/global-target-status-ui.ts`，但 `SyncStatusBadge` 仍只按 `SyncStatus` 选择色调。

## 测试缺口

- 发布探针缺少“策略源完全不存在 => Allowed”和“合法对象字段缺失 => Allowed”测试。
- MCP/Skills 页面缺少 `CLAUDE_POLICY_UNKNOWN` 与 `CLAUDE_POLICY_BLOCKED` 的标签、说明、色调、禁用及不调用命令测试。
- MCP/Skills 状态服务需要锁定首次 Allowed 为 Missing，以及 Unknown/Blocked 诊断码分离。

## 规范冲突

`.trellis/spec/backend/quality-guidelines.md:703-708`、`:727-728` 当前把缺失策略源归入 Unknown。本任务完成后必须更新为：可信确认不存在或字段未声明为 Allowed；不可验证、动态和多来源仍为 Unknown。
