# 技术设计：项目级 Agent 文件只读纳入项目原生资源

前置阅读：`research/root-cause.md`、
`.trellis/spec/backend/quality-guidelines.md`（Explicit discovery and read-only preview；
Project-native resources 场景中的 `hook_entry` 合同）、
`.trellis/spec/backend/database-guidelines.md`（0021 table rebuild 先例）。
先例任务：`.trellis/tasks/archive/2026-09/09-12-fix-project-native-hooks-empty/design.md`。

## 1. 边界与核心决策

- **只读识别**：Agent 文件只有 `active` / `missing` 两种状态；不进入禁用 / 恢复流程。
- **目录级身份行 + 同步侧守卫**：原生登记沿用"一个目标路径 = 一行 `managed_targets`
  身份行 + 按 `external_key` 登记条目"的模型，身份行路径为 Agent **目录**；Agents 同步
  显式忽略"路径不是目录内合法文件"的行。理由与被否决方案见 §3 与 §10。
- **中央所有权按文件路径 + 非空 baseline 判定**，而不是像 Hook 那样按内容哈希：
  Agents 同步本就以文件路径为唯一键，路径判定稳定；空 baseline 不构成所有权
  （0012 迁移注释、`empty_identity_row_does_not_open_ordinary_mcp_apply` 已把这条
  原则写进测试）。
- **复用而不改语义**：文件扩展名、文件级路径校验、Markdown / TOML 解析、诊断码常量、
  大小上限全部复用 `agents` 模块，只提升可见性或抽取常量。

## 2. 数据模型

### 2.1 `ProjectNativeEntryType` 新增 `AgentFile`

`src-tauri/src/projects/models.rs`：

```text
McpEntry -> "mcp_entry" | Directory -> "directory" | Symlink -> "symlink"
HookEntry -> "hook_entry" | AgentFile -> "agent_file"   (新增)
```

`from_stable_str` / `as_str` 同步补齐。前端 bindings（`src/bindings/commands.ts`）由
`pnpm bindings:generate` 重新生成后得到 `"agent_file"`。
`sync::NativeResourceEntryType`（禁用 / 恢复证据）**不加** `AgentFile`：Agent 文件永远
不会生成动作证据（§6）。`ProjectNativeResourceKind`（`Mcp | Skill`）当前无调用方，
本任务不动。

### 2.2 迁移 `0024_project_native_agent_files.sql`

`project_native_resources` 没有入向外键，按规范 12 步表重建，与 0021 逐字同构：

1. 前置校验：`sqlite_master` 中该表 SQL 必须**恰好一次**包含
   `entry_type IN ('mcp_entry', 'directory', 'symlink', 'hook_entry')`，否则
   `CHECK(matched = 1)` 中止。
2. `CREATE TABLE project_native_resources_new(...)`：完整复制 0021 的列、CHECK、UNIQUE，
   仅把 `entry_type` CHECK 改为
   `IN ('mcp_entry', 'directory', 'symlink', 'hook_entry', 'agent_file')`。
3. 显式列清单 `INSERT ... SELECT`；行数差校验 `CHECK(diff = 0)`。
4. 先删旧表两个 row_version 触发器与 `snapshots` 上的
   `trg_snapshots_reject_native_resource_id_update`，再 `DROP TABLE` 与 `RENAME`。
5. 重建三个索引、两个 row_version 触发器、快照交叉保护触发器（SQL 与 0012 / 0021
   逐字一致）。
6. `pragma_foreign_key_check` 违例数为 0、`pragma_integrity_check` 为 `ok`；清理临时表。

登记：`src-tauri/src/db/mod.rs` `MIGRATIONS` 末尾 `version: 24`；
`src-tauri/src/app/mod.rs` 三处 `schema_version` 断言 23 → 24。

迁移测试镜像 `db/tests.rs::project_native_hook_entries_migration_preserves_rows_and_widens_check`：
用 `MIGRATIONS[..23]` 建旧库，插入 `mcp_entry` / `directory` / `symlink`（disabled，持快照）
/ `hook_entry` 四行，再交给真实迁移；断言行数与字段不变、可插入 `agent_file`、
`prompt_file` 仍被拒绝、快照 id 更新仍被触发器拒绝、重开幂等。

## 3. 目标身份行与 Agents 同步隔离

### 3.1 身份行

`reconcile_observation` 通过 `repository::insert_project_target_identity_in` 按
`(project_id, tool, 'agent', 'project', <root>/.<tool>/agents)` 取 / 建身份行。
`managed_targets` 现有 CHECK（0022 放宽后）允许项目作用域 `agent`；ZCode 项目级无路径，
不会插入，`tool != 'zcode' OR ... scope = 'global'` 约束不受影响。

