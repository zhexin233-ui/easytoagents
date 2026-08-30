# 技术设计：提示词档案工具无关化

## 数据模型（迁移 0009_prompt_tool_active_flags）

不可重建 `prompt_profiles`（RESTRICT 外键子表 + 迁移事务内 foreign_keys=ON）。全部使用 ADD/DROP 列与索引 + writable_schema CHECK 原地修订（0008 先例）：

1. `ALTER TABLE prompt_profiles ADD COLUMN is_active_claude INTEGER NOT NULL DEFAULT 0 CHECK(is_active_claude IN (0,1))`；同 `is_active_codex`。
2. 种子：`is_active_claude = CASE WHEN tool='claude' AND is_active=1 THEN 1 ELSE 0 END`（codex 同理）；随后 `UPDATE prompt_profiles SET is_active = 0`（遗留列清零）。
3. `DROP INDEX uq_prompt_profiles_one_active_per_tool`；新建 `uq_prompt_profiles_one_active_claude ON prompt_profiles(is_active_claude) WHERE is_active_claude = 1`（codex 同理）——每工具至多一份生效仍由索引强制。
4. writable_schema 原地修订 `prompt_profiles` 的 CHECK：`tool IN ('claude','codex')` → `tool IN ('claude','codex','central')`；作用域限定 `name='prompt_profiles'`（provider_profiles 等表含同形 CHECK，不可误伤）。新档案统一插入 `tool='central'`（遗留来源列；UNIQUE(tool,name) 下新档案名全局唯一）。

## 后端

- `PromptProfileRecord`：去 `tool`/`is_active`，增 `is_active_claude`/`is_active_codex`；`prompt_from_row` 同步。
- `list_prompt_profiles()` 去 tool 参数，`ORDER BY name COLLATE NOCASE, id`。
- `find_active_prompt_profile(tool)`：`WHERE (CASE WHEN ?1='claude' THEN is_active_claude ELSE is_active_codex END)=1`（全局同步唯一取数点）。
- `deactivate_prompt_profiles(tool, except_id)`：清对应标志位。
- 新 `set_global_prompt_assignment(db, tool, profile_id, assigned, expected_row_version)`：
  - assigned=true：目标 row_version 校验 → 清该工具旧启用 → 置目标标志（触发器递增 row_version）。
  - assigned=false：仅清目标标志（幂等）。
- `delete_prompt_profile`：保留「项目分配占用」阻塞；工具启用位随行删除。
- `discover_prompt_import(tool)`：前置改为「无对该工具启用的档案且无 `imported_from_path=该工具目标路径` 的档案」。
- `confirm_prompt_import`：新建时置 `is_active_<tool>=1`（接管连续性：导入后同步为 in_sync）。
- `set_prompt_project_assignment`：移除跨工具校验；启用守卫改为 `is_active_<tool>`（当前分配幂等例外保留）。
- `prompt_dto`：`{ id, name, body, globalTools, importedFromPath, rowVersion }`。
- 命令：`list_prompt_profiles()`、删除 `set_active_prompt_profile`、新增 `set_global_prompt_assignment`、`create_prompt_profile({name, body})`；`pnpm bindings:generate`。
- 总览 `overview/mod.rs` 与 onboarding 检测改读新标志。

## 前端

- `prompts-page.tsx` / `prompt-panel.tsx`（Skills 页结构）：
  - 页头 + aria-live；`中央列表` 区 = 布局切换 + `新增提示词` + 卡片（名称/状态行/正文预览/`PlatformAssignmentButton`×2/编辑/删除/提示文案）；列表区移除同步与检测按钮。
  - `全局目标状态` 区：每工具卡片 = availability + promptTargetPath + promptOverride 警告 + `检测并导入已有提示词` + `预览/直接应用全局同步`。
  - 图标启用 mutation：成功后失效 prompts 查询；preview_confirm 模式提示"全局启用已更新…请预览"；direct 模式自动对该工具 preview→apply。
  - FormDialog 去 tool（标题「新增/编辑 提示词」）。
- `profile-api.ts`：`promptProfilesQueryOptions()` 去参；`profileKeys.prompts` 去 tool；`activePromptProfile` NOT_FOUND 文案更新。
- `project-detail-page.tsx`：过滤条件 `!profile.globalTools.includes(tool) || 已分配`；「· 全局生效」文案保留。
- `onboarding-wizard.tsx`：`promptManaged = prompts.some(p => p.globalTools.includes(tool))`。

## 兼容与回滚

- 迁移前自动备份（既有机制）；0009 全部操作可逆性等同于列/索引增删，失败整体回滚事务。
- bindings 断代变更（DTO 字段、命令增删）一次性提交，前后端同 PR 内一致。
