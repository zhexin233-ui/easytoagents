# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

Feature pages consume generated Tauri commands through typed API helpers. Accepted
central mutations return backend-computed `affectedSyncScopes`; the shared
`useSyncPreviewFlow` serializes a persisted Preview → Apply chain for every scope.
The persisted Preview remains transaction evidence, but it is not a user-facing
confirmation step. Skill takeover and project-native disable/restore consume their
prepared Preview immediately after the user's explicit primary action.

Prompt is global-only. The frontend must not render project Prompt assignment,
PromptFile management, or project Prompt preview/apply controls; project
scanning and native-resource views cover supported MCP/Skill resources plus
read-only Hook and Agent-file observations.

## Forbidden Patterns

- Direct `invoke` calls in feature components, hand-built RPC payload casts, or local
  copies of generated DTO types.
- Rendering API keys, bearer tokens, native secret extensions, or unredacted diffs.
- Applying a Provider/Prompt change from a CRUD success handler without the shared
  persisted Preview → Apply executor and the backend-returned scope list.
- Writing a native target directly from a status card, CRUD callback, or import
  callback without an observed `ExternalChangePlan` or a persisted Preview.
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
- Show path, status, diagnostic, redacted diff, and available actions in the shared
  status-card components. `ExternalChangeActions` may execute only an exact persisted
  plan whose observed hash, descriptor, ownership, and row versions still match.
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
- Status-card action focus, blocked-action semantics, exact plan consumption, and
  accessible loading/error states remain intact.

## Scenario: Typed Provider/Prompt profile pages

### 1. Scope / Trigger

- Trigger: any Claude/Codex Provider or Prompt form, mutation, query key, status
  scan, ExternalChangePlan action, import preview, or scope-executor change.

### 2. Signatures

- Feature code imports `commands`, `Tool`, `ProviderProfileDto`,
  `PromptProfileDto`, and `PreviewPlan` from generated bindings.
- `ProviderPanel` and `PromptPanel` return typed mutation inputs; the page hands
  the backend result's `affectedSyncScopes` to `useSyncPreviewFlow`, which calls
  `previewProfileSync` and `applyProfilePreview` with each exact scope and ID.
- Provider edits send `SecretUpdate` as `keep`, `clear`, or `replace`; activation
  and deletion send the displayed row version.

### 3. Contracts

- CRUD and activation commit central intent, invalidate it, and immediately execute
  every returned scope. Because activation commits before Preview generation, its
  query must also be invalidated when a scope fails; the UI must not retain the old
  active row. A no-op scope is a successful no-write result.
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
- Status cards render target path, change/status, plan warnings, and only
  `redactedDiff`; `ExternalChangeActions` disables actions when the plan reports a
  blocked target or a stale observation.
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

- Good: edit a masked Provider, keep the secret, and observe the returned scope
  finish Preview → Apply in the same mutation chain.
- Base: create/edit/delete central intent and refresh only affected query keys.
- Bad: call raw `invoke`, cast an ad-hoc payload, render a stored secret, skip the
  returned scope, or merge blocked/unknown states into a generic failure.

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
- Every accepted CRUD and assignment result passes its exact
  `affectedSyncScopes` to the shared executor. Apply consumes the persisted MCP
  preview ID, tool, and project identity for each scope; project-only assignments
  are not inferred from the global list DTO.

### 4. Validation & Error Matrix

| UI condition | Required rendering/behavior |
| --- | --- |
| Editing sensitive map/extra | Default to `keep`; never reconstruct or display old values |
| Global inherited project option | Read-only inherited label; no disable/remove mutation |
| Disabled MCP | Distinct disabled label independent of assignment state |
| Project/options pending, error, or empty | Separate accessible state for each query |
| Codex project not trusted | Disable the status action; backend remains authoritative |
| Preview has zero targets | Show no-write explanation; do not call Apply |
| Conflict/error target | Show codes and keep the scope blocked |

### 5. Good/Base/Bad Cases

- Good: edit an MCP while keeping sensitive values, assign it to a trusted project,
  and let the exact returned project scope finish Preview → Apply.
