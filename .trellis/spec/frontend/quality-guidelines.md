# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

Feature pages consume generated Tauri commands through typed API helpers. Native
configuration writes are always represented by a persisted preview and confirmed in
the shared change dialog, unless the user opted into the direct-apply mode below.

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
  Because activation commits before preview generation, its query must also be
  invalidated when preview generation fails; the UI must not retain the old active row.
- API-key inputs are passwords and list DTOs expose only `apiKeyConfigured`.
- A Codex profile with `options.providerId === "openai"` and no local API key is an
  OAuth-login profile. Render it as using Codex OAuth credentials, keep edits on
  `SecretUpdate::Keep`, and do not present the missing local key as an error state.
- The shared dialog renders target path, change/status, plan/target warnings,
  conflicts, and only `redactedDiff`; blocked targets disable Apply.
- Claude host-policy and Codex override/unknown states remain distinct, and all
  successful prompt/provider switches state that new sessions normally apply them.
- Provider/Prompt preview can legitimately fail before native reads when there is no
  active central profile and no managed baseline to clean. When the backend returns
  `NOT_FOUND` with `details.resource` equal to `activeProviderProfile` or
  `activePromptProfile`, render an actionable empty state instead of the generic error
  code text.
- Provider/Prompt surfaces are restricted to the shared `PROFILE_TOOLS` set
  (`claude`/`codex`). Cursor has no profile route, tab, query, form, onboarding option,
  or command payload; a stale/manual Cursor profile request must remain backend-rejected.

### 4. Validation & Error Matrix

| UI condition | Required rendering |
| --- | --- |
| Query pending | `role="status"` with feature-specific text |
| Query/mutation error | `role="alert"` with structured RPC message |
| Empty list | One explicit create-or-discover next action |
| Preview lacks active profile and cleanup baseline | Empty-state text; no raw `NOT_FOUND` dead end |
| Stale row version | Preserve the form/list and show conflict; do not retry blindly |
| Preview warning/conflict | Show exact codes; disable Apply for blocked target |
| Import preview | Display credential source plus only redacted projection or intended Prompt body; confirm separately |

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
  Codex OAuth credential-source rendering, loading/error/empty/policy/override states,
  redacted diff, blocked Apply, Escape, close, and focus restoration.

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
- `McpPage` owns central-library CRUD, global tool assignment, status, and global
  preview/apply only. Project option queries, project assignment, and project-scoped
  preview/apply belong to `ProjectDetailPage`; do not reintroduce a project selector on
  the central MCP page.
- MCP global/project tool selectors use the shared capability metadata and include
  Claude, Codex, and Cursor. Cursor selection must use the same persisted
  preview/apply flow and never unlock Provider/Prompt UI.
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
- Assert the central MCP page neither renders project-assignment controls nor calls
  project list/option/assignment commands.
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

- Trigger: Skills import/list/content/delete/status UI, drifted central-content
  adoption, global/project assignment, query keys, or Skills preview/apply behavior
  changes.

### 2. Signatures

- Feature code imports generated `SkillDto`, `SkillContentPreviewDto`, `PreviewPlan`,
  `ApplySkillPreviewInput`, and `commands` only.
- Directory import calls `commands.importSkill({ sourcePath })` after an explicit native
  directory selection. Preview/apply calls `commands.previewSkillSync(...)` and then
  consumes the returned ID with `commands.applySkillPreview(...)`.
- Drifted central content uses `commands.adoptSkillContent({ id, rowVersion })` after a
  `useDialogFocus` confirmation; it is not Preview/Apply.
- Native discovery copy uses `commands.confirmSkillImport({ previewId, candidateIds })`;
  exact formal-root takeover uses the separate
  `commands.prepareSkillTakeover({ previewId, candidateIds })` and consumes only the
  returned `SkillTakeoverPreviewResultDto.plan` through the normal Apply command.

### 3. Contracts

- Import, content preview, deletion, assignments, status, and sync have separate
  accessible pending/error/empty/conflict feedback. Central CRUD and assignment success
  invalidate the entire Skills key family because versions, inheritance, and statuses
  can change together; none applies native writes implicitly.
