# Pi 工具接入调研 —— 方向 C：MCP / Hooks / Agents（反向核验）+ 其余资源面

> **【已被取代 · 2026-09-14】** 本文档的 **MCP 结论已失效**：最初结论为 `Unsupported`，后来用户明确要求「既然 `pi-mcp-adapter` 存在就要支持」，因此 MCP 改为 **Supported（前置：适配器就绪）**。
>
> 现行权威结论见 `research/pi-mcp-adapter.md`（适配器专项合同）与 `research/verified-contract.md`（合并版）。本文档仅作为调研过程记录保留：其中 **Hooks / Agents 仍为 Unsupported**（结论未变），以及「Pi 核心无内置 MCP」的事实与第三方文件归属分析仍然有效。
> 下文出现的 `PI_MCP_UNSUPPORTED` 诊断码已作废。

- **调研方向**：C —— MCP、Hooks、Agents（子代理）三类「反向核验」（核验是否存在官方合同），以及 Pi Packages / Extensions / Themes / 安全边界等「不在现有资源模型内」的附加项。
- **调研日期**：2026-09-14
- **Pi 版本**：`@earendil-works/pi-coding-agent` **0.85.1**（本机 `package.json` 实测；文档站标注 `Latest`）
- **证据强度声明**：
  - 【强】官方文档站 `https://pi.dev/docs/latest/*` + 本机 0.85.1 随包权威副本 `~/.volta/.../pi-coding-agent/docs/*.md`（两者逐条比对一致）→ 可作为合同依据。
  - 【强-反证】官方文档站 **不存在** 的被探测页面（HTTP 404，2026-09-14 实测）→ 作为「官方未文档化」的反证。
  - 【弱-仅记录】第三方包 README / npm 元数据（`pi-mcp-adapter` 等）→ **只用于说明「谁定义了这个文件」，绝不作为 EasyToAgents 写入授权**。
  - 本方向结论一律 **fail closed**：只有官方文档明确描述的声明式配置面才可 Supported；由 TypeScript 扩展 API 或第三方私有文件提供的能力，一律 Unsupported。
- **本机权威副本根目录**（下称 `$DOCS`）：
  `/Users/zhexin/.volta/tools/image/packages/@earendil-works/pi-coding-agent/lib/node_modules/@earendil-works/pi-coding-agent/`

---

## 0. 官方 URL 探测结果（2026-09-14，HTTP 状态码实测）

| URL                                     | 状态    | 含义                                                                                          |
| --------------------------------------- | ------- | --------------------------------------------------------------------------------------------- |
| `https://pi.dev/docs/latest/`           | 200     | 文档索引：Pi 由 TypeScript extensions / skills / prompt templates / themes / pi packages 扩展 |
| `https://pi.dev/docs/latest/usage`      | 200     | Design Principles：明确不含内置 MCP、sub-agents 等                                            |
| `https://pi.dev/docs/latest/extensions` | 200     | 事件机制 = TypeScript 扩展 API                                                                |
| `https://pi.dev/docs/latest/settings`   | 200     | Resources 仅 packages/extensions/skills/prompts/themes                                        |
| `https://pi.dev/docs/latest/packages`   | 200     | packages 语义 + 安全警告 + `pi install -l` + `pi config`                                      |
| `https://pi.dev/docs/latest/security`   | 200     | Project Trust 清单 + No Built-in Sandbox                                                      |
| `https://pi.dev/docs/latest/themes`     | 200     | themes 语义                                                                                   |
| `https://pi.dev/docs/latest/skills`     | 200     | skills 语义（交叉参考）                                                                       |
| `https://pi.dev/docs/latest/hooks`      | **404** | **官方不存在 hooks 文档页**                                                                   |
| `https://pi.dev/docs/latest/mcp`        | **404** | **官方不存在 MCP 文档页**                                                                     |
| `https://pi.dev/docs/latest/agents`     | **404** | **官方不存在 agents 文档页**                                                                  |
| `https://pi.dev/docs/latest/subagents`  | **404** | 同上                                                                                          |

> HTTP 探测命令：`curl -s -o /dev/null -w "%{http_code}" https://pi.dev/docs/latest/<page>`（经系统代理 `127.0.0.1:10808`）。

---

## 1. 证据表（官方 URL + 访问日期 + 引文摘要）

