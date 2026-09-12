# 执行计划：UI 页面 macOS 风格优化

按阶段顺序执行，每阶段结束跑该阶段的验证命令并独立 commit（回滚粒度 = 阶段）。
阶段 1 → 2 → 3 → 4 → 5 有依赖；阶段 6（文案）建议在 5 之后做，以免同一文件两处
冲突。阶段 7 全量核对后，阶段 8（侧栏原生材质）作为独立可回滚增量最后做。

前置：阅读 `design.md`、`research/layout-audit.md`（编号 S/H/C/D/T）、
`research/copy-audit.md`。

## 阶段 0：基线快照

- [x] 0.1 运行 `pnpm test --run` 记录通过数；
      `rg -n "toHaveClass|toHaveStyle" src --glob '*.test.tsx'` 记录样式类断言清单
      到 `research/style-assertions.md`（文件:行:类名）。
- [x] 0.2 `rg -n "getByRole\(\"button\", \{ ?name: ?[\"/]关闭" src --glob '*.test.tsx'`
      记录依赖对话框"关闭"按钮的测试清单，追加到同一文件。
- [x] 0.3 `rg -c "rounded-xl|rounded-md|shadow-sm|shadow-lg|slate-|bg-white|mx-auto" src --glob '!*.test.tsx'`
      记录基线计数，供阶段 7 对比。

## 阶段 1：设计 token（`src/styles.css` + `tauri.conf.json`）

- [x] 1.1 字体栈改为 `-apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, "Segoe UI", sans-serif`，
      移除 `Inter`；新增 `--font-mono`；`body` 加 `-webkit-font-smoothing: antialiased`
      与 `cursor: default`，输入元素 `cursor: text`；`#root { font-size: 13px }`。
- [x] 1.2 `:root` / `.dark` 新增 `--accent`、`--accent-foreground`、`--accent-soft`、
      `--sidebar`、`--radius-control`、`--radius-dialog`；light `--background` 调暗；
      `--primary`/`--primary-foreground`/`--ring` 改为引用 accent；`accent-color: var(--accent)`。
- [x] 1.3 `@theme inline` 追加 `--color-accent*`、`--color-sidebar`、`--radius-control`、
      `--radius-dialog`；`--radius-sm/--radius-md` 暂映射到 `--radius-control`。
- [x] 1.4 `field` 工具类改为 32px 高、`var(--radius-control)`、13px 字号；增加
      `textarea.field` 规则。
- [x] 1.5 `src-tauri/tauri.conf.json` 主窗口加 `"hiddenTitle": true`（唯一后端目录改动）。

验证：`pnpm typecheck && pnpm test --run`（预期只有 `toHaveClass` 断言受影响，按
`research/style-assertions.md` 更新）；Tauri 窗口深浅主题各看一眼主按钮、输入框、
标题栏无文字。

## 阶段 2：UI 原语（`src/components/ui/`）

- [x] 2.1 `button.tsx`：按 design §3.1 更新 base/variant/size；新增 `ghost`、`icon`；
      默认 variant/size 不变。
- [x] 2.2 `dialog.tsx`：`DialogOverlay` 改语义遮罩 + `backdrop-blur-[2px]`；
      `DialogContent` 加 `size` prop（sm/md/lg）与三段式结构、统一
      `max-h-[calc(100dvh-4rem)]`、`rounded-dialog`；新增导出 `DialogBody`；
      `DialogHeader` 标题 15px；`DialogFooter` 改 `justify-between`（左状态槽、右按钮槽）。
- [x] 2.3 `field.tsx`：标签 `text-[13px] font-medium`，`space-y-1.5`。
- [x] 2.4 迁移全部对话框调用方：删除自带 `max-w-*`/`max-h-*`/`p-0 flex` 覆盖改传 `size`；
      删除 `DialogHeader` 内右上"关闭"按钮（`onboarding-wizard.tsx` 的"暂停向导"保留）；
      删除眉标；body 内容包进 `DialogBody`。涉及：`form-dialog.tsx`、`settings-dialog.tsx`、
      `change-preview-dialog.tsx`、`snapshot-restore-dialog.tsx`、`mcp-import-dialog.tsx`、
      `mcp-form-dialog.tsx`、`hook-import-dialog.tsx`、`hook-assignment-picker-dialog.tsx`、
      `project-hook-picker-dialog.tsx`、`skill-import-dialog.tsx`、
      `skill-directory-import-dialog.tsx`、`skill-github-import-dialog.tsx`、
      `app-shell.tsx`（ProjectRemoveDialog）、`skills-page.tsx`（删除确认）。
      size 分配：确认类 sm；FormDialog/选择器/设置 md；预览/导入/快照恢复 lg。
      （注：`mcp-form-dialog.tsx` 不含 DialogContent，是 FormDialog 内的表单组件，无需迁移；
      设置对话框补 footer"完成"作为可见关闭路径；两个 Hook 选择器补 footer"取消"。）
