# PRD：新增 Pi 工具支持

## 目标与用户价值

把 Pi（`@earendil-works/pi-coding-agent`）接入 EasyToAgents 作为第 6 个配置目标工具，让同时使用 Pi 的开发者能在同一处维护 Pi 支持且可复现的能力，并沿用既有中央编辑 → 显式导入 → 分配 → 预览 → 应用 → 快照恢复链路。

MCP 能力通过用户已启用的第三方适配器 `pi-mcp-adapter` 支持（Pi 核心无内置 MCP），该链路在**适配器未就绪时必须 fail closed**，避免产生「写了但无人读」的必然无效写入。

用户已确认：**全能力逐项核验、按证据开放**（无证据一律 fail closed）、**可选启用且默认关闭**、证据来源为**官方文档 + 本机实测 + 隔离版本验证**、**MCP 在 `pi-mcp-adapter` 存在时纳入支持**、**品牌图标从官方资产获取**。

## 背景与证据

- 当前工具枚举与能力矩阵：`src-tauri/src/domain/mod.rs`（`Tool::ALL` 5 项、`tool_capabilities()`）。接入合同：`docs/maintainers/adding-tool-adapter.md`，其第 9 节把 Pi 标为「待调研候选，Unknown」。
- 本任务规划期已完成证据调研并通过合并：
  - `research/provider-and-install.md`（Provider / 探针 / 配置根 / trust 边界）
  - `research/prompt-and-skills.md`（上下文文件 / Skills / 项目资源边界）
  - `research/mcp-hooks-agents.md`（Hooks / Agents 反向核验 + 范围外资源面）
  - `research/pi-mcp-adapter.md`（**MCP 适配器专项**：路径/优先级/schema/探针/失效场景/隔离 smoke）
  - `research/brand-asset.md`（官方品牌资产来源、许可与 SHA-256）
  - `research/verified-contract.md`（**合并后的唯一权威合同**，含 URL + 访问日期 2026-09-14 + Pi 0.85.1 + 适配器 2.33.0）
- 核心结论：Pi 核心是极简 harness，官方明确「不含内置 MCP、sub-agents」，事件机制只由 TypeScript 扩展提供。MCP 只有适配器就绪时才生效；适配器缺失/未加载时写入 `mcp.json` 属于必然无效的写入。

## 能力矩阵（已核验，2026-09-14 / Pi 0.85.1 + pi-mcp-adapter 2.33.0）

| Artifact                                                     | Global                        | Project                                        | Import      | Apply       | 证据摘要                                                                              |
| ------------------------------------------------------------ | ----------------------------- | ---------------------------------------------- | ----------- | ----------- | ------------------------------------------------------------------------------------- |
| Provider                                                     | Supported                     | Unsupported                                    | Supported   | Supported   | 官方自定义 provider 面 `models.json`                                                  |
| Prompt                                                       | Supported                     | Unsupported                                    | Supported   | Supported   | 全局 `AGENTS.md`；项目 Prompt 属产品政策永久 Unsupported                              |
| Skills                                                       | Supported                     | Supported（trust-gated）                       | Supported   | Supported   | 官方 skills 目录 + symlink 已被 0.85.1 实机加载                                       |
| MCP                                                          | Supported（前置：适配器就绪） | Supported（前置：适配器就绪；不受 trust 门禁） | Supported   | Supported   | 用户显式授权接管适配器的 Pi 自有配置面：`<agent dir>/mcp.json`、`<root>/.pi/mcp.json` |
| Hooks                                                        | Unsupported                   | Unsupported                                    | —           | —           | 事件机制是 TS 扩展 API，不符合 command-only 合同                                      |
| Agents                                                       | Unsupported                   | Unsupported                                    | —           | —           | 无官方目录/schema；`.pi/agents/*.md` 是 Trellis 扩展自建约定                          |
| Prompt Templates / Extensions / Themes / Packages / Sessions | 不纳入                        | 不纳入                                         | —           | —           | 不属于现有六类资源模型（代码执行面/供应链/UI/运行时状态）                             |
| Provider 凭据 `auth.json`                                    | Unsupported                   | Unsupported                                    | Unsupported | Unsupported | 凭据只读禁区                                                                          |

## 范围与需求

