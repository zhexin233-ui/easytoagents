# Pi 接入已核验合同（合并版）

- 工具：`@earendil-works/pi-coding-agent`（Pi，纯 CLI），核验版本 **0.85.1**
- MCP 依赖：`pi-mcp-adapter` **2.33.0**（MIT，© 2026 Nico Bailon，`https://github.com/nicobailon/pi-mcp-adapter`）
- 核验日期：**2026-09-14**
- 合并自：`provider-and-install.md`、`prompt-and-skills.md`、`mcp-hooks-agents.md`、`pi-mcp-adapter.md`、`brand-asset.md`
- 证据分层：【官】官方文档 `https://pi.dev/docs/latest/*`（HTTP 200）｜【源】0.85.1 与 2.33.0 随包源码/文档副本｜【机】本机实测（隔离 `PI_CODING_AGENT_DIR`/`HOME` + 真实 `~/.pi/agent`）｜【反】官方页面 404 反向证据｜【外】第三方包文档（仅说明「谁定义了这个文件」，**不**作为写入授权来源；MCP 的授权来自用户显式决策）

## 1. 能力矩阵（最终）

| Artifact                                                        | Global                        | Project                                   | Import      | Apply       | 结论                                                         | 关键证据                                                                                                                    |
| --------------------------------------------------------------- | ----------------------------- | ----------------------------------------- | ----------- | ----------- | ------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| **Provider**                                                    | Supported                     | Unsupported                               | Supported   | Supported   | 仅全局 `models.json`                                         | 【官】models / providers / custom-provider                                                                                  |
| **Prompt**                                                      | Supported                     | Unsupported                               | Supported   | Supported   | 仅全局 `AGENTS.md`；项目 Prompt 属产品政策永久 Unsupported   | 【官】quickstart / usage#context-files                                                                                      |
| **MCP**                                                         | Supported（前置：适配器就绪） | Supported（前置：适配器就绪；不受 trust） | Supported   | Supported   | 用户显式授权接管适配器的 Pi 自有配置面                       | 【源】`config.ts` / `README.md`「File Layout」「Project Config」；【机】isolation smoke（项目文件覆盖全局）                 |
| **Skills**                                                      | Supported                     | Supported（trust-gated）                  | Supported   | Supported   | 受管符号链接成立（实机已验证）                               | 【官】skills#locations；【源】`skills.js` symlink 解析；【机】`<pi_agent_dir>/skills/smart-search-cli` 链接已被 0.85.1 加载 |
| **Hooks**                                                       | Unsupported                   | Unsupported                               | —           | —           | 事件机制为 TypeScript 扩展 API，非声明式文件                 | 【官】extensions；【反】`/docs/latest/hooks` 404；settings 无 `hooks` 键                                                    |
| **Agents**                                                      | Unsupported                   | Unsupported                               | —           | —           | 无官方目录/schema；`.pi/agents/*.md` 是 Trellis 扩展自建约定 | 【官】usage「no sub-agents」；【反】`/docs/latest/agents` 404；【机】`.pi/extensions/trellis/index.ts:1244/1737`            |
| Prompt Templates / Extensions / Themes / Pi Packages / Sessions | OutOfScope                    | OutOfScope                                | —           | —           | 不纳入现有六类 ArtifactKind，不新增类型                      | 【官】settings#resources、packages、themes、extensions                                                                      |
| Provider 凭据 `auth.json`                                       | Unsupported                   | Unsupported                               | Unsupported | Unsupported | 只读禁区，永不导入/写入/明文预览                             | 【官】providers：tokens stored in `<pi_agent_dir>/auth.json`，0600，优先于环境变量与 models.json                            |

与现有五工具的对比（`provider / promptGlobal / mcp / skills / hooks / agents / projectAgents / agentToolSettings`）：

| Tool     | provider  | prompt    | mcp         | skills       | hooks | agents | projectAgents | agentToolSettings |
| -------- | --------- | --------- | ----------- | ------------ | ----- | ------ | ------------- | ----------------- |
| **pi**   | ✅ 仅全局 | ✅ 仅全局 | ✅ 需适配器 | ✅ 全局+项目 | ❌    | ❌     | ❌            | ❌                |
| opencode | ✅        | ✅        | ✅          | ✅           | ❌    | ✅     | ✅            | ❌                |
| cursor   | ❌        | ✅        | ✅          | ✅           | ✅    | ✅     | ✅            | ❌                |

