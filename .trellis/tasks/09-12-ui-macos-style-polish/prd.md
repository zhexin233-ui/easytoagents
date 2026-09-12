# UI 页面 macOS 风格优化：清理冗余提示语并优化布局

## Goal

EasyToAgents 是以 macOS 为主要平台的 Tauri 桌面应用，但当前界面仍是通用
Web 后台风格：侧栏/顶栏/卡片同色无层级、没有 accent 主色、每个页面标题下都
挂一段解释产品机制的长段落、卡片层层套边框、圆角/对话框宽度/字号取值离散。
本任务对前端做一次纯视觉与文案层面的整理，让界面符合 macOS 桌面应用的观感
（Apple HIG），去掉对用户没有决策价值的提示语，并统一页面布局骨架。
**不改变任何业务行为、命令调用与数据合同。**

## Background

- 现状盘点见 `research/copy-audit.md`（文案）与 `research/layout-audit.md`
  （布局与 HIG 差距清单，编号 S/H/C/D/T）。
- 外壳：`src/app/app-shell.tsx` 顶部栏（Logo + 副标题 + 工具入口胶囊）+ 左侧
  240px 导航；一级导航选中态为 `bg-primary`（近黑实底），无图标。
- 7 个功能页都用 `<main className="p-6 lg:p-8">` + 眉标 + `<h1>` + 一段
  `text-muted-foreground` 说明段落开头，说明段落均在解释"中央意图 / 持久化
  预览 / 不会直接改写原生配置"等机制，同一句意在同一页面重复出现 2–3 次。
- 样式 token 集中在 `src/styles.css`（oklch 变量、`--radius: 0.625rem`）、
  `src/components/ui/button.tsx`、`src/components/ui/dialog.tsx`，
  `src/components/central-list-layout.tsx` 负责列表/网格卡片骨架。
- 字体栈以 `Inter` 优先但项目未打包该字体；12 处硬编码 `slate-*` 色板已违反
  现行 spec 的语义 token 要求。

## Requirements

### R1 外壳（侧边栏 + 顶部栏）贴近 macOS 侧栏应用

- 侧边栏改为 macOS "source list" 观感：使用比内容区略深的 `bg-sidebar` 语义
  token，导航项为带 lucide 图标的 28px 圆角行，选中态使用 accent 浅色胶囊
  （非近黑实底）；项目子列表作为可折叠分组，去掉竖线缩进，行样式同一级导航。
- 顶部栏删除 Logo 方块、应用名与"多工具配置中枢"副标题；工具入口胶囊保留但
  选中态改为 accent 浅底 + accent 文字。
- 窗口加 `hiddenTitle: true` 隐藏原生标题栏文字；`titleBarStyle: Overlay` 与
  交通灯嵌入不纳入（未聚焦不可拖拽的已知缺陷，与上架无关，另立任务）。
- 侧边栏使用 macOS 原生毛玻璃材质（`macOSPrivateApi` + `transparent` +
  `windowEffects: sidebar`）：侧栏区域透出桌面/后方窗口，内容列保持不透明。
  材质外观必须跟随应用内的浅色/深色/系统选择（切换主题时同步窗口外观），
  不得出现"应用是浅色、侧栏材质是深色"的错位。该能力作为最后一个实现阶段，
  可独立回滚到不透明的 `bg-sidebar` token，不阻塞其余验收。
- 必须遵守 `.trellis/spec/frontend/component-guidelines.md` 的滚动所有权合同：
  `html, body` 保持 `overflow: hidden`，只有内容列滚动。

### R2 引入 macOS 视觉 token 并统一原语

- 字体栈以 `-apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui` 开头
  并移除 `Inter`；启用 `-webkit-font-smoothing: antialiased`；正文基准 13px。
- 新增 accent 语义色（light/dark 各一组，基于 macOS 系统蓝），`--primary` 与
  `--ring` 改为引用 accent；原生控件 `accent-color` 跟随。所有颜色仍通过 CSS
  变量 + `@theme inline` 暴露，状态色继续走 `toneClass`。
