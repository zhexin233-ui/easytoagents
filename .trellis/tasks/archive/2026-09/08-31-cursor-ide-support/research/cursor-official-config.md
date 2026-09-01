# Cursor 官方配置能力调研

调研日期：2026-08-31。

## 结论

Cursor 可以安全纳入 EasyToAgents 的文件化管理能力包括：

| Artifact | 用户级 | 项目级 | 官方证据状态 |
| --- | --- | --- | --- |
| Provider / API Key / 模型 | 不支持文件管理 | 不适用 | 官方只说明在 Cursor Settings → Models 中配置，未公开可写文件 schema |
| Rules / Prompt | 未公开用户级规则文件 | `.cursor/rules/**/*.mdc` 或 `AGENTS.md` | 支持；`.mdc` 必须含 frontmatter，普通 `.md` 在 `.cursor/rules` 中会被忽略 |
| MCP | `~/.cursor/mcp.json` | `.cursor/mcp.json` | 支持；顶层容器为 `mcpServers` |
| Skills | `~/.cursor/skills/<name>/SKILL.md` | `.cursor/skills/<name>/SKILL.md` | 支持；官方也兼容 `.agents/skills`，但使用 Cursor 专属目录可避免与其他工具共享目标冲突 |

因此，除非用户明确授权依赖未公开私有存储，否则 Cursor Provider 和用户级全局 Prompt 必须标为 `unsupported`，不能猜测路径或写入 `cli-config.json`。

## 安装探测

- Cursor Desktop 在 macOS 通过原生 `.dmg` 安装，支持 macOS 12+、Apple Silicon 与 Intel。
- 官方企业部署文档给出的生产 Bundle ID 是 `com.todesktop.230313mzl4w4u92`，并明确展示 `/Applications/Cursor.app` 内的配置参考路径。
- 官方 CLI 可执行文件：`agent`；官方版本检查：`agent --version`。
- macOS/Linux/WSL 官方 CLI 安装命令：`curl https://cursor.com/install -fsS | bash`。
- 官方文档建议把 `~/.local/bin` 加入 `PATH`。产品必须优先识别生产桌面应用，可把 PATH 中的 CLI 作为补充，不能因用户未安装独立 CLI 而误报桌面应用不存在。

来源：[Quickstart](https://cursor.com/docs/get-started/quickstart)、[Deployment Patterns](https://cursor.com/docs/enterprise/deployment-patterns)、[CLI Installation](https://cursor.com/docs/cli/installation)。

## Provider / API Key / 模型

官方流程要求：

1. 打开 Cursor Settings → Models。
2. 选择 Provider。
3. 把 API Key 粘贴到输入框并保存。

官方列出的 Provider 包括 OpenAI、Anthropic、Google、Azure OpenAI 与 AWS Bedrock。页面没有给出可由 EasyToAgents 安全修改的文件路径、字段 schema 或优先级。

来源：[Bring your own API key](https://cursor.com/help/account/bring-your-own-api-key)。

## Rules / Prompt

- 项目规则位于 `.cursor/rules`，必须使用 `.mdc` 扩展名。
- frontmatter 控制 `description`、`globs`、`alwaysApply`。
- `AGENTS.md` 是简单 Markdown 替代方案，支持根目录与嵌套目录。
- 嵌套 `AGENTS.md` 与父级合并，更具体目录优先。
- User Rules 在 Customize → Rules 中配置；官方未公开用户级规则文件路径。
- Rules 冲突顺序为 Team → Project → User，较早来源优先。

为避免与 Codex 共用项目根 `AGENTS.md` 产生双工具所有权冲突，Cursor 项目 Prompt 宜使用 EasyToAgents 独占的 `.cursor/rules/easy-to-agents.mdc`，以 `alwaysApply: true` 包装 Prompt 正文；最终路径与 ownership 仍需在设计阶段确认。

来源：[Rules](https://cursor.com/docs/rules)、[Using Agent in CLI](https://cursor.com/docs/cli/using)。

## MCP

- 用户级：`~/.cursor/mcp.json`。
- 项目级：`<project>/.cursor/mcp.json`。
- 顶层容器：`mcpServers`。
- stdio 支持 `type`、`command`、`args`、`env`、`envFile`。
- 远程服务器支持 `url`、`headers`；配置还支持环境变量与工作区变量插值。

来源：[Model Context Protocol](https://cursor.com/docs/mcp)。

## Skills

官方发现路径：

- 项目级：`.agents/skills/`、`.cursor/skills/`。
- 用户级：`~/.agents/skills/`、`~/.cursor/skills/`。
- 还兼容 Claude/Codex 的 skill 目录。

每个 Skill 是包含 `SKILL.md` 的目录，可带 `scripts/`、`references/`、`assets/`。Cursor 会递归发现嵌套 Skill root。

EasyToAgents 应优先使用 Cursor 专属 `.cursor/skills` / `~/.cursor/skills` 作为受管目标，避免与 Codex 的 `.agents/skills` 形成同一路径的多工具所有权。

来源：[Agent Skills](https://cursor.com/docs/skills)。

## 调研命令

预检时默认模型不可用，因此仅对本次命令临时设置 `OPENAI_COMPATIBLE_MODEL=grok-4.6`，没有修改持久配置。

```bash
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search doctor --format json
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search search "site:cursor.com/docs Cursor project rules .cursor/rules AGENTS.md commands skills hooks official documentation" --validation balanced --extra-sources 3 --timeout 180 --format json
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search fetch "https://cursor.com/docs/cli/installation" --format markdown
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search fetch "https://cursor.com/docs/cli/using" --format markdown
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search fetch "https://cursor.com/docs/rules" --format markdown
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search fetch "https://cursor.com/docs/mcp" --format markdown
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search fetch "https://cursor.com/docs/skills" --format markdown
OPENAI_COMPATIBLE_MODEL=grok-4.6 smart-search fetch "https://cursor.com/help/account/bring-your-own-api-key" --format markdown
```