> Pi 的 MCP 是**条件能力**：Pi 核心不含 MCP，配置只在 `pi-mcp-adapter` 加载时才生效。因此它是本接入里唯一「前置不满足即必然无效」的能力。

## 2. 稳定路径与本地约定（唯一写入面）

| 目标        | 路径                          | 形态                             | ownership                                                    |
| ----------- | ----------------------------- | -------------------------------- | ------------------------------------------------------------ |
| Provider    | `<pi_agent_dir>/models.json`  | JSON，`providers.<id>` 条目级    | selector `["providers"]`，按 id 局部合并，**禁止整文件覆写** |
| 全局 Prompt | `<pi_agent_dir>/AGENTS.md`    | Markdown 整文                    | `["$document"]`                                              |
| 全局 Skills | `<pi_agent_dir>/skills`       | 目录 + 逐名称符号链接            | `["$children"]` / `ManagedChildrenOnly`                      |
| 项目 Skills | `<project_root>/.pi/skills`   | 同上                             | 同上；`allowed_root = project_root`                          |
| 全局 MCP    | `<pi_agent_dir>/mcp.json`     | JSON，`mcpServers.<name>` 条目级 | `["mcpServers"]` + 逐名称；禁止整文件覆写                    |
| 项目 MCP    | `<project_root>/.pi/mcp.json` | 同上                             | 同上；`allowed_root = project_root`                          |

`<pi_agent_dir>` = `PI_CODING_AGENT_DIR`（显式注入）否则 `~/.pi/agent`。变量名实际为 `` `${piConfig.name.toUpperCase()}_CODING_AGENT_DIR` ``（vanilla pi 即 `PI_CODING_AGENT_DIR`）；展开规则与适配器一致：`~` → home、`~/x` → home/x、绝对路径原样、**相对路径不可映射 → fail closed**（证据：`pi-mcp-adapter/agent-dir.ts:11-25`）。

**绝对禁写清单**：

- Pi 自管/凭据：`auth.json`、`models-store.json`（`pi update --models` 会重写）、`trust.json`、`settings.json`。
- 提示词/系统：`SYSTEM.md`、`APPEND_SYSTEM.md`、`.pi/SYSTEM.md`、`.pi/APPEND_SYSTEM.md`。
- 资源目录：`extensions/`、`npm/`、`git/`、`themes/`、`bin/`、`~/.pi/skills`（非官方位置）。
- 跨工具共享 Skills：`~/.agents/skills`、`<project>/.agents/skills`。
- **MCP 共享/跨工具文件**：`~/.config/mcp/mcp.json`、`~/.agents/mcp.json`、`~/.agents/mcp/mcp.json`、`<project>/.mcp.json`（后者已被 Claude 项目 MCP 合同占用）。仅作只读冲突检测。
- **适配器自有旁路文件**：`<pi_agent_dir>/mcp-oauth/`、OS 凭据库（`bearerTokenStore`）、`mcp-cache.json`、`mcp-npx-cache.json`、`mcp-onboarding.json`、`agent-plugin-data/`、`<project>/.pi/mcp-traces/*.jsonl`。

## 3. Provider schema（0.85.1，仅记录受管/敏感边界）

顶层 `{ "providers": { "<id>": ProviderConfig } }`；`ProviderConfig` 字段：`baseUrl`、`api`、`apiKey`、`oauth`（仅 `radius`）、`headers`、`authHeader`、`models[]`、`modelOverrides`。

- 合并语义：只给 `baseUrl`/`headers` 且不给 `models` 时保留全部内置模型；给 `models` 时按 `id` upsert。→ 写入必须是条目级局部更新。
- `apiKey`/`headers` 支持 `$ENV` 与 `!command`：**不展开、不执行**，原样保留并脱敏。
- 凭据优先级：`--api-key` > `auth.json` > 环境变量 > `models.json`；`auth.json` 优先于 `models.json`，因此「写了 models.json 的 key」不等于生效，需要诊断提示。

敏感 selector：`providers/*/apiKey`、`providers/*/headers`、`providers/*/models/*/headers`、`providers/*/modelOverrides/*/headers`。

## 4. Prompt 加载语义与失效场景

全局指令载体 `<pi_agent_dir>/AGENTS.md`，整文接管。同目录候选顺序（源码 `resource-loader.js:32-53`，官方未文档化完整表）：`AGENTS.override.md` → `AGENTS.md` → `AGENTS.MD` → `CLAUDE.md` → `CLAUDE.MD`，取第一个存在的普通文件。

