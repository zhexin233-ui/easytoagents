# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

Feature pages consume generated Tauri commands through typed API helpers. Native
configuration writes are always represented by a persisted preview and confirmed in
the shared change dialog.

## Forbidden Patterns

- Direct `invoke` calls in feature components, hand-built RPC payload casts, or local
  copies of generated DTO types.
- Rendering API keys, bearer tokens, native secret extensions, or unredacted diffs.
- Applying a Provider/Prompt change from a CRUD success handler without a persisted
  preview dialog.
- Collapsing loading, empty, RPC error, policy-blocked, override, and conflict states
  into one generic message.

## Required Patterns

- Use `commands` and DTOs from `src/bindings/commands.ts`; unwrap the generated
  result union with `unwrapResult` so structured RPC failures reach the UI.
- Use TanStack Query keys/options from `src/lib/profile-api.ts`. Invalidate the source
  tool after CRUD and the target tool after cross-tool copy.
- Keep API key inputs `type="password"`; editing defaults to `SecretUpdate::Keep`.
- Show path, change/status, plan and target warnings, conflicts, and redacted diff in
  `ChangePreviewDialog`. Disable Apply for blocked targets and restore focus on close.
- Give loading text `role="status"`, failures `role="alert"`, and each empty list one
  explicit next action.

## Testing Requirements

- Mock only the generated `commands` object and render with an isolated `QueryClient`.
- Assert the exact RPC payload for secret-update and row-version operations.
- Cover masked inputs, loading/error/empty states, policy/override notices, plan-level
  warnings, redacted previews, and the preview ID consumed by Apply.

## Code Review Checklist

- No raw `invoke`, payload assertion, or secret-bearing UI state was introduced.
- Mutations invalidate every affected query key, including a copied target tool.
- Dialog focus, Escape/close behavior, blocked Apply, and accessible state semantics
  remain intact.

## Scenario: Typed Provider/Prompt profile pages

### 1. Scope / Trigger

- Trigger: any Claude/Codex Provider or Prompt form, mutation, query key, status
  notice, import preview, or shared change-preview dialog change.

### 2. Signatures

- Feature code imports `commands`, `Tool`, `ProviderProfileDto`,
  `PromptProfileDto`, and `PreviewPlan` from generated bindings.
- `ProviderPanel` and `PromptPanel` emit `(PreviewPlan) => void`; the page owns
  the open preview and calls `commands.applyProfilePreview` with its exact ID,
  tool, and artifact kind.
- Provider edits send `SecretUpdate` as `keep`, `clear`, or `replace`; activation
  and deletion send the displayed row version.

### 3. Contracts

- CRUD success only invalidates central-intent queries and shows a no-native-write
  notice. Activation may request a persisted preview but never applies implicitly.
- API-key inputs are passwords and list DTOs expose only `apiKeyConfigured`.
- The shared dialog renders target path, change/status, plan/target warnings,
  conflicts, and only `redactedDiff`; blocked targets disable Apply.
- Claude host-policy and Codex override/unknown states remain distinct, and all
  successful prompt/provider switches state that new sessions normally apply them.

### 4. Validation & Error Matrix

| UI condition | Required rendering |
| --- | --- |
| Query pending | `role="status"` with feature-specific text |
| Query/mutation error | `role="alert"` with structured RPC message |
| Empty list | One explicit create-or-discover next action |
| Stale row version | Preserve the form/list and show conflict; do not retry blindly |
| Preview warning/conflict | Show exact codes; disable Apply for blocked target |
| Import preview | Display only redacted projection or intended Prompt body; confirm separately |

### 5. Good/Base/Bad Cases

- Good: edit a masked Provider, keep the secret, activate with its row version,
  review the persisted preview, and explicitly apply it.
- Base: create/edit/delete central intent and refresh only affected query keys.
- Bad: call raw `invoke`, cast an ad-hoc payload, render a stored secret, silently
  apply after CRUD, or merge blocked/unknown states into a generic failure.

### 6. Tests Required

- Mock the generated `commands` object with an isolated `QueryClient`.
- Assert exact create/update/copy/activate/delete/import/preview/apply payloads,
  including `SecretUpdate`, row versions, tool, artifact kind, and preview ID.
- Cover password masking, multi-env edit preservation, target-tool cache refresh,
  loading/error/empty/policy/override states, redacted diff, blocked Apply, Escape,
  close, and focus restoration.

### 7. Wrong vs Correct

#### Wrong

```tsx
await invoke("update_provider", form as ProviderProfileDto);
```

#### Correct

```tsx
const result = unwrapResult(
  await commands.updateProviderProfile({
    ...input,
    apiKey: { action: "keep" },
    rowVersion: profile.rowVersion,
  }),
);
```
