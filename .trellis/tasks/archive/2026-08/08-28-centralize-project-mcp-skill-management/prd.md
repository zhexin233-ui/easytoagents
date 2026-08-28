# 集中管理项目 MCP 与 Skill

## Goal

将 MCP 与 Skill 的项目级追加/分配入口统一收口到项目管理中，让用户先选择项目，再在单个项目内切换管理 MCP 与 Skill，避免同一项能力散落在多个资源页面。

## Background

- 当前 MCP 页面和 Skill 页面各自包含项目与工具选择器、项目级 assignment、项目预览入口，和项目详情页已经具备的能力重复。
- 项目列表已通过 `/projects/:projectId` 为每个项目提供固定上下文的管理页面；该页面已包含 Claude/Codex 两列，以及完整的 MCP/Skill assignment、预览、Apply、信任/策略阻断逻辑。
- 现有后端命令与 DTO 已完整支持项目级 MCP/Skill 管理，本次不需要增加或修改后端契约。

## Requirements

- MCP 页面不再显示或承担项目追加/分配选择器。
- Skill 页面不再显示或承担项目追加/分配选择器。
- 每个项目的详情管理界面提供 MCP 与 Skill 两个可切换的管理视图，默认展示 MCP。
- 切换视图后仍按 Claude/Codex 两个工具分别管理当前资源类型。
- 项目级 MCP/Skill 变更继续使用现有的数据与命令能力，不改变全局资源的导入、编辑或删除语义。
- 全局继承项继续保持选中且只读；项目项只能追加或移除项目自己的选择。
- 项目 assignment 成功后继续同时刷新项目、MCP、Skill 三组查询，避免共享项目行版本失效。
- 项目原生配置写入继续严格经过持久化预览与显式 Apply，不允许 assignment 后隐式写入。

## Acceptance Criteria

- [ ] 访问 MCP 页面时，不再出现项目追加选择器，现有非项目级功能仍可使用。
- [ ] 访问 Skill 页面时，不再出现项目追加选择器，现有非项目级功能仍可使用。
- [ ] 从项目页面进入任意项目管理后，可以在 MCP 与 Skill 管理视图之间切换。
- [ ] 项目详情默认展示 MCP 管理；切换到 Skill 后只展示 Skill 管理，再切回 MCP 可恢复 MCP 管理视图。
- [ ] MCP 与 Skill 视图均同时保留 Claude/Codex 工具列、继承只读语义、项目追加/移除、阻断提示、Git exclude 选项、预览和显式 Apply。
- [ ] 在项目管理内对 MCP 或 Skill 执行追加/移除后，项目、MCP、Skill 查询均刷新，界面使用最新项目行版本。
- [ ] MCP 与 Skill 中央页面的全局 CRUD、导入、启停、平台分配、全局状态、全局预览与 Apply 保持可用。
- [ ] 页面测试覆盖中央页入口移除、项目详情默认视图与切换、MCP/Skill 项目级变更及预览 Apply 行为。

## Out of Scope

- 重做 MCP 或 Skill 的全局导入、编辑、删除流程。
- 改变项目发现、创建或删除机制。
- 修改底层 MCP/Skill 配置格式。
- 新增项目详情子路由、后端命令或数据库迁移。

## Technical Notes

- 切换控件使用页面内本地状态和原生按钮语义，暴露可查询的 accessible name 与 `aria-pressed`，不引入新的全局状态或路由状态。
- 继续复用 `ProjectMcpAssignments`、`ProjectSkillAssignments`、`AssignmentCard` 与 `ChangePreviewDialog`。
- 相关规范：`.trellis/spec/frontend/component-guidelines.md`、`.trellis/spec/frontend/state-management.md`、`.trellis/spec/frontend/quality-guidelines.md`。