| 编号 | 资源/能力              | 首版行为                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ---- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1   | 工具入口               | Pi 出现在设置可启用工具、总览、首次检测、MCP/Skills 页面的工具维度与诊断中；真实呈现已安装/未安装/探针不可信三种状态                                                                                                                                                                                                                                                                                                                                                          |
| R2   | Provider（仅全局）     | 导入/新增/编辑/启用 `<pi_agent_dir>/models.json` 中可无损表示的 `providers.<id>` 条目与模型；条目级局部合并，保留非受管 provider、内置模型合并语义及未知字段；`apiKey`/`headers` 按敏感字段脱敏                                                                                                                                                                                                                                                                               |
| R3   | 提示词（仅全局）       | 导入/分配/同步 `<pi_agent_dir>/AGENTS.md`，每工具一份生效提示词（`is_active_pi`）                                                                                                                                                                                                                                                                                                                                                                                             |
| R4   | Skills（全局/项目）    | 沿用中央不可变副本 + 逐名称受管符号链接；全局 `<pi_agent_dir>/skills`、项目 `<root>/.pi/skills`；首次接管确认与恢复延续现有合同                                                                                                                                                                                                                                                                                                                                               |
| R5   | 失效场景诊断（Prompt） | `AGENTS.override.md` 存在、用户依赖 `CLAUDE.md`/`AGENTS.MD` 回退、`SYSTEM.md`/`APPEND_SYSTEM.md` 存在等场景必须显式诊断；必然无效的写入不得静默执行                                                                                                                                                                                                                                                                                                                           |
| R6   | 失效场景诊断（Skills） | 断链/逃逸/越界链接自检、同名冲突、frontmatter 合同不符、项目未受信任、`settings.json` 额外 skills 来源，全部给出稳定诊断码；pi 静默忽略的情形必须由我们兜底                                                                                                                                                                                                                                                                                                                   |
| R7   | 安全与回归             | `auth.json`、`models-store.json`、`trust.json`、`settings.json`、extensions/packages/themes 绝不读写；凭据脱敏、预览失效检测、窄 allowed_root、失败回滚与恢复延续现有合同；原五工具零退化                                                                                                                                                                                                                                                                                     |
| R8   | MCP（全局/项目）       | 只接管适配器的 Pi 自有配置面：全局 `<pi_agent_dir>/mcp.json`、项目 `<root>/.pi/mcp.json`；stdio（`command`/`args`/`env`/`cwd`）与 HTTP（`url`/`headers`）双向映射，其馀字段（`oauth`/`bearerToken`/`socket`/`lifecycle`/`directTools`/`disabled` 等）作为未受管字段原样保留与展示；适配器未安装/未加载/版本不支持时两个 descriptor 直接 unsupported 且不可写；受管条目被项目 `.mcp.json` 或用户手写 `.pi/mcp.json` 同名覆盖时硬阻断 Apply；`${VAR}`/`!command` 永不展开或执行 |
| R9   | 项目资源边界           | 项目登记只产生 Pi 的 MCP 与 Skills 分配/状态；不产生任何项目 Prompt/Rules/Hooks/Agents 行；项目 `AGENTS.md`、`.pi/SYSTEM.md`、`.pi/APPEND_SYSTEM.md`、`.pi/extensions`、`.agents/skills` 属用户自有，不观察不改写                                                                                                                                                                                                                                                             |
| R10  | 文档与证据一致性       | `docs/maintainers/adding-tool-adapter.md` 第 9 节 Pi 行与 README 工具/能力文案更新为已核验结论（含适配器前置条件、共享文件禁写、MIT 归属）；代码矩阵、文案、证据三者一致                                                                                                                                                                                                                                                                                                      |
| R11  | Hooks / Agents 状态    | 两类能力明确显示为官方不支持；直接 RPC、分配、导入、descriptor、预览与写入全部 fail closed，诊断码 `PI_HOOKS_UNSUPPORTED` / `PI_AGENTS_UNSUPPORTED`                                                                                                                                                                                                                                                                                                                           |
| R12  | 品牌资产               | 使用官方 Press Kit 方形 Badge（`https://pi.dev/favicon.svg`，MIT，SHA-256 `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a`）作为 `src/assets/brand/pi-icon.svg`，字节级不变，不自绘、不 recolor；同目录 README 按既有格式记录来源/许可/抓取日期/哈希                                                                                                                                                                                                        |

## 不在范围内