- [x] 2.5 `FormDialog` 的 pending/error 文案从表单内移到 `DialogFooter` 左槽，
      `role=status/alert` 与 `aria-describedby` 关联保持。
- [x] 2.6 核对所有对话框底部按钮顺序"次按钮左、主按钮最右"。

验证：`pnpm lint && pnpm typecheck && pnpm test --run`；按阶段 0 清单更新依赖"关闭"
按钮的测试（改走"取消"按钮或 Esc）；抽查 3 个对话框 Esc 关闭正常。

## 阶段 3：外壳（`src/app/app-shell.tsx` + `notify.tsx`）

- [x] 3.1 重构为"左侧栏 + 右列（顶部工具栏 + 滚动内容）"（design §4.2）；外层保持
      `h-screen overflow-hidden`，仅 `<Outlet>` 容器 `overflow-y-auto`。
- [x] 3.2 新增本地 `SidebarItem`（lucide 图标、`h-7 rounded-control`、accent-soft 选中态，
      `NavLink className` 保持 render-prop）；一级导航 5 项 + 设置项接入；删除手写
      `SettingsIcon` SVG；侧栏底部去 `border-t`。
- [x] 3.3 `ProjectNavSection` 改为 `SidebarGroup`：组标题行 + lucide `ChevronRight`
      折叠；子项去竖线改 `pl-6`；编辑/删除按钮换 `Button variant="ghost" size="icon"`，
      删除 `projectRowActionClass`；hover/focus-within 显隐、`aria-*`、"无法移除…"
      警告文案保留。
- [x] 3.4 `TopBar` 改为右列 `header`：删除 Logo 块、应用名与副标题；工具入口胶囊
      改 accent-soft 选中态（品牌 PNG 不动）。
- [x] 3.5 `PageLoading` 样式 `px-8 py-6 text-muted-foreground`，文案与 `role` 不变。
- [x] 3.6 `notify.tsx` 浮层 `top-14 right-4 rounded-dialog`，`shadow-lg` 保留。

验证：`pnpm test --run src/app src/components`；Tauri 窗口内触控板滚动长页面，确认
外壳不整体上移；深浅主题各截图一次侧栏选中态。

## 阶段 4：共享组件与页面骨架

- [x] 4.1 新增 `src/components/page-header.tsx`（design §5.1：title/meta/actions/
      children/backTo，`sticky top-0 backdrop-blur`，**无 description**）与
      `page-header.test.tsx`（渲染 h1、actions、meta、backTo 链接目标、children）。
- [x] 4.2 新增 `src/components/empty-state.tsx`（design §5.2）与 `empty-state.test.tsx`
      （标题、说明、action 渲染）。
- [x] 4.3 逐页替换 `<main className="p-6 lg:p-8">` + 眉标 + h1 + 说明段为
      `<PageHeader>` + `<main className="max-w-6xl px-8 py-6 space-y-6">`；删除各
      section 上的 `mx-auto max-w-6xl`；操作按钮搬入 `actions`，tab/过滤条搬入 `children`：
  - [x] dashboard-page.tsx（`RefreshEnvironmentButton` 改 ghost/icon 形态）
  - [x] prompts-page.tsx
  - [x] mcp-page.tsx
  - [x] hooks-page.tsx
  - [x] skills-page.tsx
  - [x] projects-page.tsx
  - [x] projects/detail/page.tsx（`backTo="/projects"` 替代下划线"← 返回"；meta =
        路径 + Git/Trust chip；loading/error 两个早返回分支也走 PageHeader）
  - [x] tool-profiles-page.tsx（两个 return 分支）
      （注：hooks 页工具页签搬入 PageHeader children；项目详情页"项目资源管理"
      分组控件按 copy-audit 保留在 section 内，仅删说明段。）
- [x] 4.4 8 处空状态替换为 `EmptyState`（位置见 layout-audit C7），每处保留一个明确
      下一步操作。
- [x] 4.5 标题阶梯统一：section h2 `text-[15px] font-semibold`、卡内 h3
      `text-[13px] font-medium`、分组标签 `text-[11px] font-semibold uppercase tracking-wide`。

