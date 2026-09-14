# Pi MCP 接入调研（第三方适配器链路）

- **任务**：`09-14-add-pi-tool-support`，产品决策变更：Pi MCP 由 `Unsupported` 改为 **`Supported`**（用户显式授权接管第三方适配器面）。
- **核验日期**：**2026-09-14**
- **适配器**：`pi-mcp-adapter`
  - **版本 2.33.0**（npm `dist-tags.latest` = 2.33.0，发布于 2026-09-10T20:39:31Z）
  - 许可 **MIT**，`Copyright (c) 2026 Nico Bailon`（`LICENSE:1-3`）
  - 作者 `Nico Bailon`；仓库 `https://github.com/nicobailon/pi-mcp-adapter`（MIT，1465 stars，`pushed_at 2026-09-14T07:06:44Z`，未归档）
  - npm 页面 `https://www.npmjs.com/package/pi-mcp-adapter`
- **宿主**：Pi `@earendil-works/pi-coding-agent` **0.85.1**（本机），`PI_CODING_AGENT_DIR` 未设置时默认 `~/.pi/agent`
- **本机安装副本（主证据）**：`~/.pi/agent/npm/node_modules/pi-mcp-adapter/`（`pi install npm:pi-mcp-adapter` 全局装入 Pi agent dir）

## 0. 证据强度分层

| 层级                  | 含义                                       | 本报告记号 | 具体来源                                                                                                                                              |
| --------------------- | ------------------------------------------ | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| **L1 本地源码**       | 2.33.0 随包 TS/JS，行号可复核              | 【源】     | `~/.pi/agent/npm/node_modules/pi-mcp-adapter/{config.ts,agent-dir.ts,index.ts,package-mcp-loader.ts,cli.js,README.md,LICENSE,CHANGELOG.md,dist/*.js}` |
| **L2 隔离实测**       | `PI_CODING_AGENT_DIR`/`HOME` 指向 tmp 实跑 | 【机】     | §7 三条 smoke（本轮新增，可复现命令在文末）                                                                                                           |
| **L3 官方文档（Pi）** | Pi 核心文档，非适配器文档                  | 【官】     | 本机 `$DOCS = ~/.volta/.../pi-coding-agent/.../docs/{security.md,packages.md,settings.md,usage.md}`                                                   |
| **L4 外部元数据**     | npm registry / GitHub API                  | 【外】     | `registry.npmjs.org/pi-mcp-adapter`、`api.github.com/repos/nicobailon/pi-mcp-adapter`（访问日期 2026-09-14）                                          |

**证据哈希（2026-09-14，SHA-256）**：

```
271f7a11108f4cc7067f0c24572057aa1fdfb0664aae8ecb5aeeb8a76a5a2b21  package.json
de8a57dda4f80fcbaa2d0dfca8c37bddf6e1b2f97ecfae5fb53f848856617845  config.ts
72af0c49eb6acc34282d97975bd358501b46b598a0b153d32c21ef5929461614  agent-dir.ts
f709bad46aeb85798d7b059cea45b0ad2d98ab921dba58181e84fe729d9018d8  dist/config.js
02090557d6dd2ac22822b0a1c0653faa5646e32d721a54d870a7ce2b36be7511  README.md
```

> **重要限定**：Pi **核心**依然「无内置 MCP」（【官】`docs/usage.md` Design Principles；旧调研 `mcp-hooks-agents.md` O2/O3/O11 不变）。本报告的 `Supported` **专指**「Pi + 已安装并已启用的 `pi-mcp-adapter`」这一**第三方扩展链路**，且该链路是**用户显式授权的写例外**——它推翻了「第三方配置不能单独授权写入」的默认合同，因此本报告把「何时静默无效」列为第一等公民。

---

## 1. 路径、发现顺序与 writePath 归属

### 1.1 路径常量（【源】`config.ts`，行号为 2.33.0）

| 常量/函数                          | 值                                                                                    | 行号                 |
| ---------------------------------- | ------------------------------------------------------------------------------------- | -------------------- |
| `GENERIC_GLOBAL_CONFIG_PATH`       | `~/.config/mcp/mcp.json`                                                              | `config.ts:13`       |
| `AGENTS_GLOBAL_CONFIG_PATHS`       | `[~/.agents/mcp.json, ~/.agents/mcp/mcp.json]`                                        | `config.ts:14-17`    |
| `PROJECT_CONFIG_NAME`              | `.mcp.json`                                                                           | `config.ts:18`       |
| `PROJECT_PI_CONFIG_NAME`           | `mcp.json`                                                                            | `config.ts:19`       |
| `getPiGlobalConfigPath(override?)` | `<agent dir>/mcp.json`（或 `--mcp-config` override）                                  | `config.ts:180-182`  |
| `getProjectConfigPath(cwd)`        | `<cwd>/.mcp.json`                                                                     | `config.ts:188-190`  |
| `getProjectPiConfigPath(cwd)`      | `<cwd>/<configDirName>/mcp.json`，默认 `<cwd>/.pi/mcp.json`                           | `config.ts:192-194`  |
| `getConfigDirName()`               | `piConfig.configDir`（来自 `PI_PACKAGE_DIR` 清单）否则 `.pi`                          | `agent-dir.ts:5-8`   |
| `getAgentDir()`                    | `$<APP>_CODING_AGENT_DIR`（默认 `PI_CODING_AGENT_DIR`）否则 `~/<configDirName>/agent` | `agent-dir.ts:10-25` |
| `isExclusiveConfigMode()`          | `PI_MCP_CONFIG_MODE === "exclusive"`（大小写/空白不敏感）                             | `config.ts:532-534`  |

### 1.2 发现顺序与 writePath（【源】`getConfigSources()`，`config.ts:446-530`；【机】smoke-1）

`loadMcpConfig()` 按 `getConfigSources()` 的返回顺序**依次** `mergeConfigs`，**后者赢**（`config.ts:321-328`）。因此顺序即优先级（低 → 高）：

