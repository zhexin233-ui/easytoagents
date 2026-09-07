# 技术设计：完整移除项目级 Prompt

## 1. 目标与边界

本次把 Prompt 收敛为纯全局资源。项目域保留 MCP、Skill、Hook 的中央能力，以及 MCP/Skill 现有的原生资源能力；项目 Prompt 的 UI、RPC、服务、adapter descriptor、native-resource 类型和持久化状态全部退出。

项目目录中的现存文件不属于迁移删除目标。历史 Prompt 快照及恢复状态可以不可逆清除。

## 2. 移除后的架构

### 2.1 全局 Prompt 流程

Prompts 页面或 onboarding
→ preview_prompt_sync(tool)
→ 读取该工具的全局 active Prompt profile
→ 解析全局 Prompt descriptor
→ 持久化 Preview
→ apply_profile_preview 在 Global scope 应用。

公开合同收窄为：

- preview_prompt_sync 只接收 tool，不再接收 projectId。
- ApplyProfilePreviewInput 删除 projectId；Provider 与 Prompt 均固定校验 Global scope。
- Prompt CRUD、导入和 set_global_prompt_assignment 保持不变。
- ArtifactKind::Prompt、PromptProfile、WholeDocument、Markdown、CursorMdc 继续存在。

### 2.2 项目资源流程

项目详情的资源视图联合类型只包含 mcp、hook、skill。项目服务只收集这三类 descriptor；adapter discover 仍返回全局 Prompt descriptor，但不再构造 project scope Prompt descriptor。

projects/native_resources 与 sync writer 删除 PromptFile 分支，保留 MCP entry、Skill directory/symlink 及通用 snapshot/preview/write 原语。项目扫描遇到 CLAUDE.md、AGENTS.md 或 Cursor 规则时不建立 app-owned identity，也不显示管理动作。

## 3. 后端边界

### 3.1 Profiles

- 删除 SetPromptProjectAssignmentInput、PromptProjectAssignmentDto、set/get_prompt_project_assignment 的 command、re-export、service 和 repository。
- 删除 count_prompt_project_assignments 及 delete_prompt_profile 的项目引用保护；数据库迁移先消除旧 FK 关系。
- prepare_prompt_sync 固定读取 find_active_prompt_profile 和全局 descriptor，删除项目注册、项目 assignment、项目基线冲突分支。
- apply_profile_preview 保留 Provider/Prompt 分支，但固定比较 Scope::Global，且不再向 prepare_prompt_sync 传 project。
- lib.rs 删除项目 Prompt 类型与命令注册，保留全局 Prompt 命令。

### 3.2 Projects、native resources 与 sync

- project_targets/supported_project_descriptors 不再包含 ArtifactKind::Prompt。
- 删除 observe_prompt_item、prompt_target_is_managed 及所有 PromptFile 状态、DTO、禁用、恢复、ownership 和 merge 分支。
- 删除 ProjectNativeEntryType::PromptFile 与 NativeResourceEntryType::PromptFile；保持其余 enum 值及数据库字符串稳定。
- sync/apply 仅删除 PromptFile match arm；通用 snapshot、atomic write、WholeDocument 和全局 Prompt Apply 不变。

### 3.3 Adapters

Claude、Codex、Cursor、ZCode 各自删除 project scope Prompt descriptor 与对应项目测试。全局 Prompt 路径、Codex AGENTS.override.md 状态、Cursor 全局 MDC 格式及所有非 Prompt 项目 descriptor 保留。

## 4. 数据库迁移

新增 0018_remove_project_prompts.sql，并把版本 18 追加到 MIGRATIONS。0001–0017 不改。

### 4.1 事务内关系清理

