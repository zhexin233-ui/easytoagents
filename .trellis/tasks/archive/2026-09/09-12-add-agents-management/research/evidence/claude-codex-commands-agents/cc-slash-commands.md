## Documentation Index

Fetch the complete documentation index at: </docs/llms.txt>

Use this file to discover all available pages before exploring further.

![light logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/light.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=78fd01ff4f4340295a4f66e2ea54903c)
![dark logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/dark.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=1298a0c3b3a1da603b190d0de0e31712)

### Agents and parallel work

### MCP

### Skills

### Plugins

### Artifacts

### Automation

### Guides

### Troubleshooting

## On this page

# Extend Claude with skills

Create, manage, and share skills to extend Claude’s capabilities in Claude Code. Includes custom commands and bundled skills.

`SKILL.md`
`/skill-name`
`/help`
`/compact`
`/debug`
`/code-review`
`.claude/commands/deploy.md`
`.claude/skills/deploy/SKILL.md`
`/deploy`
`.claude/commands/`

## [​](#bundled-skills) Bundled skills

`/doctor`
`/code-review`
`/batch`
`/debug`
`/loop`
`/claude-api`
`/`
`/verify`
`/workflow-authoring`
`disableBundledSkills`
`/doctor`
`/doctor`
`disableBundledSkills`
`DISABLE_DOCTOR_COMMAND`
`skillOverrides`
`"doctor": "off"`
`/doctor`

### [​](#run-and-verify-your-app) Run and verify your app

| Skill | Purpose |
| --- | --- |
| `/run` | Launch and drive your app to see a change working |
| `/verify` | Build and run your app to confirm a code change does what it should, without falling back to tests or type checks |
| `/run-skill-generator` | Teach `/run` and `/verify` how to build and launch your project |

`/run`
`/verify`
`/run-skill-generator`
`/run`
`/verify`
`/run`
`/verify`
`package.json`
`Makefile`
`/run-skill-generator`
`.claude/skills/run-<name>/`
`/run`
`/verify`
`/run-skill-generator`
`/verify`
`.claude/skills/verify/SKILL.md`
`/verify`

## [​](#getting-started) Getting started

### [​](#create-your-first-skill) Create your first skill

`/summarize-changes`

Create the skill directory

`mkdir -p ~/.claude/skills/summarize-changes`

Write SKILL.md

`SKILL.md`
`---`
`description`
`~/.claude/skills/summarize-changes/SKILL.md`
`---
description: Summarizes uncommitted changes and flags anything risky. Use when the user asks what changed, wants a commit message, or asks to review their diff.
---

## Current changes

!`git diff HEAD`

## Instructions

Summarize the changes above in two or three bullet points, then list any risks you notice such as missing error handling, hardcoded values, or tests that need updating. If the diff is empty, say there are no uncommitted changes.`
`!`git diff HEAD``

Test the skill

`claude`
`What did I change?`
`/summarize-changes`

## [​](#where-skills-live) Choose where skills load

