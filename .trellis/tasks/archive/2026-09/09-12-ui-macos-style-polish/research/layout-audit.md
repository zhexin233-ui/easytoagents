# Research: 页面布局与 macOS 风格（Apple HIG）差距审计

- **Query**: 审计当前外壳/页面/原语/排版色彩与 macOS HIG 的差距，产出"当前做法 → 建议做法 → 涉及文件"清单，并列出 spec 硬约束
- **Scope**: mixed（以内部代码审计为主；Tauri 窗口能力来自本地 `node_modules/@tauri-apps/cli/config.schema.json` v2.11.4）
- **Date**: 2026-09-12
- **调研方法**: 逐行阅读 `src/app/app-shell.tsx`、`src/styles.css`、`src/components/**`、各 `*-page.tsx`、`src/features/projects/detail/**`、`settings-dialog.tsx`、`onboarding-wizard.tsx` 及全部 `*-dialog.tsx`（未读测试文件）；用 Python 对 45 个非测试 `.tsx` 做 Tailwind 类频次统计。

---

## 0. 结论速览（供主 Agent 直接决策）

1. **窗口层完全是浏览器默认**：`tauri.conf.json` 只有尺寸，没有 `titleBarStyle`/`hiddenTitle`/`trafficLightPosition`/`windowEffects`；`src/` 中 0 处 `data-tauri-drag-region`、0 处 `backdrop-blur`。所以当前是"原生标题栏 + 网页式顶栏 + 网页式侧栏"三层叠加。
2. **外壳三块面板同色**：侧栏、顶栏、对话框、页面卡片全部 `bg-card`；`--background`(0.985) 与 `--card`(1.0) 亮度几乎相同，没有 macOS 侧栏 vs 内容区的明暗层级。
3. **没有 accent 色**：`--primary` 是中性深灰/近白，`--ring` 是灰色；macOS 的系统蓝（选中、主按钮、焦点环）在本项目中不存在，只有 `--info` 是蓝色但仅用于状态徽章。
4. **卡片嵌套过深**：页面 section (`rounded-xl border p-5`) → 子卡片 (`rounded-lg border p-4`) → 行 (`rounded/rounded-lg border p-3`) → `pre bg-muted`，最深 4 层边框；全项目 0 处 `divide-y`，列表行全部是独立带边框卡片。
5. **圆角 6 档并存**：`rounded`(4px, 18 处)、`rounded-[4px]`(2)、`rounded-md`(8px, 6)、`rounded-lg`(10px, 50)、`rounded-xl`(12px, 20)、`rounded-full`(7)。
6. **对话框宽/高不统一**：宽度 6 档（md/lg/2xl/3xl 默认/4xl），最大高度 3 种写法（`88vh`/`90vh`/`calc(100dvh-2rem)`），两种结构（整体 `p-6` 内滚 vs `p-0` 三段式）。底部按钮已是"主按钮在右"，符合 macOS；但头部右上角的"关闭"按钮与底部"取消"重复。
7. **字体栈首位是 `Inter`**，但项目未打包该字体（无 `@font-face`、CSP `font-src 'self'`），实际在 macOS 上大多回落到 `system-ui`(SF Pro)；若用户本机装了 Inter 则会呈现非系统字体。
8. **提示语极多且样式单一**：`text-muted-foreground text-sm/xs` 说明段落遍布页头、卡片头、卡片尾、表单描述、空状态，多为流程性解释（"只更新中央意图…仍需预览后 Apply"），同一句意在同一页面重复出现 2–3 次。删改文案需注意 26 个测试文件、62 处 `getByText(...)` 依赖可见文案。

---

## 1. 外壳与导航（当前结构）

### 1.1 Tauri 窗口配置

`src-tauri/tauri.conf.json` → `app.windows[0]`：

```json
{ "label": "main", "title": "EasyToAgents", "width": 1200, "height": 760,
  "minWidth": 960, "minHeight": 640, "resizable": true, "fullscreen": false }
```

- 未设置：`titleBarStyle`（默认 `Visible`，原生标题栏显示文字 "EasyToAgents"）、`hiddenTitle`、`trafficLightPosition`、`transparent`、`windowEffects`、`theme`、`backgroundColor`、`decorations`。
- `app.macOSPrivateApi` 未启用。
- `src-tauri/capabilities/default.json` 权限：`["core:default", "dialog:allow-open"]`。
- 本地 schema（`node_modules/@tauri-apps/cli/config.schema.json`，CLI 2.11.4）核实到的可用项：
  - `titleBarStyle`: `Visible` | `Transparent`（标题栏透明、显示窗口背景色，避免 Overlay 的坑）| `Overlay`（标题栏覆盖在内容上；需要自定义拖拽区域；已知限制：窗口未聚焦时无法拖拽，tauri-apps/tauri#4316；标题栏高度随 macOS 版本不同）。
  - `hiddenTitle: true` 隐藏 macOS 标题文字。
  - `trafficLightPosition: {x,y}` 需要 `titleBarStyle: Overlay` + `decorations: true`。
  - `windowEffects.effects` 含 macOS 材质 `sidebar` / `headerView` / `windowBackground` / `underWindowBackground` / `contentBackground` 等；**要求窗口 `transparent: true`**，而 macOS 上 `transparent` 需要 `macOSPrivateApi`（schema 警告：使用私有 API 会导致无法上架 App Store）。
- `src/` 内 0 处 `data-tauri-drag-region`（grep 确认）。

### 1.2 AppShell 结构（`src/app/app-shell.tsx`）

```
<div class="flex h-screen flex-col overflow-hidden">        // :73
  <TopBar/>                                                  // :74  header h-14 bg-card border-b
  <div class="flex min-h-0 flex-1">                          // :75
    <aside class="bg-card flex w-60 shrink-0 flex-col border-r">   // :76  侧栏 240px
      <nav class="min-h-0 flex-1 space-y-1 overflow-y-auto px-3 py-4">  // :79
      <div class="border-t px-3 py-3"> 设置按钮 </div>       // :98-108
    </aside>
    <div class="min-w-0 flex-1 overflow-y-auto"> <Outlet/> </div>  // :110  唯一滚动列
  </div>
  <SettingsDialog/>                                           // :116
</div>
```

| 元素 | 取值 | 位置 |
|---|---|---|
| 侧栏宽度 | `w-60`（240px） | app-shell.tsx:76 |
| 侧栏背景 | `bg-card` + `border-r` | :76 |
| 顶栏高度/背景 | `h-14`（56px）`bg-card border-b px-4 lg:px-6` | :167 |
| 顶栏左侧 | 品牌块：`size-7 rounded-lg bg-primary` 内 "EA" 字母 + `text-sm font-semibold` 应用名 + `text-[11px]` 副标题"多工具配置中枢" | :169-178 |
| 顶栏右侧 | 工具入口 `NavLink`：`rounded-full border px-3 py-1.5 text-sm font-medium`，选中 `border-primary-foreground bg-primary text-primary-foreground shadow-sm`，含 `size-4 rounded-[4px]` 品牌图标 | :181-204 |
| 一级导航项 | `flex-1 rounded-md px-3 py-2 text-sm font-medium`；选中 `bg-primary text-primary-foreground`（实心深色胶囊）；未选中 `text-muted-foreground hover:bg-muted` | :42-48 |
| 一级导航图标 | **无**，纯文字：总览 / 提示词 / MCP / Hooks / Skills / 项目 | :34-40, :388-395 |
| 项目折叠按钮 | 自绘 SVG chevron `size-6 rounded`，`rotate-90` 动画 | :396-419 |
| 项目子列表 | `mt-1 ml-3 space-y-0.5 border-l pl-3`（左侧竖线缩进）；项 `rounded-md px-2 py-1.5 text-sm`，选中 `bg-muted text-foreground font-medium` | :422, :450-453 |
| 项目行悬浮操作 | 编辑/移除 `Button size=sm variant=outline` 被覆盖成透明无边框 `size-8`、`opacity-0` → hover/focus-within 显示 | :50-51, :460-501 |
| 设置入口 | 侧栏底部 `border-t` 分隔，自绘齿轮 SVG `size-4` + "设置" 文本 | :98-151 |
| 页面加载态 | `<p role="status" class="p-6 text-sm">正在加载页面…</p>` | :127-133 |

