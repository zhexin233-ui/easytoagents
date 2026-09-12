# Agents（子代理）与 Commands 官方能力矩阵（2026-09-12 核验）

调研方式：全部通过 `smart-search fetch` 抓取官方页面，原文存于 `research/evidence/`。
ZCode 另有本机官方插件 `zcode-guide` 的 `zcode-configuration-guide` 技能文档（`evidence/zcode-local-configuration-guide.md`）。
Exa / Zhipu 通道本机未配置密钥，未使用。

## 1. 子代理（Agents）——本任务范围

| 工具 | 全局路径 | 项目路径 | 格式 | 必填字段 | 同名优先级 | 证据（页面标题 / 文件） |
| --- | --- | --- | --- | --- | --- | --- |
| Claude Code | `~/.claude/agents/*.md`（实际为 `<claude_config_dir>/agents/`） | `<root>/.claude/agents/*.md` | Markdown + YAML frontmatter | `name`、`description` | Managed settings > `--agents` CLI > 项目 > 用户 > 插件；两目录均递归扫描 | "Create custom subagents" / `claude-codex-commands-agents/cc-sub-agents.md` L131-141、L186-205 |
| Codex | `~/.codex/agents/*.toml`（`<codex_home>/agents/`） | `<root>/.codex/agents/*.toml` | TOML，每文件一个 agent | `name`、`description`、`developer_instructions` | 自定义优先于内置 `default/worker/explorer`；全局与项目同名优先级**未记载**；格式官方注明"可能演进" | "Subagents" / `codex-subagents.md` L931-971 |
| Cursor | `~/.cursor/agents/*.md`；兼容读取 `~/.claude/agents/`、`~/.codex/agents/` | `<root>/.cursor/agents/*.md`；兼容读取 `.claude/agents/`、`.codex/agents/` | Markdown + YAML frontmatter | `name`（缺省取文件名）、`description` | 项目 > 用户；`.cursor/` > `.claude/` / `.codex/` | "Subagents" / `cursor-opencode-zcode-commands-agents/cursor-subagents.md` L132-136 |
| ZCode | `~/.zcode/agents/*.md`（**Beta**） | **官方明示暂不支持** | Markdown + YAML frontmatter（camelCase 键） | `name`、`description` | 未记载 | "Subagents" / `zcode-en-subagents.md` L85-146 |
| OpenCode | `~/.config/opencode/agents/*.md`（`<opencode_config_dir>/agents/`） | `<root>/.opencode/agents/*.md` | Markdown + YAML frontmatter；亦可写 `opencode.json` 的 `agent` 键 | `description`（名称取文件名） | 配置按层合并：全局 → 项目 `opencode.json` → `.opencode/` 目录 | "Agents" / `opencode-agents.md` L146-147；"Config" / `opencode-config.md` L232、L1294 |

可选字段（各工具互不通用，第一版不建模）：

- Claude：`tools, disallowedTools, model, permissionMode, maxTurns, skills, mcpServers, hooks, memory, background, effort, isolation, color, initialPrompt`
- Codex：任意 config.toml 键，如 `model, model_reasoning_effort, sandbox_mode, mcp_servers`
- Cursor：`model`（默认 `inherit`）、`readonly`、`is_background`
- ZCode：`model, thoughtLevel, color, tools, disallowedTools, maxTurns, injectAgentsMd, mcpServers`
- OpenCode：`mode(primary/subagent/all)、model、prompt、temperature、top_p、disable、hidden、color、permission`；`tools` 已弃用

## 2. 自定义命令（Commands）——已决定不做

| 工具 | 全局 | 项目 | 结论 |
| --- | --- | --- | --- |
| Claude Code | 当前文档未列 `~/.claude/commands/` | `.claude/commands/*.md`"旧格式仍可用" | 官方已把命令并入 Skills；全局无证据，fail closed |
| Codex | `~/.codex/prompts/*.md` 官方标记 **Deprecated** | 未记载 | 不做 |
| Cursor | 未记载 | 仅 1.6 changelog | 已并入 Skills，`/migrate-to-skills` |
| ZCode | `~/.zcode/commands/*.md` | `<repo>/.zcode/commands/*.md`（本机官方指南） | 支持，但因三家主流工具已弃用命令形态，整体不纳入 |
| OpenCode | `~/.config/opencode/commands/*.md` | `.opencode/commands/*.md` | 同上 |

用户决定（2026-09-12）：只做 Agents。Claude 的"全局命令"需求由现有 Skills 全局同步承接。

## 3. 对本项目现状的对照

- `domain::ArtifactKind` 只有 provider/prompt/mcp/skill/hook；五个 adapter 均无 agents descriptor。
- 最近先例：`archive/2026-09/09-05-add-hooks-management`（新增 artifact 全链路）。
- 整文件目标先例：Prompt（`TargetFormat::Markdown` + `WholeDocument`），Codex Provider（`TargetFormat::Toml`）。
- 目录型先例：Skills（`SymlinkDirectory` + `SymlinkNames`），但那是符号链接，不适用于需要按工具渲染不同内容的 agent 文件。