失效/遮蔽场景（必须诊断，不得静默）：

- `AGENTS.override.md` 存在 → 我们的 `AGENTS.md` **必然无效**（**硬阻断 Apply**）；
- 用户只有 `CLAUDE.md`/`CLAUDE.MD`/`AGENTS.MD` → 导入漏读 + 我们写入后**反向遮蔽**用户文件；
- `SYSTEM.md`/`APPEND_SYSTEM.md` → 不使 `AGENTS.md` 失效（smoke case5 证伪该担心），仅提示语义；
- 运行时 `--no-context-files / -nc` 静态不可探测 → `Unknown`，文档明示。

## 5. MCP 合同（`pi-mcp-adapter` 2.33.0）

### 5.1 文件与优先级（高优先级在后，同名字段逐字段覆盖）

```text
~/.config/mcp/mcp.json  <  ~/.agents/mcp.json  <  ~/.agents/mcp/mcp.json
  <  <pi_agent_dir>/mcp.json  <  <root>/.mcp.json  <  <root>/.pi/mcp.json
```

- Pi 自有文件是**写入目标**（README「File Layout」）：`<pi_agent_dir>/mcp.json` 为全局 override，`.pi/mcp.json` 为项目 override。
- 项目文件 **不受 Pi project trust 门禁**：`.pi/mcp.json` 不在官方 trust 清单，适配器源码零 trust 读取，全局安装的适配器在 trust 解析前加载。→ descriptor `trust = NotRequired`，但需给安全提示（stdio `command` 会在打开项目时执行）。
- 陷阱：`<pi_agent_dir>/mcp.json` 里的受管条目会被 `<root>/.mcp.json` 或用户手写 `<root>/.pi/mcp.json` 的同名 server **静默覆盖**（已实测）。Apply 前必须做遮蔽检测并硬阻断。
- `PI_MCP_CONFIG_MODE=exclusive` → 只读 `<pi_agent_dir>/mcp.json`，项目文件被忽略 → 项目 Apply 必须拒绝。

### 5.2 schema

顶层：`mcpServers`（受管，条目级；**读取时同时接受别名 `mcp-servers`**，写入只写 canonical `mcpServers`）、`imports` / `settings` / `claudePlugins`（适配器/用户自有，只读不写）。

受管映射：stdio = `command` + `args` + `env` + `cwd`；HTTP = `url` + `headers`；停用 = `disabled: true`。**不写 `type`**（适配器按字段存在性推断）。

未受管但必须原样保留：`socket`、`inheritEnv`、`auth`、`oauth.*`、`bearerToken`/`bearerTokenEnv`/`bearerTokenStore`、`requestHeadersCommand`、`caFile`、`lifecycle`、`idleTimeout`、`requestTimeoutMs`、`protocolVersion`、`exposeResources`、`directTools`、`toolPrefix`、`includeTools`、`excludeTools`、`searchKeywords`、`debug`、`trace`、以及任何未知字段。

不得求值：`${VAR}`、`$env:VAR`、`{env:VAR}`、`!command`（`!!` 为转义）。适配器官方保证这些命令**不会**在读取/合并/预览/哈希/渲染时执行；EasyToAgents 必须保持零 shell out 与逐字节保留。

敏感 selector（必须）：`mcpServers/*/env`、`mcpServers/*/headers`、`mcpServers/*/bearerToken`、`mcpServers/*/bearerTokenEnv`、`mcpServers/*/oauth`、`mcpServers/*/requestHeadersCommand/env`、`mcpServers/*/requestHeadersCommand/args`。
次级：`mcpServers/*/args`、`mcpServers/*/url`、`mcpServers/*/requestHeadersCommand/command`、`mcpServers/*/caFile`、`mcpServers/*/socket`、`mcpServers/*/cwd`。

### 5.3 双方共写同一文件

适配器自身也会写这两个文件（`writeDirectToolsConfig`、`ensureCompatibilityImports` 写 `imports`、`writeProjectServerDisabledOverride` 写项目 `disabled`）。两边序列化都是「2 空格缩进 + 尾换行」，键顺序保留。因此：Apply 必须 read-modify-write；受管条目被适配器追加字段后哈希漂移属**正确行为**（`PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY`，走正常重新接管），非受管条目必须保留。

## 6. Skills 语义