观察：
- 顶栏品牌块与原生标题栏文字 "EasyToAgents" 双重展示。
- 顶栏、侧栏、内容区、卡片背景均为 `bg-card`，页面 `--background` 仅比 `--card` 暗 1.5%（light: 0.985 vs 1.0；dark: 0.141 vs 0.21）。
- 选中态在 light 模式为深灰底白字（`--primary` oklch 0.278），dark 模式为白底深字。
- 图标来源混用：lucide（Pencil/Trash2）+ 4 个文件共 8 处手写 `<svg>`（app-shell 2、settings-dialog 3、central-list-layout 2、projects/detail/page 1）。

---

## 2. 页面结构模式

### 2.1 页面头部（所有 `*-page.tsx`）

统一骨架（11 处 `<main className="p-6 lg:p-8">`）：

```tsx
<main className="p-6 lg:p-8">
  <header className="mx-auto max-w-6xl">
    <p className="text-muted-foreground text-sm [font-medium]">眉标</p>   // 眉标文案：总览 / 提示词 / 中央配置库 / 项目 / 工具配置
    <h1 className="mt-1 text-2xl font-semibold">标题</h1>               // 7 处完全一致
    <p className="text-muted-foreground mt-2 [max-w-3xl] text-sm [leading-6]">一段流程说明</p>
  </header>
```

差异点：
- 眉标 `font-medium` 只在 dashboard（:35）、projects（:74）有；prompts/mcp/tool-profiles 没有。
- 描述段 `max-w-3xl leading-6` 只在 prompts（:302）、mcp（:265）、tool-profiles（:89）有。
- **只有 dashboard 头部有右侧操作区**（`flex flex-wrap items-start justify-between gap-4`，dashboard-page.tsx:33-50）；其余页面的主操作（新增 MCP/新增提示词/导入）都下沉到第一张 section 卡片的标题行（如 mcp-page.tsx:277-290、prompts-page.tsx:312-330、skills-page.tsx:232-248）。
- 项目详情页头部另一种结构：`<Link className="text-sm underline">← 返回项目列表</Link>` + `h1 mt-4` + `code` 路径 + 状态文本（projects/detail/page.tsx:174-184）。
- 页面级 section 标题 `h2` 有两档：`text-lg font-semibold`（12 处，多数页面 section）与 `text-xl font-semibold`（prompts 中央列表 :314、provider-panel :291、项目详情"项目原生资源" native-resources.tsx:83、"项目追加" page.tsx:371）。
- `h3` 三档：`font-semibold`（8）、`font-medium`（4）、`text-sm font-semibold text-slate-500 dark:text-slate-400`（hooks-page.tsx:532；hook.tsx:151 为 `h4 text-xs`）。

### 2.2 卡片层级与取值分布（45 个非测试 tsx 统计）

| 类别 | 取值分布（次数） | 典型用法 |
|---|---|---|
| 圆角 | `rounded-lg` 50 · `rounded-xl` 20 · `rounded` 18 · `rounded-full` 7 · `rounded-md` 6 · `rounded-[4px]` 2 | xl=页面 section 与对话框；lg=子卡片/行/空状态；`rounded`=pre/代码块/小按钮；md=Button/导航项 |
| 圆角实际像素 | `--radius: 0.625rem`（styles.css:61）→ `rounded-sm`=6px、`rounded-md`=8px、`rounded-lg`=10px（@theme 覆盖 :27-29）；`rounded-xl`=Tailwind 默认 12px；`rounded`=Tailwind 默认 4px | |
| 阴影 | `shadow-xl` 1（DialogContent）· `shadow-lg` 1（Notify）· `shadow-sm` 2（顶栏选中胶囊、ToolIconToggle 激活）· `shadow-none` 4 | 页面卡片全部无阴影、只靠 1px 边框 |
| 内边距 p-N | `p-4` 41 · `p-6` 23 · `p-5` 20 · `p-0` 18 · `p-3` 15 · `p-8` 12 · `p-2` 5 · `p-1` 1 · `p-0.5` 1 | section=p-5；子卡片=p-4；行=p-3；对话框=p-6 |
| gap | `gap-2` 54 · `gap-3` 45 · `gap-4` 10 · `gap-1.5` 5 · `gap-2.5` 2 · `gap-1` 2 · `gap-0.5` 1 | |
| space-y | `space-y-2` 17 · `space-y-3` 15 · `space-y-4` 9 · `space-y-1` 4 · `space-y-5` 2 · `space-y-0.5` 2 | |
| 纵向堆叠 | 大量 `mt-1/2/3/4/5/6` 逐元素写在子节点上（页面级 `mt-6`、卡内 `mt-3/4`、文字 `mt-1/2`） | 而非父级 gap |
| 字号 | `text-sm` 183 · `text-xs` 131 · `text-xl` 20 · `text-lg` 13 · `text-2xl` 8 · `text-3xl` 2 · `text-[11px]` 1 | 正文默认 16px 未显式出现，输入框继承 16px |
| 字重 | `font-medium` 68 · `font-semibold` 59 · `font-bold` 1 | |
| 容器宽 | `max-w-6xl` 32（所有页面内容列 1152px 居中）· `max-w-3xl` 8 · `max-w-2xl` 6 · `max-w-4xl` 1 · `max-w-lg` 1 · `max-w-md` 1 | |
| 分隔线列表 | `divide-y` **0** | 所有列表行均为独立边框卡片 |
| 虚线空状态 | `border-dashed` 5 | dashboard :82、projects :173、prompts :343、provider-panel :332、native-resources :111 |
| 硬编码调色板 | `bg-slate-50` 4 · `bg-slate-900/60` 3 · `bg-slate-900` 2 · `text-slate-500/400` 各 2 · `border-slate-200/300/600/700` · `bg-slate-950/40` 1（DialogOverlay） | hooks-page.tsx:532,581；hook.tsx:151,194；native-resources.tsx:300；tool-icon-toggle.tsx:31-32；prompts-page.tsx:465；provider-panel.tsx:418；official-login-section.tsx:156；dialog.tsx:32 |

典型嵌套（以 prompts 页为例）：

```
<main p-6 lg:p-8>
  <section bg-card rounded-xl border p-5>                 // 层 1  prompts-page.tsx:309
    <CentralListCard rounded-lg border p-4>               // 层 2  central-list-layout.tsx:96-97
      <p bg-muted rounded p-2 text-xs>正文预览</p>        // 层 3  prompts-page.tsx:404
```

onboarding 里 `fieldset rounded-lg border p-4` → `div bg-muted rounded p-3` → `pre`（onboarding-wizard.tsx:406, 422, 438）；change-preview 里 `DialogContent rounded-xl` → `article rounded-lg border p-4` → `pre bg-muted rounded-md p-3`（change-preview-dialog.tsx:83, 131）。

### 2.3 列表行

- 行高不固定，由内容撑开；行内 padding `p-3`（ProjectOptionRow option-row.tsx:27；hook picker :116；project-hook-picker :110；provider 列表项 `li rounded-lg border p-3` provider-panel.tsx:339）或 `p-4`（NativeResourceRow native-resources.tsx:154；snapshot 项 :277；import 候选 :112）。
- 最小行：hooks 事件分组内的 hook 条目 `rounded border bg-slate-50 px-3 py-2 text-xs dark:bg-slate-900`（hooks-page.tsx:581；hook.tsx:194）。
- 行之间用 `space-y-2/3` 留白分隔，没有分隔线/斑马纹。
- 行内右侧操作：文字按钮 `size=sm variant=outline`（"启用/禁用"、"移除"、"预览恢复"）；中央列表卡片则用 lucide 图标按钮 `size-8 p-0`（mcp-page.tsx:311-345 等）。

