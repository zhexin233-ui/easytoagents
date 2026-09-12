# 技术设计：项目级 Hooks 只读纳入项目原生资源

前置阅读：`research/root-cause.md`、
`.trellis/spec/backend/quality-guidelines.md`（Explicit discovery and read-only preview）、
`.trellis/spec/backend/database-guidelines.md`（Migration strategy for future schema changes）。

## 1. 边界

- 只读识别：Hook 条目只有 `active` / `missing` 两种状态；不进入禁用/恢复流程。
- 复用而不改语义：原生 hooks 的拍平、外部键与哈希规则全部复用
  `src-tauri/src/hooks/service_core.rs` 既有实现，只把可见性从
  `pub(super)` 提升到 `pub(crate)`。
- 不改中央 Hooks 同步/导入/接管的任何行为。

## 2. 数据模型

### 2.1 `ProjectNativeEntryType` 新增 `HookEntry`

`src-tauri/src/projects/models.rs`：

```text
McpEntry -> "mcp_entry" | Directory -> "directory" | Symlink -> "symlink"
HookEntry -> "hook_entry"   (新增)
```

前端 bindings（`src/bindings/commands.ts` 由 specta 生成）随之得到
`"hook_entry"`。`sync::NativeResourceEntryType`（禁用/恢复证据）**不加**
`HookEntry`：Hook 永远不会生成动作证据（见 §5）。

### 2.2 迁移 `0021_project_native_hook_entries.sql`

`project_native_resources` 没有入向外键，按规范 12 步表重建：

1. 前置校验：`SELECT` 旧表 CHECK 文本包含
   `entry_type IN ('mcp_entry', 'directory', 'symlink')`，否则 `RAISE(ABORT)`。
2. `CREATE TABLE project_native_resources_new(...)`：完整复制 0012 的列、CHECK、
   UNIQUE，仅把 `entry_type` CHECK 改为
   `IN ('mcp_entry', 'directory', 'symlink', 'hook_entry')`。
3. `INSERT INTO ..._new (显式列清单) SELECT 显式列清单 FROM project_native_resources`。
4. 校验行数相等（`SELECT RAISE(ABORT, ...)` 形式）。
5. 删除旧表上的两个 row_version 触发器与 `snapshots` 上的
   `trg_snapshots_reject_native_resource_id_update`。
6. `DROP TABLE project_native_resources`；`ALTER TABLE ..._new RENAME TO project_native_resources`。
7. 重建三个索引、两个 row_version 触发器、`snapshots` 交叉保护触发器
   （SQL 与 0012 逐字一致，只是表已含新 CHECK）。
8. `PRAGMA foreign_key_check`（结果非空则 ABORT）、`PRAGMA integrity_check` 要求 `ok`。

在 `src-tauri/src/db/mod.rs` 的 `MIGRATIONS` 末尾登记。迁移测试用 `tempfile`
根：先跑到 0020 并插入三种旧类型行（可直接用完整 `Database::open` 后回退无法
模拟时，改为：打开新库 → 插入旧类型与 `hook_entry` 行均成功、`prompt_file`
被拒绝 → 重开幂等）。若需要"旧库带数据迁移"的证据，参考
`db/tests.rs` 中 0018 的迁移测试写法（先用旧版本 SQL 建库再迁移）。

## 3. 后端观测流程

文件：`src-tauri/src/projects/native_resources.rs`。

### 3.1 描述符纳入

`supported_project_descriptors` 过滤条件扩为
`ArtifactKind::Mcp | ArtifactKind::Skill | ArtifactKind::Hook`。
（OpenCode 适配器不声明 Hook 描述符，天然排除。）

### 3.2 `observe_hook_items`

```text
observe_hook_items(database, adapter, descriptor, target_id)
  -> Result<Option<Vec<ObservedNativeItem>>>
```

- `scan = scan_target(adapter, descriptor, &hooks::build_hook_ownership(tool))`。
- `Missing` → `Some(vec![])`；`Observed` → 继续；其他（ParseError /
  PermissionDenied / TargetTypeChanged …）→ `None`（该目标本轮不对账，与
  MCP 一致）。
- `entries = hooks::native_entries(&observed, hooks::events_root(tool))`
  得到 `外部键 -> 条目 Value`。
- `managed_hashes = db::hooks::list_managed_hook_items(database, target_id)`
  的 `last_applied_item_hash` 集合。
- 每条：`item_hash = hash_json(entry)`；`centrally_owned = managed_hashes.contains(item_hash)`；
  `entry_type = HookEntry`；`external_key` 即拍平键
  `<原生事件>|<matcher>|<哈希前 16 位>`。

`observe_items` 的 `ArtifactKind::Hook` 分支改为调用它；`Prompt | Provider`
仍返回 `None`。

注意：外部键含内容哈希，条目内容被修改时旧键会变 `missing`、新键变
`active`——这是匿名数组条目的固有语义，PRD 已接受（与 hooks 同步的
基线校验一致）。

