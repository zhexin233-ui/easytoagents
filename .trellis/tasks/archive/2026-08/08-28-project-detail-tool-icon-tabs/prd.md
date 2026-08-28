# 项目详情工具类型图标切换

## Goal

让用户在项目详情页通过清晰的按钮分别选择资源类型与目标平台，页面只显示当前选择的 MCP/Skill 与 Claude/Codex 组合，避免同时展示两个平台造成的信息拥挤。

## Background

- 项目详情页当前使用 MCP、Skill 两个按钮切换资源类型，默认显示 MCP（`src/features/projects/project-detail-page.tsx:29-35,171-195`）。
- 页面当前同时渲染 Claude 与 Codex 两个平台的追加区域（`src/features/projects/project-detail-page.tsx:200-226`）。
- Claude 与 Codex 已有官方品牌资源，并由现有平台按钮组件使用（`src/components/platform-assignment-button.tsx:1-50`、`src/assets/brand/README.md:3-9`）。
- 现有测试已覆盖 MCP/Skill 默认状态、双向切换、可访问状态和内容显隐（`src/features/projects/project-detail-page.test.tsx:321-368`）。

## Requirements

1. 保留 MCP/Skill 资源类型切换；默认仍选择 MCP。
2. 在项目详情页增加 Claude/Codex 平台切换按钮；默认选择 Claude。
3. Claude/Codex 按钮使用各自现有品牌图标，不重绘、不改色、不优化品牌资源。
4. 两组按钮都应明确表达当前选中状态，并保留可访问名称与 `aria-pressed` 状态。
5. 内容区只渲染当前平台和当前资源类型对应的一个追加区域：Claude MCP、Claude Skill、Codex MCP 或 Codex Skill。
6. 切换资源类型时继续沿用当前行为，重置与上一资源视图相关的 Git exclude 临时选择；切换平台时不得错误沿用另一平台的临时选择或操作状态。
7. 沿用现有组件、按钮样式与品牌资源，不引入新的图标依赖。

## Acceptance Criteria

- [x] 首次进入项目详情页时，MCP 与 Claude 按钮处于选中状态，只显示 Claude 的 MCP 项目追加区域。
- [x] 点击 Skill 后只显示 Claude 的 Skill 项目追加区域，MCP 内容不再显示。
- [x] 点击 Codex 后只显示 Codex 与当前资源类型对应的追加区域，Claude 内容不再显示；可再次切回 Claude。
- [x] Claude 与 Codex 按钮分别显示仓库中的 Claude/Codex 品牌图标，并具有可访问名称和正确的 `aria-pressed` 值。
- [x] MCP/Skill 与 Claude/Codex 两组切换可任意组合，内容标题、查询参数与操作目标都对应当前组合。
- [x] 切换资源类型或平台不会把上一个组合的 Git exclude 临时状态泄漏到新组合。
- [x] 项目目标阻断、空数据提示、预览与应用流程在四种组合下保持现有业务语义。
- [x] 相关前端测试通过，并覆盖默认选择、两组切换、内容显隐、平台图标及可访问状态。

## Out of Scope

- 修改项目详情路由、后端命令或数据结构。
- 修改全局 MCP、全局 Skills 页的平台分配交互。
- 重绘或替换 Claude/Codex 品牌资源。
- 为 MCP/Skill 新增品牌图标；本次仅要求 Claude/Codex 使用相应图标。

## Key Decisions

- 资源类型与目标平台是两个独立的单选切换组，而不是四个扁平组合按钮。
- 平台按钮表达“当前查看的平台”，不复用 `PlatformAssignmentButton` 的“是否已分配”语义；实现时可复用其品牌资源与视觉规则。
- 本任务仅涉及单个前端页面及其测试，按轻量任务处理，PRD-only。

## Risks and Deferred Items

- 现有测试的查询次数基于 Claude/Codex 同时挂载；改为单平台渲染后需要同步调整断言。
- 品牌图标是装饰性内容，应由按钮本身提供可访问名称，避免屏幕阅读器重复朗读。
