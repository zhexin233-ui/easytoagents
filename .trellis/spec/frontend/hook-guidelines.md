# Hook Guidelines

> How hooks are used in this project.

---

## Overview

Custom hooks are intentionally rare. Use them for reusable stateful React
behavior, not as a second API layer. Server state uses TanStack Query directly
with `queryOptions` factories from `src/lib/*-api.ts`.

---

## Custom Hook Patterns

- Keep a custom hook focused on one reusable lifecycle or interaction contract.
- Return the smallest useful typed surface and keep DOM/native side effects
  inside effects with complete setup and cleanup.
- `useDialogFocus(open, onClose)` is the current reference: it stores the
  previously focused element, focuses the dialog, handles Escape/Tab, and
  restores focus when the dialog closes.

```tsx
import { useEffect, useRef } from "react";

export function useDialogFocus(open: boolean, onClose: () => void) {
  const dialogRef = useRef<HTMLElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return undefined;
    previousFocusRef.current =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const first = dialogRef.current?.querySelector<HTMLElement>(
      "button:not([disabled])",
    );
    (first ?? dialogRef.current)?.focus();
    return () => previousFocusRef.current?.focus();
  }, [open]);

  const onKeyDown = (event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  };

  return { dialogRef, onKeyDown };
}
```

The production hook also wraps Tab focus across every enabled interactive
element; keep that logic and its boundary checks when changing the hook.

### Persisted preview and synchronous form guards

`useSyncPreviewFlow` is the shared lifecycle for global and project artifact
preview/apply flows. Its typed boundary is:

```ts
useSyncPreviewFlow({
  artifactKind,
  preview: (tool) => Promise<Result<PreviewPlan, AppError>>,
  apply: ({ previewId, tool }) => Promise<Result<ApplyResult, AppError>>,
  invalidate,
  messages,
  directApply,
  readopt?,
});
```

The hook always creates a persisted preview first. A non-empty, conflict-free
plan may auto-apply only when `directApply && autoApply`; empty plans notify and
never call Apply; conflicts and errors remain in the review dialog. Apply closes
the dialog, awaits the supplied invalidation, and then notifies. `readopt` is
optional and must invalidate before regenerating a preview. Project-native
disable/restore keeps its dedicated resource mutation because its request is
keyed by `ProjectNativeResourceDto` rather than a `Tool`; it follows the same
no-implicit-write and post-Apply invalidation contract.

`useImportDialogState()` owns `{ tool, requestId }` and changes the request ID
on every rescan so discovery results cannot be reused accidentally.
`useSubmitGuard()` exposes synchronous `begin()`, `end()`, and `isInFlight()`;
call `begin()` before starting a mutation and always call `end()` on settle.
This closes the render-timing gap where `isPending` has not updated yet. Every
in-flight submit guard uses this hook; do not hand-roll `useRef(false)` flags.
A ref is only acceptable for a different contract, such as a one-shot
"attempted" latch that resets on rescan, and it must be commented as such.

---

## Data Fetching

- Create stable query-key factories with readonly tuples (`as const`) in the
  domain API module.
- Wrap generated commands in `queryOptions`; use `unwrapResult` so RPC failures
  become `ProfileRpcError` or the domain-equivalent structured error.
- Pages call `useQuery`/`useMutation` and invalidate every affected key after a
  mutation. Cross-domain mutations invalidate all domains whose row versions or
  inheritance changed. Project-native Apply invalidates `projectKeys.detail`,
  `projectKeys.nativeResources(...)`, MCP, Skill, Prompt, and recovery families
  together.
- The application owns one stable `QueryClient` in `AppProviders`; it is created
  lazily with `useState` and is not recreated on render.

---

## Naming Conventions

- Hook functions start with `use`; hook files use `use-<behavior>.ts`.
- Query option factories describe the resource, for example
  `providerProfilesQueryOptions` and `projectNativeResourcesQueryOptions`, rather than
  pretending to be custom hooks.
- Event handlers returned by a hook use the event name, such as `onKeyDown`.

---

## Common Mistakes

- Wrapping every `useQuery` call in a custom hook and hiding query keys,
  invalidation, or structured errors.
- Omitting cleanup for focus, event listeners, timers, or native subscriptions.
- Recreating `QueryClient` during render or storing server data in a custom
  global state hook.
- Suppressing the React Hooks linter instead of making dependencies and values
  stable.
- Reimplementing preview/apply lifecycle or `saveInFlight` refs in a page when
  the shared hooks already own the contract.
