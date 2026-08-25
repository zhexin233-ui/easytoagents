# Directory Structure

> How frontend code is organized in this project.

---

## Overview

The frontend is organized by application shell, domain feature, shared
component, and typed Tauri boundary. Pages live with their feature-specific
tests; reusable UI and behavior live under `components/`; query and command
adapters live under `lib/`.

---

## Directory Layout

```
src/
├── app/                         # Router, shell and top-level providers
├── bindings/
│   └── commands.ts              # Generated Specta bindings; never hand-edit
├── components/                   # Shared application components and tests
│   ├── ui/                      # Reusable UI primitives such as Button
│   └── use-dialog-focus.ts      # Shared component behavior
├── features/
│   ├── dashboard/
│   ├── mcp/
│   ├── onboarding/
│   ├── projects/
│   ├── skills/
│   └── tool-profiles/           # Domain pages and co-located tests
├── lib/                          # Typed API/query helpers and shared utilities
├── test/
│   └── setup.ts                 # Vitest DOM matcher setup
├── index.css                     # Global Tailwind/theme styles
└── main.tsx                      # Frontend entry point
```

---

## Module Organization

- Add navigable pages under `src/features/<domain>/` and register routes in
  `src/app/router.tsx`.
- Keep feature-only subcomponents next to their page. Promote a component to
  `src/components/` only when it is reused or owns a shared interaction/status
  contract.
- Put reusable primitives and their CVA variants under `src/components/ui/`.
- Put generated-command wrappers, query keys, and `queryOptions` factories in
  `src/lib/<domain>-api.ts`; pages call those helpers and generated `commands`
  rather than raw Tauri `invoke`.
- Co-locate tests as `*.test.tsx`. Shared test setup belongs in `src/test/`.

---

## Naming Conventions

- Files and feature directories use `kebab-case`; React components and exported
  types use `PascalCase`.
- Page files end in `-page.tsx`; query/API modules end in `-api.ts`; custom hooks
  start with `use-`; tests mirror the implementation filename with `.test.tsx`.
- Import application modules through the `@/` alias. Relative imports are kept
  for files that are genuinely local to the same small module.
- Generated DTOs keep the Rust/Specta names from `src/bindings/commands.ts`.

---

## Examples

- `src/app/router.tsx` is the route registry and imports pages from feature
  directories.
- `src/features/projects/project-detail-page.tsx` demonstrates a domain page
  with local subcomponents, React Query mutations, and co-located behavior tests.
- `src/components/change-preview-dialog.tsx` demonstrates a shared accessible
  dialog composed from `Button`, `BlockingState`, `SyncStatusBadge`, and
  `useDialogFocus`.
- `src/lib/profile-api.ts` demonstrates generated command use, stable query-key
  factories, `queryOptions`, and structured RPC error translation.

## Forbidden Patterns

- Do not hand-edit `src/bindings/commands.ts` or copy generated DTO definitions
  into a feature.
- Do not put raw Tauri calls, SQLite/native payload parsing, or cross-feature
  server-state caches inside a component.
- Do not create a generic dumping-ground directory for helpers. Keep a helper
  beside its feature or promote it to the boundary it actually serves.
