# 新增 Agents（子代理）全局级与项目级管理（Claude/Codex/Cursor/ZCode/OpenCode 适配）

## Goal

在 EasyToAgents 中新增 **Agents（自定义子代理）** 作为第六类可管理资源，与 MCP、Skills、Hooks 同构：

- **中央库 CRUD**：集中维护子代理定义（名称、描述、系统提示正文、启用状态）。
- **全局级管理**：把子代理分配给各工具，同步写入该工具的全局子代理目录；展示全局目标状态；支持预览 / 应用 / 重新接管 / 恢复。
- **项目级管理**：在项目详情页把子代理分配给具体项目，写入该工具的项目级子代理目录；全局分配在项目内按现有规则只读继承。
- **五工具适配**：Claude Code、Codex、Cursor、OpenCode 支持全局 + 项目；ZCode 仅支持全局（官方 Beta，项目级明示未支持）。
- **原生导入**：只读发现各工具全局子代理目录中的既有文件，用户显式选择后导入中央库。

本任务**不包含**自定义斜杠命令（Commands）。调研结论见 `research/agents-capability-matrix.md`：Claude / Cursor / Codex 已把命令并入 Skills 或标记弃用，Claude 全局命令目录无官方证据；用户已决定只做 Agents。

## 背景：五工具子代理官方合同（2026-09-12 核验）

| 工具 | 全局目录 | 项目目录 | 文件格式 | 必填字段 |
| --- | --- | --- | --- | --- |
| Claude Code | `<claude_config_dir>/agents/<name>.md` | `<root>/.claude/agents/<name>.md` | Markdown + YAML frontmatter | `name`、`description` |
| Codex | `<codex_home>/agents/<name>.toml` | `<root>/.codex/agents/<name>.toml` | TOML | `name`、`description`、`developer_instructions` |
| Cursor | `~/.cursor/agents/<name>.md` | `<root>/.cursor/agents/<name>.md` | Markdown + YAML frontmatter | `name`、`description` |
| ZCode | `~/.zcode/agents/<name>.md`（Beta） | 不支持 | Markdown + YAML frontmatter | `name`、`description` |
| OpenCode | `<opencode_config_dir>/agents/<name>.md` | `<root>/.opencode/agents/<name>.md` | Markdown + YAML frontmatter | `description`（名称取文件名） |

证据原文位于 `research/evidence/`（官方页面经 smart-search fetch 抓取）。

## Requirements

### R1 中央 Agent 库 CRUD

- 记录字段：`name`（唯一，NOCASE）、`description`（必填）、`prompt`（系统提示正文，必填）、`enabled`（默认开）。
- `name` 必须满足五工具交集规则 `^[a-z0-9][a-z0-9-]{0,63}$`（小写字母、数字、连字符，最长 64），因为它同时是文件名与各工具的标识符。
- **不建模** `model` 及任何工具特有字段（tools、permissionMode、sandbox_mode、readonly、mode 等）。各工具模型 ID 互不通用，留空即各工具的"继承父会话"默认行为。
- 乐观并发 `row_version` 与 MCP/Skills/Hooks 一致；启用 / 停用 / 删除均为显式动作，不隐式 Apply。

### R2 每工具原生投影

- 同一条中央记录按工具渲染为该工具的完整文件；文件名固定为 `<name>.md` / `<name>.toml`。
- 渲染内容只含交集字段；OpenCode 额外固定写 `mode: subagent`，避免中央子代理在 OpenCode 里被当作主代理。
- 应用写入的文件由 EasyToAgents 整文件拥有（WholeDocument）；不接管同目录下其他用户文件。

### R3 全局级管理与同步

- `set_global_agent_assignment(tool, agent_id, assigned)`：分配不隐式 Apply。
- `preview_agent_sync(tool, project_id: null)` → 持久化预览 → `apply_agent_preview` 写入原生目录；复用现有冲突检测、快照、恢复、readopt 链路。
- 全局目标状态卡按工具聚合展示：所有受管文件 in_sync 才显示 in_sync；任一文件漂移 / 缺失 / 失败则显示对应最严重状态，并可展开到单文件。
- 停用或取消分配的子代理，Apply 时删除对应受管文件（与 Hooks 停用语义一致，删除前有快照）。
- 首次接管：目标路径已存在非受管同名文件时，遵循现有整文件接管语义（预览展示脱敏差异，用户显式确认）。

### R4 项目级管理

