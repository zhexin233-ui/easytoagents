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
  `ownership`/`default_options`/`merge_import_baseline`；`discover` 返回候选**列表**）；只读探测
  `probe::probe_mcp_adapter(PiMcpAdapterProbeInput)`；
  遮蔽检测 `pi::detect_mcp_shadowing(root, managed_names, scope)`；链接自检
  `pi::managed_children_symlink_diagnostic(dir, names, allowed_root)`；提示词回退探测
  `pi::prompt_fallback_present(agent_dir)` 与 `pi::inline_api_key_diagnostic(api_key)`。
- 服务：`StoredProviderConfig::from_input(Tool::Pi, provider_id, options, extra_fields)`；
  `mcp::service::native_mcp_item`（Pi 分支不写 `type`）；`sync::build_preview_plan` 的
  `PreviewTargetRequest.hard_block`。

### 3. Contracts

- **Provider 候选与 `models` 合并**：`discover` 返回 `providers` 下**全部**条目（`defaultProvider`
  只标注默认渠道，不参与取舍）；`models` 属于档案内容，渲染时**按模型 id 合并**（原数组逐字段
  保留，仅当默认模型不在其中时追加裸 `{ "id": … }`）；禁止把数组整段替换成单个 `{ id }`，也
  禁止写 `models: []`。多候选导入的受管基线是「已导入 provider」的并集（`merge_import_baseline`），
  未导入的 provider 永不入基线。完整契约见下方 Provider 场景。
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
- **只读边界**：`settings.json` / `trust.json` 不属于受管目标，任何流程都不得写回；仅允许为
  Provider 默认选择、package 就绪和项目 trust 做限量、拒绝符号链接的只读探测。该静态证据不
  代表某次 Pi 会话的 `--approve` / `--no-approve` 或扩展临时 trust 决定。
- **scope 聚合**：全局适配器 Ready 时，项目 package 未受信任或被过滤不能撤销全局扩展已加载
  的事实；只有全局已安装版本满足最低要求时才能提前返回 Ready。全局版本过低不得提前短路，
  仍须进入既有聚合顺序；仅当全局不 Ready 时才由项目声明、trust、过滤和安装版本决定当前项目
  上下文能力，没有任何 Ready scope 时保持 `Filtered -> NotLoaded` 优先于
  `Installed(版本过低) -> VersionUnsupported`。
- **MCP 容器兼容**：所有 Pi MCP 生产观测统一按 `mcpServers ?? mcp-servers` 读取；canonical
  存在时绝不合并 alias 独有条目。alias-only 文件投影成 canonical 供 Import、Preview、漂移、
  原生资源与 Restore 共用，成功写入后删除 alias，并保留未知顶层字段与非受管 server。
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

