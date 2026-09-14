# Design：新增 Pi 工具支持

## 1. 边界与不变式

**唯一写入面**（6 个 descriptor，其余一律不生成）：

| Artifact | Scope   | 路径                          | Format             | ownership                                          | symlink policy        | capability 前置                       |
| -------- | ------- | ----------------------------- | ------------------ | -------------------------------------------------- | --------------------- | ------------------------------------- |
| Provider | Global  | `<pi_agent_dir>/models.json`  | `Json`             | `["providers"]` 条目级                             | `Reject`              | Pi 已安装                             |
| Prompt   | Global  | `<pi_agent_dir>/AGENTS.md`    | `Markdown`         | `["$document"]`                                    | `Reject`              | Pi 已安装                             |
| Skill    | Global  | `<pi_agent_dir>/skills`       | `SymlinkDirectory` | `["$children"]`                                    | `ManagedChildrenOnly` | Pi 已安装                             |
| Skill    | Project | `<project_root>/.pi/skills`   | `SymlinkDirectory` | `["$children"]`                                    | `ManagedChildrenOnly` | Pi 已安装 + 项目受信任                |
| Mcp      | Global  | `<pi_agent_dir>/mcp.json`     | `Json`             | `["mcpServers"]` + 逐名称 `["mcpServers", <name>]` | `Reject`              | 适配器已安装且已加载                  |
| Mcp      | Project | `<project_root>/.pi/mcp.json` | `Json`             | 同上                                               | `Reject`              | 适配器已加载（`trust = NotRequired`） |

`<pi_agent_dir>` = 显式注入的 `PI_CODING_AGENT_DIR`，否则 `<home>/.pi/agent`。

**不变式**：

1. Hooks / Agents **永远不生成 descriptor、不生成 assignment、不落 `managed_targets`**。两者无官方合同（事件是 TS 扩展 API，子代理目录是 Trellis 扩展私有约定），因此与「本机是否装了插件」无关。
2. MCP **只在该第三方适配器就绪时支持**。判定依据是路径 + `mcpServers` 条目级契约（适配器 2.33.0）；适配器缺失、未加载或低于最低支持版本时，两个 MCP descriptor 必须 `TargetCapability::unsupported(...)` 且 `path = None`——写无人读的 `mcp.json` 是必然无效的写入，与 `AGENTS.override.md` 同属硬阻断情形。
3. `auth.json`、`models-store.json`、`trust.json`、`settings.json`、`SYSTEM.md`、`APPEND_SYSTEM.md`、`extensions/`、`npm/`、`git/`、`themes/`、`bin/`、`~/.agents/skills`、`<root>/.agents/skills`、`~/.pi/skills`，以及 MCP 的共享/跨工具文件（`~/.config/mcp/mcp.json`、`~/.agents/mcp.json`、`~/.agents/mcp/mcp.json`、`<root>/.mcp.json`）与适配器自有旁路文件（`mcp-oauth/`、OS 凭据库、`mcp-cache.json`、`mcp-npx-cache.json`、`mcp-onboarding.json`、`agent-plugin-data/`、`.pi/mcp-traces/`）**没有任何读写代码路径**（共享文件仅做只读同级检测）。
4. `allowed_root` 只能是 `<pi_agent_dir>`（全局）或 canonicalize 后的项目根（项目），不得回退到 `~/.pi` 或 `HOME`。
5. 拒绝必须至少在一个真实边界上成立：Hooks/Agents 与项目 Prompt 在**服务层与数据库层同时**拒绝；MCP 就绪性在 **descriptor 层**拒绝（无目标路径），共享文件同名遮蔽在 **Apply 层**硬阻断。四层都不允许「先探测再写」。

## 2. 领域层合同（`src-tauri/src/domain/mod.rs`）

```rust
pub enum Tool { Claude, Codex, Cursor, Zcode, Opencode, Pi }   // "pi" 稳定序列化值
pub const ALL: [Self; 6] = [...];
```

