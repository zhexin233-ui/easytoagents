# 技术设计：UI 页面 macOS 风格优化

前置阅读：`prd.md`、`research/layout-audit.md`（差距编号 S/H/C/D/T 在本文引用）、
`research/copy-audit.md`、
`.trellis/spec/frontend/component-guidelines.md`（Styling Patterns / Theming）、
`.trellis/spec/frontend/quality-guidelines.md`（Forbidden / Required Patterns）。

## 1. 边界与原则

- **只动表现层**：改动限于 className、CSS 变量、JSX 结构（包裹/拆分容器）、
  静态文案、`tauri.conf.json` 的 `hiddenTitle`。不动 `commands.*`、query key、
  mutation、对话框状态机、`role`/`aria-*` 语义、路由。任何需要改事件处理或
  数据流的想法一律移出本任务（如 D10 `globalThis.confirm` 替换、D5 破坏性
  按钮变体）。
- **token 先行，页面后行**：先改 `styles.css` 与 `ui/` 原语，让大部分页面
  "被动"获得新观感；再用共享 `PageHeader` / `EmptyState` 收口页面骨架；最后
  逐页删文案与微调。每一步可独立跑测试、独立回滚。
- **不引入新依赖**：继续 Tailwind v4 + CVA + `cn`，图标用已有 lucide-react；
  品牌图标资产（`ToolIconToggle`、工具入口）不得替换（spec 硬约束）。
- **macOS 惯例取舍**（Apple HIG 桌面应用）：
  - 侧栏是"source list"：比内容区略深的底、行高 28px、6px 圆角选中胶囊、
    浅 accent 底 + accent 字（兼顾深色主题对比）、每项带 16px 图标。
  - 内容区 `bg-background` 略灰、卡片 `bg-card` 纯白形成浮起感，1px 细边框、
    无阴影；卡片内条目用 `divide-y` 分行，不再层层套边框（C1）。
  - 主按钮 accent 实底白字、高度 32px；次按钮浅底细边框。
  - 对话框（sheet）：窄、标题 15px、无右上"关闭"按钮（D1）、底部按钮右对齐
    且主按钮最右、遮罩轻 + 微模糊。
  - 系统字体 SF Pro（`-apple-system`），正文 13px，标题 15/22px。

## 2. 设计 token（`src/styles.css`）

### 2.1 字体与渲染（T1 / T3 / T5）

```css
:root {
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui,
    "Segoe UI", sans-serif;                 /* 去掉 Inter：未打包，装了 Inter 的机器会呈现非系统字体 */
  --font-mono: ui-monospace, "SF Mono", Menlo, monospace;
}
body { -webkit-font-smoothing: antialiased; cursor: default; }
input, textarea, [contenteditable] { cursor: text; }
#root { font-size: 13px; }                 /* 正文基准；rem 间距不受影响（rem 以 html 为准） */
aside, header, button, [role="tab"] { user-select: none; }
```

- 字号阶梯：11（分组标签）/ 12（注释、徽章）/ 13（正文，继承）/ 15（section
  标题、对话框标题）/ 22（页标题）。实现时逐步删除页面里冗余的显式
  `text-sm`（14px），让 13px 继承生效；`text-xs`(12px) 保留。
- 只在 `#root` 设字号而不改 `html`，避免 rem 间距整体坍缩。

### 2.2 颜色变量（T2 / S2 / T7 / D12）

在 `:root` / `.dark` 新增，并在 `@theme inline` 映射：

| 变量 | light | dark | 用途 |
| --- | --- | --- | --- |
| `--accent` | `oklch(0.55 0.2 255)`（≈ macOS 系统蓝） | `oklch(0.7 0.17 250)` | 主按钮、选中态、焦点环、原生控件 |
| `--accent-foreground` | `oklch(1 0 0)` | `oklch(1 0 0)` | 主按钮文字 |
| `--accent-soft` | `color-mix(in oklab, var(--accent) 14%, transparent)` | `... 24%` | 侧栏/工具入口选中胶囊 |
| `--sidebar` | `oklch(0.955 0.003 264)` | `oklch(0.17 0.006 286)` | 侧栏与顶部工具栏底 |
| `--background` | 调暗到 `oklch(0.965 0.002 248)` | 不变 | 内容底，让 `--card` 纯白浮起 |
| `--primary` / `--primary-foreground` | 改为 `var(--accent)` / `var(--accent-foreground)` | 同 | 兼容既有 `bg-primary` 调用，一步切主色 |
| `--ring` | `color-mix(in oklab, var(--accent) 45%, transparent)` | 同 | 焦点环 |
| `--radius` | `10px`（保持） | | 卡片 |
| `--radius-control` | `6px` | | 按钮、输入框、导航项、chip |
| `--radius-dialog` | `12px` | | 对话框、通知浮层 |