- `SkillsPage` owns central-library import/content/delete, global tool assignment,
  status, and global preview/apply only. Project option queries, project assignment,
  and project-scoped preview/apply belong to `ProjectDetailPage`; do not reintroduce a
  project selector on the central Skills page.
- Skill global/project tool selectors include Claude, Codex, and Cursor. Cursor native
  import is an explicit user action, and its preview/apply payload preserves the exact
  current tool and project identity.
- The ordinary list renders only the safe description and status diagnostics, never an
  arbitrary frontmatter object or Skill body. Full `SKILL.md` appears only after the
  explicit content-preview command in a closable, Escape-aware dialog.
- 「同步更改」is shown only when `skill.diagnosticCode === "CENTRAL_SKILL_CONTENT_CHANGED"`
  (list and grid), as an icon button matching the other central-card actions
  (`size-8`, `aria-label` / `title`, no visible label). Clicking it opens a
  `useDialogFocus` confirmation asking whether to adopt the current central files as
  authority; primary action is 「是」, secondary is
  「取消」. Escape, close, and cancel send no RPC and restore trigger focus. Confirm
  sends the `id` / `rowVersion` captured when the dialog opened, locks close/resubmit
  while pending, invalidates `skillKeys.all` on success, and never opens
  `ChangePreviewDialog` or calls `previewSkillSync` / `applySkillPreview`. Success copy
  must say the app record was updated and tool-directory links were not rewritten.
- Global assignments remain visually distinct. A global inherited project option is
  checked, read-only, and cannot invoke project assignment. A currently selected invalid
  project item can still be unselected so users can recover.
- Codex untrusted projects visibly disable project preview; backend trust remains
  authoritative. A zero-target inheritance preview shows a no-write explanation and
  never opens Apply. Non-empty plans use `ChangePreviewDialog`, which blocks conflicts
  and applies the exact persisted preview/tool/project identity.
- `SkillImportDialog` renders “复制到中央库” and “接管正式目录” as separate groups
  with independent, initially empty selection sets. Copy candidates and takeover
  candidates never share a submit payload. Takeover preparation locks the modal like
  copy confirmation, invalidates the Skills family, closes the import dialog, and opens
  exactly the returned persisted plan; it never calls Apply itself.
- Takeover copy must explain that an external symlink target is untouched and a real
  directory receives a complete private tree snapshot before replacement. A successful
  preparation message says review/apply is still required, not that native takeover
  already succeeded.

### 4. Validation & Error Matrix

| UI condition | Required rendering/behavior |
| --- | --- |
| Directory chooser/import/content/delete failure | Operation-specific `role="alert"`; preserve unrelated state |
| Skills/status/projects/options pending or empty | Independent status or explicit next-action message |
| Invalid/missing central Skill | Diagnostic visible; new assignment disabled, existing assignment removable |
| `CENTRAL_SKILL_CONTENT_CHANGED` | Show icon button named 「同步更改」; other central diagnostics must not |
| Adopt confirm open / cancel / Escape | No RPC; restore trigger focus |
| Adopt confirm 「是」 | Exact `{ id, rowVersion }`; pending lock; success notify + query family refresh |
| Adopt failure / stale version | Error notify; do not replay the old `rowVersion`; diagnostic remains until a fresh list read |
| Global inherited project option | Read-only inherited label; no project mutation |
| Codex project untrusted | Trust alert and disabled project preview |
| Empty persisted preview | No-write message; no Apply dialog |
| Conflict target | Exact diagnostic/redacted plan; Apply disabled |
| Exact takeover candidate | Select only in takeover group; prepare exact candidate IDs; always open returned preview |
| Takeover preparation stale/error | Keep dialog and structured alert locked against token reuse until explicit rescan |

### 5. Good/Base/Bad Cases

- Good: explicitly choose an isolated source directory, import it, inspect safe list
  metadata, open and close the explicit content preview with focus restoration, assign
  it, then apply the exact non-empty persisted preview.