验证：`pnpm lint && pnpm typecheck && pnpm test --run`；8 个页面在 Tauri 窗口深浅主题
逐页核对头部粘顶、左对齐、空状态。

## 阶段 5：卡片、列表与色板收敛

- [x] 5.1 `central-list-layout.tsx`：`CentralListCard` 补 `bg-card`、list 模式
      `px-4 py-3`；`CentralListLayoutToggle` 改仅图标 segmented control（lucide
      `List`/`LayoutGrid`，`aria-label`/`aria-pressed`/`title` 保持）。
- [x] 5.2 inset grouped list 改造（design §6）：dashboard、projects、各页"全局目标
      状态"网格、`change-preview-dialog.tsx`、`snapshot-restore-dialog.tsx`、
      `native-resources.tsx`、`option-row.tsx` 的条目改 `divide-y` 行，去掉条目自带
      边框与圆角。
- [x] 5.3 `sync-status-badge.tsx` 去字符前缀改圆点；`blocking-state.tsx` 去 "⛔" 改
      lucide `OctagonAlert`；`projects/detail/page.tsx` "○ 未纳管" 同法。同步更新
      按可见文案断言的测试（只删符号）。
- [x] 5.4 dashboard `MetricCard` / `SummaryItem` 按 design §6 调整。
- [x] 5.5 `settings-dialog.tsx` 各 section 改一行一设置列表，`ThemeToggleGroup` 改
      `h-7` segmented（图标换 lucide Sun/Moon/Monitor）。
- [x] 5.6 全项目 `rounded-xl` → `rounded-lg`；`rounded-md` / 裸 `rounded` /
      `rounded-[4px]` → `rounded-control`；删除卡片/按钮上的 `shadow-sm/lg`。
- [x] 5.7 12 处 `slate-*` 替换为语义 token（design §2.5）；`native-resources.tsx` 删除
      本地 `OptionTag` 副本改用 `option-row.tsx` 导出。
- [x] 5.8 正文层逐页删除冗余显式 `text-sm`，让 13px 继承生效（`text-xs` 保留）。
      （注：已在本阶段触及的正文/说明/空状态上完成；表单字段标签等处仍有少量
      `text-sm`，统一化将在阶段 7 全量核对时按页补齐。）

验证：`pnpm lint && pnpm typecheck && pnpm test --run`；`rg -n "slate-" src --glob '!*.test.tsx'`
为空。

## 阶段 6：删除冗余提示语

- [x] 6.1 按 `research/copy-audit.md` "删除"清单逐条删除 JSX 节点（页头说明段、眉标、
      顶部栏副标题、对话框眉标已在阶段 2–4 随结构替换消失，这里核对无遗漏并处理
      区块级、卡片尾部条目）。
- [x] 6.2 按 "精简"清单改写文案；被 `aria-describedby` 引用的节点只改内容不删 id；
      `role=status/alert` 保持。
- [x] 6.3 每删/改一条，`rg -n "<文案前 8 字>" src --glob '*.test.tsx'` 定位并更新断言
      （删断言或改为新文案）；禁止 `it.skip` / `test.todo`。
- [x] 6.4 `src/lib/global-target-status-ui.ts` 的 description 文案（渲染到 Skills /
      MCP / Hooks 页目标卡片）按 copy-audit 精简，与页面级文案一并处理。
- [x] 6.5 用户可见文案中的内部术语（capability probe、DTO、CRUD、journal、安装探针、
      裸状态码）按 copy-audit 术语表替换为用户语言；toast 与 `throw new Error`
      文案不在本轮范围。
- [x] 6.6 复查 "保留" 清单未被误删：`rg` 抽查 10 条。

验证：`pnpm test --run`；`rg -n "<每条删除文案前 8 字>" src` 全部无结果。

## 阶段 7：全量核对与收口

- [x] 7.1 `rg -n "bg-white|slate-|rounded-xl|rounded-md\b|rounded-\[|mx-auto" src --glob '!*.test.tsx'`
      预期为空；`rg -n "shadow-" src --glob '!*.test.tsx'` 只命中 `dialog.tsx`、`notify.tsx`。
