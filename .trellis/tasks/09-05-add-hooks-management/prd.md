# 新增 Hook 全局级与项目级管理（codex/claudecode/zcode/cursor 适配）

## Goal

在 EasyToAgents 中新增 Hooks（生命周期钩子）这一第三类可管理资源，与现有 MCP、Skills 完全同构：

- **中央库 CRUD**：集中维护 Hook 定义（名称、事件、匹配器、命令、超时、启用状态）。
- **全局级管理**：把 Hook 分配给各工具，同步写入该工具的全局原生配置，展示全局目标状态，支持预览/应用/重新接管。
- **项目级管理**：在项目详情页把 Hook 分配给具体项目，同步写入该工具的项目级原生配置（全局分配按现有规则继承、只读）。
- **四工具适配**：codex、claudecode（Claude Code）、zcode、cursor 全部接入。

## 背景：四工具 hooks 官方合同（2026-09-05 核验）

| 工具 | 全局位置 | 项目位置 | 承载方式 | 事件命名 |
|---|---|---|---|---|
| Claude Code | `~/.claude/settings.json` 的 `hooks` 键 | `<root>/.claude/settings.json` 的 `hooks` 键 | 与 provider 共享 settings.json，需选择器隔离 | PascalCase |
| Codex | `~/.codex/hooks.json` | `<root>/.codex/hooks.json` | 独立 JSON 文件（官方推荐每层只用一种表示，选 hooks.json 而非 config.toml 内联 `[hooks]`） | PascalCase |
| Cursor | `~/.cursor/hooks.json` | `<root>/.cursor/hooks.json` | 独立 JSON 文件，顶层 `version: 1` | camelCase |
| ZCode | `~/.zcode/cli/config.json` 的 `hooks` 键 | `<root>/.zcode/config.json` 的 `hooks` 键 | 与 MCP 共享 config.json，需选择器隔离；必须 `hooks.enabled: true` 才会运行 | PascalCase |

证据：
- Claude Code：https://code.claude.com/docs/en/hooks （fetched 2026-09-05，本地存档 /tmp/ss-evidence/hooks/claude-hooks.md.md）
- Codex：https://developers.openai.com/codex/hooks.md （fetched 2026-09-05，/tmp/ss-evidence/hooks/codex-hooks-doc.md）
- Cursor：https://cursor.com/docs/agent/hooks （fetched 2026-09-05，/tmp/ss-evidence/hooks/cursor-hooks-doc.md）
- ZCode：本机官方 zcode-configuration-guide / diagnosing-hooks 技能文档（`~/.zcode/cli/plugins/cache/zcode-plugins-official/zcode-guide/0.1.0/skills/`），与本机 `~/.zcode/cli/config.json` 实测一致；ZCode MCP 接入（迁移 0013）已核验同一配置文件。

## Requirements

### R1 中央 Hook 库 CRUD
- Hook 记录字段：`name`（唯一，NOCASE）、`event`（统一 PascalCase 事件枚举）、`matcher`（可选）、`command`（必填 shell 命令串）、`timeout_seconds`（可选）、`enabled`（布尔，默认开）。
- 乐观并发（`row_version`）与现有 MCP/Skills 一致。
- 启用/停用、删除均为显式动作，不隐式 Apply。

### R2 统一事件模型与 per-tool 校验
- 统一事件枚举（canonical，PascalCase）：`SessionStart`、`SessionEnd`、`UserPromptSubmit`、`PreToolUse`、`PermissionRequest`、`PostToolUse`、`PostToolUseFailure`、`SubagentStart`、`SubagentStop`、`PreCompact`、`PostCompact`、`Stop`、`Notification`。
- 每工具支持的官方事件集合（分配与导入时校验，不支持的组合 fail-closed）：
  - claude：SessionStart、SessionEnd、UserPromptSubmit、PreToolUse、PermissionRequest、PostToolUse、Notification、SubagentStop、Stop、PreCompact（10）
  - codex：SessionStart、SessionEnd、UserPromptSubmit、PreToolUse、PermissionRequest、PostToolUse、PreCompact、PostCompact、SubagentStart、SubagentStop、Stop（11）
  - zcode：SessionStart、UserPromptSubmit、PreToolUse、PermissionRequest、PostToolUse、PostToolUseFailure、Stop（7）
  - cursor：sessionStart、sessionEnd、preToolUse、postToolUse、postToolUseFailure、subagentStart、subagentStop、preCompact、stop（9，由 canonical 映射为 camelCase）

