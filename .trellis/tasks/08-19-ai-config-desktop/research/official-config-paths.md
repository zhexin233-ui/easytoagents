# Claude 与 Codex 官方配置路径核验

核验日期：2026-08-19

## Codex

官方来源：

- <https://developers.openai.com/codex/config-file/config-advanced>
- <https://developers.openai.com/codex/extend/mcp>
- <https://developers.openai.com/codex/build-skills>

结论：

- Codex 用户配置默认为 `~/.codex/config.toml`。
- Codex 会从仓库内 `.codex/config.toml` 读取项目级覆盖，并从项目根到当前工作目录逐层加载；同一 key 以距离工作目录最近者优先。
- 项目 `.codex` 配置仅在项目受信任时加载。
- MCP 可配置在用户 `~/.codex/config.toml` 或项目 `.codex/config.toml` 的 `[mcp_servers.*]` 表中。
- Provider 与认证重定向属于用户级配置；Codex 明确忽略项目配置中的 `openai_base_url`、`model_provider`、`model_providers`、`profile` 等键。因此渠道切换只操作用户层。
- Codex 当前从 `$HOME/.agents/skills` 加载用户 Skills，并从当前目录到仓库根的 `.agents/skills` 加载项目 Skills。
- Codex 官方支持符号链接形式的 Skill 目录。
- Codex 全局用户指令位于 `$CODEX_HOME/AGENTS.md`，默认 `~/.codex/AGENTS.md`；非空 `AGENTS.override.md` 会优先于 `AGENTS.md`。
- Codex 直接明文 Provider token 可写入 `model_providers.<id>.experimental_bearer_token`，但官方配置参考明确标注为“不推荐，优先使用 env_key”。
- Codex 自定义 Provider 支持 `env_key` 引用已有环境变量，也支持 `[model_providers.<id>.auth]` 运行外部命令并从标准输出读取令牌，可用于桥接 macOS 钥匙串而无需由本应用启动 Codex。

关键抓取证据：

- `codex-config-advanced.md:929-935`
- `codex-mcp.md:889-945`
- `codex-build-skills.md` 的 “Where Codex loads local skills” 表格
- `codex-config-advanced.md:978-1006`
- `codex-config-reference.md:1019-1038`
- `codex-agents-md.md` 的 “How Codex discovers guidance” 与 “Create global guidance”

## Claude Code

官方来源：

- <https://code.claude.com/docs/en/settings>
- <https://code.claude.com/docs/en/skills>
- <https://code.claude.com/docs/en/mcp>

结论：

- Claude 用户 Skills 位于 `~/.claude/skills`，项目 Skills 位于 `.claude/skills`。
- Claude 支持 Skill 目录为指向磁盘其他位置的符号链接，并去重同一目标。
- Claude 会从启动目录到仓库根加载项目 Skills，也可在访问嵌套目录时加载更深层的 `.claude/skills`。
- Claude 项目 MCP 使用 `.mcp.json`；用户/本机层 MCP 存储在 `~/.claude.json`。
- Claude 的项目与个人 Skills 同名时，个人级优先于项目级；产品必须在应用前提示这种遮蔽关系。
- Claude 支持 `apiKeyHelper` 外部命令动态生成认证值，可用于桥接 macOS 钥匙串；设置文件变化时该 helper 可热重载。
- Claude 用户全局指令位于 `~/.claude/CLAUDE.md`；项目指令位于项目根 `CLAUDE.md` 或 `.claude/CLAUDE.md`。
- Claude Provider 路由可通过用户 `~/.claude/settings.json` 的 `env` 写入；官方变量包含 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`、`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_MODEL` 与各默认模型变量。
- `CLAUDE_CONFIG_DIR` 可覆盖默认 `~/.claude` 配置目录；settings、全局指令和用户 Skills 因而按解析后的配置根定位。官方设置页仍把用户 MCP 单独列为 `~/.claude.json`，没有明确说明非默认 `CLAUDE_CONFIG_DIR` 是否重定位该文件；产品对这种组合必须做安装版本 capability probe，不能猜测写入。

关键抓取证据：

- `claude-skills.md:92-125`
- `claude-settings.md:1014-1022`
- `claude-settings.md:230-232,278`
- `claude-memory.md:157-170`
- `claude-env-vars.md:91-151`

## 调研工具说明

- `smart-search map` 成功获取两个官方站点的相关页面列表。
- Codex 页面通过 `smart-search fetch` 成功抓取。
- Claude `skills`、`settings` 页面抓取成功；`mcp` 页面抓取返回空内容，因此 MCP 文件位置仅采用同一官方 `settings` 页的配置来源表作为证据，不从第三方页面补充细节。