### 2.4 `central-list-layout.tsx` 布局契约

| 组件 | 契约 |
|---|---|
| `CentralListLayoutToggle` (:33-71) | 外框 `flex items-center gap-1 rounded-lg border p-1`，`role=group`；两颗 `Button size=sm` 覆盖为 `h-7 gap-1.5 px-2`，选中用 `variant=default`、未选中 `outline`；`aria-pressed`；文案"单列/三列"+ 自绘 SVG 图标 |
| `CentralList` (:73-88) | `mt-4 [&>*]:min-w-0`；list → `space-y-3`；grid → `grid auto-rows-fr items-stretch gap-3 md:grid-cols-2 lg:grid-cols-3`；`data-layout`、`data-slot="central-list"` |
| `CentralListCard` (:90-103) | `article min-w-0 rounded-lg border`（**无 bg**，继承外层 section 的 `bg-card`）；grid 时 `flex h-full flex-col overflow-hidden`，list 时 `p-4` |
| `CentralListCardBody` (:105-122) | grid 时 `flex flex-1 flex-col p-4` |
| `CentralListCardFooter` (:124-143) | `footer aria-label`；grid 时 `bg-muted/20 mt-auto border-t px-4 py-3`；list 时 `mt-3`（无分隔） |

页面侧约定（三页一致）：卡片头 `flex min-w-0 flex-wrap items-start justify-between gap-3`，`h3 font-medium`（grid 加 `truncate`），副标题 `text-muted-foreground mt-1 text-xs`；list 模式操作按钮在标题行右侧，grid 模式移到 footer；平台分配按钮 `PlatformAssignmentButton` 在 footer（grid 时 `ml-auto`）。

### 2.5 空状态 / 加载 / 错误的四种写法

1. 居中虚线大卡：`bg-card mx-auto mt-8 max-w-3xl rounded-xl border border-dashed p-8 text-center` + h2 + p + Button（dashboard-page.tsx:82-90）。
2. 虚线小卡含标题+说明：`rounded-lg border border-dashed p-5/p-4 text-sm` + `p.font-medium` + `p.text-muted-foreground`（projects-page.tsx:173-178；native-resources.tsx:111-116）。
3. 虚线单段：`text-muted-foreground mt-5 rounded-lg border border-dashed p-4 text-sm`（prompts-page.tsx:343；provider-panel.tsx:332）。
4. 纯文字：`text-muted-foreground mt-4 text-sm`（mcp-page.tsx:302；skills-page.tsx:260；hooks-page.tsx:317；project-assignments-section.tsx:73）。

加载态统一 `<p role="status" className="mt-N text-sm">正在…</p>`；错误态两种：`<p role="alert" className="text-destructive">` 与 `<BlockingState>`（`rounded-lg border p-4 text-sm` + toneClass warning + "⛔ " 前缀标题，blocking-state.tsx:18-30）。

### 2.6 状态徽章与符号

- `SyncStatusBadge`：`inline-flex rounded-full border px-2 py-1 text-xs font-medium` + toneClass（sync-status-badge.tsx:61）；文案自带字符前缀 `✓ △ ! ○ ⛔ ×`（:4-15）。
- `OptionTag`：`rounded-full border px-2 py-0.5 text-xs font-medium`（option-row.tsx:75）；native-resources.tsx:292-313 **另有一份同名 `OptionTag`**，用硬编码 `border-slate-200 bg-slate-50 text-slate-700` 作 muted，没有 `dark:` 配对。
- dashboard 最近同步用 `bg-muted rounded-full px-2 py-1 text-xs`（dashboard-page.tsx:154）；projects 卡片路径状态同款（projects-page.tsx:190）。

### 2.7 链接与原生弹窗

- 下划线链接 3 处：dashboard-page.tsx:214（"管理"）、:264（"查看"）、projects/detail/page.tsx:175（"← 返回项目列表"）。
- `globalThis.confirm` 3 处：prompts-page.tsx:370（删除提示词）、provider-panel.tsx:388（删除渠道）、official-login-section.tsx:103（重新登录确认）；而侧栏移除项目用自定义 `ProjectRemoveDialog`（app-shell.tsx:572-638）、Skills 采纳用自定义确认对话框（skills-page.tsx:561-621）。

### 2.8 通知

`NotifyViewport`：`fixed top-4 right-4 z-[60] w-[min(calc(100vw-2rem),24rem)] flex-col gap-3`（notify.tsx:94）；单条 `rounded-lg border p-4 text-sm shadow-lg` + toneClass success/destructive（:75-76）。`top-4` 落在 `h-14` 顶栏区域内，会盖住右上工具入口。

---

## 3. 原语组件

### 3.1 `ui/button.tsx`

```ts
// button.tsx:7-25
base: "inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium
       transition-colors outline-none disabled:pointer-events-none disabled:opacity-50
       focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
variant.default : "bg-primary text-primary-foreground hover:opacity-90"
variant.outline : "border border-input bg-background hover:bg-muted"
size.default    : "h-10 px-4 py-2"       // 40px
size.sm         : "h-8 px-3 text-xs"     // 32px, 12px 字
```

- 只有 2 个 variant、2 个 size；无 ghost/destructive/link/icon 尺寸。图标按钮靠调用方 `className="size-8 p-0"`（spec 认可写法）。
- `hover:opacity-90` 而非颜色变化；焦点环 `ring-ring`（灰）+ `ring-offset-2`。
- 默认按钮圆角 8px（`rounded-md`）；macOS 按钮（AppKit）约 5–6px，高度 regular 22pt / large 28pt；本项目 40px/32px 更接近 Web 规范。
- `CentralListLayoutToggle` 里再覆盖为 `h-7`（28px）。

### 3.2 `ui/dialog.tsx`

| 组件 | 类 | 行 |
|---|---|---|
| `DialogOverlay` | `fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4`；`closeOnOutsideClick` 默认 false | :28-44 |
| `DialogContent` | `section role=dialog aria-modal` `bg-card max-h-[90vh] w-full max-w-3xl overflow-auto rounded-xl p-6 shadow-xl`；内部接 `useDialogFocus` | :79-95 |
| `DialogHeader` | `flex items-start justify-between gap-4` | :104-110 |
| `DialogFooter` | `mt-6 flex flex-wrap justify-end gap-3` | :117-125 |

各调用方覆盖值：

