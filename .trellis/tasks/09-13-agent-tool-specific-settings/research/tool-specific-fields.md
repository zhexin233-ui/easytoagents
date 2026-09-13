# 调研：Agents 工具特有字段（Claude / Codex 首期）

证据日期：2026-09-12（沿用归档任务 `09-12-add-agents-management` 抓取的官方文档），核对日期 2026-09-13。
证据目录：`.trellis/tasks/archive/2026-09/09-12-add-agents-management/research/evidence/claude-codex-commands-agents/`。

## 1. Claude Code 子代理 frontmatter（`cc-sub-agents.md`）

| 字段 | 必填 | 官方定义（摘录） | 本任务处理 |
| --- | --- | --- | --- |
| `model` | 否 | `sonnet`、`opus`、`haiku`、`fable`、完整模型 ID（如 `claude-opus-5`）或 `inherit`；省略时按子代理模型顺序选择（L193） | 自由字符串，只校验非空、无空白/NUL、≤128 字节；UI 提供别名下拉 + 自定义输入 |
| `color` | 否 | `red`、`blue`、`green`、`yellow`、`purple`、`orange`、`pink`、`cyan`（L203） | 枚举白名单，其余值拒绝 |
| `tools` | 否 | 子代理可用工具列表，省略即继承全部；示例书写为逗号分隔串 `tools: Agent, Read, Bash`（L191、L320） | 中央存为字符串列表；投影渲染为 `", "` 连接的单行字符串；导入时同时接受逗号串与 YAML 列表 |

其余 Claude 字段（`disallowedTools`、`permissionMode`、`maxTurns`、`skills`、`mcpServers`、`hooks`、`memory`、`background`、`effort`、`isolation`、`initialPrompt`）本任务不建模，导入时继续进入 `dropped_fields`。

## 2. Codex 自定义 agent TOML（`codex-subagents.md`、`codex-config-reference.md`）

| 字段 | 官方定义（摘录） | 本任务处理 |
| --- | --- | --- |
| `model` | 文件内设置优先于 spawn 值与 `[agents]` 默认值（L941）；示例 `gpt-5.6-terra`、`gpt-5.3-codex-spark` | 自由字符串，校验同 Claude `model` |
| `model_reasoning_effort` | 列出 `ultra`、`max`、`xhigh`、`high`、`medium`、`low`（L866-872）；只设 `model` 时沿用已解析的 effort（L941） | 枚举白名单：`low` / `medium` / `high` / `xhigh` / `max` / `ultra` |
| `features` | `config.toml` 的 `[features]` 表，键为布尔（`features.multi_agent`、`features.hooks`、`features.memories` 等，config-reference L961-974）；`features.network_proxy` 可为布尔或表 | 中央存为「键 → 布尔」平面映射；键名 `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`；含非布尔值的 `features` 整体视为不可保留 → 进入 `dropped_fields` |

官方明示「可包含任意其他 `config.toml` 键」（L969），如 `sandbox_mode`、`mcp_servers`、`skills.config`；本任务不建模，继续 `dropped_fields`。

## 3. 现有实现中的约束点（2026-09-13 核对）

- `serde_json` 未启用 `preserve_order`，`Map` 为 BTreeMap → Codex 投影 JSON 键序按字母排序，稳定。
- `TargetFormat::Toml + WholeDocument` 渲染走 `toml_edit::ser::to_document`（`src-tauri/src/adapters/document.rs:715`），嵌套对象会成为 `[features]` 子表；子表相对顶层键的位置由 `toml_edit` 决定，实施阶段 0 需 golden 核验确定性。
- Markdown 投影 frontmatter 为 `BTreeMap<&str, String>` 经 `serde_yaml_ng` 序列化（`service_native.rs:275`）；`tools` 渲染为单行字符串即可继续沿用该类型。
- `agents.row_version` 通过 `db/mcp.rs` 的 `verify_row_version` / `touch_versioned_row` 做 CAS，分配写入已用同一模式；覆盖层写入沿用即可让已持久化预览失效。
- 导入解析 `parse_markdown_agent_file` / `parse_codex_agent_file` 目前把非交集键全部放入 `dropped_fields`，需要拆成「保留 / 丢弃」两段。
- 当前 schema 版本 22（`db/mod.rs:148`，`app/mod.rs:595/606`）。

## 4. 后续迭代候选（本任务不做）

- Cursor：`model`（默认 `inherit`）、`readonly`、`is_background`。
- OpenCode：`model`、`temperature`、`top_p`、`color`、`permission`、`hidden`（`tools` 已弃用）。
- ZCode：`model`、`thoughtLevel`、`color`、`tools`、`disallowedTools`、`maxTurns`、`injectAgentsMd`。
