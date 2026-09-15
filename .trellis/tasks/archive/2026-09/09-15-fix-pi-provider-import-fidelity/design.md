# 技术设计：Pi 渠道导入保真与批量接管

## 1. 现状与根因（已实测复现）

| 环节     | 代码                                                         | 现状                                                                                                  |
| -------- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| 候选选择 | `src-tauri/src/adapters/pi/mod.rs` `ProviderCodec::discover` | 只取 `settings.json.defaultProvider` 对应条目；无默认值且 `providers.len() != 1` 时返回 `None`        |
| 档案字段 | 同上 `extra_provider_fields` 过滤 `baseUrl/apiKey/models`    | `api`、`headers`、`modelOverrides` 等保留，`models` 被丢弃                                            |
| 写入     | 同上 `render` + `ManagedOwnership`                           | `providers/<id>/models` 被 desired 的 `[{id}]` 整段替换；逐模型元数据丢失                             |
| 导入守卫 | `src-tauri/src/db/profiles.rs` `reject_existing_profiles`    | 「首次导入仅在该工具尚无中央档案时可确认」——多 provider 分批导入被硬阻断                              |
| 目标基线 | 同上 `adopt_baseline`                                        | 每个 `(tool, artifact_kind, scope, project_id, target_path)` 只有一行，第二次导入直接覆盖第一次的投影 |
| 界面     | `src/features/tool-profiles/provider-panel.tsx`              | 单个预览卡片；`ProviderProfileDto` 不含 `api`/`models`，用户看不到保真结果                            |

结论：需要同时修 **候选枚举**、**models 合并写入**、**多档案共存与基线并集** 三件事；其中后两者
是同一份数据流，不能拆成互不依赖的子任务（拆开会在中途产生编译不通过、无法独立验收的状态），
因此本任务不建子任务，用 `implement.md` 的两个阶段顺序推进。

## 2. 边界

- 只影响 Provider 导入链路的 Pi 分支；Claude/Codex/ZCode/OpenCode 的候选枚举、守卫、DTO 语义不变
  （它们仍然最多 1 个候选，仍然只在无中央档案时可确认）。
- 不新增 Pi 的 Hook/Agent 能力，不改 `managed_targets` 的 artifact 白名单。
- 只读探测边界不变：不写 `settings.json`/`trust.json`，不执行 `pi`，不展开 `$ENV`/`!command`。

## 3. 合同变更

### 3.1 `ProviderCodec::discover` 返回候选列表

`src-tauri/src/adapters/discovery.rs`

```rust
fn discover(
    &self,
    descriptor: &TargetDescriptor,
    managed_projection: &Value,
    full_hash: &str,
) -> Result<Vec<ProviderCodecDiscovery>, AppError>;   // was Option<...>
```

- Claude/Codex/ZCode/OpenCode/OpenCursor：把现有实现包成 `Vec`（`Ok(vec![d])` / `Ok(Vec::new())`），
  逐字保持现有字段与行为。
- Pi：遍历 `providers` 全部条目（保持文件顺序，`serde_json` 关闭 `preserve_order` 故为字典序 ——
  与既有 `BTreeMap` 语义一致），每条产出独立的 `ProviderCodecDiscovery`：
  - `projection = { "providers": { <id>: <entry 原样> } }`（沿用现有目标级 `full_hash`）
  - `extra_provider_fields` = 除 `baseUrl`/`apiKey` 外的全部键（**含 `models`**），保证 `api`/`name`/
    `headers`/`modelOverrides`/未知键逐字保留
  - `provider_id` 非法（`validate_provider_id` 失败）或条目非对象 → 该条目标记为
    `invalid`（原因码 `PI_PROVIDER_ID_INVALID` / `PI_PROVIDER_ENTRY_INVALID`），不阻断其他条目
  - `default_model`：`settings.json.defaultModel` 的 `<provider>/<model>` 前缀匹配该 provider，
    否则回退 `models[0].id`（现有 `resolve_default_model` 逐字保留）
  - `suggested_name`：`entry.name` → provider id → 无（由服务层兜底 `已导入渠道`）
- `defaultProvider` 只用于给候选标注「默认渠道」，不再影响是否返回候选。

服务层 `src-tauri/src/profiles/provider_discovery.rs`：
`discover_native_provider` 改为 `discover_native_providers -> Result<Vec<DiscoveredProvider>>`，
并在其中对每个候选执行现有 `validate_provider_fields` + `validate_discovered_provider_config`
的**可导入性判定**，产出 `importable | invalid`（原因码透传，不回显凭据）。

### 3.2 目标级基线并集（新增 codec 钩子）

`ProviderCodec` 新增：

