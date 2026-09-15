# 执行计划：Pi 渠道导入保真与批量接管

三个阶段按顺序推进；每个阶段结束都有可独立执行的门禁，**阶段 1 的适配器改动不依赖阶段 2**，
可以单独验证与回滚。

## 阶段 0：准备

- [ ] 重读并遵守 `.trellis/spec/backend/pi-adapter-guidelines.md`、
      `mcp-import-guidelines.md`、`database-guidelines.md`、`frontend/*`。
- [ ] 记录基线：`cargo test --manifest-path src-tauri/Cargo.toml` 与
      `pnpm test --run` 在改动前全绿（用于区分既有失败与本次引入的失败）。

## 阶段 1：适配器保真（不触库、不触前端）

### 1.1 `ProviderCodec::discover` 返回候选列表

- [ ] `src-tauri/src/adapters/discovery.rs`：`discover` 返回值改为
      `Result<Vec<ProviderCodecDiscovery>, AppError>`，并新增带默认实现的
      `merge_import_baseline(existing, batch)`。
- [ ] `claude/mod.rs`、`codex/mod.rs`、`zcode/mod.rs`、`opencode/mod.rs` 与 Cursor 的 codec：
      把原实现包成 `Vec`，字段与判定逐字不变。
- [ ] `profiles/provider_discovery.rs`：`discover_native_provider` →
      `discover_native_providers`，调用点（`service_orchestration.rs` 的
      `discover_provider_import` / `confirm_provider_import`）暂时保持「取 defaultProvider 或
      `len()==1` 的那一个」的旧语义（用临时适配函数），保证阶段 1 结束时全仓可编译、行为不变。

### 1.2 Pi 多候选枚举

- [ ] `pi/mod.rs`：遍历 `providers` 全部条目，产出独立 `ProviderCodecDiscovery`；
      `extra_provider_fields` 不再排除 `models`；`suggested_name` 回退到 provider id。
- [ ] `pi/mod.rs`：新增诊断码常量 `PI_PROVIDER_ENTRY_INVALID`、`PI_PROVIDER_ID_INVALID`，
      非法条目跳过而不是整体失败。
- [ ] `settings.json.defaultProvider` 只读作「默认渠道」标注，不再参与候选取舍。

### 1.3 Pi `render` 的 models 合并

- [ ] 按 design §3.3 的四象限实现；「有原数组 + 默认模型已在数组中」必须逐字节保留原条目
      （含 `contextWindow`/`cost`/`thinkingLevelMap`）。
- [ ] 确认仍不写 `models: []`、不展开 `$ENV`/`!command`。

### 1.4 基线并集钩子

- [ ] Pi 覆写 `merge_import_baseline`：`providers` 条目级深合并，其余顶层键保留。

### 1.5 阶段 1 测试与门禁

- [ ] `src-tauri/src/adapters/pi/tests.rs` 新增：- 多候选（含 `defaultProvider` 不存在、无 `defaultProvider`、非法 provider id、非对象条目）- `api`/`name`/`models`/未知键进入 `extra_provider_fields` - `models` 合并四象限 + `$ENV` apiKey 原样保留 - `merge_import_baseline` 并集与「未知 provider 不入库」语义
- [ ] 复跑既有 Claude/Codex/ZCode/OpenCode provider 测试确认零回归。
- [ ] 门禁：`cargo fmt --manifest-path src-tauri/Cargo.toml --check`、
      `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`、
      `cargo test --manifest-path src-tauri/Cargo.toml`。
- [ ] **检查点 / 回滚点**：阶段 1 可整体 `git checkout` 回退，无数据库副作用。

## 阶段 2：导入生命周期（库 + 服务 + 命令）

### 2.1 新表与迁移

- [ ] 新建 `src-tauri/src/db/migrations/0026_provider_import_previews.sql`（design §3.5 的 DDL +
      索引）。
- [ ] `src-tauri/src/db/mod.rs`：注册 `version: 26`；加锚点自检（表存在、tool CHECK 命中一次、
      两条 JSON CHECK 命中一次）。
- [ ] `src-tauri/src/db/tests.rs`：`schema_version() == 25` 机械替换为 `26`；表清单断言加入
      `provider_import_previews`；补一条「升级后旧 `profile_import_previews` 行仍可读」的断言。
- [ ] 新建 `src-tauri/src/db/provider_imports.rs`：`ProviderImportPreviewRecord`、
      `persist_preview`、`get_preview`、`adopt_imported_providers`（单事务：校验预览行 →
      名称唯一校验 → 插入 N 份档案 → 基线 upsert → 条件消费预览）。
- [ ] `src-tauri/src/db/mod.rs` 挂载 `mod provider_imports;`。

### 2.2 DTO 与服务编排

- [ ] `profiles/models.rs`：新增 `ProviderImportCandidateDto` / 重写
      `ProviderImportPreviewDto` / `ConfirmProviderImportInput` / `ConfirmProviderImportItem` /
      `ProviderImportResultDto` / `PiProviderSummaryDto` / `PiProviderModelDto`；
      `ProviderProfileDto` 加 `pi: Option<PiProviderSummaryDto>`；移除旧的 `ConfirmImportInput`
      的 Provider 用法（Prompt 导入继续用它）。
