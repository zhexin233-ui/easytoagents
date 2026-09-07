# 项目级 Prompt 移除影响研究

## 已确认决策

- 完整移除项目级 Prompt：中央档案的项目分配/同步，以及项目原生 Prompt 的发现、禁用、恢复全部退出。
- 全局 Prompt 档案、全局分配、导入、预览和 Apply 保留。
- 项目目录中当前存在的 CLAUDE.md、AGENTS.md、.cursor/rules/easytoagents.mdc 不得被迁移或清理逻辑改动。
- 历史项目 Prompt 快照与恢复状态允许清除；迁移后不承诺恢复已禁用且仅存在于快照中的内容。
- 已发布迁移和归档任务保留，只追加前向迁移。

## 当前跨层数据流

项目详情的 Prompt 视图通过 profile-api 查询 set/get_prompt_project_assignment，后端写入 prompt_project_assignments；preview_prompt_sync 接收 projectId 并选择项目档案，Apply 再向项目 Prompt descriptor 写入 WholeDocument。项目扫描同时把同一路径观察为 PromptFile，并用 project_native_resources、snapshots 与 managed_targets 表达禁用、恢复和冲突。

关键证据：

- 前端项目 Prompt 入口与组件：src/features/projects/project-detail-page.tsx:409、src/features/projects/project-detail-page.tsx:1285。
- 项目分配服务：src-tauri/src/profiles/service.rs:332。
- 项目/全局 Prompt 分流：src-tauri/src/profiles/service.rs:695、src-tauri/src/profiles/service.rs:825。
- 项目 Prompt 原生观察：src-tauri/src/projects/native_resources.rs:297、src-tauri/src/projects/native_resources.rs:406。
- 分配表与 managed_targets 放宽：src-tauri/src/db/migrations/0008_prompt_project_assignments.sql:1。
- PromptFile 快照外键：src-tauri/src/db/migrations/0012_project_native_resources.sql:12。

## 删除与保留边界

| 层 | 删除 | 保留 |
| --- | --- | --- |
| 前端 | 项目 Prompt 视图、能力位、查询、分配/预览/Apply、PromptFile 展示与测试 | /prompts、全局档案、onboarding 全局导入/同步、Dashboard |
| RPC/DTO | set/get_prompt_project_assignment、项目 DTO、projectId 参数 | Prompt CRUD/import/global assignment、全局 preview/apply |
| Profile 服务 | 项目分配 repository/service、项目删除保护计数、prepare_prompt_sync 项目分支 | find_active_prompt_profile、全局 descriptor、全局同步 |
| Project/native | Prompt descriptor 项目扫描、PromptFile 类型与动作、Prompt ownership | MCP/Skill/Hook 项目资源与通用 preview/writer |
| Adapter | 四个工具的项目 Prompt descriptor | 四个工具的全局 Prompt descriptor、Codex override、Markdown/CursorMdc/WholeDocument |
| 数据库 | 项目 Prompt assignments/targets/native rows/snapshots/sync items | prompt_profiles、全局 Prompt targets/import previews、其他项目资源历史 |

特别注意：profile_import_previews 不含 project_id，当前 Prompt 记录属于全局导入流程，不能作为项目 Prompt 遗留数据整类删除。

## 迁移依赖

项目 Prompt managed target 被 sync_items 和 snapshots 以 RESTRICT 引用；PromptFile 的 disabled_snapshot_id 又以 RESTRICT 引用 snapshots。因此清理顺序必须先记录待退役快照路径，再移除项目 Prompt native rows，之后删除对应 snapshots、sync_items、仅剩空项的 Prompt run，最后删除 managed target 和 prompt_project_assignments。

同一 sync run 可能含其他 target。迁移只能删除 target_id 属于项目 Prompt 的 item/snapshot；混合 run 及其 MCP/Skill/Hook 数据必须保留。managed_targets 和 project_native_resources 的 CHECK 应通过精确锚定的 sqlite_schema 文本修订收紧，不能重建被多张表引用的 managed_targets。

物理快照位于应用私有 snapshots 根，而数据库迁移本身只处理关系数据。若清除物理文件，应在事务提交后使用可重试的退休队列，只删除通过现有 AppPaths/快照路径校验的文件；绝不使用 target_path 删除项目文件。

## 主要风险

- 粗暴删除 ArtifactKind::Prompt、WholeDocument 或 CursorMdc 会破坏全局 Prompt。
- 删除整个 project_native_resources、snapshots 或 sync_runs 表会破坏 MCP/Skill/Hook。
- 改写 0008/0012/0017 会让迁移历史前缀失配；必须新增 v18。
- 旧项目 Prompt 文件在移除后成为普通用户文件，应保持字节不变。
- 已禁用文件可能只剩快照；用户已接受清除快照后不可恢复。

## 验证重点

- v17 升级 fixture 同时包含项目 Prompt、全局 Prompt、混合 run 和 MCP/Skill 原生数据。
- v18 后项目 Prompt 表/target/native/snapshot 消失，其他数据与外键完整。
- 新库拒绝 project + prompt managed target，但接受 global + prompt。
- 四个 adapter 仍发现全局 Prompt，不再发现项目 Prompt。
- 项目详情不再出现 Prompt；全局 Prompt 页面和 onboarding 回归通过。
- bindings、TypeScript、Rust、文档和全仓残留检索一致。
