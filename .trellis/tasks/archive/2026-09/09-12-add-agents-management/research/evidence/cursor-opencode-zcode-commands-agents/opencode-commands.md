[Skip to content](#_top)

[app.header.home](/)[app.header.docs](/docs/)

# Commands

Create custom commands for repetitive tasks.

Custom commands let you specify a prompt you want to run when that command is executed in the TUI.

Custom commands are in addition to the built-in commands like `/init`, `/undo`, `/redo`, `/share`, `/help`. [Learn more](/docs/tui#commands).

---

## [Create command files](#create-command-files)

Create markdown files in the `commands/` directory to define custom commands.

Create `.opencode/commands/test.md`:

The frontmatter defines command properties. The content becomes the template.

Use the command by typing `/` followed by the command name.

---

## [Configure](#configure)

You can add custom commands through the OpenCode config or by creating markdown files in the `commands/` directory.

---

### [JSON](#json)

Use the `command` option in your OpenCode [config](/docs/config):

Now you can run this command in the TUI:

---

### [Markdown](#markdown)

You can also define commands using markdown files. Place them in:

* Global: `~/.config/opencode/commands/`
* Per-project: `.opencode/commands/`

The markdown file name becomes the command name. For example, `test.md` lets you run:

---

## [Prompt config](#prompt-config)

The prompts for the custom commands support several special placeholders and syntax.

---

### [Arguments](#arguments)

Pass arguments to commands using the `$ARGUMENTS` placeholder.

Run the command with arguments:

And `$ARGUMENTS` will be replaced with `Button`.

You can also access individual arguments using positional parameters:

* `$1` - First argument
* `$2` - Second argument
* `$3` - Third argument
* And so on…

For example:

Run the command:

This replaces:

* `$1` with `config.json`
* `$2` with `src`
* `$3` with `{ "key": "value" }`

---

### [Shell output](#shell-output)

Use *!`command`* to inject [bash command](/docs/tui#bash-commands) output into your prompt.

For example, to create a custom command that analyzes test coverage:

Or to review recent changes:

Commands run in your project’s root directory and their output becomes part of the prompt.

---

### [File references](#file-references)

Include files in your command using `@` followed by the filename.

The file content gets included in the prompt automatically.

---

## [Options](#options)

Let’s look at the configuration options in detail.

---

### [Template](#template)

The `template` option defines the prompt that will be sent to the LLM when the command is executed.

This is a **required** config option.

---

### [Description](#description)

Use the `description` option to provide a brief description of what the command does.

This is shown as the description in the TUI when you type in the command.

---

### [Agent](#agent)

Use the `agent` config to optionally specify which [agent](/docs/agents) should execute this command. If this is a [subagent](/docs/agents/#subagents) the command will trigger a subagent invocation by default. To disable this behavior, set `subtask` to `false`.

This is an **optional** config option. If not specified, defaults to your current agent.

---

### [Subtask](#subtask)

Use the `subtask` boolean to force the command to trigger a [subagent](/docs/agents/#subagents) invocation. This is useful if you want the command to not pollute your primary context and will **force** the agent to act as a subagent, even if `mode` is set to `primary` on the [agent](/docs/agents) configuration.

This is an **optional** config option.

---

### [Model](#model)

Use the `model` config to override the default model for this command.

This is an **optional** config option.

---

## [Built-in](#built-in)

opencode includes several built-in commands like `/init`, `/undo`, `/redo`, `/share`, `/help`; [learn more](/docs/tui#commands).

If you define a custom command with the same name, it will override the built-in command.
