# 技术设计

## 1. 边界与数据模型

### 1.1 认证方式枚举

```rust
// profiles/models.rs
#[derive(Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthKind { ApiKey, OfficialLogin }
```

- `ProviderOptionsInput.auth_kind: ProviderAuthKind`（`#[serde(default)]` = `ApiKey`，前端总是显式传）。
- `ProviderOptionsDto.auth_kind: ProviderAuthKind`。
- `StoredProviderConfig.auth_kind: Option<ProviderAuthKind>`（`skip_serializing_if = None`）。读取时 `effective_auth_kind(tool)`：显式值优先；否则 Codex 且 `provider_id == "openai"` → `OfficialLogin`；其余 → `ApiKey`。不做迁移。
- `ProviderProfileDto` 保持字段，`api_base_url` / `default_model` 允许空串，新增 `auth_kind` 走 options。

### 1.2 校验合同（`validate_provider_fields*` 重写为单入口）

| auth_kind | api_base_url | api_key | default_model | Claude extra_env | Codex wire_api |
| --- | --- | --- | --- | --- | --- |
| ApiKey | 必填，无凭据的绝对 HTTP(S) URL | 必填（更新时可 keep） | 可空 | 允许 | `responses` / `chat` / 空 |
| OfficialLogin | 必须为空 | 必须为空（更新时 keep/clear 均视为无） | 可空 | 允许 | 必须为空 |

- `validate_extra_env`：键名规则不变；密钥检测改为 `is_plain_env_value(value)`（仅数字/小数点、或 `true/false/0/1/on/off/yes/no` 不区分大小写）时直接放行，否则沿用 `contains_detectable_secret`。
- 新增 `discoverable_extra_env(key, value) -> bool` 复用同一规则，供 discover 过滤。

### 1.3 codec 输入

- `ProviderCodecInput` 与 `ProviderCodecProfileInput` 增加 `auth_kind: &str`（稳定字符串 `api_key` / `official_login`，避免适配层依赖 profiles DTO）。
- `ProviderCodecDiscovery` 增加 `auth_kind: String` 与 `skipped_env_keys: Vec<String>`。
- `ProviderImportPreviewDto` 增加 `auth_kind: ProviderAuthKind` 与 `skipped_env_keys: Vec<String>`。

## 2. Claude codec（`adapters/claude/mod.rs`）

- 常量 `RESERVED_ENV_KEYS = [BASE_URL, API_KEY, AUTH_TOKEN, MODEL]`。
- `render`：
  - `ApiKey`：`env[BASE_URL] = api_base_url`，`env[credential_env_key] = api_key`；
  - 两种类型都：`default_model` 非空则 `env[MODEL]`；再合并 `extra_env`。
- `ownership(baseline, desired)`：`RESERVED_ENV_KEYS ∪ keys(baseline.env) ∪ keys(desired.env)`，永不为空。
- `discover`：
  - `base_url` 非空 → `ApiKey`，凭据取 AUTH_TOKEN 优先，其次 API_KEY；
  - `base_url` 为空且无凭据键 → `OfficialLogin`；env 过滤后为空则返回 `None`；
  - `base_url` 为空但有凭据键（罕见）→ `None`。
  - `default_model` 只取 `ANTHROPIC_MODEL`。
  - `extra_env` = env 中所有非保留、字符串值、通过 `discoverable_extra_env` 的键；未通过者进入 `skipped_env_keys`。
  - projection = 保留键（存在者）+ extra_env，确保与 render 结果一致 → 首次预览 `unchanged`。

## 3. Codex codec（`adapters/codex/mod.rs`）

- `render`：
  - `OfficialLogin` 或 `provider_id == "openai"`：`{ model?, model_provider: "openai" }`（显式写回内置 provider，才能保证切换时清掉手写的 `model_provider = "relay"`；对 Codex 而言与缺省等价）；
  - `ApiKey`：`{ model?, model_provider, model_providers.<id>{ name, base_url, experimental_bearer_token, wire_api?, extra } }`。
- `ownership`：`{model_provider} ∪ ({model} if in baseline ∪ desired) ∪ model_providers.<id> (baseline ∪ desired)`；两种类型的 desired 都含 `model_provider`，因此实现无需单独追加。
- `discover`：`model` 可空；`model_provider` 缺省/`openai` → 走 `discover_openai_provider`（要求 auth.json 有 token），投影只含存在的 `model`/`model_provider`；自定义 provider 逻辑不变，`wire_api` 接受 `chat`。
- `validate_wire_api`：`None | "responses" | "chat"`。

## 4. 官方登录服务（新模块 `src-tauri/src/official_login/mod.rs`）

```rust
pub enum OfficialLoginPhase { Idle, Running, Succeeded, Failed, Cancelled, TimedOut }
pub struct OfficialLoginStatusDto {
    tool: Tool, supported: bool, phase: OfficialLoginPhase,
    logged_in: Option<bool>, auth_method: Option<String>, account: Option<String>,
    diagnostic: Option<String>, login_url: Option<String>, manual_command: String,
}
```

