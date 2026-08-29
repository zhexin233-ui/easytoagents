# 技术设计：前端暗色与亮色主题

## 总体方案

沿用项目既有 CSS 变量体系（shadcn 风格 + Tailwind v4 `@theme inline`）：亮色变量保留现状，新增 `.dark` 类下的变量覆盖组；`<html>` 元素上的 `dark` 类是唯一的全局主题信号，所有组件通过 CSS 变量与 `dark:` 变体响应，不需要新 Context 或全局 store。

## 关键决策

### 1. class 策略与 `@custom-variant`

Tailwind v4 默认 `dark:` 变体绑定 `prefers-color-scheme`。为了手动切换，在 `styles.css` 声明：

```css
@custom-variant dark (&:where(.dark, .dark *));
```

绝大部分颜色随 CSS 变量自动切换，`dark:` 变体只用于无法变量化的场景（状态色 red/amber/emerald 的底色与文字、少量阴影）。

### 2. 令牌扩展：新增 `--card`

现状 `bg-white` 承担「卡片/面板」表面色，暗色下需要比 `--background` 略亮一档的表面。新增标准 shadcn 令牌：

- `:root`：`--card: oklch(1 0 0)`（纯白，等值替换现状 `bg-white`），`--card-foreground: var(--foreground)` 同值。
- `.dark`：`--card: oklch(0.21 0.006 285.885)`（zinc-900 一档），`--card-foreground` 同 `--foreground`。
- `@theme inline` 增加 `--color-card` / `--color-card-foreground` 映射。
- 全量替换 `bg-white` → `bg-card`（约 30 处，含 AppShell 侧边栏/顶栏）。
- `@utility field` 的 `background: white` → `background: var(--card)`。

### 3. 暗色调色板（与现有 264–286 色相家族一致，zinc 系）

| 变量 | 亮色（现状不变） | `.dark` |
| --- | --- | --- |
| `--background` | `oklch(0.985 0.002 247.839)` | `oklch(0.141 0.005 285.823)` |
| `--foreground` | `oklch(0.21 0.034 264.665)` | `oklch(0.985 0.002 247.839)` |
| `--card` | `oklch(1 0 0)` | `oklch(0.21 0.006 285.885)` |
| `--primary` | `oklch(0.278 0.033 256.848)` | `oklch(0.985 0.002 247.839)` |
| `--primary-foreground` | `oklch(0.985 0.002 247.839)` | `oklch(0.21 0.034 264.665)` |
| `--muted` | `oklch(0.967 0.003 264.542)` | `oklch(0.274 0.006 285.885)` |
| `--muted-foreground` | `oklch(0.551 0.027 264.364)` | `oklch(0.705 0.015 286.067)` |
| `--border` | `oklch(0.928 0.006 264.531)` | `oklch(1 0 0 / 10%)` |
| `--input` | `oklch(0.928 0.006 264.531)` | `oklch(1 0 0 / 15%)` |
| `--ring` | `oklch(0.707 0.022 261.325)` | `oklch(0.552 0.016 285.938)` |

`color-scheme`：`:root` 保持 `light`，`.dark` 声明 `dark`，使原生滚动条、密码框、下拉控件跟随。

### 4. 状态色的暗色适配策略：追加 `dark:` 变体，不改名

测试（`mcp-page.test.tsx`、`skills-page.test.tsx` 等）断言了 `bg-amber-50`、`bg-red-50` 等浅色类名。为保持既有测试与亮色视觉零回归，状态表面采用「保留原类 + 追加 `dark:` 变体」：

- 错误：`bg-red-50 dark:bg-red-950/40`、`border-red-200 dark:border-red-900/60`、`text-red-700/800 dark:text-red-300`
- 警告：`bg-amber-50 dark:bg-amber-950/40`、`border-amber-200 dark:border-amber-900/60`、`text-amber-800/950 dark:text-amber-300`
- 成功（`SyncStatusBadge`）：`bg-emerald-50 dark:bg-emerald-950/40` 等同模式
- 涉及文件：`sync-status-badge.tsx`、`blocking-state.tsx`、各页错误/警告提示区、`platform-assignment-button.tsx`（如有）
- `bg-slate-950/40` 对话框遮罩两种模式均适用，不改。代码块 `pre` 现为 `bg-white`（如 provider-panel/prompt-panel），替换为 `bg-card` 后如对比不足，追加 `dark:bg-slate-950/60` 一类处理。

### 5. 状态与持久化：共享 hook，不进 Context

