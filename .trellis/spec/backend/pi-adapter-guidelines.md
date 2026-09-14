# Pi Adapter Guidelines

## Scenario: Pi configuration, third-party MCP dependency, and permanent fail-closed capabilities

### 1. Scope / Trigger

- Trigger：改动 `Tool::Pi` 的 descriptor、Provider/Prompt/MCP/Skills 同步、`PI_CODING_AGENT_DIR`
  解释、`pi-mcp-adapter` 就绪探测、Pi 的数据库放宽（迁移 `0025`）、Hooks/Agents 拒绝路径，或
  新增 Pi 相关诊断码。
- 覆盖 `src-tauri/src/adapters/pi/`、`src-tauri/src/adapters/discovery.rs`、
  `src-tauri/src/app/tool_probe.rs`、`src-tauri/src/profiles/`、`src-tauri/src/mcp/`、
  `src-tauri/src/skills/`、`src-tauri/src/sync/`、`src-tauri/src/db/migrations/` 与
  `src-tauri/tests/phase8_e2e.rs`。

### 2. Signatures

- 环境：`ExplicitEnvironment::with_pi_agent_dir(path)` / `with_pi_agent_dir_unmapped()` /
  `with_pi_mcp_exclusive_mode(bool)`；访问器 `pi_agent_dir()`、`pi_agent_dir_unmapped()`、
  `pi_mcp_exclusive_mode()`。Adapter 内部**永不**读 `std::env`；边界读取只在 `lib.rs`。
- Adapter：`PiAdapter::capability(context)`、`ProviderCodec for PiAdapter`（`render`/`discover`/
  `ownership`/`default_options`）；只读探测 `probe::probe_mcp_adapter(PiMcpAdapterProbeInput)`；
  遮蔽检测 `pi::detect_mcp_shadowing(root, managed_names, scope)`；链接自检
  `pi::managed_children_symlink_diagnostic(dir, names, allowed_root)`；提示词回退探测
  `pi::prompt_fallback_present(agent_dir)` 与 `pi::inline_api_key_diagnostic(api_key)`。
- 服务：`StoredProviderConfig::from_input(Tool::Pi, provider_id, options, extra_fields)`；
  `mcp::service::native_mcp_item`（Pi 分支不写 `type`）；`sync::build_preview_plan` 的
  `PreviewTargetRequest.hard_block`。

### 3. Contracts

- **唯一写入面（6 个 descriptor）**：`<pi_agent_dir>/models.json`（`providers` 条目级局部合并）、
  `<pi_agent_dir>/AGENTS.md`（整文档）、`<pi_agent_dir>/skills` 与 `<root>/.pi/skills`
  （`ManagedChildrenOnly` 受管子链接）、`<pi_agent_dir>/mcp.json` 与 `<root>/.pi/mcp.json`
  （`mcpServers` 条目级 + 逐名称 selector）。不存在 Hook/Agent descriptor。
- **`allowed_root` 只能是 `<pi_agent_dir>` 或 canonicalize 后的项目根**；不得回退到 `~/.pi` 或 `HOME`。
- **Hooks/Agents 永久 fail closed**：`domain::HookEvent::supported_for_tool(Tool::Pi) == false`、
  `ASSIGNABLE_HOOK_TOOLS`/`ASSIGNABLE_AGENT_TOOLS`/`PROJECT_AGENT_TOOLS` 不含 Pi、
  `hooks::ensure_hooks_supported` 与 `agents::unsupported_agent_tool` 返回
  `PI_HOOKS_UNSUPPORTED` / `PI_AGENTS_UNSUPPORTED`、`managed_targets` 的
  `tool != 'pi' OR artifact_kind IN ('provider','prompt','mcp','skill')` 在数据库层继续拒绝。
- **MCP 依赖第三方适配器**：`<pi_agent_dir>/settings.json` 的 `packages[]` 声明 +
  `<pi_agent_dir>/npm/node_modules/pi-mcp-adapter/package.json` 版本 ≥ 最低支持版本
  （当前 `2.33.0`）才 `supported`；否则两个 MCP descriptor 必须
  `TargetCapability::unsupported(PI_MCP_ADAPTER_MISSING|_NOT_LOADED|_VERSION_UNSUPPORTED)` 且
  `path = None`。不得用 `pi list` 输出或运行时会话作为判据。
- **零求值**：`apiKey`/`headers`/`env` 值原样保留；绝不展开 `$ENV`、绝不执行 `!command`。
  明文 `apiKey` 只产出 `PI_PROVIDER_INLINE_API_KEY` 诊断并走既有脱敏。
- **项目 trust**：`<root>/.pi/skills` 沿用 Pi 的 `trust.json`/`defaultProjectTrust` 语义（`ask`
  静态不可判定 → `Unknown`），非 Trusted 时以 `PI_PROJECT_SKILLS_UNTRUSTED` /
  `PI_PROJECT_SKILLS_TRUST_UNKNOWN` 阻断 Apply；`<root>/.pi/mcp.json` 标注
  `trust = NotRequired`（适配器不读 trust），但 Apply 前给出同名遮蔽硬阻断。
- **硬阻断优先于 warning**：`AGENTS.override.md` 存在或不可判定、MCP 同名遮蔽、断链/逃逸链接
  都必须经 `PreviewTargetRequest.hard_block` 或 Pi 专用分支把 `change_kind` 固定为 `Conflict`，
  不能只推 warning。新增诊断码一律用 `crate::adapters::pi::PI_*` 常量，不复用 Codex 文案。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| ------------------------------------------------- | -------------------------------------------------------------------- |
