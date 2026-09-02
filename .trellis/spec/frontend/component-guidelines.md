# Component Guidelines

> How components are built in this project.

---

## Overview

Components are named React functions with explicit props. Feature pages compose
shared primitives and status/dialog components, while server interaction stays
in typed query helpers and mutation callbacks. Components render distinct
loading, empty, error, blocked, and success states rather than collapsing them.

---

## Component Structure

1. Import generated types, shared components, hooks, and API helpers through
   `@/` paths.
2. Define the component-specific props interface near the top of the file.
3. Run hooks before conditional returns, derive view state as local constants,
   and return semantic JSX.
4. Keep small domain-only subcomponents in the page file; move reusable
   interaction contracts to `src/components/`.

`ChangePreviewDialog` is the reference shape:

```tsx
interface ChangePreviewDialogProps {
  preview: PreviewPlan | null;
  tool: Tool;
  artifactKind: ArtifactKind;
  applying: boolean;
  onClose: () => void;
  onApply: (previewId: string, tool: Tool, artifactKind: ArtifactKind) => void;
}

export function ChangePreviewDialog(props: ChangePreviewDialogProps) {
  const { dialogRef, onKeyDown } = useDialogFocus(
    props.preview !== null,
    props.onClose,
  );
  if (!props.preview) return null;
  // Render the typed preview with shared status components.
}
```

---

## Props Conventions

- Use an `interface <ComponentName>Props` for a component's named contract.
- Import generated DTOs with `import type`; do not recreate or weaken them.
- Callbacks use explicit argument types. Pass stable identities such as preview
  ID, tool, artifact kind, project ID, and row version rather than a loose object.
- Primitive wrappers extend the native element contract. `ButtonProps` combines
  `ComponentProps<"button">` with `VariantProps<typeof buttonVariants>`.

---

## Styling Patterns

- Styling uses Tailwind utility classes. Shared primitives use CVA for variants
  and `cn` for class merging, as in `src/components/ui/button.tsx`.
- Use shared status and blocking components for established visual language;
  do not duplicate badge colors or error-state markup in each page.
- Prettier with `prettier-plugin-tailwindcss` is authoritative for class order,
  double quotes, semicolons, and trailing commas.
- App shell scroll ownership is a document-level contract: `html, body` must
  keep `height: 100%; overflow: hidden` in `src/styles.css`, and only the
  shell's content column scrolls. macOS WKWebView (Tauri) wrongly counts an
  inner `overflow-y-auto` container's content toward
  `documentElement.scrollHeight` (Chromium follows spec), so removing that
  rule lets trackpad scrolling move the whole shell out of the viewport and
  expose the body background as a white block below the sidebar — verified
  live in the Tauri window, invisible in Chromium-based tests.

---

## Theming (Dark / Light)

The app supports `light` / `dark` / `system` appearance. The single global
theme signal is the `dark` class on `document.documentElement`; components
never read theme state from React (only `AppShell` consumes `useTheme` for
the toggle). Do not introduce a theme Context or store.

- Colors come from CSS variables declared in `src/styles.css` (`:root` light
  values, `.dark` overrides) and mapped through Tailwind v4 `@theme inline`.
  Class-based dark mode is enabled with
  `@custom-variant dark (&:where(.dark, .dark *));` — the default Tailwind v4
  `dark:` variant tracks `prefers-color-scheme` and would ignore the manual
  toggle.
- Surfaces use semantic tokens, not raw palette classes: `bg-card` for
  cards/panels/dialogs/sidebar/header (never `bg-white`), token utilities
  (`bg-background`, `bg-muted`, `text-muted-foreground`, `border`) otherwise.
  The `field` utility input background is `var(--card)`.
- Status colors (red/amber/emerald notice surfaces) keep their existing light
  classes and APPEND `dark:` variants (e.g.
  `bg-red-50 dark:bg-red-950/40 dark:text-red-300`). Existing tests assert the
  light class names; renaming them breaks tests and the light-mode regression
  guarantee. Icon-button chrome on themed cards (e.g.
  `PlatformAssignmentButton`) must pair every light `border-*`/`bg-*` with a
  `dark:` variant.