- Base: list, import, content, deletion, assignment, status, and project-option actions
  keep independent accessible feedback and invalidate the Skills query family without
  writing a native target implicitly.
- Bad: render arbitrary frontmatter/body in the ordinary list, allow a project to toggle
  inherited state, apply an empty/blocked preview, lose dialog focus, bypass generated
  bindings with raw `invoke` or an asserted payload, or route 「同步更改」 through
  `window.confirm`, content preview, or `ChangePreviewDialog`.

### 6. Tests Required

- Mock only generated commands and use an isolated `QueryClient`.
- Assert the central Skills page neither renders project-assignment controls nor calls
  project list/option/assignment commands.
- Cover directory selection, operation-specific loading/errors, list/status/project/
  option empty states, central diagnostics, safe descriptions, content dialog,
  content-dialog focus restoration, deletion conflicts, inherited controls, Codex
  trust, zero-target preview, and exact persisted preview ID/tool/project consumption.
- Assert that fixture Skill bodies and private frontmatter markers are absent from the
  ordinary rendered page and appear only in the explicit content preview when requested.
- Assert copy/takeover grouping, independent selections and exact payloads. Under both
  apply modes, takeover preparation must leave `applySkillPreview` uncalled until the
  user activates `ChangePreviewDialog` Apply.
- Assert 「同步更改」 icon-button visibility (`size-8`, `title`, hidden svg, no
  visible label), no RPC on open/cancel/Escape, exact adopt payload,
  success notify without Preview/Apply, and failure notify with the diagnostic still
  visible.

### 7. Wrong vs Correct

#### Wrong

```tsx
<pre>{JSON.stringify(skill.frontmatter)}</pre>
await invoke("apply_skill_preview", preview);
window.confirm("同步更改?");
```

#### Correct

```tsx
const plan = unwrapResult(await commands.previewSkillSync(input));
await commands.applySkillPreview({ previewId: plan.previewId, tool, projectId });
const adopted = unwrapResult(
  await commands.adoptSkillContent({ id: skill.id, rowVersion: skill.rowVersion }),
);
```

## Scenario: Project detail, dashboard, onboarding, and recovery dialogs

### 1. Scope / Trigger

- Trigger: project list/detail, project MCP/Skill assignment, dashboard cards/history,
  first-run takeover, status badges, blocking states, or snapshot restore UI changes.

### 2. Signatures

- Project CRUD uses generated `RegisterProjectInput`, `VersionedProjectInput`,
  `ProjectDto`, and `RemoveProjectResultDto`; assignment calls send the complete
  generated MCP/Skill input including project/tool/item IDs and both row versions.
- Onboarding consumes generated discovery/import/profile/preview/apply commands.
  Snapshot recovery uses `SnapshotRestoreInput`, `RestorePreview`, and
  `ApplySnapshotRestoreInput` without reconstructing a restore payload locally.
- Dashboard and shared components render generated `DashboardSummaryDto`,
  `RecentSyncRunDto`, `SyncStatus`, `ChangeKind`, and stable error enums.

### 3. Contracts

- Project pages consume generated `ProjectDto` and option DTOs. Global inheritance is
  checked and read-only; there is no project-level global-disable mutation.
- `ProjectDetailPage` is the single UI owner for project MCP/Skill assignment. It uses
  independent local resource (`"mcp" | "skill"`) and tool
  (`"claude" | "codex" | "cursor"`)
  view state, defaults to MCP + Claude, and exposes both switches as accessible
  pressed-button groups. Claude/Codex/Cursor selection uses the bundled brand
  assets with an accessible button name, `title`, and `aria-pressed`; the decorative
  image stays hidden from assistive technology. Mount only the active tool/resource
  assignment view and key that subtree by project, tool, and resource so unsubmitted
  child state cannot leak across combinations.
- Changing either project-detail view axis clears the open preview, operation message,
  and Apply observer state. A mutation that completes after its assignment subtree was
  unmounted may still invalidate server queries, but it must not reopen a preview or
  write a message for the inactive combination. Guard child mutation UI callbacks with
  the mounted-view lifecycle, and keep Apply-only UI updates on the per-call observer
  callback so resetting the observer detaches stale results.