`tool_capabilities()` —— 能力矩阵唯一跨层来源，**不得使用 `_` catch-all**：

| tool | provider | prompt_global | mcp  | skills | hooks     | agents    | project_agents | agent_tool_settings |
| ---- | -------- | ------------- | ---- | ------ | --------- | --------- | -------------- | ------------------- |
| Pi   | true     | true          | true | true   | **false** | **false** | **false**      | **false**           |

`HookEvent::supported_for_tool` 增加显式 `Tool::Pi => false` 分支（Pi 无声明式 hook 文件，任何事件都 fail closed）。

## 3. 环境与探针

### 3.1 ExplicitEnvironment（`src-tauri/src/adapters/discovery.rs`）

- 新增字段 `pi_agent_dir: PathBuf`，默认 `home/.pi/agent`；构造器 `with_pi_agent_dir(path)`；访问器 `pi_agent_dir()`。
- 复用既有 `normalize_config_root` 语义，但要按 Pi 自己的规则先展开（证据：`pi-mcp-adapter/agent-dir.ts:11-25`，`getAgentDir()`）：`~` → `home`；`~/x` → `home/x`；绝对路径 → 原样；**相对路径 → fail closed**（适配器用 `resolve(configured)` 即相对自己的进程 cwd 解析，EasyToAgents 无法可靠重现，只能标 `PI_AGENT_DIR_OVERRIDE_UNMAPPED`）。
- 环境变量名是动态的：`process.env[`${piConfig.name.toUpperCase()}_CODING_AGENT_DIR`]`（vanilla pi 即 `PI_CODING_AGENT_DIR`）。本任务只支持 vanilla Pi；若检测到 rebrand 的 `piConfig.name`（配置目录名同理来自 `piConfig.configDir`，默认 `.pi`）与内置假定不一致，按 `PI_AGENT_DIR_OVERRIDE_UNMAPPED` fail closed。
- 边界读取只在 `src-tauri/src/lib.rs` 的 setup 处进行（与 `OPENCODE_CONFIG_DIR` 同处），adapter 内部**不读 `std::env`**。

```rust
// lib.rs 边界（示意）
if let Some(raw) = environment_path("PI_CODING_AGENT_DIR") {
    match probe_input.try_with_pi_agent_dir(Some(raw)) {
        Ok(next) => probe_input = next,
        Err(_) => /* 保留默认值 + PI_AGENT_DIR_OVERRIDE_UNMAPPED 诊断 */,
    }
}
```

不可映射的覆盖**不得静默退回默认值**：退回默认值意味着写到一个 Pi 不会读的位置。诊断 `PI_AGENT_DIR_OVERRIDE_UNMAPPED`，并使 Pi 的全部 descriptor 返回 `TargetCapability::unsupported(...)`。

### 3.2 安装探针（`src-tauri/src/app/tool_probe.rs`）

- `ToolBinary` 新增 `Pi`，`executable_name() == "pi"`；`parse_version` 对 Pi 采用与 Opencode 相同的宽容解析：`strip_prefix("pi ").unwrap_or(output)` + `valid_semantic_version`。
- `ReleaseToolProbeInput` 增加 `pi_agent_dir: Option<PathBuf>` 与 `with_pi_agent_dir`；探针并行任务由五路扩到六路，`ToolAvailability` 数组由 `[_; 5]` 扩到 `[_; 6]`（`tool_index` 增加 `Tool::Pi => 5`）。
- 失败矩阵：`ENOENT`/非零退出 → `ToolNotInstalled`；超时/权限/异常输出 → `Unsupported` + `PI_INSTALLATION_PROBE_UNSUPPORTED`。
- 不探测 macOS bundle（Pi 无官方 `.app`）。官方登录（`/login`）是交互式 TUI，不纳入 `OfficialLoginRegistry`，与 Cursor/ZCode/OpenCode 同桶。

