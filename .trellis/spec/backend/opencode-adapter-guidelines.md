# OpenCode Adapter Guidelines

This contract covers the OpenCode integration under `src-tauri/`. OpenCode is a
configuration target, not a runtime dependency: discovery must remain explicit,
read-only, and fail closed when the stable configuration contract is not known.

## Scenario: OpenCode configuration and capability discovery

### 1. Scope / Trigger

- Trigger: changes to the OpenCode adapter, tool probing, JSON/JSONC target
  descriptors, Provider/Prompt/MCP/Skill synchronization, or the v19 migration.
- The supported surface is Provider, global Prompt, MCP (local and remote), and
  Skills. Plugin callbacks/hooks are intentionally unsupported.

### 2. Signatures

- `OpenCodeAdapter::discover(&DiscoveryContext) -> Result<Vec<TargetDescriptor>, AppError>`
  returns only explicit global/project descriptors.
- `ExplicitEnvironment::opencode_config_dir()`,
  `opencode_config_file()`, `opencode_config_content()`, and
  `opencode_disabled()` expose the already-validated process overrides.
- `parse_jsonc(&str) -> Result<serde_json::Value, serde_json::Error>` accepts
  JSON plus comments/trailing commas and rejects duplicate keys.
- Migration `0019_opencode_tool_support.sql` widens only the supported
  OpenCode artifact combinations; OpenCode Hook rows remain rejected.

### 3. Contracts

- Discovery uses only explicit `home`, project roots, and validated overrides.
  It must not read `std::env` inside adapters.
- Config precedence is: `OPENCODE_CONFIG_CONTENT` (no file target), explicit
  `OPENCODE_CONFIG`, `OPENCODE_CONFIG_DIR`, then the standard
  `XDG_CONFIG_HOME/opencode` or `~/.config/opencode` location. Existing
  `opencode.jsonc` wins over `opencode.json`; project `.opencode/` files win
  over root files.
- `OPENCODE_DISABLE` and `OPENCODE_DISABLE_PROJECT_CONFIG` are discovery
  policy inputs. A disabled or inline-only surface must produce a diagnostic,
  not a guessed writable path.
- Provider ownership is limited to the documented `model` reference,
  `provider.<id>.npm`, `name`, `options.baseURL`, and supported model fields.
  `auth.json` is never imported or written.
- MCP ownership is `mcp.<name>` with `type=local|remote`; local uses a command
  array and environment map, remote uses URL/headers. `enabled`, timeout,
  cwd, and OAuth fields are validated and preserved only when representable.
- Global Prompt targets `AGENTS.md`; Skills use the global config `skills`
  directory and project `.opencode/skills`. Project Prompt/Rules remain
  outside this adapter's ownership.
- JSONC selectors preserve unmanaged roots, comments, Unicode strings, and
  trailing-comma-compatible syntax. A render must not silently replace an
  unmanaged root.
- Hook RPCs, assignments, imports, descriptors, and writes return the stable
  `OPENCODE_HOOKS_UNSUPPORTED` diagnostic.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Missing installation or invalid `opencode --version` output | `ToolNotInstalled` or an unsupported diagnostic; no native read/write |
| Inline config content or discovery disabled | Unsupported diagnostic; file targets are not exposed |
| Non-UTF-8, malformed, scalar-root, or duplicate-key JSONC | Parse error; preserve the original file and block Apply |
| Unknown MCP transport, mixed local/remote fields, invalid timeout/cwd/OAuth | Individual entry is unsupported/invalid; valid entries remain importable |
| Provider without a losslessly representable model/provider contract | Do not import or project the entry |
| Any OpenCode Hook request | `OPENCODE_HOOKS_UNSUPPORTED`; zero external writes |
| Stale hash, capability evidence, row version, or unsafe path | Stable stale/conflict/policy error; no external mutation |

### 5. Good / Base / Bad Cases

- Good: a JSONC file with comments and Unicode has one managed `provider` root
  changed while unrelated roots and comments remain byte-visible.
- Base: a local MCP entry with a string command array and a remote entry with a
  URL import into central intent, with environment/header/OAuth values redacted.
- Bad: reading `auth.json`, following an unvalidated custom path, treating an
  OpenCode plugin hooks object as command hooks, or rendering the whole JSONC
  file from a semantic value and losing user comments.

### 6. Tests Required

- Adapter matrix tests for default, custom directory, explicit file, inline
  content, disable flags, JSON/JSONC precedence, and custom file-parent
  boundaries.
- JSONC tests for comments, trailing commas, Unicode/comment-like strings,
  duplicate keys at every depth, unmanaged-root preservation, and parse failure.
- Provider tests for exact ownership, model projection, no `auth.json` access,
  secret redaction, and unsupported diagnostics.
- MCP tests for local/remote round trips, environment/header/OAuth redaction,
  timeout/cwd validation, mixed transport rejection, and sibling preservation.
- Hook tests for direct RPC, assignment, import, and write fail-closed behavior.
- Migration tests for v18→v19, reopen/idempotence, supported artifact canaries,
  and rejected OpenCode Hook rows. Run `pnpm bindings:check` and `pnpm check`.

### 7. Wrong vs Correct

#### Wrong

```rust
let config = std::env::var("OPENCODE_CONFIG")?;
let value = serde_json::from_str::<Value>(&fs::read_to_string(config)?)?;
write_whole_file(serde_json::to_vec_pretty(&value)?);
```

#### Correct

```rust
let descriptors = OpenCodeAdapter::default().discover(&context)?;
let observed = scan_target(&adapter, &descriptor, ownership)?;
let rendered = adapter.render(&descriptor, Some(&observed.document), &projection, ownership)?;
// Apply consumes a persisted, redacted preview and writes only owned selectors.
```

