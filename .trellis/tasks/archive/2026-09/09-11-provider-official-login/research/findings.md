# 调研结论（2026-09-11）

## 已核实缺陷（含代码位置）

| 编号 | 现象 | 根因 |
| --- | --- | --- |
| A1 | Claude 额外 env 不完整 | `adapters/claude/mod.rs` `discover()` 只保留 `key.starts_with("ANTHROPIC_")` ��键；本机 settings.json 的 `API_TIMEOUT_MS`、`CLAUDE_CODE_*`、`MCP_TIMEOUT`、`DISABLE_INSTALLATION_CHECKS` 等全部丢失 |
| A2 | 保存 `CLAUDE_CODE_MAX_OUTPUT_TOKENS=32000` 被拒 | `profiles/models.rs` `validate_extra_env()` 调 `security::contains_detectable_secret(key, value)`，`is_sensitive_key` 对键名做子串匹配（`token`/`apikey`），不看值 |
| A3 | 无 `ANTHROPIC_MODEL` / `model` 时检测返回 None | Claude/Codex `discover()` 把 default_model 当必填；`validate_provider_fields*` 也要求非空 |
| A4 | 导入后首次预览出现新增 `ANTHROPIC_MODEL` | discover 从 `ANTHROPIC_DEFAULT_*_MODEL` 推导 default_model 且同时把该键放进 extra_env，render 又写 `ANTHROPIC_MODEL` |
| A5 | Codex `wire_api = "chat"` 被拒 | `validate_wire_api` 只允许 `responses`；Codex 官方支持 `chat`/`responses` |
| A6 | 无法新增官方登录渠道 | `ProviderProfileInput` 强制 URL/Key/Model；Codex OAuth 只能靠 auth.json 已有 token 的导入路径；Claude 官方态不可表达 |
| A7 | 切换渠道后残留手工凭据键 | Claude `ownership()` 只取 baseline ∪ desired 的键，保留键不在其中就不清理 |

## cc-switch（v3.20.3，farion1231/cc-switch）设计要点

- Claude 官方预设 `Claude Official`：`settingsConfig.env = {}`，`isOfficial: true`，`category: "official"`。切换到官方即把 env 清空，Claude Code 回到自带登录（cc-switch 不实现 Claude OAuth）。
- Codex 官方预设 `OpenAI Official`：`providerType: "codex_oauth"`，`auth: {}`，`config: ""`，`requiresOAuth`。cc-switch 在应用内实现 OpenAI Device Code 流程（`auth.openai.com/api/accounts/deviceauth/usercode` → 用户在 `auth.openai.com/codex/device` 输码 → 轮询 `deviceauth/token` 得 `authorization_code + code_verifier` → `oauth/token` 换 token，client_id `app_EMoamEEZ73f0CkXaXp7hrann`），并把 `{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{id_token,access_token,refresh_token,account_id},"last_refresh"}` 写入 `~/.codex/auth.json`。它需要 token 是因为自带反代；本项目不反代。
- 表单：官方预设隐藏 API Key 输入，展示 `CodexOAuthSection`（登录状态、账号选择、登录/登出按钮）。

## 本机官方 CLI 能力（2026-09-11）

- Claude Code 2.1.268：`claude auth login [--claudeai|--console|--email|--sso]`、`claude auth logout`、`claude auth status [--json|--text]`（默认 JSON：`loggedIn`、`authMethod`、`apiProvider`，登录 claude.ai 时另含 `email`、`subscriptionType`、`orgName`）。凭据存于 macOS Keychain，第三方应用不应自行写入。
- codex-cli 0.154.0：`codex login`（浏览器回调 localhost:1455）、`codex login --device-auth`、`codex login status`（stdout `Logged in using ChatGPT` / `Logged in using an API key` / `Not logged in`，退出码 0/1）、`codex logout`。

## 决策

- 登录委托官方 CLI 子进程，不重实现 OAuth：符合"只用官方合同"的项目原则，凭据不经过本应用。
- 官方渠道通过既有 Preview/Apply 管线写 settings.json / config.toml；登录子进程本身由用户显式点击触发。