| #   | id                     | readPath                 | writePath                                  | kind      | scope   | 说明                                         |
| --- | ---------------------- | ------------------------ | ------------------------------------------ | --------- | ------- | -------------------------------------------- |
| 1   | `shared-global`        | `~/.config/mcp/mcp.json` | **`<agent dir>/mcp.json`**（`= userPath`） | `import`  | global  | 跨工具共享（Claude/Cursor 等也读）           |
| 2   | `agents-global`        | `~/.agents/mcp.json`     | **`<agent dir>/mcp.json`**                 | `import`  | global  | 跨工具共享（agent-plugins.org 约定）         |
| 3   | `agents-nested-global` | `~/.agents/mcp/mcp.json` | **`<agent dir>/mcp.json`**                 | `import`  | global  | 同上                                         |
| 4   | `pi-global`            | `<agent dir>/mcp.json`   | `<agent dir>/mcp.json`                     | `user`    | global  | **Pi 自有全局覆盖，EasyToAgents 全局写入面** |
| 5   | `shared-project`       | `<cwd>/.mcp.json`        | `<cwd>/.mcp.json`                          | `project` | project | 跨工具共享项目文件（Claude 也读）            |
| 6   | `pi-project`           | `<cwd>/.pi/mcp.json`     | `<cwd>/.pi/mcp.json`                       | `project` | project | **Pi 自有项目覆盖，EasyToAgents 项目写入面** |

**关键结论 1**：`writePath` 字段证明了「共享文件是**导入源**，Pi 自有文件是**写入靶**」——共享 global 的 1/2/3 号源 `writePath` 全部指向 `<agent dir>/mcp.json`。适配器把从共享文件导入的 server 定义（含凭据）在需要持久化 Pi 专用字段（如 `directTools`）时写回 **Pi 自有文件**（【源】`getServerProvenance()` `config.ts:1059-1099` + `writeDirectToolsConfig()` `config.ts:1101-1128`；【机】smoke-3）。

**关键结论 2**：`~/.config/mcp/mcp.json` 与 `~/.agents/mcp.json` 的优先级**低于** `<agent dir>/mcp.json`，所以它们**不能静默覆盖我们的全局写入**（【机】smoke-B：`dup` 仍为 Pi 文件的 `url`）。真正的覆盖风险来自**项目层**（#5、#6，优先级高于 #4）。

**关键结论 3**：exclusive 模式下 `getConfigSources()` 只返回 `pi-global` 一个源（【源】`config.ts:456-465`；【机】smoke-3），共享文件与项目文件**全部不参与**。

### 1.3 为什么 EasyToAgents 只该接管这两个文件