- Base: central CRUD/assignment invalidates MCP keys and reports a no-op or a
  structured partial-scope result.
- Bad: render secret values, let a project disable an inherited item, apply an empty
  scope, or synthesize a payload outside generated bindings.

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

- Trigger: Skills import/list/content/delete/status UI, external target actions,
  global/project assignment, query keys, or Skills scope-execution behavior changes.

### 2. Signatures

- Feature code imports generated `SkillDto`, `SkillContentPreviewDto`, `PreviewPlan`,
  `ApplySkillPreviewInput`, and `commands` only.
- Directory import calls `commands.importSkill({ sourcePath })` after an explicit native
  directory selection. Preview/apply calls `commands.previewSkillSync(...)` and then
  consumes the returned ID with `commands.applySkillPreview(...)`.
- Central content mutations use their typed row-version command and refresh the
  status/query family; native-target actions always require an observed
  `ExternalChangePlan` before they can write.
- Native discovery copy uses `commands.confirmSkillImport({ previewId, candidateIds })`;
  exact formal-root takeover uses the separate
  `commands.prepareSkillTakeover({ previewId, candidateIds })` and immediately
  consumes only the returned `SkillTakeoverPreviewResultDto.plan` through the typed
  Apply command.

### 3. Contracts

- Import, content preview, deletion, assignments, status, and sync have separate
  accessible pending/error/empty/block feedback. Central CRUD and assignment success
  invalidate the entire Skills key family and execute every returned scope; no page
  guesses project scopes from the global list.
- `SkillsPage` owns central-library import/content/delete, global tool assignment,
  status, and global preview/apply only. Project option queries, project assignment,
  and project-scoped preview/apply belong to `ProjectDetailPage`; do not reintroduce a
  project selector on the central Skills page.
- Skill global/project tool selectors include Claude, Codex, and Cursor. Cursor native
  import is an explicit user action, and its preview/apply payload preserves the exact
  current tool and project identity.
- The ordinary list renders only the safe description and status diagnostics, never an
  arbitrary frontmatter object or Skill body. Full `SKILL.md` appears only after the
  explicit content-preview command in a closable, Escape-aware surface.
- `CENTRAL_SKILL_CONTENT_CHANGED` remains an inline diagnostic. A native external
  change renders `ExternalChangeActions`; its buttons consume the exact plan and
  observed hash, never raw file content. Central-content adoption still captures the
  displayed row version and refreshes `skillKeys.all` on success.
- Global assignments remain visually distinct. A global inherited project option is
  checked, read-only, and cannot invoke project assignment. A currently selected invalid
  project item can still be unselected so users can recover.
- Codex untrusted projects visibly disable project actions; backend trust remains
  authoritative. A zero-target inheritance preview shows a no-write explanation and
  never calls Apply. Non-empty plans consume the exact persisted preview/tool/project
  identity through the scope executor, which blocks unsafe targets.
- `SkillImportDialog` renders “复制到中央库” and “接管正式目录” as separate groups
  with independent, initially empty selection sets. Copy candidates and takeover
  candidates never share a submit payload. Takeover preparation locks the modal like
  copy confirmation, invalidates the Skills family, closes the import dialog, and
  immediately applies exactly the returned persisted plan.
- Takeover copy must explain that an external symlink target is untouched and a real
  directory receives a complete private tree snapshot before replacement. A
  successful preparation message is followed by an in-app Apply result; no second
  confirmation or folder hand-off is allowed.

### 4. Validation & Error Matrix

| UI condition | Required rendering/behavior |
| --- | --- |
| Directory chooser/import/content/delete failure | Operation-specific `role="alert"`; preserve unrelated state |
| Skills/status/projects/options pending or empty | Independent status or explicit next-action message |
| Invalid/missing central Skill | Diagnostic visible; new assignment disabled, existing assignment removable |
| `CENTRAL_SKILL_CONTENT_CHANGED` | Show icon button named 「同步更改」; other central diagnostics must not |
| Central-content row-version change | Error notify; do not replay the old version; diagnostic remains until a fresh list read |
| Global inherited project option | Read-only inherited label; no project mutation |
| Codex project untrusted | Trust alert and disabled project preview |
| Empty persisted preview | No-write message; no Apply call |
| Conflict/blocked target | Exact diagnostic/redacted plan; action disabled |
| Exact takeover candidate | Select only in takeover group; prepare exact candidate IDs; immediately apply returned plan |
| Takeover preparation stale/error | Keep dialog and structured alert locked against token reuse until explicit rescan |