- `color-scheme` flips with the theme (`.dark { color-scheme: dark }`) so
  native scrollbars, selects, and password inputs follow.
- No-flash bootstrap lives in `src/main.tsx` (`applyThemeFromStorage()` before
  `createRoot(...).render(...)`), NOT as an inline script in `index.html`: the
  Tauri CSP (`default-src 'self'`, no `script-src 'unsafe-inline'`) blocks
  inline scripts in production builds. `useTheme` stores the raw preference
  at `easytoagents.theme.v1` (`light | dark | system`, invalid → `system`)
  and resolves `system` through `matchMedia` with a live change listener.

```tsx
// Wrong: hardcoded surface breaks dark mode and is grepped for in review.
<section className="rounded-xl border bg-white p-5">

// Correct: token surface; status colors keep light classes + dark variants.
<section className="rounded-xl border bg-card p-5">
<div className="rounded-lg border border-red-200 bg-red-50 p-4 dark:border-red-900/60 dark:bg-red-950/40">
```

---

## Accessibility

- Use native interactive elements and queryable accessible names. Tests select
  by role, label, or visible text rather than `data-testid`.
- Pending content uses `role="status"`; failures use `role="alert"`.
- Dialogs provide `role="dialog"`, `aria-modal`, labelled title/description,
  Escape handling, focus trapping, and focus restoration through
  `useDialogFocus`.
- Disabled or blocked actions must be represented semantically and explained
  in text. Color cannot be the only status signal.

### Icon-only Row Actions

Feature-page row actions that are intentionally icon-only use `lucide-react`
icons inside the shared `Button` primitive. Keep the original action as the
button's accessible name and `title`, hide the decorative icon from assistive
technology, and expose toggle state with `aria-pressed` when applicable.

```tsx
<Button
  type="button"
  size="sm"
  variant="outline"
  className="size-8 p-0"
  aria-label="编辑"
  title="编辑"
  onClick={onEdit}
>
  <Pencil aria-hidden="true" className="size-4" />
</Button>
```

Do not replace established brand assets with Lucide glyphs, and do not rely on
an icon's shape or color as the only action or state cue.

---

## Shared Central List Controls

