# 根因调研：项目级 Hooks 在"项目原生资源"中显示为空

## 现象

以本机项目 `/Users/zhexin/github/picslicer` 为例，项目目录中确实存在项目级 hooks：

| 工具 | 文件 | 内容摘要 |
| --- | --- | --- |
| Claude | `.claude/settings.json` | `hooks.SessionStart`（3 个 matcher 组：startup/clear/compact）、`hooks.PreToolUse`（Task/Agent）、`hooks.UserPromptSubmit`（无 matcher） |
| Codex | `.codex/hooks.json` | `hooks.UserPromptSubmit` 1 条 |
| Cursor | `.cursor/hooks.json` | `version: 1`，`hooks.preToolUse` / `sessionStart` / `beforeShellExecution` 扁平条目（matcher 在条目上） |

在项目详情页切到 Hooks 视图时，"项目原生资源"分区显示"当前组合没有项目原生资源"。

## 根因

后端 `src-tauri/src/projects/native_resources.rs` 明确把 Hook 排除在观测之外：

- `supported_project_descriptors`（约 243 行）只保留 `ArtifactKind::Mcp | ArtifactKind::Skill` 的项目描述符，Hook 描述符被过滤掉。
- `observe_items`（约 364 行）对 `ArtifactKind::Hook` 直接 `Ok(None)`，注释写明"Hooks 不参与项目原生资源逐条观测（MVP 范围外）"。
- `native_ownership`（约 907 行）对 Hook 返回 `INVALID_INPUT`，fail closed。
- `ProjectNativeEntryType`（`src-tauri/src/projects/models.rs:125`）只有 `McpEntry | Directory | Symlink`；DB `project_native_resources.entry_type` CHECK 也只允许 `'mcp_entry','directory','symlink'`（0012 建表，0018 收紧）。

前端 `src/features/projects/detail/page.tsx` 对 `hook` 视图同样挂载 `ProjectNativeResources`，并以 `artifactKind: "hook"` 调用 `listProjectNativeResources`，因此始终拿到空数组。

结论：这是功能缺口而不是回归。MVP 时把 Hooks 排除，但 UI 已经把 Hooks 视图接入原生资源分区，造成"有文件却显示为空"的误导。

## 可复用的既有能力

- 四个支持 Hooks 的工具都已声明项目级 Hook 描述符（`Scope::Project`、有 path、capability Supported）：
  - Claude：`.claude/settings.json`（`src-tauri/src/adapters/claude/mod.rs:157`）
  - Codex：`.codex/hooks.json`（`src-tauri/src/adapters/codex/mod.rs:141`）
  - Cursor：`.cursor/hooks.json`（`src-tauri/src/adapters/cursor/mod.rs:111`）
  - ZCode：`.zcode/config.json`（`src-tauri/src/adapters/zcode/mod.rs:123`）
  - OpenCode 不支持 Hooks，且前端已不向 OpenCode 展示 Hooks 入口（09-09 任务）。
- `src-tauri/src/hooks/service_core.rs`：
  - `events_root(tool)`：事件映射在文档中的路径（claude/codex/cursor 为 `["hooks"]`，zcode 为 `["hooks","events"]`）。
  - `build_hook_ownership(tool)`：按选择器根接管（claude/codex/zcode `hooks`，cursor `version`+`hooks`）。
  - `native_entries(observed, events_path)`（`pub(super)`）：把原生投影拍平为 `外部键 -> 条目`，外部键形态 `<原生事件>|<matcher>|<内容哈希前 16 位>`，同时兼容 matcher 组格式与 Cursor 扁平格式。
  - `native_entry_hashes`：全部条目内容哈希集合，用于与受管 hook 条目 `last_applied_item_hash` 匹配（受管 hook 条目的 external_key 形态是 `<Event>|<身份哈希>|<matcher>`，与原生键不同，**只能按内容哈希判定中央所有权**，见 `verify_hook_item_baselines`）。
- `src-tauri/src/db/hooks.rs:533` `list_managed_hook_items(database, target_id)` 返回 `ManagedHookItemRecord { external_key, last_applied_item_hash, .. }`。
- `src-tauri/src/security` `contains_detectable_secret(field, value)`：hooks 导入用它拒绝含凭据的命令，原生资源展示命令时可复用做脱敏。

## 禁用/恢复为何不纳入本次

- 原生资源的禁用/恢复依赖"按外部键定位单条条目"的 ownership（MCP 用 `selectors([container..., key])`，Skill 用 `SymlinkNames`）。Hook 条目是匿名数组元素，现有 `ManagedOwnership` 无法表达"数组中某一条"，禁用一条会退化为重写整个 `hooks` 子树，与"不改写原生文件其余内容"的合同冲突。
- 因此本次只做只读识别（active/missing），把禁用/恢复留作后续任务，DTO 上 `canDisable/canRestore` 恒为 false。

## 数据库约束

- `project_native_resources` 没有被任何表以外键引用（只引用 `managed_targets` 与 `snapshots`；`snapshots` 上有一个触发器按名字引用它）。
- 规范（`.trellis/spec/backend/database-guidelines.md` "Migration strategy for future schema changes"）要求新迁移不得用 `writable_schema` 改 CHECK 文本，须走 12 步表重建。该表无入向外键，重建可行。
- 当前最新迁移为 `0020_github_skill_sources.sql`，新迁移编号 `0021`。