| 条件                                               | 必须结果                                                                                  |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `PI_CODING_AGENT_DIR` 存在但不可映射（相对路径等） | 6 个 descriptor 全部 `unsupported(PI_AGENT_DIR_OVERRIDE_UNMAPPED)`，不暴露默认路径        |
| Pi 未安装 / 探针异常                               | `ToolNotInstalled` / `unsupported(PI_INSTALLATION_PROBE_UNSUPPORTED)`                     |
| `pi-mcp-adapter` 缺失 / 未加载 / 版本过低          | 两个 MCP 目标 `unsupported(PI_MCP_ADAPTER_*)`，`path = None`，零外部写入                  |
| 全局版本过低 + 项目 package 被过滤                 | `PI_MCP_ADAPTER_NOT_LOADED`，并保留已观测的全局版本；不得提前返回版本不支持               |
| `PI_MCP_CONFIG_MODE=exclusive`                     | 项目 MCP descriptor `unsupported(PI_MCP_EXCLUSIVE_MODE_PROJECT_IGNORED)`                  |
| alias-only MCP 容器                                | 正常观测 + `PI_MCP_CONTAINER_ALIAS_DETECTED`；Apply 后只保留 `mcpServers`                 |
| 项目文件与受管 MCP 名称同名                        | 项目预览 `Conflict` + `PI_MCP_SHADOWED_BY_PROJECT_SHARED` / `_PROJECT_PI`                 |
| `AGENTS.override.md` 存在或不可判定                | Prompt 预览 `Conflict` + `PI_PROMPT_OVERRIDE_DETECTED`，Apply 拒绝                        |
| 用户存在 `CLAUDE.md`/`AGENTS.MD` 回退文件          | 仅提示 `PI_PROMPT_FALLBACK_PRESENT`（不阻断）                                             |
| 受管子链接断链 / 逃逸 `allowed_root`               | `Conflict` + `PI_SKILL_SYMLINK_BROKEN` / `PI_SKILL_SYMLINK_ESCAPE`                        |
| 项目未受信任（Skills）                             | `untrusted` + `PI_PROJECT_SKILLS_UNTRUSTED` / `_TRUST_UNKNOWN`                            |
| `providers` 条目非对象 / provider id 非法          | 该候选 `invalid` + `PI_PROVIDER_ENTRY_INVALID` / `PI_PROVIDER_ID_INVALID`，不阻断其他候选 |
| 候选缺 `apiKey` 或字段不受支持                     | 该候选 `invalid` + `PI_PROVIDER_FIELDS_INVALID`；确认该候选为 `INVALID_INPUT`             |
| 同一 `provider_id` 已有中央渠道档案                | 候选只读 `already_managed`；确认返回 `CONFLICT`，不产生第二份档案                         |
| 分次导入第二个 provider                            | 基线取并集；Apply 不得删除或改写先前导入的 provider 条目                                  |
| 手改原生 `models` 后点「按原生内容接管」           | 档案改为文件内容 + 基线刷新；下一次预览不再冲突，Apply 不回写用户手改内容                 |
| 预览绑定的渠道行版本缺失                           | `INVALID_INPUT`；不改档案、不改基线                                                       |
| 预览绑定的渠道行版本已过期                         | `STALE_PREVIEW`；不改档案、不改基线                                                       |
| 原生条目字段校验失败（缺 `apiKey` 等）             | `INVALID_INPUT`；不做部分接管                                                             |
| 没有任何漂移渠道                                   | 成功但 `adopted` 为空；不改任何行                                                         |
| 任意 Hook / Agent 入口携带 `tool == pi`            | `INVALID_INPUT` + `PI_HOOKS_UNSUPPORTED` / `PI_AGENTS_UNSUPPORTED`，零写入                |
| `managed_targets` 写入 `pi` + `hook`/`agent`       | 数据库 CHECK 拒绝                                                                         |

### 5. Good / Base / Bad Cases

- Good：`models.json` 已有非受管 `other` provider 与未知顶层键；Apply 后只新增/更新受管条目，
  未知键与 `other` 逐字节保留，Restore 回到 Apply 前文件。
- Base：适配器未安装时 MCP 页面只显示诊断与安装指引（`pi install npm:pi-mcp-adapter`），
  不创建 `mcp.json`、不生成 assignment 行。
- Bad：把 Pi 的 MCP 写入复用 Claude 的 `type: stdio` 写法；把 `<pi_agent_dir>/settings.json`
  或 `trust.json` 当受管目标或写回探测结果；用 `AGENTS.override.md` 场景下的 warning 代替硬阻断；把 `~/.pi/agents` 当作
  Pi 的 Agents 合同。

### 6. Tests Required

- Adapter：6 个 descriptor 的 scope/path/format/selector/trust/allowed_root 矩阵；无 Hook/Agent
  descriptor；适配器三态与版本过低；显式覆盖“全局 Ready + 项目 Filtered”和“全局版本过低 +
  项目 Filtered”，分别断言 Ready 与 NotLoaded；`exclusive` 模式；遮蔽与断链自检。
- Provider codec：多候选枚举（含无 `defaultProvider`、条目非对象、provider id 非法）、
  `models` 按 id 合并四象限（原条目逐字段保留 / 追加默认模型 / 无默认模型保留 / 无原数组回退）、
  条目级合并保留未知字段与非受管 provider、不写 `models: []`、
  `$ENV`/`!command` 原样保留、明文 apiKey 诊断与脱敏、`merge_import_baseline` 并集语义。