### 1.1 Pi 官方合同（强证据）

| #   | 来源（URL，访问日期 2026-09-14）                            | 版本   | 引文摘要                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| --- | ----------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| O1  | <https://pi.dev/docs/latest/>                               | 0.85.1 | “Pi is a minimal terminal coding harness. It is designed to stay small at the core while being extended through **TypeScript extensions, skills, prompt templates, themes, and pi packages**.”（本机副本 `$DOCS/docs/index.md`）                                                                                                                                                                                                                                                                                                                                                                                                       |
| O2  | <https://pi.dev/docs/latest/usage>                          | 0.85.1 | “It **intentionally does not include built-in MCP, sub-agents**, permission popups, plan mode, to-dos, or background bash. You can build or install those workflows as **extensions or packages**, or use external tools…”（本机副本 `$DOCS/docs/usage.md:309`）                                                                                                                                                                                                                                                                                                                                                                       |
| O3  | <https://pi.dev/> （首页）                                  | —      | “**No MCP.** Build CLI tools with READMEs (see Skills), or build an extension that adds MCP support.” / “**No sub-agents.** … Spawn Pi instances via tmux, or build your own with extensions, or install a package that does it your way.”（本机副本 `$DOCS/README.md:499-501`）                                                                                                                                                                                                                                                                                                                                                       |
| O4  | <https://pi.dev/> （首页，What's possible）                 | —      | 把 `Sub-agents and plan mode`、`MCP server integration`、`Permission gates` 等列在「**扩展能做到什么**」列表，而非核心功能。（本机副本 `$DOCS/README.md:387-395`）                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| O5  | <https://pi.dev/docs/latest/extensions>                     | 0.85.1 | “Extensions are **TypeScript modules** that extend pi's behavior. They can subscribe to lifecycle events, register custom tools…”；“`pi.on("tool_call", async (event, ctx) => {...})`”；“Extensions are loaded via jiti, so TypeScript works without compilation.”（本机副本 `$DOCS/docs/extensions.md:1-30,60-90`）                                                                                                                                                                                                                                                                                                                   |
| O6  | <https://pi.dev/docs/latest/extensions#extension-locations> | 0.85.1 | 自动发现位置仅：`~/.pi/agent/extensions/*.ts`、`~/.pi/agent/extensions/*/index.ts`、`.pi/extensions/*.ts`、`.pi/extensions/*/index.ts`；并警告 “Extensions run with your full system permissions and can execute arbitrary code.”                                                                                                                                                                                                                                                                                                                                                                                                      |
| O7  | <https://pi.dev/docs/latest/settings>                       | 0.85.1 | “### Resources … These settings define where to load **extensions, skills, prompts, and themes** from.” 表内键只有 `packages / extensions / skills / prompts / themes / enableSkillCommands`；**没有 `hooks` 键、没有 `agents` 键、没有 `mcp` 键**。（本机副本 `$DOCS/docs/settings.md:277-295`）                                                                                                                                                                                                                                                                                                                                      |
| O8  | <https://pi.dev/docs/latest/packages>                       | 0.85.1 | “> **Security:** Pi packages run with **full system access**. Extensions execute arbitrary code, and skills can instruct the model to perform any action including running executables. Review source code before installing third-party packages.”；“By default, install and remove write to user settings (`~/.pi/agent/settings.json`). Use **`-l`** to write to project settings (`.pi/settings.json`).”；“User installs go under `~/.pi/agent/npm/`.”；“Project installs go under `.pi/npm/`.”；“Use **`pi config`** to enable or disable extensions, skills, prompt templates, and themes…”（本机副本 `$DOCS/docs/packages.md`） |
| O9  | <https://pi.dev/docs/latest/security>                       | 0.85.1 | 需 trust 的项目资源清单为 `.pi/settings.json`、`.pi/extensions`、`.pi/skills`、`.pi/prompts`、`.pi/themes`、`.pi/SYSTEM.md`、`.pi/APPEND_SYSTEM.md`、项目 `.agents/skills` —— **清单中没有 `.pi/agents`、没有 `.pi/mcp.json`**；“## No Built-in Sandbox … Extensions are TypeScript modules that run with the same permissions.”                                                                                                                                                                                                                                                                                                       |
| O10 | <https://pi.dev/docs/latest/themes>                         | 0.85.1 | “Global: `~/.pi/agent/themes/*.json`；Project: `.pi/themes/*.json`（trusted 后）；Packages / Settings / CLI”                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| O11 | <https://pi.dev/docs/latest/mcp>                            | —      | **HTTP 404**：官方不存在 MCP 文档页（反向证据）                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| O12 | <https://pi.dev/docs/latest/hooks>                          | —      | **HTTP 404**：官方不存在 hooks 文档页（反向证据）                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| O13 | <https://pi.dev/docs/latest/agents>                         | —      | **HTTP 404**：官方不存在 agents 文档页（反向证据）                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

