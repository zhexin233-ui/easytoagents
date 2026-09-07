# OpenCode 实施依据与证据纠偏

核验日期：2026-09-07。当前本机 CLI 版本：1.18.29。

本文件优先于 research 中初步研究结论。Context7 的 dev/specs/v2/config.md 是另一份草案，不能用于本次稳定版配置投影。

## 当前合同

- [Config](https://opencode.ai/docs/config/)（config-page-0.md）：JSON/JSONC、全局及项目配置、合并和覆盖。
- [Providers](https://opencode.ai/docs/providers/)（config-page-1.md:5133）：`npm`、`name`、`models`、`options.baseURL`、`options.apiKey`、`options.headers` 均有官方合同。原文 **options.apiKey: Optionally set the API key, if not using auth.** 因此不能从 `/connect` 默认写 auth.json 推断 apiKey 不能写配置。
- [Rules](https://opencode.ai/docs/rules/)（config-page-2.md:41）：全局 AGENTS.md；项目规则及 Claude fallback 本任务不接管。
- [MCP servers](https://opencode.ai/docs/mcp-servers/)（mcp-current.md）：`mcp.<name>` 直接承载条目，非 `mcp.servers`；`type=local/remote`；启用键为 `enabled`，不是 `disabled`；`command` 数组，`environment` 对象，remote `headers`、`oauth=false|{clientId,clientSecret,scope}`，timeout 为数字毫秒，非对象。初步 resources 结论里的 snake_case OAuth/v2 容器不适用。
- [Skills](https://opencode.ai/docs/skills/)（resources-opencode/skills.json）：全局 config/opencode/skills、项目 .opencode/skills；frontmatter name/description 合同；兼容 .claude/.agents 目录为附加来源，不是本工具写入目标。
- [Plugins](https://opencode.ai/docs/plugins/)（resources-opencode/plugins.json）：JS/TS module 返回 hooks object，非当前项目 command-only HookEvent 的同构配置。本任务 Hook capability 保持 Unsupported，不能把 plugin API 描述为 OpenCode 没有 Hooks。

复现命令：`smart-search fetch <上述 URL> --format markdown --output <research 文件>`；MCP 初始错误路径 `/docs/mcp` 失败，改用官方 `/docs/mcp-servers/` 抓取成功。Context7 仅用于发现，不能替代稳定版验证。

## 隔离实机检查

通过 `volta which opencode` 找到实际 executable，避免 Volta shim 随临时 HOME 丢失工具链。所有命令使用临时 HOME、XDG_CONFIG_HOME/DATA_HOME/CACHE_HOME/STATE_HOME、临时项目；子进程环境仅 PATH 和上述显式值以及禁自动升级、默认插件、models fetch 的标记；没有真实凭据，未调用模型、MCP 连接或授权登录。

命令 `opencode --pure debug paths/config/skill`，记录见 smoke-*.json：

- XDG config/data 被正确解析；Provider 合成 apiKey/baseURL/npm/models/model 被原样解析。
- local 数组命令/environment 和 remote camelCase OAuth/timeout 均通过当前 CLI 配置解析。MCP 全设 enabled=false，因此未证明服务器连通性。
- 全局及项目技能使用真实目录符号链接，debug skill 返回 eta-global-smoke 和 eta-project-smoke；主线程解析结果并断言两名称均存在。
- 同时存在 json/jsonc 时，jsonc 的 model 覆盖 json；项目覆盖全局，inline 覆盖项目。各场景 exit=0 且主线程对 model 做了精确断言。
- CLI 内置技能为 `<built-in>`，不能作为文件系统导入来源；应用不能通过 debug skill 导入用户资源，应继续使用自身有边界的文件扫描。

这些检查证明格式、路径和技能发现，不代表 EasyToAgents 接入或端到端验收已经完成。1.18.29 是已验证基线，不虚构更早最低兼容版本。