- 接管 `auth.json`、`models-store.json`、`trust.json`、`settings.json` 或任何 Pi 自管/凭据文件。
- 接管 MCP 的**共享/跨工具**文件（`~/.config/mcp/mcp.json`、`~/.agents/mcp.json`、`~/.agents/mcp/mcp.json`、`<root>/.mcp.json`）与适配器自有旁路文件（`mcp-oauth/`、OS 凭据库、`mcp-cache.json`、`mcp-npx-cache.json`、`mcp-onboarding.json`、`agent-plugin-data/`、`.pi/mcp-traces/`）；这些只做只读冲突检测。
- 代跑 `pi install` / `pi config` / `pi-mcp-adapter init`，以及安装或升级适配器本身。
- Pi 官方登录（`/login` 是交互式 TUI 命令，无非交互合同）；MCP OAuth 登录流程（`auth-start`/`auth-complete`）同样不代跑，`oauth` 字段只作未受管字段原样保留。
- Extensions、Pi Packages、Themes、Sessions/JSONL、`pi config` 开关：属于代码执行面、供应链或运行时状态，不新增 ArtifactKind，不代跑 `pi install`。
- 非官方位置 `<pi_agent_dir>/../skills`（即 `~/.pi/skills`）、跨工具共享目录 `~/.agents/skills` 与 `<project>/.agents/skills`（只作只读冲突检测）。
- 项目级 Provider 与 Prompt；socket 传输不新增 transport（走未受管字段）。

## 验收标准

- AC1（R1/R11）：`Tool::Pi` 在序列化、生成 bindings 与 `tool_capabilities()` 中一致；Hooks/Agents 两行在领域层、服务层与数据库层**同时**被拒绝，任何绕过路径都无法落库或写入磁盘。
- AC2（R2）：`models.json` 的 import → 分配/编辑 → Preview → Apply → 漂移 → Restore 全链路通过；只给 `baseUrl` 的条目在写入后仍保留内置模型；非受管 provider 与未知字段字节级保留；DTO/日志/预览无明文 `apiKey`，`$ENV`/`!command` 不被展开或执行。
- AC3（R3/R5）：全局 Prompt 导入/分配/应用/恢复通过；`AGENTS.override.md` 存在时写入被阻断且给出 `PI_PROMPT_OVERRIDE_DETECTED`；回退文件场景给出遮蔽诊断；项目 Prompt 任何入口均返回 `PI_PROJECT_PROMPT_UNSUPPORTED`。
- AC4（R4/R6）：全局与项目受管 Skill 链接在隔离 `PI_CODING_AGENT_DIR` 下被 Pi 0.85.1 实际发现（fixture smoke 固化）；断链、逃逸、同名冲突、缺 `description`、frontmatter 名称不一致、项目未信任各有稳定诊断，原始来源不被改写。
- AC5（R7）：从 v24 前向迁移到 v25 后旧数据、索引、外键、重开正常；`auth.json`/`models-store.json`/`trust.json` 不出现任何读或写路径；`PI_CODING_AGENT_DIR` 未映射时 fail closed。
- AC6（R7/R10）：默认 `enabled_tools` 仍为 `[claude, codex]`，未启用 Pi 时行为零变化；README、`adding-tool-adapter.md` 与代码矩阵一致。
- AC7（R10）：`pnpm check`（format/lint/typecheck/test/bindings:check/rust:check）与 `git diff --check` 全绿。
- AC8（R8）：MCP 在适配器就绪时完成 Import → 分配 → Preview → Apply → 漂移 → Restore 全链路；stdio/HTTP 双向映射正确，未知字段（`oauth`/`directTools`/`lifecycle`/`socket` 等）与非受管条目字节级保留，`${VAR}`/`!command` 不被展开或执行，DTO/日志/预览无明文凭据；适配器缺失/未加载/版本不支持时两个 descriptor unsupported 且零外部写入；受管条目被项目 `.mcp.json` / 用户手写 `.pi/mcp.json` 同名覆盖时 Apply 被硬阻断并给出对应诊断码；`PI_MCP_CONFIG_MODE=exclusive` 时项目 Apply 被拒绝；未生成指向共享 MCP 文件的任何写入路径。
- AC9（R12）：`src/assets/brand/pi-icon.svg` 的 SHA-256 与官方 `https://pi.dev/favicon.svg` 一致且未被改写；`src/assets/brand/README.md` 条目含来源、Press Kit URL、抓取日期 2026-09-14、MIT 许可与哈希；浅色/深色主题下图标均可辨识。

## 评审决策记录（2026-09-14，用户确认）