- `@theme inline` 追加 `--color-accent`、`--color-accent-foreground`、
  `--color-accent-soft`、`--color-sidebar`、`--radius-control`、`--radius-dialog`；
  现有 `--radius-sm/--radius-md` 派生临时映射到 `--radius-control`，阶段 6 删除。
- 新增 `:root { accent-color: var(--accent); }`，原生 checkbox/radio/select 跟随。
- `toneClass` (`src/lib/tone-class.ts`) 语义不改；其内部若有 `rounded-*` 按 §2.3 收敛。
- **spec 冲突**：component-guidelines 现写 sidebar/header 用 `bg-card`。引入
  `bg-sidebar` 后必须在阶段 7 `update-spec` 改写该句（sidebar/header → `bg-sidebar`，
  cards/panels/dialogs → `bg-card`）。

### 2.3 圆角与阴影三档（C2 / C3 / T4）

| 用途 | 类 | 值 |
| --- | --- | --- |
| 控件（按钮、输入、导航项、chip、内嵌 `pre`） | `rounded-control` | 6px |
| 卡片、列表容器、分区 | `rounded-lg` | 10px |
| 对话框、通知浮层 | `rounded-dialog` | 12px |
| 圆形/胶囊（工具入口、状态圆点、开关） | `rounded-full` | 保留 |

现有 `rounded-xl`(20) → `rounded-lg`；`rounded-md`(6)、裸 `rounded`(18)、
`rounded-[4px]`(2) → `rounded-control`。阴影：卡片与按钮上的 `shadow-sm/lg` 删除；
仅 `DialogContent`（`shadow-xl`）与 `notify.tsx` 浮层（`shadow-lg`）保留。

### 2.4 `field` 工具类（D7）

```css
@utility field {
  height: 2rem;                        /* 32px，与 Button 默认对齐 */
  border-radius: var(--radius-control);
  padding: 0 0.625rem;
  font-size: 13px; line-height: 1.25rem;
  /* border / background / focus 规则不变 */
}
textarea.field { height: auto; min-height: 5rem; padding: 0.5rem 0.625rem; }
```

### 2.5 硬编码色板清理（T6 / C9 / C10 / D3）

12 处 `slate-*` 全部改语义 token：`DialogOverlay` → `bg-foreground/30 dark:bg-black/50`；
`hooks-page.tsx` / `hook.tsx` 事件条目底与分组标题 → `bg-muted/40` / `text-muted-foreground`；
`native-resources.tsx` 删除本地 `OptionTag` 副本改用 `option-row.tsx` 导出；
`tool-icon-toggle.tsx` 激活态 chrome → `border-border bg-muted`（品牌图标不动）；
`prompts-page.tsx:465`、`provider-panel.tsx:418`、`official-login-section.tsx:156` 同法。

## 3. 原语（`src/components/ui/`）

### 3.1 `button.tsx`

```ts
base: "inline-flex items-center justify-center whitespace-nowrap rounded-control font-medium transition-colors outline-none select-none disabled:pointer-events-none disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-0"
variant: {
  default: "bg-accent text-accent-foreground hover:bg-accent/90 active:bg-accent/80",
  outline: "border border-input bg-card hover:bg-muted",
  ghost:   "text-muted-foreground hover:bg-muted hover:text-foreground",   // 新增：行内图标操作
}
size: {
  default: "h-8 px-3 text-[13px]",   // 32px
  sm:      "h-7 px-2.5 text-xs",     // 28px
  icon:    "size-7 p-0",             // 新增：纯图标
}
```

默认 variant/size 不变，接口向后兼容。`ghost`/`icon` 用于替换 `app-shell.tsx`
手写的 `projectRowActionClass`、各页面自定义图标按钮类以及 `RefreshEnvironmentButton`
等行内操作。不新增 `destructive`（HIG 的 sheet 不给破坏性按钮特殊底色，且属行为
语义变更）。

