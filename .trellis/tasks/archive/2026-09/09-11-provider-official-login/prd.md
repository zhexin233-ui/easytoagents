# 修复 Claude/Codex 渠道问题并支持官方账号登录渠道

## 目标

修复 Claude 与 Codex 渠道（Provider）在检测、导入、编辑与同步中的已核实缺陷，并新增"官方账号登录"这一渠道类型：用户在"新增渠道"时可以直接选择官方账号，在应用内一键触发官方 CLI 的 OAuth 登录（`claude auth login` / `codex login`），登录状态在应用内可见。设计参考 cc-switch：官方预设 = 清空第三方接入配置、回到工具自带登录；第三方预设 = API Key + 接入地址。

## 需求来源

用户 2026-09-11 反馈：

1. "修复 claude 和 codex 的渠道问题"。
2. "支持新增渠道的时候直接可以 oauth 登录官方账号，可以参考 cc-switch 的设计"。
3. "claude 的额外 env 并没有完全获取配置"。

调研核实的具体缺陷见 `research/findings.md`。

## 需求

### R1 Claude 额外 env 完整接管

- 检测/导入 Claude 渠道时，`settings.json` 的 `env` 中所有字符串值的键都应进入"额外 env"，不再只保留 `ANTHROPIC_*` 前缀；保留键（`ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`、`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_MODEL`）仍由专用字段承载。
- 可识别为凭据的键值（如 `ANTHROPIC_CUSTOM_HEADERS=Authorization: Bearer …`、`AWS_BEARER_TOKEN_BEDROCK=…`）不纳入额外 env，也不进入受管基线；导入预览需列出这些被跳过的键名（不含值）。
- 纯数值/布尔样式的值不因键名含 `TOKEN`/`KEY` 被误判为密钥：`CLAUDE_CODE_MAX_OUTPUT_TOKENS=32000`、`MAX_THINKING_TOKENS=…`、`CLAUDE_CODE_API_KEY_HELPER_TTL_MS=…` 必须可保存与导入。
- 默认模型只来自 `ANTHROPIC_MODEL`；`ANTHROPIC_DEFAULT_*_MODEL` 作为额外 env 原样保留，导入后首次预览应为"未变更"。

### R2 默认模型可选

- Claude/Codex 渠道的默认模型允许为空：为空时不写入 `ANTHROPIC_MODEL` / `model`，也不删除用户自行维护的该键（除非上一份生效档案曾写入它）。
- 检测已有配置时，缺少 `ANTHROPIC_MODEL` / `model` 不再导致"未检测到可导入配置"。

### R3 官方账号登录渠道

- "新增渠道"提供"认证方式"选择：`API Key`（第三方/自定义接入）或 `官方账号登录`。
- 官方账号登录渠道不填写 API 地址与 API Key；Claude 官方渠道仍可维护额外 env 与可选默认模型，Codex 官方渠道可维护可选默认模型。
- 应用 Claude 官方渠道时移除受管的 `ANTHROPIC_BASE_URL` / `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN`，使 Claude Code 回到自带的 claude.ai 登录；应用 Codex 官方渠道时移除旧的自定义 `model_providers.<id>` 并显式写回 `model_provider = "openai"`，使 Codex 回到内置 provider（`auth.json` 登录态）。
- 官方渠道表单与列表中展示登录状态（已登录/未登录/登录方式/账号），并提供"登录官方账号"按钮：由应用启动官方 CLI（`claude auth login`、`codex login`）完成 OAuth，应用轮询状态直至完成；提供取消与手动命令提示，并在浏览器未自动打开时展示授权地址。
- 已登录状态下再次发起登录必须先确认；Codex 在开始登录时会立即清除现有 `auth.json` 凭据（2026-09-11 真机核验），说明与确认文案必须写明"中途取消需要重新登录"。
- 应用自身不实现 OAuth 端点、不保存 access/refresh token；凭据始终由官方 CLI 写入其自己的存储（Keychain / `auth.json`）。
- 既有通过"检测已有配置"导入的 Codex OAuth 档案（`providerId == "openai"`）自动视为官方账号登录渠道，行为不回退。

