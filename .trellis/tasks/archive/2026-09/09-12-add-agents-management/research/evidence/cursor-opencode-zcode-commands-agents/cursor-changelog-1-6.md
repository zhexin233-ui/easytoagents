[Skip to content](#main)

[Sign in](/dashboard)[ContactContact sales](/contact-sales?source=navbar)[Download](/download)



1.6  · [Changelog](/changelog)

[Changelog](/changelog)

# Slash commands, summarization, and improved Agent terminal

### [#](#custom-slash-commands)Custom slash commands

You can now create reusable prompts and quickly share them with your team. [Commands](https://cursor.com/help/customization/skills#how-do-i-migrate-commands-to-skills) are stored in `.cursor/commands/[command].md`. Run them by typing `/` in the Agent input and selecting the command from the dropdown menu.

We've been using them for running linters, fixing compile errors, and creating PRs with detailed descriptions and conventional commits.

### [#](#summarization-triggers)Summarization triggers

Cursor automatically summarizes long conversation for you when reaching the context window limit. You can now summarize context on-demand with the `/summarize` slash command. This can be useful when you don't want to create a new chat, but want to free up space in the context window.

### [#](#mcp-resources-support)MCP Resources support

We've added support for MCP Resources. [Resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources) allow servers to share data that provides context to language models, such as files, database schemas, or application-specific information.

Additionally, interpolated variables are now supported for MCP. This enables using environment variables in strings when defining configuration for MCP servers.

### [#](#improved-terminal-for-agent)Improved terminal for Agent

When Agent decides to create a terminal to run shell commands, we've dramatically improved the stability and reliability of the environment.

This solves known issues around terminal commands hanging and not properly exiting when completing tasks, as well as improving the SSH experience.

We've also polished the terminal UI, made it faster to run, and added OS notifications when shell commands require user approval.

* 1.6.1: Fixed git issues
* 1.6.2: Improved terminal stability
* 1.6.3: Fixed shell environment issues
* 1.6.4: Fixed CLI parsing issues
* 1.6.5: Performance improvements
* 1.6.6: Fixed terminal rendering issues
* 1.6.7: Enhanced git diff parsing
* 1.6.8: Shell command reliability improvements
* 1.6.9: Fixed MCP server connection issues
* 1.6.10: Performance optimizations
* 1.6.11: Fixed git branch switching issues
* 1.6.12: General bug fixes and stability improvements
* 1.6.13-1.6.23: Terminal fixes
* 1.6.24: Stability improvements
* 1.6.25: MCP admin tooling improvement
* 1.6.26: Summarization and extension improvements
* 1.6.27: Native menu notification badge
* 1.6.28: file loading performance improvements
* 1.6.29: Agent conversation UX adjustments
* 1.6.30: Agent TODO UX changes
* 1.6.31: Agent terminal/shell changes for zsh
* 1.6.32-35: Agent window beta changes, MCP reinstallation bug fix
* 1.6.36-1.6.42: WSL improvements for agent terminal and agent conversation bug fixes
* 1.6.42-1.6.45: Further Agent terminal fixes for Bash/ZSH state restoration.

[Skip to content](#main)

[Cursor](/home)

[Sign in](/dashboard)[ContactContact sales](/contact-sales?source=navbar)[Download](/download)



1.6  · [Changelog](/changelog)

[Changelog](/changelog)

# Slash commands, summarization, and improved Agent terminal

### [#](#custom-slash-commands)Custom slash commands

You can now create reusable prompts and quickly share them with your team. [Commands](https://cursor.com/help/customization/skills#how-do-i-migrate-commands-to-skills) are stored in `.cursor/commands/[command].md`. Run them by typing `/` in the Agent input and selecting the command from the dropdown menu.

We've been using them for running linters, fixing compile errors, and creating PRs with detailed descriptions and conventional commits.

### [#](#summarization-triggers)Summarization triggers

Cursor automatically summarizes long conversation for you when reaching the context window limit. You can now summarize context on-demand with the `/summarize` slash command. This can be useful when you don't want to create a new chat, but want to free up space in the context window.

### [#](#mcp-resources-support)MCP Resources support

We've added support for MCP Resources. [Resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources) allow servers to share data that provides context to language models, such as files, database schemas, or application-specific information.

Additionally, interpolated variables are now supported for MCP. This enables using environment variables in strings when defining configuration for MCP servers.

### [#](#improved-terminal-for-agent)Improved terminal for Agent

When Agent decides to create a terminal to run shell commands, we've dramatically improved the stability and reliability of the environment.

This solves known issues around terminal commands hanging and not properly exiting when completing tasks, as well as improving the SSH experience.

We've also polished the terminal UI, made it faster to run, and added OS notifications when shell commands require user approval.

* 1.6.1: Fixed git issues
* 1.6.2: Improved terminal stability
* 1.6.3: Fixed shell environment issues
* 1.6.4: Fixed CLI parsing issues
* 1.6.5: Performance improvements
* 1.6.6: Fixed terminal rendering issues
* 1.6.7: Enhanced git diff parsing
* 1.6.8: Shell command reliability improvements
* 1.6.9: Fixed MCP server connection issues
* 1.6.10: Performance optimizations
* 1.6.11: Fixed git branch switching issues
* 1.6.12: General bug fixes and stability improvements
* 1.6.13-1.6.23: Terminal fixes
* 1.6.24: Stability improvements
* 1.6.25: MCP admin tooling improvement
* 1.6.26: Summarization and extension improvements
* 1.6.27: Native menu notification badge
* 1.6.28: file loading performance improvements
* 1.6.29: Agent conversation UX adjustments
* 1.6.30: Agent TODO UX changes
* 1.6.31: Agent terminal/shell changes for zsh
* 1.6.32-35: Agent window beta changes, MCP reinstallation bug fix
* 1.6.36-1.6.42: WSL improvements for agent terminal and agent conversation bug fixes
* 1.6.42-1.6.45: Further Agent terminal fixes for Bash/ZSH state restoration.
