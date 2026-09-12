<p align="center">
  <img src="docs/assets/github-hero.png" alt="EasyToAgents — Claude、Codex、Cursor、ZCode、OpenCode、MCP、全局 Prompts 与 Skills 的本地配置中枢" width="100%" />
</p>

<h1 align="center">EasyToAgents</h1>

<p align="center"><strong>把 Claude、Codex、Cursor、ZCode、OpenCode、MCP、全局提示词与 Skills 收拢到一个可预览、可同步、可恢复的本地工作台。</strong></p>

<p align="center"><em>A local-first macOS app to preview, sync, and restore Claude, Codex, Cursor, ZCode, OpenCode, MCP, global prompts, and registered-project resources.</em></p>

<p align="center">
  <a href="https://github.com/zhexin233-ui/easytoagents/releases/tag/v0.1.0"><img src="https://img.shields.io/badge/release-v0.1.0-2563EB" alt="最新版本 v0.1.0" /></a>
  <img src="https://img.shields.io/badge/macOS-13%2B-000000?logo=apple&logoColor=white" alt="macOS 13+" />
  <img src="https://img.shields.io/badge/Apple%20Silicon-ARM64-000000?logo=apple&logoColor=white" alt="Apple Silicon ARM64" />
  <img src="https://img.shields.io/badge/Local--first-1E3A5F" alt="Local-first" />
  <img src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111827" alt="React 19" />
  <img src="https://img.shields.io/badge/Rust-1.77.2%2B-000000?logo=rust&logoColor=white" alt="Rust 1.77.2+" />
</p>

<p align="center">
  <a href="#核心能力">核心能力</a> ·
  <a href="#产品实景">产品实景</a> ·
  <a href="#安全同步模型">安全同步</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="#下载安装包">下载安装包</a> ·
  <a href="#参与贡献">参与贡献</a>
</p>

<p align="center">
  <a href="https://github.com/zhexin233-ui/easytoagents/releases/download/v0.1.0/EasyToAgents_0.1.0_aarch64.dmg"><strong>下载 EasyToAgents v0.1.0 for Apple Silicon</strong></a>
</p>

EasyToAgents 面向同时使用 Claude、Codex、Cursor、ZCode 与 OpenCode 的开发者。它把全局配置与已登记项目中的受支持资源整理成中央意图，同时保留对原生目标状态的检查；默认先展示变更计划，再由用户确认是否写入磁盘。

- **中央意图**：在一个界面维护希望启用的 Provider、全局提示词、MCP、Skills 与 Hooks。
- **原生目标**：继续使用 Claude、Codex、Cursor、ZCode、OpenCode 各自公开支持的配置格式和目录，不引入专有运行时。
- **Local-first**：中央数据、同步记录与私有恢复点保留在本机，配置管理不依赖独立网站或云端控制台。

## 核心能力

| 能力                   | 可以做什么                                                                                                                                                                                                                     | 写入边界                                                                           |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------- |
| **Providers / 提示词** | 检测并导入 Claude、Codex、ZCode、OpenCode 的 Provider，以及五个工具的全局提示词；Claude/Codex 渠道支持 API Key 与官方账号登录两种认证方式，官方登录由 `claude auth login` / `codex login` 在浏览器中完成；提示词不提供项目分配 | 中央档案的编辑与原生配置写入分离；默认先预览再确认应用；应用不保存任何官方登录凭据 |
| **MCP**                | 在中央库维护 MCP Server，按 Claude、Codex、Cursor、ZCode、OpenCode 或具体项目分配                                                                                                                                              | 分配变化先更新中央意图，原生目标通过同步计划写入                                   |
| **Skills**             | 将 Skill 复制到中央目录，并通过受管符号链接同步到 Claude、Codex、Cursor、ZCode 与 OpenCode 目标                                                                                                                                | 应用前展示目标计划，应用后保留恢复快照                                             |
| **Hooks**              | 在中央库维护生命周期钩子；事件随分配指定。OpenCode 插件回调不兼容当前 command-only 合同，保持暂不支持                                                                                                                          | 受支持工具的分配变化先更新中央意图，OpenCode 不写入                                |
| **Projects**           | 登记并只读扫描本地项目，在项目维度管理 MCP、Skills 与 Hooks；全局提示词不进入项目资源模型                                                                                                                                      | 移除项目登记不会删除或改写已有原生配置                                             |