### 5. Good/Base/Bad Cases

- Good: explicitly choose an isolated source directory, import it, inspect safe list
  metadata, open and close the explicit content preview with focus restoration, assign
  it, then apply the exact non-empty persisted preview.
- Base: list, import, content, deletion, assignment, status, and project-option actions
  keep independent accessible feedback and invalidate the Skills query family without
  writing a native target implicitly.
- Bad: render arbitrary frontmatter/body in the ordinary list, allow a project to toggle
  inherited state, apply an empty/blocked plan, bypass generated bindings with raw
  `invoke` or an asserted payload, or write from a status card without a plan.

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
- Assert copy/takeover grouping, independent selections and exact payloads. Takeover
  preparation must consume `applySkillPreview` immediately with the exact returned ID.
- Assert external-change action visibility, redacted summaries, exact plan/action
  payloads, stale-hash blocking, success refresh, and failure diagnostics.

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
  (`"mcp" | "hook" | "skill" | "agent"`) and tool view state, defaults to MCP + the first
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
  Skill observations plus read-only Hook and Agent-file observations; Hook and Agent
  assignment uses its own central assignment flow. Agent-file rows never render a
  disable/restore action and show "Agent 文件暂不支持临时禁用与恢复。".
  Prompt/Rules files are not queried, listed, or managed. Native `safeSummary` and
  diagnostics never render MCP secrets.
- Native disable/restore always call `previewProjectNativeResourceAction` and then
  consume the exact returned plan immediately. Success invalidates
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
  reopening re-detects native state and intersects persisted choices with the current
  evidence. An active central Provider or global Prompt is not rendered as a selectable
  item and never generates a second import/sync preview; when all supported items for a
  tool are active, its card is hidden and treated as resolved. All-skip calls the typed
  completion command and performs no native Apply. When multiple previews are applied
  sequentially, a partial success removes only consumed previews from the retry set and
  disables returning to the import-selection step; retry must never resubmit a consumed
  preview. A persisted skip choice must not disable an otherwise available Provider/Prompt
  checkbox; selecting Provider/Prompt clears skip so users can recover without first
  toggling skip off.
- `ExternalChangeActions`, `SyncStatusBadge`, `BlockingState`, and
  `SnapshotRestoreDialog` own the shared status language. Action surfaces expose
  labels, descriptions, disabled states, and stale-plan feedback. Color is never the
  only status signal.
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
| Native disable/restore intent | Consume the exact prepared plan; zero writes when the plan is empty or blocked |
| Native Apply success | Invalidate project, native-resources, MCP, Skill, and recovery keys |
| Project has disabled/conflict native resources | Disable remove; show an actionable restore hint |
| Active writer / `rollback_failed` on project detail | Global block; do not present native restore as writable |
| Project resource/tool view switch | Update both groups' `aria-pressed`; show/query only the active tool/resource combination; reset transient state |
| Selected resource unsupported by the next tool | Hide the unsupported resource button and render/query the first supported resource without an intermediate unsupported RPC |
| Mutation completes after a project view switch | Invalidate affected server queries when required; ignore stale preview/message/dialog UI effects |
| Active central onboarding item | Hide the item and exclude it from choice, import, and sync preview |
| All supported onboarding items active | Hide the entire tool card and treat it as resolved |
| Tool onboarding choice omitted | Keep preview disabled until choose import/manage or explicit skip |
| Persisted onboarding skip plus newly available import | Provider/Prompt checkbox remains enabled; selecting it clears skip |
| All tools skipped | Call typed completion only; no preview/apply command |
| Empty or blocked persisted preview | Explain no-write/block; do not expose enabled Apply |
| Status action failure/stale | Keep the diagnostic visible, invalidate status, and offer an in-app retry |
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
- Assert the native-resources heading appears above 中央追加; disable/restore
  consumes the exact prepared plan without a second confirmation; MCP fixture
  secrets never appear in rendered native copy.
