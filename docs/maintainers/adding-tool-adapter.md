# 接入新的工具 Adapter

EasyToAgents 以 capability 为先，不要求新工具复制 Claude 或 Codex 的全部能力。只有得到官方、可复现证据的资源，才能进入导入、Preview/Apply 和恢复链路；未知能力必须保持 `unsupported`，不得猜测私有路径或 schema。

## 1. 先建立证据与能力矩阵

开始改代码前，为每个 `artifact × scope × operation` 记录官方来源、验证日期和结论：

| Artifact       | Global  | Project     | Import  | Apply   | 证据与诊断                                       |
| -------------- | ------- | ----------- | ------- | ------- | ------------------------------------------------ |
| Provider       | Unknown | N/A         | Unknown | Unknown | 官方文件路径、schema、优先级                     |
| Prompt / Rules | Unknown | Unsupported | Unknown | Unknown | Prompt 仅支持全局；项目提示词/规则不纳入产品能力 |
| MCP            | Unknown | Unknown     | Unknown | Unknown | 路径、容器、transport、敏感字段                  |
| Skills         | Unknown | Unknown     | Unknown | Unknown | 发现目录、嵌套规则、链接兼容性                   |
| Hooks          | Unknown | Unknown     | Unknown | Unknown | 事件集合、承载方式、matcher 语义                 |
| Agents         | Unknown | Unknown     | Unknown | Unknown | 子代理目录、Markdown/TOML 字段、项目支持范围     |

状态只允许：

- `Supported`：官方合同明确，且本地 fixture 或实机 smoke 可以复现。
- `Unsupported`：官方明确不支持，或产品选择不纳入该能力。
- `Unknown`：证据不足；行为与 `Unsupported` 一样 fail closed，但保留后续调研入口。
- `ToolNotInstalled`：合同已知，但本机安装探针没有得到可信结果。

证据表至少包含官方 URL、页面标题、访问日期、稳定路径/格式、版本或渠道限制。第三方博客、论坛和逆向得到的私有存储不能单独授权写入。

For Prompt/Rules, `Import` and `Apply` are evaluated only for the supported
global scope. A project-scope result is always `Unsupported`, so a new adapter
must not add a project Prompt descriptor, assignment, or native-resource path.
Registered project files such as `CLAUDE.md`, `AGENTS.md`, and Cursor
`.cursor/rules/` remain user-owned; project discovery and synchronization do not
observe, disable, restore, or rewrite them.

Cursor 的当前矩阵是一个非对称示例：全局 Prompt/Rules 与全局/项目 MCP、Skills、Hooks 为 Supported；Provider、API Key、模型和项目 Prompt/Rules 为 Unsupported（Prompt 依据 2026-09-06 官方核验开放全局 `~/.cursor/rules`，见 cursor.com/docs/rules 与 cursor.com/help/customization/rules；应用只接管自有单文件 `rules/easytoagents.mdc`，由 `TargetFormat::CursorMdc` 包装/剥离 `alwaysApply: true` frontmatter）。项目 `.cursor/rules` 文件不被 EasyToAgents 观察或写入。不要因为 `Tool` 已存在就自动开放所有页面或数据库表。

Cursor Prompt/Rules 证据矩阵（2026-09-06）：

