# 技术设计：Hooks 全局级与项目级管理

## 1. 总体思路

完整复用现有 MCP 的「中央库 + 分配 + 预览/Apply + 快照/恢复」管线，不新增并行机制：

```
前端 hooks-page / project-detail-page
  → commands/hooks.rs (tauri-specta)
    → hooks/service.rs (prepare_hooks_sync)
      → tool_adapter(tool).discover() 过滤 ArtifactKind::Hook descriptor
      → repository::list_assigned_hooks(tool, project_id|None)
      → build_desired_projection（每工具原生 JSON 投影）
      → sync::scan_target / build_preview_plan / apply_persisted_preview（完全复用）
        → managed_targets + managed_items + snapshots
```

唯一新增的领域概念：`ArtifactKind::Hook`（序列化值 `"hook"`）。

## 2. 领域合同（domain + adapters）

### 2.1 domain/mod.rs
- `ArtifactKind` 增加 `Hook => "hook"`；同步更新其序列化往返测试。
- 新增 `HookEvent` string_enum（canonical PascalCase，13 值，见 PRD R2）与 per-tool 支持集合：
  ```rust
  pub fn hook_event_supported(tool: Tool, event: HookEvent) -> bool
  ```
  Cursor 映射：序列化时按工具转换 —— canonical → cursor 用 camelCase（`hook_event_native_key(tool, event)`）。
- `ASSIGNABLE_HOOK_TOOLS: [Tool; 4]`（adapters/mod.rs，四工具全部支持）。

### 2.2 各 adapter discover() 新增 descriptor

| 工具 | Scope | 目标路径 | Format | managed_selector_roots | ownership 语义 |
|---|---|---|---|---|---|
| claude | Global | `<claude_config_dir>/settings.json` | Json | `["hooks"]` | 选择器（与 provider 的 `["env"]` 同文件共存） |
| claude | Project | `<root>/.claude/settings.json` | Json | `["hooks"]` | 选择器 |
| codex | Global | `<codex_home>/hooks.json` | Json | `[]`（整文档） | WholeDocument |
| codex | Project | `<root>/.codex/hooks.json` | Json | `[]` | WholeDocument + 沿用项目 `.codex` 信任层（与项目 MCP 同 trust 语义） |
| cursor | Global | `~/.cursor/hooks.json` | Json | `[]`（整文档） | WholeDocument |
| cursor | Project | `<root>/.cursor/hooks.json` | Json | `[]` | WholeDocument |
| zcode | Global | `~/.zcode/cli/config.json` | Json | `["hooks"]` | 选择器（与 `mcp.servers` 同文件共存，zcode/mod.rs:7 既有约定） |
| zcode | Project | `<root>/.zcode/config.json` | Json | `["hooks"]` | 选择器 |

- 四工具 hooks capability 全部 `Supported`（官方证据见 PRD）；`sensitive_selectors` 为空（hook 命令行与 MCP command 同级，不视为敏感值）。
- Codex 项目信任：复用 codex/mod.rs 现有项目 trust 发现（config.toml `projects.<root>.trust_level`），descriptor 携带与项目 MCP 相同的 trust/policy 字段。
- Claude customization policy（strictPluginOnlyCustomization）不封锁 settings.json 的 hooks（该策略封锁的是 mcp/skills 自定义文件；hooks 属核心 settings 合同）——不做策略门禁，注释说明该假设。

### 2.3 原生投影（build_desired_projection）

中央记录 `{name, event, matcher?, command, timeout_seconds?, enabled}` → 各工具原生结构。同一 `(event, matcher)` 的多条 hook 合并进同一 matcher 组；事件键与组内条目按确定性顺序（BTreeMap + hook name）排列，保证幂等渲染。

- **claude**：投影值 = `hooks` 键下的对象
  `{"<Event>": [{"matcher": M?, "hooks": [{"type":"command","command":C,"timeout":T?}]}]}`
- **codex**：整文档
  `{"hooks": {"<Event>": [ ... 同 claude 结构 ... ]}}`
- **cursor**：整文档（camelCase 事件键、扁平条目）
  `{"version": 1, "hooks": {"<event>": [{"command": C, "matcher": M?, "timeout": T?}]}}`