- Agents view must query `artifactKind: "agent"`, render Agent-file display name,
  file name, description or the redacted-description notice, and the read-only
  explanation without rendering disable/restore or "不可操作" controls.
- Resolve deferred preview, assignment, and Apply mutations after switching combinations
  and assert they cannot reopen a stale dialog, close the current dialog, or write the
  inactive combination's message. Keep the exact current tool in preview/Apply payload
  assertions.
- Cover explicit all-skip completion, interrupted onboarding choice reconciliation when
  an item becomes centrally active, fully/partially managed item filtering,
  redacted discovery/preview rendering, exact preview ID Apply, partial-success retry
  that submits only remaining preview IDs, and no implicit native write command.
- Cover status-action labels, blocked/stale actions, in-app retry, and snapshot-list
  restoration after a failed or completed operation.
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

## Scenario: 统一 Preview → Apply 与外部变化动作

### 1. Scope / Trigger

- Trigger: any central Provider/Prompt/MCP/Skill/Hook/Agent mutation, project
  assignment, Skill takeover, project-native disable/restore, status-card action,
  focus/environment rescan, or operation notification change.

### 2. Signatures

- Every accepted central mutation returns `affectedSyncScopes: SyncScopeDto[]`.
  `useSyncPreviewFlow` accepts these scopes, stable-deduplicates them, and runs one
  typed Preview → Apply chain per scope.
- A scope Preview is persisted and carries `previewId`, descriptor/ownership,
  observed full and managed hashes, row versions, redacted diff, and target status.
  Apply receives only the exact generated input and `previewId`.
- Passive status queries use read-only `scanTarget`/status DTOs. When a target is
  `external_owned_change` or `external_non_owned_change`, `ExternalChangeActions`
  prepares an `ExternalChangePlan`; it exposes “以中央配置覆盖” only when that
  capability is true, and exposes “采纳原生更改” only for an owned target whose
  native-adoption capability is true.
- `AppShell` listens for window focus and environment-ready events, throttles them,
  and invalidates only the visible route's status family. It does not start a watcher
  or background poll.

### 3. Contracts

- The persisted Preview is internal transaction evidence, not a confirmation dialog.
  Accepted central intent automatically consumes every non-empty safe Preview; empty
  scopes finish as no-op. Hard blocks and the second stale result remain visible as
  structured in-app errors with retry.
- Skill takeover and project-native actions preserve their explicit primary click as
  the authorization boundary, then immediately consume the exact prepared Preview.
  They never ask for a second confirmation or send the user to a folder.
- Central mutations invalidate committed intent before generating scopes. Scope order
  is stable and execution is serial; one failure does not race or contaminate another
  target. Final feedback distinguishes success, partial failure, and total failure.
- “以中央配置覆盖” reuses the ordinary persisted Preview → Apply safety boundary.
  It rechecks descriptor, ownership, path/type, observation hash, and all row versions
  before writing; stale returns without a native write and may be re-planned once.
- “采纳原生更改” updates the uniquely matched central entity and its managed
  baseline in the same guarded operation. It never performs baseline-only adoption,
  guesses a renamed/anonymous/unknown item, or exposes secrets. Unmappable targets
  offer an in-app match/import route with a diagnostic.
- Selector projections preserve unknown fields; whole-document projections replace
  only owned documents; symlink projections replace only selected managed names.
  Parse, permission, policy, trust, unsupported, unsafe path, and target-type errors
  remain fail-closed.
- Status cards and actions are non-modal, redacted, keyboard accessible, and show
  path, diagnostic, capability, pending, stale, and retry states. Query invalidation
  completes before success notification; no page-level legacy message region is used.

### 4. Validation & Error Matrix