| 对话框 | className 覆盖 | 结构 |
|---|---|---|
| `FormDialog`（form-dialog.tsx:53-114） | `flex max-h-[calc(100dvh-2rem)] max-w-2xl min-w-0 flex-col overflow-hidden p-0` | 三段式：`DialogHeader shrink-0 border-b p-6`（标题 `text-xl font-semibold` + 描述 + 右上"关闭" outline sm）→ `form > div min-h-0 space-y-4 overflow-y-auto p-6` → `DialogFooter shrink-0 border-t px-6 py-4`（取消 outline / 提交 default） |
| `SettingsDialog`（:102-123） | `max-h-[calc(100dvh-2rem)] max-w-2xl min-w-0 p-0` | 头部 `border-b p-6` 眉标+标题+描述+"关闭"；正文 `space-y-4 p-6` 四个 `section rounded-lg border p-4`；**无 footer** |
| `SkillImportDialog`（:208） | 同 FormDialog 但 `max-w-3xl` | 三段式，footer 4 颗按钮（取消/重新检测/复制所选/预览接管） |
| `SkillDirectoryImportDialog`（:72）、`SkillGithubImportDialog` | 同 FormDialog `max-w-2xl` | 三段式 |
| `ChangePreviewDialog`（:52）、`SnapshotRestoreDialog`（:156）、skills 内容预览（skills-page.tsx:525） | `max-h-[88vh]`（宽度默认 3xl） | 单块 `p-6` 整体滚动；头部眉标 `text-muted-foreground text-sm` + `h2 mt-1 text-xl font-semibold` + 右上"关闭"；footer `justify-end`（ChangePreview 用 `DialogFooter`，Snapshot 内嵌 `mt-4 flex justify-end gap-3`） |
| `McpImportDialog`（:61）、`HookImportDialog` | `max-h-[90vh]` | 单块；右上"关闭"是 **default size**（非 sm）；底部 `mt-6 flex flex-wrap justify-end gap-3` |
| `HookAssignmentPickerDialog`（:75）、`ProjectHookPickerDialog`（:75） | `max-h-[90vh] max-w-2xl` | 单块，**无 footer**，仅右上"关闭"(default size) |
| `OnboardingWizard`（:337） | `max-h-[90vh] max-w-4xl` | 单块；右上按钮文案"暂停向导"；步骤条为 `ol flex gap-2 text-xs`（:356-370） |
| `ProjectRemoveDialog`（app-shell.tsx:594） | `max-w-md` | 单块，**无右上关闭**，仅底部 取消/确认移除 |
| skills 采纳确认（skills-page.tsx:568） | `max-w-lg` | 头部含"关闭"，底部 取消/"是" |

按钮顺序：全部"次要在左、主按钮在右"（符合 macOS 习惯）。主按钮 = `variant=default`（中性深灰），没有 destructive 样式区分"确认移除/确认删除"。

### 3.3 `ui/field.tsx` 与 `.field` utility

- `Field`：`div/label block space-y-2 text-sm` + `label font-medium`（field.tsx:13-27）。
- `.field`（styles.css:114-127）：`width:100%; border:1px solid var(--input); border-radius: var(--radius-sm)`(6px)`; background: var(--card); padding: .6rem .75rem; outline: none`；focus：`border-color: var(--ring); box-shadow: 0 0 0 3px color-mix(ring 24%)`。
- 输入框字号继承 body（16px），按钮是 14px；输入框高度 ≈ 24 + 19 ≈ 43px，比 `Button` 默认 40px 高。
- 页面内直接写 label 的三种方式并存：`Field` 组件（hooks/mcp/provider 表单）、`<label className="mb-1 block text-sm font-medium">`（prompts-page.tsx:591-596、app-shell.tsx:528-533）、`<label className="text-sm font-medium">` 包裹 input（projects-page.tsx:90-107）。
- checkbox/radio/select 全部原生控件，无自定义样式（settings-dialog.tsx:165, 219；onboarding :465；mcp-form :164；provider-panel :455）；`color-scheme` 已随主题切换，所以原生控件在 macOS 上会呈现系统外观（这是与 macOS 一致的正面点）。

### 3.4 其他共享组件

- `ToolIconToggle`（tool-icon-toggle.tsx:24-50）：`Button size=sm outline` 改 `size-8 p-0 shadow-none`；激活 `border-slate-300 bg-slate-50 shadow-sm dark:border-slate-600 dark:bg-slate-800`，未激活 `border-slate-200 bg-transparent dark:border-slate-700` + 图标 `opacity-25 grayscale`（spec 规定必须 light/dark 成对，这里已成对）。
- `ThemeToggleGroup`（settings-dialog.tsx:262-295）：`inline-flex gap-0.5 rounded-md border p-0.5` 内三颗 `size-6 rounded` 图标按钮，选中 `bg-muted`——形态接近 macOS segmented control，但 24px 偏小，图标为手绘 SVG。
- `RefreshEnvironmentButton`（refresh-environment-button.tsx:45-82）：按钮下方最多 3 段 `role=status/alert` 说明 + 工具列表 `ul`。

---

## 4. 排版与色彩（`src/styles.css`）

### 4.1 字体

```css
/* styles.css:34-41 */
font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
```

- `Inter` 排首位，但：`index.html` 无字体 `<link>`；`styles.css` 无 `@font-face`；CSP `font-src 'self'`；`package.json` 无字体包 → Inter 仅在用户本机安装时生效，否则回落 `ui-sans-serif`/`system-ui`（macOS = SF Pro）。
- `text-rendering: optimizeLegibility`（:104）；无 `-webkit-font-smoothing`、无 `font-feature-settings`、无 `user-select: none`/`cursor: default` 处理（WKWebView 内非文本 UI 上仍显示 I 型光标、可被选中）。
- `button, input, textarea, select { font: inherit }`（:107-112）。
- 根字号未设，浏览器默认 16px；Tailwind 字号阶梯 12/14/16/18/20/24/30 全用（见 §2.2）。macOS 常用阶梯（SF）约 11/13/15/17/22/28；本项目正文 14px、注释 12px 与 macOS 13/11 接近但整体偏大 1px；标题 24px 接近 Title 2（22）。
- 等宽：`font-mono` 用于命令/路径/JSON 输入（prompts :615、provider-panel :576、hooks-page :685/699、mcp-form :178/240）。

### 4.2 CSS 变量清单

`:root`（:32-62）与 `.dark`（:64-85）各定义：`--background --foreground --card --card-foreground --primary --primary-foreground --muted --muted-foreground --border --input --ring --destructive(-foreground) --warning(-foreground) --success(-foreground) --info(-foreground) --radius`。全部 oklch。

| Token | Light | Dark | 备注 |
|---|---|---|---|
| background | 0.985 0.002 247 | 0.141 0.005 285 | 与 card 几乎同亮度 |
| card | 1 0 0（纯白） | 0.21 0.006 285 | 侧栏/顶栏/对话框/section 全用它 |
| primary | 0.278 0.033 256（深灰蓝） | 0.985（近白） | **中性色，非 accent** |
| muted | 0.967 | 0.274 | hover 背景、pre 背景 |
| border | 0.928 | `oklch(1 0 0 / 10%)` | dark 用透明白 |
| ring | 0.707（灰） | 0.552（灰） | 焦点环无色相 |
| info | 0.6 0.15 250（蓝） | 0.707 0.165 254 | 唯一蓝色，仅状态徽章 |
| radius | 0.625rem (10px) | 同 | 派生 sm 6 / md 8 / lg 10 |

`@theme inline`（:7-30）把上述映射为 `--color-*` 与 `--radius-sm/md/lg`；`--radius-xl` 未覆盖。`* { border-color: var(--border) }`（:87-89）。

### 4.3 毛玻璃 / 半透明 / 阴影

- `backdrop-blur`/`backdrop-*`：**0 处**。
- 半透明仅：`DialogOverlay bg-slate-950/40`、`CentralListCardFooter bg-muted/20`、toneClass 的 `/10 /30`、dark 边框 10%。
- 阴影见 §2.2：卡片 0 阴影、仅对话框 `shadow-xl` 与通知 `shadow-lg`。

### 4.4 主题机制（与 spec 一致，改动时必须保留）

- `@custom-variant dark (&:where(.dark, .dark *))`（:5）类驱动；`color-scheme` 随 `.dark` 翻转（:33, :65）。
- `src/main.tsx:10` 在 `createRoot` 前调用 `applyThemeFromStorage()`（use-theme.ts:46 `classList.toggle("dark")`），存储键 `easytoagents.theme.v1`。
- `html, body { height: 100%; overflow: hidden }`（:91-97）——外壳滚动所有权契约，注释写明 WKWebView 的 `scrollHeight` 误计问题。

---

## 5. 与 macOS HIG 的差距清单

说明：以下"建议做法"是可执行方向，不改代码；标注 ⚠ 的项涉及 spec 硬约束或 Tauri 平台限制，需在 design 阶段明确取舍。

