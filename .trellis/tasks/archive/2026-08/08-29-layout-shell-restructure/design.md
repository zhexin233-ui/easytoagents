# Design: 布局外壳重构

## 现状

- `src/app/app-shell.tsx`：单层左侧 220px 侧边栏，一级导航为 总览/Claude/Codex/MCP/Skills/项目 平铺；`<Outlet />` 直接撑起页面，每页自己 `min-h-screen`。
- Claude/Codex 是路由页 `/claude`、`/codex`（`ToolProfilesPage`），同时也是一级导航项。
- 项目列表在 `ProjectsPage` 内展示，进入单个项目需先到 `/projects` 再点「打开详情」。

## 目标结构

```
┌────────────────────────────────────────────────────────┐
│ top bar: [品牌 EasyToAgents]      [Claude] [Codex]     │  ← 右上角工具入口
├──────────────┬─────────────────────────────────────────┤
│ sidebar      │  <Outlet /> 页面内容（独立滚动）           │
│  总览         │                                         │
│  MCP         │                                         │
│  Skills      │                                         │
│  项目 ▾       │                                         │
│   ├ 项目 A    │                                         │
│   ├ 项目 B    │                                         │
│   └ …        │                                         │
└──────────────┴─────────────────────────────────────────┘
```

## 组件设计

### `src/app/app-shell.tsx`（重写）

- 外层：`flex h-screen overflow-hidden`，`lg` 以下退化为纵向堆叠（顶栏 + 横向滚动/堆叠导航），保证小窗口可用。
- **TopBar**（内联组件或同文件子组件）：
  - 左：应用名 `EasyToAgents` + 副标语（`Claude · Codex 配置中枢`）。
  - 右：两个 `NavLink`（`/claude`、`/codex`），使用 `src/assets/brand/claude-icon-square.svg` 与 `codex-icon-light.png` 图标 + 文本；激活态用 `active` 样式（`bg-primary text-primary-foreground` 或描边高亮）。
- **Sidebar**：
  - 一级导航：总览 `/`（end）、MCP `/mcp`、Skills `/skills`、项目 `/projects`。
  - 项目子项：`useQuery(projectsQueryOptions())` 在 shell 层拉取（React Query 已有全局 provider，缓存键与 ProjectsPage 复用 `projectKeys.list()`，页面内修改会自动同步）。
  - 子项用 `NavLink` → `/projects/:id`，`isActive` 高亮；激活态样式与一级项区分（更轻的底色 + 左侧缩进）。
  - 状态处理：`isPending` → 显示占位行「正在读取项目…」；空 → 「暂无已登记项目」；`isError` → 灰字提示 + 保持可点击的「项目」一级项，不抛 BlockingState（侧边栏是非关键路径）。
  - 可折叠：`<details>` 或受控 state；默认展开（子项即核心快速入口）。用受控 state + chevron 旋转，项目路由激活时强制展开。
- 侧边栏宽度 `w-60`（240px），`border-r bg-white`。

### 路由（`router.tsx`）

- 路径与元素全部不变；仅 `AppShell` 内部结构变化。

### 页面容器统一（R4）

- 新外壳下 `<Outlet />` 位于 `flex-1 overflow-y-auto` 容器内，页面自身不再需要 `min-h-screen`。
- 各页面 root `<main>` 统一为：`p-6 lg:p-8`（去掉 `min-h-screen` 与 `p-5 sm:p-8` 的不一致），内部保持 `mx-auto max-w-6xl` 节奏。
- 页头统一模式：kicker（`text-muted-foreground text-sm`）+ `h1 text-2xl font-semibold` + 描述；项目详情页保留返回链接。

### 样式（`styles.css`）

- 保持 light 主题 token 不变；必要时微调（如侧边栏底色），不引入新的主题体系。

## 兼容性与风险

- Tests：`dashboard-page.test` 等以 role/text 查询，容器 class 改动不影响；`app-shell` 无既有测试。`ProjectsPage` 测试中的交互不受影响。
- React Query 缓存复用：shell 与页面共享 `projectKeys.list()`，登记/移除项目后 `invalidateQueries(projectKeys.all)` 会让侧边栏同步刷新——这是行为增强而非回归。
- codex-icon-light.png 是深色底用的浅色图标；在浅色顶栏上可能对比不足，改为放在圆形/深色小底衬里，或只用 SVG claude 图标 + 文字。实现时检查实际视觉效果。

## 回滚

- 单 commit 前端改动；回滚 = revert 该 commit。