- **zcode**：`hooks` 键下的对象（runner 级 `enabled` 恒为 true，否则配置文件 hooks 不运行）
  `{"enabled": true, "events": {"<Event>": [{"matcher": M?, "hooks": [{"type":"command","command":C,"timeout":T?}]}]}}`

### 2.4 per-hook managed item（复用 managed_items 机制）

- `external_key`（确定性、稳定）：`<Event>|<matcher 或 ->|<sha256(command\0timeout) 前 16 hex>`。
- 定位/校验：按 `(event, matcher)` 找到原生组，组内任一条目的内容哈希等于该 item 的 `last_applied_item_hash` 即视为同步；找不到 → 该 item 漂移（`ManagedItemBaselineMismatch`）。数组无名称键，因此用哈希匹配代替 MCP 的名称匹配，external_key 唯一性由 preflight 现有约束保证。
- apply.rs 的 `resource_kind == artifact_kind` 校验天然兼容（`hook == hook`）。

## 3. 数据库（迁移 0014_hooks.sql）

1. 新建表（风格随 0001：UUID CHECK、json_valid、row_version + guard/bump 触发器）：
   - `hooks(id PK, name TEXT NOT NULL COLLATE NOCASE UNIQUE, event TEXT NOT NULL CHECK(event IN (...13 个 canonical 值...)), matcher TEXT, command TEXT NOT NULL, timeout_seconds INTEGER CHECK(timeout_seconds IS NULL OR timeout_seconds > 0), enabled INTEGER NOT NULL CHECK(enabled IN (0,1)), row_version, created_at, updated_at)`
   - `hook_global_assignments(tool CHECK IN ('claude','codex','cursor','zcode'), hook_id FK, PK(tool, hook_id))`
   - `hook_project_assignments(project_id FK, tool CHECK 同上, hook_id FK, PK(project_id, tool, hook_id))`
   - 互斥触发器：镜像 mcp/skills 的全局/项目分配互斥触发器集（0001:212-290）。
2. writable_schema 放宽（每个替换限定表名 + 精确旧锚点，未命中即回滚；先例 0013）：
   - `managed_targets`：`CHECK(artifact_kind IN ('provider', 'prompt', 'mcp', 'skill'))` → 加入 `'hook'`；
     project 行约束 `artifact_kind IN ('mcp', 'skill')` → 加入 `'hook'`。
   - `managed_items`：`CHECK(resource_kind IN ('provider', 'prompt', 'mcp', 'skill'))` → 加入 `'hook'`。
3. `db/mod.rs` MIGRATIONS 追加 version 14；`app/mod.rs:323` schema_version 断言 13 → 14。
4. 新增 `db/hooks.rs`：镜像 db/mcp.rs（CRUD、global/project assignment、list_assigned_hooks(project_id|None)、managed item 读写、乐观并发 + Immediate 事务）。

## 4. 服务层（hooks/service.rs）

镜像 mcp/service.rs 的公开面（全部同步、`&Database` 第一参）：

- CRUD：`list_hooks / get_hook / create_hook / update_hook / set_hook_enabled / delete_hook`
- 分配：`set_global_hook_assignment / set_project_hook_assignment / list_hook_projects / list_hook_project_options`
  - 分配校验：`hook_event_supported(tool, hook.event)`，不支持 → fail-closed 拒绝（诊断码 `HOOK_EVENT_UNSUPPORTED`）。
- 状态：`list_global_hook_target_statuses`
- 同步：`preview_hook_sync / apply_hook_preview / readopt_hook_target`（内部 `prepare_hooks_sync` 镜像 `prepare_mcp_sync`：desired + 全局继承、空投影不建目标、allowed_root 按 tool 推导同 MCP）
- 导入：`discover_hook_import(tool)`（只读解析全局目标 → 候选 + 诊断）/ `confirm_hook_import`（显式选择入中央库；名称冲突报 conflict）

关键辅助（模块内）：
- `native_selector_root(tool)`：claude/zcode `["hooks"]`，codex/cursor `[]`（整文档）。
- `events_root(tool)`：claude `[]`、zcode `["events"]`、codex/cursor `["hooks"]` —— 在 managed projection 内定位事件映射的路径。
- `group_key(event, matcher)` / `entry_hash(...)`：per-item 匹配的基础。

