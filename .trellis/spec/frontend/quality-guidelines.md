# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

Feature pages consume generated Tauri commands through typed API helpers. Native
configuration writes are always represented by a persisted preview and confirmed in
the shared change dialog, unless the user opted into the direct-apply mode below.
Skill takeover preparation and project-native disable/restore are hard exceptions:
they always open `ChangePreviewDialog` and never auto-Apply.

Prompt is global-only. The frontend must not render project Prompt assignment,
PromptFile management, or project Prompt preview/apply controls; project
scanning and native-resource views cover supported MCP and Skill resources only.

## Forbidden Patterns

- Direct `invoke` calls in feature components, hand-built RPC payload casts, or local
  copies of generated DTO types.
- Rendering API keys, bearer tokens, native secret extensions, or unredacted diffs.
- Applying a Provider/Prompt change from a CRUD success handler without a persisted
  preview dialog.
- Auto-applying a project-native disable/restore preview because `applyMode` is
  `"direct"`.
- Collapsing loading, empty, RPC error, policy-blocked, override, and conflict states
  into one generic message.
- Navigating with `window.location.assign("#/...")` or a raw `<a href="#/...">`.
  Route changes go through react-router (`useNavigate` / `<Link to>`), so the
  router state stays authoritative and tests can assert the destination with
  `MemoryRouter`.
- Page-header mechanism-explanation paragraphs: a `PageHeader` has no
  description slot, and feature pages must not render an explanatory
  "只更新中央意图 / 需预览后 Apply" style paragraph under the title. Copy that
  affects user decisions (脱敏、只读、需重启、破坏性确认) lives where it is
  shown.
- Deriving a form control `id` from its label text. Use `useId()` (or an explicit
  `id` prop) to link `<label htmlFor>` to the input so copy changes cannot break
  the accessible association.

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

### Shared test infrastructure

- Use `renderWithProviders` from `src/test/render.tsx` for page and dialog tests.
  It creates a fresh `QueryClient` with query and mutation retries disabled,
  `refetchOnWindowFocus` disabled, and a `MemoryRouter`; pass `queryClient` when
  a test needs to inspect invalidation. Use `path` with `initialEntries` when
  the rendered page reads route parameters.
- Mock the generated command surface with `mockCommands(actual.commands)` from
  `src/test/commands-mock.ts` inside the `vi.mock("@/bindings/commands", ...)`
  factory. The factory derives mocks from `Object.keys`, so adding a generated
  command cannot leave a stale hand-written mock list. Use `okResult` and
  `errResult` for typed result unions.
- Reuse DTO and preview builders from `src/test/fixtures/`; feature-specific
  values belong in builder overrides rather than a second complete `PreviewPlan`
  literal. Global DOM cleanup and jest-dom matchers are registered once in
  `src/test/setup.ts`.
- Keep scenario tests split by behavior when a file grows beyond 900 lines;
  split files must retain the complete assertion set and initialize their own
  command defaults in `beforeEach`.

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
- `options.authKind` (`api_key` | `official_login`) is the only credential-source
  discriminator. Claude/Codex forms offer both kinds as radios on create and render
  the radio disabled on edit (the kind is fixed server-side). An `official_login`
  form hides API URL/key, Claude credential-key, and Codex `wire_api` controls, sends
  empty URL/key with `SecretUpdate::Keep` on edit, and mounts `OfficialLoginSection`.
  Lists render official profiles as “官方账号登录”, disable cross-tool copy for them,
  and never show the missing local key as an error. Default model is optional for
  Claude/Codex; render an empty model as “工具默认模型”.
- `OfficialLoginSection` queries `getOfficialLoginStatus` only while mounted, polls
  every 2 s while `phase === "running"`, and drives `startOfficialLogin` /
  `cancelOfficialLogin` with `type="button"` controls so they never submit the form.
  It renders distinct states for unsupported CLI (manual command, disabled login),
  logged in / not logged in / unknown, and each terminal phase (`succeeded`,
  `failed`, `cancelled`, `timed_out`) with the redacted diagnostic. While running it
  shows `loginUrl` for manual access. When the status is already logged in, starting
  again requires a `confirm`; the Codex copy states that Codex clears its existing
  credentials as soon as login starts and that cancelling means logging in again.