### 3.2 `dialog.tsx`（D1 / D2 / D3 / D4）

- `DialogOverlay`：`bg-foreground/30 dark:bg-black/50 backdrop-blur-[2px]`。
- `DialogContent`：`rounded-dialog shadow-xl bg-card`；新增 `size?: "sm" | "md" | "lg"`
  → `max-w-md`(448, 确认类) / `max-w-xl`(576, 表单/选择器, **默认**) / `max-w-3xl`(768,
  预览/导入/设置)；统一 `max-h-[calc(100dvh-4rem)]` 与三段式结构
  （`flex flex-col overflow-hidden p-0`，header/footer 固定，body `overflow-y-auto`）。
  各调用方删除自带的 `max-w-*` / `max-h-*` / `p-0 flex` 覆盖，改传 `size`。
- `DialogHeader`：`px-5 pt-5 pb-3`；标题统一 `text-[15px] font-semibold`；**不再渲染
  右上角"关闭"按钮**，关闭靠 Esc 与 footer 取消（`useDialogFocus` 的 Esc 路径不变）。
  例外：`onboarding-wizard.tsx` 的"暂停向导"语义不同，保留。
- `DialogBody`（新增导出）：`min-h-0 flex-1 overflow-y-auto px-5 py-3`。
- `DialogFooter`：`px-5 pb-5 pt-3 flex items-center justify-between gap-2`；左槽放
  `role=status/alert` 文案（D8，`FormDialog` 的 pending/error 从表单内移到此处），
  右槽按钮组"次按钮左、主按钮最右"。
- 眉标（"持久化预览" / "私有恢复点" / "应用偏好" 等 `DialogHeader` 内的小字）删除（D4）。

### 3.3 `field.tsx`（D6）

标签 `text-[13px] font-medium`，`space-y-1.5`。**不做**两列 inline 布局（涉及所有表单
的重排，收益低于风险），保持 stacked。

## 4. 外壳（`src/app/app-shell.tsx`）

### 4.1 窗口（S1 / S2）

`src-tauri/tauri.conf.json` 主窗口：

```json
{
  "hiddenTitle": true,
  "transparent": true,
  "windowEffects": { "effects": ["sidebar"], "state": "followsWindowActiveState" }
}
```

并在 `app` 下启用 `"macOSPrivateApi": true`（本应用不上架 App Store，用户已确认）。
`titleBarStyle` 保持默认 `Visible`：`Overlay` 路线（交通灯嵌入侧栏、
`trafficLightPosition`）有未聚焦不可拖拽的已知缺陷 #4316，与上架无关，不纳入。
`backgroundColor`（T8）不动：静态值与深色首帧冲突。

**材质只作用于侧栏的做法**：`windowEffects` 作用于整个窗口背景，因此由 CSS 决定
哪些区域透出材质：

- `html, body, #root` 背景改为 `transparent`（现 `body { background: var(--background) }`）。
- `<aside>` 背景 `bg-transparent`，不再用 `bg-sidebar` 实底；仅保留 `border-r`。
- 右列容器（顶部工具栏 + 内容）背景 `bg-background` 不透明；对话框、通知浮层保持
  `bg-card` 不透明。
- `--sidebar` token 仍然定义并映射为 `bg-sidebar`，作为**回退**：若材质在某台机器
  不可用或验收不通过，把 `<aside>` 从 `bg-transparent` 改回 `bg-sidebar`、body 背景
  恢复即可，其余设计不受影响（阶段 9 单独一个 commit）。

**外观同步**：NSVisualEffectView 跟随窗口的 NSAppearance，而应用主题由
`use-theme.ts` 的手动偏好决定。若不同步，会出现"应用浅色、侧栏材质深色"错位。
处理：在 `applyResolvedTheme(resolved)` 里追加

```ts
// 仅 Tauri 运行时；vitest / 浏览器下 __TAURI_INTERNALS__ 不存在则跳过
void getCurrentWindow().setTheme(preference === "system" ? null : resolved).catch(() => {});
```

`setTheme(null)` 让窗口回到跟随系统。需要在 `src-tauri/capabilities/default.json`
新增 `core:window:allow-set-theme`。这是本任务唯一的行为性新增，仍不引入 React
主题 Context（spec 硬约束不变）。`window.theme` 配置项不设（初始外观由启动时
`applyThemeFromStorage()` 同步）。