| Tool   | Artifact     | Global    | Project     | Import    | Apply     | 官方证据                                                                                                      | 核验日期   |
| ------ | ------------ | --------- | ----------- | --------- | --------- | ------------------------------------------------------------------------------------------------------------- | ---------- |
| Cursor | Prompt/Rules | Supported | Unsupported | Supported | Supported | [User rule files](https://cursor.com/help/customization/rules)；[`.mdc` rules](https://cursor.com/docs/rules) | 2026-09-06 |

合同：Prompt 仅有全局目标 `~/.cursor/rules/easytoagents.mdc`；应用固定写入
`alwaysApply: true` frontmatter，导入/观测剥离该 frontmatter，只把正文纳入全局档案
投影。项目 `.cursor/rules` 及规则目录中的其他文件不属于受管范围。

## 2. 领域合同与 Adapter

1. 在 `src-tauri/src/domain/mod.rs` 为 `Tool` 添加稳定的小写序列化值和往返测试。值写入 SQLite 后不能随显示名称重命名。
2. 在 `src-tauri/src/adapters/<tool>/mod.rs` 实现 `ToolAdapter`。每个 descriptor 必须显式声明：
   - artifact、scope、project root 与目标路径；
   - `TargetFormat`、ownership selector、敏感 selector；
   - capability、policy、trust、prompt override 和 symlink policy。
3. Unsupported descriptor 不提供目标路径，并在任何 scan、path unwrap、Preview 持久化或 Apply 之前由服务入口拒绝。
4. 把 Adapter 注册到实际支持资源的 registry。共享集合位于 `src-tauri/src/adapters/mod.rs`：
   - `PROFILE_TOOLS` 只含 Provider/Prompt 工具；
   - `ASSIGNABLE_MCP_TOOLS`、`ASSIGNABLE_SKILL_TOOLS` 分别列出可分配工具。
5. 完成固定注册点：
   - `domain::Tool` 枚举及其 `Tool::ALL` 稳定顺序；
   - `adapters::adapter_for` 的单例映射与对应 `src-tauri/src/adapters/<tool>/mod.rs`；
   - 数据库中保存工具值的表级 `CHECK` 约束及其前向迁移（如适用）。
     业务代码通过 `tool.adapter()` 或能力集合迭代，不再要求为新增工具逐处检索并复制
     `match Tool`；确需穷举时仍必须显式处理每个变体，不能用 `_` 把新工具误当成 Codex。

## 3. 安装探针与显式环境

探针只负责读取可信安装事实，不负责创建配置：

- macOS Desktop 优先校验生产 Bundle ID，再读取大小受限、类型受限的 `Info.plist` 版本。
- CLI 只能作为官方明确支持的补充证据；CLI 缺失不能否定已验证的桌面应用。
- 候选路径、命令、超时和环境全部通过显式输入注入，测试不得读取真实 HOME 或 PATH。
- 不可信 Bundle、异常版本、链接路径、超时和权限问题返回 `unsupported`/`unavailable`，不得降级为可写。

同步的 `allowed_root` 必须按 tool、artifact、scope 精确推导。全局配置根不能回退到整个 HOME；项目目标只能使用已 canonicalize 的登记项目根。缺失根、链接逃逸或类型冲突必须阻止 Apply。

## 4. 数据库迁移

只能追加前向迁移，不能改历史 SQL。逐表回答“这个 artifact 是否真的会保存新 Tool”：

- 仅放宽 Supported artifact 的 assignment、import preview 和 managed target。
- Provider/Prompt 不受支持时，其表继续拒绝该工具值，形成服务层之外的第二道边界。
- SQLite CHECK 需要 `writable_schema` 时，必须以表名和精确旧锚点限定替换；迁移前验证每个锚点恰好命中一次，未命中立即回滚。
- 测试从前一 schema version 升级，覆盖旧行保留、同连接插入、重开、外键、索引、重复打开和 unsupported canary。
- 回滚代码时不倒迁数据库；放宽的 CHECK 必须对旧数据无破坏。

## 5. 服务与同步链路

逐项检查 MCP、Skills、Profiles、Projects、Overview、Sync 与 Restore：

- 中央 CRUD 与工具分配是不同动作；分配不隐式 Apply。
- Import 是只读发现 → 持久化脱敏预览 → 用户显式选择 → 中央导入，不隐式接管原生目标。全局 Skill 只有在正式目标入口与 Ready 中央副本的名称和完整树哈希精确一致时，才可另行准备 takeover-aware Preview；首次接管即使开启 direct Apply 也必须再次确认。
- MCP renderer/parser 必须保留未知字段，只修改受管名称；`headers`、`env`、`auth` 和扩展凭据不能进入普通 DTO、日志或预览明文。
- Skills 继续使用中央不可变副本和逐名称受管链接。普通 Apply 对普通目录、外部链接、断链和逃逸保持冲突保护；显式首次接管只能通过持久化证据生成专用 mutation。外部链接只替换入口，普通目录必须先创建可恢复目录树快照。工具是否发现符号链接必须由实机 smoke 证明。
- Project service 只创建该工具支持的 MCP、Skills、Hooks、Agents assignment/status；Prompt 是全局资源，项目服务不能产生 Prompt 行。ZCode 项目 Agents 必须保持 Unsupported。
- Overview 可以展示 Unsupported，但不能把它描述为“未接管”。
- Restore 必须从 snapshot 的 tool/artifact/scope 重新推导同一窄 allowed root，并复用现有 journal、snapshot、写后校验与回滚。

## 6. 前端元数据与页面

Rust 的 `Tool` 是 TypeScript 联合类型的唯一来源。修改 Rust 后运行：

```bash
pnpm bindings:generate
pnpm bindings:check
```

不要手改 `src/bindings/commands.ts`。工具显示与能力集中在 `src/lib/tool-metadata.ts`：label、icon、profile route 和 Provider/Prompt/MCP/Skills capability 必须来自同一条 metadata。

检查以下界面：

- MCP/Skills/Agents：全局分配、导入、目标状态、直接应用与错误状态；
- Projects：工具切换、资源标签、项目 assignment、Preview/Apply；Agents 页签只列出 Claude、Codex、Cursor、OpenCode；
- Dashboard：工具计数、Supported/Unsupported 文案和管理入口；
- Profiles/Prompts/AppShell：只为 `PROFILE_TOOLS` 提供 CRUD 与导航；
- Onboarding：只展示真正可导入的首次配置，不为 Unsupported 能力创建空卡片；
- Restore：工具标签、目标路径与诊断一致。

组件边界也要 fail closed。即使路由当前不可达，把不支持工具直接传给组件也不能触发 Provider/Prompt 查询或 mutation。

品牌资源放在 `src/assets/brand/`，并在同目录 README 记录来源、许可或自行绘制说明。

## 7. Fixtures 与测试

每个新工具至少覆盖：

- Tool 序列化、生成 bindings 与 metadata 集合；
- Desktop/CLI 探针的成功、缺失、错误 ID、异常版本、链接路径和超时；
- Adapter 的 global/project descriptor、unsupported capability、ownership、敏感 selector 和 allowed root；Prompt 只测试全局 descriptor，项目作用域必须保持 unsupported；
- 数据库上一版本升级、精确锚点、旧数据、约束 canary、外键/索引与重开；
- MCP stdio/HTTP round-trip、未知字段、敏感值、Missing/InSync/漂移/解析失败/类型冲突/stale/恢复；
- Skills 全局/项目分配、导入来源、普通目录/外部链接/断链/逃逸、恢复和实机发现 smoke；
- 前端分配、导入、状态、项目视图、Unsupported、取消分配和无隐式 Apply；Agents 还要展示导入时被丢弃的工具特有字段；
- `src-tauri/tests/phase8_e2e.rs` 的跨层 Preview → Apply → 漂移 → Restore。

完整质量门：

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test --run
pnpm bindings:check
pnpm rust:check
pnpm check
git diff --check
```

## 8. 发布与回滚

合入前确认 capability 文案、官方证据和代码矩阵完全一致。某项能力在实现阶段失去证据时，回到规划并把该能力关闭；不要临时改成另一种未设计的写入模式。

代码回滚顺序：先从 UI 和共享集合关闭 capability，再移除 service/registry，最后移除 Adapter 分支。已应用的前向数据库迁移保留。任何原生写入失败都使用现有 snapshot/journal 恢复，不增加旁路清理脚本。

## 9. Pi、ZCode 与 OpenCode 示例

Pi 当前只作为待调研候选，不代表已知路径；ZCode 已于 2026-09-05 依据本机核验
与官方 zcode-configuration-guide 完成证据核验并正式接入（迁移 `0013`）：

| 工具     | Provider  | Prompt/Rules（全局） | MCP                       | Skills    | Hooks       | 下一步                                                                                                                                                                    |
| -------- | --------- | -------------------- | ------------------------- | --------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pi       | Unknown   | Unknown              | Unknown                   | Unknown   | Unknown     | 找到官方配置与安装文档，建立版本化 fixture                                                                                                                                |
| ZCode    | Supported | Supported（仅全局）  | Supported                 | Supported | Supported   | 已接入：desktop bundle（`dev.zcode.app`）探针；`~/.zcode/v2/config.json` 的 provider 条目只接管 name/kind/options/enabled；MCP 为 `mcp.servers` 嵌套键                    |
| OpenCode | Supported | Supported（仅全局）  | Supported（local/remote） | Supported | Unsupported | 已接入：PATH `opencode --version` 探针；Provider/MCP 使用官方 JSON/JSONC `provider`/`mcp` 根，Skills 使用全局 config/skills 与项目 `.opencode/skills`；不接管 `auth.json` |

Cursor 的全局 Prompt/Rules 已于 2026-09-06 依据官方证据接入（历史迁移 `0017`
曾扩展过项目作用域，现行 v18 已清理该历史状态）：当前只使用全局
`~/.cursor/rules/easytoagents.mdc`（cursor.com/help/customization/rules 的
"User rule files" 段落）；`TargetFormat::CursorMdc` 在写入时包装固定
`alwaysApply: true`、观测/导入时剥离 frontmatter。项目 `.cursor/rules` 与其中的
其他用户文件不受 EasyToAgents 观察或写入。

## 10. Hooks 能力矩阵（2026-09-07 官方证据核验）

Hooks 已作为第五类 artifact 接入四工具，统一事件模型见 `domain::HookEvent`
（canonical PascalCase；Cursor 原生键为 camelCase，由
`HookEvent::native_key` 映射）。证据来源与合同：

| 工具        | 全局                                     | 项目                           | 事件集              | 备注                                                                                                                                              |
| ----------- | ---------------------------------------- | ------------------------------ | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code | `~/.claude/settings.json` 的 `hooks` 键  | `<root>/.claude/settings.json` | 10                  | 与 Provider 共享文件，选择器隔离；https://code.claude.com/docs/en/hooks                                                                           |
| Codex       | `~/.codex/hooks.json`                    | `<root>/.codex/hooks.json`     | 11                  | 独立文件；官方要求每层只用一种表示，不管理 config.toml 内联 `[hooks]`；信任审查由 CLI `/hooks` 完成；https://developers.openai.com/codex/hooks.md |
| Cursor      | `~/.cursor/hooks.json`                   | `<root>/.cursor/hooks.json`    | 9（camelCase 映射） | 接管 `version` + `hooks` 两个顶层键；matcher 属于条目；https://cursor.com/docs/agent/hooks                                                        |
| ZCode       | `~/.zcode/cli/config.json` 的 `hooks` 键 | `<root>/.zcode/config.json`    | 7                   | 恒写 `hooks.enabled: true`（配置文件 hooks 必须 enabled 才运行）；事件嵌套在 `events` 键；官方 zcode-configuration-guide                          |
| OpenCode    | 不接入（插件返回 hooks object）          | 不接入                         | —                   | 插件回调不是当前 command-only 合同；直接 RPC、分配、导入和写入均 fail closed，诊断码 `OPENCODE_HOOKS_UNSUPPORTED`                                 |

不在统一事件模型内的工具特有事件（Cursor 的 `beforeShellExecution` 等、
Claude 的 `PostToolUseFailure`、ZCode 的 `process` 型、Cursor 的 `prompt` 型）
一律 fail closed：不能分配、不能导入，也不猜测映射。

OpenCode 的 Plugins/hooks object 仍不映射为统一事件；在新的插件运行时合同和回滚边界审核通过前，不为它新增 Hook 事件、猜测目标目录或复制 Cursor 的 Adapter。

## 11. Agents 能力矩阵与合同（2026-09-12 官方证据核验）

Agents 是独立的中央资源类型。实现前必须分别核对“目录 descriptor → 文件级目标 → 投影/解析 → Preview/Apply/Restore”四段边界；不能把目录直接交给文件扫描器，也不能因为工具枚举存在就推断项目级支持。

| 工具          | 全局目录                       | 项目目录                  | 文件格式                    | 全局      | 项目        | 导入         | 关键约束                                                             |
| ------------- | ------------------------------ | ------------------------- | --------------------------- | --------- | ----------- | ------------ | -------------------------------------------------------------------- |
| Claude Code   | `<claude_config_dir>/agents`   | `<root>/.claude/agents`   | Markdown + YAML frontmatter | Supported | Supported   | 直属 `.md`   | `name`、`description` 必填；遵循 customization policy                |
| Codex         | `<codex_home>/agents`          | `<root>/.codex/agents`    | TOML                        | Supported | Supported   | 直属 `.toml` | `name`、`description`、`developer_instructions` 必填；项目沿用 trust |
| Cursor        | `~/.cursor/agents`             | `<root>/.cursor/agents`   | Markdown + YAML frontmatter | Supported | Supported   | 直属 `.md`   | 不读取 `.claude/agents` 或 `.codex/agents` 兼容目录                  |
| ZCode（Beta） | `~/.zcode/agents`              | —                         | Markdown + YAML frontmatter | Supported | Unsupported | 直属 `.md`   | 项目接口返回 `ZCODE_PROJECT_AGENTS_UNSUPPORTED`                      |
| OpenCode      | `<opencode_config_dir>/agents` | `<root>/.opencode/agents` | Markdown + YAML frontmatter | Supported | Supported   | 直属 `.md`   | 投影固定 `mode: subagent`，避免被当作主代理                          |

### 11.1 Scope / Trigger

- Trigger：新增或修改 Agent CRUD、工具分配、原生 Agents 目录、导入、状态聚合、Preview/Apply/Readopt/Restore 或生成 bindings。
- 该合同覆盖后端数据库、服务、适配器、Tauri 命令以及前端 `/agents` 与项目详情页签；任何一层变更都必须重新跑跨层质量门。

### 11.2 Signatures

- 中央记录：`agents(id, name, description, prompt, enabled, row_version, ...)`；名称满足 `^[a-z0-9][a-z0-9-]{0,63}$`。
- 分配命令：`set_global_agent_assignment(tool, agent_id, assigned, row_version)`、`set_project_agent_assignment(project_id, tool, agent_id, assigned, agent_row_version, project_row_version)`。
- 同步命令：`preview_agent_sync(tool, project_id|null, exclude_from_git)`、`apply_agent_preview(preview_id, tool, project_id|null)`、`readopt_agent_target(tool, project_id|null, target_path)`。
- 导入命令：`discover_agent_import(tool)`（只读全局目录）与 `confirm_agent_import(tool, agents[])`（写中央库，不改原文件；首次分配时若交集字段仍一致则自动登记当前文件基线）。
- 一个受管文件对应一行 `managed_targets`（`artifact_kind = 'agent'`、`WholeDocument`）；目录级状态由 `AgentToolTargetStatusDto` 聚合，能力/策略诊断在卡片级 `diagnostic_code`，文件漂移诊断在 `files`。

### 11.3 Contracts

- 中央记录仍只保留 `name`、`description`、`prompt`、`enabled` 四个交集字段；工具特有字段通过 `agent_tool_settings(agent_id, tool, settings_json)` 白名单覆盖层建模。首期（官方证据核对日期 2026-09-12）Claude 保留 `model`、`color`、`tools`，Codex 保留 `model`、`model_reasoning_effort`、`features`；Cursor、OpenCode、ZCode 明确不支持覆盖层。
- Markdown 投影写 YAML frontmatter + 正文；Claude 会合并其覆盖层（`tools` 渲染为逗号分隔单行），OpenCode 额外写 `mode: subagent`。Codex 投影在 `name`、`description`、`developer_instructions` 之外合并顶层覆盖键，并将布尔 `features` 渲染为 `[features]` 子表。
- 分配改变中央意图但不隐式 Apply。Preview 必须持久化目标身份、基线与所有参与的 row versions；Apply 在通用 snapshot/journal 事务内写入或删除文件。
- 停用、取消分配或中央删除在下一次确认 Apply 时删除对应受管文件；删除前快照可经通用 Restore 恢复。目录内非受管同名之外文件保持不变。
- 全局分配在项目内只读继承；项目分配与全局分配互斥。Codex 项目未受信任时返回 `untrusted` 并禁止 Apply；ZCode 项目始终 Unsupported。
- 导入只扫描全局目录直属普通文件；符号链接、子目录和扩展名不匹配项跳过。白名单字段进入 `retained_fields` 并写入覆盖层，其他工具特有字段进入 `dropped_fields`，两类都必须在 UI 中明确提示，不静默丢弃。

### 11.4 Validation & Error Matrix

| 条件                                                                    | 必须结果                                                               |
| ----------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| 名称为空、大写、下划线、冒号、路径分隔符或长度 >64                      | 创建、更新、导入确认均返回 `INVALID_INPUT` / `AGENT_NAME_INVALID`      |
| Markdown frontmatter/TOML 无法解析                                      | 候选 `importable=false`，诊断 `AGENT_FRONTMATTER_INVALID`              |
| 缺少 `description` 或正文（Codex 还缺 `name`/`developer_instructions`） | 候选不可导入，诊断 `AGENT_REQUIRED_FIELD_MISSING`                      |
| ZCode 项目分配或预览                                                    | 服务层和数据库均拒绝，诊断 `ZCODE_PROJECT_AGENTS_UNSUPPORTED`          |
| 受管文件受外部改写                                                      | Preview 为 `ExternalOwnedChange`/Conflict；显式 Readopt 后才可再次写入 |
| 目标路径缺失、类型变化、权限/策略/trust 不安全                          | fail closed，不写入原生目录                                            |
| 覆盖层未知键、类型不符、枚举外取值或 JSON 超过 16 KiB                   | `INVALID_INPUT` / `AGENT_FIELD_INVALID`；不写入数据库                  |
| Cursor、OpenCode、ZCode 请求工具特有设置                                | `INVALID_INPUT` / `AGENT_TOOL_SETTINGS_UNSUPPORTED`                    |

### 11.5 Good / Base / Bad Cases

- Good：同一 Agent 分配到 Claude 与 Codex，各自产生确定性 Markdown/TOML 文件；修改非受管文件不会被删除，停用后删除快照可恢复。
- Base：全局 Agents 目录不存在时，Preview 只报告可创建的文件目标；无分配且无既有目标时不创建空目标或空运行。
- Bad：把 Agents 目录当作单个文件扫描、把 OpenCode `mode` 省略、读取 Cursor 兼容目录、自由透传原始 frontmatter，或把导入候选的 `tools/model` 静默丢弃/写回中央交集字段。

### 11.6 Tests Required

- Adapter：五工具 global/project descriptor、ZCode 无路径 Unsupported、allowed root 与文件扩展名。
- Database：从 v22 升级至 v23、旧 Agent 行保留、`agent_tool_settings` 的 JSON/tool CHECK 金丝雀、级联删除、外键与重开；既有 `managed_targets` 五处 CHECK 金丝雀仍需保留。
- Service：CRUD/CAS、五工具投影 golden、覆盖层未知键/类型/枚举 fail-closed、导入 `retained_fields` / `dropped_fields`、全局继承、Codex untrusted、停用删除与状态聚合。
- E2E：`src-tauri/tests/phase8_e2e.rs` 覆盖 Claude Markdown 与 Codex TOML 的 Preview → Apply → 漂移 → Readopt → 停用删除 → Restore。
- Frontend：`/agents` CRUD/分配/状态展开/导入和项目详情页签；工具能力来自生成 bindings，ZCode 不得出现在项目工具切换。
- 每次命令或 DTO 变化运行 `pnpm bindings:generate && pnpm bindings:check`，并通过 `pnpm check` 与 `git diff --check`。

### 11.7 Wrong vs Correct

#### Wrong

```rust
// 目录 descriptor 被直接交给文件扫描器，目录会被当作普通文件读取，
// 还可能让 allowed_root 跟着文件名逃逸。
let scan = scan_target(tool.adapter(), &agent_directory, &ManagedOwnership::WholeDocument);
```

#### Correct

```rust
// 目录只负责能力与写入边界；每个名称派生一个文件级目标。
let file = agent_directory.for_agent_file(&agent.name, agent_file_extension(tool))?;
let scan = scan_target(tool.adapter(), &file, &ManagedOwnership::WholeDocument);
```

> **Warning**：目录状态不是一个可写目标。状态聚合必须读取每个受管文件的 `SyncStatus`，取最严重状态；目录内未受管文件既不进入 `managed_targets`，也不因中央列表变化被删除。