### 1.2 第三方包私有面（弱证据，仅用于区分「谁定义了配置」）

| #   | 来源（URL，访问日期 2026-09-14）                      | 版本   | 引文摘要                                                                                                                                                                                                                                                         |
| --- | ----------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T1  | <https://pi.dev/packages/pi-mcp-adapter>              | 2.33.0 | 第三方包页面：类型 `extension, skill`；作者 `nicopreme`；“Pi packages can execute code and influence agent behavior. Review the source before installing third-party packages.”；README 标题 “**Pi MCP Adapter** — Use MCP servers with Pi…”                     |
| T2  | <https://github.com/nicobailon/pi-mcp-adapter>        | 2.33.0 | 第三方仓库（`package.json` author `Nico Bailon`，`keywords` 含 `pi-package`、`mcp`）。本机安装副本：`~/.pi/agent/npm/node_modules/pi-mcp-adapter/`                                                                                                               |
| T3  | 本机第三方包源码（非官方）                            | 2.33.0 | `pi-mcp-adapter/agent-dir.ts`：`getAgentDir()` = `join(homedir(), getConfigDirName(), "agent")`；`config.ts`：`const PROJECT_PI_CONFIG_NAME = "mcp.json"`、`getAgentPath("mcp.json")`。**`~/.pi/agent/mcp.json` 与 `.pi/mcp.json` 由该扩展自行拼接、自行读取。** |
| T4  | <https://github.com/nicobailon/pi-mcp-adapter> README | 2.33.0 | 配置优先级（该插件私有）：`~/.config/mcp/mcp.json` → `~/.agents/mcp.json` → `~/.agents/mcp/mcp.json` → `<Pi agent dir>/mcp.json`（默认 `~/.pi/agent/mcp.json`）→ `.mcp.json` → `.pi/mcp.json`。                                                                  |
| T5  | 本机第三方包 README（context-mode）                   | —      | 另一第三方包 context-mode 同样文档化 “Add to `~/.pi/agent/mcp.json` (or `.pi/mcp.json` for project-level)”。两份第三方文档都在描述**同一插件的私有文件**，不是 Pi 核心。                                                                                         |
| T6  | 本机 Pi 核心 dist（反证）                             | 0.85.1 | 在 `$DOCS/dist/` 全量 grep `mcp.json`、`mcpServers`、`mcp-server` **未命中**任何核心配置读取；唯一 `mcp` 命中是 Anthropic OAuth scope 字符串 `user:mcp_servers`（无关）。→ 核心不读 MCP 配置。                                                                   |

### 1.3 本仓库 `.pi/agents/*.md` 的真实来源（强证据，本机）