**已知副作用**：`macOSPrivateApi` 同时把 `fullScreenEnabled` 置为 true，仅影响
全屏偏好，不改变现有窗口行为；`transparent: true` 后任何未铺底色的区域都会透出
桌面，所以阶段 9 必须逐页检查无"漏底"（对话框遮罩 `bg-foreground/30` 覆盖全窗，
遮罩下的侧栏透出属正常）。

### 4.2 结构（S2–S9）

```
<div class="flex h-screen overflow-hidden">
  <aside class="bg-sidebar flex w-[220px] shrink-0 flex-col border-r select-none">   ← 阶段 9 改 bg-transparent 透出原生材质
    <nav class="flex-1 min-h-0 overflow-y-auto px-2 pt-3 space-y-px">
      <SidebarItem icon={LayoutDashboard} to="/" end>总览</SidebarItem>
      <SidebarItem icon={FileText} to="/prompts">提示词</SidebarItem>
      <SidebarItem icon={Plug} to="/mcp">MCP</SidebarItem>
      <SidebarItem icon={Webhook} to="/hooks">Hooks</SidebarItem>
      <SidebarItem icon={Sparkles} to="/skills">Skills</SidebarItem>
      <SidebarGroup label="项目" to="/projects" open onToggle>   ← 组标题行 + 折叠 chevron
        <SidebarItem indent to="/projects/:id">{displayName}</SidebarItem>   ← 无竖线，pl-6
      </SidebarGroup>
    </nav>
    <div class="px-2 pb-3"><SidebarItem icon={Settings} onClick>设置</SidebarItem></div>   ← 去 border-t
  </aside>
  <div class="flex min-w-0 flex-1 flex-col">
    <header class="bg-sidebar/70 backdrop-blur h-11 shrink-0 border-b flex items-center justify-end px-4">
      <ToolSwitcher />                                    ← 原 TopBar 的工具入口 nav
    </header>
    <div class="min-w-0 flex-1 overflow-y-auto">          ← 唯一滚动容器（spec 合同）
      <Suspense fallback={<PageLoading />}><Outlet /></Suspense>
    </div>
  </div>
</div>
```

- 删除 Logo "EA" 方块、应用名与"多工具配置中枢"副标题（应用名由系统标题栏/菜单栏承担）。
- `SidebarItem`：`h-7 rounded-control px-2 gap-2 text-[13px]`；选中
  `bg-accent-soft text-accent font-medium`，hover `bg-muted`；图标 `size-4 shrink-0`。
  `NavLink className` 保持 render-prop 写法，不包进 `cn(...)`（spec 陷阱）。
- 项目行编辑/删除按钮改 `Button variant="ghost" size="icon"`，删除
  `projectRowActionClass`；hover/focus-within 显隐与 `aria-*` 保留；"无法移除…"
  警告文案保留（测试断言且是受阻解释）。
- `ToolSwitcher`：`h-7 rounded-full px-2.5 text-xs`，选中 `bg-accent-soft text-accent
  border-transparent`，未选中 `text-muted-foreground hover:bg-muted`；品牌 PNG 不动。
- `notify.tsx` 浮层定位 `top-14 right-4`（S8），圆角 `rounded-dialog`。
- `PageLoading` 文案保留，样式 `px-8 py-6 text-muted-foreground`。

### 4.3 滚动所有权

不改 `html, body { overflow: hidden }`；外层 `h-screen overflow-hidden`，只有 `<Outlet>`
容器 `overflow-y-auto`。实现与检查时都要在 Tauri 窗口中触控板验证。

## 5. 共享页面头部与空状态

### 5.1 `src/components/page-header.tsx`（H1–H5 / S7）

```tsx
interface PageHeaderProps {
  title: ReactNode;      // h1
  meta?: ReactNode;      // 标题下一行元信息（项目路径、Git/Trust chip）
  actions?: ReactNode;   // 右侧操作区：一颗主操作 + 次要操作/布局切换
  children?: ReactNode;  // 头部下方的 tab / 过滤条
  backTo?: string;       // 可选：左侧 ChevronLeft 返回按钮（项目详情用，替代下划线"← 返回"）
}
```