## 5. Commands 与 bindings（commands/hooks.rs + lib.rs）

- DTO 全部 `#[derive(Serialize, Deserialize, Type, Specta)]`，命名对齐 mcp 命令集：`HookDto`、`CreateHookInput`、`UpdateHookInput`、`SetGlobalHookAssignmentInput`、`SetProjectHookAssignmentInput`、`PreviewHookSyncInput`、`ApplyHookPreviewInput`、`ReadoptHookTargetInput`、`DiscoverHookImportInput/Output`、`ConfirmHookImportInput` 等。
- `lib.rs`：`.typ::<...>()` 注册所有 DTO + `collect_commands!` 追加 hooks 命令。
- `pnpm bindings:generate` 更新 `src/bindings/commands.ts`；`bindings:check` 测试防漂移。

## 6. 前端

- `src/lib/tool-metadata.ts`：`capabilities.hooks: boolean`（四工具 true）+ `HOOK_TOOLS`。
- `src/lib/hooks-api.ts`：镜像 mcp-api.ts（`hooksKeys`、queryOptions 工厂、导入查询 `retry:false, staleTime:Infinity, gcTime:0`）。
- `src/features/hooks/hooks-page.tsx`：镜像 mcp-page.tsx 骨架 —— 中央列表（list/grid 持久化 key 增加 `hooks`）、FormDialog 编辑表单（name/event 下拉/matcher/command/timeout/enabled）、每工具 PlatformAssignmentButton（事件不支持的工具禁用 + title 说明）、全局目标状态卡（globalTargetStatusPresentation + SyncStatusBadge）、ChangePreviewDialog、导入对话框。
- `src/features/projects/project-detail-page.tsx`：资源 tab 增加 Hooks；`ProjectHookAssignments` 镜像 ProjectMcpAssignments（双乐观锁 row_version）。
- 路由：`router.tsx` 加 `/hooks`；`app-shell.tsx` primaryLinks 加 Hooks；同步更新 `app-shell.test.tsx` 导航断言。
- 文案：简体中文硬编码（与现状一致）。

## 7. 权衡与替代方案

- **统一事件枚举 vs 每工具独立记录**：选统一枚举 + per-tool 校验。理由：中央库跨工具分配是本产品核心价值；MCP 已确立该模式（transport 映射）。代价是 cursor 独有事件与 prompt 型 hook 无法建模 —— 已列入非目标，且 fail-closed 不猜测。
- **Codex 用独立 hooks.json vs config.toml 内联**：官方明确「同层混用两种表示会合并并警告」，且 config.toml 已被 provider/MCP/trust 多选择器管理；独立文件用 WholeDocument 最干净、漂移语义最清晰。
- **Claude settings.json 与 provider 同文件**：两者都是选择器化子树（env / hooks），render 保留非受管内容，与 zcode 既有共存经验一致。
- **per-item 基线 vs 目标级基线**：选 per-item（哈希匹配）。apply 管线是通用机制，增量成本低，换来精确到单条 hook 的漂移定位。
- **导入范围限全局目标**：项目级导入涉及 trust/继承组合爆炸，MVP 不做（PRD 非目标）。
- **导入不做持久化证据/接管**（实现期修订）：MCP 导入的持久化预览 + `adopt_import` 是为了在导入事务内完成基线接管（需要 `mcp_import_previews` 表）。Hooks 导入简化为「只读发现 → 用户显式选择 → 仅创建中央记录」，不做基线接管；导入后通过常规分配 + 预览/Apply 进入受管状态（内容一致时 Apply 近似 no-op，无删除风险）。免去了新增 import_previews 表与整套接管事务，同时不违反「导入不隐式接管原生目标」的合同。

## 8. 回滚

- UI/共享集合先关闭 `HOOK_TOOLS`，再移除 service/commands，最后移除 adapter 分支；迁移 0014 前向保留（放宽的 CHECK 对旧数据无破坏）。
- 任何原生写入失败走现有 snapshot/journal 恢复，不加旁路清理。