### 5.1 外壳（窗口 / 侧栏 / 顶栏）

| # | 当前做法 | 建议做法 | 涉及文件 |
|---|---|---|---|
| S1 | 原生 `Visible` 标题栏显示 "EasyToAgents" + 网页顶栏再放一次品牌块（"EA" 方块 + 名称 + 副标题） | 两条路线择一：**A（低风险）** `titleBarStyle: "Transparent"` + `hiddenTitle: true`，去掉顶栏品牌块，只保留原生标题栏；**B（macOS 原生感）** `titleBarStyle: "Overlay"` + `hiddenTitle: true` + `trafficLightPosition`，侧栏顶部预留约 52px（`pt-13`）交通灯留白并加 `data-tauri-drag-region`，顶栏合并进内容列成为"统一工具栏"。⚠ Overlay 已知限制：未聚焦时不可拖拽（#4316）、标题栏高度随系统版本变化。 | `src-tauri/tauri.conf.json`、`src/app/app-shell.tsx:153-209` |
| S2 | 侧栏、顶栏、内容、卡片全 `bg-card`；`--background` 与 `--card` 亮度差 1.5%，无层级 | 建立"侧栏 < 内容底 < 卡片"三级明度：新增语义 token（如 `--sidebar` light≈0.955、dark≈0.17）并在 `@theme inline` 映射为 `bg-sidebar`；内容区用 `bg-background` 且把 `--background` 调暗到 ≈0.965（light），卡片保持 `--card` 纯白形成浮起感。⚠ spec 现规定 sidebar/header 用 `bg-card`，新增 token 需同步 `update-spec`。真正的 NSVisualEffect `sidebar` 材质需 `transparent + windowEffects`，而 macOS 上 `transparent` 依赖 `macOSPrivateApi`（App Store 警告），建议 MVP 不做。 | `src/styles.css:42-85, 7-30`、`app-shell.tsx:76, 167`、`.trellis/spec/frontend/component-guidelines.md` |
| S3 | 一级导航项选中态 = 实心 `bg-primary`（light 深灰、dark 白）胶囊，`rounded-md px-3 py-2`，无图标 | 采用 macOS 源列表样式：选中 `bg-accent/15 text-accent`（或激活窗口时 `bg-accent text-white`）圆角 6px、高度 28px（`h-7 px-2`），每项左侧 16px lucide 图标（LayoutDashboard / MessageSquareText / Plug / Webhook / Sparkles / FolderKanban），`text-[13px]`。需要先定义 accent token（见 T2）。 | `app-shell.tsx:34-48, 81-96, 388-395` |
| S4 | 项目子列表用 `border-l pl-3` 竖线缩进 + 自绘 chevron 按钮 | 改为 macOS 源列表分组："项目" 作为可折叠 section header（`text-[11px] font-semibold uppercase text-muted-foreground`，右侧 lucide `ChevronRight` 旋转），子项去掉竖线，仅 `pl-6` 缩进；hover 显示编辑/移除图标按钮（现有 `projectRowActionClass` 逻辑可保留）。 | `app-shell.tsx:386-515, 50-51` |
| S5 | 侧栏底部"设置"用手绘齿轮 SVG + `border-t` | 统一为 lucide `Settings` 图标，样式与导航项一致；去掉 `border-t`（macOS 侧栏底部通常无分隔线，用间距即可）。 | `app-shell.tsx:98-151` |
| S6 | 顶栏右侧工具入口 = `rounded-full border` 药丸标签，选中实心深色 + `shadow-sm` | 若保留顶栏：改成 macOS 工具栏 segmented control（相邻按钮共享边框、`rounded-md` 外框、选中 `bg-muted`）或"仅图标 + tooltip"的工具栏按钮；若采纳 S1-B，则把工具入口移到侧栏一级导航"工具"分组下。 | `app-shell.tsx:181-205`、`src/lib/tool-metadata.ts`（路由元数据，只读） |
| S7 | 内容列 `p-6 lg:p-8` + `mx-auto max-w-6xl` 居中 | 桌面端内容通常左对齐、不居中；建议 `px-8 py-6` 固定，去掉 `mx-auto`，保留 `max-w-6xl`（或 `max-w-5xl`）作为可读宽上限。 | 全部 `*-page.tsx` 的 `<main>`/`<header>`/section 上的 `mx-auto max-w-6xl`（32 处） |
| S8 | 通知 `fixed top-4 right-4 z-[60]` 覆盖顶栏 | 若顶栏保留，改 `top-16`（顶栏下方）；若 S1-B 合并工具栏，则 `top-14`；`shadow-lg` 保留，圆角统一 `rounded-lg`。 | `src/components/notify.tsx:94` |
| S9 | `PageLoading` 是一行 `p-6 text-sm` 文字 | 保留 `role="status"` 语义，样式改成与页面头部对齐的位置（`px-8 py-6 text-muted-foreground`）；不引入 spinner 也可。 | `app-shell.tsx:127-133` |

### 5.2 页面头部

| # | 当前做法 | 建议做法 | 涉及文件 |
|---|---|---|---|
| H1 | 眉标 + h1 + 一段流程说明（多为"只更新中央意图…仍需预览 Apply"）；主操作下沉到第一张 section 卡片标题行 | 统一为 macOS 工具栏式头部：`flex items-end justify-between` 左 h1（`text-[22px] font-semibold tracking-tight`）右侧主操作（新增 / 导入 / 布局切换）；**删除眉标**（与侧栏选中项重复）；说明段改为可选、一行以内，或迁移到空状态/tooltip。 | dashboard-page.tsx:33-51；prompts-page.tsx:299-305, 312-330；mcp-page.tsx:262-290；skills-page.tsx:（header 同款）232-248；hooks-page.tsx:292-305；projects-page.tsx:73-80；tool-profiles-page.tsx:86-93 |
| H2 | 眉标 `font-medium` 有/无不一致；描述 `leading-6 max-w-3xl` 有/无不一致 | 随 H1 删除眉标后自然消除；若保留描述，统一 `text-sm text-muted-foreground leading-6 max-w-3xl`。 | 同上 |
| H3 | 项目详情页用 `underline` 文字"← 返回项目列表" | 改为工具栏内 lucide `ChevronLeft` 图标按钮（`variant=ghost`，需新增 ghost variant）或面包屑 "项目 / {displayName}"；路径 `code` 与状态行合并为一行 `text-xs text-muted-foreground`。 | `src/features/projects/detail/page.tsx:174-184` |
| H4 | section 标题 h2 两档（lg/xl）、h3 三档 | 定标：页面 h1 = `text-[22px] font-semibold`；section h2 = `text-[15px] font-semibold`（macOS Headline）；卡内 h3 = `text-sm font-medium`；分组标签 = `text-[11px] font-semibold uppercase tracking-wide text-muted-foreground`（替代 `text-slate-500`）。 | 见 §2.1 列表；hooks-page.tsx:532；hook.tsx:151 |
| H5 | dashboard 头部右侧同时放"检测现有配置"和 `RefreshEnvironmentButton`（含 3 段状态文字） | 头部只放一颗主操作；重新检测入口保留在设置对话框（代码注释 dashboard-page.tsx:41-43 已表达此意图）。 | dashboard-page.tsx:41-50 |

### 5.3 卡片与列表