| #   | 证据路径 + 行号                                                            | 内容摘要                                                                                                                                                         |
| --- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A1  | `.pi/agents/trellis-implement.md:1-6`                                      | YAML frontmatter 仅 `name` / `description` / `tools`：“`name: trellis-implement` … `tools: read, write, edit, bash, find, grep`”。**没有 Pi 官方 schema 依据。** |
| A2  | `.pi/agents/trellis-research.md:1-6`                                       | 同上，`name: trellis-research` / `tools: read, write, bash, find, grep`。                                                                                        |
| A3  | `.pi/extensions/trellis/index.ts:1244`                                     | `return existsSync(join(root, ".pi", "agents",`${agent}.md`));` —— 由 **Trellis 扩展**决定 agents 目录位置。                                                     |
| A4  | `.pi/extensions/trellis/index.ts:1253, 1520`                               | `const raw = readText(join(root, ".pi", "agents",`${agent}.md`));` —— 扩展自行解析文件。                                                                         |
| A5  | `.pi/extensions/trellis/index.ts:1737-1743`                                | 注册自定义工具 `name: "trellis_subagent"`，描述 “Run a Trellis project sub-agent…” —— 子代理是**扩展注册的工具**，不是 Pi 核心 feature。                         |
| A6  | `.pi/extensions/trellis/index.ts:1791`                                     | 报错文案 “`trellis_subagent` is only for Trellis workflow agents with a definition file in `.pi/agents/`.” —— 自建约定自证。                                     |
| A7  | `.agents/skills/trellis-meta/references/platform-files/platform-map.md:23` | “Pi Agent → `.pi/agents/`；`.pi/extensions/trellis/`（native `trellis_subagent` tool）” —— Trellis 自己的平台映射，确认是 Trellis 约定。                         |
| A8  | `security.md` trust 清单（O9）                                             | Pi 官方 trust 清单 **不含 `.pi/agents`**，证明 Pi 核心根本不认识这个目录。                                                                                       |

> **结论**：`.pi/agents/*.md` 是 **Trellis 扩展自建约定**（frontmatter `name/description/tools`，由 `.pi/extensions/trellis/index.ts` 读取并注册为 `trellis_subagent` 工具），**不是 Pi 官方合同**。任何「Pi 支持 Agents」的结论都无官方依据。

---

## 2. 逐条结论（Sub-结论 + 来源）

### 2.1 MCP —— 结论：**Unsupported**（全局 / 项目，导入 / 应用）

1. **Pi 核心没有内置 MCP 客户端，也没有 MCP 配置面。**
   - 官方 usage 的 Design Principles 明确 “intentionally does not include built-in MCP”（O2）；首页 Philosophy “No MCP”（O3）；`/docs/latest/mcp` 404（O11）。
   - 本机 0.85.1 dist 全量 grep 无 MCP 配置读取（T6）。
2. **网络流传的 `~/.pi/agent/mcp.json` / `.pi/mcp.json` 是第三方扩展 `pi-mcp-adapter` 的私有文件。**
   - 该包在包页面自我标注为 `extension, skill`，非官方 core（T1/T2）；其源码自行拼出这两个路径（T3），并定义了一套**包含非 Pi 标准文件的优先级链**（`~/.config/mcp/mcp.json`、`~/.agents/mcp.json`、`.mcp.json` 等，T4）。
   - 另一第三方包 context-mode 也文档化同一路径（T5），进一步说明这是插件生态约定，不代表 Pi 官方。
3. **对 EasyToAgents 的含义：不能接管。**
   - 项目合同：第三方/逆向配置不能单独授权写入。
   - 该配置面**归属一个可选插件**；用户未装插件时文件无意义，装了插件后优先级链还可能被 `~/.config/mcp/mcp.json` / `.mcp.json` 覆盖，写入 `.pi/mcp.json` 不保证生效、语义由插件版本决定。
   - 因此 MCP 行判 **Unsupported**（有意不支持，非仅信息缺失），**不是 Unknown**。建议诊断码 **`PI_MCP_UNSUPPORTED`**。

### 2.2 Hooks —— 结论：**Unsupported**（全局 / 项目，导入 / 应用）

1. **Pi 的事件机制是 TypeScript 扩展 API，不是声明式 hook 文件。**
   - 官方 extensions 文档：Extensions 是 TypeScript 模块，通过 `pi.on("tool_call", handler)` 等订阅生命周期事件（O5）；事件名是代码里的字符串，不是 `settings.json` 里的配置键。
2. **官方没有可声明式配置的 hook 文件（类似 Claude `settings.json.hooks` 的键）。**
   - `/docs/latest/hooks` **404**（O12）；settings 文档 Resources 段只有 `packages/extensions/skills/prompts/themes`，**无 `hooks` 键**（O7）；security trust 清单也无 hooks 文件（O9）。
3. **与 EasyToAgents command-only 统一事件模型不兼容。**
   - 对照 OpenCode 判例：OpenCode 的 plugin 回调返回 hooks object，被判 **Unsupported**（`OPENCODE_HOOKS_UNSUPPORTED`），理由正是「插件回调不是当前 command-only 合同」。
   - Pi 完全同构：能力由可执行 TypeScript 回调提供，无法映射为「文件型、command-only、可快照回滚」的统一 HookEvent。
   - 因此 Hooks 判 **Unsupported**，建议诊断码 **`PI_HOOKS_UNSUPPORTED`**。