MCP, Skills, and Prompts central libraries share `CentralList`,
`CentralListLayoutToggle`, and `PlatformAssignmentButton` from
`src/components/`. Keep their list/grid behavior and Claude/Codex assignment
semantics in these components rather than duplicating Tailwind classes or SVGs
inside feature pages. Prompt profiles are tool-agnostic central documents:
each card toggles per-tool global enablement with the shared
`PlatformAssignmentButton` (at most one profile enabled per tool, enforced by
partial unique indexes; enabling a profile replaces the tool's previous one).

```tsx
const [layout, setLayout] = usePersistedCentralListLayout("mcp");

<CentralListLayoutToggle value={layout} onChange={setLayout} />;
<CentralList layout={layout}>{items}</CentralList>;
<PlatformAssignmentButton
  tool={tool}
  assigned={globalTools.includes(tool)}
  disabled={mutation.isPending}
  onClick={toggleAssignment}
/>;
```

### Scenario: Persisted central-list layout

#### 1. Scope / Trigger

- Trigger: a page uses the shared central-list layout toggle and must preserve
  the user's list/grid choice after route unmount and remount.

#### 2. Signatures

- `usePersistedCentralListLayout(preference: "mcp" | "skills" | "prompts")`
  returns a readonly `[CentralListLayout, (layout: CentralListLayout) => void]`
  tuple.
- `centralListLayoutStorageKeys` owns the separate versioned MCP, Skills, and
  Prompts keys; callers must not duplicate their string values.

#### 3. Contracts

- Persist only `"list"` or `"grid"`; never persist central-list data, paths,
  secrets, or native configuration payloads with this hook.
- MCP, Skills, and Prompts preferences are independent. A setter updates React
  state even when `localStorage` cannot be written.
- Missing, invalid, or unreadable storage falls back to `"list"`.

#### 4. Validation & Error Matrix

| Storage condition | Required behavior |
| --- | --- |
| Stored `"list"` or `"grid"` | Restore that layout on mount |
| Missing or any other string | Render list layout |
| `getItem` throws | Render list layout without failing the page |
| `setItem` throws | Keep the newly selected layout for the current mount |

#### 5. Good/Base/Bad Cases

- Good: select grid in MCP, visit another route, then return to the MCP grid
  while Skills keeps its own selection.
- Base: first visit with no stored preference renders list.
- Bad: cast an arbitrary stored string to `CentralListLayout`, share one key
  between both pages, or let a storage exception break the toggle.

#### 6. Tests Required

- Assert each page writes its own key, remounts with the stored layout, and does
  not overwrite the other page's key.
- Assert missing/invalid values and `getItem` failures render list; assert a
  `setItem` failure still updates `aria-pressed` for the current page.

#### 7. Wrong vs Correct

```tsx
// Wrong: route unmount loses the choice.
const [layout, setLayout] = useState<CentralListLayout>("list");

// Correct: page-scoped, validated browser preference.
const [layout, setLayout] = usePersistedCentralListLayout("mcp");
```

Grid uses one column by default, two at `md`, and three at `lg` so a normal
desktop window actually presents three cards per row. Grid cards use
equal-height bodies plus a separate border-top action footer, and children keep
`min-w-0` to contain long paths or previews.

Claude and Codex assignment controls must use unchanged, locally bundled
official brand assets with source and checksum provenance recorded beside the
assets. Do not redraw brand marks with inline SVG paths or generated artwork.
The active asset is shown clearly and the inactive asset is dimmed/grayscaled.
Because dimming is supplementary, every icon-only control must still expose an
accessible name, `title`, and `aria-pressed` that identify the tool and assigned
state. Feature pages still own mutation payloads and domain-specific disabled
rules.

---

## Common Mistakes

- Calling raw `invoke`, asserting an ad-hoc payload, or exposing secret-bearing
  native data from a component.
- Applying native configuration directly after CRUD instead of opening the
  persisted `ChangePreviewDialog` flow.
- Reimplementing `Button`, dialog focus, sync badges, or blocking-state language
  inside a feature.
- Hiding loading, empty, policy, conflict, and RPC failure behind one generic
  message.
- Wrapping a `NavLink`/`Link` `className` render-prop function inside `cn(...)`
  (or any clsx call): clsx skips function arguments silently, so the
  `isActive`/`isPending` classes disappear with no error or test failure. Pass
  the function directly to `className` and call `cn` inside its body.

## 新增与编辑弹窗

- MCP、Provider、Prompt 的新增/编辑使用 `FormDialog`，页面默认只展示列表和操作入口，不平铺表单。新增按钮从空草稿打开，编辑按钮携带安全字段与当前行版本。
- `FormDialog` 接收 `open`、`title`、`description`、`submitLabel`、`pending`、`error`、`onClose`、`onSubmit` 和 `children`；只负责弹窗交互，业务状态和 mutation 留在页面。
- 关闭、取消和 Escape 都清理草稿、编辑模式及旧保存/校验错误；失败保留输入并在弹窗内展示 `role="alert"`，成功等待查询刷新后关闭。CRUD 不触发隐式 Apply。
- 保存期间禁用关闭/取消/提交；页面另用 `saveInFlight` ref 同步阻止重复提交及关闭，不能只依赖下一次渲染才更新的 `isPending`。
- 弹窗限制最大高度，表单内容内部滚动，标题与底部操作保持可见；窄屏不得使表单横向溢出。
- 提交按钮变为 disabled 时，浏览器可能把焦点移到 `body`。提交前必须聚焦弹窗容器；`useDialogFocus` 在容器持焦时将 Tab 导向首个可用控件、Shift+Tab 导向末个可用控件，避免键盘焦点逃逸。

```tsx
// 禁用提交按钮前先保留弹窗焦点。
if (!pending) {
  dialogRef.current?.focus();
  onSubmit(event);
}
```

验证优先扩展现有 MCP 和工具档案页面测试：默认无表单、按钮打开、取消重开清理、精确 payload、失败保留、保存与刷新期间的锁、Tab/Shift+Tab/Escape 和焦点恢复。键盘提交后的原生焦点迁移须用隔离浏览器确认，不能只依赖 jsdom 的 `fireEvent.submit`。
