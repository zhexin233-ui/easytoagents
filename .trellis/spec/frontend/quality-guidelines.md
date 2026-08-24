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

## Scenario: Typed MCP central library and project assignment page

### 1. Scope / Trigger

- Trigger: MCP form/list/status UI, global/project assignment, MCP query keys, or MCP
  preview/apply behavior changes.

### 2. Signatures

- Create/edit forms send generated `McpServerInput` or `UpdateMcpServerInput`;
  sensitive maps/extensions use the generated keep/clear/replace union.
- Assignment mutations send the displayed MCP/project row versions plus exact
  tool/project identity.
- Preview/apply uses `PreviewMcpSyncInput`, `PreviewPlan`, and
  `ApplyMcpPreviewInput` from generated bindings.

### 3. Contracts

- MCP feature code imports generated commands and DTOs only. Query options live in
  `src/lib/mcp-api.ts`, and every successful central mutation invalidates the MCP key
  family because row versions and inheritance can change together.
- Header/env inputs are password fields. Editing starts with `keep`; secret values are
  never reconstructed from header/env names or redacted extension values.
- Global inheritance is visibly read-only and cannot call the project-assignment
  mutation. Disabled central items remain distinguishable from selected/inherited
  state.
- Project and option loading, failure, and empty states are distinct and accessible.
  Codex trust prevents an obviously blocked preview in the UI, while the backend still
  rechecks current native trust.
- Apply consumes the exact persisted MCP preview ID, tool, and project identity through
  `ChangePreviewDialog`; CRUD and assignment success never apply implicitly.

### 4. Validation & Error Matrix

| UI condition | Required rendering/behavior |
| --- | --- |
| Editing sensitive map/extra | Default to `keep`; never reconstruct or display old values |
| Global inherited project option | Read-only inherited label; no disable/remove mutation |
| Disabled MCP | Distinct disabled label independent of assignment state |
| Project/options pending, error, or empty | Separate accessible state for each query |
| Codex project not trusted | Disable obvious preview action; backend remains authoritative |
| Preview has zero targets | Show no-write explanation; do not open an Apply dialog |
| Conflict/error target | Show codes and keep Apply disabled |

### 5. Good/Base/Bad Cases

- Good: edit an MCP while keeping sensitive values, assign it to a trusted project,
  inspect a non-empty redacted preview, and apply its exact preview ID.
- Base: central CRUD/assignment invalidates MCP query keys and displays a no-native-
  write notice.
- Bad: render secret values, let a project disable an inherited item, open Apply for
  an empty preview, or synthesize a payload outside generated bindings.

### 6. Tests Required

- Mock only the generated `commands` object with an isolated `QueryClient`.
- Assert create/update secret payloads, row versions, inherited disabled controls,
  exact project/tool identity, redacted previews, and exact preview ID consumption.
- Cover list/project-option loading, errors, empty states, and the absence of secret
  values in rendered editing state.

### 7. Wrong vs Correct

#### Wrong

```tsx
setHeaders(server.headers);
await commands.setProjectMcpAssignment({ ...option, assigned: false });
```

#### Correct

```tsx
const update: UpdateMcpServerInput = {
  ...safeFields,
  headers: { action: "keep" },
  env: { action: "keep" },
  extra: { action: "keep" },
  rowVersion: server.rowVersion,
};
```

## Scenario: Typed Skills central library and assignment page

### 1. Scope / Trigger

- Trigger: Skills import/list/content/delete/status UI, global/project assignment,
  query keys, or Skills preview/apply behavior changes.

### 2. Signatures

- Feature code imports generated `SkillDto`, `SkillContentPreviewDto`, `PreviewPlan`,
  `ApplySkillPreviewInput`, and `commands` only.
- Directory import calls `commands.importSkill({ sourcePath })` after an explicit native
  directory selection. Preview/apply calls `commands.previewSkillSync(...)` and then
  consumes the returned ID with `commands.applySkillPreview(...)`.

### 3. Contracts

- Import, content preview, deletion, assignments, status, and sync have separate
  accessible pending/error/empty/conflict feedback. Central CRUD and assignment success
  invalidate the entire Skills key family because versions, inheritance, and statuses
  can change together; none applies native writes implicitly.
- The ordinary list renders only the safe description and status diagnostics, never an
  arbitrary frontmatter object or Skill body. Full `SKILL.md` appears only after the
  explicit content-preview command in a closable, Escape-aware dialog.
- Global assignments remain visually distinct. A global inherited project option is
  checked, read-only, and cannot invoke project assignment. A currently selected invalid
  project item can still be unselected so users can recover.
- Codex untrusted projects visibly disable project preview; backend trust remains
  authoritative. A zero-target inheritance preview shows a no-write explanation and
  never opens Apply. Non-empty plans use `ChangePreviewDialog`, which blocks conflicts
  and applies the exact persisted preview/tool/project identity.

### 4. Validation & Error Matrix

| UI condition | Required rendering/behavior |
| --- | --- |
| Directory chooser/import/content/delete failure | Operation-specific `role="alert"`; preserve unrelated state |
| Skills/status/projects/options pending or empty | Independent status or explicit next-action message |
| Invalid/missing central Skill | Diagnostic visible; new assignment disabled, existing assignment removable |
| Global inherited project option | Read-only inherited label; no project mutation |
| Codex project untrusted | Trust alert and disabled project preview |
| Empty persisted preview | No-write message; no Apply dialog |
| Conflict target | Exact diagnostic/redacted plan; Apply disabled |

### 5. Good/Base/Bad Cases

- Good: explicitly choose an isolated source directory, import it, inspect safe list
  metadata, open and close the explicit content preview with focus restoration, assign
  it, then apply the exact non-empty persisted preview.
- Base: list, import, content, deletion, assignment, status, and project-option actions
  keep independent accessible feedback and invalidate the Skills query family without
  writing a native target implicitly.
- Bad: render arbitrary frontmatter/body in the ordinary list, allow a project to toggle
  inherited state, apply an empty/blocked preview, lose dialog focus, or bypass generated
  bindings with raw `invoke` or an asserted payload.

### 6. Tests Required

- Mock only generated commands and use an isolated `QueryClient`.
- Cover directory selection, operation-specific loading/errors, list/status/project/
  option empty states, central diagnostics, safe descriptions, content dialog,
  content-dialog focus restoration, deletion conflicts, inherited controls, Codex
  trust, zero-target preview, and exact persisted preview ID/tool/project consumption.
- Assert that fixture Skill bodies and private frontmatter markers are absent from the
  ordinary rendered page and appear only in the explicit content preview when requested.

### 7. Wrong vs Correct

#### Wrong

```tsx
<pre>{JSON.stringify(skill.frontmatter)}</pre>
await invoke("apply_skill_preview", preview);
```

#### Correct

```tsx
const plan = unwrapResult(await commands.previewSkillSync(input));
await commands.applySkillPreview({ previewId: plan.previewId, tool, projectId });
```