- Either MCP or Skill project assignment invalidates project, MCP, and Skill query-key
  families together because the backend increments the shared project row version.
- Project targets keep capability, policy, trust, missing, parse, permission, managed
  drift, and external same-name conflict states distinct. Unknown is never styled or
  described as synchronized.
- Onboarding follows detect → explicit per-tool choice → persisted preview → exact
  Apply. Detection renders only redacted native evidence. Closing preserves choices;
  reopening re-detects native state and can preview an already imported active central
  profile without confirming the import twice. All-skip calls the typed completion
  command and performs no native Apply. When multiple previews are applied sequentially,
  a partial success removes only consumed previews from the retry set and disables
  returning to the import-selection step; retry must never resubmit a consumed preview.
  A persisted skip choice must not disable an otherwise available Provider/Prompt
  checkbox; selecting Provider/Prompt clears skip so users can recover without first
  toggling skip off.
- `ChangePreviewDialog`, `SyncStatusBadge`, `BlockingState`, and
  `SnapshotRestoreDialog` own the shared status language. Dialogs have labels,
  descriptions, modal semantics, Escape handling, focus trapping/restoration, and
  clear stale state when reopened. Color is never the only status signal.
- When one backend status carries materially different diagnostics, one shared UI helper
  owns the diagnostic-aware label, description, badge tone, and action availability.
  Feature pages consume that presentation instead of parsing diagnostic codes locally;
  an explicit `SyncStatusBadge` tone override is limited to this shared mapping.
- Dashboard counts, recent runs, conflicts, interrupted-run recovery, and snapshots
  come only from generated DTOs; components never parse SQLite/native payloads or show
  snapshot content.
- Snapshot rows render generated `storageKind` and `restorable`. A legacy
  `metadata_only` directory keeps delete selection available but disables restore with
  an explanation. A `directory_tree` restore preview warns that restoring the original
  directory removes the central link and will intentionally surface as external drift.

### 4. Validation & Error Matrix

| UI condition | Required rendering/behavior |
| --- | --- |
| Project/register/rescan/remove pending or stale | Disable duplicate action; preserve context; render structured conflict |
| Inherited MCP/Skill option | Checked/read-only text; no project mutation path |
| Policy/trust/parse/permission/drift/external-name block | Distinct text/code and `BlockingState`; never imply synchronized |
| Assignment success | Invalidate project, MCP, and Skill key families together |
| Project resource/tool view switch | Update both groups' `aria-pressed`; show/query only the active tool/resource combination; reset transient state |
| Mutation completes after a project view switch | Invalidate affected server queries when required; ignore stale preview/message/dialog UI effects |
| Tool onboarding choice omitted | Keep preview disabled until choose import/manage or explicit skip |
| Persisted onboarding skip plus newly available import | Provider/Prompt checkbox remains enabled; selecting it clears skip |
| All tools skipped | Call typed completion only; no preview/apply command |
| Empty or blocked persisted preview | Explain no-write/block; do not expose enabled Apply |
| Dialog close/Escape/reopen | Trap and restore focus; clear stale preview/mutation state |
| Non-restorable metadata-only directory snapshot | Disabled restore action; deletion remains explicit and available |
| Directory-tree restore preview | Show storage type and post-restore drift warning before executing restore |

### 5. Good/Base/Bad Cases

- Good: register an isolated project, inspect distinct target states, assign an
  additional item with the displayed versions, review a persisted preview, and use
  the shared restore dialog whose focus and preview lifecycle are deterministic.
- Base: dashboard and project lists render generated metadata; onboarding detection
  and explicit skip remain read-only and resumable.
- Bad: reuse a stale project version from a selection closure, toggle inherited state,
  serialize an asserted RPC payload, apply on import success, show unknown as healthy,
  or reopen a restore dialog with its previous preview.

### 6. Tests Required