| # | 当前做法 | 建议做法 | 涉及文件 |
|---|---|---|---|
| C1 | 页面 section `bg-card rounded-xl border p-5` 内再嵌 `rounded-lg border p-4` 卡片，再嵌 `rounded border p-3`，最深 4 层边框 | 采用 macOS "inset grouped list"：外层 section 保留一层 `rounded-xl border bg-card`（去 `p-5`，改 `overflow-hidden`），内部条目改为 `divide-y` 行（`px-4 py-3`），**不再给条目单独加边框和圆角**；`pre` 代码块保留 `bg-muted rounded-md`。 | central-list-layout.tsx:90-143；dashboard-page.tsx:117-164；projects-page.tsx:180-241；mcp/skills/hooks/prompts "全局目标状态" 网格（mcp-page.tsx:475-537 等）；change-preview-dialog.tsx:82-134；snapshot-restore-dialog.tsx:274-313；native-resources.tsx:154；option-row.tsx:27 |
| C2 | 圆角 6 档（4/6/8/10/12/full） | 收敛为 3 档 + full：容器 `rounded-xl`（12）、控件/子块 `rounded-lg`（改 `--radius` 使 lg=8px 或直接用 md）、内嵌代码块 `rounded-md`；删除所有裸 `rounded`（4px）与 `rounded-[4px]`。可把 `--radius` 调为 `0.5rem` 并补 `--radius-xl: 0.75rem` 到 `@theme`。 | `styles.css:27-29, 61`；18 处 `rounded`、2 处 `rounded-[4px]`（app-shell.tsx:200；settings-dialog.tsx:232） |
| C3 | 卡片 0 阴影、纯 1px 边框 | macOS 卡片同样偏平，可保持；仅在 `bg-background` 调暗（S2）后，section 卡片加极轻 `shadow-[0_1px_2px_rgba(0,0,0,.04)]`（dark 不加）。不建议大范围 `shadow-*`。 | `styles.css`（可新增 `--shadow-card` token） |
| C4 | 列表行 `space-y-2/3` 留白 + 各自边框；行高由内容决定 | 行统一 `min-h-11`（44px）`px-4 py-2.5`，`divide-y`，hover `bg-muted/50`；行内主文 `text-sm`、副文 `text-xs text-muted-foreground`。 | option-row.tsx；hook-assignment-picker-dialog.tsx:114-141；project-hook-picker-dialog.tsx:108-130；provider-panel.tsx:337-402；hooks-page.tsx:577-611；hook.tsx:190-216 |
| C5 | `CentralListLayoutToggle`：带文字"单列/三列"+ 自绘 SVG，外框 `rounded-lg border p-1` | 改为 macOS segmented control 风格：仅图标（lucide `List` / `LayoutGrid`）、`h-7`、相邻无间距共享外框 `rounded-md border`、选中 `bg-muted`；`aria-label`/`aria-pressed`/`title` 契约保持（spec 要求）。 | central-list-layout.tsx:33-71, 145-173 |
| C6 | 中央列表卡片 list 模式 footer 下再放一段 `text-xs` 提示（"图标启用或停用只更新中央配置…"） | 移除卡片级重复提示，全页仅在页头或空状态保留一次；`PlatformAssignmentButton` 的 `title` 已说明分配含义。 | prompts-page.tsx:444-448；skills-page.tsx:409-415 |
| C7 | 空状态 4 种写法 | 统一一个 `EmptyState`（新组件，`src/components/`）：居中、`py-10`、lucide 图标 `size-8 text-muted-foreground`、标题 `text-sm font-medium`、一句说明、可选主操作；不用虚线边框（macOS 极少用 dashed）。 | dashboard-page.tsx:82-90；projects-page.tsx:173-178；prompts-page.tsx:343；provider-panel.tsx:332；native-resources.tsx:111；mcp-page.tsx:302；skills-page.tsx:260；hooks-page.tsx:317 |
| C8 | 状态文案用字符符号前缀 `✓ △ ! ○ ⛔ ×` | 徽章改为左侧 6px 圆点（`before:size-1.5 before:rounded-full before:bg-current`）+ 纯文字；`BlockingState` 标题去 "⛔"，改 lucide `OctagonAlert size-4`。⚠ 徽章文案是测试断言对象（`SyncStatusBadge` 有 `sr-only` 状态码，但可见文案也被断言），改前需 grep 测试。 | sync-status-badge.tsx:4-15, 61；blocking-state.tsx:22；projects/detail/page.tsx:271（"○ 未纳管"） |
| C9 | native-resources.tsx 自带一份 `OptionTag`，muted 用硬编码 slate 且无 `dark:` | 删除本地副本，复用 `option-row.tsx` 导出的 `OptionTag`（走 `toneClass`）。 | native-resources.tsx:292-313；option-row.tsx:65-82 |
| C10 | hooks 事件条目 `bg-slate-50 dark:bg-slate-900`、分组标题 `text-slate-500 dark:text-slate-400` | 改 `bg-muted/40`、`text-muted-foreground`（语义 token）。 | hooks-page.tsx:532, 581；hook.tsx:151, 194 |
| C11 | dashboard `MetricCard` 大数字 `text-3xl` + 下划线"查看"链接；`SummaryItem` 灰底小格 | 数值 `text-2xl font-semibold tabular-nums`，"查看"改 `Button variant=link/ghost size=sm` 带 `ChevronRight`；`SummaryItem` 去灰底，改 `dl` 两列 `text-sm` 键值对 + `divide-y`。 | dashboard-page.tsx:198-269 |

### 5.4 表单与对话框

