[Skip to content](#_top)

[app.header.home](/)[app.header.docs](/docs/)

# MCP servers

Add local and remote MCP tools.

You can add external tools to OpenCode using the *Model Context Protocol*, or MCP. OpenCode supports both local and remote servers.

Once added, MCP tools are automatically available to the LLM alongside built-in tools.

---

#### [Caveats](#caveats)

When you use an MCP server, it adds to the context. This can quickly add up if you have a lot of tools. So we recommend being careful with which MCP servers you use.

Certain MCP servers, like the GitHub MCP server, tend to add a lot of tokens and can easily exceed the context limit.

---

## [Enable](#enable)

You can define MCP servers in your [OpenCode Config](https://opencode.ai/docs/config/) under `mcp`. Add each MCP with a unique name. You can refer to that MCP by name when prompting the LLM.

You can also disable a server by setting `enabled` to `false`. This is useful if you want to temporarily disable a server without removing it from your config.

---

### [Overriding remote defaults](#overriding-remote-defaults)

Organizations can provide default MCP servers via their `.well-known/opencode` endpoint. These servers may be disabled by default, allowing users to opt-in to the ones they need.

To enable a specific server from your organization’s remote config, add it to your local config with `enabled: true`:

Your local config values override the remote defaults. See [config precedence](/docs/config#precedence-order) for more details.

---

## [Local](#local)

Add local MCP servers using `type` to `"local"` within the MCP object.

The command is how the local MCP server is started. You can also pass in a list of environment variables as well.

For example, here’s how you can add the test [`@modelcontextprotocol/server-everything`](https://www.npmjs.com/package/@modelcontextprotocol/server-everything) MCP server.

And to use it I can add `use the mcp_everything tool` to my prompts.

---

#### [Options](#options)

Here are all the options for configuring a local MCP server.

| Option | Type | Required | Description |
| --- | --- | --- | --- |
| `type` | String | Y | Type of MCP server connection, must be `"local"`. |
| `command` | Array | Y | Command and arguments to run the MCP server. |
| `cwd` | String | Working directory for the MCP server process. Relative paths resolve from the workspace. |
| `environment` | Object | Environment variables to set when running the server. |
| `enabled` | Boolean | Enable or disable the MCP server on startup. |
| `timeout` | Number | Timeout in ms for fetching tools from the MCP server. Defaults to 5000 (5 seconds). |

---

## [Remote](#remote)

Add remote MCP servers by setting `type` to `"remote"`.

The `url` is the URL of the remote MCP server and with the `headers` option you can pass in a list of headers.

---

#### [Options](#options-1)

| Option | Type | Required | Description |
| --- | --- | --- | --- |
| `type` | String | Y | Type of MCP server connection, must be `"remote"`. |
| `url` | String | Y | URL of the remote MCP server. |
| `enabled` | Boolean | Enable or disable the MCP server on startup. |
| `headers` | Object | Headers to send with the request. |
| `oauth` | Object | OAuth authentication configuration. See [OAuth](#oauth) section below. |
| `timeout` | Number | Timeout in ms for fetching tools from the MCP server. Defaults to 5000 (5 seconds). |

---

## [OAuth](#oauth)

OpenCode automatically handles OAuth authentication for remote MCP servers. When a server requires authentication, OpenCode will:

1. Detect the 401 response and initiate the OAuth flow
2. Use **Dynamic Client Registration (RFC 7591)** if supported by the server
3. Store tokens securely for future requests

---

### [Automatic](#automatic)

For most OAuth-enabled MCP servers, no special configuration is needed. Just configure the remote server:

If the server requires authentication, OpenCode will prompt you to authenticate when you first try to use it. If not, you can [manually trigger the flow](#authenticating) with `opencode mcp auth` .

---

### [Pre-registered](#pre-registered)

If you have client credentials from the MCP server provider, you can configure them:

---

### [Authenticating](#authenticating)

You can manually trigger authentication or manage credentials.

Authenticate with a specific MCP server:

List all MCP servers and their auth status:

Remove stored credentials:

The `mcp auth` command will open your browser for authorization. After you authorize, OpenCode will store the tokens securely in `~/.local/share/opencode/mcp-auth.json`.

---

#### [Disabling OAuth](#disabling-oauth)

If you want to disable automatic OAuth for a server (e.g., for servers that use API keys instead), set `oauth` to `false`:

---

#### [OAuth Options](#oauth-options)

| Option | Type | Description |
| --- | --- | --- |
| `oauth` | Object | false | OAuth config object, or `false` to disable OAuth auto-detection. |
| `clientId` | String | OAuth client ID. If not provided, dynamic client registration will be attempted. |
| `clientSecret` | String | OAuth client secret, if required by the authorization server. |
| `scope` | String | OAuth scopes to request during authorization. |

#### [Debugging](#debugging)

If a remote MCP server is failing to authenticate, you can diagnose issues with:

The `mcp debug` command shows the current auth status, tests HTTP connectivity, and attempts the OAuth discovery flow.

---

## [Manage](#manage)

Your MCPs are available as tools in OpenCode, alongside built-in tools. So you can manage them through the OpenCode config like any other tool.

---

### [Global](#global)

This means that you can enable or disable them globally.

We can also use a glob pattern to disable all matching MCPs.

Here we are using the glob pattern `my-mcp*` to disable all MCPs.

---

### [Per agent](#per-agent)

If you have a large number of MCP servers you may want to only enable them per agent and disable them globally. To do this:

1. Disable it as a tool globally.
2. In your [agent config](/docs/agents#tools), enable the MCP server as a tool.

---

#### [Glob patterns](#glob-patterns)

The glob pattern uses simple regex globbing patterns:

* `*` matches zero or more of any character (e.g., `"my-mcp*"` matches `my-mcp_search`, `my-mcp_list`, etc.)
* `?` matches exactly one character
* All other characters match literally

---

## [Examples](#examples)

Below are examples of some common MCP servers. You can submit a PR if you want to document other servers.

---

### [Sentry](#sentry)

Add the [Sentry MCP server](https://mcp.sentry.dev) to interact with your Sentry projects and issues.

After adding the configuration, authenticate with Sentry:

This will open a browser window to complete the OAuth flow and connect OpenCode to your Sentry account.

Once authenticated, you can use Sentry tools in your prompts to query issues, projects, and error data.

---

### [Context7](#context7)

Add the [Context7 MCP server](https://github.com/upstash/context7) to search through docs.

If you have signed up for a free account, you can use your API key and get higher rate-limits.

Here we are assuming that you have the `CONTEXT7_API_KEY` environment variable set.

Add `use context7` to your prompts to use Context7 MCP server.

Alternatively, you can add something like this to your [AGENTS.md](/docs/rules/).

---

### [Grep by Vercel](#grep-by-vercel)

Add the [Grep by Vercel](https://grep.app) MCP server to search through code snippets on GitHub.

Since we named our MCP server `gh_grep`, you can add `use the gh_grep tool` to your prompts to get the agent to use it.

Alternatively, you can add something like this to your [AGENTS.md](/docs/rules/).
