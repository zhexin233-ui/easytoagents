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

## Common Mistakes

- Calling raw `invoke`, asserting an ad-hoc payload, or exposing secret-bearing
  native data from a component.
- Applying native configuration directly after CRUD instead of opening the
  persisted `ChangePreviewDialog` flow.
- Reimplementing `Button`, dialog focus, sync badges, or blocking-state language
  inside a feature.
- Hiding loading, empty, policy, conflict, and RPC failure behind one generic
  message.