- Import previews (provider panel and onboarding wizard) render `authKind` as the
  credential source and list `skippedEnvKeys` by name when non-empty; values never
  reach the UI.
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
  (`claude`/`codex`/`cursor`/`zcode`). Cursor has a profile route, status query,
  global Prompt tab/import flow, and onboarding Prompt option, but no Provider
  form/query/apply path or project Prompt assignment; capability metadata must keep
  that Provider and project Prompt surface hidden and backend rejection fail-closed.

### 4. Validation & Error Matrix

| UI condition | Required rendering |
| --- | --- |
| Query pending | `role="status"` with feature-specific text |
| Query/mutation error | `role="alert"` with structured RPC message |
| Empty list | One explicit create-or-discover next action |
| Preview lacks active profile and cleanup baseline | Empty-state text; no raw `NOT_FOUND` dead end |
| Stale row version | Preserve the form/list and show conflict; do not retry blindly |
| Preview warning/conflict | Show exact codes; disable Apply for blocked target |
| Import preview | Display credential source plus only redacted projection or intended Prompt body; list skipped env key names; confirm separately |
| Official-login form | No URL/key/`wire_api` controls; login status block with start/cancel/refresh and the manual CLI command |
| Official CLI unsupported or missing | Disabled login button, manual command, diagnostic; form can still be saved |

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
  official-login credential-source rendering, auth-kind radio switching and the
  hidden/disabled controls it implies, start/cancel login payloads and phase text,
  unsupported-CLI fallback, skipped env key listing, loading/error/empty/policy/override
  states, redacted diff, blocked Apply, Escape, close, and focus restoration.

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

- Trigger: project list/detail, project MCP/Skill assignment, project-native
  resource disable/restore, dashboard cards/history, first-run takeover, status
  badges, blocking states, or snapshot restore UI changes.

### 2. Signatures

- Project CRUD uses generated `RegisterProjectInput`, `VersionedProjectInput`,
  `ProjectDto`, and `RemoveProjectResultDto`; assignment calls send the complete
  generated MCP/Skill input including project/tool/item IDs and both row versions.
  Native resource list/preview/apply uses `ProjectNativeResourceQueryInput`,
  `PreviewProjectNativeResourceActionInput` (`resourceId`, `rowVersion`, `action`
  only), and `ApplyProjectNativeResourcePreviewInput` (`previewId` only).
- Onboarding consumes generated discovery/import/profile/preview/apply commands.
  Snapshot recovery uses `SnapshotRestoreInput`, `RestorePreview`, and
  `ApplySnapshotRestoreInput` without reconstructing a restore payload locally.
- Dashboard and shared components render generated `DashboardSummaryDto`,
  `RecentSyncRunDto`, `SyncStatus`, `ChangeKind`, and stable error enums.

### 3. Contracts

- Project pages consume generated `ProjectDto` and option DTOs. Global inheritance is
  checked and read-only; there is no project-level global-disable mutation.
- `ProjectDetailPage` is the single UI owner for project MCP/Skill/Hook assignment
  **and** project-native resources. It uses independent local resource
  (`"mcp" | "hook" | "skill"`) and tool view state, defaults to MCP + the first
  enabled tool, and exposes both switches as accessible pressed-button groups. Tool
  selection uses the bundled brand assets with an accessible button name, `title`, and
  `aria-pressed`; the decorative image stays hidden from assistive technology. Filter
  the resource switch from the active tool's shared capability metadata. Clamp the
  effective resource view during render to the first supported resource, just like the
  enabled-tool fallback; do not repair either state in an effect. Mount and query only
  this effective tool/resource assignment view, and key that subtree by project, tool,
  and effective resource so unsupported combinations (for example OpenCode + Hook)
  never issue an RPC and unsubmitted child state cannot leak across combinations.
  Inside the active combination, render a "项目原生资源" heading
  **above** "中央追加". The project-native resource list contains supported MCP and
  Skill observations only; Hook assignment uses its own central assignment flow.
  Prompt/Rules files are not queried, listed, or managed. Native `safeSummary` and
  diagnostics never render MCP secrets.