- Provider 服务（Pi 全链路）：两候选检测 → 批量导入 → Preview → Apply 后逐模型元数据与 `api`
  保留、未受管 provider 逐字节不变；只导一个后再导第二个的增量导入与基线并集；重复导入冲突；
  不完整条目只作废自身；`provider_dto` 的只读摘要不含凭据。
- 数据库：迁移 `0026` 的 `provider_import_previews` 表结构与 tool 白名单（`cursor` 拒绝、
  `pi` 接受），以及批量接管的单事务原子性（第二条失败时零档案、零基线、预览仍未消费）。
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

## Scenario: Pi Provider 多条目枚举、models 保真与批量接管

### 1. Scope / Trigger

- Trigger：改动 Pi 的 `models.json` Provider 检测、候选状态、导入确认、`models` 渲染、
  受管基线并集、渠道档案只读摘要，或新增 `PI_PROVIDER_*` 诊断码。
- 覆盖 `src-tauri/src/adapters/pi/mod.rs`、`src-tauri/src/adapters/discovery.rs`（`ProviderCodec`）、
  `src-tauri/src/profiles/{provider_discovery,service_orchestration,provider,sync,models,helpers}.rs`、
  `src-tauri/src/db/provider_imports.rs`、`src-tauri/src/db/migrations/0026_*.sql`、
  `src-tauri/src/commands/profiles.rs` 与 `src/features/tool-profiles/*`。Provider 检测已迁出
  `profile_import_previews`：该表仅保留 Prompt 单预览语义。

### 2. Signatures

- `ProviderCodec::discover(&self, descriptor, managed_projection, full_hash)
  -> Result<Vec<ProviderCodecDiscovery>, AppError>`。单 provider 工具最多返回一个元素；
  Pi 返回 `providers` 下全部条目。
- `ProviderCodecDiscovery` 携带 `provider_id`、`suggested_name`（回退 provider id）、
  `is_default_provider`、`unimportable_reason`（适配层已判定结构不可证明时的稳定原因码）。
- `ProviderCodec::merge_import_baseline(&self, existing: Option<&Value>, batch: &Value)
  -> Result<Value, AppError>`，默认实现为**替换**（单 provider 工具的首次接管语义）；
  Pi 覆写为 `providers` 条目级深合并。
- `discover_provider_import(database, environment, redactor, tool)
  -> Result<ProviderImportPreviewDto, AppError>`（非 `Option`；`previewId` 可空）。
- `confirm_provider_import(database, environment, redactor,
  ConfirmProviderImportInput { preview_id, items: Vec<ConfirmProviderImportItem { candidate_id, name }> })
  -> Result<ProviderImportResultDto, AppError>`。
- 迁移 `0026_provider_import_previews.sql`：`id`/`tool`/`target_path`/`observed_full_hash`/
  `context_json`/`redacted_preview_json`/`status`/时间戳 + `(status, created_at)` 索引；
  tool 白名单 `claude|codex|zcode|opencode|pi`（Cursor 无 Provider 合同）。
- `adopt_provider_native(database, environment, redactor, AdoptProviderNativeInput {
  tool, target_path, row_versions }) -> AdoptProviderNativeResultDto { tool, adopted }`。
- 生成绑定：`commands.discoverProviderImport(tool)`、
  `commands.confirmProviderImport({ previewId, items })`、
  `commands.adoptProviderNative({ tool, targetPath, rowVersions })`。

### 3. Contracts

- **候选枚举**：`providers` 的每个条目都是独立候选，顺序与原生顺序一致；
  `settings.json.defaultProvider` 只写 `is_default_provider`，缺失或指向不存在的条目时
  仍然返回全部候选（不得再 fail closed 整份文件）。
- **候选状态**：`importable` / `already_managed` / `invalid`。只有 `importable` 可勾选；
  `already_managed` 由「该工具已有中央渠道档案的 `config_json.provider_id` 命中」判定。