### R3 全局级管理与同步
- `set_global_hook_assignment(tool, hook_id, assigned)`：分配不隐式 Apply。
- `preview_hook_sync(tool, project_id: null)` → 持久化预览 → `apply_hook_preview` 写入原生文件；与 MCP 相同的冲突检测、快照、恢复链路。
- 全局目标状态卡：in_sync / external_non_owned_change / external_owned_change（可重新接管 readopt）/ missing / failed / policy_blocked / untrusted 等现有状态语义完全复用。
- 首次接管：对已存在原生 hooks 内容的目标，遵循现有接管语义（预览中呈现删除/修改明细，用户显式确认）。

### R4 项目级管理
- 项目详情页新增 Hooks 资源页签：`set_project_hook_assignment` + 项目级预览/Apply（`preview_hook_sync(tool, project_id)`）。
- 全局分配的项目内只读继承、互斥触发器等约束与 MCP/Skills 相同。
- Codex 项目级 hooks 沿用其项目 `.codex/` 信任层语义（与 Codex 项目 MCP 相同的 trust 机制）；不受信任时呈现 untrusted/不可应用状态。

### R5 原生导入
- `discover_hook_import(tool)`：只读解析该工具全局目标的既有 hooks 条目为候选（含建议名称）；事件无法映射到统一枚举的条目标记为不可导入（诊断原因）。
- `confirm_hook_import`：用户显式选择后写入中央库，不隐式接管原生目标。

### R6 前端页面
- 侧边栏新增 Hooks 一级入口；页面结构镜像 MCP 页（中央列表 list/grid、编辑表单、每工具分配按钮、全局目标状态、变更预览对话框、导入对话框）。
- 事件下拉只展示统一枚举；分配按钮对不支持该事件的工具禁用并说明原因。
- 项目详情页新增 Hooks 页签，交互镜像项目 MCP/Skills。

## 非目标（Out of Scope）

- Cursor 独有事件（`beforeShellExecution`、`beforeSubmitPrompt`、`beforeReadFile`、`afterFileEdit`、Tab hooks、`workspaceOpen` 等）与 prompt 型 hook；Cursor 第三方 hooks 兼容层。
- ZCode `process` 型 hook（仅管理 `command` 型）；ZCode runner 级 `timeoutMs`/`maxOutputBytes` 设置。
- Claude 的 `if` 条件、`args` 数组、`settings.local.json` / 托管策略 / 插件 / skill frontmatter 中的 hooks。
- Codex `config.toml` 内联 `[hooks]` 表示、`--dangerously-bypass-hook-trust`、hook 信任审查的自动化（信任状态仅呈现，由用户在 CLI 内完成审查）。
- 从项目级原生配置导入（导入仅覆盖全局目标）。

## Acceptance Criteria

- [ ] 四个工具各产出全局 + 项目级 Hook descriptor；capability、ownership（选择器或整文档）、allowed root 与上表证据一致。
- [ ] 迁移 0014：`hooks` / `hook_global_assignments` / `hook_project_assignments` 建表 + 互斥触发器；`managed_targets`、`managed_items` CHECK 通过 writable_schema 放宽加入 `'hook'`；旧库升级测试通过，`schema_version` 断言更新。
- [ ] 事件校验：把 Hook 分配给不支持该事件的工具被拒绝；Cursor 用 camelCase、ZCode 强制 `hooks.enabled: true`、Codex/Cursor 为独立文件。
- [ ] 全局与项目级预览 → Apply → 漂移 → readopt → 恢复链路各有测试覆盖；未确认预览不写盘、分配不隐式 Apply。
- [ ] 导入：四工具全局目标解析 + 不可映射事件的 fail-closed 诊断。
- [ ] 前端 Hooks 页与项目详情 Hooks 页签可用；`/hooks` 路由与导航断言更新。
- [ ] `pnpm bindings:generate` 后 `bindings:check` 通过；`pnpm check` 全绿。
