# Agents 中央列表支持按工具特有字段配置

## 背景

`09-12-add-agents-management` 交付的中央 Agent 只保留 `name`、`description`、`prompt`、`enabled` 四个交集字段。
导入原生文件时，Claude 的 `color`、`model`、`tools` 与 Codex 的 `features`、`model`、`model_reasoning_effort`
等工具特有字段被列入 `dropped_fields` 后丢弃；中央列表也没有任何入口去设置它们。
用户希望在中央列表就能为每个工具单独配置这些字段，并随分配同步到原生目录。

## Goal

在不改变交集字段语义的前提下，为中央 Agent 增加**按工具的特有设置覆盖层**：
用户可在 Agents 页面为 Claude 与 Codex 分别配置白名单字段，投影时合并进原生文件；
导入时白名单内的字段被保留而非丢弃。

## 用户故事

- 作为用户，我在中央列表编辑一个 Agent 时，可以展开"工具特有设置"，为 Claude 设置 `model`、`color`、`tools`，为 Codex 设置 `model`、`model_reasoning_effort`、`features`。
- 作为用户，我从 Claude 全局目录导入含 `color: cyan` 的 agent 时，导入候选提示"将保留：color"，导入后中央记录带上该设置。
- 作为用户，我只填了 Claude 设置、没填 Codex 设置时，Codex 投影文件与现在完全一样。

## Requirements

### 功能

- 中央 Agent 增加"按工具的特有设置"，键为 `(agent_id, tool)`，每个工具一份、可空；四个交集字段不变。
- 首期支持字段白名单（详见 `research/tool-specific-fields.md`）：
  - Claude：`model`（自由字符串）、`color`（八色枚举）、`tools`（字符串列表）。
  - Codex：`model`（自由字符串）、`model_reasoning_effort`（`low`/`medium`/`high`/`xhigh`/`max`/`ultra`）、`features`（键 → 布尔）。
- Cursor、OpenCode、ZCode 本期不提供特有设置，接口层保持可扩展（工具枚举穷举、未支持工具返回稳定诊断码 `AGENT_TOOL_SETTINGS_UNSUPPORTED`）。
- 投影：
  - Claude：frontmatter 在 `name`、`description` 之外合并覆盖字段；`tools` 渲染为 `", "` 连接的单行字符串；未设置的键不写。
  - Codex：TOML 在三字段之外合并 `model`、`model_reasoning_effort` 顶层键与 `[features]` 子表；未设置的键不写。
  - 无覆盖行或覆盖行为空对象时，投影字节与当前实现完全一致（现有 golden 不变）。
- 校验（服务层 fail-closed，未知键、类型不符、枚举外取值一律拒绝并返回 `INVALID_INPUT`）：
  - `model`：去首尾空白后非空、无内部空白、无 NUL、≤128 字节。
  - `color`：仅 `red`/`blue`/`green`/`yellow`/`purple`/`orange`/`pink`/`cyan`。
  - `tools`：1..=64 项，每项 `^[A-Za-z][A-Za-z0-9_:*()./\- ]{0,127}$` 去重后保序；允许空列表表示"显式不设置"= 不写键。
  - `model_reasoning_effort`：六值枚举。
  - `features`：1..=64 个键，键名 `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`，值只能是布尔。
  - 整份设置 JSON 序列化 ≤ 16 KiB。
- 并发：写覆盖层通过 agent 的 `row_version` 做 CAS，并使 `agents.row_version` 递增，从而让已持久化预览失效。
- 导入：解析器把非交集键拆成"该工具白名单内可保留"与"丢弃"两类；候选卡片分别列出"将保留"与"将丢弃"；确认导入时一并写入覆盖层。白名单键类型不符（例如 `color: 3`）时该候选标记 `AGENT_FIELD_INVALID` 不可导入，而不是静默降级。
- 前端：编辑弹窗增加"工具特有设置"折叠区，按工具分面板（仅列出支持特有设置的工具）；列表行显示"N 个工具有特有设置"徽标；DTO 变化后重新生成 bindings。
- 文档：README 与 `docs/maintainers/adding-tool-adapter.md` §11 合同从"只保留四字段、不建模 model/tools"改为"交集 + 白名单覆盖层"，并记录首期白名单。

### 约束

- 不把 `model` 或任何工具特有字段提升为中央交集字段。
- 不引入自由透传原始 frontmatter 的"高级模式"。
- 不改变一个受管文件一行 `managed_targets`、`WholeDocument` 所有权与预览/Apply/快照链路。
- 迁移前向保留；旧库升级后无覆盖行，行为与 v22 一致。
- 文案简体中文。

## 非目标（Out of Scope）

- Cursor / OpenCode / ZCode 的特有字段（留作后续任务，本任务只保证接口可扩展）。
- Claude 的 `disallowedTools`、`permissionMode`、`hooks`、`memory`、`skills`、`mcpServers` 等；Codex 的 `sandbox_mode`、`mcp_servers`、`skills.config` 等。
- 项目级差异化设置（同一 Agent 在不同项目使用不同 model）；覆盖层只按工具区分。
- 从原生文件反向"接管"已受管文件中的特有字段（Readopt 语义不变，仍以中央为准）。
- 校验 `model` 是否为真实可用的模型 ID。

## Acceptance Criteria

- [x] 迁移 0023 新建 `agent_tool_settings(agent_id, tool, settings_json, ...)`，主键 `(agent_id, tool)`，外键级联删除；从 v22 升级测试通过，schema_version 断言更新为 23。
- [x] 服务层 `set_agent_tool_settings(agent_id, tool, settings|null, row_version)` 对 Claude / Codex 白名单做 fail-closed 校验；未知键、枚举外值、类型不符、Cursor/OpenCode/ZCode 均返回稳定诊断。
- [x] `AgentDto` 携带 `toolSettings`，列表与详情一致；写入后 `row_version` 递增，旧版本 CAS 失败返回 conflict。
- [x] 投影 golden：Claude 带 `model`/`color`/`tools` 的 frontmatter、Codex 带 `model`/`model_reasoning_effort`/`[features]` 的 TOML；无覆盖时与既有 golden 字节一致；重复渲染字节一致。
- [x] 修改覆盖层后 Preview 显示 Update；Apply 后文件内容含新字段；Readopt / 停用删除 / Restore 链路不受影响（E2E 至少 Claude 与 Codex 各一条）。
- [x] 导入：Claude `color: cyan` + `hooks: {...}` 的候选显示"保留 color、丢弃 hooks"；`color: 3` 的候选不可导入且诊断 `AGENT_FIELD_INVALID`；Codex `features` 含表值时 `features` 整体进入丢弃列表；确认导入后覆盖层写入。
- [x] 前端 `/agents` 编辑弹窗可编辑两工具设置并保存；列表徽标正确；测试覆盖表单校验与提交负载。
- [x] `pnpm bindings:generate && pnpm bindings:check`、`pnpm check`、`git diff --check` 全绿。
- [x] README、`docs/maintainers/adding-tool-adapter.md` §11 与相关 spec 更新为"交集 + 白名单覆盖层"合同，并列出首期白名单与证据日期。