- Mock only generated commands with an isolated `QueryClient`.
- Assert inherited controls cannot mutate, assignment payloads use the displayed row
  versions, and project/MCP/Skill active queries all refetch after either assignment.
- Assert MCP + Claude is the default project view; both directions of the MCP/Skill and
  Claude/Codex/Cursor switches update `aria-pressed`; only the active combination query runs;
  and remounting a combination resets unsubmitted preview-only state such as the local
  Git-exclude checkbox.
- Resolve deferred preview, assignment, and Apply mutations after switching combinations
  and assert they cannot reopen a stale dialog, close the current dialog, or write the
  inactive combination's message. Keep the exact current tool in preview/Apply payload
  assertions.
- Cover explicit all-skip completion, interrupted active-profile preview regeneration,
  redacted discovery/preview rendering, exact preview ID Apply, partial-success retry
  that submits only remaining preview IDs, and no implicit native write command.
- Cover dialog label/modal attributes, Tab containment, Escape, focus restoration,
  blocked Apply, and snapshot-list restoration after closing a preview and reopening.
- Cover payload-file, metadata-only, and directory-tree labels; disabled legacy
  directory restore; directory-tree drift warning; deletion of both restorable and
  non-restorable rows.
- Cover same-status diagnostic variants with visible text, tone semantics, disabled
  actions, and an assertion that blocked actions invoke no preview command.

### 7. Wrong vs Correct

#### Wrong

```tsx
await commands.setProjectMcpAssignment({
  ...option,
  projectId: selectedProject.id,
  projectRowVersion: selectedProject.rowVersion,
});
```

#### Correct

```tsx
const input: SetProjectMcpAssignmentInput = {
  projectId: project.id,
  tool,
  mcpId: option.mcpId,
  assigned: true,
  mcpRowVersion: option.rowVersion,
  projectRowVersion: project.rowVersion,
};
projectAssignmentMutation.mutate(input);
```

---

## Scenario: Direct-apply mode and central-page operation notifications

### 1. Scope / Trigger

- Trigger: any change to global MCP/Skills sync buttons, project MCP/Skill
  append buttons, Provider/Prompt sync and activate buttons, central-list
  assignment or enable toggles, `src/lib/settings-api.ts`, or the settings page
  apply-mode toggle. The notification rules also trigger when an MCP, Skills,
  or Prompts central-page mutation adds or changes transient operation feedback.

### 2. Signatures

- `appSettingsQueryOptions()` (`src/lib/settings-api.ts`) reads the backend
  singleton; pages derive `directApply = settingsQuery.data?.applyMode === "direct"`.
- `canAutoApplyPreview(plan)` mirrors the `ChangePreviewDialog` Apply-enabled
  condition: at least one target, no `conflict` changeKind, no `errorCode`.
- `useNotify()` returns page-local `notification` state plus a `notify({ kind,
  message })` callback. `Notify` renders that state; `kind` is exactly
  `"success" | "error"`, and the shared lifetime is 3,000 ms.
- Central-page previews use page-only `autoApply: boolean` to decide whether a
  safe plan continues into Apply. Apply needs no notification flag; neither
  concept enters generated RPC inputs.

### 3. Contracts

- Direct apply still generates a persisted preview first; the preview is
  auto-confirmed only when `canAutoApplyPreview` is true. Conflicts, errors, or
  blocked targets fall back to opening the preview dialog with Apply disabled.
- Explicit Skill takeover is a hard exception to auto-confirmation: the plan returned by
  `prepareSkillTakeover` always opens `ChangePreviewDialog`, even when direct mode is
  enabled and `canAutoApplyPreview(plan)` is true. This exception applies only to the
  takeover preparation path; ordinary Skills sync retains normal direct-mode behavior.
- Warnings never block auto-apply (same as the dialog). An empty target list
  keeps the existing no-op message and must not apply.
- Settings are backend-owned server state: derive `directApply` from the query,
  never copy it into local state or localStorage. Missing/unloaded settings
  behave as `preview_confirm`.
- The apply itself must keep calling the existing `apply*Preview` command with
  the exact preview ID; no new write path may be introduced.