```rust
/// 把本批次导入的候选投影并入目标级受管基线。默认替换（单渠道语义）。
fn merge_import_baseline(
    &self,
    existing: Option<&Value>,
    batch: &Value,
) -> Result<Value, AppError> {
    let _ = existing;
    Ok(batch.clone())
}
```

Pi 覆写：对 `providers` 做条目级深合并（同名条目以 `batch` 为准），其余顶层键取 `existing`，
`batch` 中的未知顶层键并入。这样：

- 首导 `cc` 时基线 = `{providers:{cc}}`；
- 再导 `gemini` 时基线 = `{providers:{cc, gemini}}`，不会覆盖第一次；
- 未被导入的 provider 永不进入基线，因此不会被 Apply 当作「受管但缺失」删除。

默认实现为替换，保住 Claude/Codex/ZCode/OpenCode 的既有单渠道语义。

### 3.3 Pi `render` 的 models 合并规则

`src-tauri/src/adapters/pi/mod.rs::render`，`apiKey`/`baseUrl` 逻辑不变，`models` 改为：

| `extra_provider_fields.models` | `default_model` | 渲染结果                                                                 |
| ------------------------------ | --------------- | ------------------------------------------------------------------------ |
| 数组（含导入原值）             | 非空            | 逐字节保留原数组；仅当其中没有该 `id` 时追加 `{ "id": <default_model> }` |
| 数组                           | 空              | 原样保留（不裁剪、不排序）                                               |
| 无                             | 非空            | `[{ "id": <default_model> }]`（既有行为）                                |
| 无                             | 空              | 不写 `models`（绝不写 `models: []`）                                     |

未知键、`api`、`name`、`headers`、`modelOverrides` 继续由 `extra_provider_fields` 原样落盘。

### 3.4 DTO

`src-tauri/src/profiles/models.rs`

```rust
#[serde(rename_all = "camelCase")]
pub struct ProviderImportCandidateDto {
    pub candidate_id: String,             // 不透明 UUID；前端只回传它
    pub provider_id: String,
    pub suggested_name: String,
    pub status: ProviderImportCandidateStatus, // importable | already_managed | invalid
    pub reason: Option<String>,           // 稳定诊断码（PI_*）
    pub api_base_url: String,
    pub api_key_configured: bool,
    pub default_model: String,
    pub api_format: Option<String>,       // 只读展示
    pub model_count: u32,                 // 只读展示
    pub redacted_projection: Value,
}

pub struct ProviderImportPreviewDto {
    pub preview_id: Option<String>,       // 无可导入候选时为 None
    pub tool: Tool,
    pub target_path: String,
    pub default_provider: Option<String>, // 只用于「默认渠道」标记
    pub candidates: Vec<ProviderImportCandidateDto>,
    pub message: Option<String>,
}

pub struct ConfirmProviderImportInput {
    pub preview_id: String,
    pub items: Vec<ConfirmProviderImportItem>, // { candidateId, name }
}

pub struct ProviderImportResultDto {
    pub tool: Tool,
    pub imported_count: u32,
}
```

`ProviderProfileDto` 增加只读 Pi 摘要（其他工具为 `None`）：

```rust
pub struct PiProviderSummaryDto {
    pub api_format: Option<String>,
    pub models: Vec<PiProviderModelDto>, // { id, name: Option<String> }
}
```

数据来源是 `StoredProviderConfig.extra_provider_fields` 的 `api` 与 `models[].id/name`；
只投影这两个非敏感字段，**不得**把 `headers`/`apiKey` 放进 DTO。

### 3.5 新表 `provider_import_previews`（migration 0026）

沿用 MCP 导入的既定形态（`mcp_import_previews`）：一次检测一行，`context_json` 承载候选身份
证据，`redacted_preview_json` 承载展示 DTO。**不扩展现有 `profile_import_previews`**
（该表保持 Provider/Prompt 单预览语义，MCP 规范已明确「不扩展 Provider/Prompt-only 导入表」）。

```sql
CREATE TABLE provider_import_previews (
    id TEXT PRIMARY KEY CHECK(<uuid-v4-lowercase>),
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'zcode', 'opencode', 'pi')),
    target_path TEXT NOT NULL CHECK(<absolute, no //, no /../, no trailing />),
    observed_full_hash TEXT NOT NULL CHECK(<64 hex lowercase>),
    context_json TEXT NOT NULL CHECK(json_valid(context_json) AND json_type(context_json) = 'object'),
    redacted_preview_json TEXT NOT NULL CHECK(json_valid(redacted_preview_json) AND json_type(redacted_preview_json) = 'object'),
    status TEXT NOT NULL DEFAULT 'previewed' CHECK(status IN ('previewed', 'consumed')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    consumed_at TEXT
);
CREATE INDEX idx_provider_import_previews_status ON provider_import_previews(status, created_at);
```

