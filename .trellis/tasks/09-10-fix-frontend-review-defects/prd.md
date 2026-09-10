# 修复前端审阅缺陷

父任务：`09-10-codebase-optimization`。覆盖审阅条目 A1、A2、A3、A7、A8（见父任务 `research/review-findings.md`）。

## 目标
修复已核实的五处前端缺陷，并通过编译期约束防止同类"手抄漏一处"再次发生。

## 需求
- R1 / A1：`project-detail-page.tsx` 的 `artifactLabel` 覆盖全部 `ArtifactKind`（含 `hook`），改为 `Record<ArtifactKind, string>` 形式，缺项即编译错误；`tsconfig.json` 开启 `noImplicitReturns`，并修复因此暴露的其它函数。
- R2 / A2：Hooks 页 `activeTool` 在渲染期按 `visibleTools` 夹逼，写法与 `project-detail-page.tsx:96-100` 一致；关闭某工具后目标状态、导入入口与事件分组都切到第一个启用工具。
- R3 / A3：Prompts 页 `applyMutation.onSuccess` 与其它三页一致地刷新数据。
- R4 / A7：`project-detail-page.tsx` 的 `window.location.assign` 与 `tool-profiles-page.tsx` 的 `<a href="#/prompts">` 改用 react-router 的 `useNavigate` / `<Link>`。
- R5 / A8：`mcp-page.tsx` 的 `SensitiveField` 通过显式 `id` prop 或 `useId()` 关联 label，不再依赖 label 文案推断。

## 验收条件
- A1：项目详情页测试夹具加入 `artifactKind: "hook"` 目标，断言渲染出 `Hooks` 标签。
- A2：Hooks 页测试：关闭 claude 后默认展示第一个启用工具的状态区，且不出现 Claude 的导入按钮。
- A3：Prompts 页测试：应用成功后相关查询被重新拉取。
- A4：搜索 `window.location.assign` 与 `href="#/` 在 `src/` 中为零命中；相关跳转测试改用 `MemoryRouter` 断言路由变化。
- A5：`pnpm check` 全绿，`noImplicitReturns` 已开启且无新增 `eslint-disable`。

## 范围外
页面结构重构、通用组件抽取（由 `09-10-frontend-dedup-and-split` 负责）。