- Under direct mode, central-list intent mutations auto-trigger the affected
  sync: Skills/MCP global assignment toggles sync that tool, MCP enable/disable
  syncs every tool in the server's `globalTools`, project assignment checkboxes
  sync that project+tool, and Provider/Prompt activation (切换并直接应用)
  previews then auto-applies. Save/delete/import stay manual; their success
  notifications still identify the required preview or sync action.
- The direct-mode branch must run after mutation invalidations so the UI
  reflects committed intent; the backend preview reads committed DB state.
- Central MCP/Skills/Prompts mutation successes, terminal no-ops, and page-level
  failures use shared notification, never persistent `message`, `notice`,
  `applyMessage`, or aggregate error regions. Latest replaces current and restarts
  3,000 ms. Success/no-op uses `status`, failure uses `alert`; render once.
- Query, form, and import-dialog errors plus persistent diagnostics stay inline
  because their correction context must remain visible.
- Manual/direct preview failures notify. Non-empty manual and conflict/blocked
  previews open `ChangePreviewDialog`; zero targets notify success without Apply.
  All manual/automatic Apply results notify.
- Notify only after required invalidation resolves. MCP readopt notifies before
  regenerating preview; a later failure replaces that success notification.

### 4. Tests Required

- With `applyMode: "direct"`: clean preview auto-applies (exact preview ID
  asserted) without the dialog; conflicted preview opens the dialog with Apply
  disabled and never calls apply.
- With `applyMode: "preview_confirm"` (default): existing preview→confirm
  behavior is unchanged and central toggles never trigger an implicit sync.
- Assignment/enable toggles under direct mode assert both the preview command
  payload and the auto-applied preview ID.
- A clean takeover plan under direct mode opens the dialog and asserts zero Apply calls
  until the user explicitly confirms it.
- Settings page: toggle persists both directions and surfaces read failures
  without rendering the toggle.
- Shared-notification fake-timer tests cover 3,000 ms expiry, replacement, and
  unmount cleanup. Central-page tests cover representative CRUD, assignment,
  import/takeover, empty preview, manual/direct Apply, correct role,
  `aria-atomic="true"`, and single rendering.
- Deferred invalidation tests assert no early success or MCP replacement preview.
  Keep form/import errors contextual and `autoApply` out of generated commands.

### 7. Wrong vs Correct

#### Wrong

```tsx
// Feedback persists and page-only metadata leaks into RPC.
setMessage("已应用");
commands.applyMcpPreview({ ...input, notifyResult: true });
```

#### Correct

```tsx
// Auto-apply stays in preview metadata; Apply receives only typed input.
previewMutation.mutate({ tool, autoApply: directApply });
applyMutation.mutate({ input });
await invalidateMcp();
notify({ kind: "success", message: "已应用" });
```

---

## Scenario: Conflict readopt in ChangePreviewDialog

### 1. Scope / Trigger

- Trigger: any change to `ChangePreviewDialog` readopt props, the preview plan
  `baselineMismatchedItems` / `readoptAvailable` fields, or page-level
  `readoptMcpTarget` wiring.

### 2. Signatures

- `ChangePreviewDialog` takes optional `readopting: boolean` and
  `onReadopt: () => void`; the button renders only when the target has
  `readoptAvailable && onReadopt` and sits inside the errorCode block.
- Pages pass the plan identity to `commands.readoptMcpTarget({ tool,
  projectId })`.

### 3. Contracts

- Mismatched items render as「内容不一致的受管条目：a、b」next to the blocking
  state; the button explains that re-adoption only moves baselines and does not
  write files immediately.
- The MCP page closes the dialog, invalidates, and regenerates the preview
  automatically (direct-apply mode then continues into Apply). The project page
  closes the dialog and asks the user to press the sync button again because
  the preview mutation lives in the child components.
- Central-page readopt success notifies after invalidation and before preview
  regeneration; later failures notify error. Project detail keeps local feedback.
- Skills/Provider/Prompt plans never set `readoptAvailable`; do not wire the
  handler there until the backend supports those ownership kinds.
