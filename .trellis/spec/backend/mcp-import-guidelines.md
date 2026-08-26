# Native Global MCP Import

## 1. Scope / Trigger

Read this contract when changing native MCP discovery, import confirmation,
ownership adoption, or the MCP import dialog. It supplements the central intent
scenario in [quality-guidelines.md](./quality-guidelines.md).

Global sync previews use enabled central assignments, not every native entry.
An empty preview does not prove the tool has no MCP configuration. Native entries
enter the central library only after an explicit selection and confirmation.

## 2. Signatures

- `discover_mcp_import(tool: Tool) -> Result<McpImportPreviewDto, AppError>`.
- `confirm_mcp_import(input: ConfirmMcpImportInput) -> Result<McpImportResultDto, AppError>`.
- Generated TypeScript methods are `commands.discoverMcpImport(tool)` and
  `commands.confirmMcpImport({ previewId, candidateIds })`.
- Migration `0005_mcp_import_previews.sql` adds `mcp_import_previews`: UUID,
  tool, canonical target path, observed full hash, object-shaped `context_json`
  and `redacted_preview_json`, `previewed`/`consumed` status, and timestamps.
  It does not extend the Provider/Prompt-only import table or alter old migrations.

## 3. Contracts

### Discovery and conversion

- Consume `AppState`'s explicit environment and version-bound capability/policy
  probes. Reuse the adapter descriptor and `scan_target`; do not read process
  environment or guess non-default Claude paths.
- Discovery may persist private, redacted import evidence. It must not create
  central records, assignments, managed targets/items, or write native files.
- The preview contains `previewId: string | null`, `tool`, `targetPath`,
  `candidates`, and `message`. Each candidate has an opaque ID, safe name,
  optional transport, status, optional create/reuse action, reason, and redacted
  projection. No eligible candidates means no confirmable preview ID.
- Accept enabled stdio and HTTP configurations representable by
  `ValidatedMcpConfiguration`. Claude `type=http` maps to `streamable_http`;
  Codex `http_headers` maps to central headers. Preserve supported extra fields;
  missing collections become empty, but null or wrong types are invalid.
- Disabled entries, SSE, `env_http_headers`, mixed transports, and unsafe ordinary
  fields remain unselectable and unmanaged. Do not relax central validation.
  Central `enabled=false` removes an item from the desired set: adopting a native
  disabled entry would cause later deletion, even if a renderer emitted false.
- Reuse requires an exact native name and full normalized private configuration
  equality, including secrets, extra fields, and enabled state. NOCASE-only name
  matches, different configurations, or a source-tool project assignment conflict
  cannot be overwritten or silently renamed.

### Confirmation and ownership

- The client submits only a preview ID and nonempty, unique candidate IDs. Tool,
  path, native key/hash, and reuse ID come from persisted evidence, never client
  reconstruction or the redacted projection.
- Bind evidence to descriptor identity, source full hash, central IDs/versions,
  source-tool assignment sets, and managed target/item IDs/versions. The current
  fingerprint conservatively includes every central MCP row; unrelated central
  edits can therefore require rescanning.
- Re-read and validate the source at confirmation. In one `IMMEDIATE` transaction,
  recheck evidence and state, reject applying/restoring/rollback_failed runs, then
  validate the source again after acquiring the write lock and before consuming
  the token. SQLite does not lock external files; do not claim cross-resource
  atomicity. Any observed mismatch rolls back the whole database batch.
- Atomically create/reuse selected central records, add only source-tool global
  assignments, record actual observed native item hashes, and consume the token.
  Duplicate confirmation fails without duplicating rows.
- Before incremental adoption, verify every old managed item still exists with its
  stored hash and that the old union projection matches the baseline. Only then
  extend ownership with selected entries. Never refresh a drifted old baseline.
- Baselines use the actual observed native representation, not renderer output.
  Later normalization is shown in a separate sync preview. Unselected entries
  remain external and survive later Apply.
- Confirmation returns `tool`, `createdCount`, `reusedCount`, and `assignedCount`.
  It never writes native files or calls Apply.

### Secrets and UI lifecycle