- 项目详情页新增 Agents 资源页签：`set_project_agent_assignment` + 项目级预览 / Apply。
- 全局分配在项目内只读继承、全局/项目互斥触发器与 MCP/Skills/Hooks 相同。
- Codex 项目级沿用 `.codex` 目录既有信任层语义；不受信任时呈现 untrusted，不可应用。
- ZCode 项目级为 `Unsupported`（诊断码 `ZCODE_PROJECT_AGENTS_UNSUPPORTED`）：不出现在项目页签的工具切换中，服务层拒绝分配与预览。

### R5 原生导入

- `discover_agent_import(tool)`：只读解析该工具**全局**子代理目录中的文件；提取 `name`（缺省取文件名）、`description`、正文。
- 含工具特有字段（如 `tools`、`model`、`sandbox_mode`）的候选仍可导入，但候选卡片列出将被丢弃的字段名，让用户显式知情；解析失败或缺少必填字段的候选标记为不可导入并给出诊断码。
- `confirm_agent_import`：用户显式选择后写入中央库，不隐式接管原生文件；名称冲突报 conflict。
- Cursor 导入只读取 `~/.cursor/agents/`，不读取其兼容目录 `~/.claude/agents/`、`~/.codex/agents/`。

### R6 前端页面

- 侧边栏新增 Agents 一级入口 `/agents`；页面结构镜像 Hooks 页（中央列表 list/grid、编辑表单、每工具分配按钮、全局目标状态、变更预览、导入对话框）。
- 编辑表单：名称（实时校验交集规则）、描述、正文（多行）、启用。
- 项目详情页新增 Agents 页签，交互镜像项目 Hooks；ZCode 不出现在该页签。
- Dashboard 工具计数与 Supported/Unsupported 文案加入 Agents。

## 非目标（Out of Scope）

- 自定义斜杠命令 / prompts（Claude `.claude/commands`、Codex `~/.codex/prompts`、ZCode/OpenCode `commands/`）。
- 工具特有字段建模：Claude `tools/permissionMode/hooks/memory/...`、Codex `model/sandbox_mode/mcp_servers`、Cursor `model/readonly/is_background`、ZCode `thoughtLevel/injectAgentsMd`、OpenCode `permission/temperature/...`。
- Claude `--agents` CLI JSON、Managed settings 子代理、插件提供的子代理。
- Codex `config.toml` 的 `[agents]` 表（`default_subagent_model`、并发上限等）与 `agents.<role>.config_file` 角色声明。
- OpenCode `opencode.json` 的 `agent` 键（只用目录文件形式）。
- 项目级原生导入；Cursor 兼容目录（`.claude/agents`、`.codex/agents`）的观测或写入。
- ZCode 项目级子代理。
- 项目原生资源登记（`project_native_resources`）对子代理目录中非受管文件的只读扫描、禁用与恢复；本版只观测和写入受管的同名文件。

## Acceptance Criteria

- [ ] 五个工具各产出 Agent descriptor：Claude/Codex/Cursor/OpenCode 全局 + 项目，ZCode 仅全局、项目级 Unsupported 且无目标路径；capability、ownership、allowed root 与上表一致。
- [ ] 迁移 0022：`agents` / `agent_global_assignments` / `agent_project_assignments` 建表 + 互斥触发器；`managed_targets` CHECK 放宽加入 `'agent'`（含项目作用域、Cursor、OpenCode 三处约束）；从 v21 升级测试通过，schema_version 断言更新为 22。
- [ ] 名称校验：不满足交集规则的名称在创建、更新、导入确认时被拒绝。
- [ ] 每工具投影 golden 测试：Claude/Cursor/ZCode frontmatter、OpenCode 含 `mode: subagent`、Codex TOML 三字段。
- [ ] 全局与项目级预览 → Apply → 漂移 → readopt → 恢复链路各有测试覆盖；未确认预览不写盘、分配不隐式 Apply；停用后 Apply 删除文件且可恢复。
- [ ] ZCode 项目级分配 / 预览被服务层拒绝并返回 `ZCODE_PROJECT_AGENTS_UNSUPPORTED`；Codex 项目级 untrusted 时不可应用。
- [ ] 导入：五工具全局目录解析、缺必填字段 fail-closed、特有字段丢弃提示、名称冲突诊断。
- [ ] 前端 `/agents` 页与项目详情 Agents 页签可用；导航与 dashboard 断言更新；ZCode 不出现在项目页签。
- [ ] 实机 smoke：至少在 Claude Code 与 Codex 上确认写入的文件被 CLI 识别（`/agents` 列表或 spawn 可见）；ZCode Beta 至少确认文件被设置页 Subagents 列出。
- [ ] `pnpm bindings:generate` 后 `bindings:check` 通过；`pnpm check` 与 `git diff --check` 全绿。
- [ ] README 与 `docs/maintainers/adding-tool-adapter.md` 能力矩阵新增 Agents 一列，证据日期 2026-09-12。
