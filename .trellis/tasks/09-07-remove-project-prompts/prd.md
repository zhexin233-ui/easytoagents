# 移除项目级提示词功能

## Goal

完整移除应用中的“项目级提示词”能力，包括中央分配/同步与项目原生 Prompt 的发现/禁用/恢复，降低项目资源模型、同步流程和恢复链的复杂度；保留彼此独立的全局提示词档案与全局同步能力，并确保移除过程不删除用户项目目录中当前实际存在的提示词文件。

## Background / Confirmed Facts

- 全局提示词与项目级提示词共用 `ArtifactKind::Prompt` 和部分同步管线，但分配语义彼此独立；全局功能由 `/prompts` 页面、onboarding 和工具状态继续使用（`.trellis/spec/backend/quality-guidelines.md:430`）。
- 项目详情当前提供“提示词”资源视图，可把中央档案分配并写入项目 `CLAUDE.md`、`AGENTS.md` 或 `.cursor/rules/easytoagents.mdc`（`src/features/projects/project-detail-page.tsx:409`、`src/features/projects/project-detail-page.tsx:1285`）。
- 后端通过 `set/get_prompt_project_assignment`、带 `projectId` 的 `preview_prompt_sync` 以及 `prompt_project_assignments` 表持久化该能力（`src-tauri/src/profiles/service.rs:332`、`src-tauri/src/db/migrations/0008_prompt_project_assignments.sql:22`）。
- “项目原生资源”还会把已有项目 Prompt 文件识别为 `PromptFile`，并支持禁用、快照恢复、冲突与移除阻塞；它与中央项目分配共用同一路径（`src-tauri/src/projects/native_resources.rs:297`、`.trellis/spec/backend/quality-guidelines.md:1151`）。
- 现有合同规定解除项目分配只停止纳管并清空基线，不能删除项目文件（`.trellis/spec/backend/quality-guidelines.md:441`）。最近提交 `32034d6`、`af16a1c` 分别修复了解除后的文件保留，以及原生冲突/恢复行为。
- 历史迁移必须继续支持已经升级过的数据库；不能通过改写旧迁移来假装该功能从未存在。用户已确认项目 Prompt 的历史恢复快照可以清除。

## Requirements

- **R1 — 保留全局提示词：** 保留提示词档案 CRUD、全局工具分配、全局导入、全局预览/应用、Dashboard/工具状态和 onboarding 中的全局提示词流程。
- **R2 — 完整移除项目 Prompt：** 项目详情不再提供 Prompt 资源视图；移除中央提示词档案的项目分配/解除/预览/应用，以及项目原生 Prompt 的发现/禁用/恢复。前后端不再暴露对应 RPC、DTO、查询键、能力位或 `PromptFile` 项目资源类型。
- **R3 — 停止项目写入：** 移除后，任何全局提示词操作都不得写入已登记项目的 Prompt/Rules 文件；项目作用域不得再创建新的 Prompt `managed_targets`、分配记录或原生 Prompt 资源记录。
- **R4 — 文件与快照策略：** 升级或移除功能不得删除、清空或覆盖项目目录中当前实际存在的 `CLAUDE.md`、`AGENTS.md`、`.cursor/rules/easytoagents.mdc` 或其他用户内容；历史项目 Prompt 恢复快照及只由这些快照支撑的恢复状态允许清除，不提供迁移后恢复能力。
- **R5 — 历史数据库兼容：** 新旧数据库均可正常启动；通过新增迁移清除旧项目 Prompt 分配、项目作用域 managed target、原生资源记录及其历史快照，并移除不再需要的 schema 能力；旧记录不得继续阻止全局提示词档案管理或项目移除。历史迁移文件保持不变。
- **R6 — 同步文档与规范：** 更新 README、后端/前端质量规范、数据库规范及维护者能力矩阵中仍宣称支持项目 Prompt 的现行内容；归档任务作为历史记录保留。
- **R7 — 生成绑定一致：** Rust 命令/DTO 变化后重新生成前端 bindings，不手改生成结果作为最终来源。

## Acceptance Criteria

- [ ] **AC1 (R1):** 全局提示词页面、档案 CRUD、导入、全局分配以及全局预览/应用的现有测试继续通过。
- [ ] **AC2 (R2):** 项目详情对所有工具均不再显示“提示词”资源切换、中央项目提示词分配 UI 或原生 Prompt 管理入口，也不再调用任何项目 Prompt RPC。
- [ ] **AC3 (R2, R3):** 生成 bindings 中不存在项目提示词分配命令、DTO 或 `PromptFile` 项目资源类型；后端命令注册、service/repository/原生资源入口及能力字段同步移除。
- [ ] **AC4 (R3):** 全局 Prompt 的预览与应用只能解析全局目标；不存在可由公开命令触发的项目 Prompt 写入路径。
- [ ] **AC5 (R4, R5):** 从含既有项目 Prompt 分配、原生资源和快照的旧数据库升级后，项目目录中的现存文件字节不变；项目 Prompt 的历史分配、基线、原生资源与恢复快照被清除且不再展示为可恢复。
- [ ] **AC6 (R5):** 旧数据库升级后可删除不再被其他有效关系引用的全局提示词档案，并可按现有规则移除项目，不受遗留项目 Prompt 外键、计数或快照阻塞。
- [ ] **AC7 (R6):** 现行 README、spec 与维护者文档不再宣称支持项目级 Prompt；历史归档材料保持不变。
- [ ] **AC8 (R7):** `pnpm bindings:check`、相关前端/Rust 定向测试和 `pnpm check` 全部通过。
- [ ] **AC9:** 全仓残留检索只命中明确保留的历史迁移、归档任务或全局 Prompt 概念；每个例外都有说明。

## Out of Scope

- 移除全局提示词档案或全局 Prompt 同步。
- 删除或改写用户项目中的提示词/规则文件。
- 改写已经发布的历史迁移或归档 Trellis 任务。
- 借此重构 Provider、MCP、Skill、Hook 或通用预览/Apply 管线。

## Technical Notes

- 该任务横跨前端、Rust 服务、SQLite 迁移、生成绑定、测试和现行规范，按复杂任务处理；`design.md`、`implement.md` 与实施/检查 manifests 已补齐。
- 初步判断无需拆成父子任务：各层变化共享同一个兼容边界和端到端验收，独立启动会提高接口漂移风险。
