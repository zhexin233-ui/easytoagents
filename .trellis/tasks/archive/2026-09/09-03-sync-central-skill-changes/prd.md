# 中央 Skill 内容变更后同步更改

## Goal

导入到中央列表的 Skill 被用户改了中央副本、出现 `CENTRAL_SKILL_CONTENT_CHANGED` 时，卡片提供「同步更改」。点按钮先问是否确认，选「是」后把当前中央文件采纳为权威内容，应用恢复 Ready，并解除因此阻断的预览、删除与同步。

## Background

用户从中央 Skills 页导入 Skill 后修改该 Skill，界面只显示 `CENTRAL_SKILL_CONTENT_CHANGED`，没有操作入口。既有任务 `09-02-manage-project-resources` 仍为 `in_progress`，本需求单独处理，不并入项目原生资源禁用/恢复。

## Confirmed Facts

- `CENTRAL_SKILL_CONTENT_CHANGED` 表示中央私有副本完整树 hash 与 `skills.content_hash` 不一致，或记录状态不是 Ready。判定在 `inspect_central_skill`（`src-tauri/src/skills/library.rs:470-476`）。
- 列表将该 Skill 标为 `Invalid`。全局目标因 `inspect_record != Ready` 短路为 `external_owned_change` 并带同一诊断码（`src-tauri/src/skills/service.rs:439-452`、`:1748-1752`）。
- 内容预览与已分配 Skill 的同步准备会因此阻断（`preview_skill_content` 在 `src-tauri/src/skills/service.rs:107-111`；`validate_ready_records` 在 `:981-992`）。质量规范把中央 hash 漂移列为预览/同步/删除阻断（`.trellis/spec/backend/quality-guidelines.md:631`）。
- 中央列表卡片目前只原样展示诊断码（`src/features/skills/skills-page.tsx:337-341`）。
- Skills 目标条目只记录符号链接类型与链接路径，`full_hash` / `last_applied_item_hash` 不含被链接目录的文件内容（`src-tauri/src/sync/mod.rs:216-254`；`src-tauri/src/skills/service.rs:839-856`）。改中央文件后链接本身未漂移；中央记录重新 Ready 后，已 Apply 目标即可回到 in_sync。
- 导入不随原来源自动更新（`src/features/skills/skills-page.tsx:572`）。改原来源不会产生本诊断。
- 尚无把中央 Skill 新 hash / frontmatter 写回记录的命令。中央目录以 `frontmatter.name` 命名且 NOCASE 唯一；改名不能当作单纯内容采纳。
- `skills` 表有 `row_version` 自动 bump 触发器（`src-tauri/src/db/migrations/0001_initial.sql:599-606`）。Skills 页已有可访问确认对话框模式（`useDialogFocus`），不是 `window.confirm`。

## Requirements

- R1：仅当中央列表卡片（列表与网格）诊断码为 `CENTRAL_SKILL_CONTENT_CHANGED` 时显示「同步更改」按钮。其他中央诊断不显示该按钮。
- R2：点击按钮只打开确认对话框，不发 RPC。对话框问是否将当前中央文件采纳为权威内容；主操作为「是」，次操作为「取消」。Escape、关闭、「取消」均不写入并恢复触发按钮焦点。
- R3：点「是」后，用打开对话框时的 `id` 与 `row_version` 调用新命令，把当前中央目录重新校验并采纳为权威内容。不展示 Skill 正文预览，不走 `ChangePreviewDialog` / Preview-Apply。
- R4：校验合同与现有中央 inspect/导入一致：no-follow、树预算、`SKILL.md` UTF-8 与 frontmatter。校验通过后 CAS 更新 `content_hash` 与 `frontmatter_json`，将 `status` 置为 `ready`，名称与 `central_path` 不变。存在 `applying` / `restoring` / `rollback_failed` writer 时拒绝。
- R5：frontmatter `name` 与记录名不一致、类型/路径变化、缺失、解析失败、树越限、权限失败或 `row_version` 过期时拒绝；不改写中央文件，诊断保持可见。若磁盘 hash 已与记录一致且内容合法，操作为幂等成功。
- R6：成功后关闭对话框，刷新整个 Skills query family，并用成功通知说明已采纳当前中央文件、未改写工具目录链接。失败用错误通知；过期版本须重新读取列表后再试。不隐式创建 assignment，不隐式 Apply。

## Acceptance Criteria

- AC1（R1-R2）：诊断为 `CENTRAL_SKILL_CONTENT_CHANGED` 的卡片显示「同步更改」；Ready 或其他诊断不显示。点击后出现确认对话框且尚未调用采纳命令；取消或 Escape 关闭对话框、恢复焦点、仍无 RPC。
- AC2（R3-R4）：在对话框点「是」后，命令只携带该 Skill 的 `id` 与 `row_version`；返回的 DTO 为 Ready、无该诊断码，且 `content_hash` / description 与当前中央文件一致。中央目录字节与工具目录中的符号链接未被该命令改写。
- AC3（R4-R6）：已分配并 Apply 过的全局目标在刷新后不再因该诊断显示 `external_owned_change`；内容预览与「预览全局同步」恢复为现有 Ready 合同，且本次操作没有调用 `previewSkillSync` / `applySkillPreview`。
- AC4（R5）：改名、损坏的 `SKILL.md`、类型/路径变化、过期 `row_version`、活动 writer 均以稳定冲突/写入中错误拒绝；磁盘文件与旧记录保持原样，卡片仍显示原诊断（过期版本除外，刷新后以最新 inspect 为准）。
- AC5：相关 Rust 服务测试、Skills 页交互测试、生成绑定检查以及 `pnpm check` 通过。

## Out of Scope

- `CENTRAL_SKILL_MISSING` / `TYPE_CHANGED` / `PATH_CHANGED` / `INVALID` 的修复按钮。
- 从原来源重新导入或覆盖中央副本。
- 重命名 Skill、改中央目录名、批量采纳。
- 采纳前展示 `SKILL.md` 正文或文件 diff。
- MCP / Prompt 目标 readopt，以及项目原生资源禁用/恢复。
- 在本功能中提供 Skill 编辑器。
- 改写工具目录符号链接，或刷新 managed target/item 基线。

## Key Decisions

- 「同步更改」= 把当前中央目录文件采纳为新的权威内容，只更新数据库指纹与 frontmatter。
- 不从原来源复制，不改写符号链接，不走原生 Preview/Apply。
- 不要内容预览确认；要点按钮后出现是否对话框，点「是」再直接采纳。
- 目标哈希不含链接目录内容，因此不必在本操作中刷新目标基线。
