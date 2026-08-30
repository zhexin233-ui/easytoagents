# 提示词页中央列表改版与项目级隐藏全局生效档案

## Goal

1. **项目级提示词**：全局生效的提示词档案不再出现在项目详情页的可分配列表中，后端同时拒绝把全局生效档案分配到项目（防止同一份指令内容同时写入全局文件与项目根文件造成重复）。
2. **提示词页面改版**：全局提示词页改为参考 Skills 页的「中央列表」形态（`CentralList` + 单列/三列布局切换 + 卡片），每份档案用图标按钮选中生效，同一工具同时只有一份生效。

## Background（已核实事实）

- 每工具恰好一份 `is_active = 1` 的提示词档案（`uq_prompt_profiles_one_active_per_tool`）；`set_active_prompt_profile` 已保证切换时互斥（`src-tauri/src/db/profiles.rs:721`）。
- 项目分配链路 `set_prompt_project_assignment`（`src-tauri/src/profiles/service.rs:313`、`src-tauri/src/db/profiles.rs:515`）目前允许分配任意同工具档案，**包括全局生效档案**；项目详情页 `ProjectPromptAssignments`（`src/features/projects/project-detail-page.tsx:702`）展示全部档案并把生效档案标注「· 全局生效」。
- Skills 页（`src/features/skills/skills-page.tsx`）的中央列表形态：`CentralList` / `CentralListCard` / `CentralListCardBody` / `CentralListCardFooter` / `CentralListLayoutToggle` + `usePersistedCentralListLayout("skills")`；图标按钮范式见 `PlatformAssignmentButton`（size-8 图标 + aria-pressed + 亮暗色变体成对）。
- `usePersistedCentralListLayout` 目前只有 `mcp` / `skills` 两个存储键，需要新增 `prompts` 键。
- 既有交互合同：激活/切换生效走 `set_active_prompt_profile` → `previewPromptSync` → 预览确认（或直接应用模式自动 Apply）；CRUD 不隐式 Apply；新增/编辑用 `FormDialog`。

## Requirements

- **R1 项目级隐藏**：项目详情页提示词分配列表只展示**未全局生效**的档案；全局生效档案不展示、不可再次分配。
  - 边界：若某档案**已被本项目分配**后又变为全局生效（历史数据/先分配后激活），它仍需保留在列表中并标注「当前分配」，保证可解除分配；只是不再提供「分配到此项目」入口。
- **R2 后端守卫**：`set_prompt_project_assignment` 在分配（非解除）时校验目标档案 `is_active`；全局生效档案返回冲突错误，文案说明需先切换全局生效档案。
- **R3 中央列表改版**：`PromptsPage` 采用 Skills 页同款结构：页头 + aria-live 消息区 + 「中央列表」区（布局切换 + 新增/检测导入/同步预览按钮 + 档案卡片）+ 工具状态区。
- **R4 图标选中生效**：每张档案卡片提供一个图标按钮用于「设为当前生效」（复用 PlatformAssignmentButton 的图标按钮范式，check 图标），当前生效档案呈选中态；同一工具只有一个生效（数据层已保证，UI 呈现选中/未选中）。
- **R5 行为保持**：新增/编辑/删除/检测导入/FormDialog/预览-应用链路、工具页签（Claude/Codex）与状态提示全部保留；布局偏好持久化到独立存储键 `prompts`。

## Acceptance Criteria

- [ ] **AC1** 项目提示词列表不出现未分配的全局生效档案；后端对激活档案的分配请求返回冲突错误（含测试）。
- [ ] **AC2** 已分配且全局生效的档案在项目列表保留「当前分配」展示，可解除分配。
- [ ] **AC3** 提示词页为中央列表形态：单列/三列切换持久化（独立键），卡片含名称、正文预览、编辑/删除操作与图标选中生效按钮。
- [ ] **AC4** 图标按钮点击走 `setActivePromptProfile` → 预览确认链路；当前生效档案图标呈选中态且不可重复激活。
- [ ] **AC5** `pnpm check` 全绿；`cargo test` 通过。

## Out of Scope

- Skills/MCP 页面行为不变（仅复用共享组件）。
- 不改变全局同步/项目同步的后端投影与预览语义。
- 不新增「项目内新建档案」入口。
