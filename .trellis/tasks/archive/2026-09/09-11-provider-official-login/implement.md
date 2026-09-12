# 执行计划

按阶段推进；每个阶段结束运行对应校验，全部通过后再进入下一阶段。

## 阶段 0：基线

- [x] `cargo test --manifest-path src-tauri/Cargo.toml` 与 `pnpm test --run` 记录基线通过数（Rust 319 / 前端 283）。

## 阶段 1：模型与校验（后端，无行为破坏）

- [x] `profiles/models.rs`：新增 `ProviderAuthKind`；`ProviderOptionsInput` / `ProviderOptionsDto` / `StoredProviderConfig` 增加 `auth_kind`；`StoredProviderConfig::effective_auth_kind()`。
- [x] `validate_extra_env`：改用 `security::env_entry_is_manageable`（纯数值/布尔值放行；新增 `passphrase` 敏感键标记）。
- [x] `validate_wire_api` 接受 `chat`。
- [x] 重写 `validate_provider_fields` 为按 `auth_kind` + 工具的单入口（`ProviderFieldsInput`），Claude/Codex 模型可空、接入地址可空。
- [x] `ProviderImportPreviewDto` 增加 `auth_kind`、`skipped_env_keys`。
- 校验：`cargo build` ✓。

## 阶段 2：codec 与 profiles 服务

- [x] `adapters/discovery.rs`：`ProviderCodecInput` / `ProviderCodecProfileInput` 增加 `auth_kind`；`ProviderCodecDiscovery` 增加 `auth_kind`、`skipped_env_keys`；稳定字符串常量。
- [x] Claude codec：render / ownership（始终拥有四个保留键）/ discover（全量 env、跳过键、官方登录识别、模型只取 `ANTHROPIC_MODEL`）。
- [x] Codex codec：render（模型可空、官方登录）/ discover（模型可空）。
- [x] ZCode / OpenCode codec：补 `auth_kind: "api_key"` 与空 `skipped_env_keys`，行为不变。
- [x] profiles 服务贯通 `auth_kind`，官方档案跳过 URL/Key，`copy` 拒绝官方来源，更新不可改认证方式。
- [x] Rust 测试：新增 8 个用例并更新 3 个既有断言。
- 校验：`cargo test` 327 通过 ✓。

## 阶段 3：官方登录服务与命令

- [x] `app/tool_probe.rs`：抽出 `run_command_bounded` / `apply_tool_process_environment`，`resolve_executable`、`terminate_process_group`、`set_nonblocking`、`search_path()` 暴露为 `pub(crate)`。
- [x] 新增 `src-tauri/src/official_login/{mod,tests}.rs`：状态探测、启动/监视/取消登录子进程、DTO；6 个用假 CLI 脚本的测试。
- [x] `app/mod.rs`：`AppState.official_logins` 与 `official_login_context()`。
- [x] `commands/official_login.rs` + `lib.rs` 注册 + `commands/mod.rs` `COMMAND_SOURCES`。
- [x] `pnpm bindings:generate`。
- [x] 用真实 `claude auth status` / `codex login status` 做过一次只读探测验证（Claude 268 ms、Codex 130 ms，均识别为已登录）。
- 校验：`pnpm rust:check`、`pnpm bindings:check` ✓（含于 `pnpm check`）。

## 阶段 4：前端

- [x] `src/lib/profile-api.ts`：官方登录 query key/options。
- [x] `src/features/tool-profiles/official-login-section.tsx`、`provider-text.ts`。
- [x] `provider-panel.tsx`：认证方式单选、条件字段、列表文案、导入跳过键。
- [x] onboarding 向导复用共享文案；fixtures 与既有测试更新。
- [x] 新增 3 个前端测试（官方渠道编辑、新增官方渠道登录/取消/创建、CLI 不支持回退、导入跳过键）。
- 校验：`pnpm lint`、`pnpm typecheck`、`pnpm test --run`（286 通过）、`pnpm format:check` ✓。

## 阶段 5：收尾

- [x] 更新 `.trellis/spec/backend/quality-guidelines.md`（Provider 场景合同、错误矩阵、测试要求；新增"官方账号登录委托官方 CLI"场景）与 `.trellis/spec/frontend/quality-guidelines.md`。
- [x] README 核心能力表补充官方账号登录说明。
- [x] 独立 `trellis-check` 审阅：1 项严重（无 TTY 登录未验证）+ 9 项建议；已处理 S1（真机核验）、B1/B9（文档同步）、B2（规范升级说明）、B3（密码类键名不豁免）、B4（向导列出跳过键）、B5（终态测试）、B6（退出时 `cancel_all` + `Drop`）、B7（回收前清空进程组标识）；B8 保持既有行为。
- [x] `pnpm check`、`git diff --check`；提交待用户确认。

## 真机核验记录（2026-09-11）

- 只读探测：真实 `claude auth status --json` / `codex login status` 经本模块解析，Claude 268 ms、Codex 130 ms。
- 登录子进程：用只含 CLI 与基础工具、不含 `open` 的受限 PATH 启动真实 `claude auth login --claudeai` 与 `codex login`，两者在无 TTY 下都正常启动、打印授权地址并等待回调，6 秒后取消，阶段正确变为 `cancelled`。
- **事故**：`codex login` 启动时即删除了本机 `~/.codex/auth.json`，取消后 Codex 变为未登录，且本机没有任何备份可恢复；需要用户重新执行一次 `codex login`（或在应用内点击"登录官方账号"）。由此新增：已登录时再次登录先确认、Codex 说明文案写明清除行为、规范禁止在开发机上用真实登录命令做验证。
- 未做：真机 Tauri 窗口内点击按钮并在浏览器中完成整个授权（会改写本机凭据）。

## 回滚点

- 阶段 1/2 只改后端合同与测试，可整体 revert。
- 阶段 3 新模块独立，可移除命令注册回退。
- 阶段 4 依赖阶段 3 的绑定，需一起回退。