### 3.3 MCP 适配器探针（只读文件探测，不执行 `pi`）

MCP 的 descriptor 能力由一个独立的只读探测决定，输入只有 `pi_agent_dir`、项目根与显式环境：

| 探测点                                                                                             | 结论                                                         |
| -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `<pi_agent_dir>/settings.json` 的 `packages[]` 含 `npm:pi-mcp-adapter`（字符串或 `{source}` 对象） | 「已声明启用」                                               |
| 对象形 `packages` 条目的 `extensions: []`                                                          | 「被 `pi config` 过滤，未加载」→ `PI_MCP_ADAPTER_NOT_LOADED` |
| `<pi_agent_dir>/npm/node_modules/pi-mcp-adapter/package.json` 存在 + `version`                     | 「已下载」+ 版本判据                                         |
| `<project>/.pi/settings.json` 的 `packages[]` + `<project>/.pi/npm/...`                            | 项目级声明/安装（受 project trust 约束）                     |
| 以上全部缺失                                                                                       | `PI_MCP_ADAPTER_MISSING`                                     |
| `version` 低于最低支持版本                                                                         | `PI_MCP_ADAPTER_VERSION_UNSUPPORTED`                         |

不得作为判据：`<pi_agent_dir>/git/`（仅 git 源安装）、`pi list` 输出（不反映 `pi config` 禁用过滤）、运行时会话是否真的注册（需跑 Pi 会话并消耗额度）。最低支持版本以「本次核验的 2.33.0」为基线向下给出显式常量，并随升级回归调整。

## 4. Adapter 契约（新增 `src-tauri/src/adapters/pi/mod.rs`）

`PiAdapter` 实现 `ToolAdapter`：

- `tool() == Tool::Pi`；`provider_codec() == Some(self)`。
- `discover()` 按 `(PI_CODING_AGENT_DIR 可映射?, tool_availability(Tool::Pi))` 决定 capability：
  - 未映射覆盖 → `unsupported("PI_AGENT_DIR_OVERRIDE_UNMAPPED")`
  - `Installed` → `supported()`
  - `Unavailable` → `tool_not_installed()`
  - `Unsupported` → `unsupported("PI_INSTALLATION_PROBE_UNSUPPORTED")`