Hooks 采用统一事件模型：Claude（`settings.json` 的 `hooks` 键）与 ZCode（`~/.zcode/cli/config.json`、项目 `.zcode/config.json` 的 `hooks` 键，恒写 `hooks.enabled: true`）为选择器化子树，Codex 与 Cursor 为独立 `hooks.json`（Cursor 事件键为 camelCase 并额外接管 `version`）。Cursor 当前支持全局提示词，以及全局/项目 MCP、Skills 与 Hooks；Provider、API Key、模型和项目级 Prompt/Rules 均不受支持，也不会被读取或写入。ZCode 支持 Provider、全局 Prompt、MCP、Skills 与 Hooks：Provider 写入 `~/.zcode/v2/config.json` 的 provider 条目，只接管 name/kind/options/enabled，`models` 等应用自管字段原样保留；全局 Prompt 使用用户级 `AGENTS.md`；MCP 使用 `~/.zcode/cli/config.json` 与项目 `.zcode/config.json` 的 `mcp.servers`；Skills 使用 `~/.zcode/skills`。OpenCode 使用 JSON/JSONC 配置（全局 `~/.config/opencode/opencode.json(c)`、项目 `opencode.json` 或 `.opencode/` 覆盖）、全局 `AGENTS.md`、local/remote MCP 与全局/项目 Skills；Provider 不接管 `auth.json`，Hooks 插件回调保持不支持。总览页将中央意图、各工具原生目标状态、同步历史与恢复点放在同一处，便于判断“希望的配置”和“磁盘上的实际配置”是否一致。

## 产品实景

<p align="center">
  <img src="docs/assets/app-overview.png" alt="EasyToAgents 深色模式总览：Claude、Codex、Cursor、ZCode 与 OpenCode 配置状态、项目、冲突、快照和最近同步" width="100%" />
</p>

<p align="center"><sub>总览界面 · 隔离空数据状态，不包含个人配置或项目路径</sub></p>

## 安全同步模型

1. **只读检测**：各类配置在接管前先扫描 Claude、Codex、Cursor、ZCode、OpenCode 的全局目标，或已登记项目中的 MCP、Skills 与 Hooks，不立即写入。
2. **选择性接管**：只把明确选择的 Provider、全局提示词、MCP、Skills 或 Hooks 纳入中央管理。
3. **变更预览**：生成将要创建、更新或移除的目标计划，展示警告、冲突和脱敏差异。
4. **确认应用**：默认仅在确认后写入原生目标，并保留不属于 EasyToAgents 管理的内容。
5. **快照恢复**：成功应用会产生私有恢复点，可从同步历史回到先前状态。

> 设置中也提供跳过确认对话框的直接应用模式；它仍会先生成预览，并且只会自动应用无冲突、无错误且目标未受阻的计划。默认模式始终是“预览并确认”。

首次接管全局 Skill 是直接应用模式的例外：只有名称与完整内容都和中央副本一致的入口才可选择，且仍须确认持久化预览。外部符号链接接管只替换入口，不修改其外部来源；普通目录会先复制为应用私有的目录树恢复点，并持续占用本地空间，直至你显式删除该恢复点。

Prompt 仅支持全局档案、全局分配和全局同步。项目扫描与同步不观察、不创建、不禁用、不恢复或改写项目中的 `CLAUDE.md`、`AGENTS.md`、`.cursor/rules/` 等提示词/规则文件。升级到 v19 时，应用只清理数据库中已退役的项目 Prompt 分配、原生资源记录和私有历史快照；项目目录中的现有文件保持原样，迁移不会使用项目目标路径做删除。

## 快速开始

当前支持 **macOS 13+**。公开安装包仅支持 Apple Silicon（M 系列）Mac；开发者也可以从源码运行。

### 下载安装包