- 官方位置：全局 `<pi_agent_dir>/skills/`、`~/.agents/skills/`；项目 `.pi/skills/`、`.agents/skills/`（trusted 后）。
- 同名优先级（源码 `package-manager.js:51-64`）：project 显式 0 > project 自动 1 > user 显式 2 > user 自动 3 > package 4，**保留第一个**。→ 项目同名会压过全局同名。
- frontmatter：`name` 官方必填（≤64，`a-z0-9-`），0.85.1 实际缺失时回退目录名；`description` 必填，缺失即**不加载**；`name` 与目录名不一致时以 frontmatter 为准。
- symlink：0.85.1 跟随符号链接的 skill 目录与符号链接的 `SKILL.md`（源码 + smoke + 实机三重证据）；断链**静默忽略**，EasyToAgents 必须自检并诊断。
- 项目 trust：`.pi/skills` 仅项目受信任后加载；`defaultProjectTrust=ask`（默认）在非交互模式等同忽略。→ 未信任时禁止 Apply 并给诊断。

## 7. 安装探针合同

- Pi 只有 **CLI**，无官方 macOS `.app`。探针：显式注入 PATH 与超时，执行 `pi --version`，宽容 semver 校验；`ENOENT`/非零退出 → `ToolNotInstalled`；超时/权限/异常输出 → `Unsupported`（`PI_INSTALLATION_PROBE_UNSUPPORTED`）。
- 反例（不得作为判据）：volta/nvm shim 的 realpath、`npm ls -g`、`<pi_agent_dir>/bin/{fd,rg}`、配置文件是否存在、`<pi_agent_dir>/git/`。
- 官方登录：`/login` 是交互式 TUI 命令，无非交互合同 → Pi 官方登录 Unsupported（与 Cursor/ZCode/OpenCode 同桶）。MCP OAuth（`auth-start`/`auth-complete`）同样不代跑。

**MCP 适配器就绪探测**（只读文件）：

| 判据                                                                           | 结论                                              |
| ------------------------------------------------------------------------------ | ------------------------------------------------- |
| `<pi_agent_dir>/settings.json` 的 `packages[]` 含 `npm:pi-mcp-adapter`         | 已声明启用                                        |
| 对象形 `packages` 条目 `extensions: []`                                        | 被 `pi config` 过滤 → `PI_MCP_ADAPTER_NOT_LOADED` |
| `<pi_agent_dir>/npm/node_modules/pi-mcp-adapter/package.json` 存在 + `version` | 已下载 + 版本判据                                 |
| `<project>/.pi/settings.json` 与 `<project>/.pi/npm/...`                       | 项目级启用（受 project trust）                    |
| 全部缺失                                                                       | `PI_MCP_ADAPTER_MISSING`                          |
| 版本低于最低支持                                                               | `PI_MCP_ADAPTER_VERSION_UNSUPPORTED`              |

## 8. 诊断码（`PI_` 前缀，不复用 Codex 专用常量）