- 只返回 §1 的 6 个 descriptor；**不生成** `ArtifactKind::Hook | Agent` 的 descriptor（无合同，故连 descriptor 都不存在）。
- MCP descriptor（2 个）的 capability 由 §3.3 的适配器探针决定：适配器未就绪 → `unsupported(PI_MCP_ADAPTER_MISSING | _NOT_LOADED | _VERSION_UNSUPPORTED)` 且 `path = None`；`PI_CODING_AGENT_DIR` 不可映射同样使 6 个 descriptor 全部 unsupported。
- `native_mcp_container(Tool::Pi) == ["mcpServers"]`；ownership 是 `["mcpServers"]` + 逐名称 `["mcpServers", <name>]`（与既有 MCP 合同一致），因此 `settings`/`imports`/`claudePlugins` 与非受管 `mcpServers.*` 永不被写。
- 容器别名：适配器读取时接受 `raw.mcpServers ?? raw["mcp-servers"]`（`config.ts:732`）。**导入/观测必须同时识别两者**（否则漏读用户条目），但**写入只写 canonical `mcpServers`**。识别到别名时给提示性诊断。
- MCP 遮蔽检测（**Apply 前硬阻断**）：受管条目名在 `<root>/.mcp.json` 或用户手写（非本任务受管）的 `<root>/.pi/mcp.json` 中存在同名 server 时，`PI_MCP_SHADOWED_BY_PROJECT_SHARED` / `PI_MCP_SHADOWED_BY_PROJECT_PI` 阻断；`PI_MCP_CONFIG_MODE=exclusive` 时项目 descriptor 直接 unsupported（`PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED`）。
- MCP 双向映射：stdio `command`/`args`/`env`/`cwd`，HTTP `url`/`headers`；**不写 `type`**（适配器靠字段存在性推断）；停用映射为 `disabled: true`；`socket`/`oauth`/`bearerToken*`/`lifecycle`/`directTools`/`requestHeadersCommand`/`caFile` 等一律进未受管字段原样保留（不得求值、不得丢弃）。
- MCP 敏感 selector：`mcpServers/*/env`、`mcpServers/*/headers`、`mcpServers/*/bearerToken`、`mcpServers/*/bearerTokenEnv`、`mcpServers/*/oauth`、`mcpServers/*/requestHeadersCommand/env`、`mcpServers/*/requestHeadersCommand/args`；次级 `mcpServers/*/args`、`mcpServers/*/url`、`mcpServers/*/requestHeadersCommand/command`、`mcpServers/*/caFile`、`mcpServers/*/socket`、`mcpServers/*/cwd`。
- 适配器与 EasyToAgents **双方都会写同一文件**：适配器会为共享源条目持久化 `directTools` 等字段，两边序列化均为「2 空格缩进 + 尾换行」。因此 Apply 必须是 read-modify-write 的条目级替换；受管条目被适配器追加字段后哈希漂移属正确行为（`PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY`），非受管条目必须保留。
- Prompt descriptor 携带 `prompt_override(discover_pi_prompt_override(agent_dir))`：
  - `<pi_agent_dir>/AGENTS.override.md` 存在 → `PromptOverrideState::Present`；
  - `<pi_agent_dir>/AGENTS.md` 不存在但用户存在 `CLAUDE.md`/`CLAUDE.MD`/`AGENTS.MD` → 额外诊断 `PI_PROMPT_FALLBACK_PRESENT`（导入不完整 + 我们写入会反向遮蔽）。
- Provider codec：`TargetFormat::Json` + selector `["providers"]`，按 provider id 做**条目级局部合并**：
  - 只写受管条目参与投影的字段，保留同条目下未知字段；
  - 只给 `baseUrl`/`headers` 且无 `models` 的条目必须保留内置模型合并语义（不得写成 `models: []`）；
  - `apiKey`/`headers` 值原样保留，**不展开 `$ENV`、不执行 `!command`**；`apiKey` 内联明文 → 诊断 `PI_PROVIDER_INLINE_API_KEY` + 脱敏。

敏感 selector：`providers/*/apiKey`、`providers/*/headers`、`providers/*/models/*/headers`、`providers/*/modelOverrides/*/headers`。

## 5. 注册点（全部集中，新增工具不再逐处复制判断）

| 位置                                                                       | 变更                                                                 |
| -------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `domain::Tool` / `Tool::ALL`                                               | 新增 `Pi`，长度 5 → 6                                                |
| `domain::tool_capabilities()`                                              | 见 §2 矩阵                                                           |
| `domain::HookEvent::supported_for_tool`                                    | `Tool::Pi => false`                                                  |
| `adapters::adapter_for`                                                    | `Tool::Pi => &PI_ADAPTER`，新增 `static PI_ADAPTER`                  |
| `adapters::global_root_for`                                                | `(Tool::Pi, _) => environment.pi_agent_dir().to_path_buf()`          |
| `adapters::path_text` / `tool_index` / `ToolAvailability`                  | 6 元素显式映射                                                       |
| `PROFILE_TOOLS`                                                            | `Tool::ALL`（Pi 支持 Provider 与 Prompt，纳入）                      |
| `ASSIGNABLE_MCP_TOOLS`                                                     | `Tool::ALL`（6 元素，Pi 纳入；**不再缩小集合**）                     |
| `ASSIGNABLE_SKILL_TOOLS`                                                   | `Tool::ALL`（6 元素，Pi 纳入）                                       |
| `ASSIGNABLE_HOOK_TOOLS` / `ASSIGNABLE_AGENT_TOOLS` / `PROJECT_AGENT_TOOLS` | **不变**（Pi 不进入）                                                |
| `agent_file_extension`                                                     | 显式 `Tool::Pi => "md"` 分支 + 注释说明服务层先拒绝；不得靠 `_` 兜底 |
| `native_mcp_container`                                                     | `(Tool::Pi, _) => &["mcpServers"]`                                   |