### 3.2 Agents 同步守卫（`src-tauri/src/agents/service_core.rs::prepare_agents_sync`）

在 `existing_rows = repository::list_agent_managed_targets(...)` 之后、
`if desired.is_empty() && existing_rows.is_empty()` 之前，插入过滤：

```rust
// 原生资源登记会为 Agent 目录创建一行空 baseline 的目录级身份行；它不是受管文件，
// 既不能触发"空集仍建运行"，也不能进入删除候选。只有派生路径与行路径完全一致的
// 行才是本目录内的合法 <name>.<ext> 文件。
existing_rows.retain(|row| {
    agent_file_descriptor(&directory_descriptor, &row.target_path, input.tool).is_ok()
});
```

`agent_file_descriptor` 已实现"派生路径必须等于 `target_path`"的校验，目录路径
（stem 为 `agents`，派生为 `<dir>/agents.<ext>`）必然被拒绝。

其他入口无需改动（根因文档 §3.2）：全局状态聚合只查 `Scope::Global`；
`readopt_agent_target` 按精确文件路径匹配并再经 `agent_file_descriptor` 校验；
`projects/service.rs` 对 Agent 直接返回 `None`。

### 3.3 必须先做的验证测试

`native_resources_tests.rs::agent_directory_identity_row_never_enters_agents_sync`：

1. 登记含 `.claude/agents/agents.md` 与 `.claude/agents/helper.md` 的项目（故意用
   `agents.md` 命中目录 stem）；列出原生 Agent 资源，断言目录身份行存在且 baseline 为空；
2. 无分配：`preview_agent_sync(claude, Some(project))` 返回零目标；
3. 创建并分配一个中央 Agent `reviewer` 后再预览：目标只有 `reviewer.md` 一条
   `delete_target = false`，不含 `agents.md` / `helper.md`；
4. 全程断言两份原生文件字节不变。

## 4. 后端观测流程

文件：`src-tauri/src/projects/native_resources.rs`。

### 4.1 描述符纳入

`supported_project_descriptors` 过滤扩为
`Mcp | Skill | Hook | Agent`。ZCode 项目级 Agent descriptor `path: None` 且 capability
Unsupported，被既有 `path.is_some()` 与 `observe_descriptor` 的 Supported 门禁排除。
Codex 项目未受信任时仍只读观测（与 MCP 一致），信任只在动作阶段校验。

### 4.2 `observe_agent_items`

```text
observe_agent_items(database, descriptor, project_id, target_id)
  -> Result<Option<ObservedItems>>
```

- `ext = agent_file_extension(descriptor.tool)`；`dir = descriptor.path`。
- `fs::read_dir(dir)`：`NotFound` → `Some(ObservedItems::plain(vec![]))`（已有记录转
  `missing`）；其他错误 → `None`（本轮不对账，与 MCP 非 Observed 扫描一致）。
- 遍历条目（按文件名 `BTreeMap` 排序保证确定性）：
  - 文件名必须以 `.<ext>` 结尾，stem 非空且不以 `.` 开头；
  - `symlink_metadata` 为普通文件，或为符号链接且 `metadata` 解析为文件；其余跳过；
  - `fs::read` 失败（含权限）→ 整个目标返回 `None`，不做部分对账；
  - `item_hash = hash_bytes(&bytes)`；
  - `external_key = 文件名（含扩展名）`，`entry_type = AgentFile`；
  - 解析展示信息（§5）并写入展示索引。
- 中央所有权：调用一次
  `db::agents::list_agent_managed_targets(database, tool, Scope::Project, Some(project_id))`
  建 `path -> has_baseline` 映射；`centrally_owned = map[dir/external_key] == true`，
  其中 `has_baseline = baseline_full_hash.is_some() || baseline_managed_hash.is_some()`。
- `observe_items` 的 `ArtifactKind::Agent` 分支调用它；`Prompt | Provider` 仍 `None`。
  `observe_descriptor` 需要把 `project_id` 传入（现签名已有）。

`reconcile_observation` 对 `centrally_owned && 无既有原生记录` 的条目跳过登记（既有逻辑），
所以已应用的中央文件不会被登记；曾是原生、后被首次接管的文件保留记录并由
`should_hide_centralized` 隐藏。

### 4.3 中央托管隐藏