```
<header class="sticky top-0 z-10 bg-background/85 backdrop-blur border-b px-8 py-4">
  <div class="flex items-center justify-between gap-3">
    <div class="min-w-0 flex items-center gap-2">
      {backTo && <Button asChild variant="ghost" size="icon"><Link to={backTo}><ChevronLeft/></Link></Button>}
      <div class="min-w-0"><h1 class="truncate text-[22px] font-semibold tracking-tight">{title}</h1>{meta}</div>
    </div>
    <div class="flex shrink-0 items-center gap-2">{actions}</div>
  </div>
  {children}
</header>
```

- **没有 description slot**：从组件层面杜绝机制说明段落回流；也不渲染眉标
  （与侧栏选中项重复，H1）。
- 页面主体统一 `<main className="max-w-6xl px-8 py-6 space-y-6">`：**左对齐、不
  `mx-auto`**（S7，桌面应用惯例），删除各 section 上的 `mx-auto max-w-6xl`（32 处）。
- `sticky` 相对 `<Outlet>` 滚动容器生效，不影响滚动所有权；若 WKWebView 实测异常，
  `PageHeader` 单点去掉 `sticky` 退化。
- 标题阶梯（H4）：h1 22px；section h2 `text-[15px] font-semibold`；卡内 h3
  `text-[13px] font-medium`；分组标签 `text-[11px] font-semibold uppercase
  tracking-wide text-muted-foreground`。

各页面接入（操作只搬移不增减，dashboard 例外见 H5）：

| 页面 | title | meta / backTo | actions | children |
| --- | --- | --- | --- | --- |
| dashboard | 总览 | — | 检测现有配置（`RefreshEnvironmentButton` 仍在头部但改 ghost/icon 形态） | — |
| prompts | 提示词 | — | 新建、布局切换 | — |
| mcp | MCP | — | 新建、导入、布局切换 | — |
| hooks | Hooks | — | 新建、导入、布局切换 | 工具选择 tab |
| skills | Skills | — | 导入入口、布局切换 | — |
| projects | 项目 | — | 登记项目 | — |
| projects/detail | `displayName` | meta = 路径 + Git/Trust chip；backTo=/projects | 重新扫描 | 资源类型 tab |
| tool-profiles | `{title}` | — | 新建档案 | 工具 tab |

### 5.2 `src/components/empty-state.tsx`（C7）

```tsx
interface EmptyStateProps { icon?: LucideIcon; title: string; description?: string; action?: ReactNode; }
```

居中 `py-10`，图标 `size-8 text-muted-foreground`，标题 `text-[13px] font-medium`，
一句说明，可选主操作；不用虚线边框。替换 8 处各自写法的空状态（位置见
layout-audit C7）。每个空状态保留"一个明确的下一步操作"（spec Required）。

## 6. 列表与卡片（C1 / C4 / C5 / C6 / C8 / C11）

- **inset grouped list**：section 保留一层 `rounded-lg border bg-card overflow-hidden`，
  内部条目 `divide-y` 行（`min-h-11 px-4 py-2.5`，hover `bg-muted/50`），条目不再
  自带边框与圆角；`pre` 代码块保留 `bg-muted rounded-control`。涉及
  `central-list-layout.tsx`、dashboard、projects、各页"全局目标状态"网格、
  `change-preview-dialog.tsx`、`snapshot-restore-dialog.tsx`、`native-resources.tsx`、
  `option-row.tsx`。
- `CentralListCard` 补 `bg-card`；list/grid 契约（1/2/3 列、等高 body、独立 border-top
  footer、`min-w-0`）不变。
- `CentralListLayoutToggle` 改 segmented control：仅 lucide `List`/`LayoutGrid` 图标、
  `h-7`、相邻共享外框 `rounded-control border`、选中 `bg-muted`；`aria-label`/
  `aria-pressed`/`title` 契约保持。
- 卡片尾部重复提示（prompts / skills 的"图标启用或停用只更新中央配置…"）删除（C6，
  与 copy-audit 一致）。
- 状态徽标 `sync-status-badge.tsx`：去掉 `✓ △ ! ○ ⛔ ×` 字符前缀，改左侧 6px 圆点
  （`before:` 伪元素 + `bg-current`）+ 纯文字，`rounded-full h-5 px-2 text-[11px]`，
  颜色仍走 `toneClass`；`BlockingState` 标题去 "⛔" 改 lucide `OctagonAlert`。
  可见文案被测试断言，改动需同步测试（只删符号，不改文字）。