| `PI_CODING_AGENT_DIR` 存在但不可映射（相对路径等） | 6 个 descriptor 全部 `unsupported(PI_AGENT_DIR_OVERRIDE_UNMAPPED)`，不暴露默认路径 |
| Pi 未安装 / 探针异常 | `ToolNotInstalled` / `unsupported(PI_INSTALLATION_PROBE_UNSUPPORTED)` |
| `pi-mcp-adapter` 缺失 / 未加载 / 版本过低 | 两个 MCP 目标 `unsupported(PI_MCP_ADAPTER_*)`，`path = None`，零外部写入 |
| `PI_MCP_CONFIG_MODE=exclusive` | 项目 MCP descriptor `unsupported(PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED)` |
| 项目文件与受管 MCP 名称同名 | 项目预览 `Conflict` + `PI_MCP_SHADOWED_BY_PROJECT_SHARED` / `_PROJECT_PI` |
| `AGENTS.override.md` 存在或不可判定 | Prompt 预览 `Conflict` + `PI_PROMPT_OVERRIDE_DETECTED`，Apply 拒绝 |
| 用户存在 `CLAUDE.md`/`AGENTS.MD` 回退文件 | 仅提示 `PI_PROMPT_FALLBACK_PRESENT`（不阻断） |
| 受管子链接断链 / 逃逸 `allowed_root` | `Conflict` + `PI_SKILL_SYMLINK_BROKEN` / `PI_SKILL_SYMLINK_ESCAPE` |
| 项目未受信任（Skills） | `untrusted` + `PI_PROJECT_SKILLS_UNTRUSTED` / `_TRUST_UNKNOWN` |
| 任意 Hook / Agent 入口携带 `tool == pi` | `INVALID_INPUT` + `PI_HOOKS_UNSUPPORTED` / `PI_AGENTS_UNSUPPORTED`，零写入 |
| `managed_targets` 写入 `pi` + `hook`/`agent` | 数据库 CHECK 拒绝 |

### 5. Good / Base / Bad Cases

- Good：`models.json` 已有非受管 `other` provider 与未知顶层键；Apply 后只新增/更新受管条目，
  未知键与 `other` 逐字节保留，Restore 回到 Apply 前文件。
- Base：适配器未安装时 MCP 页面只显示诊断与安装指引（`pi install npm:pi-mcp-adapter`），
  不创建 `mcp.json`、不生成 assignment 行。
- Bad：把 Pi 的 MCP 写入复用 Claude 的 `type: stdio` 写法；把 `<pi_agent_dir>/settings.json`
  当受管目标；用 `AGENTS.override.md` 场景下的 warning 代替硬阻断；把 `~/.pi/agents` 当作
  Pi 的 Agents 合同。

### 6. Tests Required

- Adapter：6 个 descriptor 的 scope/path/format/selector/trust/allowed_root 矩阵；无 Hook/Agent
  descriptor；适配器三态与版本过低；`exclusive` 模式；遮蔽与断链自检。
- Provider codec：条目级合并保留未知字段与非受管 provider、不写 `models: []`、
  `$ENV`/`!command` 原样保留、明文 apiKey 诊断与脱敏。
- Database：v24 → v25 升级、旧行保留、同连接插入、重开、外键/索引、
  canary（`pi` + `hook|agent` 被拒，`pi` + `provider|prompt|mcp|skill` 可插入，
  `agent_*`/`hook_*` 表继续拒绝 `pi`）。
- Service/E2E：`src-tauri/tests/phase8_e2e.rs` 的 Pi 全链路（Provider/Prompt/MCP 的
  Preview → Apply → 漂移 → Restore、override 硬阻断、MCP 遮蔽、适配器缺失、
  Hooks/Agents 拒绝、项目 Skills 未受信任）。
- 隔离 smoke：`src-tauri/tests/pi_adapter_smoke.rs` 在 `HOME`/`PI_CODING_AGENT_DIR` 隔离下直接
  验证适配器遵守 agent dir 与项目文件优先级；未安装适配器或 `node` 时跳过并打印原因。
- 每次改动跑 `pnpm rust:check`、`pnpm typecheck`、`pnpm test --run`、`pnpm bindings:check`、
  `git diff --check`。

### 7. Wrong vs Correct

#### Wrong

```rust
// 适配器未就绪时仍然沿用 Claude 的 MCP 写法写入 mcp.json：
// Pi 核心不会读它，写入必然无效，而且用户以为已经生效。
(Tool::Pi, McpTransport::Stdio) => {
    object.insert("type".to_owned(), Value::String("stdio".to_owned()));
    object.insert("command".to_owned(), Value::String(stdio_command(value)?));
}
```

#### Correct

```rust
// 1) 先由 descriptor 层 fail closed：适配器未就绪时 capability = unsupported(...)、path = None；
// 2) 就绪时按 Pi 的字段存在性契约投影，不写 type，停用映射为 disabled。
(Tool::Pi, McpTransport::Stdio) => {
    object.insert("command".to_owned(), Value::String(stdio_command(value)?));
    if !value.enabled {
        object.insert("disabled".to_owned(), Value::Bool(true));
    }
}
```