### 2.3 Agents（子代理）—— 结论：**Unsupported**（全局 **且** 项目，导入 / 应用）

1. **Pi 官方没有 `.pi/agents/*.md` 子代理配置面。**
   - `/docs/latest/agents` 404（O13）；首页把 sub-agents 归为「扩展能做到什么」（O4）；官方 Philosophy “No sub-agents”（O3）；README “Sub-agents and plan mode” 列在 extensions 能力清单（O4）。
   - 官方 trust 清单、Resources 清单都 **没有 agents**（O7/O9）。
2. **本仓库 `.pi/agents/trellis-*.md` 是 Trellis 扩展自建约定，不是 Pi 核心合同**（A1–A8）：frontmatter 只有 `name/description/tools`，由 `.pi/extensions/trellis/index.ts` 读取（`:1244/1253/1520`）并注册 `trellis_subagent` 工具（`:1737`）。
3. **官方文档里 “Sub-agents” 只出现在「扩展能做到什么」列表**（O4），没有格式规范、没有目录规范、没有 schema。
4. **因此 Agents 全局与项目均判 Unsupported**，建议诊断码 **`PI_AGENTS_UNSUPPORTED`**。理由：唯一可观察到的是本仓库某扩展的私有约定，既非官方合同，也无法跨扩展复用（换个扩展就换目录/格式），写入会破坏该扩展私有契约。

### 2.4 Pi Packages / Extensions / Themes —— 结论：**不纳入本期资源模型**

官方语义（O6/O7/O8/O10）：

| 路径                                                            | 官方语义                                                            | 是否对应现有 ArtifactKind |
| --------------------------------------------------------------- | ------------------------------------------------------------------- | ------------------------- |
| `~/.pi/agent/extensions/*.ts`、`.pi/extensions/*.ts`            | 自动发现的 **TypeScript 可执行扩展**                                | ❌ 否                     |
| `~/.pi/agent/themes/*.json`、`.pi/themes/*.json`                | TUI 主题 JSON                                                       | ❌ 否                     |
| `~/.pi/agent/npm/`、`.pi/npm/`                                  | npm/git 包安装目录（`pi install`，`-l` 写项目 `.pi/settings.json`） | ❌ 否                     |
| `~/.pi/agent/settings.json` / `.pi/settings.json` 的 `packages` | 包来源数组                                                          | ❌ 否                     |
| `pi config`                                                     | 启用/禁用全局或项目级 extensions/skills/prompts/themes              | ❌ 否                     |
| `~/.pi/agent/sessions/`、会话 JSONL                             | 会话运行时数据                                                      | ❌ 否                     |

**为什么不应硬塞进现有资源模型（provider/prompt/mcp/skill/hook/agent）**：

1. **extensions 是可执行代码 + 完整系统权限**（O6/O8/O9）。现有六类 ArtifactKind 都是「声明式数据 + 快照/journal 可回滚」；写入 `extensions/*.ts` 等于代表用户安装可执行代码，风险等级、回滚边界、审计语义完全不同。
2. **packages 是供应链边界**：`pi install` 会跑 npm/git、执行 `npm install`、从远程拉代码（O8）。EasyToAgents 明确定位「不引入专有运行时、不做代码执行」，故不代跑 `pi install`，也不接管 `~/.pi/agent/npm`、`.pi/npm`。
3. **themes 是纯 UI JSON**，不属于用户跨工具迁移的核心资产；纳入会稀释资源模型、增加 schema 维护面而收益极低。
4. **sessions / session JSONL 是运行时状态**，不是可分配、可同步的「中央意图」。读取会带来隐私与体量问题，写入更是越界。
5. 结论：这些一律 **「本期不纳入」**（OutOfScope，不是 Unsupported 能力）。**不新增 ArtifactKind**；如未来要支持，必须单独立项、单独安全评审。

### 2.5 安全边界（官方原文）及其对 EasyToAgents 的意义

**官方原文（O8，packages.md）**：

> “**Security:** Pi packages run with **full system access**. Extensions execute arbitrary code, and skills can instruct the model to perform any action including running executables. Review source code before installing third-party packages.”