- 圆角收敛为三档（控件 6px / 卡片 10px / 对话框 12px）+ `rounded-full`；阴影只
  在对话框与通知浮层使用。
- `Button` 默认高 32px、小号 28px、字号 13px；新增 `ghost` 与 `icon` 变体用于
  行内图标操作。
- 对话框：`DialogContent` 增加 `size`（sm 448 / md 576 默认 / lg 768），统一
  三段式（header 固定、body 滚动、footer 固定）与最大高度；删除右上角"关闭"
  按钮（Esc 与"取消"仍可关闭，Onboarding "暂停向导"除外）；删除标题上方眉标；
  底部主按钮最右；遮罩改语义色并加轻模糊。
- 12 处硬编码 `slate-*` 色板全部替换为语义 token。

### R3 删除冗余提示语

- 删除 7 个功能页 `<h1>` 上方的眉标与下方的机制说明段落，页面头部只保留标题、
  元信息与操作按钮。
- 删除顶部栏副标题、对话框眉标、中央列表卡片尾部的重复提示、以及
  `copy-audit.md` 中判定为"删除"的区块说明。
- 保留并可精简的文案：空状态、错误/阻塞状态、脱敏/只读/需重启等**影响用户决策
  的约束说明**、确认破坏性操作的描述。`copy-audit.md` 中判定为"保留/精简"的
  条目按其建议处理。
- 被删除或改写的文案若被 vitest 用 `getByText`/`findByText`/`getByRole(name)`
  断言，同步更新测试；不得为保留测试而保留文案，不得 `it.skip`。

### R4 统一页面布局骨架

- 抽出共享 `PageHeader`（标题 22px 左对齐、可选返回按钮与元信息、操作区右对齐、
  可选 tab 行，**无描述 slot**），7 个页面（含项目详情两个早返回分支与工具档案
  页两个分支）全部接入；内容区改为左对齐固定内边距 + 最大宽度，不再居中。
- 抽出共享 `EmptyState`（图标 + 标题 + 一句说明 + 一个明确的下一步操作），替换
  现有 4 种空状态写法。
- 卡片改为 inset grouped list：section 保留一层圆角边框，内部条目用 `divide-y`
  分行（行高 ≥ 44px，hover 高亮），条目不再自带边框与圆角；`central-list-layout`
  的 list/grid 契约保持不变。
- 列表布局切换改为仅图标的 segmented control；状态徽标去掉 `✓ △ ! ○ ⛔ ×`
  字符前缀改为圆点 + 文字；设置对话框各分区改为一行一设置的列表。
- 表单：字段标签 13px、`field` 控件高 32px 与 Button 对齐、圆角为控件档。
- 深浅两套主题下所有改动均可用，不得出现 `bg-white` / `slate-*` 等裸色板类。

## Acceptance Criteria

- [ ] `research/copy-audit.md` 中判定为"删除"的文案在源码中不再出现；"精简"的
      文案已按建议改写；"保留"的文案未被误删。
- [ ] 7 个功能页均通过 `PageHeader` 渲染，`<h1>` 上方无眉标、下方无说明段落；
      顶部栏无 Logo 与副标题；`PageHeader` 组件不存在 description 属性。
- [ ] 侧边栏一级导航与项目子列表选中态使用 accent 浅色胶囊且每个一级项带图标；
      `bg-primary` 不再用于导航选中态。
- [ ] `src/styles.css` 字体栈以 `-apple-system` 开头且不含 `Inter`；accent /
      sidebar 变量在 `:root` 与 `.dark` 各有定义并通过 `@theme inline` 映射。
- [ ] `rg -n "rounded-xl|rounded-md\b|rounded\b(?![-/])|rounded-\[" src --glob '!*.test.tsx'`
      无结果；`rg -n "shadow-" src --glob '!*.test.tsx'` 只命中 `dialog.tsx` 与
      `notify.tsx`。
