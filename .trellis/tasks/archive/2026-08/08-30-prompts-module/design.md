# 技术设计：提示词模块（prompts）

> 模型：命名提示词档案 = 中央库；全局分配 = 现有「每工具一份生效」；项目级分配 = 新增，硬拷贝为项目根 `CLAUDE.md`/`AGENTS.md`。整文档（WholeDocument + Markdown `$document`）同步引擎全部复用，不新增 TargetFormat、不用符号链接。

## 1. 结构与边界

| 层 | 决策 |
|---|---|
| 后端 | **prompt 逻辑留在 `src-tauri/src/profiles/`**，不拆新 Rust 模块——prompt 与 provider 共享 import preview 表、`apply_profile_preview`、`get_tool_profile_status` 等管线，拆分是高风险低收益；项目级能力在原位扩展 |
| 前端 | 新模块 `src/features/prompts/` 统一收纳提示词 UI；`tool-profiles-page.tsx` 移除 `PromptPanel`（渠道页只留渠道面板）；`src/features/prompts/` 的 `.gitkeep` 占位即为预定位置 |
| ArtifactKind | 沿用 `"prompt"`，不新增枚举 |

## 2. 原生目标矩阵（适配器描述符）

| 工具 | 范围 | 路径 | 格式（全部已存在） |
|---|---|---|---|
| Claude | 全局 | `~/.claude/CLAUDE.md` | `TargetFormat::Markdown` + `$document` + `WholeDocument`（现状） |
| Claude | 项目 | `<root>/CLAUDE.md` | 同上（**新增描述符**，`adapters/claude/mod.rs` 项目数组） |
| Codex | 全局 | `$CODEX_HOME/AGENTS.md` | 同上（现状；`AGENTS.override.md` 阴影由现有 `promptOverride` 状态呈现） |
| Codex | 项目 | `<root>/AGENTS.md` | 同上（**新增描述符**） |

## 3. 后端改造点

### 3.1 存储迁移 `0008_prompt_project_assignments.sql`（编号=现有最大+1）
- 新表 `prompt_project_assignments(project_id → projects, tool, prompt_profile_id → prompt_profiles, created_at, updated_at, row_version, PK(project_id, tool))`——每项目每工具**至多一份**（项目根只有一个 CLAUDE.md/AGENTS.md），比 mcp/skill 的多对多更简。
- **重建 `managed_targets`** 以扩展其 CHECK（现约束 project 作用域仅 `artifact_kind IN ('mcp','skill')`，SQLite 不能改 CHECK）：按 SQLite 12 步流程重建表并原样保留数据，CHECK 放开为包含 `'prompt'`。此步需事务包裹 + 外键关闭/重开 + 迁移后行数/内容断言。

### 3.2 服务层（`profiles/service.rs`）
- `descriptor_for(env, tool, kind)` 泛化出项目形态：`prompt_descriptor_for(env, tool, scope, project_root: Option<&str>)`（全局行为不变）。
- `prepare_prompt_sync(database, env, tool, target: PromptSyncTarget)`，其中 `PromptSyncTarget = Global | Project { project_id }`：
  - Global：desired = `find_active_prompt_profile(tool)`（现状语义，含「无生效且无基线」报错、外部修改→预览失效的严格性**保持不变**）。
  - Project：desired = 该项目该工具的分配档案；desired 投影同 WholeDocument；**允许目标文件与基线不一致**——外部修改不算 stale，而是一条「本地已修改」预览项，应用=覆盖（复用 `build_preview_plan` 的 `baseline_mismatched_items`/readopt 通道，参照 mcp 项目目标）。
  - 基线：`ensure_profile_target` 按 (tool, kind='prompt', scope, project_id) 读写 `managed_targets`。
- `persist_prepared_preview` 去 hardcode：把 `Scope::Global/None` 参数化（`build_preview_plan` 本就接收 scope/project_id，mcp/skill 已在用）。
- `apply_profile_preview`：`ApplyProfilePreviewInput` 增加 `projectId: Option<String>`（camelCase，specta），校验 preview 记录的 scope 与之一致。
- 解除分配：删除 `prompt_project_assignments` 行 + 清理该项目目标基线；**不删除项目文件**（内容归项目所有，预览文案明示「文件保留、仅停止纳管」）。