## 6. 数据库迁移 `0025_pi_tool_support.sql`

沿用 `0013`/`0019` 的 `writable_schema` 原地修订 + `instr(...) > 0` 精确旧锚点 + 金丝雀测试模式。逐表回答「Pi 是否真的会保存该 artifact」：

| 表                                                                                                               | 放宽内容                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `mcp_global_assignments`、`mcp_project_assignments`、`mcp_import_previews`                                       | CHECK 枚举增加 `'pi'`（MCP 由适配器支持）；`mcp_import_previews` 的 source_kind 复用现有来源语义                          |
| `skill_global_assignments`、`skill_project_assignments`、`skill_import_previews`                                 | CHECK 枚举增加 `'pi'`；`skill_import_previews.source_kind` 不新增来源（Pi 复用现有全局/项目来源语义，如需新增则单列）     |
| `managed_targets`                                                                                                | 枚举增加 `'pi'`，并**显式限定 artifact**：`AND (tool != 'pi' OR artifact_kind IN ('provider', 'prompt', 'mcp', 'skill'))` |
| `provider_profiles`、`profile_import_previews`                                                                   | 枚举增加 `'pi'`（Provider 与 Prompt 均支持，故无额外 artifact 限制）                                                      |
| `prompt_profiles`                                                                                                | 新增 `is_active_pi INTEGER NOT NULL DEFAULT 0 CHECK(is_active_pi IN (0, 1))` + 每工具至多一份生效的部分唯一索引           |
| `hook_global_assignments`、`hook_project_assignments`、`hook_assignment_events`、`agents`、`agent_tool_settings` | **不放宽**（Pi 无 Hooks / Agents）                                                                                        |

迁移测试必须覆盖：从前一版本升级、旧行保留、同连接插入新值、重开、外键与索引完整、重复打开，以及 canary（`pi` + `hook|agent` 在 `managed_targets` 被拒；`pi` + `provider|prompt|mcp|skill` 可插入；任何 `pi` hook/agent 行在各自表被拒）。

## 7. 服务与同步链路

- `profiles/`：Provider 发现/校验/渲染新增 Pi 分支（`models.json` 条目级 codec，保留未知字段与内置合并语义）；Prompt 支持 `is_active_pi`（`profiles/prompt.rs` 的默认值表与 preview 映射、`db/profiles.rs:632` 附近的列映射、`overview` 的每工具生效提示词计数）。
- `skills/`：`ASSIGNABLE_SKILL_TOOLS` 自动纳入 Pi；导入来源文案与 `skill_import_previews.source_kind` 复用现有全局/项目语义；实机 smoke 证明符号链接被发现。
- `mcp/`：新增 Pi 分支——导入（`native_mcp_item`/`parse_native_item` 的 `mcpServers` 容器）与会话投影（stdio/HTTP，不写 `type`，停用→`disabled`），并接线适配器就绪门禁、遮蔽检测与 `exclusive` 模式诊断；未知字段进 `extra` 原样保留，零求值。
- `hooks/`、`agents/`：新增 `ensure_*_supported` 式守卫，对 `Tool::Pi` 直接返回 `AppError::invalid_input(...)`（诊断码见 §8），零外部写入；`build_native_events`/projection 分支的穷举 match 必须显式列出 Pi 并拒绝，不得用 `_`。
- `projects/`：只为 Pi 生成 Skills 的 assignment/status；项目 trust 未确认 → `TargetTrustState` 非 Trusted 时禁止 Apply（对齐 Codex `untrusted` 模式），诊断 `PI_PROJECT_SKILLS_UNTRUSTED` / `PI_PROJECT_SKILLS_TRUST_UNKNOWN`。
- `overview/`：工具计数与 Supported/Unsupported 文案；`global_allowed_root` 复用 `global_root_for`。
- `sync/`：新增 Pi 专用警告/错误常量（`PI_PROMPT_OVERRIDE_DETECTED` 等），**不复用** `WARNING_CODEX_PROMPT_OVERRIDE`；`AGENTS.override.md` 场景使该 Prompt 目标**不可 Apply**（硬阻断错误，而非 warning），因为写入必然不生效。
- `settings`：`enabled_tools` 默认保持 `[claude, codex]`；Pi 只在用户显式启用后参与总览与服务迭代。