- 输入 `OfficialLoginContext { search_path: OsString, home, claude_config_dir, codex_home, proxy: Option<String> }`，由 `AppState` 从 `EnvironmentProbeConfig.input`（新增 `search_path()` 访问器）与 `github_proxy` 组装；探针未配置（测试进程）时命令返回 `INVALID_INPUT`。
- 可执行文件解析复用 `app::tool_probe::resolve_executable`（改为 `pub(crate)`）。
- 状态探测 `probe_status(tool)`：`claude auth status --json` / `codex login status`，8 秒超时，输出上限 16 KiB；解析：
  - Claude：JSON `loggedIn`(bool)、`authMethod`、`email`；解析失败或退出码非 0 且 stderr 含 `unknown command` → `supported=false`；
  - Codex：退出码 0 且 stdout 含 `Logged in` → `logged_in=true`，`auth_method` 取 `chatgpt` / `api_key`；退出码 1 且含 `Not logged in` → `false`。
- 登录会话 `start(tool)`：`claude auth login --claudeai` / `codex login`；`Command::env_clear` 后注入 HOME、CLAUDE_CONFIG_DIR、CODEX_HOME、PATH、NO_COLOR=1、TERM=dumb、代理三件套（有则注入）；`process_group(0)`；stdin null；stdout/stderr 由监视线程非阻塞排空到 4 KiB 尾部缓冲；超时 10 分钟后 `terminate_process_group`。会话状态 `Arc<Mutex<LoginSession { phase, output, cancel_requested, process_group }>>` 存于 `AppState.official_logins`；同一工具同时只允许一个运行中的会话。
- `login_url`：从会话输出中取第一个 `https://` 地址（两个 CLI 都会打印授权地址，只含 PKCE challenge/state 等公开参数），运行中返回给前端供浏览器未自动打开时手动访问。
- `cancel(tool)`：终止进程组，phase=Cancelled；`cancel_all()` 供 Tauri `RunEvent::Exit` 与 `Drop` 调用，避免退出后 `codex login` 继续占用 1455 回调端口。
- `status(tool)`：先读会话 phase；若 Running 直接返回（不跑探测）；否则跑一次探测并合并；`diagnostic` 取子进程输出末尾 ≤ 300 字符，经 `SecretRedactor::redact_text` 脱敏。
- 2026-09-11 真机核验（受限 PATH、无浏览器）：两个 CLI 在无 TTY 下都能启动并打印授权地址后等待回调；**`codex login` 在启动时立即删除现有 `~/.codex/auth.json`**，中途取消即等于登出——因此前端在已登录时再次登录必须先确认，并在说明中写明该行为。
- 命令层 `commands/official_login.rs`：`get_official_login_status`、`start_official_login`、`cancel_official_login`，均 `#[tauri::command(async)]`；加入 `COMMAND_SOURCES` 与 `collect_commands!`。

## 5. Profiles 服务

- `create/update/copy/confirm_import` 统一经 `ProviderFieldsInput { auth_kind, name, api_base_url, api_key: Option, default_model }` 校验；`copy_provider_profile` 对 `OfficialLogin` 来源拒绝（`INVALID_INPUT`）。
- `update_provider_profile`：允许 `default_model` 为空写 NULL；`OfficialLogin` 忽略 `api_key`（存 NULL）。
- `provider_dto`：`default_model` / `api_base_url` 为 NULL 时输出空串（已有行为）。
- `confirm_provider_import` / `validate_discovered_provider_config` 传递 `auth_kind`。

## 6. 前端

- `provider-panel.tsx`：
  - 表单新增字段 `authKind`（radio：`API Key` / `官方账号登录`），仅 Claude/Codex 显示，其余工具固定 `api_key`；编辑时按 DTO 初始化且不可切换（避免同一档案在两种合同间漂移）。
  - `official_login` 时隐藏 API 地址/API Key，显示 `OfficialLoginSection`。
  - 列表凭据文案：`官方账号登录` / `密钥已遮罩保存` / `密钥未配置`；官方档案禁用"复制到 …"。
  - 导入预览：显示 `authKind` 文案；`skippedEnvKeys` 非空时列出"未纳入管理的疑似凭据键"。
- 新组件 `src/features/tool-profiles/official-login-section.tsx`：`useQuery(officialLoginStatusQueryOptions(tool))`，`refetchInterval` 在 phase=running 时 2 s；按钮：登录官方账号 / 取消登录 / 刷新状态；展示手动命令。
- `src/lib/profile-api.ts`：`profileKeys.officialLogin(tool)`、`officialLoginStatusQueryOptions(tool)`。
- 测试：`tool-profiles-page.test.tsx` 增补；新增 `official-login-section.test.tsx`。

## 7. 兼容与回滚

- 旧 `config_json` 无 `authKind`：读取时推导；不改列。
- 旧前端调用（无 `authKind`）：serde default `api_key`。
- 回滚点：每个 implement.md 阶段独立可编译、测试通过；官方登录模块可整体不注册命令而不影响其余修复。