- Register source header/env values and detectable extra secrets before constructing
  any display projection, including values in rejected entries. Check ordinary
  names/command/args/URL against this redactor. Rejected unsafe content must not
  appear in diagnostics. Evidence stores only safe identity/versions/hashes.
- Raw secrets may exist only in allowed private central records and baselines;
  RPC, import evidence/display JSON, errors, sync records, and journals are redacted.
- The UI uses a separate `['mcp-import', tool, requestId]` query for each explicit
  open/rescan: `retry: false`, `staleTime: Infinity`, `gcTime: 0`, and no focus or
  reconnect refetch. Closing/reopening discards selection and isolates old responses.
- Start with no selection; only `importable` rows are selectable. Confirmation
  pending blocks close/repeat submission; failure requires a fresh scan. Reuse the
  dialog focus helper for Tab/Escape and trigger focus restoration.
- Successful confirmation closes the dialog and invalidates the central MCP query
  family, not the import query. Explain that native files are unchanged and offer
  a separate global preview. Never call create, assignment, or Apply from the dialog.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Missing file or empty MCP container | Distinct empty message; no preview token |
| Malformed JSON/TOML, unreadable file, unsafe path | Stable parse/permission/conflict error; never pretend no MCP exists |
| Unknown capability or policy | Fail closed; no guessed target or adoption |
| Disabled, unsupported, or malformed individual entry | Unselectable status/reason; other valid entries remain eligible |
| Already managed source key | Read-only `already_managed`; do not refresh baseline |
| Different private values, case-only collision, project assignment | `name_conflict`; no overwrite |
| Empty, duplicate, or unknown candidate IDs | `INVALID_INPUT`; no partial rows |
| Changed source identity/hash or central/managed versions | `STALE_PREVIEW` (or current safe-read error); rescan |
| Old managed item or baseline drift | `CONFLICT`; preserve old baseline |
| Consumed token | `PREVIEW_ALREADY_CONSUMED`; no second adoption |
| Active apply/restore or rollback failure | `WRITE_IN_PROGRESS`; token stays unconsumed |
| Any insert/update or source revalidation fails | Roll back records, assignments, ownership, and token consumption together |

## 5. Good / Base / Bad Cases

- Good: select one native stdio entry, confirm without modifying file bytes, then
  scan again to select another entry; the next sync preview has proven ownership
  and Apply preserves all unselected entries.
- Base: matching configurations from two tools reuse one central row with two
  explicit source assignments; a disabled neighbor stays external.
- Bad: create central records alone and treat a matching native name as ownership,
  compare redacted configurations for equality, or use new import to refresh drift.

## 6. Tests Required

- Extend `mcp::service::tests` with isolated JSON/TOML homes: selective and batch
  adoption, private-value equality, project conflicts, source/version changes,
  repeated or forged selection, drift rejection, writer exclusion, and rollback
  after both source revalidation points. Assert discover/confirm/preview keep native
  bytes identical and subsequent real fixture Apply preserves external entries.
- Audit nonempty serialized RPC, import context/display storage, errors, sync items,
  and journal carriers for fixture secrets; also prove allowed private values survive.
- Extend `db::tests` to upgrade a v4 fixture, preserve existing rows, and reopen v5.
- Extend `mcp-page.test.tsx`: both tools, exact selection payload, no default checks,
  unselectable status, loading/error/empty feedback, stale/rescan, pending close,
  late response isolation, focus, invalidation, and no create/Apply calls.
- Regenerate/check bindings; run `pnpm check`. A jsdom test is not a real desktop
  verification. Never point import confirmation or Apply tests at the user's home.

## 7. Wrong vs Correct

Wrong:

```rust
create_mcp_server(database, redactor, &native_input)?;
// 同名不构成所有权证据，这也没有证明旧基线仍然有效。
```

Correct:

```typescript
const preview = await commands.discoverMcpImport(tool);
// 仅提交用户勾选的候选；原生写入仍需后续独立预览和确认。
await commands.confirmMcpImport({ previewId, candidateIds });
```

The backend must use the evidence-bound atomic adoption path above, not a sequence
of independently committed create/assignment operations.