## 8. fail-closed 合同

| 入口                                        | 条件                                               | 结果                                                                                      |
| ------------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| 任意 Hook RPC / 分配 / 导入 / 写入          | `tool == pi`                                       | `PI_HOOKS_UNSUPPORTED`，零外部写入                                                        |
| 任意 Agents RPC / 分配 / 导入 / 预览 / 写入 | `tool == pi`                                       | `PI_AGENTS_UNSUPPORTED`，零外部写入                                                       |
| 项目 Prompt 分配 / 导入 / 原生资源          | `tool == pi && scope == project`                   | `PI_PROJECT_PROMPT_UNSUPPORTED`                                                           |
| MCP descriptor（全局/项目）                 | 适配器未安装 / 未加载 / 版本过低                   | `unsupported(PI_MCP_ADAPTER_MISSING / _NOT_LOADED / _VERSION_UNSUPPORTED)`，`path = None` |
| MCP descriptor（项目）                      | `PI_MCP_CONFIG_MODE=exclusive`                     | `unsupported(PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED)`                                      |
| MCP Apply                                   | 受管条目被 `<root>/.mcp.json` 同名 server 覆盖     | 硬阻断 + `PI_MCP_SHADOWED_BY_PROJECT_SHARED`                                              |
| MCP Apply                                   | 受管条目被用户手写 `.pi/mcp.json` 同名 server 覆盖 | 硬阻断 + `PI_MCP_SHADOWED_BY_PROJECT_PI`                                                  |
| MCP 导入/预览                               | 条目含明文字面凭据                                 | `PI_MCP_INLINE_SECRET` + 脱敏                                                             |
| MCP 漂移检测                                | 受管条目被适配器追加 `directTools` 等字段          | `PI_MCP_ADAPTER_WRITES_MANAGED_ENTRY`（走正常重新接管）                                   |
| DB `managed_targets`                        | `tool == pi && artifact_kind IN (hook, agent)`     | CHECK 拒绝                                                                                |
| DB hook/agent 各表                          | `tool == pi`                                       | CHECK 拒绝                                                                                |
| Apply                                       | `AGENTS.override.md` 存在                          | 硬阻断 + `PI_PROMPT_OVERRIDE_DETECTED`                                                    |
| Apply                                       | 项目未受信任（Skills）                             | 禁止 + `PI_PROJECT_SKILLS_UNTRUSTED`                                                      |
| Apply                                       | 受管链接断链/逃逸                                  | `PI_SKILL_SYMLINK_BROKEN` / `PI_SKILL_SYMLINK_ESCAPE`                                     |
| 任意 descriptor                             | `PI_CODING_AGENT_DIR` 不可映射                     | `TargetCapability::unsupported("PI_AGENT_DIR_OVERRIDE_UNMAPPED")`                         |

## 9. 前端