- [ ] `rg -n "slate-|bg-white" src --glob '!*.test.tsx'` 无结果。
- [ ] 所有对话框通过 `DialogContent size` 控制宽度，调用方无自带 `max-w-*`；
      `DialogHeader` 内无"关闭"按钮（Onboarding 除外）；主按钮位于底部最右。
- [ ] `src/components/empty-state.tsx` 存在且被 dashboard / projects / prompts /
      mcp / skills / hooks / provider-panel / native-resources 的空状态引用。
- [ ] `src-tauri/tauri.conf.json` 变化限于 `hiddenTitle`、`transparent`、
      `windowEffects`、`macOSPrivateApi`；`src-tauri/capabilities/default.json` 只新增
      窗口主题权限；`git diff --stat src-tauri` 无其他文件（无 Rust 代码改动）。
- [ ] 侧栏在 Tauri 窗口中呈现原生毛玻璃材质（拖动窗口到不同壁纸/窗口上方可见
      透出）；在设置中切换浅色 / 深色 / 系统三种偏好，侧栏材质外观与应用主题
      一致；内容列与对话框仍为不透明表面。
- [ ] `pnpm format:check`、`pnpm lint`、`pnpm typecheck`、`pnpm test --run` 全部
      通过；被删除/改写文案相关的测试断言已更新而非跳过。
- [ ] `.trellis/spec/frontend/component-guidelines.md` Theming 段已改写
      sidebar/header 的表面 token 规定（`bg-sidebar`），并补充三档圆角、accent、
      `PageHeader` 无描述 slot 的合同。
- [ ] 手动验收：在 Tauri 窗口（非浏览器）切换全部页面与深浅主题；打开至少
      FormDialog、ChangePreviewDialog、ProjectRemoveDialog 各一次；触控板滚动
      长列表页时外壳不被整体顶出视口。

## Constraints

- 纯前端视觉/文案改动：不新增或修改任何 `commands.*` 调用、query key、DTO 用法、
  mutation 逻辑、对话框开合状态机、路由。唯一的行为性新增是主题切换时同步窗口
  外观（`@tauri-apps/api/window` 的 `setTheme`），且只在 Tauri 运行时调用，
  vitest 环境下静默跳过。
- 本应用不上架 Mac App Store（用户决定，2026-09-12），因此允许启用
  `macOSPrivateApi`。
- 遵循 `.trellis/spec/frontend/component-guidelines.md`：语义 token、`toneClass`、
  类驱动 dark 变体、滚动所有权、`main.tsx` 无闪烁主题引导、`NavLink className`
  不包 `cn(...)`、品牌图标资产不替换、`FormDialog` 契约不变。
- 不引入新的 UI 依赖库；继续使用 Tailwind v4 + CVA + 现有原语与 lucide-react。
- 文案改动只删/精简，不新增解释性长文；所有文案仍为简体中文。
- 一次性完成 7 个页面，不允许"部分页面新头部、部分页面旧头部"的中间状态被合入。

## Out of Scope

- 信息架构调整（路由、页面拆分合并、导航项增减）。
- 交互流程变更（预览/Apply 流程、导入向导步骤、Onboarding 步骤条重绘）。
- 行为语义变更：`globalThis.confirm` 替换为自定义对话框、破坏性按钮 `destructive`
  变体、表单两列 inline 布局、设置对话框左侧 tab 重构。
- 原生标题栏 Overlay / 交通灯嵌入 / 窗口 `backgroundColor`。
- 内容列、对话框、通知浮层的毛玻璃材质（只做侧栏）。

## Decisions

- 2026-09-12 用户确认：不做实现，只规划；范围外事项（`globalThis.confirm`
  替换、破坏性按钮变体、两列表单、设置对话框 tab）按推荐保持范围外；删除对话框
  右上"关闭"按钮按推荐纳入；不上架 App Store，因此侧栏原生材质纳入。
- 图标系统整体替换（沿用 lucide-react；只为侧边栏导航与空状态补图标）。
- Windows / Linux 平台的原生风格适配；国际化 / i18n 基础设施。