### R4 Codex wire_api

- `wire_api` 接受 `responses` 与 `chat` 两个官方取值。

### R5 渠道所有权收紧

- Claude 渠道同步始终拥有四个保留键：切换渠道后不会残留上一份手工写入的凭据键。
- Codex 渠道同步始终拥有 `model_provider`。

## 约束

- 不新增数据库迁移：官方登录类型存放在 `config_json`，`api_base_url` / `api_key` / `default_model` 列本就允许 NULL。
- 保持"中央档案 CRUD 不写原生配置，先预览再应用"的合同；登录子进程是用户显式点击触发的官方 CLI 动作，不经过预览，但只由官方 CLI 写入其自己的凭据存储。
- 子进程遵循现有探针的安全约束：只在安全 PATH 条目中解析可执行文件、`env_clear` 后显式注入 HOME / CLAUDE_CONFIG_DIR / CODEX_HOME / PATH / 代理变量、独立进程组、超时后终止整个进程组、输出有界且脱敏后才进入 DTO。
- 前端不新增类型断言、`any` 或 `eslint-disable`；后端生产路径不新增 `unwrap/expect/panic`。
- 提交前 `pnpm check`、`pnpm bindings:check`、`git diff --check` 全绿。

## 验收标准

- [x] 用一份含 `API_TIMEOUT_MS`、`CLAUDE_CODE_MAX_OUTPUT_TOKENS`、`ANTHROPIC_DEFAULT_*_MODEL`、`ANTHROPIC_CUSTOM_HEADERS`（含 Bearer）的 `settings.json` 检测并导入 Claude 渠道：前三类进入额外 env，`ANTHROPIC_CUSTOM_HEADERS` 被跳过并在预览中列名；导入后首次预览为未变更。（Rust 测试 `claude_provider_import_captures_all_env_and_official_switch_removes_credentials`）
- [x] 编辑 Claude 渠道时保存 `CLAUDE_CODE_MAX_OUTPUT_TOKENS=32000` 成功；保存 `ANTHROPIC_CUSTOM_HEADERS=Authorization: Bearer x` 仍被拒绝。（`claude_extra_env_accepts_numeric_limit_keys_and_rejects_credentials_and_reserved_keys`、`provider_validation_rejects_url_credentials_and_unsupported_wire_api`）
- [x] 无 `ANTHROPIC_MODEL` 的 settings.json 与无 `model` 的 config.toml 都可被检测导入，默认模型为空。（`claude_provider_import_with_base_url_but_no_model_keeps_empty_default_model`、`codex_provider_import_without_model_keeps_empty_default_model`）
- [x] 新增 Claude 官方渠道并应用后，settings.json 中受管的 BASE_URL/API_KEY/AUTH_TOKEN 被移除，其余 env 与非 env 内容保持不变；再切回第三方渠道可恢复。（同第一条测试）
- [x] 新增 Codex 官方渠道并应用后，config.toml 不含旧的自定义 `model_providers` 表、显式回到 `model_provider = "openai"`，其余表保持不变。（`codex_official_login_switch_and_chat_wire_api_are_supported`）
- [x] 官方渠道表单可显示登录状态；点击"登录官方账号"会启动官方 CLI 子进程，完成/失败/取消/超时四种结果都能在界面上区分；未安装或 CLI 不支持登录子命令时显示手动命令提示。（Rust `official_login::tests` 用假 CLI 覆盖四种结果；前端测试覆盖登录/取消/不支持回退；真实 CLI 只做了只读状态探测验证）
- [x] Codex 渠道 `wire_api = "chat"` 可保存并写入。
- [x] Rust 单元测试与前端测试覆盖上述项。
- [x] `pnpm check` 与 `pnpm bindings:check` 全绿；`.trellis/spec` 后端/前端 Provider 场景更新。

## 范围外

- 应用内自行实现 OpenAI/Anthropic OAuth 端点或代理转发（cc-switch 的反代能力）。
- 多账号管理、订阅额度查询、模型列表拉取。
- ZCode / OpenCode / Cursor 渠道行为变更。