`should_hide_centralized` 增加 `"agent"` 分支：
`find_project_target_identity(database, &record.project_id, tool, ArtifactKind::Agent,
&join(record.target_path, record.external_key))` 存在且 baseline 非空 → 隐藏。
`summarize_project` 自动受益（`ProjectDto.nativeResources` 计数含 Agent 文件；
`hasBlockedNativeResources` 只看 disabled / conflict，不受影响）。

### 4.4 解析与 `parse_artifact`

`parse_artifact` 增加 `"agent" => ArtifactKind::Agent`（否则 DTO 组装会对新行报
`INVALID_INPUT`）。

## 5. 展示数据（display_name / safe_summary）

`project_native_resources` 只存外部键与哈希；列表接口每次先对账（已读过文件），因此
展示信息只在内存索引中传递，不落库（与 Hook 相同，避免把正文 / 描述持久化）：

- 把 `HookDisplayIndex` 泛化为 `NativeDisplayIndex = BTreeMap<(target_path, external_key), Value>`；
  对账函数对 Hook 与 Agent 观测都填充；`DescriptorObservation.hook_entries` 改名为
  `display_entries`（语义：外部键 → 展示 JSON）。
- Agent 条目展示值（在 `observe_agent_items` 内构造）：
  - 字节数 > `MAX_AGENT_FILE_BYTES`（从 `agents/import.rs` 抽为 `agents` 模块
    `pub(crate)` 常量）→ `{ kind: "agent", fileName, parseError: "AGENT_FILE_TOO_LARGE" }`，
    不读取正文做解析（哈希仍基于全文件字节）；
  - 非 UTF-8 → `parseError: "AGENT_FRONTMATTER_INVALID"`；
  - Codex 用 `parse_codex_agent_file(text)`，其余用 `parse_markdown_agent_file(text, tool)`；
    `Err(code)` → `parseError: code`；
  - `Ok(parsed)`：`name = parsed.name.unwrap_or(stem)`；`description`：
    `contains_detectable_secret("description", &d)` 命中 → `descriptionRedacted: true`，
    否则按字符截断到 200 字符并加 `…` 后写入 `description`；
  - 统一携带 `fileName = external_key`；**不携带 prompt 正文**。
- `to_dto`：`entry_type == AgentFile` 时 `display_name = summary.name`（索引未命中，即
  `missing` 行 → stem of external_key），`safe_summary = 索引值`（未命中 →
  `{ kind: "agent", fileName }`）。
- `safe_summary(artifact_kind, entry_type)` 的 Agent 分支返回 `{ "kind": "agent" }` 作为
  兜底。
- 解析用函数 `parse_markdown_agent_file` / `parse_codex_agent_file` 与诊断码常量、
  `agent_file_descriptor` 提升为 `pub(crate)` 并在 `agents/mod.rs` 重导出。

## 6. 动作入口 fail closed

- `to_dto`：`can_disable = state == Active && !read_only`，
  `can_restore = state == Disabled && !read_only`，其中
  `read_only = matches!(entry_type, HookEntry | AgentFile)`。
- `native_ownership`：`ArtifactKind::Agent` 单独分支返回
  `INVALID_INPUT("artifactKind", "Agent 文件暂不支持临时禁用与恢复")`；
  `prepare_native_action` 在此即拒绝，不会走到投影构造。
- `disable_evidence_details` / `restore_desired_projection` / `item_hash_from_scan` /
  `validate_live_occupancy` / `evidence_entry_type` / `skill_entry_item_hash` 的 `match`
  补 `AgentFile` 分支：与 `HookEntry` 一样返回 `AppError::internal`（理论不可达），
  保证穷尽匹配且 fail closed。
- `rebuild_desired_projection` 里 `NativeResourceEntryType -> ProjectNativeEntryType` 的
  映射无需改动。

## 7. 前端

### 7.1 类型与查询（`src/lib/projects-api.ts`）

- `ProjectResourceKind = Exclude<ArtifactKind, "provider" | "prompt">`；
  `ProjectScopeKind = ProjectResourceKind | "project"`。删除"Agent 不是整项目原生资源"
  的注释，改为说明 Agent 目录按文件只读列出。
- `invalidateProjectScope` 的 `"agent"` 分支已存在，不变。

### 7.2 页面（`src/features/projects/detail/page.tsx`）

- 去掉 `activeResourceView !== "agent"` 门禁，所有视图都挂载 `ProjectNativeResources`。
- `applyMutation.onSuccess` 的失效列表补 `"agent"`（当前 Agent 不会产生原生动作预览，
  补上是为了与视图集合保持穷举）。

### 7.3 原生资源行（`src/features/projects/detail/native-resources.tsx`）