### 3.3 命令层（`commands/profiles.rs`，prompt 族）
- 改：`preview_prompt_sync(tool, projectId: Option<String>)`；`ApplyProfilePreviewInput` 增 `projectId`。
- 增：`set_prompt_project_assignment(input { projectId, tool, profileId: Option<String>, rowVersions })`（None=解除分配）与 `get_prompt_project_assignment(projectId, tool) -> Option< assigned dto + 漂移状态 >`。
- 既有 list/create/update/set_active/delete/discover_import/confirm_import 不动（onboarding 向导与总览依赖保持兼容）。
- `lib.rs` 注册变化后 `pnpm bindings:generate`（`tests/bindings.rs` 门禁）。

## 4. 前端

- **新页 `src/features/prompts/prompts-page.tsx`**（路由 `/prompts`，侧边栏 `primaryLinks` 增加「提示词」）：
  - 工具切换（Claude/Codex 页签，复用现有 tab 模式）+ 档案库列表：新建/编辑（`FormDialog` name+Markdown textarea）/删除/设为全局生效 + 「检测已有提示词」接管导入——UI 主体从 `prompt-panel.tsx` 迁移；
  - 激活 → `previewPromptSync(tool)` → `ChangePreviewDialog artifactKind="prompt"`（直接应用模式 `canAutoApplyPreview` 逻辑随迁）；
  - 全局目标状态（`promptTargetPath`、`promptOverride` 提示）沿用 `ToolProfileStatusDto`。
- **`tool-profiles-page.tsx` 瘦身**：移除 PromptPanel 与 prompt 分支（openPreview/handlePreview 的 "prompt" 支），渠道面板不动；其测试相应裁剪。
- **`src/lib/profile-api.ts` 扩展**：`promptKeys` 增加 project 维度（assigned/options），失效键覆盖 `profileKeys.all` + 项目键；unwrapResult 复用。
- **`project-detail-page.tsx`**：`ProjectResourceView` 增加 `"prompt"`（第三个切换按钮）；新增 `ProjectPromptAssignments`（仿 `ProjectSkillAssignments` 的 AssignmentCard/ProjectOptionRow/blocked-state 模式）：展示当前分配（或未分配）、从档案库选择 → 预览 → 应用（apply 分支按 `artifactKind === "prompt"` 调 `applyProfilePreview` 带 projectId）；`projectBlocked(project, tool)` 既有闸门直接生效；`artifactLabel("prompt") => "提示词"` 已存在（line 853）。
- 总览（dashboard）与 onboarding：不改数据源，随命令签名变化仅调整调用处。

## 5. 兼容与迁移

- 全局行为零变化（同一套命令/基线/语义）；新增能力全部走新 projectId 参数与新表。
- bindings 再生成后前端类型同步；旧调用（无 projectId）默认 None=全局，不破坏 onboarding。
- 回滚：迁移为「新增表 + 重建 managed_targets（保留数据）」；前端移除新路由/入口即回旧 UI；无行为开关需求。

## 6. 测试

- Rust：`profiles/service.rs` 测试新增——项目分配→预览→应用写出 `<root>/CLAUDE.md`/`AGENTS.md`、外部修改→「本地已修改」预览项→覆盖、解除分配不删文件、global 严格语义回归不变、managed_targets 重建迁移断言；e2e（`tests/phase8_e2e.rs`）追加项目链路；`command_smoke.rs` 增新命令冒烟。
- 前端：`prompts-page.test.tsx`（迁移原 prompt-panel 用例 + 工具页签 + 接管导入）；`project-detail-page.test.tsx` 增项目提示词分配/漂移覆盖/解除分配；`tool-profiles-page.test.tsx` 移除 prompt 用例；bindings 一致性。
- 质量门：`pnpm check` 全绿。

## 7. 决策记录

1. **后端不拆 profiles 模块**：prompt/provider 共享管线多，拆分高风险低收益；「新模块」落在前端与交互层。
2. **项目级每 (项目,工具) 至多一份**（PK 设计）：目标文件唯一，模型天然如此。
3. **项目目标放宽外部修改语义、全局保持严格**：项目文件预期会被用户编辑（产品核心诉求），外部修改进入「本地已修改→覆盖」流程而非 stale 报错。
4. **解除分配不删项目文件**：内容所有权已转移，删除用户内容不可接受。
5. **无全局继承**：全局生效不进入项目（PRD R4，用户决策）。
