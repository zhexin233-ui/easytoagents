# 执行计划：前端暗色与亮色主题

前置：`task.py start` 之后才动手；实现前先读 `.trellis/spec/frontend/` 各规范（component / hook / state / quality / directory）。

## 步骤清单

### Step 1 — 样式基座（styles.css）

- [ ] `@custom-variant dark (&:where(.dark, .dark *));`
- [ ] `@theme inline` 增加 `--color-card` / `--color-card-foreground` 映射
- [ ] `:root` 增加 `--card: oklch(1 0 0)`、`--card-foreground`（亮色等值）
- [ ] 新增 `.dark` 变量组（按 design.md 调色板表逐项核对）
- [ ] `:root` 保持 `color-scheme: light`，`.dark` 增加 `color-scheme: dark`
- [ ] `@utility field` 的 `background: white` → `var(--card)`
- 验证：`pnpm typecheck`（CSS 不参与，但确认无破坏）

### Step 2 — 主题 hook 与启动引导

- [ ] 新增 `src/components/use-theme.ts`：`themeStorageKey`、`ThemePreference`、`useTheme()`、`applyThemeFromStorage()`；localStorage try/catch；非法值回退 `system`；`system` 模式订阅 `matchMedia` change（完整 cleanup）；`dark` class 只挂在 `document.documentElement`
- [ ] `src/main.tsx`：render 前同步调用 `applyThemeFromStorage()`（不加 index.html 内联脚本，CSP 不允许）
- 验证：`pnpm test --run use-theme`

### Step 3 — TopBar 三态切换

- [ ] `app-shell.tsx`：侧边栏/顶栏 `bg-white` → `bg-card`
- [ ] TopBar 右侧新增三态按钮组（太阳/月亮/显示器内联 SVG），`aria-pressed` + `title`，选中态 `bg-muted text-foreground`
- [ ] 消费 `useTheme()`，点击设置偏好即时生效
- 验证：`pnpm test --run app-shell`（新增测试见 Step 6）

### Step 4 — 硬编码表面色清扫

- [ ] 全局 `bg-white` → `bg-card`（grep 清点，见 design.md 影响面；逐文件人工确认语义：卡片/面板/对话框表面）
- [ ] 状态色追加 `dark:` 变体：`sync-status-badge.tsx`（含 emerald 成功色调）、`blocking-state.tsx`、各页 `bg-red-50`/`bg-amber-50` 提示区、`text-red-700` 错误文本、`border-red-200`/`border-amber-200` 边框
- [ ] 代码块 `pre`（provider-panel/prompt-panel）暗色可读性处理
- [ ] grep 复查：`grep -rn "bg-white" src` 仅允许出现在注释或确有理由处（逐条说明）

### Step 5 — 测试

- [ ] 新增 `src/components/use-theme.test.ts`：存储初始化、非法值回退、setPreference 持久化、class 切换、system 监听添加/移除/触发（stub matchMedia）
- [ ] AppShell 切换控件测试：三按钮 `aria-pressed` 初始状态、点击后持久化与 class 变化
- [ ] 跑全量测试，修复因颜色类名断言受影响的用例（预期仅少数；`bg-amber-50` 等浅色类保留，不应受影响）

### Step 6 — 视觉验收（两种模式 × 主要页面）

- [ ] `pnpm dev` 启动（127.0.0.1:1420），用浏览器实测：总览 / 项目 / 项目详情 / MCP / Skills / Claude / Codex / 首次运行向导 + 至少一个对话框与导入向导
- [ ] 亮色：与改动前一致；暗色：无白底残留、对比度可读、原生控件（滚动条/密码框）跟随
- [ ] 无闪烁：暗色偏好下刷新，首帧即暗
- [ ] 注：纯浏览器环境 Tauri RPC 不可用，页面会显示查询错误态——错误态样式本身也是验收点之一

### Step 7 — 质量门槛（Phase 2.2 收口）

- [ ] `pnpm format:check` / `pnpm lint` / `pnpm typecheck` / `pnpm test --run`
- [ ] （`pnpm check` 中的 rust:check 本任务未触碰 Rust，可按仓库惯例在最终 check 一并跑）

## 回滚点

- Step 1–2 后回滚：revert styles.css + use-theme.ts + main.tsx 即可。
- 全部步骤均为前端文件，`git checkout -- <files>` / revert 单提交即可恢复。

## 评审门

- Step 4 完成后：grep 复查无遗漏 `bg-white`、无遗漏 `color-scheme` 相关原生控件问题。
- Step 6 视觉验收发现问题：回到对应步骤修复后重跑 Step 7。
