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

---

## Shared Central List Controls

MCP and Skills central libraries share `CentralList`,
`CentralListLayoutToggle`, and `PlatformAssignmentButton` from
`src/components/`. Keep their list/grid behavior and Claude/Codex assignment
semantics in these components rather than duplicating Tailwind classes or SVGs
inside feature pages.

```tsx
const [layout, setLayout] = useState<CentralListLayout>("list");

<CentralListLayoutToggle value={layout} onChange={setLayout} />;
<CentralList layout={layout}>{items}</CentralList>;
<PlatformAssignmentButton
  tool={tool}
  assigned={globalTools.includes(tool)}
  disabled={mutation.isPending}
  onClick={toggleAssignment}
/>;
```

Layout choice is transient page state: list remains the default, while grid
uses one column by default, two at `md`, and three at `lg` so a normal desktop
window actually presents three cards per row. Grid cards use equal-height
bodies plus a separate border-top action footer, and children keep `min-w-0`
to contain long paths or previews.

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