- Native disable/restore always call `previewProjectNativeResourceAction` then open
  `ChangePreviewDialog`. `applyMode: "direct"` must not call
  `applyProjectNativeResourcePreview` until the user confirms. Success invalidates
  `projectKeys.detail`, `projectKeys.nativeResources(project, tool, artifactKind)`,
  MCP, Skill, and recovery query families together.
- `active` shows disable; `disabled` shows restore and `disabledAt`; `missing` is
  unrestorable; `conflict` keeps restore materials but blocks the write. An active
  writer or `rollback_failed` is a page-level block, not a per-item conflict.
- Project cards/register feedback render `ProjectDto.nativeResources` counts. Remove
  is disabled while `disabled + conflict > 0`.
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
| Native disable/restore preview | Open `ChangePreviewDialog`; zero Apply calls until confirm, including `applyMode: "direct"` |
| Native Apply success | Invalidate project, native-resources, MCP, Skill, and recovery keys |
| Project has disabled/conflict native resources | Disable remove; show an actionable restore hint |
| Active writer / `rollback_failed` on project detail | Global block; do not present native restore as writable |
| Project resource/tool view switch | Update both groups' `aria-pressed`; show/query only the active tool/resource combination; reset transient state |
| Selected resource unsupported by the next tool | Hide the unsupported resource button and render/query the first supported resource without an intermediate unsupported RPC |
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
  tool switches update `aria-pressed`; only the active combination query runs; and
  remounting a combination resets unsubmitted preview-only state such as the local
  Git-exclude checkbox. Parameterize capability fallbacks: switching from Hook on each
  Hook-capable tool to OpenCode must hide Hook, select MCP, issue no OpenCode Hook
  options query, and render none of the Hook-only error/empty/controls UI.
- Assert the native-resources heading appears above 中央追加; disable/restore with
  `applyMode: "direct"` opens `ChangePreviewDialog` and does not call Apply until
  confirm; MCP fixture secrets never appear in rendered native copy.
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
- `NotifyProvider` owns a `Notification[]` queue and `NotifyViewport` renders
  it once from `AppShell`. `useNotify()` returns `{ notification, notify,
  clear }`; `notification` is the newest item for compatibility, while
  `notify({ kind, message })` appends a queue item. `kind` is exactly
  `"success" | "error"`, and each item lives for 3,000 ms.
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
- Project-native disable/restore is the same class of exception: preview from
  `previewProjectNativeResourceAction` always opens `ChangePreviewDialog`. Direct mode
  must not auto-call `applyProjectNativeResourcePreview`.
- Project-native rows use a dedicated `nativePreview` mutation because their
  request carries `resourceId`, `rowVersion`, and `action` rather than a global
  `Tool`. This is an explicit boundary exception to `useSyncPreviewFlow`, not
  a second write path: it must still open the shared dialog, consume the exact
  persisted `previewId`, invalidate the project scope after Apply, and never
  auto-apply in direct mode.
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
  previews then auto-applies. MCP save syncs the edited server's current
  `globalTools` (create has none yet), MCP delete syncs the deleted server's
  `globalTools` to clean up managed entries, MCP import success syncs the
  imported tool, and global Prompt save/delete sync the profile's global targets.
  Prompt operations never select a project or invoke a project-scoped preview/apply.
  Skill deletion is backend-blocked while assigned and Skills directory import
  owns its own confirm flow, so neither adds auto-sync.
- Direct mode hides the manual global-sync buttons entirely (MCP/Skills status
  cards and the Prompt tool card); default mode keeps 预览/生成全局预览 as the
  only manual sync entry. The per-tool 检测并导入 buttons render in both modes.
  Direct-mode notifications announce auto-sync (e.g. 正在自动同步) and must not
  reference the hidden sync button.
- The direct-mode branch must run after mutation invalidations so the UI
  reflects committed intent; the backend preview reads committed DB state.
- Central MCP/Skills/Prompts mutation successes, terminal no-ops, and page-level
  failures use shared notification, never persistent `message`, `notice`,
  `applyMessage`, or aggregate error regions. The provider appends each item,
  expires it independently after 3,000 ms, and stacks the viewport. Success/no-op
  uses `status`, failure uses `alert`; render the viewport once.
- Query, form, and import-dialog errors plus persistent diagnostics stay inline
  because their correction context must remain visible.