| # | 当前做法 | 建议做法 | 涉及文件 |
|---|---|---|---|
| D1 | 头部右上 "关闭" outline 按钮（有的 sm、有的 default）+ 底部 "取消" 重复 | macOS sheet 无右上关闭按钮，靠 Esc / 取消；建议删除 `DialogHeader` 里的"关闭"，仅保留 footer 取消（或改为右上 `X` 图标 ghost 按钮，尺寸 `size-7`，仅在无 footer 的对话框保留）。⚠ 部分测试可能按 `getByRole("button", {name: "关闭…"})` 断言，需同步。 | form-dialog.tsx:67-75；settings-dialog.tsx:120-122；change-preview-dialog.tsx:64-66；snapshot-restore-dialog.tsx:168-170；mcp-import-dialog.tsx:67-74；hook-import-dialog.tsx；hook-assignment-picker-dialog.tsx:81-88；project-hook-picker-dialog.tsx:81-88；skill-*-import-dialog.tsx；onboarding-wizard.tsx:346-348（"暂停向导" 语义不同，保留） |
| D2 | 宽度 6 档、最大高度 3 种写法、两种内部结构 | 在 `DialogContent` 增加 `size` prop：`sm`=`max-w-md`（确认类）、`md`=`max-w-xl`（表单/选择器）、`lg`=`max-w-3xl`（预览/导入）；统一 `max-h-[calc(100dvh-4rem)]`；统一三段式（header/body 滚动/footer 固定）并把 `p-0 flex flex-col overflow-hidden` 收进原语，调用方不再各写一套。 | dialog.tsx:79-95；所有 `DialogContent className=` 覆盖（见 §3.2 表） |
| D3 | 对话框 `rounded-xl p-6 shadow-xl`，遮罩 `bg-slate-950/40` 无模糊 | 圆角 `rounded-xl`(12px) 与 macOS sheet 接近，保留；遮罩改语义 `bg-foreground/30 dark:bg-black/50` + `backdrop-blur-sm`（WKWebView 支持 `backdrop-filter`，spec 未禁止）；`shadow-xl` 保留。⚠ `bg-slate-950/40` 是硬编码调色板，spec 要求语义 token。 | dialog.tsx:32, 89 |
| D4 | 头部结构：眉标 `text-muted-foreground text-sm` + `h2 mt-1 text-xl font-semibold` + 描述 | macOS sheet 标题 `text-[15px] font-semibold` 居左或居中、无眉标；描述 `text-[13px] text-muted-foreground`；删除眉标（"持久化预览"/"私有恢复点"/"应用偏好"/"首次接管向导"）。 | change-preview-dialog.tsx:55-63；snapshot-restore-dialog.tsx:159-167；settings-dialog.tsx:105-119；onboarding-wizard.tsx:340-345；skills-page.tsx:528-538 |
| D5 | 主按钮 `variant=default`（中性深灰），破坏性操作（确认移除/确认删除/移出中央库）与普通提交同色 | 新增 `variant: destructive`（`bg-destructive text-destructive-foreground`）用于移除/删除确认；普通主按钮改 accent（见 T2）。按钮顺序已是"主按钮在右"，保持。 | button.tsx:11-14；app-shell.tsx:622-634；snapshot-restore-dialog.tsx:239-244；skills-page.tsx:606-617 |
| D6 | 表单里 label 三种写法；`Field` 用 `space-y-2` 垂直堆叠 | 统一走 `Field`；macOS 偏好"标签右对齐 + 控件左对齐"两列表单：`Field` 增加 `layout="inline"`（`grid grid-cols-[8rem_1fr] items-center gap-x-4`），对话框内表单默认 inline，页面内保持 stacked。 | field.tsx；prompts-page.tsx:590-619；app-shell.tsx:527-544；projects-page.tsx:89-124；provider-panel.tsx:476-633；mcp-form-dialog.tsx；hooks-page.tsx:651-736 |
| D7 | `.field` 输入 16px 字、≈43px 高；按钮 14px、40px 高 | `.field` 加 `font-size: 0.875rem; line-height: 1.25rem; padding: .5rem .75rem` → 高 ≈36px；`Button` 默认改 `h-9`（36px）对齐；`sm` 保持 `h-8`。 | styles.css:114-127；button.tsx:15-18 |
| D8 | `FormDialog` body 内 pending/error 文案放在字段后（`role=status/alert`） | 保留语义，但移到 footer 左侧（`DialogFooter` 改 `justify-between`，左放状态文本、右放按钮），减少表单抖动。 | form-dialog.tsx:89-114 |
| D9 | `SettingsDialog` 四个 `section rounded-lg border p-4` + `h3 font-semibold`，无 footer；复选框说明段 200+ 字 | 参考 macOS 系统设置：左侧 tab 或顶部 segmented（外观/应用/工具）；每组用 C1 的 inset grouped list（一行一设置：左标签、右控件），`ThemeToggleGroup` 放大到 `h-7` segmented；"直接应用"长说明压缩为一句 + "了解更多"折叠。 | settings-dialog.tsx:125-241, 262-295 |
| D10 | 原生 `globalThis.confirm` 3 处 | 改用现有自定义确认对话框模式（`ProjectRemoveDialog` 形态，`max-w-md`，destructive 主按钮）。⚠ spec quality-guidelines 中 Skill 删除已明确不用 `window.confirm`。 | prompts-page.tsx:368-376；provider-panel.tsx:386-394；official-login-section.tsx:103 |
| D11 | Onboarding 步骤条是纯文字 `ol` | 改水平 stepper：圆形序号 `size-6 rounded-full`（当前步 accent 底、已完成 `Check` 图标）+ 连接线；宽度 `max-w-4xl` 改 `lg` 档。 | onboarding-wizard.tsx:356-370 |
| D12 | 原生 checkbox/radio/select 直接使用 | 保留原生（`color-scheme` 已跟随主题，在 macOS 上就是系统外观），这是当前最"macOS"的部分；只需保证 `accent-color: var(--accent)` 与新 accent 一致。 | styles.css（新增 `accent-color`） |

### 5.5 排版与色彩

| # | 当前做法 | 建议做法 | 涉及文件 |
|---|---|---|---|
| T1 | `font-family: Inter, ui-sans-serif, system-ui, -apple-system, …`；Inter 未打包 | 改为 `-apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, "Segoe UI", sans-serif`（去掉 Inter，避免本机装了 Inter 的用户看到非系统字体）；补 `-webkit-font-smoothing: antialiased`（仅 dark 或全局，按视觉验证）；等宽栈补 `ui-monospace, "SF Mono", Menlo`。 | styles.css:34-41 |
| T2 | 无 accent 色：`--primary` 中性灰、`--ring` 灰 | 新增 `--accent` / `--accent-foreground`（light `oklch(0.55 0.2 255)` ≈ macOS 系统蓝 #0A84FF/#007AFF；dark `oklch(0.7 0.17 250)`），映射 `--color-accent`；主按钮 `bg-accent`、导航选中 `bg-accent/15 text-accent`、`--ring` 改为 accent 的 40%、`accent-color` 赋给原生控件。⚠ `--primary` 现被 Button default、导航选中、顶栏 logo 使用，改 `--primary` 值即可全局生效，但要评估 dark 模式主按钮从"白底"变"蓝底"对截图基线的影响。 | styles.css:46-47, 52, 70-71, 76；button.tsx:12；app-shell.tsx:46, 171, 190 |
| T3 | 字号阶梯 12/14/16/18/20/24/30，正文 16 | 桌面端阶梯：11（分组标签）/12（注释）/13（正文）/15（section 标题）/22（页标题）；把 body 设 `font-size: 13px`（或 `text-[13px]` 于 `main`），`text-sm` 场景多数改成默认继承。⚠ 影响面大（183 处 `text-sm`），建议只在 `main`/`aside` 根设置字号，让继承生效，逐步移除显式 `text-sm`。 | styles.css:99-105；全部页面 |
| T4 | 圆角 token 仅 sm/md/lg，`rounded-xl` 用 Tailwind 默认 | 补 `--radius-xl: calc(var(--radius) + 4px)`，让整套圆角随 `--radius` 联动。 | styles.css:27-30 |
| T5 | 无 `user-select`/`cursor` 处理 | 在 `aside`、`header`、按钮、导航项上加 `select-none`；全局 `body { cursor: default }` 并对可输入元素恢复 `cursor: text`（WKWebView 默认对所有文本显示 I 光标，与原生 app 不符）。 | styles.css；app-shell.tsx |
| T6 | 硬编码调色板 12 处（见 §2.2 表最后一行） | 全部替换为语义 token（`bg-muted`、`text-muted-foreground`、`bg-foreground/xx`）；`ToolIconToggle` 激活态可改 `border-border bg-muted shadow-sm`（已成对 dark:，替换后可去掉 dark: 分支）。 | tool-icon-toggle.tsx:31-32；hooks-page.tsx:532, 581；hook.tsx:151, 194；native-resources.tsx:300；prompts-page.tsx:465；provider-panel.tsx:418；official-login-section.tsx:156；dialog.tsx:32 |
| T7 | dark `--border: oklch(1 0 0 / 10%)`、`--card` 0.21 | 与 macOS dark 相近（系统 separator ≈ white 10–15%），保留；侧栏 dark 建议 0.17 以低于 card。 | styles.css:64-85 |
| T8 | 窗口 `backgroundColor` 未设，首帧靠 JS 挂 `dark` class | 可在 `tauri.conf.json` 设 `backgroundColor` 为 light `--background` 近似值以减少启动白闪；dark 首帧仍由 `applyThemeFromStorage()` 处理（spec 规定不能用内联脚本）。 | src-tauri/tauri.conf.json；src/main.tsx:10 |

### 5.6 "无用提示语"候选清单（仅列位置，删改决策交主 Agent）

同一语义（"只更新中央意图 / 不改原生 / 需预览后 Apply"）在同一页面重复出现的段落：

