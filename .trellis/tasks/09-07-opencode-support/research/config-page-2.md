[Skip to content](#_top)

[app.header.home](/)[app.header.docs](/docs/)

# Rules

Set custom instructions for opencode.

You can provide custom instructions to opencode by creating an `AGENTS.md` file. This is similar to Cursor’s rules. It contains instructions that will be included in the LLM’s context to customize its behavior for your specific project.

---

## [Initialize](#initialize)

To create a new `AGENTS.md` file, you can run the `/init` command in opencode.

`/init` scans the important files in your repo, may ask a couple of targeted questions when the codebase cannot answer them, and then creates or updates `AGENTS.md` with concise project-specific guidance.

It focuses on the things future agent sessions are most likely to need:

* build, lint, and test commands
* command order and focused verification steps when they matter
* architecture and repo structure that are not obvious from filenames alone
* project-specific conventions, setup quirks, and operational gotchas
* references to existing instruction sources like Cursor or Copilot rules

If you already have an `AGENTS.md`, `/init` will improve it in place instead of blindly replacing it.

---

## [Example](#example)

You can also just create this file manually. Here’s an example of some things you can put into an `AGENTS.md` file.

We are adding project-specific instructions here and this will be shared across your team.

---

## [Types](#types)

opencode also supports reading the `AGENTS.md` file from multiple locations. And this serves different purposes.

### [Project](#project)

Place an `AGENTS.md` in your project root for project-specific rules. These only apply when you are working in this directory or its sub-directories.

### [Global](#global)

You can also have global rules in a `~/.config/opencode/AGENTS.md` file. This gets applied across all opencode sessions.

Since this isn’t committed to Git or shared with your team, we recommend using this to specify any personal rules that the LLM should follow.

### [Claude Code Compatibility](#claude-code-compatibility)

For users migrating from Claude Code, OpenCode supports Claude Code’s file conventions as fallbacks:

* **Project rules**: `CLAUDE.md` in your project directory (used if no `AGENTS.md` exists)
* **Global rules**: `~/.claude/CLAUDE.md` (used if no `~/.config/opencode/AGENTS.md` exists)
* **Skills**: `~/.claude/skills/` — see [Agent Skills](/docs/skills/) for details

To disable Claude Code compatibility, set one of these environment variables:

---

## [Precedence](#precedence)

When opencode starts, it looks for rule files in this order:

1. **Local files** by traversing up from the current directory (`AGENTS.md`, `CLAUDE.md`)
2. **Global file** at `~/.config/opencode/AGENTS.md`
3. **Claude Code file** at `~/.claude/CLAUDE.md` (unless disabled)

The first matching file wins in each category. For example, if you have both `AGENTS.md` and `CLAUDE.md`, only `AGENTS.md` is used. Similarly, `~/.config/opencode/AGENTS.md` takes precedence over `~/.claude/CLAUDE.md`.

---

## [Custom Instructions](#custom-instructions)

You can specify custom instruction files in your `opencode.json` or the global `~/.config/opencode/opencode.json`. This allows you and your team to reuse existing rules rather than having to duplicate them to AGENTS.md.

Example:

You can also use remote URLs to load instructions from the web.

Remote instructions are fetched with a 5 second timeout.

All instruction files are combined with your `AGENTS.md` files.

---

## [Referencing External Files](#referencing-external-files)

While opencode doesn’t automatically parse file references in `AGENTS.md`, you can achieve similar functionality in two ways:

### [Using opencode.json](#using-opencodejson)

The recommended approach is to use the `instructions` field in `opencode.json`:

### [Manual Instructions in AGENTS.md](#manual-instructions-in-agentsmd)

You can teach opencode to read external files by providing explicit instructions in your `AGENTS.md`. Here’s a practical example:

This approach allows you to:

* Create modular, reusable rule files
* Share rules across projects via symlinks or git submodules
* Keep AGENTS.md concise while referencing detailed guidelines
* Ensure opencode loads files only when needed for the specific task
