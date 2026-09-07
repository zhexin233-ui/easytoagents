[Skip to content](#_top)

[app.header.home](/)[app.header.docs](/docs/)

# Config

Using the OpenCode JSON config.

You can configure OpenCode using a JSON config file.

---

## [Format](#format)

OpenCode supports both **JSON** and **JSONC** (JSON with Comments) formats.

---

## [Locations](#locations)

You can place your config in a couple of different locations and they have a different order of precedence.

Configuration files are merged together, not replaced. Settings from the following config locations are combined. Later configs override earlier ones only for conflicting keys. Non-conflicting settings from all configs are preserved.

For example, if your global config sets `autoupdate: true` and your project config sets `model: "anthropic/claude-sonnet-4-5"`, the final configuration will include both settings.

---

### [Precedence order](#precedence-order)

Config sources are loaded in this order (later sources override earlier ones):

1. **Remote config** (from `.well-known/opencode`) - organizational defaults
2. **Global config** (`~/.config/opencode/opencode.json`) - user preferences
3. **Custom config** (`OPENCODE_CONFIG` env var) - custom overrides
4. **Project config** (`opencode.json` in project) - project-specific settings
5. **`.opencode` directories** - agents, commands, plugins
6. **Inline config** (`OPENCODE_CONFIG_CONTENT` env var) - runtime overrides
7. **Managed config files** (`/Library/Application Support/opencode/` on macOS) - admin-controlled
8. **macOS managed preferences** (`.mobileconfig` via MDM) - highest priority, not user-overridable

This means project configs can override global defaults, and global configs can override remote organizational defaults. Managed settings override everything.

---

### [Remote](#remote)

Organizations can provide default configuration via the `.well-known/opencode` endpoint. This is fetched automatically when you authenticate with a provider that supports it.

Remote config is loaded first, serving as the base layer. All other config sources (global, project) can override these defaults.

For example, if your organization provides MCP servers that are disabled by default:

You can enable specific servers in your local config:

---

### [Global](#global)

Place your global OpenCode config in `~/.config/opencode/opencode.json`. Use global config for user-wide server/runtime preferences like providers, models, and permissions.

For TUI-specific settings, use `~/.config/opencode/tui.json`.

Global config overrides remote organizational defaults.

---

### [Per project](#per-project)

Add `opencode.json` in your project root. Project config has the highest precedence among standard config files - it overrides both global and remote configs.

For project-specific TUI settings, add `tui.json` alongside it.

When OpenCode starts up, it first looks for a config file in the current directory, then traverses up to the nearest Git directory.

This is also safe to be checked into Git and uses the same schema as the global one.

---

### [Custom path](#custom-path)

Specify a custom config file path using the `OPENCODE_CONFIG` environment variable.

Custom config is loaded between global and project configs in the precedence order.

---

### [Custom directory](#custom-directory)

Specify a custom config directory using the `OPENCODE_CONFIG_DIR` environment variable. This directory will be searched for agents, commands, modes, and plugins just like the standard `.opencode` directory, and should follow the same structure.

The custom directory is loaded after the global config and `.opencode` directories, so it **can override** their settings.

---

### [Managed settings](#managed-settings)

Organizations can enforce configuration that users cannot override. Managed settings are loaded at the highest priority tier.

#### [File-based](#file-based)

Drop an `opencode.json` or `opencode.jsonc` file in the system managed config directory:

| Platform | Path |
| --- | --- |
| macOS | `/Library/Application Support/opencode/` |
| Linux | `/etc/opencode/` |
| Windows | `%ProgramData%\opencode` |

These directories require admin/root access to write, so users cannot modify them.

#### [macOS managed preferences](#macos-managed-preferences)

On macOS, OpenCode reads managed preferences from the `ai.opencode.managed` preference domain. Deploy a `.mobileconfig` via MDM (Jamf, Kandji, FleetDM) and the settings are enforced automatically.

OpenCode checks these paths:

1. `/Library/Managed Preferences//ai.opencode.managed.plist`
2. `/Library/Managed Preferences/ai.opencode.managed.plist`

The plist keys map directly to `opencode.json` fields. MDM metadata keys (`PayloadUUID`, `PayloadType`, etc.) are stripped automatically.

**Creating a `.mobileconfig`**

Use the `ai.opencode.managed` PayloadType. The OpenCode config keys go directly in the payload dict:

Generate unique UUIDs with `uuidgen`. Customize the settings to match your organization’s requirements.

**Deploying via MDM**

* **Jamf Pro:** Computers > Configuration Profiles > Upload > scope to target devices or smart groups
* **FleetDM:** Add the `.mobileconfig` to your gitops repo under `mdm.macos_settings.custom_settings` and run `fleetctl apply`

**Verifying on a device**

Double-click the `.mobileconfig` to install locally for testing (shows in System Settings > Privacy & Security > Profiles), then run:

All managed preference keys appear in the resolved config and cannot be overridden by user or project configuration.

---

## [Schema](#schema)

The server/runtime config schema is defined in [**`opencode.ai/config.json`**](https://opencode.ai/config.json).

TUI config uses [**`opencode.ai/tui.json`**](https://opencode.ai/tui.json).

Your editor should be able to validate and autocomplete based on the schema.

---

### [TUI](#tui)

Use a dedicated `tui.json` (or `tui.jsonc`) file for TUI-specific settings.

Use `OPENCODE_TUI_CONFIG` to point to a custom TUI config file.

When `cursor.style` is `"default"`, the terminal default cursor is restored, so `cursor.blinking` has no effect.

Set `attention.enabled` to turn on TUI desktop notifications and sounds. See [TUI attention](/docs/tui#attention).

Legacy `theme`, `keybinds`, and `tui` keys in `opencode.json` are deprecated and automatically migrated when possible.

---

### [Server](#server)

You can configure server settings for the `opencode serve` and `opencode web` commands through the `server` option.

Available options:

* `port` - Port to listen on.
* `hostname` - Hostname to listen on. When `mdns` is enabled and no hostname is set, defaults to `0.0.0.0`.
* `mdns` - Enable mDNS service discovery. This allows other devices on the network to discover your OpenCode server.
* `mdnsDomain` - Custom domain name for mDNS service. Defaults to `opencode.local`. Useful for running multiple instances on the same network.
* `cors` - Additional origins to allow for CORS when using the HTTP server from a browser-based client. Values must be full origins (scheme + host + optional port), eg `https://app.example.com`.

[Learn more about the server here](/docs/server).

---

### [Shell](#shell)

You can configure the shell used for the interactive terminal using the `shell` option. Compatible shells are also used for agent tool calls.

If not specified, OpenCode will automatically discover and use a sensible default based on your operating system (e.g. `pwsh` or `cmd.exe` on Windows, `/bin/zsh` or `/bin/bash` on macOS/Linux). You can provide an absolute path or a short name.

---

### [Tools](#tools)

You can manage the tools an LLM can use through the `tools` option.

[Learn more about tools here](/docs/tools).

---

### [Models](#models)

You can configure the providers and models you want to use in your OpenCode config through the `provider`, `model` and `small_model` options.

The `small_model` option configures a separate model for lightweight tasks like title generation. By default, OpenCode tries to use a cheaper model if one is available from your provider, otherwise it falls back to your main model.

Provider options can include `timeout`, `headerTimeout`, `chunkTimeout`, and `setCacheKey`:

* `timeout` - Request timeout in milliseconds (default: 300000). Set to `false` to disable.
* `headerTimeout` - Timeout in milliseconds to wait for response headers (default: 300000, or 5 minutes). This timer stops once headers arrive and does not limit the streamed response body. Set to `false` to disable.
* `chunkTimeout` - Timeout in milliseconds between streamed response chunks (default: 300000, or 5 minutes). If no chunk arrives in time, the request is aborted. Set to `false` to disable.
* `setCacheKey` - Ensure a cache key is always set for designated provider.

You can also configure [local models](/docs/models#local). [Learn more](/docs/models).

---

### [Policies](#policies)

Use the `experimental.policies` option to allow or deny OpenCode actions on configured resources. Currently, policies can control which providers OpenCode may use.

[Learn more about policies here](/docs/policies).

---

### [Image attachments](#image-attachments)

OpenCode normalizes image attachments before sending them to the model. By default, images are resized when they exceed `2000x2000` pixels or `5242880` base64 bytes.

Configure image attachment limits with the `attachment.image` option:

* `auto_resize` - Resize images that exceed the configured limits before provider requests. Set to `false` to reject oversized images instead.
* `max_width` - Maximum image width in pixels before resizing or rejection.
* `max_height` - Maximum image height in pixels before resizing or rejection.
* `max_base64_bytes` - Maximum encoded image payload size. This is the base64 payload size, not the original file size.

If an image still cannot fit after resizing, OpenCode omits oversized tool-result images or fails oversized user-provided images with an image size error.

---

#### [Provider-Specific Options](#provider-specific-options)

Some providers support additional configuration options beyond the generic `timeout` and `apiKey` settings.

##### [Amazon Bedrock](#amazon-bedrock)

Amazon Bedrock supports AWS-specific configuration:

* `region` - AWS region for Bedrock (defaults to `AWS_REGION` env var or `us-east-1`)
* `profile` - AWS named profile from `~/.aws/credentials` (defaults to `AWS_PROFILE` env var)
* `endpoint` - Custom endpoint URL for VPC endpoints. This is an alias for the generic `baseURL` option using AWS-specific terminology. If both are specified, `endpoint` takes precedence.

[Learn more about Amazon Bedrock configuration](/docs/providers#amazon-bedrock).

---

### [Themes](#themes)

Set your UI theme in `tui.json`.

[Learn more here](/docs/themes).

---

### [Agents](#agents)

You can configure specialized agents for specific tasks through the `agent` option.

You can also define agents using markdown files in `~/.config/opencode/agents/` or `.opencode/agents/`. [Learn more here](/docs/agents).

---

### [Default agent](#default-agent)

You can set the default agent using the `default_agent` option. This determines which agent is used when none is explicitly specified.

The default agent must be a primary agent (not a subagent). This can be a built-in agent like `"build"` or `"plan"`, or a [custom agent](/docs/agents) you’ve defined. If the specified agent doesn’t exist or is a subagent, OpenCode will fall back to `"build"` with a warning.

This setting applies across all interfaces: TUI, CLI (`opencode run`), desktop app, and GitHub Action.

---

### [Subagent depth](#subagent-depth)

You can control how deeply subagents can invoke other subagents using the `subagent_depth` option.

The default is `1`, which allows primary agents to launch subagents but prevents those subagents from launching additional subagents. Set it to `2` to allow one additional level of nested subagents, or `0` to prevent all subagent launches.

---

### [Sharing](#sharing)

You can configure the [share](/docs/share) feature through the `share` option.

This takes:

* `"manual"` - Allow manual sharing via commands (default)
* `"auto"` - Automatically share new conversations
* `"disabled"` - Disable sharing entirely

By default, sharing is set to manual mode where you need to explicitly share conversations using the `/share` command.

---

### [Commands](#commands)

You can configure custom commands for repetitive tasks through the `command` option.

You can also define commands using markdown files in `~/.config/opencode/commands/` or `.opencode/commands/`. [Learn more here](/docs/commands).

---

### [Keybinds](#keybinds)

Customize TUI keyboard shortcuts in `tui.json` with `keybinds`.

`keybinds` is merged with built-in defaults, so you only need to configure the shortcuts you want to change.

[Learn more here](/docs/keybinds).

---

### [Snapshot](#snapshot)

OpenCode uses snapshots to track file changes during agent operations, enabling you to undo and revert changes within a session. Snapshots are enabled by default.

For large repositories or projects with many submodules, the snapshot system can cause slow indexing and significant disk usage as it tracks all changes using an internal git repository. You can disable snapshots using the `snapshot` option.

Note that disabling snapshots means changes made by the agent cannot be rolled back through the UI.

---

### [Autoupdate](#autoupdate)

OpenCode will automatically download any new updates when it starts up. You can disable this with the `autoupdate` option.

If you don’t want updates but want to be notified when a new version is available, set `autoupdate` to `"notify"`. Notice that this only works if it was not installed using a package manager such as Homebrew.

---

### [Formatters](#formatters)

You can enable and configure code formatters through the `formatter` option. Omit it to keep formatters disabled.

Use an object to keep built-ins enabled while configuring overrides or custom formatters.

[Learn more about formatters here](/docs/formatters).

---

### [LSP Servers](#lsp-servers)

You can enable and configure LSP servers through the `lsp` option. Omit it to keep LSP disabled.

Use an object to keep built-ins enabled while configuring overrides or custom LSP servers.

[Learn more about LSP servers here](/docs/lsp).

---

### [Permissions](#permissions)

By default, opencode **allows all operations** without requiring explicit approval. You can change this using the `permission` option.

For example, to ensure that the `edit` and `bash` tools require user approval:

[Learn more about permissions here](/docs/permissions).

---

### [Compaction](#compaction)

You can control context compaction behavior through the `compaction` option.

* `auto` - Automatically compact the session when context is full (default: `true`).
* `prune` - Remove old tool outputs to save tokens (default: `false`). Set to `true` to enable pruning.
* `reserved` - Token buffer for compaction. Leaves enough window to avoid overflow during compaction.

---

### [Watcher](#watcher)

You can configure file watcher ignore patterns through the `watcher` option.

Patterns follow glob syntax. Use this to exclude noisy directories from file watching.

---

### [MCP servers](#mcp-servers)

You can configure MCP servers you want to use through the `mcp` option.

[Learn more here](/docs/mcp-servers).

---

### [Plugins](#plugins)

[Plugins](/docs/plugins) extend OpenCode with custom tools, hooks, and integrations.

Place plugin files in `.opencode/plugins/` or `~/.config/opencode/plugins/`. You can also load plugins from npm through the `plugin` option.

[Learn more here](/docs/plugins).

---

### [Instructions](#instructions)

You can configure the instructions for the model you’re using through the `instructions` option.

This takes an array of paths and glob patterns to instruction files. [Learn more about rules here](/docs/rules).

---

### [Disabled providers](#disabled-providers)

You can disable providers that are loaded automatically through the `disabled_providers` option. This is useful when you want to prevent certain providers from being loaded even if their credentials are available.

The `disabled_providers` option accepts an array of provider IDs. When a provider is disabled:

* It won’t be loaded even if environment variables are set.
* It won’t be loaded even if API keys are configured through the `/connect` command.
* The provider’s models won’t appear in the model selection list.

---

### [Enabled providers](#enabled-providers)

You can specify an allowlist of providers through the `enabled_providers` option. When set, only the specified providers will be enabled and all others will be ignored.

This is useful when you want to restrict OpenCode to only use specific providers rather than disabling them one by one.

If a provider appears in both `enabled_providers` and `disabled_providers`, the `disabled_providers` takes priority for backwards compatibility.

---

### [Experimental](#experimental)

The `experimental` key contains options that are under active development.

---

## [Variables](#variables)

You can use variable substitution in your config files to reference environment variables and file contents.

---

### [Env vars](#env-vars)

Use `{env:VARIABLE_NAME}` to substitute environment variables:

If the environment variable is not set, it will be replaced with an empty string.

---

### [Files](#files)

Use `{file:path/to/file}` to substitute the contents of a file:

File paths can be:

* Relative to the config file directory
* Or absolute paths starting with `/` or `~`

These are useful for:

* Keeping sensitive data like API keys in separate files.
* Including large instruction files without cluttering your config.
* Sharing common configuration snippets across multiple config files.