- `pnpm bindings:generate` 后 `src/lib/tool-metadata.ts` 增加 `pi`（label `Pi`、图标、`profileRoute: "/pi"`、无 `agentToolSettings`），能力值全部来自生成 bindings，不手写。
- `src/features/settings/settings-dialog.tsx` 的 `ENABLED_TOOL_ORDER` 增加 `pi`（默认不勾选）。
- 页面按能力自动收敛：Hooks/Agents 页面不出现 Pi 分配入口（数据源是 `ASSIGNABLE_*` 与 bindings）；MCP 页面按 descriptor 能力呈现适配器缺失/未加载诊断与安装指引（`pi install npm:pi-mcp-adapter`）；Projects 工具切换只显示支持项目资源的工具；Providers/Prompts 导航只为 `PROFILE_TOOLS` 提供 CRUD。
- 品牌资源 `src/assets/brand/pi-icon.svg`：**使用官方资产**，字节级复制 `https://pi.dev/favicon.svg`（官方 Press Kit「Badge」方形 mark，MIT，SHA-256 `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a`），**不自绘、不 recolor、不优化**；同目录 README 按既有格式补条目（来源描述 + Press Kit URL + 资产 URL + 抓取日期 2026-09-14 + MIT 许可 + SHA-256）。若哈希不符（被工具改写）必须重新从官方 URL 下载。
- 文案：能力矩阵文案必须与 `research/verified-contract.md` 完全一致，明示「文件状态与运行时 CLI 开关无关」。

## 10. 兼容性与回滚

- **前向迁移只增不删**：`0025` 只放宽 Supported artifact，Pi 的 Hooks/Agents 在表级继续被拒。代码回滚时不倒迁数据库，被放宽的 CHECK 对旧数据无破坏。
- **默认关闭**：`enabled_tools` 默认仍为 `[claude, codex]`，Pi 未启用时服务迭代不经过 Pi，原五工具行为零变化。
- **第三方适配器依赖**：`ASSIGNABLE_MCP_TOOLS` 保持 `Tool::ALL`，Pi 的 MCP 目标在 `adapter_for` / descriptor 层收敛；因此**没有**「缩小共享集合」的改动。取而代之的风险是「适配器升级改变 schema」——由版本探测 + 最低支持版本常量 + 未知字段全量保留 + canary 测试共同兜底。
- **代码回滚顺序**：先在 UI 与共享集合关闭 Pi capability，再移除 service/project 分支，最后移除 Adapter；已应用迁移保留。
- **证据回滚**：任何原生写入失败走现有 snapshot/journal 恢复，不新增旁路脚本。

## 11. 测试矩阵

| 层             | 必测                                                                                                                                                                              |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Domain         | `Tool::Pi` 序列化往返、`Tool::ALL` 顺序、`tool_capabilities()` Pi 行、`supported_for_tool` Pi 全 false、`ASSIGNABLE_MCP_TOOLS` 精确成员（保持 6 元素）                            |
| Probe          | 六路探针成功/缺失/超时/异常输出/链接路径；`PI_CODING_AGENT_DIR` 可映射与不可映射两态；不探测 bundle                                                                               |
| Adapter probe  | 适配器三态（缺安装目录 / 已声明未加载 / 已声明已加载）、版本过低、项目级 `packages`、`exclusive` 模式                                                                             |
| Adapter        | 6 个 descriptor 的 scope/path/format/selector/symlink_policy/allowed_root/trust；无 Hook/Agent descriptor；Trusted/Untrusted 项目 Skills 矩阵；override 三态                      |
| Provider codec | 只给 `baseUrl` 保留内置模型；未知字段保留；`$ENV`/`!command` 不展开不执行；明文 apiKey 脱敏；stale/冲突                                                                           |
| MCP codec      | stdio/HTTP 双向映射；不写 `type`；停用↔`disabled`；`socket`/`oauth`/`directTools` 等未受管字段保留；零求值；明文凭据脱敏                                                          |
| MCP shadow     | `<root>/.mcp.json` / 用户手写 `.pi/mcp.json` 同名 → 硬阻断；非同名放行；适配器追加 `directTools` → 漂移检出；read-modify-write 不误删适配器搬入的非受管条目                       |
| DB             | v24 → v25 升级、旧行保留、canary（pi+hook/agent 被拒、pi+mcp/skill/provider/prompt 通过）、外键/索引/重开、`is_active_pi` 唯一索引                                                |
| Skills         | 全局/项目分配、导入、普通目录/外部链接/断链/逃逸、恢复、frontmatter 合同、同名冲突、`settings.json` 额外来源诊断                                                                  |
| 隔离 smoke     | `PI_CODING_AGENT_DIR=<tmp>` fixture：0.85.1 真实 loader 发现受管链接与 `AGENTS.md`；`AGENTS.override.md` 使写入无效；未信任项目忽略 `.pi/skills`（这些 smoke 固化为回归 fixture） |
| 跨层 E2E       | `src-tauri/tests/phase8_e2e.rs` 增加 Pi Prompt/Provider/MCP 的 Preview → Apply → 漂移 → Restore；Hook/Agent 请求全链路拒绝；适配器缺失时 MCP 全链路拒绝（零外部写入）             |
| 前端           | metadata 集合（含官方图标）、设置页默认不勾选、Hooks/Agents 页无 Pi 入口、MCP 页适配器缺失诊断、项目页签、能力文案与代码一致                                                      |