| UI condition | Required rendering/behavior |
| --- | --- |
| Mutation returns empty scopes | Invalidate intent; show a successful no-op; do not call Apply |
| Mutation returns multiple scopes | Stable dedupe; serial Preview → Apply; aggregate all outcomes |
| First stale Preview | Re-plan the same scope once; never blind-write |
| Second stale / hard block | Stop that scope, keep diagnostic and retry action, report partial/failed result |
| External owned drift with safe mapping | Show both actions with redacted evidence |
| External drift without safe mapping | Disable direct adoption; offer in-app matching/import reason |
| Status action hash/row mismatch | Return stale, invalidate status, and require a fresh plan |
| Focus/environment-ready | Throttled invalidation of only the visible status family |
| Project native disable/restore or Skill takeover | Apply exact prepared Preview immediately; preserve snapshots and rollback |

### 5. Good / Base / Bad Cases

- Good: edit the active Provider, receive its global scope, and observe the native
  target reach in-sync without switching profiles or opening a confirmation surface.
- Good: a passive scan finds a safe external Provider change; “采纳原生更改” updates
  the central row and baseline, while “以中央配置覆盖” writes the observed plan.
- Base: an unassigned create returns no scopes and changes no native file; a project-only
  assignment returns exactly one project scope.
- Bad: infer scopes from `globalTools`, auto-write from a status DTO, reuse a stale
  plan, silently absorb unknown fields, or finish with a folder-hand-off instruction.

### 6. Tests Required

- Assert every six-resource mutation consumes backend scopes, including active Provider
  edit, project-only assignment, deletion cleanup, Hook add/switch/remove symmetry,
  empty no-op, stable dedupe, serial order, partial failure, and one bounded stale retry.
- Assert status pages render redacted external evidence and both action payloads; safe
  adoption updates the central entity and baseline, coverage preserves unknown fields,
  stale hash/row versions perform zero writes, and unmappable items route to in-app
  matching/import.
- Assert focus, environment-ready, page entry, and explicit rescan refresh only the
  visible status family and do not create watcher/polling calls.
- Assert Skill takeover and project-native disable/restore apply exact returned IDs
  immediately, preserve snapshot/recovery behavior, and never render a second
  confirmation or hand-off message.
- Assert query invalidation precedes success notification, and failures remain
  accessible with `role="alert"` plus an in-app retry.

### 7. Wrong vs Correct

#### Wrong

```tsx
// Status DTO is not an authorization or write plan.
commands.applyMcpPreview({ previewId: status.targetPath });
```

#### Correct

```tsx
const scopes = mutationResult.affectedSyncScopes ?? [];
await requestPreview(scopes);
// ExternalChangeActions applies only the exact persisted plan after hash/row checks.
```

---

## Scenario: 启用的工具全局设置（enabled-tools display filter）

### 1. Scope / Trigger

- Trigger: any change to `AppSettingsDto` / `UpdateAppSettingsInput` /
  `app_settings` storage keys, or any new render site that draws a tool icon,
  tool entry link, or per-tool card/column.

### 2. Signatures

- Rust (`src-tauri/src/settings.rs`):
  `AppSettingsDto { enabled_tools: Vec<Tool> }`; `UpdateAppSettingsInput` mirrors
  it and serializes `enabledTools`.
- Storage: `app_settings` KV table, key `enabled_tools`, value is a JSON array
  string (e.g. `["claude","codex"]`). The historical `apply_mode` key may remain
  unread, and `update_app_settings` writes only enabled tools.
- Frontend shared surface: `DEFAULT_ENABLED_TOOLS` and
  `filterEnabledTools<T extends Tool>(tools, enabled: ReadonlySet<Tool>)` in
  `src/lib/tool-metadata.ts`; `useEnabledTools(): ReadonlySet<Tool>` in
  `src/components/use-enabled-tools.ts` (query key `["settings"]`, falls back
  to `DEFAULT_ENABLED_TOOLS` while loading or on error).

### 3. Contracts

- Defaults: missing `enabled_tools` key → `["claude","codex"]` (Cursor off by
  default, it is opt-in). The default is resolved without an implicit write.
- Filtering is display-layer only: assignments (`globalTools`), syncs, and
  stored data for disabled tools stay untouched; profile routes `/claude`
  `/codex` stay reachable. The settings dialog description must keep saying so.