1. 物化 scope='project' 且 artifact_kind='prompt' 的 target id。
2. 在删除 snapshot 行前，把这些 target 对应的 snapshot id、run id、snapshot_path、storage_kind 和 content_hash 写入耐久的退休清理队列。
3. 删除这些 target 对应的 project_native_resources，先解除 disabled_snapshot_id 的 RESTRICT。
4. 删除这些 target 对应的 snapshots 和 sync_items；只删除清理后已无 item/snapshot 的项目 run，保留混合 run。
5. 删除这些 target；managed_items 由 CASCADE 清理。
6. DROP TABLE prompt_project_assignments；其私有索引随表删除。
7. 精确锚定 sqlite_schema，把 managed_targets 的项目 artifact CHECK 从 mcp/skill/prompt/hook 收紧为 mcp/skill/hook，把 project_native_resources.entry_type 移除 prompt_file。
8. 保留 prompt_profiles、全局 Prompt managed target、profile_import_previews、全局/其他资源的 snapshots 与 runs。

validate_migration_preconditions 增加 v18 的精确旧锚点计数；迁移提交后继续由现有 schema cookie 刷新逻辑强制重解析。新库完整执行 1→18，旧库只执行 18，最终 schema 一致。

### 4.2 物理快照清理

Database::open 完成 SQL 迁移后处理退休队列：

- 只接受位于 AppPaths.snapshots 下、且与记录 run/snapshot 身份匹配的普通 payload 快照路径。
- 合法存在的文件删除；不存在视为已完成；越界、符号链接或特殊文件绝不删除。
- 每项完成后删除队列记录；未完成项可在下次启动重试，队列清空后可删除临时队列表。
- 该流程只使用 snapshot_path，禁止使用 managed target 的 target_path，因此不会触碰项目文件。

物理快照删除是用户确认的不可逆行为。数据库启动备份仍用于迁移失败恢复，但不承诺恢复已清除的 Prompt 快照内容。

## 5. 前端与生成绑定

- tool-metadata 删除 promptProject，仅保留 promptGlobal。
- profile-api 删除 promptProject key/query。
- project-detail-page 删除 prompt 资源视图、Prompt 标题/能力回退、ProjectPromptAssignments 及 PromptFile 展示/动作。
- 全局 prompts-page 与 onboarding 将 previewPromptSync(tool, null) 改为 previewPromptSync(tool)；所有全局 apply payload 删除 projectId。
- Rust DTO/命令修改完成后运行 bindings:generate，生成删除项目 RPC、DTO、projectId 和 prompt_file union 值的 commands.ts。
- 删除或改写项目 Prompt 测试；新增“不显示 Prompt、不调用项目 RPC”的负向测试。全局 Prompt 测试保留并适配收窄签名。

## 6. 文档与规范

更新 README、docs/maintainers/adding-tool-adapter.md、backend/database-guidelines.md、backend/quality-guidelines.md、frontend/quality-guidelines.md，使当前合同明确：

- Prompt 仅支持全局作用域。
- 项目资源不观察或管理 Prompt 文件。
- v18 清理旧项目 Prompt 状态但不改项目文件。

归档 Trellis 任务与 0001–0017 继续作为历史证据，不做重写。

## 7. 验证策略

- 迁移：v17→v18、全新库、幂等 reopen、外键检查、精确 CHECK 金丝雀、混合 run 保留、项目文件字节不变。
- 后端：全局 Prompt CRUD/import/assign/preview/apply；项目 Prompt 命令和 descriptor 不存在；MCP/Skill/Hook 项目原生资源回归。
- 前端：项目详情无 Prompt；全局 Prompts、onboarding、Dashboard 正常。
- 绑定：生成后检查 Rust/TS 一致。
- 残留审计：当前源码和现行文档不再含项目 Prompt 公共能力；历史迁移与归档命中列为允许例外。

## 8. 回滚与风险控制

- 每个阶段保持可编译边界：先落迁移测试与后端合同，再生成 bindings，最后裁剪前端。
- 如 v18 迁移失败，IMMEDIATE 事务回滚且 schema_migrations 不推进；Database::open 已创建数据库/WAL/SHM 备份。
- v18 成功后，旧二进制无法识别新迁移前缀；代码回滚必须配套恢复迁移前数据库备份。
- 已清除的项目 Prompt 快照不可恢复，这是本次已接受的产品取舍；项目目录中的现存文件始终不参与清理。