## 12. 权衡与替代方案

| 决策                 | 选择                              | 被否决的替代与原因                                                                                                                                                                                                    |
| -------------------- | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| MCP                  | Supported（前置：适配器就绪）     | 一律 Unsupported：用户已明确要求「适配器存在就要支持」；但**无适配器时仍 fail closed**，因为写无人读的文件必然无效。也不接管共享文件（`.mcp.json` 已被 Claude 项目 MCP 合同占用，双写会争 ownership 并泄漏给 Claude） |
| MCP 项目作用域       | Supported + `trust = NotRequired` | 套用 Skills 的 trust 门禁：`.pi/mcp.json` 不在 Pi 官方 trust 清单，适配器源码零 trust 读取，未信任项目同样生效——如实建模并给安全提示，避免与真实行为不符                                                              |
| MCP 遮蔽             | Apply 硬阻断                      | 仅 warning：被项目同名 server 覆盖时写入静默无效，属必然无效写入                                                                                                                                                      |
| Hooks                | Unsupported                       | 映射 `pi.on(...)` 扩展回调：与 OpenCode plugin 回调同构，不符合 command-only 可快照合同                                                                                                                               |
| Agents               | Unsupported                       | 把 `.pi/agents/*.md` 当 Pi 官方目录：它是 Trellis 扩展私有约定，换扩展即换目录/格式                                                                                                                                   |
| Provider 写入        | 条目级 JSON codec                 | 整文件 `WholeDocument` 覆写：会丢内置模型合并语义与用户未知字段                                                                                                                                                       |
| MCP 写入             | read-modify-write 条目级          | 整文件覆写：适配器自身也写这两个文件（`directTools`/`imports`/`disabled`），整文件覆写会丢适配器状态                                                                                                                  |
| `AGENTS.override.md` | 硬阻断 Apply                      | 仅 warning（Codex 先例）：Pi 此处写入**必然无效**，warning 会诱导用户以为已生效                                                                                                                                       |
| Prompt Templates     | 不纳入                            | 复用 `Prompt` artifact：作用域、文件形态、语义都不同，硬塞会污染现有模型                                                                                                                                              |
| Pi 默认启用          | 默认关闭                          | 随 Claude/Codex 默认开启：会让存量用户总览页出现未预期工具                                                                                                                                                            |
| 品牌图标             | 官方 Badge（MIT）字节级复制       | 自绘占位（ZCode 先例）：用户已确认走官方资产，且官方 Press Kit 提供许可清晰的方形 mark                                                                                                                                |