| 候选                                            | 是否作为写入靶    | 理由（证据）                                                                                                                                                                                                     |
| ----------------------------------------------- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<agent dir>/mcp.json`                          | ✅ **全局写入靶** | Pi 自有全局覆盖；优先级高于全部共享 global；`writePath` 目标本身（`config.ts:488-497`）                                                                                                                          |
| `<cwd>/.pi/mcp.json`                            | ✅ **项目写入靶** | Pi 自有项目覆盖；全链最高优先级、最后 merge（`config.ts:514-524`）；README「File Layout」明列                                                                                                                    |
| `~/.config/mcp/mcp.json`                        | ❌ 只读冲突检测   | (a) 跨工具共享，写它会外溢到 Claude/Cursor 等；(b) 优先级**低于** Pi 文件，写这里的受管项可能被 `<agent dir>/mcp.json` 静默盖掉；(c) 非 Pi 专有，易与其他工具的 descriptor 争 ownership                          |
| `~/.agents/mcp.json` / `~/.agents/mcp/mcp.json` | ❌ 只读冲突检测   | 同上；且 `~/.agents/*` 是跨工具约定（不是 Pi 目录）                                                                                                                                                              |
| `<cwd>/.mcp.json`                               | ❌ 只读冲突检测   | (a) 跨工具共享；(b) **已被本仓库 Claude 项目 MCP descriptor 占用**（`adapters/claude/mod.rs:126` 路径 `<root>/.mcp.json`），若 Pi 也写会产生同文件双工具 ownership 冲突；(c) 写入 Pi 专属 server 会泄漏给 Claude |

> **推论**：EasyToAgents 只需两个 MCP descriptor：`Mcp × Global → <agent dir>/mcp.json`、`Mcp × Project → <root>/.pi/mcp.json`。共享文件全部降级为「冲突/遮蔽诊断输入」，绝不写入。

---

## 2. 合并 / 覆盖语义（精确证据）

### 2.1 merge 算法

`mergeConfigs(base, next)` → `mergeServerMaps(base.mcpServers, next.mcpServers)`，**按 server 名、逐字段**合并，`next` 覆盖 `base`（【源】`config.ts:536-628`）。

- 同名 server：高优先级源的字段覆盖低优先级源的同名字段；未给出的字段**继承**低优先级源。
- 传输类型切换会清理不兼容字段（`command` ↔ `url` ↔ `socket` 互斥组，`config.ts:566-600`）。
- **凭据/URL 绑定安全**：若高优先级源改了 `url`，低优先级源的 `headers`/`bearerToken`/`bearerTokenEnv`/`bearerTokenStore`/`requestHeadersCommand`/`caFile`/`oauth` 会被**丢弃**，避免把旧端点凭据发到新端点（`URL_BOUND_AUTH_FIELDS`，`config.ts:545-556, 601-610`）。
- `settings` 浅合并（高覆盖低）；`imports` 并集去重；`claudePlugins` 高优先级整体替换（`config.ts:537-543`）。

### 2.2 同名 server 谁赢（顺序总结）

```
~/.config/mcp/mcp.json  <  ~/.agents/mcp.json  <  ~/.agents/mcp/mcp.json
   <  <agent dir>/mcp.json  <  <cwd>/.mcp.json  <  <cwd>/.pi/mcp.json
```

（`pi-mcp-adapter` 的 `imports` / `hostConfigDiscovery:"on"` / Pi package `pi.mcp` / Agent Plugins / Claude 插件是**更低**的兜底层，见 `config.ts:329-371`；它们不会覆盖以上正常源。）

### 2.3 「Pi 自有文件永远赢」是**错的**——这是必须诊断的失效场景

- 项目 `.mcp.json`（#5）优先级**高于** `<agent dir>/mcp.json`（#4）。
- 项目 `.pi/mcp.json`（#6）优先级**最高**。

【机】smoke-2 / smoke-4 实测：`<agent dir>/mcp.json` 里 `github.url = https://api.githubcopilot.com/mcp`，同时 `<cwd>/.mcp.json` 里 `github.command = "project-github"` → 实际生效 `{"command":"project-github"}`。

适配器自带冲突上报 `getConfigConflicts()`（【源】`config.ts:394-444`），其 `winner` 取 `sources[last]`，实测输出：

```json
{
  "serverName": "github",
  "sources": [
    { "kind": "shared", "path": "~/.config/mcp/mcp.json" },
    { "kind": "pi", "path": "<agent dir>/mcp.json" },
    { "kind": "shared", "path": "<cwd>/.mcp.json" }
  ],
  "winner": { "kind": "shared", "path": "<cwd>/.mcp.json" }
}
```

→ EasyToAgents 必须在 Preview/Apply 前做**同名遮蔽检测**：
`PI_MCP_SHADOWED_BY_PROJECT_SHARED`（被 `<root>/.mcp.json` 覆盖）与 `PI_MCP_SHADOWED_BY_PROJECT_PI`（被 `<root>/.pi/mcp.json` 覆盖）。**项目 Pi 覆盖是最高优先级，不构成失效**；被 `.mcp.json` 覆盖、或被用户手写的 `.pi/mcp.json` 条目覆盖才是。

> 反向（非失败、但需诚实展示）：写入 `<agent dir>/mcp.json` 会**遮蔽**同名共享 global 项；写入 `<root>/.pi/mcp.json` 会**遮蔽**同名 `.mcp.json` 项。这不是 bug，是适配器文档化的优先级（README「Project Config」：「Project files override both user-global shared MCP config and Pi global overrides.」）。

---

## 3. 项目作用域与 trust

### 3.1 `.pi/mcp.json` **不受** Pi project trust 门禁

- Pi 官方 trust 资源清单（【官】`$DOCS/security.md`：「Project Trust」）：`.pi/settings.json`、`.pi/extensions`、`.pi/skills`、`.pi/prompts`、`.pi/themes`、`.pi/SYSTEM.md`、`.pi/APPEND_SYSTEM.md`、项目 `.agents/skills`。**不含** `.pi/mcp.json`，也**不含** `.mcp.json`。
- 全局安装的 Pi 扩展在 trust 解析**之前**加载（【官】`security.md`：「Before trust is resolved, pi only loads context files, user/global extensions, and CLI `-e` extensions.」）。
- 适配器源码**完全没有** trust 读取（【源】`grep -n "trust|project_trust" index.ts` 零命中；`config.ts` 零命中）；它直接用 `process.cwd()` 读 `.pi/mcp.json`（`getProjectPiConfigPath(cwd = process.cwd())`）。

**结论**：只要适配器作为**用户/全局** package 启用，**未受信任项目的 `.pi/mcp.json` 依然生效**。EasyToAgents 若如实建模，Pi 项目 MCP descriptor 的 `trust` 应为 `NotRequired`（不同于 Pi Skills 的 trust-gated）。

- 唯一例外：若适配器通过**项目** `.pi/settings.json` 的 `packages` 安装/启用，则它本身受 trust 门禁（项目 package 需 trust 才加载，【官】`packages.md`「Project settings can be shared with your team, and pi installs any missing packages automatically on startup after the project is trusted.」）。

### 3.2 给 EasyToAgents 的建议

- 状态文案必须显式说明：**`.pi/mcp.json` 不受 Pi trust 门禁**；未信任项目也会加载。
- 这是**安全提示**而非阻断项：写入 `.pi/mcp.json` 的服务器会在任何人打开该项目时被执行（stdio `command`）。建议在项目 Apply 前给一次确认提示，但不设为硬阻断（否则与 Pi 实际行为不符，且用户无法在未信任仓库里使用 Pi MCP）。
- 若适配器是**项目级**启用（`.pi/settings.json packages`），则应读 `.pi/settings.json` 并沿用 Pi 的 trust 语义（未信任 → `PI_MCP_ADAPTER_NOT_LOADED`）。

---

## 4. 安装 / 启用探测：写入何时静默无效

### 4.1 判定链（全部可只读探测）

| 判据                                                                                          | 可靠性                   | 证据                                                                                                          | 说明                                                          |
| --------------------------------------------------------------------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `<agent dir>/settings.json` `.packages[]` 含 `npm:pi-mcp-adapter`（字符串或 `{source}` 对象） | **可靠（启用声明）**     | 【官】`$DOCS/settings.md:285,294-318`；【源】`package-mcp-loader.ts:59-83` 解析同样的 `packages` 结构         | 声明来源；但被 `pi config` 禁用时可能带 `extensions: []` 过滤 |
| `<agent dir>/npm/node_modules/pi-mcp-adapter/package.json` 存在                               | **可靠（已下载）**       | 【官】`$DOCS/packages.md`：user 装到 `~/.pi/agent/npm/`；【外】本机实测存在                                   | 只能证明文件在磁盘，不证明会被加载                            |
| `<agent dir>/npm/node_modules/pi-mcp-adapter/package.json` 的 `version`                       | **可靠（版本）**         | 【源】`package.json`                                                                                          | 用于版本探测与 fail-closed                                    |
| `<project>/.pi/npm/node_modules/pi-mcp-adapter/package.json`                                  | **可靠（项目级安装）**   | 【官】`$DOCS/packages.md`：`pi install -l` → `.pi/npm/`                                                       | 受 project trust 门禁                                         |
| `<project>/.pi/settings.json` `.packages[]` 含适配器                                          | **可靠（项目启用声明）** | 同上                                                                                                          | 未信任时**不会加载**                                          |
| `settings.json packages` 为对象形且 `extensions: []`（`pi config` 禁用扩展）                  | **可靠（未加载）**       | 【官】`$DOCS/packages.md`「Enable and Disable Resources」+「Package Filtering」(`extensions: []` = load none) | → `PI_MCP_ADAPTER_NOT_LOADED`                                 |
| `pi list` 输出含 `npm:pi-mcp-adapter` + 解析路径                                              | **中等**                 | 【机】`pi list` 非交互、零额度、输出 10 行                                                                    | 只能证明「声明 + 解析路径」；不反映 `pi config` 的禁用过滤    |
| `<agent dir>/git/...`                                                                         | **不可靠**               | —                                                                                                             | 仅 git 源安装；npm 安装时不存在，不能作否定判据               |
| 探测运行时是否真的注册了扩展                                                                  | **不可行（静态）**       | 需要跑 Pi 会话 → 消耗额度                                                                                     | 列为 Unknown                                                  |

### 4.2 静默无效矩阵

| 状态                                                        | 写入 `<agent dir>/mcp.json` / `.pi/mcp.json` 的效果                                         | 建议诊断码                                                   |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `pi` 二进制未安装                                           | 完全无意义（无人读）                                                                        | 走 `ToolAvailability::Unavailable`（`tool_not_installed()`） |
| `pi` 已装，但适配器未出现在任何 `packages` 且安装目录不存在 | **静默无效**（Pi 核心不读 mcp.json）                                                        | `PI_MCP_ADAPTER_MISSING`                                     |
| 适配器 `packages` 已声明、但安装目录缺失                    | Pi 启动时会尝试自动安装；期间**静默无效**                                                   | `PI_MCP_ADAPTER_MISSING`（或 `..._PENDING_INSTALL`）         |
| 安装目录存在，但 `pi config` 把扩展禁用（`extensions: []`） | **静默无效**（扩展不加载，文件无人读）                                                      | `PI_MCP_ADAPTER_NOT_LOADED`                                  |
| 适配器加载成功                                              | 生效                                                                                        | —                                                            |
| 适配器版本 < 已知最低支持版本                               | 行为未知，fail closed                                                                       | `PI_MCP_ADAPTER_VERSION_UNSUPPORTED`                         |
| `PI_MCP_CONFIG_MODE=exclusive`                              | 只读 `<agent dir>/mcp.json`，共享源被忽略；写入全局仍生效，写入 `.pi/mcp.json` **静默无效** | `PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED`                      |

### 4.3 建议：适配器未就绪时**必须 fail closed（禁止 Apply）**

理由：

1. **「必然无效的写入不得静默执行」是本任务已确立的产品原则**（PRD R5 对 `AGENTS.override.md` 的硬阻断、design.md §7）。适配器缺失时写 `mcp.json` 是**字面意义上的必然无效**。
2. EasyToAgents 的 descriptor 语义里，单文件 MCP 目标被拒绝写入时走的是 `TargetCapability::unsupported(code)` → 目标不可用（与 Claude/Codex 探针失败同构）。这样**读、写、分配**整体 fail closed，零旁路。
3. 允许「能导入但不能应用」需要新增与 `prompt_override` 类似的双态机制，成本高且易产生半生效错觉；MVP 不必引入。

**推荐实现**：适配器缺失/未加载/版本不支持 → `Mcp × Global` 与 `Mcp × Project` 两个 descriptor 的 `capability = TargetCapability::unsupported("PI_MCP_ADAPTER_MISSING" | "..._NOT_LOADED" | "..._VERSION_UNSUPPORTED")`，`path = None`，不生成 assignment/预览/写入。诊断详情在总览与 MCP 页展示安装指引（`pi install npm:pi-mcp-adapter`）。

---

## 5. 适配器自身也会写这两个文件：冲突与丢字段防范

**已证实的适配器写入路径**（都会写 `<agent dir>/mcp.json` 或 `.pi/mcp.json`）：

| 适配器函数                                                | 目标                                                                                                                           | 证据                                                            |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| `writeDirectToolsConfig(changes, provenance, fullConfig)` | `provenance.path`：import 源 → `<agent dir>/mcp.json`；`pi-global` → 自身；`pi-project` → 自身；`shared-project` → `.mcp.json` | 【源】`config.ts:1101-1128`；【机】smoke-3                      |
| `ensureCompatibilityImports()`                            | `<agent dir>/mcp.json`（写 `imports` 数组）                                                                                    | 【源】`config.ts:1006-1019`                                     |
| `writeProjectServerDisabledOverride()`                    | `.pi/mcp.json`（只写 `mcpServers.<name>.disabled`）                                                                            | 【源】`config.ts:1053-1099`；`index.ts:1035`、`commands.ts:665` |
| `writeSharedServerEntry()` / `writeStarterSharedConfig()` | 显式 target：`~/.config/mcp/mcp.json` 或 `.mcp.json`（**不是** Pi 自有文件，除非 target 就是它）                               | 【源】`config.ts:1043-1057`；`commands.ts:508-531`              |
| `mcp({ action: "install" })`                              | 全局 → Pi 全局配置；`target:"project"` → `.mcp.json`                                                                           | 【源】README「Install from one URL」                            |

**序列化格式**（决定是否「我们写、适配器又改写」造成持续漂移）：

- 读：`parseJsonWithComments`（允许注释与尾逗号，`utils.ts:8-10`）。
- 写：`writeRawConfigObject` → `JSON.stringify(raw, null, 2) + "\n"`，先写 `.<pid>.tmp` 再 `rename`（原子）（【源】`config.ts:1040-1051`）。
- 键顺序：`JSON.parse` 保留插入顺序，因此**未改动的键顺序原样保留**；仅新增字段追加在条目末尾（【机】smoke-3：`shared-only` 定义被追加，`settings` 保持在末尾且内容不变）。

**EasyToAgents 侧的对齐结论（好消息）**：

1. EasyToAgents 现有 `render_document` 对 `Json + Selectors` 用 `serde_json::to_vec_pretty` + `\n`（`adapters/document.rs:627-650`），与适配器 **2 空格 + 尾换行** 一致。
2. EasyToAgents 的 MCP ownership 是**条目级**：`build_mcp_ownership()` 生成 `["mcpServers", "<name>"]` 选择器（`mcp/service.rs:812-829`），`set_json_path` 只替换该叶子（`adapters/document.rs:850-875`）。因此**非受管条目（含适配器写入的 `directTools`/import 定义）会被保留**，不会被整容器覆盖。
3. 受影响面收敛为：**受管条目**若被适配器追加 `directTools`，EasyToAgents 的条目哈希会变 → 触发漂移 → 需用户重新接管。这是**正确且安全**的行为，不是数据丢失；必须写进文档与 UI 文案。

**必须遵守的写入纪律**：

- Apply 前重新读取目标文件（read-modify-write），永远基于磁盘当前内容做条目级替换；禁止用内存快照整文件覆写。
- 写入前先跑遮蔽检测（§2.3）。若受管条目会被 `.mcp.json` 覆盖 → 硬阻断，避免「写了但无效」。
- `settings` / `imports` / `claudePlugins` / 非受管 `mcpServers.*` **一律不写**（`managed_selectors = ["mcpServers"]` + 条目级 ownership 自然满足）。
- **绝不要**把 `<agent dir>/mcp.json` 视为 EasyToAgents 独占文件；它是适配器的持久化靶，双方共享。

---

## 6. Schema：受管字段 / 必须保留 / 敏感字段

### 6.1 顶层文件结构（2.33.0，【源】`config.ts:726-740` `validateConfig`，`types.d.ts`）

```jsonc
{
  "mcpServers": { "<name>": ServerEntry },   // 受管（条目级）
  "imports": ["cursor", "claude-code", ...], // 适配器/用户自有 —— 只读，不写
  "settings": { ... },                        // 适配器/用户自有 —— 只读，不写
  "claudePlugins": [{ "path": ..., "mcp": true, "skills": true }] // 只读，不写
}
```

`mcpServers` 也接受别名 `mcp-servers`（`config.ts:733`）；EasyToAgents 写入统一用 `mcpServers`（与 `native_mcp_container` 一致）。

### 6.2 `ServerEntry` 受管字段（【源】`dist/types.d.ts:265-332`，README「Server Options」）

| 字段                               | 形态                                                                                   | EasyToAgents 结构化映射                                                                                                                                                        |
| ---------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `command`                          | string，stdio                                                                          | `McpTransport::Stdio.command`                                                                                                                                                  |
| `args`                             | string[]                                                                               | `args`                                                                                                                                                                         |
| `env`                              | `Record<string,string>`，支持 `${VAR}`/`$env:VAR`/`{env:VAR}`，`!` 前缀 = 连时执行命令 | `env`（BTreeMap）                                                                                                                                                              |
| `inheritEnv`                       | bool（默认 true）                                                                      | `extra`                                                                                                                                                                        |
| `literalEnv`                       | bool                                                                                   | `extra`                                                                                                                                                                        |
| `cwd`                              | string，支持插值/`~`                                                                   | `extra`                                                                                                                                                                        |
| `socket`                           | string（rmcp-mux unix socket），与 command/url 互斥                                    | `extra`（**注意**：EasyToAgents `McpTransport` 只有 stdio/http，socket 只能走 `extra`，且不能与 command/url 同时给出）                                                         |
| `url`                              | string，支持插值，缺失变量 fail before request                                         | `McpTransport::StreamableHttp.url`                                                                                                                                             |
| `headers`                          | `Record<string,string>`，支持插值，`!` = 执行命令                                      | `headers`（BTreeMap）                                                                                                                                                          |
| `requestHeadersCommand`            | `{command,args?,env?,timeoutMs?}`                                                      | `extra`                                                                                                                                                                        |
| `caFile`                           | string（PEM 路径，支持插值/`~`）                                                       | `extra`                                                                                                                                                                        |
| `auth`                             | `"bearer" \| "oauth" \| false`                                                         | `extra`                                                                                                                                                                        |
| `bearerToken`                      | string，支持插值，`!` = 执行命令                                                       | `extra`（敏感）                                                                                                                                                                |
| `bearerTokenEnv`                   | string（env 变量名）                                                                   | `extra`                                                                                                                                                                        |
| `bearerTokenStore`                 | `true`（OS keyring）                                                                   | `extra`                                                                                                                                                                        |
| `oauth`                            | `false \| OAuthConfig`                                                                 | `extra`（敏感）                                                                                                                                                                |
| `lifecycle`                        | `"lazy"(默认) \| "eager" \| "keep-alive" \| "lazy-keep-alive"`                         | `extra`                                                                                                                                                                        |
| `idleTimeout` / `requestTimeoutMs` | number                                                                                 | `extra`                                                                                                                                                                        |
| `exposeResources`                  | bool（默认 true）                                                                      | `extra`                                                                                                                                                                        |
| `directTools`                      | `bool \| string[] \| "search"`                                                         | `extra`                                                                                                                                                                        |
| `toolPrefix`                       | `"server"\|"short"\|"none"\|"mcp"`                                                     | `extra`                                                                                                                                                                        |
| `includeTools`/`excludeTools`      | string[]                                                                               | `extra`                                                                                                                                                                        |
| `searchKeywords`                   | `Record<string,string[]>`                                                              | `extra`                                                                                                                                                                        |
| `approveTools`                     | `bool \| string[]`                                                                     | `extra`                                                                                                                                                                        |
| `debug` / `trace`                  | bool                                                                                   | `extra`                                                                                                                                                                        |
| `httpTransport`                    | `"streamable-http"\|"sse"`                                                             | `extra`                                                                                                                                                                        |
| `pluginDataDir`                    | string                                                                                 | `extra`                                                                                                                                                                        |
| `protocolVersion`                  | `"legacy"(默认)\|"auto"\|"2026-07-28"`                                                 | `extra`                                                                                                                                                                        |
| `disabled`                         | `true` 才禁用（**反向语义**）                                                          | 由 `enabled` 反推：`enabled=false` → `disabled:true`；读时 `disabled===true` → `enabled=false`。`RESERVED_EXTRA_KEYS` 已含 `disabled`，不能进 `extra`（`mcp/models.rs:19-32`） |

**注意**：适配器**没有** `type` 字段（与 Claude/OpenCode 不同），传输类型由 `command` / `url` / `socket` 的存在性推断。Pi 的 `native_mcp_item` 分支**不得**插入 `"type"`（否则被当作未知字段原样保留，无害但污染；且 `type` 在 EasyToAgents 里是 reserved extra key）。

### 6.3 必须保留的未知字段

`validateConfig` 会丢弃未知**顶层**键？——不会：它只**构造**出 `{mcpServers, imports?, settings?, claudePlugins?}`，但**写入路径**走 `readRawConfigObject()`（保留原始顶层对象全部键），只在 `mcpServers`/`imports` 上做增删（【源】`config.ts:1027-1051, 1043-1057, 1101-1128`）。因此：

- 顶层未知键（例如未来版本新增）→ 适配器保留。
- `ServerEntry` 内未知字段 → `toServerEntries` 用 `isRecord` 全量保留（`config.ts:757-767`）。
- → EasyToAgents 必须走**条目级** `extra`/未知字段保留（现有 MCP `extra` 机制天然支持，上限 64KiB/depth 8，`mcp/models.rs:15-17, 523-547`）。

### 6.4 敏感 selector（用于 preview/diff 脱敏，不参与写）

**必须（Primary）**：

```
mcpServers/*/env
mcpServers/*/headers
mcpServers/*/bearerToken
mcpServers/*/bearerTokenEnv
mcpServers/*/oauth
mcpServers/*/requestHeadersCommand/env
mcpServers/*/requestHeadersCommand/args
```

**建议（Secondary，可能承载凭据）**：

```
mcpServers/*/args                 # 常见 --api-key=... / --token=...
mcpServers/*/url                  # userinfo: https://user:pass@host
mcpServers/*/requestHeadersCommand/command
mcpServers/*/caFile               # 路径，低敏感
mcpServers/*/socket               # 路径，低敏感
mcpServers/*/cwd                  # 路径，低敏感
mcpServers/*/pluginDataDir
```

现有 `SecretRedactor` 已按启发式命中的键：`authorization/apikey/token/secret/password/passphrase/cookie/credential/bearer`（`security/mod.rs:202-217`），容器键 `headers/env/environment` 强制全脱敏（`security/mod.rs:219-224`）。因此 `bearerToken`/`clientSecret` 会被启发式覆盖，但 **`oauth.clientId`、`requestHeadersCommand.args/env`、`args` 不会被键名启发式覆盖**，必须显式 selector。

### 6.5 「不得求值」的语法（写/预览/哈希/渲染时一律不展开、不执行）

- `${VAR}`、`$env:VAR`、`{env:VAR}` —— 插值语法（【源】`utils.ts:105-113`，`interpolateEnvVars`）。
- `!command` —— **执行命令取秘密**；`!!` 转义为字面 `!`（【源】`utils.ts:126-129`，`interpolateSecretExpression`）。
- 适用字段：`env`、`headers`、`bearerToken`、`oauth.clientSecret`、`url`、`socket`、`cwd`、`caFile`、`requestHeadersCommand.*`（README「Server Options」+「Stdio environment boundaries」）。
- **官方保证**：README L370「Commands are **not run during the preliminary MCP OAuth challenge probe or while reading, merging, previewing, hashing, or rendering configuration**.」→ EasyToAgents 只要不 shell out 就安全；必须确保导入/预览/哈希链路**零命令执行**，且**逐字节保留**表达式原文。
- 现有 `McpServerInput` 只结构化 `env`/`headers`，其余进 `extra`；只要不做插值即满足。需在 `native_mcp_item` 的 Pi 分支与 `extra` 校验中**禁止**任何字符串变换。

### 6.6 适配器自有旁路文件（绝对不读不写）

| 路径                                    | 用途                        | 证据                                              |
| --------------------------------------- | --------------------------- | ------------------------------------------------- |
| `<agent dir>/mcp-oauth/`                | 遗留 OAuth token 导入目录   | `mcp-auth.ts:421`（`MCP_OAUTH_DIR` 优先）         |
| OS 凭据库（Nico keyring）               | `bearerTokenStore` 令牌存储 | README「bearerTokenStore」；`mcp-bearer-store.ts` |
| `<agent dir>/mcp-cache.json`            | 工具元数据缓存              | `metadata-cache.ts:41`                            |
| `<agent dir>/mcp-npx-cache.json`        | npx 解析缓存                | `npx-resolver.ts:434`                             |
| `<agent dir>/mcp-onboarding.json`       | 引导状态                    | `onboarding-state.ts:19`                          |
| `<agent dir>/agent-plugin-data/<name>/` | Agent Plugin 数据目录       | `agent-plugin-loader.ts:211`                      |
| `<cwd>/.pi/mcp-traces/*.jsonl`          | 协议 trace（默认）          | `mcp-trace.ts:208-215`                            |
| `<agent dir>/settings.json`             | Pi 设置（`packages`）       | 只读探测，不写                                    |
| `<agent dir>/auth.json` 等              | 旧调研已列的只读禁区        | `verified-contract.md` §2                         |

---

## 7. 隔离 smoke（已实跑，可复现）

隔离手段：`HOME=<tmp>/home` + `PI_CODING_AGENT_DIR=<tmp>/agent`，**未触碰真实 `~/.pi/agent`**。库级 import：`~/.pi/agent/npm/node_modules/pi-mcp-adapter/dist/config.js`。

### smoke-1：路径与发现顺序

```bash
HOME=$TMP/home PI_CODING_AGENT_DIR=$TMP/agent node --input-type=module -e "
import { getPiGlobalConfigPath, getProjectPiConfigPath, getConfigDiscoveryPaths } from '$PKG/dist/config.js';
console.log(getPiGlobalConfigPath(), getProjectPiConfigPath('$TMP/proj'));
for (const p of getConfigDiscoveryPaths(undefined, '$TMP/proj')) console.log(p.label, p.path, p.exists);
"
```

结果：`getPiGlobalConfigPath()` = `<tmp>/agent/mcp.json`（**证明 `agent-dir.ts` 尊重 `PI_CODING_AGENT_DIR`**）；`getProjectPiConfigPath()` = `<tmp>/proj/.pi/mcp.json`；发现顺序 = shared-global → agents-global → agents-nested-global → pi-global → shared-project → pi-project（与 §1.2 一致）。

### smoke-2：最小 stdio + http 定义可被解析，且项目文件覆盖全局

fixture：`<tmp>/agent/mcp.json` = `{mcpServers:{pi-only:{url:...}, dup:{url:...}}}`；`<tmp>/proj/.mcp.json` = `{mcpServers:{dup:{command:"project-shared-cmd"}}}`；`<tmp>/proj/.pi/mcp.json` = `{mcpServers:{dup:{url:"https://proj-pi.../dup"}}}`；`<tmp>/home/.config/mcp/mcp.json` 有 `shared-only`。

结果：`loadMcpConfig()` 解析出 stdio（`shared-only.command`）与 http（`pi-only.url`）两类；`dup` = 项目 Pi 的 url（最高优先级）。移除 `.pi/mcp.json` 后，`dup` = `.mcp.json` 的 command（**证明项目共享文件静默覆盖 `<agent dir>/mcp.json`**）；仅保留共享 global 时 `dup` = Pi 全局 url（**证明共享 global 不能覆盖 Pi 全局**）。

### smoke-3：适配器自身写 `<agent dir>/mcp.json`；exclusive 模式忽略共享

```bash
HOME=$TMP/home PI_CODING_AGENT_DIR=$TMP/agent node --input-type=module -e "
import { loadMcpConfig, getServerProvenance, writeDirectToolsConfig } from '$PKG/dist/config.js';
writeDirectToolsConfig(new Map([['shared-only', true]]), getServerProvenance(undefined,'$TMP/proj'), loadMcpConfig(undefined,'$TMP/proj'));
"
```

结果：`<agent dir>/mcp.json` 被重写为 2 空格 + 尾换行，新增 `shared-only: {command:"echo", directTools:true}`（**完整定义从共享文件被搬入 Pi 文件**），原 `settings` 与既有条目原样保留，共享文件未被改动。exclusive 模式下 `getConfigDiscoveryPaths()` 仅返回 `<agent dir>/mcp.json`，`loadMcpConfig()` 只含 Pi 文件条目。

**未做**：真实 `pi --offline -p` 端到端运行（会消耗模型额度，且无法证明「扩展已加载」以外的更多信息）；`pi-mcp-adapter token`（依赖 OS keyring 交互）。二者列 Unknown。

### 复现脚本骨架

```bash
TMP=$(mktemp -d); PKG=~/.pi/agent/npm/node_modules/pi-mcp-adapter
mkdir -p $TMP/home/.config/mcp $TMP/agent $TMP/proj/.pi
# ... 写 fixture ...（见 smoke-1/2/3）
HOME=$TMP/home PI_CODING_AGENT_DIR=$TMP/agent node --input-type=module -e "import {...} from '$PKG/dist/config.js'; ..."
```

---

## 8. 能力矩阵行（MCP）

| Artifact | Scope   | Import        | Apply         | 说明                                                                                                                  |
| -------- | ------- | ------------- | ------------- | --------------------------------------------------------------------------------------------------------------------- |
| **MCP**  | Global  | **Supported** | **Supported** | 目标 `<agent dir>/mcp.json`；`mcpServers` 条目级；**前置**：适配器已安装且已加载                                      |
| **MCP**  | Project | **Supported** | **Supported** | 目标 `<root>/.pi/mcp.json`；条目级；**不受 Pi trust 门禁**；受 project 级 packages/trust 影响（仅当适配器项目级启用） |

条件态（descriptor `capability`）：

- `pi` 未安装 → `ToolNotInstalled`（两行都不可用）。
- 适配器缺失 → `Unsupported(PI_MCP_ADAPTER_MISSING)`（两行都不可用，fail closed）。
- 适配器声明但未加载 → `Unsupported(PI_MCP_ADAPTER_NOT_LOADED)`。
- 版本低于已知最低支持 → `Unsupported(PI_MCP_ADAPTER_VERSION_UNSUPPORTED)`。
- `PI_CODING_AGENT_DIR` 不可映射 → `Unsupported(PI_AGENT_DIR_OVERRIDE_UNMAPPED)`（沿用 design.md §3.1）。

**与 Provider/Prompt/Skills 的差异**：

| 维度       | Provider / Prompt / Skills（官方合同） | MCP（第三方适配器）                                                         |
| ---------- | -------------------------------------- | --------------------------------------------------------------------------- |
| 授权来源   | Pi 官方文档                            | **用户显式授权**接管第三方扩展面                                            |
| 生效前置   | Pi 安装即可                            | **必须**适配器已安装且已加载；否则静默无效                                  |
| 写入冲突   | 无第三方写者                           | **适配器自己也会写这两个文件**（directTools / imports / disabled）          |
| 覆盖风险   | 有 AGENTS.override 等遮蔽（同层）      | **跨文件优先级**：`.mcp.json`/`.pi/mcp.json` 可覆盖 `<agent dir>/mcp.json`  |
| trust      | Skills 项目级 trust-gated              | **不受** trust 门禁（项目 `.pi/mcp.json`）                                  |
| 第三方依赖 | 无                                     | npm 包 `pi-mcp-adapter`，版本演进快（64 个版本，2.33.0 于 2026-09-10）      |
| 证据强度   | 【官】+【源】+【机】                   | 【源】+【机】为主；无 Pi 官方文档（`pi.dev/docs` 无 MCP 页，历史 404 不变） |

---

## 9. 给 EasyToAgents 的落地建议

### 9.1 descriptor（2 个）

```
Mcp × Global:
  path               = <pi_agent_dir>/mcp.json
  format             = Json
  mcp_container      = ["mcpServers"]
  managed_selectors  = ["mcpServers"]          （实际 ownership 仍是条目级 ["mcpServers", <name>]）
  sensitive_selectors= §6.4 Primary + Secondary
  symlink_policy     = Reject
  allowed_root       = <pi_agent_dir>
  trust              = NotRequired
  capability         = 探针决定（§4.3 / §8）

Mcp × Project:
  path               = <project_root>/.pi/mcp.json
  format             = Json
  mcp_container      = ["mcpServers"]
  managed_selectors  = ["mcpServers"]
  sensitive_selectors= 同全局
  symlink_policy     = Reject
  allowed_root       = canonicalize(project_root)
  trust              = NotRequired（如实反映 Pi 行为）
  capability         = 同全局（adapter ready 时 supported）
```

**禁止**：为 `.mcp.json`、`~/.config/mcp/mcp.json`、`~/.agents/mcp.json` 生成任何写 descriptor。

### 9.2 诊断码

| 码                                      | 触发                                                                                                                               |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `PI_MCP_ADAPTER_MISSING`                | `pi` 已装，但 packages 未声明且安装目录不存在                                                                                      |
| `PI_MCP_ADAPTER_NOT_LOADED`             | packages 已声明（或目录存在），但 `pi config` 过滤 `extensions: []` / 项目 packages 未 trust                                       |
| `PI_MCP_ADAPTER_VERSION_UNSUPPORTED`    | `package.json.version` 低于最低已知支持版本                                                                                        |
| `PI_MCP_SHADOWED_BY_PROJECT_SHARED`     | 受管全局条目被 `<root>/.mcp.json` 同名 server 覆盖 → **硬阻断 Apply**                                                              |
| `PI_MCP_SHADOWED_BY_PROJECT_PI`         | 受管全局条目被用户手写 `<root>/.pi/mcp.json` 同名 server 覆盖 → **硬阻断 Apply**（若该文件正是本项目 descriptor 目标且条目非受管） |
| `PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED` | `PI_MCP_CONFIG_MODE=exclusive` 且用户试图 Apply 项目 MCP                                                                           |
| `PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY`   | 检测到受管条目被适配器追加 `directTools` 等字段（漂移）                                                                            |
| `PI_MCP_INLINE_SECRET`                  | 导入/预览时发现明文字面凭据（非 `$ENV`/`!command`）                                                                                |

> 命名对齐现有 `PI_*` 与 `OPENCODE_*_UNSUPPORTED` 风格。`PI_MCP_UNSUPPORTED` 作废（产品决策变更）。

### 9.3 探针

- 复用 `ReleaseToolProbeInput` 的 `pi_agent_dir`（design.md §3.2 已规划）。
- 新增只读文件探针（`app/tool_probe.rs` 或 adapter 内静态读）：
  1. `<pi_agent_dir>/settings.json` → `packages` 是否含适配器（字符串或对象 `source`）；对象形是否 `extensions: []`。
  2. `<project>/.pi/settings.json` → 同上（项目级）。
  3. `<pi_agent_dir>/npm/node_modules/pi-mcp-adapter/package.json` → 存在 + `version`。
  4. `<project>/.pi/npm/node_modules/pi-mcp-adapter/package.json` → 同项目级。
- **不探测**：OS keyring、`pi --offline -p`（额度）、`mcp-oauth/`、trace 文件。
- 失败矩阵：ENOENT/解析失败 → `PI_MCP_ADAPTER_MISSING`；不可写/权限 → `..._NOT_LOADED` 或探针 `Unsupported`。

### 9.4 fixture 清单

1. `agent/mcp.json`：stdio + http 各一，含 `env`/`headers`（含 `${VAR}`、`!cmd`）、`oauth.clientSecret`、`directTools`、未知字段、`settings`、`imports`。
2. `proj/.pi/mcp.json`：同名覆盖全局；`disabled: true` 条目。
3. `proj/.mcp.json`：同名 server（遮蔽检测）。
4. `home/.config/mcp/mcp.json`：global 共享条目。
5. `agent/settings.json`：`packages:["npm:pi-mcp-adapter"]` 正例；`packages:[{source:"npm:pi-mcp-adapter",extensions:[]}]` 禁用反例；缺省反例。
6. `agent/npm/node_modules/pi-mcp-adapter/package.json`：2.33.0 正例 / `0.1.0` 低版本反例 / 缺失反例。
7. `PI_MCP_CONFIG_MODE=exclusive` 变体。

### 9.5 测试矩阵要点

| 层      | 必测                                                                                                                                                             |
| ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Adapter | 2 个 descriptor 的 path/selector/sensitive/symlink_policy/allowed_root/trust；无共享文件 descriptor；adapter-missing 两态                                        |
| Codec   | `command`/`args`/`env` → stdio；`url`/`headers` → http；**不写 `type`**；`enabled=false` ↔ `disabled:true`；`extra` 未知字段保留；`$ENV`/`!command` 零展开零执行 |
| Shadow  | 项目 `.mcp.json` / `.pi/mcp.json` 同名 → 硬阻断 + 诊断码；非同名 → 放行                                                                                          |
| Interop | 受管条目被追加 `directTools` → 漂移检出；非受管条目被适配器搬入 → 不误删（read-modify-write）                                                                    |
| Secrets | Primary selectors 全覆盖；`oauth.clientId`/`args`/`requestHeadersCommand.*` 显式脱敏；DTO/日志/预览无明文                                                        |
| E2E     | Import → 分配 → Preview → Apply → 漂移 → Restore；exclusive 模式项目 Apply 拒绝；adapter-missing 全链路拒绝（零外部写入）                                        |

---

## 10. Unknown / 无法证实清单

| #   | 项                                                                  | 原因                                                             | 处置                                                                                                                      |
| --- | ------------------------------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| U1  | 适配器运行时是否真的注册扩展（静态无法探测）                        | 需跑 Pi 会话，消耗额度                                           | 依赖 `packages` + 安装目录 + `pi config` 过滤三态推断；文案注明                                                           |
| U2  | `pi config` 禁用扩展后的精确 settings 表示                          | 未跑交互式 `pi config`；仅从文档「Package Filtering」推断        | 覆盖字符串/对象+空数组/缺省三态；保守按「未加载」处理                                                                     |
| U3  | 2.34+ 是否保持 schema/路径稳定                                      | 只能看 CHANGELOG 与语义化版本；2.x 已有一次 `lifecycle` BREAKING | 版本探测 + 未知字段保留 + 低于最低支持版本 fail closed                                                                    |
| U4  | `socket` 传输是否应作为受管 transport 暴露                          | EasyToAgents `McpTransport` 只有 stdio/http；socket 语义特殊     | 首版走 `extra`，不新增 transport；UI 不支持新建 socket 类型                                                               |
| U5  | `mcp-oauth/` 与 keyring 令牌的实际内容/格式                         | 涉及凭据，明确不读                                               | 永久禁区                                                                                                                  |
| U6  | Pi package `pi.mcp` / Agent Plugins / Claude 插件的完整 server 集合 | 低优先级兜底层，不在写入面                                       | 只读、不导入、不写；如需导入单列任务                                                                                      |
| U7  | `~/.agents/mcp.json` 的确切跨工具语义                               | 非 Pi 官方，属 agent-plugins 约定                                | 仅作冲突检测                                                                                                              |
| U8  | `pi --offline -p` 端到端「扩展已加载 + 读到 mcp.json」              | 消耗额度；本轮未跑                                               | 列为待办；实现阶段用 `PI_CODING_AGENT_DIR=<tmp>` + 真实 Pi 跑一次 fixture smoke（不调模型，仅启动/`/mcp status`）优先验证 |
| U9  | npm 2.33.0 与 GitHub main 是否同源（`pushed_at` 晚于发布 4 天）     | 未做 diff                                                        | 以 npm 2.33.0 为准；升级时重新核验                                                                                        |

---

## 11. 参考 URL（访问日期 2026-09-14）

- npm：`https://www.npmjs.com/package/pi-mcp-adapter`（latest 2.33.0，2026-09-10）
- registry：`https://registry.npmjs.org/pi-mcp-adapter`
- GitHub：`https://github.com/nicobailon/pi-mcp-adapter`（MIT，1465 stars，未归档）
- Pi 官方（核心，MCP 仍无页面）：`https://pi.dev/docs/latest/usage`、`https://pi.dev/docs/latest/security`、`https://pi.dev/docs/latest/packages`、`https://pi.dev/docs/latest/settings`（本机 0.85.1 副本：`$DOCS/{usage,security,packages,settings}.md`）
- Pi 包页：`https://pi.dev/packages/pi-mcp-adapter`