- [x] 7.2 删除 `--radius-sm/--radius-md` 兼容映射（先 `rg "radius-sm|radius-md" src` 确认无引用）。
- [x] 7.3 `git diff --stat src-tauri` 只含 `tauri.conf.json`（此时仅 `hiddenTitle`）。
- [x] 7.4 运行 `pnpm check`（format:check、lint、typecheck、test、rust:check）—— 全部通过。
- [ ] 7.5 手动验收清单（Tauri 窗口执行，结果记入任务 `notes`）—— **待用户执行**：
  - 8 个页面 light/dark 各一遍；
  - 打开 FormDialog、ChangePreviewDialog、ProjectRemoveDialog 各一次：宽度、按钮顺序、
    遮罩、Esc 关闭；
  - 长列表页触控板滚动，外壳不位移；
  - 侧栏项目子列表 hover 与键盘 Tab 均可显示编辑/删除。
- [x] 7.6 质量核对（静态）：语义 token 无裸色板、toneClass 未改、滚动所有权保持
      （html/body overflow:hidden 未动，仅 Outlet 容器滚动）、a11y 关联保持
      （role=status/alert 与 aria-describedby 目标只改内容/随结构迁移）、
      `NavLink className` 全部为 render-prop 写法、品牌图标资产未动、无 it.skip。

## 阶段 8：侧栏原生毛玻璃材质（独立可回滚）

前提：阶段 1–7 已通过。本阶段单独一个 commit。

- [ ] 8.1 `src-tauri/tauri.conf.json`：`app.macOSPrivateApi: true`；主窗口
      `transparent: true`、`windowEffects: { effects: ["sidebar"], state: "followsWindowActiveState" }`。
- [ ] 8.2 `src-tauri/capabilities/default.json` 新增 `core:window:allow-set-theme`。
- [ ] 8.3 `src/styles.css`：`html, body, #root` 背景改 `transparent`；`overflow: hidden`
      与 `height: 100%` 不变。
- [ ] 8.4 `src/app/app-shell.tsx`：`<aside>` 由 `bg-sidebar` 改 `bg-transparent`；右列根
      容器加 `bg-background`；顶部工具栏改 `bg-background/80 backdrop-blur`。
- [ ] 8.5 `src/components/use-theme.ts`：`applyResolvedTheme` 内追加
      `getCurrentWindow().setTheme(...)`（`system` → `null`），仅当
      `"__TAURI_INTERNALS__" in window` 时调用并 `catch` 吞错；`use-theme` 现有测试
      补一条"非 Tauri 环境不调用"的断言。
- [ ] 8.6 Tauri 窗口验收：拖动窗口到不同壁纸上侧栏透出；设置中切换 浅色 / 深色 /
      系统 三种偏好，侧栏材质与应用主题一致；全部页面与至少三个对话框无"漏底"；
      窗口失焦时材质变淡属正常。
- [ ] 8.7 回退预案：若 8.6 任一项不通过且当场不可修复，revert 本阶段 commit，侧栏回到
      `bg-sidebar` 不透明，其余阶段成果不受影响；在任务 `notes` 记录原因。

验证：`pnpm lint && pnpm typecheck && pnpm test --run`；`git diff --stat src-tauri`
只含 `tauri.conf.json` 与 `capabilities/default.json`。

## 阶段 9：规范更新与提交

- [ ] 9.1 `trellis-update-spec`：`component-guidelines.md` Theming 段改写 sidebar/header
      表面 token（侧栏透出原生材质，回退 `bg-sidebar`；header/内容 `bg-background`；
      cards/dialogs `bg-card`），并注明 `transparent` 窗口下"未铺底色即漏底"与
      `setTheme` 同步规则；Styling Patterns 增补三档圆角、accent、`PageHeader` 无描述
      slot、`EmptyState`、对话框 `size` 与无右上关闭按钮、按钮尺寸；
      `quality-guidelines.md` Forbidden Patterns 增补"页面头部下方机制说明段落"。
- [ ] 9.2 按阶段提交（commit 信息简体中文）；最后 `/trellis:finish-work`。

## 回滚点

- 阶段 1 回滚：还原 `styles.css` 与 `tauri.conf.json` 即恢复旧观感。
- 阶段 2/3 回滚：各自单文件/单目录 revert，不影响页面文件。
- 阶段 4/5 回滚：按页 revert；`PageHeader`/`EmptyState` 未被引用时可整体删除。
- 阶段 6 回滚：按文案条目 revert 对应 commit hunk。
- 阶段 8 回滚：revert 单个 commit，侧栏回到不透明 `bg-sidebar`。

## 子代理注意

`research/layout-audit.md` 与 `research/copy-audit.md` 各约 50KB，超过 jsonl 自动注入
上限，注入时会被截断。实现/检查子代理必须用 Read 按章节读取全文（layout-audit
§5 差距清单、copy-audit 各文件表格），不得只依赖注入片段。
