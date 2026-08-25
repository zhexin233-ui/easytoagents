# State Management

> How state is managed in this project.

---

## Overview

The frontend separates local interaction state from backend-owned server state.
React `useState` owns transient UI and form state; TanStack Query owns data
returned by Tauri commands. The project has no Redux, Zustand, or general
application-state Context.

---

## State Categories

- **Local UI state:** dialog visibility, selected tool/project, form fields,
  pending preview, and operation-specific messages use `useState` in the owning
  page or component.
- **Server state:** profiles, projects, MCP servers, Skills, status, previews,
  and dashboard data use TanStack Query and generated Tauri commands.
- **Persistent draft state:** onboarding choices are the existing narrow use of
  `localStorage`; tests clear it explicitly. Do not persist secrets or native
  configuration payloads there.
- **URL state:** navigation and `projectId` route identity use React Router.
- **Derived state:** compute flags such as a blocked preview from current typed
  data during render; do not mirror them into another state variable.

---

## When to Use Global State

There is currently no mutable frontend global store. Promote data only when it
is truly shared:

- Backend-owned data belongs in a TanStack Query cache with a domain key.
- Application infrastructure belongs in `src/app/providers.tsx`.
- Reusable component behavior belongs in a focused custom hook or shared
  component, not in a new global store.
- Keep forms and dialogs local even when they render generated DTOs. Lift them
  only to the nearest common owner that coordinates the interaction.

---

## Server State

- Define key families and `queryOptions` in `src/lib/<domain>-api.ts`.
- Use generated commands and `unwrapResult`; do not cache the generated
  `{ status: "error" }` union as successful data.
- Mutations invalidate all affected key families after success. For example,
  project assignment changes can invalidate project, MCP, and Skill data because
  the shared project row version changes.
- Native configuration writes remain preview-driven. CRUD success updates
  central intent; it does not optimistically claim the native target was applied.

```tsx
export const profileKeys = {
  all: ["profiles"] as const,
  providers: (tool: Tool) => [...profileKeys.all, tool, "providers"] as const,
};

export function providerProfilesQueryOptions(tool: Tool) {
  return queryOptions({
    queryKey: profileKeys.providers(tool),
    queryFn: async () => unwrapResult(await commands.listProviderProfiles(tool)),
  });
}
```

---

## Common Mistakes

- Copying query results into local state and allowing the two sources to drift.
- Storing API keys, headers, environment values, or unredacted native payloads
  in browser persistence.
- Invalidating only the initiating query when a mutation changes shared row
  versions or inheritance.
- Treating a persisted preview as applied state before the explicit Apply
  command succeeds.