- Manual/direct preview failures notify. Non-empty manual and conflict/blocked
  previews open `ChangePreviewDialog`; zero targets notify success without Apply.
  All manual/automatic Apply results notify.
- Notify only after required invalidation resolves. MCP readopt notifies before
  regenerating preview; a later failure replaces that success notification.
- Every central page's `applyMutation.onSuccess` (MCP, Hooks, Skills, Prompts)
  closes the preview, awaits the page's query-family refresh, then notifies.
  Apply changes native target status, so skipping the refresh leaves status
  cards stale until the next unrelated mutation.

### 4. Tests Required

- With `applyMode: "direct"`: clean preview auto-applies (exact preview ID
  asserted) without the dialog; conflicted preview opens the dialog with Apply
  disabled and never calls apply.
- With `applyMode: "preview_confirm"` (default): existing preview→confirm
  behavior is unchanged and central toggles never trigger an implicit sync.
- Assignment/enable toggles under direct mode assert both the preview command
  payload and the auto-applied preview ID.
- Direct mode asserts the manual global-sync buttons are absent on all three
  pages, and that MCP save/delete/import plus Prompt save/delete auto-trigger
  the same preview + apply payloads; a conflicted preview from those flows
  still falls back to the dialog with Apply disabled.
- A clean takeover plan under direct mode opens the dialog and asserts zero Apply calls
  until the user explicitly confirms it.
- A clean project-native disable/restore plan under direct mode opens the dialog and
  asserts zero `applyProjectNativeResourcePreview` calls until confirm.
- Settings page: toggle persists both directions and surfaces read failures
  without rendering the toggle.