`context_json` 只存身份：`{ "version": 1, "candidates": [ { "candidateId", "providerId",
"suggestedName" } ] }`，**不含任何投影或凭据**。确认时服务端据此还原「哪个候选」，前端只提交
`candidateId` 与用户确认的名称。

配套改动：

- `src-tauri/src/db/mod.rs`：`MIGRATIONS` 追加 `version: 26, name: "provider_import_previews"`，
  并仿照现有 `migration.version == 20` 的写法加一条锚点自检（表存在 + 含 tool CHECK + 含两条 JSON CHECK）。
- `src-tauri/src/db/tests.rs`：`schema_version() == 25` → `26`（机械替换）；表清单断言加入新表。
- 新文件 `src-tauri/src/db/provider_imports.rs`（`mcp_imports.rs` 同构）：
  `persist_preview` / `get_preview` / `adopt_imported_providers`（单事务：校验预览行 → 逐条
  校验名称唯一 → 插入 N 份 `provider_profiles` → upsert `managed_targets` 基线并集 →
  条件 `UPDATE ... status='consumed'`）。

### 3.6 服务层编排

`src-tauri/src/profiles/provider_discovery.rs`（新）

```rust
fn discover_provider_candidates(
    database: &Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Vec<ProviderImportCandidate>, AppError>;
```

- 允许 `discover` 的时机从「该工具无任何中央档案」收敛为 **per-candidate**：
  - 非 Pi：保持旧守卫（存在任一中央渠道档案 → 无候选，等价旧 `None`）。
  - Pi：无条件枚举候选，`provider_id` 命中已有 Pi 档案 `config_json.provider_id` 的候选标为
    `already_managed`（不可勾选）。
- 候选的可导入性由 `validate_provider_fields` + `StoredProviderConfig::from_input` 决定；
  不合格的标 `invalid` + 原因码，不产生预览。

`src-tauri/src/profiles/service_orchestration.rs`

```rust
pub fn discover_provider_import(database, environment, redactor, tool)
    -> Result<ProviderImportPreviewDto, AppError>;      // 非 Option；previewId 可空

pub fn confirm_provider_import(database, environment, redactor, input: ConfirmProviderImportInput)
    -> Result<ProviderImportResultDto, AppError>;
```

`confirm_provider_import` 流程：

1. `get_preview`（新表）；`status != previewed` → `preview_already_consumed`。
2. `ArtifactName::parse` 每个 `name`；`items` 非空、`candidateId` 唯一。
3. 重新扫描原生文件（`descriptor_for` + `scan_target`）；`target_path` / `full_hash` 与预览行不一致
   → `stale_preview`。
4. 按 `context_json` 的 `providerId` 与新鲜候选求交；缺失即 `stale_preview`。
5. 该 provider 已有中央档案 → `conflict("import", ...)`（不重复建档）；名称冲突同样拒绝。
6. `extra_provider_fields`（含 `models`）与 `refresh_default_model`（刷新为当前 `settings.json`
   解析结果）写进 `StoredProviderConfig`；`projection` 原样作为基线片段。
7. 基线 = `codec.merge_import_baseline(existing_baseline_projection, batch_union_projection)`，
   `managed_hash = hash_json(基线)`。
8. `db::provider_imports::adopt_imported_providers` 一个事务完成插入 + 基线 upsert + 消费预览。
9. `redactor.register_secret` 登记 `apiKey`；返回导入数量。

`profiles/provider.rs::provider_projection` 不变；`provider_dto` 增加 Pi 摘要投影。

### 3.7 命令层

`src-tauri/src/commands/profiles.rs`：`discover_provider_import` 返回值由
`Option<ProviderImportPreviewDto>` 改为 `ProviderImportPreviewDto`；`confirm_provider_import`
入参换成 `ConfirmProviderImportInput`、返回 `ProviderImportResultDto`。类型唯一来源仍是 Rust
（`pnpm bindings:generate`）。

### 3.8 前端

- 新 `src/features/tool-profiles/provider-import-dialog.tsx`：照抄 `mcp-import-dialog.tsx` 的结构
  （候选复选、状态文案、`previewId` 缺失时「没有可导入项」），额外提供**每条候选的名称输入**
  （默认 `suggestedName`），确认调用 `commands.confirmProviderImport({ previewId, items })`；
  失败保持对话框打开并展示 `profileErrorText`。
- `provider-panel.tsx`：`检测已有配置` 检测到候选即打开对话框；无候选沿用现有 notify 文案；
  渠道卡片增加只读摘要行「API 格式 · N 个模型」。
