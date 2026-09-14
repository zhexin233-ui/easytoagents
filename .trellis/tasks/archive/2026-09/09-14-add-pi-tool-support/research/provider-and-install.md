# Pi 接入调研 — 方向 A：Provider + 安装探针 + 配置根/信任边界

- 调研方向：Provider（模型/服务商）配置面、凭据语义、项目信任、安装探针、配置根覆盖与敏感字段。
- 调研日期：2026-09-14
- 目标工具：Pi（`@earendil-works/pi-coding-agent`），本机版本 **0.85.1**
- 目标任务：`.trellis/tasks/09-14-add-pi-tool-support`
- 取证方式：`smart-search fetch/search`（硬要求）+ 本机 0.85.1 权威副本 + 本机安装产物/grep 交叉校验。

---

## 0. 证据强度声明

本文件中的每条结论都标注证据来源。可信度分级如下：

| 级别                               | 含义                                                                                                                                | 本文件中的来源                                                                                                  |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| **E1 官方 URL**                    | 官方文档站 `https://pi.dev/docs/latest/...`（2026-09-14 通过 `smart-search fetch` 抓取，`--format markdown`）。可授权 schema/语义。 | providers / models / settings / security / environment-variables / quickstart / usage / custom-provider / index |
| **E2 官方运维文档（本机 0.85.1）** | 安装包自带 `docs/*.md`（与 E1 同名，版本 0.85.1）。用于把 E1 锚定到实际安装版本；不能替代官方 URL 授权。                            | `.../@earendil-works/pi-coding-agent/docs/*.md`                                                                 |
| **E3 本机实测**                    | 本机安装布局、`pi --version` 输出、`~/.pi/agent/*` 实机结构、dist bundle grep。                                                     | 只用于交叉校验与探针设计；不单独授权路径/schema。                                                               |
| **L 线索（第三方）**               | 第三方博客/镜像/社区仓库。**不能单独授权写入**，仅作线索或反证。                                                                    | mintlify 镜像、社区 GitHub 仓库、社区 macOS 前端                                                                |

**重要版本偏差警告（必须写进代码注释/证据表）**：
`pi.dev/docs/latest` 页面顶部带 `Latest` 标记，且内容与本机 0.85.1 副本**并不完全一致**。实测差异举例：

- 抓取的 `settings.md` 包含小节 `#### Per-model compaction overrides`，本机 0.85.1 `docs/settings.md` 中不存在（本机 369 行）。
- 本机 `~/.pi/agent/settings.json` 中存在被 Pi 自管的键 `lastChangelogVersion`，官方 settings 文档表**未列出**。

结论：官方 URL 证据用于确认「能力与语义稳定存在」，但具体键位的存在性/默认值必须在 **0.85.1 本机副本 + 实机 fixture** 上复核。任何官方文档未列出的键（如 `lastChangelogVersion`）视为 Pi 自管，不得由 EasyToAgents 写入。

---

## 1. 结论速览

### 1.1 Provider 能力矩阵（方向 A 范围）

| Artifact                            | Scope   | Import          | Apply           | 结论                                                    |
| ----------------------------------- | ------- | --------------- | --------------- | ------------------------------------------------------- |
| Provider（`models.json`）           | Global  | **Supported**   | **Supported**   | 官方文档化的用户可编辑配置面；schema 明确               |
| Provider                            | Project | **Unsupported** | **Unsupported** | 不存在项目级 Provider 配置；`.pi/` 下只有 settings/资源 |
| Provider 凭据（`auth.json`）        | Global  | **Unsupported** | **Unsupported** | 产品选择：永不导入/永不写入（对照 OpenCode 合同）       |
| Provider 凭据                       | Project | **Unsupported** | **Unsupported** | 同上，且 Pi 本身无项目级凭据文件                        |
| `models-store.json`（模型目录缓存） | Global  | **Unsupported** | **Unsupported** | Pi 自管缓存，`pi update --models` 会重写                |
| 安装/版本探针（CLI）                | Global  | —               | —               | 只有 CLI，无官方 macOS `.app`；`pi --version`           |

### 1.2 关键风险 3 条（详见 §5）

1. **配置根被覆盖 → 静默无效**：若用户设置了 `PI_CODING_AGENT_DIR`，EasyToAgents 写 `~/.pi/agent/models.json` 会被 Pi 完全忽略。探针/写入必须解析该变量（E1）。
2. **`models-store.json` 与 `models.json` 职责混淆**：`models-store.json` 是 Pi 自管的目录缓存（含 `etag/checkedAt/lastModified`），`pi update --models` 会刷新它。任何把模型清单写进 store 的做法都会被 Pi 覆盖（E1 + E3）。
3. **凭据泄漏与优先级错觉**：`models.json` 允许内联明文 `apiKey`，也允许 `$ENV`/`!command`；而 `auth.json` 凭据优先级高于 `models.json`。EasyToAgents 必须脱敏 selector，且永不执行 `!command`、永不展开 `$ENV`、永不读写 `auth.json`。

---

## 2. 逐题结论（Q1–Q7）

### Q1. `~/.pi/agent/models.json` 是官方文档化的用户可编辑 Provider 配置面吗？schema 与职责？

**结论：是（E1，官方明确）。`models.json` 是唯一官方文档化的用户可编辑 Provider 配置面；`models-store.json` 是 Pi 自管缓存。**

**官方 URL 证据**