- Apply-success tests on each central page assert the list query was refetched
  (compare the list command's call count before and after Apply).
- Shared-notification fake-timer tests cover independent 3,000 ms expiry,
  stacking, and unmount cleanup. Central-page tests cover representative CRUD, assignment,
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

---

## Scenario: 启用的工具全局设置（enabled-tools display filter）

### 1. Scope / Trigger

- Trigger: any change to `AppSettingsDto` / `UpdateAppSettingsInput` /
  `app_settings` storage keys, or any new render site that draws a tool icon,
  tool entry link, or per-tool card/column.

### 2. Signatures

- Rust (`src-tauri/src/settings.rs`):
  `AppSettingsDto { apply_mode: ApplyMode, enabled_tools: Vec<Tool> }`,
  `UpdateAppSettingsInput` mirrors it; both serialize camelCase
  (`applyMode`, `enabledTools`).
- Storage: `app_settings` KV table, key `enabled_tools`, value is a JSON array
  string (e.g. `["claude","codex"]`); `apply_mode` key unchanged. One command
  `update_app_settings` writes BOTH keys in a single `Immediate` transaction.
- Frontend shared surface: `DEFAULT_ENABLED_TOOLS` and
  `filterEnabledTools<T extends Tool>(tools, enabled: ReadonlySet<Tool>)` in
  `src/lib/tool-metadata.ts`; `useEnabledTools(): ReadonlySet<Tool>` in
  `src/components/use-enabled-tools.ts` (query key `["settings"]`, falls back
  to `DEFAULT_ENABLED_TOOLS` while loading or on error).

### 3. Contracts

- Defaults: missing `enabled_tools` key → `["claude","codex"]` (Cursor off by
  default, it is opt-in). Missing `apply_mode` → `preview_confirm`. Defaults
  resolve per key; never written back implicitly.
- Filtering is display-layer only: assignments (`globalTools`), syncs, and
  stored data for disabled tools stay untouched; profile routes `/claude`
  `/codex` stay reachable. The settings dialog description must keep saying so.
- Toggle submissions are whole-DTO read-modify-write: the dialog always sends
  `{ applyMode, enabledTools }`; `enabledTools` is normalized to canonical
  order `claude → codex → cursor` via `filterEnabledTools(ENABLED_TOOL_ORDER, next)`.
- Every tool-icon render site must derive visibility at render time from
  `useEnabledTools()`: top-bar links, central-list assignment icon groups
  (prompts/MCP/Skills), 全局目标状态 status cards, dashboard tool cards,
  onboarding tool iteration, provider-panel copy-to-counterpart entry, project
  platform selector + 工具配置状态 targets.
- Project detail derives `activeTool = visibleTools.includes(toolView) ?
  toolView : (visibleTools[0] ?? toolView)` at render time; the `toolView`
  state itself is never pruned in an effect.
- Every page that keeps a selected-tool `useState` (project detail, Hooks page,
  any future per-tool view) applies the same render-time clamp and renders
  **all** tool-dependent regions from the clamped value: the selector buttons,
  the status/target card, the per-tool import entry, and the event/assignment
  grouping. Filtering only the selector buttons while other regions still read
  the raw state leaves a disabled tool's status and import entry visible.

### 4. Validation & Error Matrix

- Stored `enabled_tools` is not valid JSON → `DatabaseError`
  （“应用设置包含未知取值”）, no silent fallback.
- Stored array contains a tool outside the `Tool` enum → same `DatabaseError`.
- Empty array is legal: pages degrade to hidden groups/sections, onboarding
  completes with zero prepared tools.

### 5. Good/Base/Bad Cases

- Good: toggle Cursor off → its assignment button, status card, dashboard card
  disappear immediately (shared `["settings"]` cache invalidation); re-enable
  restores them and any stale `toolView === "cursor"` selection re-applies.
- Base: fresh install (no keys) → top bar shows Claude + Codex only.
- Bad: filtering `statusesQuery.data`/`project.targets` by tool id while still
  rendering the section wrapper with zero cards, or clamping `toolView` inside
  a `useEffect` (`react-hooks/set-state-in-effect`).

### 6. Tests Required

- Rust `settings::tests`: per-key defaults, both-key round-trip, reopen
  durability, bogus JSON and unknown-tool values → `DatabaseError`.
- `settings-dialog.test`: default checked states, toggle payload carries full
  `{ applyMode, enabledTools }` in canonical order.
- One hide-assertion per surface: app-shell (top bar), mcp/skills (icon column
  + status card), dashboard (tool card), project-detail and hooks page
  (`activeTool` fallback when the selected tool is disabled — assert the first
  enabled tool's status card is shown, the disabled tool's import entry is
  absent, and there is no unselected intermediate state).
- Every `getAppSettings` mock must include `enabledTools`; TS enforces it.

### 7. Wrong vs Correct

#### Wrong

```tsx
// module-level constant renders disabled tools and never reacts to settings
const toolLinks = PROFILE_TOOLS.map((tool) => ({ ... }));
```

#### Correct

```tsx
function TopBar() {
  const enabledTools = useEnabledTools();
  const toolLinks = filterEnabledTools(PROFILE_TOOLS, enabledTools).map(
    (tool) => ({ ... }),
  );
}
```

## Scenario: Sidebar project soft removal

### 1. Scope / Trigger

- Trigger: any project-list or app-shell action that removes a registered project
  through the existing soft-removal command.

### 2. Signatures

- Frontend command: `commands.removeProject({ id, rowVersion })`.
- Result: `RemoveProjectResultDto { id, removed, nativeConfigurationLeftUnmanaged }`.
- A sidebar row must pass the displayed project's current `id` and `rowVersion`;
  do not reconstruct or omit the version field.

### 3. Contracts

- Removal hides the registration and clears project assignments, but never deletes
  the project directory or rewrites existing native configuration.
- The UI must confirm the project name and soft-removal semantics before calling
  the command, and must prevent navigation from the row while opening confirmation.
- Success invalidates `projectKeys.all`, `mcpKeys.projects()`, and
  `skillKeys.projects()`; a removed current-project route navigates to `/projects`.
- Blocked native resources disable the action and explain the reason. Structured
  command errors keep the project visible and render through `Notify` with
  `role="alert"`.

### 4. Validation & Error Matrix

- `disabled + conflict > 0` → disabled button, visible recovery explanation,
  zero `removeProject` calls.
- Confirmation cancelled or escaped → dialog closes, zero command calls.
- Command succeeds → affected query families invalidate, the row disappears,
  and a `role="status"` notice states that files/configuration were preserved.
- Command returns an error → dialog remains available for retry/cancel, the row
  remains visible, and the structured reason is shown in an alert notification.

### 5. Good/Base/Bad Cases

- Good: capture the row DTO, confirm with the displayed name, send exact
  `{ id, rowVersion }`, invalidate the three affected families, then route away
  only when the removed project is currently open.
- Base: removing a different project leaves the current route unchanged.
- Bad: using a stale local id/version, deleting files from the project root, or
  invalidating only the sidebar list while project MCP/Skill data remains stale.

### 6. Tests Required

- Assert the row button's project-specific accessible name and that clicking it
  does not activate the `NavLink`.
- Assert confirmation text, cancel/Escape behavior, exact command payload,
  blocked-resource behavior, pending duplicate protection, error notification,
  affected query invalidation, and current/non-current route outcomes.

### 7. Wrong vs Correct

#### Wrong

```tsx
onClick={() => commands.removeProject({ id: project.id })}
```

#### Correct

```tsx
onClick={() => {
  event.preventDefault();
  event.stopPropagation();
  openRemoveConfirmation(project);
}}

commands.removeProject({ id: project.id, rowVersion: project.rowVersion });
await invalidate(projectKeys.all, mcpKeys.projects(), skillKeys.projects());
```

### Gotcha: mocking Tauri IPC for browser walkthroughs

> `window.__TAURI_INTERNALS__.invoke(cmd, args)` must return the **raw DTO** —
> Tauri already unwraps Rust `Result::Ok` (and rejects on `Err`), and the
> generated `commands` wrapper adds the `{ status: "ok", data }` envelope
> itself. Returning `{ status, data }` from the mock double-wraps and
> `unwrapResult` hands pages the envelope, crashing on shapes like
> `data.tools.filter`. Commands with input receive `{ input: {...} }` in
> `args`. A root-level `mock.html` (copy of `index.html` plus an inline script
> defining `__TAURI_INTERNALS__` before the module script) lets the dev server
> render the real UI with deterministic data without touching the local app
> database; delete the file before committing.

## Scenario: Backend-owned tool capability metadata

### 1. Scope / Trigger

- Trigger: adding a tool, changing a tool's Provider/MCP/Skills/Hook/Agents support,
  changing Hook event support, or changing the generated capability constants.

### 2. Signatures

- Rust exports specta constants `TOOL_CAPABILITIES` and
  `HOOK_EVENT_SUPPORT` from `src-tauri/src/domain/mod.rs` through the app
  binding registration in `src-tauri/src/lib.rs`.
- Frontend imports the generated readonly constants from
  `src/bindings/commands.ts`; `src/lib/tool-metadata.ts` derives
  `PROFILE_TOOLS`, `MCP_TOOLS`, `SKILL_TOOLS`, `HOOK_TOOLS`, `AGENT_TOOLS`,
  `PROJECT_AGENT_TOOLS`, and event support
  from those values while keeping labels/icons as presentation metadata.

### 3. Contracts

- Every capability row has a generated `tool` plus boolean `provider`,
  `promptGlobal`, `mcp`, `skills`, `hooks`, `agents`, and `projectAgents`
  fields. Every Hook support row
  has a generated `tool` and `event`.
- A tool is included in a frontend capability set exactly when the backend flag
  is `true`; unsupported tools must not render controls or issue unsupported
  queries.
- The binding file is generated output. Rust is the source of truth; do not
  hand-edit `src/bindings/commands.ts` to change capability values.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Rust constant and generated binding differ | `pnpm bindings:check` fails; regenerate bindings |
| Capability flag is false | Hide the corresponding route/control and skip its query |
| Hook event absent for a tool | `hookEventSupportedByTool` returns false and the event is not selectable |
| Agent capability flag is false | Hide the Agents route/control and skip the corresponding list, assignment, or status query |
| New enum/tool variant lacks metadata | TypeScript `Record`/exhaustiveness check fails before runtime |

### 5. Good/Base/Bad Cases

- Good: add one backend capability row, regenerate bindings, and let all page
  filters/tests derive the new support from the generated constant.
- Base: presentation metadata may add a label/icon without changing capability
  truth.
- Bad: maintain a second handwritten support matrix in `hook-events.ts`, show a
  disabled control after issuing its unsupported RPC, expose ZCode in project
  Agents, or edit generated bindings.

### 6. Tests Required

- Assert `TOOL_CAPABILITIES` covers every `Tool` and that each derived frontend
  set equals the backend `true` flags.
- Assert Hook event lookup matches `HOOK_EVENT_SUPPORT` for supported and
  unsupported pairs.
- Assert `AGENT_TOOLS` and `PROJECT_AGENT_TOOLS` exactly match the generated
  `agents`/`projectAgents` flags, including ZCode's global-only support.
- Run `pnpm bindings:generate`, `pnpm bindings:check`, `pnpm check`, and the
  page capability fallback tests whenever the constants change.

### 7. Wrong vs Correct

#### Wrong

```ts
const HOOK_TOOLS = ["claude", "codex", "cursor", "zcode"] as const;
```

#### Correct

```ts
const HOOK_TOOLS = TOOL_CAPABILITIES.filter((item) => item.hooks).map(
  (item) => item.tool,
);
```

## Scenario: Agents 页面与项目页签

### 1. Scope / Trigger

- Trigger：新增或修改 Agents 中央列表、全局分配/导入/同步、项目详情 Agents 页签，或 Agents 的生成命令绑定。

### 2. Signatures

- `agentsQueryOptions()` → `commands.listAgents()`；`globalAgentStatusesQueryOptions()` → `commands.listGlobalAgentTargetStatuses()`。
- `/agents` 页面使用 `commands.createAgent/updateAgent/setAgentEnabled/deleteAgent`，分配使用 `setGlobalAgentAssignment`，同步使用 `previewAgentSync/applyAgentPreview/readoptAgentTarget`。
- 项目页签使用 `agentProjectOptionsQueryOptions(projectId, tool)` 与 `setProjectAgentAssignment`；工具集合必须来自 `PROJECT_AGENT_TOOLS`。

### 3. Contracts

- `TOOL_CAPABILITIES` 是唯一能力来源；`AGENT_TOOLS` 包含五个工具，`PROJECT_AGENT_TOOLS` 排除 ZCode。关闭工具时不渲染其 Agents 控件或发起查询。
- 分配成功只刷新中央意图；默认模式必须打开持久化 Preview 对话框，直接应用模式也只能自动应用无冲突预览。
- 导入对话框只显示全局直属文件候选；`droppedFields` 必须明确展示，用户确认后只创建中央记录，不改写原生文件、不自动分配。
- 全局状态按工具显示聚合状态，能力/策略诊断读取卡片级 `diagnosticCode`，展开可查看每个文件的漂移诊断；Readopt 按目标文件路径触发，不能用目录路径替代。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| ---- | -------- |
| 后端能力为 false（ZCode 项目 Agents） | 不出现在工具切换，不调用项目 Agents 查询/命令 |
| 名称不符合交集规则 | 表单阻止提交并显示小写字母、数字、连字符与 1–64 长度提示 |
| 预览状态为 failed/policy/untrusted/conflict | Apply 按钮禁用；诊断码和中文说明可见 |
| 导入候选不可导入或含 dropped fields | 复选框禁用或显示丢弃提示；不可绕过 UI 校验提交 |

### 5. Good / Base / Bad Cases

- Good：中央 Agent 保存后刷新列表与 Dashboard，分配按钮仅更新意图；用户在状态卡生成预览并确认后文件才变化。
- Base：没有中央 Agent 或没有受管文件时显示可操作空状态，目录探测错误不被渲染成“未接管”。
- Bad：把 Agents 加入 `MCP_TOOLS` 复用项目原生资源查询、在 ZCode 项目页签发送请求、或导入后自动 Apply。

### 6. Tests Required

- Agents 页面测试 CRUD、名称校验、全局分配、状态展开、Preview/Apply、导入 `droppedFields` 展示。
- 项目详情测试全局继承只读、项目追加、Preview/Apply，并断言 ZCode 不在项目工具切换。
- Dashboard/AppShell/tool metadata 测试 Agents 计数、导航顺序与能力集合；运行 `pnpm typecheck`、`pnpm lint`、`pnpm test --run`。

### 7. Wrong vs Correct

#### Wrong

```tsx
// Agents 误用项目原生资源 API，会把整文件目标当作可禁用的条目。
useQuery(projectNativeResourcesQueryOptions(projectId, tool, "agent"));
```

#### Correct

```tsx
// Agents 使用独立的中央/项目分配查询与持久化 Preview 流程。
useQuery(agentProjectOptionsQueryOptions(projectId, tool));
requestPreview(tool, false); // 先展示 Preview，再由用户确认 Apply
```