### 3.3 目标身份行与中央 hooks 同步共用

`reconcile_observation` 通过 `repository::insert_project_target_identity_in`
按 `(tool, scope=project, artifact_kind=hook, path)` 取/建 `managed_targets`
行。实现时必须确认它与 `hooks/service_native.rs::ensure_hook_target` 命中同一
行（同 UNIQUE 键、空 baseline 不构成 ownership），否则中央所有权判定拿到的
`target_id` 不一致。若不一致，以 `find_project_target_identity` 的规则为准
并补一个断言测试。

### 3.4 中央托管隐藏

`should_hide_centralized` 增加 `"hook"` 分支：
`list_managed_hook_items(target_id)` 中任一 `last_applied_item_hash ==
record.observed_item_hash`（按哈希而非 external_key）。`summarize_project`
自动受益。

## 4. 展示数据（display_name / safe_summary）

`project_native_resources` 只存外部键与哈希，不存命令文本。列表接口
`list_project_native_resources` 每次调用都会先对账（即已扫描文件），因此：

- 让内部对账函数返回 `HookDisplayIndex = BTreeMap<(target_path, external_key), Value>`
  （只收集 Hook 条目），公开的 `reconcile_project_native_resources` 保持签名
  不变，内部改为调用带索引返回的私有函数。
- `to_dto` 增加可选参数（或新增 `to_dto_with_hook_display`）：
  - `hook_entry` 且索引命中：
    - `display_name = "<原生事件>[ · <matcher>]"`；
    - `safe_summary = { "kind": "hook", "event", "matcher", "timeout", "command" | "commandRedacted": true }`。
      `command` 经 `security::contains_detectable_secret("command", ..)` 判定，
      命中则不输出原文，仅 `commandRedacted: true`。
  - `hook_entry` 且索引未命中（`missing`）：按 `splitn(3, '|')` 解析外部键得到
    事件与 matcher；`safe_summary = { "kind": "hook", "event", "matcher" }`。
- 其他类型保持现状。

前端 `ProjectNativeResourceDto.safeSummary` 类型为 `JsonValue`，前端按
`kind === "hook"` 读取字段，做窄化处理即可，不改 DTO 结构。

## 5. 动作入口 fail closed

- `to_dto`：`can_disable = state == Active && entry_type != HookEntry`；
  `can_restore = state == Disabled && entry_type != HookEntry`。
- `native_ownership`：`ArtifactKind::Hook` 保持返回 `INVALID_INPUT`（文案改为
  "Hooks 暂不支持临时禁用与恢复"），`prepare_native_action` 在此处即拒绝，
  不会走到 `build_action_projection`。
- `evidence_entry_type(ProjectNativeEntryType) -> NativeResourceEntryType`
  改为返回 `Result`，`HookEntry` 分支返回 `AppError::internal`（理论不可达）；
  `restore_desired_projection` / `disable_evidence_details` / `item_hash_from_scan`
  的 `match` 对 `HookEntry` 同样返回 `internal` 错误，保证穷尽匹配且 fail closed。
- `rebuild_desired_projection` 里 `NativeResourceEntryType -> ProjectNativeEntryType`
  的映射无需改动。

## 6. 前端

文件：`src/features/projects/detail/native-resources.tsx`。

- `entryTypeLabel` 增加 `hook_entry -> "Hook 条目"`。
- `NativeResourceRow`：当 `resource.entryType === "hook_entry"`：
  - 不渲染操作按钮；渲染一行说明"Hooks 暂不支持临时禁用与恢复"；
  - 在名称下方渲染 `safeSummary`：`matcher`（若有）、`command`（`<code>`）
    或"命令包含可识别凭据，已脱敏"、`timeout`（若有）。
- 分区说明文案不变；空态文案不变。
- `src/bindings/commands.ts` 通过项目既有的 bindings 生成命令重新生成
  （实现时在 `package.json` / `src-tauri` 测试里找到生成入口，禁止手改）。

## 7. 兼容性与回滚

- 全新库：0021 直接建含新 CHECK 的表。
- 既有库：迁移重建表，旧数据保留；无数据回填。
- 之前 Hooks 视图的"空"状态是缺口而非合同，测试
  `project-detail-page.3.test.tsx` 只断言 MCP 视图空态，不受影响。
- 回滚：代码层 revert 即可；数据库层 0021 是超集 CHECK，旧代码读到
  `hook_entry` 行会在 `from_stable_str` 返回 `None` 而报 `INVALID_INPUT`，
  因此回滚代码前需清理 `hook_entry` 行（写入回滚说明即可，无自动化需求）。

## 8. 取舍记录

- 不用 `writable_schema` 文本改写：规范已禁止新迁移使用。
- 不把命令写入 DB：避免持久化潜在凭据；列表本就每次重新扫描，内存索引成本可忽略。
- 外部键沿用 hooks 模块的哈希键而不是新造"事件+序号"键：序号在数组
  增删时会漂移，导致大量假 missing/active；哈希键与同步基线一致。