- `provider-text.ts`：新增候选/摘要文案 helper（API 格式、模型数量、模型 id 列表 title）。
- `onboarding-wizard.tsx`：`providerSupported && provider.candidates` → 勾选该工具的 provider
  时用全部 `importable` 候选与其 `suggestedName` 一次确认（保持向导「一次接管」体验）。

## 4. 关键决策与权衡

| 决策                                    | 备选                                                                       | 选择理由                                                                                                                               |
| --------------------------------------- | -------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| 新表 `provider_import_previews`         | 复用 `profile_import_previews`，把 providerId 塞进 `redacted_preview_json` | 复用会从「展示字段」还原原生 key，违反 MCP 导入规范「身份只来自持久化证据」；新表与既有 MCP/Skills 导入形态一致，迁移是纯 CREATE TABLE |
| 基线并集由 codec 钩子实现               | 在服务层 `match tool` 写 Pi 特例                                           | 钩子让「单渠道 = 替换」成为显式默认，服务层不引入工具分支                                                                              |
| Pi 允许增量导入（per-provider_id 去重） | 保持「无中央档案才可确认」                                                 | 多选导入天然可能只选一部分；不允许增量会让未导入的 provider 永久不可导入                                                               |
| 候选默认名用 provider id                | 统一 `已导入渠道`                                                          | provider id 在同一文件内唯一，避免批量导入时的名称冲突；`ArtifactName` 只要求非空/无控制字符                                           |
| 对话框内可编辑名称                      | 只用 `suggestedName`                                                       | 冲突时可自解，不必回退重来                                                                                                             |
| `models` 归受管并集                     | 完全不管 `models`                                                          | 用户明确要求按 id 合并保留元数据；完全不接管则默认模型无法随档案更新                                                                   |

## 5. 兼容、迁移与回滚

- 旧 `profile_import_previews` 里遗留的 `artifact_kind='provider'` 行在升级后不再被任何代码读取
  （新流程走新表），不删数据、不阻断启动。
- 已存在的 Pi 渠道档案（`extra_provider_fields` 无 `models`）继续可用：`render` 落入「无 models +
  有默认模型」行，行为与今天一致；用户重新检测导入即可获得保真档案。
- 回滚：迁移应用循环只补齐 `schema_migrations` 中缺失的版本、不校验更高版本，因此代码回退后
  旧版本仍可打开含 0026 记录的库（多出的空表不参与读写）。回滚不需要恢复备份；实现阶段用一次
  真实的「升到 26 → 回退断言」验证该结论。

## 6. 风险与缓解

| 风险                                                                                | 缓解                                                                                            |
| ----------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Pi 的 `models` 变成受管字段后，用户在 `models.json` 手改模型会被 Preview 显示为漂移 | Preview 会显示差异并由用户确认；`merge_import_baseline` 只在导入时并集，不影响 Apply 的常规合并 |
| 删除某个 Pi 渠道档案后基线仍含该 provider，Apply 会移除原生条目                     | 与既有单渠道语义一致（现状同样如此）；作为已知语义写进测试断言，不做行为变更                    |
| 新增 `models` 使 `config_json` 变大                                                 | 单条目 JSON，受原生文件大小与既有 row 限制约束；不做额外压缩                                    |
| 20+ 处 `schema_version == 25` 断言                                                  | 机械替换为 26，`cargo test` 全量校验                                                            |
| 前端测试大量 mock `discoverProviderImport` 旧结构                                   | 同步更新 `tool-profiles-page.test.tsx` / `onboarding-wizard.test.tsx` 的 mock 与断言            |

## 7. 验证策略

- 适配器：`src-tauri/src/adapters/pi/tests.rs` 新增多候选（含无 `defaultProvider`、非法条目、
  `$ENV` apiKey）、`render` models 合并四象限。
- 服务：`src-tauri/src/profiles/tests.rs` 新增 Pi 导入端到端（检测 2 候选 → 全选导入 →
  Preview → Apply → 断言 `models` 元数据与 `api` 逐字段保留、另一 provider 逐字节不变；
  只导 1 个后再导第二个 → 基线并集 + 档案数 2；重复导入 → 冲突）。
- 数据库：`src-tauri/src/db/tests.rs` 迁移 26、表结构、单事务原子性（插入失败不消费预览）。
- 前端：渠道面板对话框（多选、名称编辑、错误回显）、onboarding 批量导入、只读摘要渲染。
- 门禁：`pnpm check`（`format:check` / `lint` / `typecheck` / `vitest --run` /
  `rust:check` = fmt + clippy + test / `bindings:check`）。