| 页面 | 位置 | 备注 |
|---|---|---|
| prompts | 页头描述 :302-304；卡片标题下 :317-319；每张卡片 footer :444-448；FormDialog description :579-583；assignment 成功 notify :164-166 | 同页 5 次 |
| mcp | 页头 :265-269；空状态 :302-305；FormDialog description（mcp-form-dialog）；`messages.empty` :246-247 | |
| skills | 每张卡片 footer :409-415；页头（未逐行核对，h1 结构同款） | |
| hooks | 页头；空状态 :316-321；FormDialog description :626-630；checkbox 说明 :735 | |
| projects | 页头 :76-79；空状态 :175-177（"唯一下一步：…"）；卡片内 "移除登记前须先恢复已禁用资源" :202-204 | |
| project detail | "项目资源管理" 描述 :310-312；native-resources 描述 :87-89；ProjectAssignmentsSection description（每个资源一段）+ checkbox 长文案 :93-95 | |
| tool-profiles | 页头 :89-92；`newSessionNotice` :127；provider-panel 描述 :294-296；FormDialog description :439 | |
| settings | 对话框描述 :113-118；"直接应用" 200+ 字说明 :177-181；"工具检测" :194-196；"启用的工具" :207-209 | |
| dialogs | ChangePreview 尾注 :138-143（"非受管字段与表会被保留…"）；Snapshot 描述 :172-177；导入对话框描述（mcp :76-82、hook :80-84、skill :215-220） | |
| dashboard | 页头 :37-39（"中央意图、原生目标状态、同步历史与私有恢复点集中在这里。"） | |
| 侧栏 | 项目移除受阻说明 :503-510 | 有 `aria-describedby` 关联，属于可访问性契约 |

⚠ 测试依赖：26 个 `*.test.tsx` 使用 `getByText/findByText/queryByText`，共 62 处按可见文案断言；`role="status"`/`role="alert"` 文案也是 spec 要求的语义载体。删除任何提示语前需 grep `src/**/*.test.tsx`。

---

## 6. 必须遵守的 spec 硬约束（`.trellis/spec/frontend/component-guidelines.md`）

| 约束 | 出处 | 对本次建议的影响 |
|---|---|---|
| `html, body { height:100%; overflow:hidden }` 必须保留，只有外壳内容列滚动（WKWebView `scrollHeight` 误计） | :69-76；styles.css:91-97 | S1-B 合并工具栏时，滚动容器仍必须是 `app-shell.tsx:110` 那一列；不能让 `main` 或 `body` 滚动 |
| 主题唯一信号是 `<html class="dark">`；不得引入 React theme Context/store；`dark:` 变体用 `@custom-variant` | :80-92 | 新 token 只能加在 `:root/.dark` + `@theme inline`；不得在组件里读主题 |
| 表面用语义 token：`bg-card` 用于 cards/panels/dialogs/**sidebar/header**，禁止 `bg-white` 与原始调色板 | :93-96 | S2 引入 `bg-sidebar` 与 spec 字面冲突，需用 `update-spec` 同步修改该句；T6 是在纠正现有违规 |
| 状态色一律 `toneClass(tone)`（`src/lib/tone-class.ts`），页面不得自造 red/amber/emerald 组合；图标按钮 chrome 的 light `border-*/bg-*` 必须配 `dark:` | :97-101 | C8/C9/C10 替换必须仍经 `toneClass`；`toneClass` 里的 light 调色板类被称为"视觉回归契约"，改动需谨慎 |
| `color-scheme` 随主题翻转 | :102-103 | D12 依赖此项 |
| 无内联脚本（CSP）；主题引导在 `main.tsx` | :104-109 | T8 不能靠 index.html 脚本 |
| 原生交互元素 + 可查询的可访问名；测试按 role/label/text 选取，不用 `data-testid` | :124-126 | 删除文案、改按钮名都会影响测试 |
| pending 用 `role="status"`，失败用 `role="alert"`；禁用/受阻必须有文字解释，颜色不能是唯一信号 | :127-131 | C8 去符号后仍需文字；D8 移动位置但保留 role |
| 对话框必须用 `DialogOverlay/DialogContent/DialogHeader/DialogFooter`，`DialogContent` 拥有 role/aria/Escape/`useDialogFocus` | :256-259 | D2 的 size prop 应加在原语上，而非各处覆盖 |
| 图标行操作：lucide 图标放在共享 `Button` 内，保留 `aria-label`+`title`，`aria-pressed` 表状态；**不得用 lucide 替换品牌资产** | :133-155 | S3/S5/C5 可用 lucide 替换手绘 SVG；`ToolIconToggle` 的品牌 PNG/SVG 必须保留 |
| `CentralList` grid 1/2/3 列、等高 body + 独立 border-top footer、子项 `min-w-0`；`CentralListLayoutToggle` 契约与 `usePersistedCentralListLayout` | :159-243 | C1/C5 改样式时保持 grid 列数、footer 分隔、`aria-pressed` |
| 品牌图标必须用未修改的本地官方资产（含 provenance），激活清晰/未激活 dim+grayscale，且必须有 `aria-label`/`title`/`aria-pressed` | :245-252 | ToolIconToggle 仅可改 chrome，不可改图标 |
| 只挂一个 `NotifyProvider/NotifyViewport` 于 AppShell | :263-266 | S8 仅改定位类 |
| `NavLink className` 不得包在 `cn(...)` 里（clsx 会吞函数） | :285-288 | S3/S4 改导航样式时保持 render-prop 写法 |
| Tool profile 路由必须带 `key=<tool>` | :289-295 | 与本次无关但改 AppShell 时勿动 `tool-profile-routes.tsx` |
| FormDialog 契约：props 集合固定；表单 state 在独立表单组件；`useId` 关联 label；关闭/取消/Esc 清理草稿；保存期间禁用关闭；`useSubmitGuard`；弹窗限高、内容内滚、标题与底部操作可见；提交前 `dialogRef.current?.focus()` | :297-320 | D1 删"关闭"按钮不影响 Esc/取消路径；D2 统一三段式正好满足"标题与底部可见"；D8 移动状态文案不改 props |
| Prettier + `prettier-plugin-tailwindcss` 决定类顺序 | :67-68 | 所有类名改动后需 format |
| quality-guidelines：Skill 删除等破坏性操作不得用 `window.confirm`（:381） | quality-guidelines.md:379-381 | D10 与之一致 |

---

## 7. 外部/平台参考（已本地核实）

- Tauri 配置 schema（`node_modules/@tauri-apps/cli/config.schema.json`，2.11.4）：`titleBarStyle` 三值及 Overlay 限制说明、`hiddenTitle`、`trafficLightPosition`（需 Overlay + decorations）、`windowEffects`（需 transparent；macOS 材质列表含 `sidebar`/`headerView`/`windowBackground`/`underWindowBackground`）、`macOSPrivateApi`（启用 transparent，含 App Store 警告）、`backgroundColor`、`theme`。
- 拖拽区域 `data-tauri-drag-region` 依赖 `core:window:allow-start-dragging`；当前 capability 为 `core:default`，是否已包含该权限**未核实**（需查 `src-tauri/gen/schemas/desktop-schema.json` 或运行时验证）。
- Apple HIG（依据既有知识，未联网核对具体页面）：macOS 侧栏为半透明源列表、选中项 accent 圆角高亮、行高 28pt、组标题小号大写；sheet 主按钮在右、Esc 取消；表单标签右对齐；系统蓝 accent；文本阶梯 11/13/15/17/22。

---

## Caveats / Not Found

- 未读取 `hooks-page.tsx:1-289`、`skills-page.tsx:1-227`、`mcp-page.tsx:1-239`、`provider-panel.tsx:1-269`、`onboarding` 之外的 `official-login-section.tsx:1-89`（均为 hooks/mutation 逻辑，非布局）；页头结构以 h1/main 类名统计（7 处一致）推断。
- 未读取任何 `*.test.tsx`，测试对文案/按钮名的依赖只给出计数（26 文件 / 62 处 `getByText`），未逐条列出。
- `rounded`（裸）= 4px 依据 Tailwind v4 默认值判断，未在 `node_modules/tailwindcss` 中逐行核实。
- `core:default` 是否包含 `allow-start-dragging` 未核实。
- 未做实机截图；所有"层级不足/偏大偏小"判断基于 token 数值与类名，非视觉测量。