- 新文件 `src/components/use-theme.ts`（与 `use-dialog-focus.ts`、`use-persisted-central-list-layout.ts` 同层，遵循「共享行为 = 组件层 hook」约定）：

```ts
export const themeStorageKey = "easytoagents.theme.v1";
export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

export function useTheme(): {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
}
```

- 惰性初始化：`localStorage` try/catch 读取，非法值回退 `"system"`（与 `usePersistedCentralListLayout` 同款容错）。
- 副作用：effect 内解析 `preference === "system" ? matchMedia("(prefers-color-scheme: dark)") : preference`，同步切换 `document.documentElement` 的 `dark` class，并持久化选择。
- 系统监听：仅当 `preference === "system"` 时订阅 `matchMedia` 的 `change` 事件实时重解析，离开该模式或卸载时移除监听（完整 setup/cleanup）。
- `matchMedia` 缺失时按亮色解析并跳过监听（jsdom 测试环境兜底；测试内用 stub 覆盖监听路径）。
- 全局只有 AppShell 一个消费点；其余组件纯靠 CSS 响应，无重复实例问题。

### 6. 无闪烁启动：CSP 约束下的取舍

`src-tauri/tauri.conf.json` 的 CSP 未放行 `script-src 'unsafe-inline'`（回退 `default-src 'self'`），**index.html 内联引导脚本在生产构建会被拦截**，因此不采用常见的 head 内联脚本方案。

改为：`src/main.tsx` 在 `createRoot(...).render(...)` 之前同步调用 `use-theme.ts` 导出的纯函数 `applyThemeFromStorage()`（读偏好 → 算出解析结果 → 直接挂 `dark` class）。文档 body 为空、CSS 为渲染阻塞资源，模块脚本执行先于首帧，类在 React 渲染前已就位，满足无闪烁要求；Phase 2 用浏览器实测确认。

### 7. 切换控件（TopBar）

- 位置：顶栏右侧，工具链接（Claude/Codex）之后，外包一层 `flex items-center gap-2` 与分隔。
- 结构：三枚图标按钮（太阳=亮色、月亮=暗色、显示器=跟随系统），容器 `rounded-md border p-0.5`，选中项 `bg-muted text-foreground`，未选中 `text-muted-foreground hover:bg-muted`。
- 可访问性：每个按钮 `aria-pressed` + `title`（「亮色模式 / 暗色模式 / 跟随系统外观」），图标 `aria-hidden`，按钮文本名由 `title`+`aria-label` 提供；与项目「pressed-button 组」模式一致。
- 图标：项目未安装 lucide-react，沿用现有内联 SVG 写法（同 ProjectNavSection 的 chevron）。
- hook 与控件都在 AppShell 消费；三枚按钮为 AppShell 局部子组件，不晋升 `components/ui`（单一使用点）。

## 影响面

| 文件 | 变更 |
| --- | --- |
| `src/styles.css` | `@custom-variant dark`、`--card` 令牌、`.dark` 变量组、`color-scheme`、`field` 背景 |
| `src/components/use-theme.ts` | 新增：主题偏好 hook + 存储读写/应用纯函数 |
| `src/components/use-theme.test.ts(x)` | 新增：初始化/回退/持久化/class 切换/系统监听 |
| `src/main.tsx` | render 前调用 `applyThemeFromStorage()` |
| `src/app/app-shell.tsx` | TopBar 三态切换、`bg-white` → `bg-card`（侧边栏/顶栏） |
| `src/components/sync-status-badge.tsx`、`blocking-state.tsx` | 状态色 `dark:` 变体 |
| 其余 `bg-white` 所在页/对话框（dashboard、projects、project-detail、mcp、skills、tool-profiles、onboarding、form-dialog、change-preview-dialog、snapshot-restore-dialog、mcp-import-dialog、skill-import-dialog） | `bg-white` → `bg-card`，错误/警告提示区补 `dark:` 变体 |

## 兼容与回滚

- 纯前端变更，不触碰生成绑定与 RPC；无数据迁移。
- localStorage 新 key 与既有 key 互不影响；旧用户默认「跟随系统」。
- 回滚：revert 即可，`--card` 令牌与 `dark:` 变体对亮色为等值或不可见改动。

## 明确不做

- 不做主题跟随窗口装饰/原生标题栏定制。
- 不引入 next-themes 等第三方主题库。
- 不为每个语义状态新建全局令牌（避免大范围重命名测试断言的类名）。