| 码                                                                    | 语义                                                           |
| --------------------------------------------------------------------- | -------------------------------------------------------------- |
| `PI_HOOKS_UNSUPPORTED`                                                | 任何 Pi Hook 请求                                              |
| `PI_AGENTS_UNSUPPORTED`                                               | 任何 Pi Agents 请求（全局与项目）                              |
| `PI_PROJECT_PROMPT_UNSUPPORTED`                                       | 项目 Prompt 分配/导入/写入                                     |
| `PI_INSTALLATION_PROBE_UNSUPPORTED`                                   | 探针结果不可信                                                 |
| `PI_AGENT_DIR_OVERRIDE_UNMAPPED`                                      | 进程存在 `PI_CODING_AGENT_DIR` 但调用方未显式映射              |
| `PI_PROMPT_OVERRIDE_DETECTED` / `PI_PROMPT_OVERRIDE_UNKNOWN`          | `AGENTS.override.md` 存在/无法判定（硬阻断）                   |
| `PI_PROMPT_FALLBACK_PRESENT`                                          | 目标依赖 `CLAUDE.md`/`AGENTS.MD` 回退（导入不完整 + 遮蔽风险） |
| `PI_PROJECT_SKILLS_UNTRUSTED` / `PI_PROJECT_SKILLS_TRUST_UNKNOWN`     | 项目 Skills trust 状态                                         |
| `PI_SKILLS_SETTINGS_EXTRA_PATHS`                                      | `settings.json` 的 `skills`/`prompts` 另有来源或排除模式       |
| `PI_SKILL_NAME_COLLISION`                                             | 同名 skill 跨 scope/来源冲突（pi 保留第一个）                  |
| `PI_SKILL_SYMLINK_BROKEN` / `PI_SKILL_SYMLINK_ESCAPE`                 | 受管链接断链/越界（pi 静默忽略，必须自检）                     |
| `PI_SKILL_FRONTMATTER_NAME_MISMATCH` / `PI_SKILL_DESCRIPTION_MISSING` | SKILL.md 合同不符                                              |
| `PI_MCP_ADAPTER_MISSING`                                              | `pi` 已装但适配器未声明且未安装（写入必然无效）                |
| `PI_MCP_ADAPTER_NOT_LOADED`                                           | 已声明/已安装但被 `pi config` 过滤，或项目级启用未受信任       |
| `PI_MCP_ADAPTER_VERSION_UNSUPPORTED`                                  | 适配器版本低于最低支持                                         |
| `PI_MCP_SHADOWED_BY_PROJECT_SHARED`                                   | 受管全局条目被 `<root>/.mcp.json` 同名 server 覆盖（硬阻断）   |
| `PI_MCP_SHADOWED_BY_PROJECT_PI`                                       | 受管条目被用户手写 `.pi/mcp.json` 同名 server 覆盖（硬阻断）   |
| `PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED`                               | `PI_MCP_CONFIG_MODE=exclusive` 且请求项目 MCP                  |
| `PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY`                                 | 受管条目被适配器追加字段（漂移，走重新接管）                   |
| `PI_MCP_INLINE_SECRET`                                                | 条目含明文字面凭据                                             |
| `PI_PROVIDER_INLINE_API_KEY`                                          | Provider 条目含明文 `apiKey`（脱敏 + 提示）                    |

`PI_MCP_UNSUPPORTED` 已作废（产品决策变更）。

## 9. 显式 Unknown（不得写成 Supported）

| 项                                              | 状态                   | 缺口                                                                                                                                                    |
| ----------------------------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pi --version` 输出格式的版本间稳定性           | 部分 Unknown           | 仅 0.85.1 单版本 smoke；探针需容忍宽容 semver 前缀                                                                                                      |
| `PI_CODING_AGENT_DIR` 的相对路径语义            | 已核验（不再 Unknown） | 适配器实现为 `resolve(configured)`（相对自己进程 cwd）→ EasyToAgents 无法可靠重现，**按 fail closed 处理**（证据：`pi-mcp-adapter/agent-dir.ts:11-25`） |
| `latest` 文档与具体安装版本的键位差异           | Unknown                | 已发现 `Per-model compaction overrides` 差异                                                                                                            |
| `--no-skills` / `--no-context-files` 运行时禁用 | Unknown                | CLI 开关，静态不可探测                                                                                                                                  |
| 适配器运行时是否真的注册了扩展                  | Unknown                | 需跑 Pi 会话并消耗额度；只能由 `packages` + 安装目录 + `pi config` 三态推断                                                                             |
| `pi config` 禁用扩展后的精确 settings 表示      | Unknown                | 未跑交互式 `pi config`；按字符串/对象+空数组/缺省三态保守处理                                                                                           |
| 适配器 2.34+ 是否保持 schema/路径稳定           | Unknown                | 2.x 已出现 `lifecycle` BREAKING；靠版本探测 + 未知字段保留 + 最低版本 fail closed                                                                       |
| `socket` 传输是否应作为受管 transport           | Unknown                | 首版走未受管字段，不新增 transport                                                                                                                      |
| npm 2.33.0 与 GitHub main 是否同源              | Unknown                | 以 npm 2.33.0 为准，升级时重新核验                                                                                                                      |

## 10. 升级回归点

- 调研期 smoke（`prompt-and-skills.md` §5、`pi-mcp-adapter.md` §7）必须固化为 fixture 测试，Pi 或适配器升级后重跑。
- 一旦 symlink 跟随、候选文件表、skills precedence、`mcpServers` 容器名或 Pi 自有文件路径变化，能力矩阵回到调研阶段；不得静默降级为「另一种写入方式」。
- 品牌资产：`https://pi.dev/favicon.svg` 的 SHA-256 `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a`（2026-09-14 抓取，MIT）作为已 bundle 基线；上游改版需按 `src/assets/brand/README.md` 流程重新抓取并更新哈希。