**官方原文（O6，extensions.md）**：

> “**Security:** Extensions run with your full system permissions and can execute arbitrary code. Only install from sources you trust.”

**官方原文（O9，security.md）**：

> “Pi does not include a built-in sandbox. … **Extensions are TypeScript modules that run with the same permissions.** … Project trust is only an input-loading guard. It prevents a repository from silently changing pi's settings or extensions before you approve it. It does not make untrusted code, untrusted prompts, or untrusted model output safe.”

**对 EasyToAgents 定位的含义**：

- 现有资源模型写的是**声明式配置**，可 diff、可快照、可回滚。Pi 的 extensions/packages 属于**代码执行面**，与「不引入专有运行时、不做代码执行」的定位直接冲突。
- 因此：**不安装扩展/包、不执行 `pi install`、不写入 `.ts`/可执行文件、不代管 `~/.pi/agent/npm|extensions`**。
- Pi 的 Project Trust 只是「加载前询问」，不是沙箱；EasyToAgents 不能把「Pi 会 trust」当成写入安全依据。
- 反向推论：任何需要「让 Pi 执行我们写入的东西」的接入方案都应被否决；只有纯声明式、且 Pi 核心文档化的配置面才可能 Supported。

---

## 3. fail-closed 判例对齐（逐条对照 OpenCode）

**OpenCode 判例回顾**（本仓库证据）：

- `.trellis/spec/backend/opencode-adapter-guidelines.md:53` — “Hook RPCs, assignments, imports, descriptors, and writes return the stable `OPENCODE_HOOKS_UNSUPPORTED` diagnostic.”
- `:64` — “Any OpenCode Hook request → `OPENCODE_HOOKS_UNSUPPORTED`; zero external writes.”
- `src-tauri/src/hooks/service_core.rs:112-118` — `ensure_hooks_supported()`：`if tool == Tool::Opencode { return Err(... "OPENCODE_HOOKS_UNSUPPORTED") }`
- `:696-704` / `:766-770` — `hook_target_descriptor` 与 `build_desired_projection` 同样对 Opencode fail closed。
- `src-tauri/src/db/migrations/0019_opencode_tool_support.sql:27-31` — DB 层 `managed_targets` 的 tool CHECK 显式把 OpenCode 限制在 `('provider','prompt','mcp','skill')`，**Hook rows 在数据库层被拒**。

**逐条对齐：Pi 应如何被拒绝**

| 维度   | OpenCode 判例                                                                          | Pi 对应判定                                                                                  | 依据                 |
| ------ | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- | -------------------- |
| MCP    | OpenCode MCP 是**官方 JSON/JSONC `mcp` 根**，故 Supported                              | Pi 无官方 MCP 合同，**唯一文件由第三方插件定义** → **Unsupported**                           | O2/O3/O11 + T1–T6    |
| Hooks  | 插件回调返回 hooks object，非 command-only → **Unsupported**                           | 事件由 TypeScript `pi.on(...)` 回调提供，非声明式文件 → **Unsupported**（同构）              | O5/O7/O12            |
| Agents | OpenCode 有**官方** `<root>/.opencode/agents` Markdown 合同 → Supported（项目 fix 后） | Pi 无官方 agents 目录；`.pi/agents` 是 Trellis 扩展私有约定 → **Unsupported**（全局 + 项目） | O4/O7/O9/O13 + A1–A8 |

**必须的两道边界（服务层 + DB 层）**

1. **服务层（第一道，给稳定诊断码）**
   - 在任何 `discover` / `hook_target_descriptor` / `build_desired_projection` / `set_*_assignment` / `preview_*` / `apply_*` / `import` 入口，对 `Tool::Pi` 的 `ArtifactKind::Mcp | Hook | Agent` 直接返回 `AppError::invalid_input(...)`。
   - 建议诊断码：`PI_MCP_UNSUPPORTED`、`PI_HOOKS_UNSUPPORTED`、`PI_AGENTS_UNSUPPORTED`。
   - 保持与 OpenCode 相同的结构（专用 `ensure_*_supported(tool)` 帮助函数），**零外部写入**，不得先探测再写。