当前稳定版本为 **v0.1.0**，可直接下载 [EasyToAgents_0.1.0_aarch64.dmg](https://github.com/zhexin233-ui/easytoagents/releases/download/v0.1.0/EasyToAgents_0.1.0_aarch64.dmg)。历史版本与发布说明位于 [GitHub Releases](https://github.com/zhexin233-ui/easytoagents/releases)。

安装包 SHA-256：`96342f0f3f6756b7f2cb237f77820559d6494d594602769e24d8921e6ef0b12a`

打开 DMG 后，将 EasyToAgents 拖入“应用程序”目录。当前安装包未使用 Apple Developer 签名，也未经 Apple 公证，因此首次打开可能被 Gatekeeper 拦截。遇到提示时，在“应用程序”中右键 EasyToAgents 并选择“打开”，然后再次确认；也可以前往“系统设置 → 隐私与安全性”，在安全提示旁选择“仍要打开”。

### 环境要求

- Node.js 与 pnpm 10（仓库当前声明 `pnpm@10.13.1`）
- Rust 1.77.2 或更高版本
- Tauri 2 在 macOS 上所需的系统开发环境

### 从源码运行

```bash
git clone https://github.com/zhexin233-ui/easytoagents.git
cd easytoagents
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` 会同时启动 Vite 前端与 Tauri 桌面窗口。

只调试界面时可以运行 `pnpm dev`；涉及配置检测、导入、预览、应用或恢复的功能仍需在 Tauri 窗口中使用。

### 本地构建

```bash
pnpm tauri build
```

项目的 Tauri 配置会在 macOS 本地构建 `.app` 与 `.dmg`。

### 维护者发布版本

发布由 GitHub Actions 手动执行，普通 push 和 Pull Request 不会触发发布：

1. 确认 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 与 `src-tauri/Cargo.lock` 中的应用版本一致。
2. 在仓库的 **Actions** 页面选择 **发布 macOS ARM64 DMG**，点击 **Run workflow**，选择要发布的提交并输入不带 `v` 前缀的版本号。
3. 等待构建、DMG 挂载检查、应用版本和 ARM64 架构检查全部通过。工作流随后创建 `v<版本>` 标签，上传附件，并公开 Release。
4. 从 Release 下载 DMG，确认能正常挂载并显示 EasyToAgents 应用。

同一版本的工作流会串行执行。只有标签仍指向最初发布的提交时才能重跑；重跑会替换该 Release 中同名的 ARM64 DMG，不会移动标签或修改其他版本的附件。构建失败不会创建公开 Release；上传中断可能留下标签和草稿 Release，可在同一提交上重跑恢复。

## 首次使用

1. 从总览点击 **开始首次检测**，只读发现本机已有的 Claude、Codex、ZCode、OpenCode 的 Provider，以及五个工具的全局提示词；Cursor、OpenCode 的 MCP/Skills 也可从对应资源页导入。
2. 按工具选择要导入的 Provider 或全局提示词；不希望接管的内容可以直接跳过。
3. 在中央库中补充 MCP、Skills 与 Hooks，并按全局或项目范围分配；提示词始终按全局范围同步。
4. 生成同步预览，检查目标路径、变更类型、警告、冲突和脱敏差异。
5. 确认应用；需要回退时，从总览的私有快照入口预览并执行恢复。

## 开发

### 常用命令

| 命令                  | 用途                                |
| --------------------- | ----------------------------------- |
| `pnpm dev`            | 启动 Vite 开发服务器                |
| `pnpm tauri dev`      | 以开发模式启动桌面应用              |
| `pnpm tauri build`    | 构建 macOS `.app` 与 `.dmg`         |
| `pnpm build`          | 执行 TypeScript 检查并构建前端      |
| `pnpm test --run`     | 单次运行 Vitest 测试                |
| `pnpm lint`           | 运行 ESLint                         |
| `pnpm typecheck`      | 运行 TypeScript 类型检查            |
| `pnpm bindings:check` | 检查 Rust → TypeScript 绑定是否最新 |
| `pnpm rust:check`     | 检查 Rust 格式、Clippy 与测试       |
| `pnpm check`          | 运行项目完整质量检查                |

### 技术栈

| 层级       | 技术                                              |
| ---------- | ------------------------------------------------- |
| 桌面端     | Tauri 2、Rust、SQLite（rusqlite）                 |
| 界面层     | React 19、TypeScript 6、React Router 7            |
| 状态与数据 | TanStack Query 5                                  |
| 构建与样式 | Vite 8、Tailwind CSS 4                            |
| 测试与质量 | Vitest、Testing Library、ESLint、Prettier、Clippy |

## 当前范围

- 预编译安装包仅支持 Apple Silicon（M 系列）Mac 与 macOS 13+；Intel Mac 和其他桌面平台尚未纳入当前支持范围。
- 公开下载入口为 GitHub Releases，目前没有独立官网。
- 仓库目前未提供 `LICENSE` 文件。
- README 只描述仓库中已经实现且可验证的配置管理流程，不代表所有第三方工具配置都已覆盖。

## 参与贡献

欢迎通过 [Issues](https://github.com/zhexin233-ui/easytoagents/issues) 报告问题或讨论改进，也欢迎提交 [Pull Requests](https://github.com/zhexin233-ui/easytoagents/pulls)。开始修改前，请先说明变更范围；提交前运行：

接入新的工具前，请先阅读[《接入新的工具 Adapter》](docs/maintainers/adding-tool-adapter.md)，从官方证据和 capability matrix 开始，未知能力必须 fail closed。

```bash
pnpm check
git diff --check
```