- Toggle submissions are whole-DTO read-modify-write: the dialog always sends
  `{ enabledTools }`; the list is normalized to canonical order
  `claude → codex → cursor` via `filterEnabledTools(ENABLED_TOOL_ORDER, next)`.
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
- `settings-dialog.test`: default checked states and toggle payload carrying
  `{ enabledTools }` in canonical order.
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
- `/agents` 页面使用 `commands.createAgent/updateAgent/setAgentEnabled/deleteAgent`，分配使用 `setGlobalAgentAssignment`，同步使用 `useSyncPreviewFlow` 消费后端返回的 `affectedSyncScopes`；状态操作使用 `ExternalChangeActions`。
- 项目页签使用 `agentProjectOptionsQueryOptions(projectId, tool)` 与 `setProjectAgentAssignment`；工具集合必须来自 `PROJECT_AGENT_TOOLS`。

### 3. Contracts

- `TOOL_CAPABILITIES` 是唯一能力来源；`AGENT_TOOLS` 包含五个工具，`PROJECT_AGENT_TOOLS` 排除 ZCode。关闭工具时不渲染其 Agents 控件或发起查询。
- `AGENT_TOOL_SETTINGS_TOOLS` 只来自生成的 `agentToolSettings` 能力位（首期 Claude/Codex）。编辑弹窗通过 `setAgentToolSettings` 串行提交差异，携带最新 `rowVersion`；保存完成后统一刷新 Agents 与 Dashboard 查询。
- 工具设置表单只允许 Claude `model`/`color`/`tools` 与 Codex `model`/`modelReasoningEffort`/`features`，前端校验字节长度、集合数量和键名格式，但后端仍是最终校验边界。
- 分配成功刷新中央意图并立即消费后端返回的精确 scope；空 scope 是 no-op，硬阻断保留应用内重试。
- 导入对话框只显示全局直属文件候选；`retainedFields` 与 `droppedFields` 必须分别明确展示，确认负载携带 `toolSettings`，不改写原生文件、不自动分配；首次分配若交集字段仍一致，后端自动登记当前基线。
- 全局状态按工具显示聚合状态，能力/策略诊断读取卡片级 `diagnosticCode`，展开可查看每个文件的漂移诊断；外部变化动作必须携带目标文件路径、观测 hash 与 row version，不能用目录路径或过期状态替代。

### 4. Validation & Error Matrix

| 条件 | 必须结果 |
| ---- | -------- |
| 后端能力为 false（ZCode 项目 Agents） | 不出现在工具切换，不调用项目 Agents 查询/命令 |
| 名称不符合交集规则 | 表单阻止提交并显示小写字母、数字、连字符与 1–64 长度提示 |
| 预览状态为 failed/policy/untrusted/conflict | 目标动作禁用；诊断码和中文说明可见，并提供刷新/重试入口 |
| 导入候选不可导入或含 retained/dropped fields | 复选框禁用或分别显示保留/丢弃提示；不可绕过 UI 校验提交 |

### 5. Good / Base / Bad Cases

- Good：中央 Agent 保存后刷新列表与 Dashboard，分配按钮仅更新意图；状态卡先生成持久化计划，用户点击动作后立即消费精确 Preview，文件随 Apply 变化。
- Base：没有中央 Agent 或没有受管文件时显示可操作空状态，目录探测错误不被渲染成“未接管”。
- Bad：把 Agents 加入 `MCP_TOOLS` 复用项目原生资源查询、在 ZCode 项目页签发送请求、或导入后自动 Apply。

### 6. Tests Required

- Agents 页面测试 CRUD、名称校验、全局分配、状态展开、Preview/Apply、导入 `droppedFields` 展示。
- 工具设置测试表单枚举/字节/集合校验、差异提交的 row version、列表徽标以及导入 retained/dropped 字段展示。
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
// Agents 使用独立的中央/项目分配查询与持久化 Preview → Apply 流程。
useQuery(agentProjectOptionsQueryOptions(projectId, tool));
requestPreview(scopes); // 自动消费持久化 Preview → Apply，不展示二次确认
```