| 决策                                | 结论                                                                                                                                                                               | 影响                                                                                                                                       |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| 规划产物评审                        | **先评审，暂不实施**；任务保持 `planning`，未 `task.py start`                                                                                                                      | 实施需用户再次确认后启动                                                                                                                   |
| 首版范围（初版）                    | 按证据全做：Provider（全局）+ Prompt（全局）+ Skills（全局/项目）                                                                                                                  | 已被下一条决策部分取代                                                                                                                     |
| **首版范围（变更）**                | **MCP 改为 Supported**：既然 `pi-mcp-adapter` 存在且用户已启用，就接管其 Pi 自有配置面（全局 + 项目）；**前置条件**为适配器就绪，否则 fail closed；Hooks/Agents 仍永久 fail closed | 即 `design.md` §1 的六个 descriptor                                                                                                        |
| **品牌图标（变更）**                | **从官方资产获取**：`https://pi.dev/favicon.svg`（Press Kit「Badge」方形 mark，MIT），字节级复制为 `src/assets/brand/pi-icon.svg`，不自绘                                          | 见 `design.md` §9 与 `research/brand-asset.md` §7                                                                                          |
| `AGENTS.override.md` 存在           | **硬阻断 Apply**                                                                                                                                                                   | `PI_PROMPT_OVERRIDE_DETECTED` 为错误而非 warning，见 `design.md` §7/§12                                                                    |
| 项目未受 Pi 信任（Skills）          | **禁止 Apply**（对齐 Codex `untrusted` 模式）                                                                                                                                      | `PI_PROJECT_SKILLS_UNTRUSTED` / `PI_PROJECT_SKILLS_TRUST_UNKNOWN`，见 `design.md` §7                                                       |
| 适配器未就绪（MCP）                 | **fail closed，禁止 Apply 且不暴露目标路径**（由实现方案推荐并由用户 MCP 支持要求隐含确认）                                                                                        | `PI_MCP_ADAPTER_MISSING` / `_NOT_LOADED` / `_VERSION_UNSUPPORTED`；写入无效的失败模式与 `AGENTS.override.md` 硬阻断同源，见 `design.md` §8 |
| 项目 `.pi/mcp.json` 不受 trust 门禁 | **如实建模 `trust = NotRequired` + 显式安全提示**，不作为硬阻断                                                                                                                    | 见 `research/pi-mcp-adapter.md` §3；`.pi/mcp.json` 不在 Pi 官方 trust 清单且适配器不读 trust                                               |

## 风险与验证边界

- 本任务证据基于 **Pi 0.85.1** 与 **pi-mcp-adapter 2.33.0**；官方文档 `latest` 与本机 0.85.1 已发现键位差异（`Per-model compaction overrides`）。实现按「官方 URL 授权 + 本机版本复核」双标注，并在解析层保持宽松（未知字段原样保留）。
- 官方未文档化的项（同目录候选文件完整表、skills precedence 数值、symlink 跟随）只能由源码 + smoke 证明，已标 Unknown 并转为 fixture 回归项，不得写成 Supported 承诺。
- **MCP 依赖第三方适配器**：该面由 `pi-mcp-adapter` 定义（非 Pi 官方文档），版本演进较快（2.x 已出现 `lifecycle` BREAKING）。缓解措施为：只依赖路径 + `mcpServers` 条目级契约、未知字段全量保留、适配器版本探测、低于最低支持版本 fail closed、适配器启用状态三态探测。
- **MCP 跨文件优先级陷阱**：`<pi_agent_dir>/mcp.json` 会被项目 `.mcp.json` / `.pi/mcp.json` 的同名 server 覆盖（已实测）。EasyToAgents 必须在 Apply 前做遮蔽检测并硬阻断，否则写入静默无效。
- **适配器与 EasyToAgents 双方都会写同一文件**：适配器会为共享源条目持久化 `directTools` 等字段。受管条目因此发生哈希漂移属正确行为（需重新接管），非受管条目必须靠条目级 ownership 保留；禁止整文件覆写。
- 若实现阶段证据否定某项能力（例如 Pi 新版移除 symlink 发现、适配器 2.34+ 改变 schema），必须回到规划关闭该能力，不得临时改为另一种未设计的写入模式；升级回归通过「架构签名 + 行为定义」而非与参考实现逐行等价来判断。
- 运行时 CLI 开关（`--no-skills`、`--no-context-files`）与适配器运行时是否真正注册（需跑 Pi 会话、消耗额度）静态不可探测，已标 Unknown，UI 文案必须明示「文件状态与运行时开关/会话状态无关」。