- **增量导入**：Pi 允许在已有中央档案时继续检测（按 `provider_id` 逐候选去重）；
  其他工具保留「该工具尚无中央档案」的首次接管守卫。确认同一 `provider_id` 必须
  `CONFLICT`，不得产生第二份档案。
- **候选身份**：`context_json` 只保存 `{version, candidates:[{candidateId, providerId,
  suggestedName}]}`，不含投影或凭据；`providerId` 允许为 `null`（Claude / Codex 官方登录
  没有原生 key）。确认时重新扫描原生文件，用 `providerId` 与证据求交，缺失即 `STALE_PREVIEW`。
  前端只回传不透明 `candidateId` 与用户确认的名称。
- **`models` 按 id 合并**：档案的 `extra_provider_fields` 必须包含 `models` 原数组；渲染时
  逐字段保留原条目，仅当默认模型 id 不在数组中时追加 `{ "id": <默认模型> }`；没有原数组时
  退回 `[{ "id": … }]`；两者皆无时不写 `models`（绝不写 `models: []`）。
- **目标级 desired 必须并集**：Pi 的 `models.json` 承载多个 provider，同步意图
  （`provider_sync_intent`）取**全部**中央 Pi 渠道投影的并集；只写当前生效档案会让
  未生效渠道的原生条目被删空。非 Pi 工具仍只取当前生效档案。
- **基线并集**：批量接管写入的 `managed_targets.baseline_projection_json` 是
  `merge_import_baseline(既有基线, 本批次并集)`；未导入的 provider 永不入基线，
  因此不会被当成「受管但缺失」删除。
- **原子性**：`db::provider_imports::adopt_imported_providers` 在单个 `IMMEDIATE` 事务内
  校验预览行与 `context_json`、拒绝活动 writer、插入 N 份档案、写并集基线、条件消费预览；
  任一失败整体回滚且预览保持 `previewed`。取到写锁后重新扫描原生文件再提交。
- **只读摘要**：`ProviderProfileDto.pi` 仅含 `apiFormat` 与 `models[]` 的 `id`/`name`；
  绝不暴露 `apiKey`、`headers`、`modelOverrides` 的值。
- **按原生内容接管**（`adopt_provider_native`）：只处理**已漂移**的渠道（档案投影 ≠ 原生
  条目），按原生文件内容改写档案（`apiKey` 原样采纳：明文入库、`$ENV` 保持引用），并在同一
  `IMMEDIATE` 事务里把目标基线刷新为全部中央渠道投影的并集。与
  `readopt_provider_target` 的区别：后者只刷新基线、档案保持旧内容，因此下一次 Apply 会把
  用户手改的原生内容改回去——接管必须同时改档案，否则用户点完仍会被回写。
  必须按用户所看预览绑定的行版本（`DatabaseEntityType::ProviderProfile`）做乐观校验：缺条目
  报 `INVALID_INPUT`、已过期报 `STALE_PREVIEW`，不得静默覆盖其他窗口的档案编辑。
  配对规则：档案有 `provider_id` 时按 id 匹配；没有稳定原生 key 的 codec（Claude）只在
  唯一配对时匹配。本操作不写原生文件。

### 4. Validation & Error Matrix

| 条件                                            | 必须结果                                                                      |
| ----------------------------------------------- | ----------------------------------------------------------------------------- |
| `providers` 条目非对象                          | 该候选 `invalid` + `PI_PROVIDER_ENTRY_INVALID`，其余候选照常                  |
| provider id 非法（空/控制字符/分隔符/超长）     | 该候选 `invalid` + `PI_PROVIDER_ID_INVALID`，不阻断同文件其他 provider        |
| 缺 `apiKey`、`baseUrl` 非法或选项不受支持       | 该候选 `invalid` + `PI_PROVIDER_FIELDS_INVALID`；确认该候选为 `INVALID_INPUT` |
| 无 `defaultProvider` / 指向不存在的条目         | 仍返回全部候选，`is_default_provider` 全为 false                              |
| `provider_id` 已有中央档案                      | 候选只读 `already_managed`；确认 `CONFLICT`                                   |
| 候选进入更新状态（原生文件变化 / 中央档案变化） | `STALE_PREVIEW`，零档案、零基线、预览未消费                                   |
| 批次内名称重复或 NOCASE 冲突                    | `CONFLICT`（同一事务回滚）                                                    |
| 活动 apply/restore 或 rollback_failed writer    | `WRITE_IN_PROGRESS`，预览保持未消费                                           |
| 第二条档案插入失败                              | 零档案、零基线、预览仍 `previewed`                                            |
| 分次导入第二个 provider                         | 基线取并集；Apply 保留先前 provider 条目                                      |
| `managed_targets` 写入 `pi` + `hook`/`agent`    | 数据库 CHECK 拒绝                                                             |

