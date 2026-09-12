# 阶段 0 基线快照（2026-09-12）

## 0.1 样式类断言清单（`toHaveClass` / `toHaveStyle`）

> 格式：文件:行 — 断言类名。阶段 1–5 改动影响到的行需同步更新。

- src/features/skills/skills-page-sync.test.tsx:46 — `size-8`, `p-0`（同步更改图标按钮）
- src/features/skills/skills-page-sync.test.tsx:84 — `size-8`, `p-0`
- src/features/skills/skills-page-sync.test.tsx:249 — `bg-amber-50`（○ 待初始化 toneClass）
- src/app/app-shell.test.tsx:166-172 — 顶部工具入口选中态 `border-primary-foreground`（有/无）
- src/app/app-shell.test.tsx:226 — 项目行移除按钮类（多行断言）
- src/app/app-shell.test.tsx:270 — 项目行编辑按钮类（多行断言）
- src/features/projects/projects-page.test.tsx:79 — `sr-only`（状态码）
- src/features/skills/skills-page.test.tsx:51-110 — CentralList list/grid 结构类：
  :51 `space-y-3` / :52 非 `grid` / :64 grid 类 / :71 非 `space-y-3` / :73 grid 卡类 /
  :79 `flex flex-1 flex-col p-4`（body）/ :80 `mt-auto border-t px-4 py-3`（footer）/
  :92 描述类 / :99 `size-8 p-0` / :110 `ml-auto`
- src/features/skills/skills-page.test.tsx:184,193 — PlatformAssignmentButton 图标
  `opacity-100` / dim 类
- src/features/skills/skills-page-initial-status.test.tsx:290 — `bg-amber-50`（toneClass）
- src/features/projects/project-detail-page.2.test.tsx:96-97 — 同 PlatformAssignmentButton
- src/features/tool-profiles/official-login-section.test.tsx:57,59 — `text-destructive`（有/无）
- src/features/skills/skills-page-detection.test.tsx:104 — toneClass（动态）
- src/features/mcp/mcp-page-conflict.test.tsx:211,573 — `bg-amber-50` / toneClass
- src/features/prompts/prompts-page.test.tsx:187,193 — `size-8`, `p-0`
- src/features/mcp/mcp-page-list-crud.test.tsx:220-271 — CentralList 同 skills-page
  （:220 `space-y-3` / :233 grid / :242 卡类 / :248 body / :249 footer / :260 `size-8 p-0` / :271 `ml-auto`）
- src/features/mcp/mcp-page-list-crud.test.tsx:390-391,418 — PlatformAssignmentButton
- src/features/tool-profiles/tool-profiles-page.test.tsx:528 — toneClass（当前生效）

### toneClass 相关（状态色不改，预期不受影响）

- `bg-amber-50`、`text-destructive`、`bg-amber-*` 等来自 `src/lib/tone-class.ts`，本任务不改 toneClass 语义。

### 预计受阶段改动影响的断言

- 阶段 3（外壳）：app-shell.test.tsx:166-172（工具入口选中态改 accent-soft 后
  `border-primary-foreground` 断言需改）、:226/:270（项目行按钮改 `Button ghost icon`）。
- 阶段 5（central-list-layout）：skills-page.test.tsx:51-110、mcp-page-list-crud.test.tsx:220-271
  （toggle 改 segmented、card 补 bg-card、list px-4 py-3）。
- 徽标去符号：skills-page-sync.test.tsx:249、mcp-page-conflict.test.tsx:211、
  skills-page-initial-status.test.tsx:290 按可见文本 `○ 待初始化` 选取，去符号后改纯文字。

## 0.2 依赖对话框"关闭"按钮的测试

> 阶段 2 删除 `DialogHeader` 右上"关闭"按钮（Onboarding"暂停向导"除外），以下断言改走
> footer"取消"或 Esc 路径。注意区分"关闭 N"这类 aria-label 与可见文案"关闭"。

- src/components/snapshot-restore-dialog.test.tsx:82 — `name: "关闭"`（且断言 toHaveFocus，
  改 Esc 或取消路径需保持焦点语义）
- src/features/skills/skills-page-sync.test.tsx:213 — `name: "关闭"`（断言 disabled；skills 内容预览对话框）
- src/features/skills/skills-page-layout-import.test.tsx:362 — `name: "关闭"`
- src/features/skills/skills-page-detection.test.tsx:522,564 — `name: "关闭 Skills 导入"`
- src/features/prompts/prompts-page.test.tsx:225,243 — `name: "关闭"`
- src/features/mcp/mcp-page-import.test.tsx:515,541 — `name: "关闭 MCP 导入"`
- src/features/mcp/mcp-page-preview.test.tsx:288,323 — `name: "关闭"`
- src/features/projects/project-detail-page.2.test.tsx:545 — `name: "关闭"`
- src/app/app-shell.test.tsx:142 — `name: "关闭"`
- src/features/tool-profiles/tool-profiles-page.test.tsx:309,329 — `name: "关闭"`

## 0.3 样式基线计数（`rg -c`，非测试文件）

> 命令：`rg -c "rounded-xl|rounded-md|shadow-sm|shadow-lg|slate-|bg-white|mx-auto" src --glob '!*.test.tsx'`

| 文件 | 计数 |
|---|---|
| src/app/app-shell.tsx | 4 |
| src/components/change-preview-dialog.tsx | 1 |
| src/components/notify.tsx | 1 |
| src/components/ui/button.tsx | 1 |
| src/components/ui/dialog.tsx | 2 |
| src/components/tool-icon-toggle.tsx | 2 |
| src/features/dashboard/dashboard-page.tsx | 11 |
| src/features/tool-profiles/tool-profiles-page.tsx | 4 |
| src/features/prompts/prompts-page.tsx | 5 |
| src/features/hooks/hooks-page.tsx | 6 |
| src/features/projects/detail/native-resources.tsx | 1 |
| src/features/projects/projects-page.tsx | 4 |
| src/features/tool-profiles/provider-panel.tsx | 2 |
| src/features/settings/settings-dialog.tsx | 1 |
| src/features/projects/detail/assignments/project-assignments-section.tsx | 1 |
| src/features/tool-profiles/official-login-section.tsx | 1 |
| src/features/projects/detail/page.tsx | 5 |
| src/features/skills/skills-page.tsx | 4 |
| src/features/projects/detail/assignments/hook.tsx | 2 |
| src/features/mcp/mcp-page.tsx | 4 |

合计 20 个文件 67 行命中（阶段 7 目标：`rounded-xl|rounded-md|slate-|bg-white|mx-auto` 清零，
`shadow-` 仅剩 dialog.tsx 与 notify.tsx）。

## 0.1b 测试基线

- `pnpm test --run` 基线（2026-09-12，改动前）：**33 个测试文件 / 299 个用例全部通过**，耗时约 42.6s（tests）。