- `entryTypeLabel` 增加 `agent_file -> "Agent 文件"`。
- `NativeResourceRow`：`readOnly = entryType === "hook_entry" || entryType === "agent_file"`；
  只读行不渲染操作按钮；Agent 行渲染 `AgentSummary`：
  - `文件：<code>{fileName}</code>`；
  - `描述：…` 或 "描述包含可识别凭据，已脱敏。"；
  - `parseError` 存在时显示 `解析失败：<code>`；
  - 固定说明 "Agent 文件暂不支持临时禁用与恢复。"
- `safeSummary` 仍按宽松 JSON 读取（复用 `hookSummaryFields` 的窄化辅助，重命名为通用
  名称）。

### 7.4 测试

- `project-detail-page.test-helpers.tsx` 增加 `agentNativeResource`（`agent_file`、
  `safeSummary = { kind: "agent", name, description, fileName }`）与
  `agentRedactedResource`（`descriptionRedacted: true`）fixture。
- `project-detail-page-agents.test.tsx` 新增：
  - Agents 视图调用 `listProjectNativeResources` 且 `artifactKind: "agent"`；渲染显示名、
    "Agent 文件"标签、文件名、描述与只读说明；不存在禁用 / 恢复 / 不可操作按钮；
    `previewProjectNativeResourceAction` 未被调用；
  - 脱敏条目不显示描述原文。

## 8. 后端测试清单（`native_resources_tests.rs`）

| 用例 | 覆盖 |
| --- | --- |
| `claude_project_agent_files_are_listed_read_only` | 顶层 `.md` 识别、干扰项跳过、DTO 字段、字节不变 |
| `codex_cursor_opencode_agent_files_are_listed` | 多工具扩展名与解析；ZCode 无结果 |
| `removed_agent_file_becomes_missing` | 单文件删除与目录删除 |
| `centrally_applied_agent_file_is_hidden_from_native_list` | 应用后隐藏、同目录原生文件仍列出、仅预览未应用不隐藏 |
| `agent_file_action_preview_is_rejected` | `INVALID_INPUT` |
| `agent_description_with_secret_is_redacted_in_summary` | 脱敏 |
| `unparseable_agent_files_are_listed_with_diagnostic` | frontmatter 非法 / 非 UTF-8 / 超大 |
| `agent_directory_identity_row_never_enters_agents_sync` | §3.3 安全守卫 |

`agents/service_tests.rs` 可补一条最小用例：手工插入目录级 agent 身份行后
`prepare_agents_sync` 不产生删除目标（若 §3.3 的集成用例已覆盖则可省略）。

## 9. 兼容性与回滚

- 全新库：0024 直接建含新 CHECK 的表。既有库：表重建，旧数据保留，无回填。
- 首次运行后，每个含 Agent 目录的已登记项目会多出一行目录级 `managed_targets`
  身份行（空 baseline）；`remove_project` 的 `managed_target_count` 计数因此可能从 0 变为
  正数并提示"原生配置保持未纳管"，与 MCP / Skill 身份行的既有语义一致。
- 回滚：代码层 revert；数据库层 0024 是超集 CHECK，旧代码读到 `agent_file` 行会在
  `from_stable_str` 返回 `None` 而报 `INVALID_INPUT`，回滚代码前需清理 `agent_file` 行
  与对应目录身份行（写入回滚说明即可）。

## 10. 取舍记录

- **否决"按文件创建身份行并与 Agents 同步共用"**：每个原生文件都会进入
  `list_agent_managed_targets`，在删除候选循环里被当作"不再需要的受管文件"删除；
  且文件消失时没有目录级观测可把它标为 `missing`。
- **否决"把守卫放进 `list_agent_managed_targets` SQL"**：SQL 层不知道目录路径与扩展名，
  用 `LIKE '%.md'` 之类的模式既脆弱又会误伤全局作用域；服务层已有精确的
  `agent_file_descriptor` 校验。
- **否决"顺带把删除候选限制为非空 baseline 行"**：这是对既有同步语义的独立修改
  （仅预览未应用的行是否应被删除），与本任务无关，若需要另立任务。
- **中央所有权按路径而非哈希**：Agents 同步的键就是文件路径；按哈希会让用户微改一个
  受管文件后立刻"变成原生资源"，与漂移检测的语义冲突。
- **不用 `scan_target` 逐文件扫描**：只需要字节哈希与展示解析，直接 `fs::read`
  更简单，且 TOML 语法错误的文件也能被列出并带诊断码。
- **不持久化描述与正文**：与 Hook 命令同理，列表本就每次重新扫描，内存索引成本可忽略。