- [https://pi.dev/docs/latest/models](https://pi.dev/docs/latest/models)（访问 2026-09-14）
  > "Add custom providers and models (Ollama, vLLM, LM Studio, proxies) via `~/.pi/agent/models.json`."
  > "The file reloads each time you open `/model`. Edit during session; no restart needed."
- [https://pi.dev/docs/latest/providers](https://pi.dev/docs/latest/providers)（访问 2026-09-14）
  > "Built-in catalogs ship with pi; configured providers may refresh newer catalogs and cache them in `~/.pi/agent/models-store.json` for offline use."
  > 凭据解析顺序（Resolution Order）：1. CLI `--api-key` → 2. `auth.json` → 3. 环境变量 → 4. **Custom provider keys from `models.json`**。
- [https://pi.dev/docs/latest/custom-provider](https://pi.dev/docs/latest/custom-provider)（访问 2026-09-14）
  > "Pi composes `models.json` overrides above registered native providers."
  > 官方推荐：**数据型自定义 provider（Ollama/LM Studio/vLLM/任意 OpenAI 兼容端点）用 `models.json`；需要自定义 API 实现或 OAuth 流程时用 extension `pi.registerProvider()`**（见 custom-provider.md "Via extensions"）。

**schema（E1，providers.md/models.md 明确）**

顶层：`{ "providers": { "<provider-id>": ProviderConfig } }`

`ProviderConfig`（models.md "Provider Configuration"）：

| 字段             | 类型    | 说明                                                                                                                                                     |
| ---------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `baseUrl`        | string  | API 端点                                                                                                                                                 |
| `api`            | string  | `openai-completions` / `openai-responses` / `anthropic-messages` / `google-generative-ai`（provider 级默认，可被 model 覆盖）                            |
| `apiKey`         | string  | 可选；支持 literal、`$ENV_VAR`/`${ENV_VAR}`、`!command`、`$$`/`$!` 转义                                                                                  |
| `oauth`          | string  | 目前仅 `"radius"`，需配合网关 `baseUrl`                                                                                                                  |
| `headers`        | object  | 自定义 header；值同样支持 `$ENV`/`!command`                                                                                                              |
| `authHeader`     | boolean | `true` 时自动加 `Authorization: Bearer <apiKey>`                                                                                                         |
| `models`         | array   | 模型数组，每项至少 `id`；见下                                                                                                                            |
| `modelOverrides` | object  | 对 built-in / extension 模型做局部覆盖，支持 `name, reasoning, thinkingLevelMap, input, cost, contextWindow, maxTokens, samplingParams, headers, compat` |

`ModelConfig` 关键字段（models.md "Model Configuration"）：`id`（必填）、`name`、`api`、`reasoning`、`thinkingLevelMap`、`input`、`contextWindow`（默认 128000）、`maxTokens`（默认 16384）、`samplingParams`、`cost`、`compat`。

内置 provider 覆盖语义（E1，models.md "Overriding Built-in Providers"）：

- 只给 `baseUrl`/`headers` 且**不给 `models`** → 保留全部内置模型，仅改端点；
- 给 `models` → 按 `id` upsert：内置模型保留，同 `id` 替换，新 `id` 追加。
- ⚠️ 因此 EasyToAgents 的写入**不能整体替换 `providers.<id>`**，否则会丢失用户其它字段/内置合并语义。

**`models.json` vs `models-store.json` 职责（E1 + E3）**

|                    | `models.json`                                              | `models-store.json`                                                               |
| ------------------ | ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| 官方定位           | 用户自定义 provider/model 配置（唯一用户可编辑）           | "cache them ... for offline use"（Pi 自管缓存）                                   |
| 写入者             | 用户 / EasyToAgents（Apply）                               | Pi（`/login`、catalog 刷新、`pi update --models`）                                |
| 本机实测结构（E3） | `{providers: {cc: {baseUrl, api, apiKey, models: [...]}}}` | `{ "openai-codex": {models:[...], checkedAt, lastModified, etag}, "xai": {...} }` |
| 权限（E3）         | `-rw-r--r--`（0644）                                       | `-rw-------`（0600）                                                              |
| EasyToAgents 处理  | 可 Import/Apply                                            | **不读不写**（Unsupported）                                                       |

`models-store.json` 是 catalog 缓存 → 结论：**Provider 的 Apply 目标只能是 `models.json`**。

**官方推荐注册路径**：数据配置 → `models.json`（E1）；需要编程式 API/OAuth → extension `pi.registerProvider()`（E1）。两者可叠加，`models.json` 覆盖层在 extension 注册的原生 provider 之上（E1 custom-provider.md）。

---

### Q2. 是否存在项目级 Provider 配置？

**结论：不存在（Unsupported）。官方仅文档化全局 `~/.pi/agent/models.json`；项目 `.pi/` 只有 `settings.json` 与资源目录。**

**官方 URL 证据**

- [pi.dev/docs/latest/models](https://pi.dev/docs/latest/models)：全文只出现 `~/.pi/agent/models.json`，未出现任何 `.pi/models.json` 或项目作用域。
- [pi.dev/docs/latest/settings](https://pi.dev/docs/latest/settings)："Location / Scope" 表：`~/.pi/agent/settings.json` = Global；`.pi/settings.json` = Project。Resources 列表里项目级只有 `extensions/skills/prompts/themes/packages`，无 provider。
- <https://pi.dev/docs/latest/security> 的 trust 触发列表：`.pi/settings.json`、`.pi/extensions|skills|prompts|themes`、`.pi/SYSTEM.md|APPEND_SYSTEM.md`、项目 `.agents/skills` —— **无 provider/models 文件**。

**本机 0.85.1 交叉校验（E2/E3）**：dist bundle 中 `models.json` 恒为 `join(getAgentDir(), "models.json")`（`getModelsPath()`），未发现任何 `cwd/.pi/models.json` 读取路径。项目配置路径只有 `<CONFIG_DIR_NAME>/settings.json` 与 `.pi/{extensions,prompts,skills,npm}`。

**缺失证据说明**：不适用——「不存在」由官方文档范围 + 本机代码双重确认。

**结论：项目级 Provider = `Unsupported`（不是 Unknown）。**

---

### Q3. `~/.pi/agent/auth.json` 的语义；第三方配置管理器可否接管？

**结论：`auth.json` 是 Pi 的凭据存储（API key + OAuth token），由 `/login`/`/logout` 管理；EasyToAgents 必须视其为只读禁区：不导入、不写入、不预览明文（对照 OpenCode "auth.json 从不导入或写入" 合同）。**

**官方 URL 证据**

- [https://pi.dev/docs/latest/providers](https://pi.dev/docs/latest/providers)（访问 2026-09-14）
  > "Tokens are stored in `~/.pi/agent/auth.json` and auto-refresh when expired."
  > "Use `/login` in interactive mode and select a provider to store an API key in `auth.json`..."
  > "The file is created with `0600` permissions (user read/write only). **Auth file credentials take priority over environment variables.**"
  > 凭据解析顺序第 2 位即 `auth.json`（高于环境变量与 `models.json`）。
- [pi.dev/docs/latest/quickstart](https://pi.dev/docs/latest/quickstart)："You can also run `/login` and select an API-key provider to store the key in `~/.pi/agent/auth.json`."

**语义要点（E1）**

- `key` 支持三种形态：literal、`$ENV`/`${ENV}`、`!command`（命令执行，stdout 作为凭据，进程内缓存）。
- API key 凭据还可携带 provider-scoped `env` 对象（如 Cloudflare account/gateway、Azure、Vertex、Bedrock 配置、`PI_CACHE_RETENTION`、`HTTP_PROXY`）。
- OAuth 凭据（`type: "oauth"`）也存放于此并自动刷新。
- 本机 0.85.1 还有迁移逻辑：把 legacy `oauth.json` 与 `settings.json.apiKeys` 迁移进 `auth.json`（E3，`migrations.js`：`migrateAuthToAuthJson()`）。

**是否可被第三方接管？官方没有授权第三方配置管理器写入 `auth.json`。** 官方把它描述为登录/凭据产物，且明确限制权限 0600。结合 EasyToAgents 现有 OpenCode 合同与安全边界：

- **Import：Unsupported**（凭据不进普通 DTO/日志/预览明文）。
- **Apply：Unsupported**（不写入、不迁移、不清理）。
- EasyToAgents 只登记「凭据由用户在 Pi 内经 `/login` 或环境变量提供」这个事实。

---

### Q4. `settings.json` 键位清单、自管键、项目 trust 语义

**结论：settings 有全局 `~/.pi/agent/settings.json` 与项目 `.pi/settings.json` 两个官方位置；项目资源是否生效完全由 project trust 决定。Provider 不在 settings 里，因此本方向只需记录边界，不接管 settings。**

**官方 URL 证据**

- [https://pi.dev/docs/latest/settings](https://pi.dev/docs/latest/settings)（访问 2026-09-14）：
  - 位置/作用域表：`~/.pi/agent/settings.json`（Global）、`.pi/settings.json`（Project）；"Project settings ... override global settings. Nested objects are merged."
  - Project Trust 段（同页）：见下。
- [https://pi.dev/docs/latest/security](https://pi.dev/docs/latest/security)（访问 2026-09-14）：Project Trust 全文（引文见下）。

**settings 键位清单（E1，0.85.1 与 latest 共有的稳定集合）**

| 分组              | 键                                                                                                                                                                                                                                                                                                                                                                        |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Model & Thinking  | `defaultProvider`, `defaultModel`, `defaultThinkingLevel`, `modelThinkingLevels`, `hideThinkingBlock`, `showCacheMissNotices`, `thinkingBudgets`                                                                                                                                                                                                                          |
| UI & Display      | `theme`, `externalEditor`, `quietStartup`, `defaultProjectTrust`（**仅全局**）, `collapseChangelog`, `enableInstallTelemetry`, `enableAnalytics`, `trackingId`, `doubleEscapeAction`, `treeFilterMode`, `editorPaddingX`, `outputPad`, `autocompleteMaxVisible`, `showHardwareCursor`, `tuiMode`, `fullscreenExitOutput`, `fullscreenScrollbar`, `fullscreenCopyOnSelect` |
| Network           | `httpProxy`（**仅全局**）                                                                                                                                                                                                                                                                                                                                                 |
| Warnings          | `warnings.anthropicExtraUsage`                                                                                                                                                                                                                                                                                                                                            |
| Compaction        | `compaction.enabled`, `compaction.reserveTokens`, `compaction.keepRecentTokens`（latest 另有 per-model overrides，0.85.1 未见）                                                                                                                                                                                                                                           |
| Branch Summary    | `branchSummary.reserveTokens`, `branchSummary.skipPrompt`                                                                                                                                                                                                                                                                                                                 |
| Retry             | `retry.enabled`, `retry.maxRetries`, `retry.baseDelayMs`, `retry.provider.{timeoutMs,maxRetries,maxRetryDelayMs}`                                                                                                                                                                                                                                                         |
| Message Delivery  | `steeringMode`, `followUpMode`, `transport`, `httpIdleTimeoutMs`, `websocketConnectTimeoutMs`                                                                                                                                                                                                                                                                             |
| Terminal & Images | `terminal.showImages`, `terminal.imageWidthCells`, `terminal.clearOnShrink`, `terminal.hyperlinks`, `terminal.images`, `terminal.trueColor`, `images.autoResize`, `images.blockImages`                                                                                                                                                                                    |
| Shell             | `shellPath`, `shellCommandPrefix`, `npmCommand`                                                                                                                                                                                                                                                                                                                           |
| Tools             | `defaultTools`                                                                                                                                                                                                                                                                                                                                                            |
| Sessions          | `sessionDir`                                                                                                                                                                                                                                                                                                                                                              |
| Model Cycling     | `enabledModels`                                                                                                                                                                                                                                                                                                                                                           |
| Markdown          | `markdown.codeBlockIndent`, `markdown.mermaid`                                                                                                                                                                                                                                                                                                                            |
| Resources         | `packages`, `extensions`, `skills`, `prompts`, `themes`, `enableSkillCommands`                                                                                                                                                                                                                                                                                            |

**Pi 自管 vs 用户可写（E1 + E3）**

- 官方说明「Edit directly or use `/settings`」→ 上表键默认视为用户可写。
- 但实测存在官方文档未列出的 Pi 自管键：`lastChangelogVersion`（E3，`~/.pi/agent/settings.json` 中由 Pi 写入；dist bundle 中出现 15 次）。UI 交互保存的键（`/model` Ctrl+S → `defaultProvider/defaultModel`；`/thinking` Ctrl+S → `defaultThinkingLevel`；`enableAnalytics` 开启时自动生成 `trackingId`）也是 Pi 写入路径。
- 结论：**EasyToAgents 当前不应接管 settings**（本方向 Provider 不在 settings 里）。若未来纳入，必须按 CAS/局部键合并，禁止整体覆写（否则会抹掉 `lastChangelogVersion` 等自管键）。

**项目 trust 语义（E1，security.md/settings.md）**

- trust 触发条件（存在其一即需要 trust）：
  - `.pi/settings.json`
  - `.pi/extensions`、`.pi/skills`、`.pi/prompts`、`.pi/themes`
  - `.pi/SYSTEM.md` 或 `.pi/APPEND_SYSTEM.md`
  - 项目 `.agents/skills`（当前目录或祖先目录）
  - "A bare `.pi` directory does not count as a project resource that requires trust."
- 决策存储：`~/.pi/agent/trust.json`，按 canonical directory 存布尔；"the closest saved decision on the current or parent path applies before the global default"（本机实测：`{"/Users/zhexin/github": true}`，E3）。
- 全局回退：`defaultProjectTrust` ∈ `"ask"`(默认) | `"always"` | `"never"`，**仅全局 settings**。
- 非交互模式（`-p`、`--mode json`、`--mode rpc`）不弹 trust 提示；没有已保存决策时，`"ask"` 与 `"never"` 都**忽略**项目资源，`"always"` 信任；`--approve/-a`、`--no-approve/-na` 可单次覆盖。
- **未信任 = 静默忽略**：security.md 原文：
  > "Declining trust skips protected resources. ... Project-local extensions, project package-managed extensions, and project settings are loaded only after the project is trusted."
- 例外：**context 文件不受 trust 限制**——`AGENTS.override.md`、`AGENTS.md`、`CLAUDE.md` 无论 trust 与否都会加载（除非关闭 context loading）（E1）。这与 `AGENTS.override.md` 覆盖面问题相关，但属于 Prompt/Rules 方向，本文件只记录边界。

**对本方向的影响**：

1. Provider 只在全局 → **PROJECT TRUST 不影响 Provider**（`models.json` 在 `~/.pi/agent`，不属于项目资源）。
2. 但若 EasyToAgents 未来为 Pi 增加任何 `.pi/` 项目级写入（Provider 之外），**未信任项目的写入会在运行时被静默忽略**，Preview 必须显示 `untrusted` 且禁止 Apply（应复用 Codex 项目 untrusted 的处理模式，见 adding-tool-adapter.md §11.3）。

---

### Q5. 安装探针合同：macOS `.app`？安装方式、可执行文件位置、版本探测

**结论：Pi 官方只有 CLI/TUI，没有官方 macOS `.app`。安装方式为 npm 全局包或 `curl https://pi.dev/install.sh | sh`。安装探针应使用 PATH 上的 `pi --version`，严格解析 semver，不得依赖固定安装路径或 `npm ls -g`。**

**官方 URL 证据**

- [https://pi.dev/docs/latest/index](https://pi.dev/docs/latest/index)（访问 2026-09-14）
  > "Install Pi with npm: `npm install -g --ignore-scripts @earendil-works/pi-coding-agent`"
  > "On Linux or macOS, you can also use the installer: `curl -fsSL https://pi.dev/install.sh | sh`"
- [pi.dev/docs/latest/quickstart](https://pi.dev/docs/latest/quickstart)：同上 npm 命令；卸载说明覆盖 npm/pnpm/Yarn/Bun。
- <https://pi.dev/docs/latest/usage> "CLI Reference"（访问 2026-09-14）
  > `| -v, --version | Show version |`
- 全站/安装文档未出现任何官方 `.app`、bundle id 或 macOS 桌面安装页（E1 搜索亦未发现官方来源）。

**macOS 桌面 `.app`（结论：Unsupported）**

- `smart-search search "pi coding agent @earendil-works install macOS desktop app bundle .app" --timeout 180`（2026-09-14）返回的均为**社区前端**（`rubengarciajr/pi-desktop`、`pi-desktop.app`、`dodo-reach/apple-pi` 等），并明确 "There is no single official desktop GUI from Earendil Works (Pi is primarily a powerful terminal/TUI coding agent)"。
- 来源等级 L（第三方），**不能授权写入**；因此**不提供 Bundle ID / Info.plist 探针**。Pi 的安装探针是 CLI-only。

**安装产物与可执行文件位置（E3，本机 0.85.1）**

- `package.json`：`"bin": { "pi": "dist/bundle/cli.js" }`；`"piConfig": { "configDir": ".pi" }`。
- 本机 `which pi` → `/Users/zhexin/.volta/bin/pi` → `readlink -f` 落在 `/opt/homebrew/Cellar/volta/2.0.2/bin/volta-shim`。即 **PATH 上的 `pi` 可能是包管理器 shim**，realpath 形状不可信。
- `npm ls -g` 在本机**看不到** pi（volta image 安装，npm global root 为空）——**探针绝不能用 `npm ls -g` 判断安装**。
- 源码中还存在 `bun-binary` 独立可执行安装形态（E3，dist `detectInstallMethod()` 返回 `"bun-binary"`）；官方文档未文档化其路径。进一步说明固定路径不可行。

**版本探测命令与输出格式**

- 官方命令：`pi --version` / `pi -v`（E1）。
- 输出格式：本机实测 `pi --version` → `0.85.1`（纯 semver，无前缀，E3）。官方文档只写 "Show version"，**未文档化精确输出格式** → 输出格式属于「本机 smoke 证据」，探针须严格匹配 `^[0-9]+\.[0-9]+\.[0-9]+`，其余返回 Unsupported。

**`~/.pi/agent/bin/` 里是什么？（E3，非官方文档）**

- dist `config.js`：`getBinDir()` → `join(getAgentDir(), "bin")`，注释 "managed binaries directory (fd, rg)"；代码注释 "fd for autocomplete, rg for grep"。
- 本机实测 `~/.pi/agent/bin/{fd,rg}`（两个原生二进制）。
- **官方文档未文档化 `~/.pi/agent/bin`** → 标记为 E3 内部实现细节；探针**不得**用它的存在与否判断安装，也不得把它当作配置根。

**探针结论**：

- `pi --version` 成功且输出 semver → `Installed`。
- 命令不存在 / 非零退出 → `ToolNotInstalled`。
- 超时、权限错误、输出非 semver → `Unsupported` / `unavailable`（fail closed，不降级为可写）。
- 无 bundle 探针；不解析 shim realpath；不依赖 `~/.pi/agent/bin`。

---

### Q6. 配置根覆盖与 PI_* 变量

**结论：`PI_CODING_AGENT_DIR` 是唯一官方文档化的「配置目录整体覆盖」变量，直接决定 `models.json`/`auth.json`/`settings.json`/`trust.json` 的读写位置，必须纳入探针与写入策略。其余 `PI_*` 主要影响运行时/网络行为，不进写入路径。**

**官方 URL 证据**：[https://pi.dev/docs/latest/environment-variables](https://pi.dev/docs/latest/environment-variables)（访问 2026-09-14）

> "Pi Process Configuration — These variables are read by Pi itself:"
> | `PI_CODING_AGENT_DIR` | **Override the config directory; default is `~/.pi/agent`** |
> | `PI_CODING_AGENT_SESSION_DIR` | Override session storage; overridden by `--session-dir` |
> | `PI_PACKAGE_DIR` | Override the package directory, useful for Nix/Guix store paths |
> | `PI_OFFLINE` | Disable startup network operations, including update checks, package updates, and install/update telemetry |
> | `PI_SKIP_VERSION_CHECK` | Disable the `pi.dev` latest-version request |
> | `PI_TELEMETRY` | Override install/update telemetry and provider attribution headers (`1/true/yes` or `0/false/no`) |
> | `PI_CACHE_RETENTION` | `long` for extended provider prompt caching |
> | `PI_SHARE_VIEWER_URL` / `PI_HARDWARE_CURSOR` / `PI_HYPERLINKS` / `PI_IMAGE_PROTOCOL` / `PI_TRUE_COLOR` / `PI_TUI_ESC_TIMEOUT` | UI/终端行为 |
> | `VISUAL`, `EDITOR` | 外部编辑器回退 |
> | `HTTP_PROXY`, `HTTPS_PROXY` | 代理 |

还有 session-only 变量（`PI_SESSION_ID/FILE/PROVIDER/MODEL/REASONING_LEVEL`）与进程标记（`AI_AGENT=pi`、`PI_CODING_AGENT=true`）——不改变配置位置。

**本机 0.85.1 交叉校验（E3）**：dist `config.js` 中 `getAgentDir()` 读取 `PI_CODING_AGENT_DIR`，缺失时回退 `join(homedir(), ".pi", "agent")`；`getModelsPath()/getAuthPath()/getSettingsPath()/getBinDir()/getSessionsDir()/getNpmDir()` 与 `ProjectTrustStore(agentDir)` **全部基于 `getAgentDir()`**。即覆盖是整体生效的。

**必须纳入策略的变量**

- **`PI_CODING_AGENT_DIR`（必须）**：决定写入根与 allowed_root；用于隔离验证（fixture 指向临时目录）。
- **`PI_OFFLINE=1`（探针/测试建议）**：避免 `pi --version` / `--list-models` 触发网络（版本检查、catalog 刷新）。
- **`PI_SKIP_VERSION_CHECK=1`**：可选，进一步避免 pi.dev 版本请求。
- 其余 `PI_*`（telemetry、UI、session、proxy）不影响 Provider 写入位置；`PI_PACKAGE_DIR` 影响包资源解析，不影响 `models.json`。

**缺失证据说明**：官方未给出「`PI_CODING_AGENT_DIR` 相对路径/`~` 展开规则」的细节。本机实现用 `normalizePath()`（支持 `~`），但属 E3；EasyToAgents 应只接受绝对路径并自行 canonicalize。

---

### Q7. Provider 相关敏感字段清单与脱敏边界

**结论**：`models.json` 的敏感面是 `apiKey` 与所有 `headers`（provider 级 + model/modelOverrides 级）；`auth.json` 整体敏感（禁读禁写）；`settings.json` 的 legacy `apiKeys`、`trackingId`、`httpProxy` 内嵌凭据为次要敏感面。**

**敏感字段清单**

- `models.json`
  - `providers.<id>.apiKey`（literal 明文 / `$ENV_VAR` / `${ENV_VAR}` / `!command`）
  - `providers.<id>.headers.*`（同上取值语法）
  - `providers.<id>.models[].headers`（若存在）
  - `providers.<id>.modelOverrides.<model>.headers`
  - `providers.<id>.oauth` 侧配置（Radius 网关，`baseUrl` 通常不含 secret）
- `auth.json`（**整体禁区**）：`{ "<provider>": { "type": "api_key"|"oauth", "key", "env": {...}, "refresh", "access" } }`
- `settings.json`：legacy `apiKeys`（0.85.1 会迁移进 `auth.json`，E3）、`trackingId`（分析标识）、`httpProxy`（URL 可能内嵌凭据）
- 环境变量名（官方 provider 表，E1）：`ANTHROPIC_API_KEY`、`OPENAI_API_KEY`、`DEEPSEEK_API_KEY`、`GEMINI_API_KEY`、`XAI_API_KEY`、`OPENROUTER_API_KEY`、`AWS_BEARER_TOKEN_BEDROCK`、`AZURE_OPENAI_API_KEY`、`CLOUDFLARE_API_KEY`(+`CLOUDFLARE_ACCOUNT_ID`/`CLOUDFLARE_GATEWAY_ID`)、`HF_TOKEN`、`FIREWORKS_API_KEY`、`TOGETHER_API_KEY`、`BASETEN_API_KEY`、`KIMI_API_KEY`、`MINIMAX_API_KEY`、`MINIMAX_CN_API_KEY`、`QWEN_TOKEN_PLAN_API_KEY`、`QWEN_TOKEN_PLAN_CN_API_KEY`、`XIAOMI_API_KEY`、`ZAI_API_KEY`、`ZAI_CODING_CN_API_KEY`、`OPENCODE_API_KEY`、`RADIUS_API_KEY`、`ANT_LING_API_KEY`、`NVIDIA_API_KEY`、`MISTRAL_API_KEY`、`GROQ_API_KEY`、`CEREBRAS_API_KEY`、`AI_GATEWAY_API_KEY` 等。

**脱敏边界建议**

1. Provider descriptor 的 `sensitive_selectors` 至少覆盖：`providers/*/apiKey`、`providers/*/headers`、`providers/*/models/*/headers`、`providers/*/modelOverrides/*/headers`。
2. Import：敏感值**不得**进入普通 DTO/日志/Preview 明文。保留键名与结构，值替换为脱敏占位或直接不导入并要求用户重填。`!command` 与 `$ENV` 视为**引用表达式**，不得求值、不得展开、不得执行。
3. Apply：写回时保留用户已有的未知字段与引用表达式；只写 EasyToAgents 受管的 provider 条目；不触碰 `auth.json`。
4. 绝不对 `models.json` 里 `apiKey: "!command"` 执行命令（Pi 在请求时解析，EasyToAgents 不复制该副作用）。
5. 本机 fixture 已证明 `models.json` 会内联明文 key（E3：`providers.cc.apiKey` 为明文）→ 脱敏必须有单测覆盖。

---

## 3. artifact × scope × operation 证据表

取值仅允许 `Supported` / `Unsupported` / `Unknown` / `ToolNotInstalled`。

| Artifact                            | Scope   | Import          | Apply           | 官方 URL 证据（访问 2026-09-14）                                                                                                                              | 稳定路径 / 格式                                                                                                                                 | 诊断                                                     |
| ----------------------------------- | ------- | --------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| Provider                            | Global  | **Supported**   | **Supported**   | [models](https://pi.dev/docs/latest/models)；[providers](https://pi.dev/docs/latest/providers)；[custom-provider](https://pi.dev/docs/latest/custom-provider) | `~/.pi/agent/models.json`（或 `$PI_CODING_AGENT_DIR/models.json`），JSON，`providers.<id>.{baseUrl,api,apiKey,headers,models[],modelOverrides}` | 仅当 `pi --version` 成功；根由 agent dir 派生            |
| Provider                            | Project | **Unsupported** | **Unsupported** | [models](https://pi.dev/docs/latest/models)（全文仅全局）；[settings](https://pi.dev/docs/latest/settings)（项目仅 settings/资源）                            | 无官方项目路径                                                                                                                                  | 服务层拒绝，不生成项目 descriptor                        |
| Provider 凭据（`auth.json`）        | Global  | **Unsupported** | **Unsupported** | [providers](https://pi.dev/docs/latest/providers)（`/login` 管理、0600、优先于 env）                                                                          | `~/.pi/agent/auth.json`；**禁读禁写**                                                                                                           | 产品决策 + 安全边界（对照 OpenCode §9）                  |
| Provider 凭据                       | Project | **Unsupported** | **Unsupported** | 同上（Pi 无项目级凭据文件）                                                                                                                                   | —                                                                                                                                               | —                                                        |
| 模型目录缓存（`models-store.json`） | Global  | **Unsupported** | **Unsupported** | [providers](https://pi.dev/docs/latest/providers)（"cache them ... for offline use"）；[usage](https://pi.dev/docs/latest/usage)（`pi update --models`）      | `~/.pi/agent/models-store.json`（0600，Pi 自管）                                                                                                | 写入会被 `pi update --models` 覆盖                       |
| 安装/版本探针（CLI）                | Global  | —               | —               | [index](https://pi.dev/docs/latest/index)；[quickstart](https://pi.dev/docs/latest/quickstart)；[usage](https://pi.dev/docs/latest/usage)                     | PATH `pi --version` / `-v` → semver                                                                                                             | 缺失/非 semver/超时 → `ToolNotInstalled` / `Unsupported` |
| 官方 macOS `.app`                   | Global  | **Unsupported** | **Unsupported** | 官方安装文档无 `.app`（仅社区前端，来源 L）                                                                                                                   | —                                                                                                                                               | 不提供 bundle/Info.plist 探针                            |

> 注：`Import`/`Apply` 是 Provider 资源语义；`安装/版本探针` 是所有 Provider 能力的前置 gate——`pi --version` 不可信时，Provider 单元格整体表现为 `ToolNotInstalled`。

---

## 4. 反例 / 风险（会导致写错位置或静默无效的具体场景）

| #   | 场景                                                                                  | 后果                                                                                                             | 依据                                                |
| --- | ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| R1  | 用户设置了 `PI_CODING_AGENT_DIR=/custom`，EasyToAgents 仍写 `~/.pi/agent/models.json` | Pi 读 `/custom/models.json`，写入**静默无效**                                                                    | E1 environment-variables                            |
| R2  | 把 `models-store.json` 当作 Provider 配置读写                                         | 用户自定义模型被 `pi update --models` 刷新覆盖；catalog/etag 元数据被破坏                                        | E1 providers + usage；E3 结构                       |
| R3  | 读/写 `auth.json`                                                                     | 泄漏 OAuth/API 凭据；破坏 Pi 自动刷新状态；违反安全边界                                                          | E1 providers；产品合同（OpenCode §9）               |
| R4  | 在 `models.json` 内联明文 `apiKey` 进入 Import/Preview/日志                           | 凭据明文泄漏                                                                                                     | E1 models（apiKey 语法）；E3 本机明文 key 实例      |
| R5  | EasyToAgents 对 `apiKey: "!command"` 做求值/执行                                      | 副作用（执行用户命令）、结果不可复现                                                                             | E1 models（命令在请求时解析）                       |
| R6  | 整体替换 `providers.<id>`（未保留未知字段/合并语义）                                  | 丢失用户 `modelOverrides`/`headers`，或破坏 built-in 模型合并（给 `models` 时按 id upsert，不给时仅改端点）      | E1 models "Overriding Built-in Providers"           |
| R7  | 未来新增 `.pi/` 项目级写入，但项目未 trust                                            | 运行时被静默忽略（`ask`/`never`）；Preview 却显示成功                                                            | E1 security/settings Project Trust                  |
| R8  | 用 `npm ls -g` 或固定路径判断 Pi 安装                                                 | volta/bun-binary 场景漏检，误报 ToolNotInstalled，进而拒绝已验证能力                                             | E3 本机 `npm ls -g` 为空、PATH 是 volta shim        |
| R9  | 用 `~/.pi/agent/bin/{fd,rg}` 存在性判断安装                                           | 非官方文档化，且这些二进制可能被懒加载/缺失                                                                      | E3 dist `getBinDir()`；官方文档未列                 |
| R10 | 假设 `pi --version` 输出包含前缀文本                                                  | 解析失败 → 误判；官方未文档化格式，只有本机 smoke                                                                | E1 usage（仅 "Show version"）；E3 输出 `0.85.1`     |
| R11 | 以 `latest` 文档新增键（如 per-model compaction、未来 schema）为依据写 0.85.1         | 写入未知键；或遗漏版本差异                                                                                       | §0 版本偏差警告                                     |
| R12 | 在 Provider Apply 时读取 `auth.json` 判断「是否已配置 key」                           | 违反禁区；且 `auth.json` 优先级高于 `models.json`，据此推断会得到错误结论（用户明明写了 models.json key 却无效） | E1 providers 解析顺序                               |
| R13 | `AGENTS.override.md` / `AGENTS.md` / `CLAUDE.md` 不受 trust 限制（覆盖面例外）        | 与 Prompt/Rules 方向相关：若把 context 文件误当受 trust 保护的资源，会错误 fail closed 或错误放行                | E1 security（"loaded regardless of project trust"） |

---

## 5. 给 EasyToAgents 的落地建议

### 5.1 Adapter descriptor（Provider，仅 Global）

| 项                  | 建议值                                                                                                                  |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| artifact            | `ArtifactKind::Provider`                                                                                                |
| scope               | `Scope::Global` only（**不新增**任何 Project Provider descriptor）                                                      |
| path                | `{pi_agent_dir}/models.json`，其中 `pi_agent_dir` = `PI_CODING_AGENT_DIR`（若显式注入）否则 `~/.pi/agent`               |
| format              | `TargetFormat::Json`（单文件 + 局部 selector，不能用 WholeDocument 整体替换）                                           |
| managed_selectors   | `["providers"]`（按 provider id 做条目级 ownership；禁止整文件覆写）                                                    |
| sensitive_selectors | `["providers/*/apiKey", "providers/*/headers", "providers/*/models/*/headers", "providers/*/modelOverrides/*/headers"]` |
| 禁写清单            | `auth.json`、`models-store.json`、`trust.json`、`settings.json`、`~/.pi/agent/bin/**`                                   |
| symlink policy      | 单文件，沿用现有单文件策略；Apply 前 canonicalize 并做逃逸检查                                                          |

### 5.2 allowed_root 如何收窄

- 复用 `global_root_for()` 统一映射，新增 `(Tool::Pi, ArtifactKind::Provider) => environment.pi_agent_dir().to_path_buf()`。
- `pi_agent_dir` 必须是**显式注入**的 `ExplicitEnvironment` 字段（默认 `home/.pi/agent`，测试用 `with_pi_agent_dir(tmp)`），**不得**在服务层读取真实 `HOME`/`PI_CODING_AGENT_DIR`。
- 边界只能是 `~/.pi/agent`（而非 `~/.pi`、更非整个 `HOME`），恢复链路（`overview::global_allowed_root`）复用同一映射。
- 若检测到真实进程环境存在 `PI_CODING_AGENT_DIR` 但调用方未显式映射该覆盖 → **fail closed**（诊断码建议 `PI_AGENT_DIR_OVERRIDE_UNMAPPED`），避免 R1。

### 5.3 探针怎么做

1. **CLI-only**：注入候选命令（默认 `pi`）与超时，执行 `pi --version`；严格匹配 `^\d+\.\d+\.\d+$`（或宽松 semver 前缀）。
   - 成功 + semver → `Installed`，记录版本。
   - `ENOENT`/非零退出 → `ToolNotInstalled`。
   - 超时/权限/异常输出 → `Unsupported`（`PI_INSTALLATION_PROBE_UNSUPPORTED`），不降级为可写。
2. 不探测 macOS bundle（无官方 `.app`）。
3. 不用 `npm ls -g`、不解析 `which pi` 的 realpath、不依赖 `~/.pi/agent/bin`。
4. 探针环境全部显式注入（PATH、HOME、`PI_CODING_AGENT_DIR`、`PI_OFFLINE=1`），测试不读真实 HOME/PATH（对照 adding-tool-adapter.md §3）。
5. **配置文件是否存在不等于是否安装**：`models.json` 是用户按需创建的，缺失不得判为 `ToolNotInstalled`；安装 gate 只看 CLI 探针。

### 5.4 隔离验证与 fixtures

- 用 `PI_CODING_AGENT_DIR=<tmp>` + `PI_OFFLINE=1` + `PI_SKIP_VERSION_CHECK=1` 做 smoke：写入 fixture `models.json` 后，用 `pi --offline --list-models` 验证 Pi 是否读到（注意 `--list-models` 的 auth 预检查行为，见 models.md）。
- fixture 至少覆盖：
  1. 最小 provider（`baseUrl`+`api`+`models[].id`）；
  2. `apiKey: "$ENV"` 与 `apiKey: "!command"`（验证不展开/不执行）；
  3. built-in provider 覆盖（仅 `baseUrl`，无 `models`）——验证合并语义不丢内置模型；
  4. `modelOverrides` + provider 级 `headers`（验证敏感 selector）；
  5. 明文 `apiKey`（验证脱敏与 Preview 无明文）。
- 数据库/前端：Provider assignment 只对 Supported 的 Global Provider 放宽；项目 Provider 保持表级 CHECK 拒绝（第二道边界）。

### 5.5 能力文案

- Provider 行标 `Supported（仅全局`~/.pi/agent/models.json`）`；显式注明不接管 `auth.json`、`models-store.json`、项目作用域。
- 若 `PI_CODING_AGENT_DIR` 覆盖无法映射，UI/diagnostics 显示 `Unsupported` 原因，而非静默写入。

---

## 附录 A：本次取证命令与产物

- `smart-search fetch https://pi.dev/docs/latest/{providers,models,settings,security,environment-variables,quickstart,custom-provider,usage,index} --format markdown`（2026-09-14）
- `smart-search search "pi coding agent project models.json .pi/settings.json providers pi.dev" --timeout 180 --format markdown`
- `smart-search search "pi coding agent @earendil-works install macOS desktop app bundle .app" --timeout 180 --format markdown`
- 本机交叉校验：0.85.1 `docs/*.md`、`dist/config.js`、`dist/migrations.js`、`~/.pi/agent` 布局、`pi --version`。

## 附录 B：显式 Unknown / 缺失证据清单

| 项                                                              | 状态                    | 缺失证据                                                                      |
| --------------------------------------------------------------- | ----------------------- | ----------------------------------------------------------------------------- |
| `pi --version` 精确输出格式（是否始终纯 semver）                | 部分 Unknown            | 官方仅 "Show version"；只有本机 0.85.1 smoke。多版本 fixture 待补             |
| `PI_CODING_AGENT_DIR` 相对路径/`~` 展开与 canonicalization 规则 | Unknown                 | 官方只写 "Override the config directory"；本机实现细节属 E3                   |
| `bun-binary` 独立安装形态的可执行路径                           | Unknown（官方未文档化） | 官方安装文档只列 npm/curl；探针故改用 PATH                                    |
| `models.json` model 级 `headers` 是否 0.85.1 即支持             | 待 0.85.1 fixture 复核  | 官方模型字段表未列 `headers`，但 `modelOverrides` 支持 `headers`（E1 latest） |
| `latest` 文档与 0.85.1 的全部键位差异                           | Unknown                 | 需逐版本 diff；已发现 `Per-model compaction overrides` 差异                   |