- [ ] `profiles/provider_discovery.rs`：`discover_provider_candidates`（per-candidate 守卫、
      `importable|already_managed|invalid` 判定、原因码透传、只读 `api_format`/`model_count`）。
- [ ] `profiles/service_orchestration.rs`：重写 `discover_provider_import` 与
      `confirm_provider_import`（design §3.6 的 9 步），复用 `descriptor_for`、`scan_target`、
      `validate_provider_fields`、`StoredProviderConfig::from_input`、
      `codec.merge_import_baseline`、`hash_json`。
- [ ] `profiles/helpers.rs::provider_dto` 投影 Pi 只读摘要；`profiles/provider.rs` 不变。
- [ ] `profiles/sync.rs` / `db/profiles.rs` 中与旧单预览 Provider 导入相关的死代码清理
      （保留 Prompt 导入路径）。

### 2.3 命令与绑定

- [ ] `src-tauri/src/commands/profiles.rs`：两个命令签名按 design §3.7 更新。
- [ ] `pnpm bindings:generate`，确认 `src/bindings/commands.ts` 只出现预期的类型变化。

### 2.4 阶段 2 测试与门禁

- [ ] `profiles/tests.rs`（Pi 全链路）：- 检测 `cc`+`gemini` → 2 候选；全选导入 → 2 档案 → Preview → Apply →
      `cc.models` 元数据与 `api` 逐字段保留、`gemini` 逐字节不变（AC1/AC3）- 只导 `cc` → 再检测 `gemini` 可导入 → 导入后基线并集、档案数 2（AC4）- 重复导入同一 provider → 冲突、档案数不变（AC5）- 无 `apiKey` 条目 → `invalid` + 原因码，同文件其他候选仍可导入（AC6）- 无 `defaultProvider` + 2 条 → 2 候选（AC2）- 预览后原生文件被改 → `stale_preview`，档案数为 0
- [ ] `db/tests.rs`：迁移 26、单事务原子性（第二条档案插入失败时不留半成品、不消费预览）。
- [ ] 门禁：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、
      `cargo test --manifest-path src-tauri/Cargo.toml`、`pnpm bindings:check`。
- [ ] **检查点 / 回滚点**：阶段 2 的数据库变更为「新增空表」，回退代码后旧版本仍可打开数据库
      （design §5 已说明并需实测一次）。

## 阶段 3：前端与收口

### 3.1 渠道面板

- [ ] 新建 `src/features/tool-profiles/provider-import-dialog.tsx`（候选复选 + 名称输入 +
      状态文案 + 错误回显 + 焦点管理，参照 `mcp-import-dialog.tsx`）。
- [ ] `provider-panel.tsx`：`检测已有配置` → 有候选打开对话框，无候选沿用现有提示；
      卡片增加「API 格式 · N 个模型」只读摘要。
- [ ] `provider-text.ts`：候选/摘要文案 helper。

### 3.2 首次接管引导

- [ ] `onboarding-wizard.tsx`：provider 开关打开时批量确认全部 `importable` 候选。

### 3.3 前端测试与门禁

- [ ] `provider-panel` / `tool-profiles-page.test.tsx` /
      `tool-profiles-page-accessibility.test.tsx`：更新 `discoverProviderImport` mock 到新结构，
      新增「多候选多选导入」「无可导入项」「名称冲突提示」「只读摘要」用例。
- [ ] `onboarding-wizard.test.tsx`：更新 mock 与批量确认断言。
- [ ] 门禁：`pnpm format:check`、`pnpm lint`、`pnpm typecheck`、`pnpm test --run`。

## 阶段 4：整体验收

- [ ] 全量 `pnpm check`。
- [ ] 用真实样例（`cc` + `gemini` 两条 provider 的 `models.json` 副本，放在临时目录并以
      `PI_CODING_AGENT_DIR` 映射）走一遍「检测 → 多选导入 → Preview → Apply」，人工比对
      Apply 前后 `models.json` 的 diff 只包含预期字段。
- [ ] 更新 `.trellis/spec/backend/pi-adapter-guidelines.md`：补「Provider 候选枚举、models 按 id
      合并、增量导入与基线并集」合同，以及新的诊断码与 Validation Matrix 行。
- [ ] 若引入新的导入表形态，同步在 spec 中登记 `provider_import_previews` 与 0026 迁移。
- [ ] 记录 journal，提交（由 `trellis-finish-work` 流程执行）。

## 依赖与顺序

- 阶段 2 依赖阶段 1 的 `discover -> Vec` 与 `extra_provider_fields` 含 `models`；不可并行。
- 阶段 3 依赖阶段 2 生成的绑定（`src/bindings/commands.ts`）；不可先行。
- 阶段 4 只在阶段 1–3 全绿后执行。
