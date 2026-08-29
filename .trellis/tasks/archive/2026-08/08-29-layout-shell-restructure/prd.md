# 重构应用整体布局：顶栏工具切换 + 侧边栏项目导航

## Goal

按用户要求重构 EasyToAgents 桌面前端的整体布局与页面设计：

- Claude / Codex 从左侧一级导航移到**右上角**（顶栏右侧的工具入口）。
- 左侧边栏保留：总览 / MCP / 项目（Skills 保留为侧边栏导航项）。
- 「项目」在侧边栏中带**子项**：每个已登记项目是一个可点击的子项，直达项目详情页（快速进入）。
- 整体前端页面设计随之统一调整（外壳、页面容器、视觉细节）。

## Requirements

- R1 顶栏（top bar）：左侧为应用标识，右侧为 Claude 与 Codex 两个工具入口（带品牌图标），点击分别进入 `/claude`、`/codex` 工具配置页；当前路由命中时高亮。
- R2 侧边栏（sidebar）：主导航为 总览（`/`）、MCP（`/mcp`）、项目（`/projects`）；Skills（`/skills`）保留为导航项，不删除该功能页。
- R3 侧边栏「项目」节点展开显示已登记项目列表（来自 `listProjects` 查询），每个子项链接到 `/projects/:projectId`；需要处理加载中 / 空列表 / 加载失败状态，且不阻塞主导航。
- R4 内容区改为由外壳统一管理的滚动区域；各页面移除 `min-h-screen` 双层滚动问题，统一页面容器与页头样式。
- R5 不改变任何业务逻辑、命令绑定（bindings）、预览/应用流程与路由路径（`/claude`、`/codex`、`/mcp`、`/skills`、`/projects`、`/projects/:projectId`、`/` 全部保留）。
- R6 现有测试全部通过；如有因结构调整失效的断言，按新结构修正。

## Acceptance Criteria

- [x] 顶栏右上角可见 Claude / Codex 两个入口（含图标），可导航且路由激活态正确。
- [x] 侧边栏一级项为 总览 / MCP / Skills / 项目，且「项目」下能列出所有已登记项目并可点击直达详情页。
- [x] 无已登记项目时侧边栏项目区显示空态提示；列表加载失败时显示非阻断性错误但不影响导航。
- [x] 所有页面在新外壳下无双层滚动、宽度与页头样式一致。
- [x] `pnpm lint`、`pnpm typecheck`、`pnpm test --run`、`pnpm format:check` 通过。

## Verification Notes (2026-08-29)

- 浏览器 + 注入 mock 数据完成视觉验证：顶栏工具激活态、侧边栏项目子项高亮、折叠/自动展开、非阻断错误态均符合预期。
- 过程中发现并修复：`cn()` 包住 NavLink 的 className 函数会静默丢弃 isActive 样式（clsx 不调用函数参数），已记入 frontend/component-guidelines.md Common Mistakes。

## Notes

- 纯前端改动，不触碰 `src-tauri`。
- 语言：界面文案继续使用简体中文。