2. **DB 层（第二道，迁移金丝雀）**
   - 新增 Pi 到 `Tool` 枚举后，必须新增/扩写迁移，对 `managed_targets` 的 tool CHECK **显式限制 Pi 允许的 artifact 集合**，例如：
     `(tool != 'pi' OR artifact_kind IN (<Provider/Prompt/Skills 调研员结论合并后的允许集，本方向要求排除 'mcp'、'hook'、'agent'>))`
   - 不得因为「Pi 枚举存在」就放开 artifact；沿用 0014/0019/0022 的 `writable_schema` 原地修订 + `instr(...) > 0` 锚点守卫 + 迁移金丝雀测试模式。
   - 服务层与 DB 层必须**同时**拒绝：服务层保证用户可读诊断，DB 层保证任何绕过路径（直接仓储调用、旧库重开、未来新增 RPC）也无法落库。
3. **交叉层单一来源**
   - `src-tauri/src/domain/mod.rs:277-292` 的 `tool_capabilities()` 是能力矩阵唯一跨层来源，前端从生成的 bindings 读取。Pi 的 MCP/Hooks/Agents 必须在该处为 `false`；不要只在前端隐藏按钮。

---

## 4. Pi 完整能力矩阵（本方向负责 MCP / Hooks / Agents 三行 + 附加项）

取值：`Supported` / `Unsupported` / `Unknown` / `ToolNotInstalled` / `OutOfScope`（不在现有资源模型内）。

| ArtifactKind                 | Scope          | Import      | Apply       | 结论        | 证据                              | 备注                                                                                                                |
| ---------------------------- | -------------- | ----------- | ----------- | ----------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| **MCP**                      | Global         | Unsupported | Unsupported | Unsupported | O2/O3/O11, T1–T6                  | 文件归属第三方插件 `pi-mcp-adapter`，优先级链含非 Pi 文件；`PI_MCP_UNSUPPORTED`                                     |
| **MCP**                      | Project        | Unsupported | Unsupported | Unsupported | 同上                              | 同上                                                                                                                |
| **Hooks**                    | Global         | Unsupported | Unsupported | Unsupported | O5/O7/O12                         | 事件机制是 TS 扩展 API；`PI_HOOKS_UNSUPPORTED`                                                                      |
| **Hooks**                    | Project        | Unsupported | Unsupported | Unsupported | 同上                              | 同上                                                                                                                |
| **Agents**                   | Global         | Unsupported | Unsupported | Unsupported | O3/O4/O7/O9/O13, A1–A8            | `.pi/agents` 是 Trellis 扩展私有约定；`PI_AGENTS_UNSUPPORTED`                                                       |
| **Agents**                   | Project        | Unsupported | Unsupported | Unsupported | 同上                              | 同上                                                                                                                |
| Provider                     | Global/Project | （待合并）  | （待合并）  | （待合并）  | —                                 | 由 Provider 调研员负责；本方向不表态                                                                                |
| Prompt                       | Global/Project | （待合并）  | （待合并）  | （待合并）  | —                                 | 由 Prompt 调研员负责                                                                                                |
| Skill                        | Global/Project | （待合并）  | （待合并）  | （待合并）  | —                                 | 由 Skills 调研员负责；注意 Skill 是 Pi **官方**声明式面（`/docs/latest/skills` 200），但本方向不判定其 Import/Apply |
| **Extensions**               | Global/Project | OutOfScope  | OutOfScope  | OutOfScope  | O5/O6/O9                          | 可执行 TypeScript；不纳入资源模型                                                                                   |
| **Themes**                   | Global/Project | OutOfScope  | OutOfScope  | OutOfScope  | O10                               | 纯 UI，不纳入                                                                                                       |
| **Packages / npm-git**       | Global/Project | OutOfScope  | OutOfScope  | OutOfScope  | O8                                | 供应链 + 代码执行；不代跑 `pi install`                                                                              |
| **Sessions / session JSONL** | —              | OutOfScope  | OutOfScope  | OutOfScope  | O9 相关 + `~/.pi/agent/sessions/` | 运行时状态，不纳入                                                                                                  |

> `ToolNotInstalled` 在本方向不适用：能力判定依据是**官方合同是否存在**，而不是本机是否装了 `pi`（本机确实装了 0.85.1 与第三方 `pi-mcp-adapter`）。合同不存在时，即使工具已装也应为 `Unsupported`，不能因探测到插件而降级为「可用」。