| Location | Path | Loads in |
| --- | --- | --- |
| Enterprise | `.claude/skills/<skill-name>/SKILL.md` in the [managed settings directory](/docs/en/managed-settings#delivery-mechanisms) | All users on machines where your organization deploys it |
| Personal | `~/.claude/skills/<skill-name>/SKILL.md` | All your projects on this machine, but not [Cowork or cloud sessions](#skills-in-cowork-and-cloud-sessions) |
| Project | `.claude/skills/<skill-name>/SKILL.md` | Sessions in this repository. Commit it so your team gets it too |
| Nested | `<subdir>/.claude/skills/<skill-name>/SKILL.md` | Sessions started in or below `<subdir>`. A session started above it loads the skill once Claude works on files there. See [monorepos and subdirectories](#discovery-from-parent-and-nested-directories) |
| Additional directory | `.claude/skills/<skill-name>/SKILL.md` in a directory you pass with `--add-dir` | That session. See [directories outside the project](#skills-from-additional-directories) |
| Plugin | `<plugin>/skills/<skill-name>/SKILL.md` | Wherever the [plugin](/docs/en/plugins) is enabled, as `/plugin-name:skill-name` |
| claude.ai account | Skills you enable in your claude.ai settings | Cowork and cloud sessions. See [Skills synced from claude.ai](#how-synced-skills-behave) for local sessions |

`.claude/skills/<skill-name>/SKILL.md`
`~/.claude/skills/<skill-name>/SKILL.md`
`.claude/skills/<skill-name>/SKILL.md`
`<subdir>/.claude/skills/<skill-name>/SKILL.md`
`<subdir>`
`.claude/skills/<skill-name>/SKILL.md`
`--add-dir`
`<plugin>/skills/<skill-name>/SKILL.md`
`/plugin-name:skill-name`
`<skill-name>`
`SKILL.md`
`synced`
`~/.claude/skills/synced/`
`.claude/commands/`
`name`
`paths`
`.claude-plugin/plugin.json`
`<name>@skills-dir`
`.claude/skills/`

### [​](#discovery-from-parent-and-nested-directories) Load skills in monorepos and subdirectories

`.claude/skills/`
`packages/frontend/`
`/cd`
`.claude/skills/`
`/`
`/add-dir`
`deploy`
`apps/web/.claude/skills/`
`/deploy`
`apps/web/`
`/apps/web:deploy`

### [​](#skills-from-additional-directories) Load skills from a directory outside the project

`--add-dir`
`/add-dir`
`.claude/skills/`
`.claude/commands/`
`.claude/agents/`
`additionalDirectories`
`add_dirs`
`--add-dir`
`permissions.additionalDirectories`
`settings.json`
`.claude/skills/`
`--add-dir`
`.claude/commands/`
`.claude/agents/`
`project`
`strictPluginOnlyCustomization`
`--safe-mode`
`CLAUDE.md`

### [​](#resolve-skills-that-share-a-name) Resolve skills that share a name

`/name`

| Same name in | Which one runs |
| --- | --- |
| Two of enterprise, personal, and project | Enterprise over personal, and personal over project. With `deploy` in both `~/.claude/skills/` and the project’s `.claude/skills/`, `/deploy` runs the personal one |
| Any of those locations and a [bundled skill](#bundled-skills) | Your skill replaces the bundled command, but not its aliases. A project `code-review` skill replaces `/code-review`, and the bundled alias `/review` never runs your skill |
| A skill and a file in `.claude/commands/` | The skill |
| A project-root skill and a nested skill | Both load. See [monorepos and subdirectories](#discovery-from-parent-and-nested-directories) |
| A plugin skill and a skill at any of the locations above | Both load, because plugin skills are namespaced as `/plugin-name:skill-name` |
| Any of the above and a skill synced from claude.ai | The other skill or command. See [When a synced skill name matches another command](#when-a-synced-skill-name-matches-another-command) |

`deploy`
`~/.claude/skills/`
`.claude/skills/`
`/deploy`
`code-review`
`/code-review`
`/review`
`.claude/commands/`
`/plugin-name:skill-name`

### [​](#skills-in-cowork-and-cloud-sessions) Use skills in Cowork and cloud sessions

`~/.claude/skills/`
`.claude/skills/`
`~/.claude/skills/`
`.claude/skills/`
`.claude/settings.json`
`~/.claude/skills/`

### [​](#how-synced-skills-behave) Skills synced from claude.ai

`CLAUDE_CODE_SYNC_SKILLS`

#### [​](#where-synced-skills-load) Where synced skills load

Enable the skills for your claude.ai account

Run Claude Code in non-interactive mode with syncing turned on

`-p`
`CLAUDE_CODE_SYNC_SKILLS`
`1`
`CLAUDE_CODE_SYNC_SKILLS=1 claude -p "List the skills you have available"`
`~/.claude/skills/synced/`
`CLAUDE_CODE_SYNC_SKILLS`
`CLAUDE_CODE_SYNC_SKILLS_WAIT_TIMEOUT_MS`

Confirm the skills load in a local session

`CLAUDE_CODE_SYNC_SKILLS`
`/skills`
`claude.ai sync`
`~/.claude/skills/synced/`

#### [​](#when-a-synced-skill-name-matches-another-command) When a synced skill name matches another command

`.claude/commands/`
`/skills`
`/context`
`claude.ai sync`
`/`
`Commit`
`commit`
`claude.ai sync`

#### [​](#how-claude-code-handles-the-frontmatter-of-a-synced-skill) How Claude Code handles the frontmatter of a synced skill

`allowed-tools`

#### [​](#how-claude-code-handles-the-body-of-a-synced-skill) How Claude Code handles the body of a synced skill

`!`
`disableSkillShellExecution`
`!`
`@`
`${CLAUDE_PROJECT_DIR}`
`${CLAUDE_SESSION_ID}`
`@`
`!`
`disableSkillShellExecution`

### [​](#live-change-detection) Edit a skill during a session

`~/.claude/skills/`
`.claude/skills/`
`.claude/skills/`
`--add-dir`
`SKILL.md`
`hooks/`
`.mcp.json`
`agents/`
`output-styles/`
`/reload-plugins`

### [​](#remove-a-skill) Remove a skill

`~/.claude/skills/<skill-name>/`
`.claude/skills/<skill-name>/`
`/skills`
`.claude/skills/`
`/etc/claude-code/.claude/skills/<skill-name>/`
`/plugin`
`/plugin uninstall <plugin-name>@<marketplace-name>`
`/reload-plugins`
`~/.claude/skills/synced/`
`disableBundledSkills`
`true`
`/doctor`
`"off"`
`skillOverrides`
`disable-model-invocation: true`
`"user-invocable-only"`
`skillOverrides`

## [​](#configure-skills) Configure skills

`SKILL.md`

### [​](#types-of-skill-content) Types of skill content

`---
name: api-conventions
description: API design patterns for this codebase
---

When writing API endpoints:
- Use RESTful naming conventions
- Return consistent error formats
- Include request validation`
`/skill-name`
`disable-model-invocation: true`
`context: fork`
`---
name: deploy
description: Deploy the application to production
context: fork
disable-model-invocation: true
---

Deploy the application:
1. Run the test suite
2. Build the application
3. Push to the deployment target`

### [​](#frontmatter-reference) Frontmatter reference

`---`
`SKILL.md`
`---
name: my-skill
description: What this skill does
disable-model-invocation: true
allowed-tools: Read Grep
---

Your skill instructions here...`
`description`
`---`
`---`
`yes`
`no`
`on`
`off`
`1`
`0`
`true`
`false`
`true`
`false`

| Field | Required | Description |
| --- | --- | --- |
| `name` | No | Display name shown in skill listings. Defaults to the directory name. See [How a skill gets its command name](#how-a-skill-gets-its-command-name) for how the field interacts with the name you type to invoke the skill. |
| `description` | Recommended | What the skill does and when to use it. Claude uses this to decide when to apply the skill. If omitted, uses the first paragraph of markdown content. Put the key use case first: the combined `description` and `when_to_use` text is truncated at 1,536 characters in the skill listing to reduce context usage. |
| `when_to_use` | No | Additional context for when Claude should invoke the skill, such as trigger phrases or example requests. Appended to `description` in the skill listing and counts toward the 1,536-character cap. |
| `argument-hint` | No | Hint shown during autocomplete to indicate expected arguments. Example: `[issue-number]` or `[filename] [format]`. |
| `arguments` | No | Named positional arguments for [`$name` substitution](#available-string-substitutions) in the skill content. Accepts a space-separated string or a YAML list. Names map to argument positions in order. |
| `disable-model-invocation` | No | Set to `true` to prevent Claude from automatically loading this skill. Use for workflows you want to trigger manually with `/name`. Also prevents the skill from being [preloaded into subagents](/docs/en/sub-agents#preload-skills-into-subagents). As of v2.1.196, also prevents the skill from running when a [scheduled task](/docs/en/scheduled-tasks) fires with the skill as its prompt. Default: `false`. |
| `user-invocable` | No | Set to `false` when only Claude should invoke the skill: Claude Code hides it from the `/` menu and doesn’t run it when you type `/name`. Use for background knowledge users shouldn’t invoke directly. Default: `true`. |
| `allowed-tools` | No | Tools Claude can use without asking permission during the turn that invokes this skill. The grant clears when you send your next message. Accepts a space- or comma-separated string, or a YAML list. See [Pre-approve tools for a skill](#pre-approve-tools-for-a-skill). |
| `disallowed-tools` | No | Tools removed from Claude’s available pool while this skill is active. Use for autonomous skills that should never call certain tools, such as `AskUserQuestion` for a background loop. Accepts a space- or comma-separated string, or a YAML list. The restriction clears when you send your next message. Like deny rules, the field can’t remove [`EndConversation`](/docs/en/tools-reference#endconversation-tool-behavior) while any other tool remains. |
| `model` | No | Model to use when this skill is active. The override applies for the rest of the current turn and isn’t saved to settings. The session model resumes when you send your next prompt. Accepts the same values as [`/model`](/docs/en/model-config), or `inherit` to keep the active model. A value excluded by your organization’s [`availableModels`](/docs/en/model-config#restrict-model-selection) allowlist isn’t used, and the session keeps its current model. In [auto mode](/docs/en/permission-modes#eliminate-prompts-with-auto-mode), and in [plan mode while the classifier reviews commands](/docs/en/permission-modes#analyze-before-you-edit-with-plan-mode), a model that auto mode doesn’t support also isn’t used, and the session keeps its current model. With `context: fork`, the value sets the [forked subagent’s model](#run-skills-in-a-subagent) instead, and an excluded value follows the [same rules as a subagent model override](/docs/en/model-config#restrict-model-selection). |
| `effort` | No | [Effort level](/docs/en/model-config#adjust-effort-level) when this skill is active. Overrides the session effort level. Default: inherits from session. Options: `low`, `medium`, `high`, `xhigh`, `max`; available levels depend on the model. |
| `context` | No | Set to `fork` to run in a forked subagent context. See [Run skills in a subagent](#run-skills-in-a-subagent). |
| `agent` | No | Which subagent type to use when `context: fork` is set. |
| `background` | No | Only applies with `context: fork`. Set to `false` to wait for the forked subagent’s result in the turn that invoked the skill, instead of [running it in the background](#run-skills-in-a-subagent). Default: `true`. Requires Claude Code v2.1.218 or later. |
| `hooks` | No | Hooks that Claude Code registers when the skill is invoked and keeps running for the rest of the session. See [Hooks in skills and agents](/docs/en/hooks#hooks-in-skills-and-agents) for the configuration format and the `once` option. |
| `paths` | No | Glob patterns that limit when this skill is activated. Accepts a comma-separated string or a YAML list. When set, Claude loads the skill automatically only when working with files matching the patterns. Uses the same format as [path-specific rules](/docs/en/memory#path-specific-rules). |
| `shell` | No | Shell to use for `!`command`` and ````!` blocks in this skill. Accepts `bash` (default) or `powershell`. Setting `powershell` runs inline shell commands via PowerShell when the [PowerShell tool](/docs/en/tools-reference#powershell-tool) is enabled: it’s on by default on Windows without Git Bash, on by default with Git Bash for claude.ai and Console accounts, and needs `CLAUDE_CODE_USE_POWERSHELL_TOOL=1` in Amazon Bedrock, Google Cloud’s Agent Platform, and Microsoft Foundry sessions and on macOS, Linux, and WSL. Set it to `0` to turn the tool off. |
| `metadata` | No | Free-form YAML map for your own key-value data, such as entitlement or catalog fields, read by your own tooling from `SKILL.md`. Claude Code doesn’t act on its contents, and drops a value that isn’t a map. Don’t reuse frontmatter field names such as `paths` as keys. |
| `license` | No | License covering the skill. Part of the [Agent Skills](https://agentskills.io) spec; see [Using skill frontmatter outside Claude Code](#using-skill-frontmatter-outside-claude-code). Claude Code accepts the field but doesn’t act on it. |
| `compatibility` | No | Environment requirements for the skill, such as intended products or system prerequisites, as defined by the [Agent Skills](https://agentskills.io) spec; see [Using skill frontmatter outside Claude Code](#using-skill-frontmatter-outside-claude-code). Accepts a string of up to 500 characters. Claude Code accepts the field but doesn’t act on it. |

`name`
`description`
`description`
`when_to_use`
`when_to_use`
`description`
`argument-hint`
`[issue-number]`
`[filename] [format]`
`arguments`
`$name`
`disable-model-invocation`
`true`
`/name`
`false`
`user-invocable`
`false`
`/`
`/name`
`true`
`allowed-tools`
`disallowed-tools`
`AskUserQuestion`
`EndConversation`
`model`
`/model`
`inherit`
`availableModels`
`context: fork`
`effort`
`low`
`medium`
`high`
`xhigh`
`max`
`context`
`fork`
`agent`
`context: fork`
`background`
`context: fork`
`false`
`true`
`hooks`
`once`
`paths`
`shell`
`!`command``
````!`
`bash`
`powershell`
`powershell`
`CLAUDE_CODE_USE_POWERSHELL_TOOL=1`
`0`
`metadata`
`SKILL.md`
`paths`
`license`
`compatibility`

#### [​](#using-skill-frontmatter-outside-claude-code) Using skill frontmatter outside Claude Code

| Distribution path | Frontmatter fields you can use |
| --- | --- |
| Claude Code skills at [any level](#where-skills-live), including [plugin](/docs/en/plugins) skills | Every field in the table above |
| claude.ai skill uploads, the Skills API, and packaging with `package_skill.py` from [anthropics/skills](https://github.com/anthropics/skills) | `name`, `description`, `license`, `compatibility`, `metadata`, `allowed-tools` |

`package_skill.py`
`name`
`description`
`license`
`compatibility`
`metadata`
`allowed-tools`
`Unexpected key(s) in SKILL.md frontmatter: argument-hint. Allowed properties are: allowed-tools, compatibility, description, license, metadata, name`

#### [​](#how-a-skill-gets-its-command-name) How a skill gets its command name

`name`
`name`
`name`

| Skill location | Command name source | Example |
| --- | --- | --- |
| Skill directory under `~/.claude/skills/` or `.claude/skills/` | Directory name | `.claude/skills/deploy-staging/SKILL.md` → `/deploy-staging` |
| [Nested](#where-skills-live) `.claude/skills/` directory, when the name clashes with another skill | Subdirectory path relative to the working directory, then the skill directory name | `apps/web/.claude/skills/deploy/SKILL.md` → `/apps/web:deploy` |
| File under `.claude/commands/` | File name without extension | `.claude/commands/deploy.md` → `/deploy` |
| Plugin `skills/` subdirectory | Frontmatter `name` or the directory name, namespaced by plugin | `my-plugin/skills/review/SKILL.md` → `/my-plugin:review`, or `/my-plugin:fancy` with `name: fancy` |
| Plugin root `SKILL.md` | Frontmatter `name`, with the plugin directory name as a fallback | `my-plugin/SKILL.md` with `name: review` → `/my-plugin:review`. See [Path behavior rules](/docs/en/plugins-reference#path-behavior-rules) |

`~/.claude/skills/`
`.claude/skills/`
`.claude/skills/deploy-staging/SKILL.md`
`/deploy-staging`
`.claude/skills/`
`apps/web/.claude/skills/deploy/SKILL.md`
`/apps/web:deploy`
`.claude/commands/`
`.claude/commands/deploy.md`
`/deploy`
`skills/`
`name`
`my-plugin/skills/review/SKILL.md`
`/my-plugin:review`
`/my-plugin:fancy`
`name: fancy`
`SKILL.md`
`name`
`my-plugin/SKILL.md`
`name: review`
`/my-plugin:review`
`name`
`my-plugin/skills/review/SKILL.md`
`name: fancy`
`/my-plugin:fancy`
`/fancy`
`name`
`name: my-plugin:fancy`
`/my-plugin:fancy`
`name`
`help`
`feedback`
`/login`
`help`
`feedback`
`SKILL.md`
`name`
`name`

#### [​](#available-string-substitutions) Available string substitutions

| Variable | Description |
| --- | --- |
| `$ARGUMENTS` | All arguments passed when invoking the skill. When no placeholder receives an argument, Claude Code appends them as `ARGUMENTS: <value>`. See [Pass arguments to skills](#pass-arguments-to-skills). |
| `$ARGUMENTS[N]` | Access a specific argument by 0-based index, such as `$ARGUMENTS[0]` for the first argument. |
| `$N` | Shorthand for `$ARGUMENTS[N]`, such as `$0` for the first argument or `$1` for the second. |
| `$name` | Named argument declared in the [`arguments`](#frontmatter-reference) frontmatter list. Names map to positions in order, so with `arguments: [issue, branch]` the placeholder `$issue` expands to the first argument and `$branch` to the second. |
| `${CLAUDE_SESSION_ID}` | The current session ID. Useful for logging, creating session-specific files, or correlating skill output with sessions. |
| `${CLAUDE_EFFORT}` | The current effort level: `low`, `medium`, `high`, `xhigh`, or `max`. Ultracode is not a distinct level and reports as `xhigh`. Use this to adapt skill instructions to the active effort setting. |
| `${CLAUDE_SKILL_DIR}` | The directory containing the skill’s `SKILL.md` file. For plugin skills, this is the skill’s subdirectory within the plugin, not the plugin root. Use this in bash injection commands to reference scripts or files bundled with the skill, regardless of the current working directory. |
| `${CLAUDE_PROJECT_DIR}` | The project root directory. This is the same path [hooks](/docs/en/hooks#reference-scripts-by-path) and MCP servers receive as `CLAUDE_PROJECT_DIR`. Use this to reference project-local scripts or files, such as `${CLAUDE_PROJECT_DIR}/.claude/hooks/helper.sh`, independent of where the skill is installed. |
| `${CLAUDE_PLUGIN_ROOT}` | The plugin’s installation directory. Substituted only in plugin skills. Use this to reference scripts or files bundled anywhere in the plugin, including resources shared between the plugin’s skills. See [plugin environment variables](/docs/en/plugins-reference#environment-variables). |
| `${CLAUDE_PLUGIN_DATA}` | The plugin’s [persistent data directory](/docs/en/plugins-reference#persistent-data-directory), which survives plugin updates. Substituted only in plugin skills. Use this to reference installed dependencies, generated files, or caches that must outlive an update. |

`$ARGUMENTS`
`ARGUMENTS: <value>`
`$ARGUMENTS[N]`
`$ARGUMENTS[0]`
`$N`
`$ARGUMENTS[N]`
`$0`
`$1`
`$name`
`arguments`
`arguments: [issue, branch]`
`$issue`
`$branch`
`${CLAUDE_SESSION_ID}`
`${CLAUDE_EFFORT}`
`low`
`medium`
`high`
`xhigh`
`max`
`xhigh`
`${CLAUDE_SKILL_DIR}`
`SKILL.md`
`${CLAUDE_PROJECT_DIR}`
`CLAUDE_PROJECT_DIR`
`${CLAUDE_PROJECT_DIR}/.claude/hooks/helper.sh`
`${CLAUDE_PLUGIN_ROOT}`
`${CLAUDE_PLUGIN_DATA}`
`${CLAUDE_SKILL_DIR}`
`${CLAUDE_PROJECT_DIR}`
`allowed-tools`
`${CLAUDE_PLUGIN_ROOT}`
`${CLAUDE_PLUGIN_DATA}`
`---
name: render-chart
description: Render a chart from a CSV file
allowed-tools: Bash(${CLAUDE_SKILL_DIR}/scripts/render.sh *)
---

Run `${CLAUDE_SKILL_DIR}/scripts/render.sh <csv-file>` to render the chart.`
`~/.claude/skills/render-chart/`
`${CLAUDE_SKILL_DIR}`
`allowed-tools`
`${CLAUDE_PROJECT_DIR}`
`/my-skill "hello world" second`
`$0`
`hello world`
`$1`
`second`
`$ARGUMENTS`
`$2`
`arguments`
`$1`
`$ARGUMENTS`
`Summarize $0`
`/summarize "$ARGUMENTS from yesterday"`
`Summarize $ARGUMENTS from yesterday`
`${CLAUDE_*}`
`${CLAUDE_SKILL_DIR}`
`$`
`ARGUMENTS`
`$1.00`
`\$1.00`
`$`
`\\$1`
`$1`
`${CLAUDE_*}`
`---
name: session-logger
description: Log activity for this session
---

Log the following to logs/${CLAUDE_SESSION_ID}.log:

$ARGUMENTS`

### [​](#add-supporting-files) Add supporting files

`SKILL.md`
`my-skill/
├── SKILL.md (required - overview and navigation)
├── reference.md (detailed API docs - loaded when needed)
├── examples.md (usage examples - loaded when needed)
└── scripts/
 └── helper.py (utility script - executed, not loaded)`
`SKILL.md`
`## Additional resources

- For complete API details, see [reference.md](reference.md)
- For usage examples, see [examples.md](examples.md)`
`SKILL.md`

### [​](#control-who-invokes-a-skill) Control who invokes a skill

`/skill-name`
`disable-model-invocation: true`
`/commit`
`/deploy`
`/send-slack-message`
`user-invocable: false`
`legacy-system-context`
`/legacy-system-context`
`disable-model-invocation: true`
`---
name: deploy
description: Deploy the application to production
disable-model-invocation: true
---

Deploy $ARGUMENTS to production:

1. Run the test suite
2. Build the application
3. Push to the deployment target
4. Verify the deployment succeeded`
`/deploy`

| Frontmatter | You can invoke | Claude can invoke | When loaded into context |
| --- | --- | --- | --- |
| (default) | Yes | Yes | Description always in context, full skill loads when invoked |
| `disable-model-invocation: true` | Yes | No | Description not in context, full skill loads when you invoke |
| `user-invocable: false` | No | Yes | Description always in context, full skill loads when invoked |

`disable-model-invocation: true`
`user-invocable: false`

### [​](#skill-content-lifecycle) Skill content lifecycle

`SKILL.md`
`allowed-tools`
`description`

### [​](#pre-approve-tools-for-a-skill) Pre-approve tools for a skill

`allowed-tools`
`allowed-tools`
`-p`
`allowed-tools`
`---
name: commit
description: Stage and commit the current changes
disable-model-invocation: true
allowed-tools: Bash(git add *) Bash(git commit *) Bash(git status *)
---`
`disallowed-tools`
`EndConversation`

### [​](#pass-arguments-to-skills) Pass arguments to skills

`$ARGUMENTS`
`$ARGUMENTS`
`---
name: fix-issue
description: Fix a GitHub issue
disable-model-invocation: true
---

Fix GitHub issue $ARGUMENTS following our coding standards.

1. Read the issue description
2. Understand the requirements
3. Implement the fix
4. Write tests
5. Create a commit`
`/fix-issue 123`
`ARGUMENTS: <your input>`
`$ARGUMENTS`
`$1`
`/write-tests /fix-issue 123`
`123`
`$ARGUMENTS`
`/fix-issue 123`
`/code-review`
`/loop`
`/code-review`
`$ARGUMENTS[N]`
`$N`
`---
name: migrate-component
description: Migrate a component from one language to another
---

Migrate the $ARGUMENTS[0] component from $ARGUMENTS[1] to $ARGUMENTS[2].
Preserve all existing behavior and tests.`
`/migrate-component SearchBar JavaScript TypeScript`
`$ARGUMENTS[0]`
`SearchBar`
`$ARGUMENTS[1]`
`JavaScript`
`$ARGUMENTS[2]`
`TypeScript`
`$N`
`---
name: migrate-component
description: Migrate a component from one language to another
---

Migrate the $0 component from $1 to $2.
Preserve all existing behavior and tests.`

## [​](#advanced-patterns) Advanced patterns

### [​](#inject-dynamic-context) Inject dynamic context

`!`<command>``
`!`gh pr diff``
`---
name: pr-summary
description: Summarize changes in a pull request
context: fork
agent: Explore
allowed-tools: Bash(gh *)
---

## Pull request context
- PR diff: !`gh pr diff`
- PR comments: !`gh pr view --comments`
- Changed files: !`gh pr diff --name-only`

## Your task
Summarize this pull request...`
`!`<command>``
`!`
`!`
`KEY=!`cmd``
````!`
`## Environment
```!
node --version
git status --short
````
`"disableSkillShellExecution": true`
`[shell command execution disabled by policy]`
`ultrathink`

#### [​](#how-injected-commands-run) How injected commands run

`shell`
`shell: powershell`
`shell: bash`
`Skill <name> requires bash (`shell: bash` in frontmatter) but Git Bash was not found`
`cd`
`${CLAUDE_SKILL_DIR}`
`${CLAUDE_PROJECT_DIR}`
`bash`

#### [​](#when-an-injected-command-fails) When an injected command fails

`Shell command failed for pattern "..."`
`[stderr]`
`bash`
`bash`
`shell: powershell`
`grep`
`git diff`
`find`
`diff`
`bash`
`|| true`
`Shell command permission check failed for pattern "..."`
`allowed-tools`
`allowed-tools`

### [​](#run-skills-in-a-subagent) Run skills in a subagent

`context: fork`
`background: false`
`background: false`
`-p`
`CLAUDE_CODE_DISABLE_BACKGROUND_TASKS`
`1`
`background: false`
`/rewind`
`context: fork`

| Approach | System prompt | Task | Also loads |
| --- | --- | --- | --- |
| Skill with `context: fork` | From agent type | SKILL.md content | CLAUDE.md, except when the agent is Explore or Plan |
| Subagent with `skills` field | Subagent’s markdown body | Claude’s delegation message | Preloaded skills + CLAUDE.md |

`context: fork`
`skills`
`context: fork`
`agent: Explore`

#### [​](#example-research-skill-using-explore-agent) Example: Research skill using Explore agent

`---
name: deep-research
description: Research a topic thoroughly
context: fork
agent: Explore
---

Research $ARGUMENTS thoroughly:

1. Find relevant files using Glob and Grep
2. Read and analyze the code
3. Summarize findings with specific file references`
`agent`
`agent`
`Explore`
`Plan`
`general-purpose`
`.claude/agents/`
`general-purpose`

### [​](#restrict-claude’s-skill-access) Restrict Claude’s skill access

`disable-model-invocation: true`
`allowed-tools`
`/init`
`/security-review`
`/compact`
`/permissions`
`# Add to deny rules:
Skill`
`# Allow only specific skills
Skill(commit)
Skill(review-pr *)

# Deny specific skills
Skill(deploy *)`
`Skill(name)`
`Skill(name *)`
`deny`
`Skill(review)`
`/code-review`
`/review`
`Skill(deploy)`
`apps/web:deploy`
`allow`
`disable-model-invocation: true`
`user-invocable: false`
`disable-model-invocation: true`

### [​](#override-skill-visibility-from-settings) Override skill visibility from settings

`skillOverrides`
`/skills`
`Space`
`Esc`
`.claude/settings.local.json`

| Value | Listed to Claude | In `/` menu |
| --- | --- | --- |
| `"on"` | Name and description | Yes |
| `"name-only"` | Name only | Yes |
| `"user-invocable-only"` | Hidden | Yes |
| `"off"` | Hidden | Hidden |

`/`
`"on"`
`"name-only"`
`"user-invocable-only"`
`"off"`
`/skills`
`"user-invocable-only"`
`user-only`
`"off"`
`/`
`skillOverrides`
`skillOverrides`
`"on"`
`{
 "skillOverrides": {
 "legacy-context": "name-only",
 "deploy": "off"
 }
}`
`checkup`
`/doctor`
`skillOverrides`
`--settings`
`review`
`review`
`/code-review`
`/review`
`skillOverrides`
`/plugin`

### [​](#find-unused-skills) Find unused skills

`/skill-doctor`
`/plugin`
`-p`
`/skill-doctor`
`/skill-doctor`
`Skill usage reports are not available on this connection.`
`/skill-doctor`

## [​](#evaluate-and-iterate-on-a-skill) Evaluate and iterate on a skill

### [​](#run-evals-with-skill-creator) Run evals with skill-creator

`skill-creator`
`/plugin install skill-creator@claude-plugins-official`
`Marketplace "claude-plugins-official" not found`
`/plugin marketplace add anthropics/claude-plugins-official`
`Run /reload-plugins to activate.`
`evaluate my summarize-changes skill with skill-creator`
`evals/evals.json`
`grading.json`
`benchmark.json`

## [​](#share-skills) Share skills

`.claude/skills/`
`skills/`

### [​](#generate-visual-output) Generate visual output

`mkdir -p ~/.claude/skills/codebase-visualizer/scripts`
`~/.claude/skills/codebase-visualizer/SKILL.md`
`${CLAUDE_SKILL_DIR}`
`---
name: codebase-visualizer
description: Generate an interactive collapsible tree visualization of your codebase. Use when exploring a new repo, understanding project structure, or identifying large files.
allowed-tools: Bash(python3 *)
---

# Codebase Visualizer

Generate an interactive HTML tree view that shows your project's file structure with collapsible directories.

## Usage

Run the visualization script from your project root:

```bash
python3 ${CLAUDE_SKILL_DIR}/scripts/visualize.py .
```

This creates `codebase-map.html` in the current directory and opens it in your default browser.

## What the visualization shows

- **Collapsible directories**: Click folders to expand/collapse
- **File sizes**: Displayed next to each file
- **Colors**: Different colors for different file types
- **Directory totals**: Shows aggregate size of each folder`
`~/.claude/skills/codebase-visualizer/scripts/visualize.py`
`#!/usr/bin/env python3
"""Generate an interactive collapsible tree visualization of a codebase."""

import json
import sys
import webbrowser
from html import escape
from pathlib import Path
from collections import Counter

IGNORE = {'.git', 'node_modules', '__pycache__', '.venv', 'venv', 'dist', 'build'}

def scan(path: Path, stats: dict) -> dict:
 result = {"name": path.name, "children": [], "size": 0}
 try:
 for item in sorted(path.iterdir()):
 if item.name in IGNORE or item.name.startswith('.'):
 continue
 if item.is_file():
 size = item.stat().st_size
 ext = item.suffix.lower() or '(no ext)'
 result["children"].append({"name": item.name, "size": size, "ext": ext})
 result["size"] += size
 stats["files"] += 1
 stats["extensions"][ext] += 1
 stats["ext_sizes"][ext] += size
 elif item.is_dir():
 stats["dirs"] += 1
 child = scan(item, stats)
 if child["children"]:
 result["children"].append(child)
 result["size"] += child["size"]
 except PermissionError:
 pass
 return result

def generate_html(data: dict, stats: dict, output: Path) -> None:
 ext_sizes = stats["ext_sizes"]
 total_size = sum(ext_sizes.values()) or 1
 sorted_exts = sorted(ext_sizes.items(), key=lambda x: -x[1])[:8]
 colors = {
 '.js': '#f7df1e', '.ts': '#3178c6', '.py': '#3776ab', '.go': '#00add8',
 '.rs': '#dea584', '.rb': '#cc342d', '.css': '#264de4', '.html': '#e34c26',
 '.json': '#6b7280', '.md': '#083fa1', '.yaml': '#cb171e', '.yml': '#cb171e',
 '.mdx': '#083fa1', '.tsx': '#3178c6', '.jsx': '#61dafb', '.sh': '#4eaa25',
 }
 lang_bars = "".join(
 f'<div class="bar-row"><span class="bar-label">{ext}</span>'
 f'<div class="bar" style="width:{(size/total_size)*100}%;background:{colors.get(ext,"#6b7280")}"></div>'
 f'<span class="bar-pct">{(size/total_size)*100:.1f}%</span></div>'
 for ext, size in sorted_exts
 )
 def fmt(b):
 if b < 1024: return f"{b} B"
 if b < 1048576: return f"{b/1024:.1f} KB"
 return f"{b/1048576:.1f} MB"

 html = f'''<!DOCTYPE html>
<html><head>
 <meta charset="utf-8"><title>Codebase Explorer</title>
 <style>
 body {{ font: 14px/1.5 system-ui, sans-serif; margin: 0; background: #1a1a2e; color: #eee; }}
 .container {{ display: flex; height: 100vh; }}
 .sidebar {{ width: 280px; background: #252542; padding: 20px; border-right: 1px solid #3d3d5c; overflow-y: auto; flex-shrink: 0; }}
 .main {{ flex: 1; padding: 20px; overflow-y: auto; }}
 h1 {{ margin: 0 0 10px 0; font-size: 18px; }}
 h2 {{ margin: 20px 0 10px 0; font-size: 14px; color: #888; text-transform: uppercase; }}
 .stat {{ display: flex; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid #3d3d5c; }}
 .stat-value {{ font-weight: bold; }}
 .bar-row {{ display: flex; align-items: center; margin: 6px 0; }}
 .bar-label {{ width: 55px; font-size: 12px; color: #aaa; }}
 .bar {{ height: 18px; border-radius: 3px; }}
 .bar-pct {{ margin-left: 8px; font-size: 12px; color: #666; }}
 .tree {{ list-style: none; padding-left: 20px; }}
 details {{ cursor: pointer; }}
 summary {{ padding: 4px 8px; border-radius: 4px; }}
 summary:hover {{ background: #2d2d44; }}
 .folder {{ color: #ffd700; }}
 .file {{ display: flex; align-items: center; padding: 4px 8px; border-radius: 4px; }}
 .file:hover {{ background: #2d2d44; }}
 .size {{ color: #888; margin-left: auto; font-size: 12px; }}
 .dot {{ width: 8px; height: 8px; border-radius: 50%; margin-right: 8px; }}
 </style>
</head><body>
 <div class="container">
 <div class="sidebar">
 <h1>📊 Summary</h1>
 <div class="stat"><span>Files</span><span class="stat-value">{stats["files"]:,}</span></div>
 <div class="stat"><span>Directories</span><span class="stat-value">{stats["dirs"]:,}</span></div>
 <div class="stat"><span>Total size</span><span class="stat-value">{fmt(data["size"])}</span></div>
 <div class="stat"><span>File types</span><span class="stat-value">{len(stats["extensions"])}</span></div>
 <h2>By file type</h2>
 {lang_bars}
 </div>
 <div class="main">
 <h1>📁 {escape(data["name"])}</h1>
 <ul class="tree" id="root"></ul>
 </div>
 </div>
 <script>
 const data = {json.dumps(data)};
 const colors = {json.dumps(colors)};
 function fmt(b) {{ if (b < 1024) return b + ' B'; if (b < 1048576) return (b/1024).toFixed(1) + ' KB'; return (b/1048576).toFixed(1) + ' MB'; }}
 function esc(s) {{ return s.replace(/[&<>"']/g, c => ({{"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}}[c])); }}
 function render(node, parent) {{
 if (node.children) {{
 const det = document.createElement('details');
 det.open = parent === document.getElementById('root');
 det.innerHTML = `<summary><span class="folder">📁 ${{esc(node.name)}}</span><span class="size">${{fmt(node.size)}}</span></summary>`;
 const ul = document.createElement('ul'); ul.className = 'tree';
 node.children.sort((a,b) => (b.children?1:0)-(a.children?1:0) || a.name.localeCompare(b.name));
 node.children.forEach(c => render(c, ul));
 det.appendChild(ul);
 const li = document.createElement('li'); li.appendChild(det); parent.appendChild(li);
 }} else {{
 const li = document.createElement('li'); li.className = 'file';
 li.innerHTML = `<span class="dot" style="background:${{colors[node.ext]||'#6b7280'}}"></span>${{esc(node.name)}}<span class="size">${{fmt(node.size)}}</span>`;
 parent.appendChild(li);
 }}
 }}
 data.children.forEach(c => render(c, document.getElementById('root')));
 </script>
</body></html>'''
 output.write_text(html)

if __name__ == '__main__':
 target = Path(sys.argv[1] if len(sys.argv) > 1 else '.').resolve()
 stats = {"files": 0, "dirs": 0, "extensions": Counter(), "ext_sizes": Counter()}
 data = scan(target, stats)
 out = Path('codebase-map.html')
 generate_html(data, stats, out)
 print(f'Generated {out.absolute()}')
 webbrowser.open(f'file://{out.absolute()}')`
`Generated /path/to/codebase-map.html`

## [​](#troubleshooting) Troubleshooting

### [​](#skill-not-triggering) Skill not triggering

`What skills are available?`
`/skill-name`
`/skill-name`
`description`
`--debug`
`SKILL.md`
`claude plugin validate`
`claude plugin validate .claude/skills`
`claude plugin validate ~/.claude/skills`

### [​](#skill-triggers-too-often) Skill triggers too often

`disable-model-invocation: true`

### [​](#skill-descriptions-are-cut-short) Skill descriptions are cut short

`/doctor`
`/skill-doctor`
`--debug`
`/context`
`skillListingBudgetFraction`
`0.02`
`SLASH_COMMAND_TOOL_CHAR_BUDGET`
`"name-only"`
`skillOverrides`
`description`
`when_to_use`
`skillListingMaxDescChars`

## [​](#related-resources) Related resources

Was this page helpful?

![light logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/light.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=78fd01ff4f4340295a4f66e2ea54903c)
![dark logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/dark.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=1298a0c3b3a1da603b190d0de0e31712)

Company

Help and security

Learn

Terms and policies
