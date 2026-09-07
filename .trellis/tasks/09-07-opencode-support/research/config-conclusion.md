# OpenCode 配置/Provider/规则/安装探针证据

核验日期：2026-09-07。来源均为 OpenCode 官方文档或官方 GitHub 仓库；原始抓取文件见同目录 `config-page-*.md`、`config-context7-*.json`。

## 官方配置文件与优先级

官方 URL：<https://opencode.ai/docs/config/>。

- 支持 JSON 与 JSONC；原文（`config-page-0.md:15`）：“OpenCode supports both **JSON** and **JSONC** (JSON with Comments) formats.”
- 全局配置为 `~/.config/opencode/opencode.json`（`config-page-0.md:60`）；项目配置为项目中的 `opencode.json`（同页约 68 行）。
- 官方列出的低到高优先级原文（`config-page-0.md:29-40`）：Remote `.well-known/opencode` → Global → `OPENCODE_CONFIG` → Project → `.opencode` directories → `OPENCODE_CONFIG_CONTENT` → managed files → macOS MDM managed preferences。配置“merged together, not replaced”；后者冲突键覆盖前者，非冲突字段保留。
- `OPENCODE_CONFIG` 是自定义配置文件路径；`OPENCODE_CONFIG_DIR` 是自定义配置目录（可承载 agents/commands/modes/plugins，`config-page-0.md:82-90`）。CLI 文档也列出 `OPENCODE_CONFIG`、`OPENCODE_CONFIG_DIR`、`OPENCODE_CONFIG_CONTENT`（`config-page-3.md:472-475`）。
- 配置 schema：`https://opencode.ai/config.json`；TUI schema：`https://opencode.ai/tui.json`（`config-page-0.md` 的 Schema 段）。

Context7 对官方源码的补充证据：`config-context7-0.json` 内容引用 `packages/core/src/config.ts`，确认加载 entries 低到高为 global config → project files → `.opencode` files；源码原文注释为“Apply general settings first and more specific settings last”。该源码摘要还提到 `OPENCODE_CONFIG_CONTENT` 解析为 JSON 的 local source，位于 project configs 后、remote account config 前；网页文档的完整优先级应作为当前用户可见合同。

## Provider、模型与凭据存储

官方 URL：<https://opencode.ai/docs/providers/>。

- OpenCode 使用 AI SDK 与 Models.dev，支持“75+ LLM providers”（`config-page-1.md:189`）。启用 provider 的官方两步是：用 `/connect` 添加 API key，再在配置的 `provider` section 配置 provider（`config-page-1.md:193-200`）。
- 凭据存储区与 provider 配置分开：`/connect` 写入 `~/.local/share/opencode/auth.json`（`config-page-1.md:200`）。模型/Provider 的运行时定义在 `opencode.json` 的 `provider`、`model`、`small_model`（`config-page-0.md:198-201`）。
- Provider 支持自定义 base URL（Providers 页 `Base URL` 段，约 `config-page-1.md:210` 起）及自定义 models。配置文件示例中的 provider model 仅表达 provider/model 元数据，不应据此推断 API key 应写进 config。
- 官方 CLI URL：<https://opencode.ai/docs/cli/>；`opencode auth login` 可配置任意支持 provider 的 API key，仍保存到 `~/.local/share/opencode/auth.json`（`config-page-3.md:103`）。
- 官方仓库源码 URL：<https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/auth/index.ts>。抓取页（`config-page-4.md`）显示：`const file = path.join(Global.Path.data, "auth.json")`；Api schema 为 `{ type: "api", key: string, metadata? }`，OAuth schema 为 `{ type: "oauth", refresh, access, expires, ... }`；写入调用 `writeJson(..., 0o600)`。源码还显示 `OPENCODE_AUTH_CONTENT` 可提供内联 auth JSON。网页/源码均未证明系统钥匙串加密，因此探针应把 auth.json 作为敏感文件处理；是否跨平台加密未在网页合同中声明。
- Provider config 字段的官方合同至少包含 `provider`、provider 内 `models`、`model`、`small_model`、base URL；实际 provider-specific fields 需按 schema/provider 文档解析。不要把 auth.json 当成 provider config 合并写回。

## 全局与项目规则入口

官方 URL：<https://opencode.ai/docs/rules/>。

- 项目根或向上目录的 `AGENTS.md` 是项目规则；全局规则是 `~/.config/opencode/AGENTS.md`（`config-page-2.md:41-49`）。
- 兼容回退：项目 `CLAUDE.md`（无 `AGENTS.md` 时）；全局 `~/.claude/CLAUDE.md`（无 `~/.config/opencode/AGENTS.md` 时）（`config-page-2.md:57-58`）。查找顺序原文（`config-page-2.md:65-73`）：本地遍历 → 全局 OpenCode AGENTS → Claude 全局 CLAUDE；“The first matching file wins in each category.”
- 也可在 `opencode.json` 或全局 config 用 `instructions` 指定自定义 instruction 文件（`config-page-2.md:79-87`；config 页的 `instructions` 段）。
- CLI `OPENCODE_DISABLE_PROJECT_CONFIG` 可跳过项目级 AGENTS/CLAUDE 探索，但全局 AGENTS 总是加载；该事实来自 Context7 `config-context7-2.json` 对官方 `packages/core/src/instruction-context.ts` 的摘要，需以目标版本源码复核。

## CLI/Desktop 安装探针

官方仓库 README（Context7 `config-context7-3.json`，来源 <https://github.com/anomalyco/opencode/blob/dev/README.md>）给出：

- CLI 快速安装：`curl -fsSL https://opencode.ai/install | bash`。
- macOS/Linux Homebrew：`brew install anomalyco/tap/opencode`（README 同时列出 `brew install opencode`）；npm：`npm i -g opencode-ai@latest`。
- Desktop（Beta）macOS：`brew install --cask opencode-desktop`；Windows Scoop：`scoop bucket add extras; scoop install extras/opencode-desktop`。

## 与当前项目类型的只读对照

当前 ZCode adapter（`src-tauri/src/adapters/zcode/mod.rs:35-61`）将 Provider 建模为全局 JSON、显式 managed selector `provider` 与敏感 selector `provider/*/options/apiKey`；Prompt 建模为全局 Markdown 整文档（`.../mod.rs:54-61`）。OpenCode 官方事实要求 Provider config 与 auth.json 分离、规则既有项目又有全局，因此不能直接复用 ZCode 的单文件 provider/API-key 假设。当前项目 `ArtifactKind` 已有 `Provider`/`Prompt`（`src-tauri/src/domain/mod.rs:96-97`），类型层面可承载这两类资源；是否实现由主代理决定。

## 未覆盖/存疑

- 本轮未读取真实用户凭据，未安装或运行 OpenCode，也未验证本机安装探针返回值。
- 官方 config 网页与 Context7 当前源码摘要对 `OPENCODE_CONFIG_DIR` 的发现顺序表述略有差异（网页列为低于 inline、高于 `.opencode` 的 custom directory 描述；源码摘要称其加入 global config discovery）。应以目标 OpenCode 版本的实际实现/版本锁定后再定探针优先级。
- Provider 的完整 provider-specific schema、Models.dev 缓存路径、Desktop 应用数据目录未在本轮完全提取；仅有上述官方文档可验证字段与路径。