### 4.1 交叉验证意见（供合并）

- **对 Provider 行**：Pi 的 provider 存在官方声明式面（`models.json` / `~/.pi/agent/models.json`，见首页 “Add custom providers and models via models.json or extensions”），但请 Provider 调研员确认「extensions 路线是否与声明式路线混用」。本方向建议：只有 `models.json` 这类官方面可 Supported；`registerProvider()` 属扩展 API，应排除。
- **对 Prompt 行**：`~/.pi/agent/prompts/`、`.pi/prompts/` 出现在 settings Resources（O7）与 prompt-templates 官方文档，属官方声明式面。
- **对 Skills 行**：Skills 是官方声明式面（`/docs/latest/skills` 200），且官方明确把「CLI tools with READMEs」作为 MCP 的替代（O3）。本方向无异议。
- **统一要求**：无论其他三行结论如何，Pi 的 MCP/Hooks/Agents 三行必须是 `Unsupported`；DB 迁移允许集必须显式排除这三类，不能只靠服务层。

---

## 5. 「不在范围内」段（明确列出并给理由）

以下能力**本期不进入 EasyToAgents 资源模型**，也不新增 ArtifactKind：

| 对象                                                                     | 官方语义（来源）                                                   | 不纳入理由                                                          |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------- |
| Extensions（`~/.pi/agent/extensions/`、`.pi/extensions/`）               | TypeScript 模块，自动发现（O5/O6）                                 | 可执行代码 + 完整系统权限；与「不做代码执行」定位冲突；无法快照回滚 |
| Pi Packages（`packages` / `pi install` / `~/.pi/agent/npm` / `.pi/npm`） | npm/git 资源包，`-l` 写项目设置（O8）                              | 供应链边界；安装即跑 npm install 与第三方代码；不应代跑             |
| Themes（`~/.pi/agent/themes/`、`.pi/themes/`）                           | TUI 颜色 JSON（O10）                                               | 纯 UI 资产，非跨工具可迁移核心；纳入收益低、维护面大                |
| `pi config` 的启用/禁用状态                                              | 全局/项目资源开关（O8）                                            | 是 Pi 私有运行时状态，不映射统一资源意图                            |
| Sessions / `~/.pi/agent/sessions/` / session JSONL                       | 会话树与运行时历史（`docs/sessions.md`、`docs/session-format.md`） | 运行时状态 + 隐私体量；非中央意图                                   |
| `.pi/agents/*.md`                                                        | **非官方**，Trellis 扩展私有约定（A1–A8）                          | 非 Pi 合同，换扩展即失效；写入会破坏该扩展私有契约                  |
| `~/.pi/agent/mcp.json` / `.pi/mcp.json` / `.mcp.json`                    | **非官方**，`pi-mcp-adapter` 私有（T1–T6）                         | 第三方/逆向配置，不能单独授权写入                                   |
| `~/.pi/agent/trust.json`、`auth.json`、`models-store.json`               | 运行时/凭证状态（O9 等）                                           | 凭证与信任状态绝不读写                                              |

---

## 6. 建议（供 PRD / design 合并）

1. **能力矩阵**：Pi 的 MCP/Hooks/Agents 三行全 `Unsupported`（Global/Project × Import/Apply）。
2. **诊断码**：`PI_MCP_UNSUPPORTED`、`PI_HOOKS_UNSUPPORTED`、`PI_AGENTS_UNSUPPORTED`，命名对齐 `OPENCODE_HOOKS_UNSUPPORTED`。
3. **双边界拦截**：服务层 `ensure_*_supported(Tool::Pi)` + DB 迁移 tool CHECK 显式排除 `mcp|hook|agent`；加迁移金丝雀测试。
4. **不新增 ArtifactKind**：Extensions/ Themes / Packages / Sessions 一律 OutOfScope，不硬塞现有模型。
5. **安全红线**：不安装扩展/包、不写入可执行文件、不代跑 `pi install`、不读写 `auth.json`/`trust.json`。
6. **待合并**：Provider/Prompt/Skills 三行结论由另外两位调研员给出后再补齐矩阵；DB 允许集以合并结果为准（但本方向要求至少排除 mcp/hook/agent）。