- dashboard `MetricCard` 数值 `text-2xl font-semibold tabular-nums`，"查看"改
  `Button variant="ghost" size="sm"` + `ChevronRight`；`SummaryItem` 去灰底，改
  `dl` 键值对 + `divide-y`。
- `SettingsDialog`（D9）：四个 section 改为 inset grouped list（一行一设置：左标签、
  右控件），`ThemeToggleGroup` 改 `h-7` segmented；"直接应用"长说明按 copy-audit
  精简为一句。不做左侧 tab 重构。

## 7. 文案删除策略（配合 `research/copy-audit.md`）

- 删除类：页面 `<h1>` 下方的说明 `<p>` 与眉标（7 页）、顶部栏副标题、对话框眉标、
  卡片尾重复提示、判定为"删除"的区块说明。直接删 JSX 节点，不留空容器。
- 精简类：按 audit 建议改写；保持 `role="status"` / `role="alert"` 与
  `aria-describedby` 关联不变；若被精简的文案是某 `aria-describedby` 目标，只改
  内容不删 id。
- 测试同步：每条用 `rg -n "<被删文案前 8 字>" src --glob '*.test.tsx'` 定位，删除
  对应 `getByText`/`findByText` 断言或改为新文案；对话框"关闭"按钮移除后，
  `getByRole("button", {name: /关闭/})` 的断言改为 Esc 或"取消"路径；不允许
  `it.skip`。已知 `app-shell.test.tsx:399` 的"无法移除…"属保留类。

## 8. 兼容与回滚

- `--primary` 改指向 accent 后，旧 `bg-primary` 调用自动变为 accent 色，这是期望
  行为；深色主题主按钮从"白底"变"蓝底"，涉及的 `toHaveClass` 断言在阶段 0 列清单。
- `--radius-sm/--radius-md` 派生变量保留一版映射到 `--radius-control`，阶段 6 `rg`
  确认无引用后删除。
- 每个阶段一个 commit，回滚粒度为阶段；`tauri.conf.json` 的 `hiddenTitle` 与
  阶段 9 的材质配置分属两个 hunk，可各自独立 revert。
- 不涉及数据库、后端 Rust、bindings；`git diff --stat src-tauri` 只允许
  `tauri.conf.json` 与 `capabilities/default.json`。

## 9. 风险

| 风险 | 缓解 |
| --- | --- |
| `#root` 13px 基准让未显式设字号的文本变小、与显式 `text-sm` 混排不齐 | 阶段 4 逐页核对，正文统一删 `text-sm` 用继承；`text-xs` 保留 |
| 删除对话框"关闭"按钮影响按名称选取的测试与键盘用户 | Esc 与 footer"取消"路径保留；测试改走取消按钮；`useDialogFocus` 不动 |
| `sticky` 头部在 WKWebView 内部滚动容器中的表现 | Tauri 窗口实测；异常则 `PageHeader` 去 `sticky` |
| accent 蓝深色主题下 `text-accent` 对比不足 | `.dark --accent` L=0.7；`text-accent` 只用于选中态文字，主按钮用白字 |
| `bg-sidebar` 与 spec 现行字面冲突 | 阶段 7 `update-spec` 同步改写 Theming 段落，并作为本任务验收项 |
| 文案删除误伤 `aria-describedby` 目标或 `role=status/alert` 语义 | §7 规则 + `trellis-check` 检查 a11y 关联 |
| 徽标去符号后可见文案变化 | 仅删符号保留文字；测试断言同步改为纯文字 |
| `transparent: true` 后未铺底色区域透出桌面（"漏底"） | body/#root 透明只为侧栏服务，右列根容器铺 `bg-background`；阶段 9 逐页检查，对话框/浮层保持 `bg-card` |
| 侧栏材质外观与应用手动主题错位 | `applyResolvedTheme` 调 `setTheme`，`system` 传 `null`；Tauri 不存在时静默跳过；验收时三种偏好各切一次 |
| 材质在旧 macOS 或虚拟机上不可用 | `--sidebar` token 保留为回退，`<aside>` 一处类名切换即可退回不透明 |