### 5. Good / Base / Bad Cases

- Good：`models.json` 含 `cc` 与 `gemini`，`defaultProvider=cc`；检测返回 2 个可导入候选，
  全选导入生成 2 份档案，Apply 后 `cc.models[0]` 的 `contextWindow`/`reasoning`/`cost`/
  `thinkingLevelMap` 与 `api` 逐字段保留，`gemini` 逐字节不变。
- Base：只导入 `cc`；再次检测时 `cc` 显示 `already_managed`、`gemini` 仍可导入，
  导入后基线含两者且 Apply 不删除 `cc`。
- Bad：只取 `defaultProvider` 那一条（旧行为，静默丢 provider）；把 `models` 整段替换成
  `[{ "id": 默认模型 }]`；只写当前生效档案的 desired；把 providerId 从 `redacted_preview_json`
  反推；把 Pi 的多条目标记当作「同一目标多行基线」。

### 6. Tests Required

- 适配器（`adapters/pi/tests.rs`）：多候选枚举、无 `defaultProvider`、空 `providers`、
  非法条目降级、`api`/`models`/未知键进入 `extra_provider_fields`、`models` 合并四象限、
  `merge_import_baseline` 并集与同名覆盖。
- 服务（`profiles/tests.rs`）：Pi 两候选全选导入 → Preview → Apply 的逐字段保真与
  未受管 provider 不变；增量导入与基线并集；重复导入冲突与预览消费；
  不完整条目只作废自身；无 `defaultProvider` 仍列出全部候选。
- 数据库（`db/tests.rs`）：`0026` 表结构（9 列 + 状态索引）、tool 白名单（`cursor` 拒绝、
  `pi` 接受）、批量接管原子性（第二条失败时零档案 / 零基线 / 预览未消费）与并集基线落库。
- 前端（`tool-profiles-page.test.tsx`、`onboarding-wizard.test.tsx`）：多候选勾选与名称编辑的
  精确 payload、`already_managed`/`invalid` 不可勾选且显示原因、只读摘要文案、
  首次接管批量导入、检测失败/无候选时不留残留预览。
- 按原生内容接管：手改原生文件后接管 → 档案改按文件（只读摘要可见）、基线刷新、
  下一次 Apply 不回写；过期行版本 → `STALE_PREVIEW`；无漂移 → 空结果。前端断言同步 Preview
  冲突态同时提供「重新接管」（只刷基线）与「按原生内容接管」，后者带 `rowVersions` 调用
  且不触发 Apply。

### 7. Wrong vs Correct

#### Wrong

```rust
// 旧行为：只认 defaultProvider，其余 provider 静默丢失；models 被整段替换。
let Some((provider_id, entry)) = providers
    .iter()
    .find(|(id, _)| Some(id.as_str()) == default_provider.as_deref())
else {
    return Ok(Vec::new());
};
set_or_remove(
    &mut entry,
    "models",
    default_model.map(|model| json!([{ "id": model }])),
);
```

#### Correct

```rust
// 1) 每个 provider 条目都是候选，坏条目只作废自己；
// 2) models 按 id 合并，原条目逐字段保留；
// 3) 多个已导入 provider 的 desired 与基线都取并集。
for (provider_id, value) in providers {
    if validate_provider_id(provider_id).is_err() {
        discoveries.push(unimportable_discovery(/* … */ PI_PROVIDER_ID_INVALID));
        continue;
    }
    /* … extra_provider_fields 包含 models 原数组 … */
}
```
