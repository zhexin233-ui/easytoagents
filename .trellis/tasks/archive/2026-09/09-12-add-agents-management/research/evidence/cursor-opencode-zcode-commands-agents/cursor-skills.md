Sign in

## Command Palette

Search for a command to run...

## Get Started

## Agent

[Agents Window](/docs/agent/agents-window)[Agent Review](/docs/agent/agent-review)[Design Mode](/docs/agent/design-mode)

## Grok Bot

[Overview](/docs/grok-bot)[Get Started](/docs/grok-bot/get-started)[Use Cases](/docs/grok-bot/use-cases)[Work with Grok Bot](/docs/grok-bot/work)[Settings](/docs/grok-bot/settings)

## Customize

[Overview](/docs/customize-cursor)[Plugins](/docs/plugins)[Rules](/docs/rules)[Skills](/docs/skills)[Subagents](/docs/subagents)[Hooks](/docs/hooks)[MCP](/docs/mcp)

## Cloud Agents

[Overview](/docs/cloud-agent)[Setup](/docs/cloud-agent/setup)[Builds](/docs/cloud-agent/builds)[Best Practices](/docs/cloud-agent/best-practices)[Choose Where Cloud Agents Run](/docs/cloud-agent/self-hosted/choose-runtime)[Automations](/docs/cloud-agent/automations)[Bugbot](/docs/bugbot)[Security Agents](/docs/security-agents)[PR Routing & Approval](/docs/approval-agents)[Mobile](/docs/cloud-agent/mobile)[Settings](/docs/cloud-agent/settings)[API](/docs/cloud-agent/api/endpoints)

## Origin

[Overview](/docs/origin)[Create a repository](/docs/origin/create-repository)[Clone, Push & Pull](/docs/origin/git)[Mirror GitHub](/docs/origin/mirror-github)[Pull requests](/docs/origin/pull-requests)[Browse & Search](/docs/origin/browse)[Settings](/docs/origin/settings)[Codebase settings](/docs/origin/codebase-settings)[Integrations](/docs/origin/integrations)

## Integrations

[Slack](/docs/integrations/slack)[Microsoft Teams](/docs/integrations/microsoft-teams)[Jira](/docs/integrations/jira)[Linear](/docs/integrations/linear)[Notion](/docs/integrations/notion)[GitHub](/docs/integrations/github)[GitLab](/docs/integrations/gitlab)[Azure DevOps](/docs/integrations/azure-devops)[Bitbucket](/docs/integrations/bitbucket)[JetBrains](/docs/integrations/jetbrains)[Xcode](/docs/integrations/xcode)[Deeplinks](/docs/reference/deeplinks)

## SDK

[TypeScript](/docs/sdk/typescript)[Python](/docs/sdk/python)[Bridge](/docs/sdk/bridge)[Changelog](/docs/sdk/changelog)

## CLI

[Overview](/docs/cli/overview)[Installation](/docs/cli/installation)[Capabilities](/docs/cli/using)[Changelog](/docs/cli/changelog)[Shell Mode](/docs/cli/shell-mode)[ACP](/docs/cli/acp)[Headless / CI](/docs/cli/headless)

## Teams & Enterprise

Customize

# Agent Skills

Agent Skills is an open standard for extending AI agents with specialized capabilities. Skills package domain-specific knowledge and workflows that agents can use to perform specific tasks.

## [What are skills?](#what-are-skills)

A skill is a portable, version-controlled package that teaches agents how to perform domain-specific tasks. Skills can include scripts, templates, and references that agents may act on using their tools.

Portable

Skills work across any agent that supports the Agent Skills standard.

Version-controlled

Skills are stored as files and can be tracked in your repository, or installed via GitHub repository links.

Actionable

Skills can include scripts, templates, and references that agents act on using their tools.

Progressive

Skills load resources on demand, keeping context usage efficient.

## [How skills work](#how-skills-work)

When Cursor starts, it automatically discovers skills from skill directories and makes them available to Agent. The agent is presented with available skills and decides when they are relevant based on context.

Skills can also be manually invoked by typing `/` in Agent chat and searching for the skill name. A skill invoked this way attaches to one message. To keep a skill on for the whole session, use it as a Custom Mode with `Option+EnterAlt+Enter` (Mac) or `Alt+Enter` (Windows). See [Custom Modes](/docs/agent/prompting#custom-modes).

## [Built-in Cursor skills](#built-in-cursor-skills)

Cursor includes a small set of built in skills to improve your general workflows. These skills are managed by Cursor and appear alongside the skills you add yourself.

| Skill | What it does |
| --- | --- |
| `/automate` | Creates Cursor Automations triggered by schedules, Slack messages, GitHub events, and other sources. |
| `/autopilot` | Monitors a pull request and addresses feedback, conflicts, failing checks, and follow-up work. |
| `/canvas` | Creates interactive React artifacts that render alongside the conversation. |
| `/create-hook` | Creates Cursor hooks and updates `hooks.json` for agent lifecycle events. |
| `/create-rule` | Creates Cursor rules with the appropriate scope and instructions. |
| `/create-skill` | Creates Agent Skills, including their structure and `SKILL.md` files. |
| `/create-subagent` | Creates custom subagents with focused roles and delegation instructions. |
| `/cursor-blame` | Investigates AI-authored changes and the prompts that produced them. |
| `/loop` | Runs a prompt or skill repeatedly at a specified interval. |
| `/migrate-to-skills` | Converts eligible dynamic rules and slash commands into Agent Skills. |
| `/review` | Selects and runs the appropriate code-review agent. |
| `/review-bugbot` | Reviews code for likely bugs and regressions with Bugbot. |
| `/review-security` | Reviews code for security vulnerabilities with Security Review. |
| `/sdk` | Helps you build applications and integrations with the Cursor SDK. |
| `/shell` | Runs the provided text as a literal shell command. |
| `/split-to-prs` | Splits large changes into smaller pull requests. |
| `/statusline` | Configures the Cursor CLI status line. |
| `/update-cli-config` | Updates Cursor CLI settings in `~/.cursor/cli-config.json`. |
| `/update-cursor-settings` | Finds and updates the appropriate Cursor or VS Code setting. |

You can run any built-in skill by typing `/` in Agent chat and selecting its name. Agent may also use some built-in skills automatically when your request clearly matches their purpose.

## [Skill directories](#skill-directories)

Skills are automatically loaded from these locations:

| Location | Scope |
| --- | --- |
| `.agents/skills/` | Project-level |
| `.cursor/skills/` | Project-level |
| `~/.agents/skills/` | User-level (global) on the local machine |
| `~/.cursor/skills/` | User-level (global) on the local machine |

User-level skills in `~/.cursor/skills/` stay on your machine until you [sync them for Cloud Agents](#use-personal-skills-with-cloud-agents) or [publish them to your team](/docs/plugins#publish-a-skill-to-your-team). Cursor does not copy `~/.agents/skills/` or unsynced local skills to Cloud Agents, Agents Window remote SSH sessions, or [self-hosted workers](/docs/cloud-agent/self-hosted/pool). On self-hosted workers, use project skills from the repo or bake skills into the worker image.

For compatibility, Cursor also loads skills from Claude and Codex directories: `.claude/skills/`, `.codex/skills/`, `~/.claude/skills/`, and `~/.codex/skills/`.

Each skill should be a folder containing a `SKILL.md` file:

```
.agents/└── skills/ └── my-skill/ └── SKILL.md
```

Skills can also include optional directories for scripts, references, and assets:

```
.agents/└── skills/ └── deploy-app/ ├── SKILL.md ├── scripts/ │ ├── deploy.sh │ └── validate.py ├── references/ │ └── REFERENCE.md └── assets/ └── config-template.json
```

### [Nested skill directories](#nested-skill-directories)

Skill directories can be organized into subdirectories. This is useful for grouping related skills by category, team, or domain. Cursor walks the skills root recursively and picks up any `SKILL.md` it finds:

```
.cursor/└── skills/ ├── shipping/ │ ├── land-it/ │ │ └── SKILL.md │ └── careful-merge-conflicts/ │ └── SKILL.md ├── debugging/ │ └── using-datadog-mcp/ │ └── SKILL.md └── workflow/ └── tdd/ └── SKILL.md
```

The category folder is purely organizational. The skill's identity comes from the folder containing `SKILL.md` (here `land-it`, `tdd`, etc.), not the parent category.

Cursor also discovers skills inside nested project subdirectories. A `.cursor/skills/` (or `.agents/skills/`) folder anywhere inside your repository is picked up, so monorepos can colocate skills with the package they apply to:

```
my-monorepo/├── .cursor/skills/ # repo-wide skills│ └── land-it/SKILL.md└── apps/ └── web/ └── .cursor/skills/ # app-specific skills └── deploy-web/SKILL.md
```

Skills in nested project directories are automatically scoped to files inside that directory. In the example above, `deploy-web` is only surfaced when the agent works with files under `apps/web/`, while skills in the repo-wide `.cursor/skills/` are available everywhere. This is similar to the [`paths` frontmatter field](#scoping-a-skill-to-specific-files) — you don't need to set `paths` on a nested skill to scope it to its directory.

## [SKILL.md file format](#skillmd-file-format)

Each skill is defined in a `SKILL.md` file with YAML frontmatter:

```
---name: my-skilldescription: Short description of what this skill does and when to use it. --- # My Skill Detailed instructions for the agent. ## When to Use - Use this skill when... - This skill is helpful for... ## Instructions - Step-by-step guidance for the agent - Domain-specific conventions - Best practices and patterns - Use the ask questions tool if you need to clarify requirements with the user
```

### [Frontmatter fields](#frontmatter-fields)

| Field | Required | Description |
| --- | --- | --- |
| `name` | Yes | Skill identifier. Lowercase letters, numbers, and hyphens only. Must match the parent folder name. |
| `description` | Yes | Describes what the skill does and when to use it. Used by the agent to determine relevance. |
| `paths` | No | Glob patterns that scope the skill to matching files. Accepts a comma-separated string or a list. When set, the skill is only surfaced when the agent works with files that match. |
| `disable-model-invocation` | No | When `true`, the skill is only included when explicitly invoked via `/skill-name`. The agent will not automatically apply it based on context. |
| `icon` | No | Icon shown on the badge when the skill is used as a [Custom Mode](/docs/agent/prompting#custom-modes). Defaults to a lightning icon. |
| `color` | No | Badge color when the skill is used as a Custom Mode. One of `default`, `green`, `cyan`, `blue`, `purple`, `magenta`, `orange`, `yellow`, `red`, or `brand`. |
| `metadata` | No | Arbitrary key-value mapping for additional metadata. |

## [Scoping a skill to specific files](#scoping-a-skill-to-specific-files)

Use the `paths` field to limit a skill to files that match one or more glob patterns. The skill is then only surfaced to the agent when it is reading or editing matching files. This keeps file-specific guidance out of context for unrelated work.

```
---name: react-component-patternsdescription: Conventions for writing React components in this codebase.paths: - "**/*.tsx" - "packages/ui/**/*.ts" --- # React component patterns ...
```

You can also pass a single comma-separated string:

```
---name: python-styledescription: Style rules for Python files.paths: "**/*.py, scripts/**/*.py" ---
```

Patterns follow standard glob syntax. Leave `paths` unset for a skill that should be available regardless of which files are open.

The legacy `globs` field is still accepted as a fallback for older skills, but new skills should use `paths`.

## [Disabling automatic invocation](#disabling-automatic-invocation)

By default, skills are automatically applied when the agent determines they are relevant. Set `disable-model-invocation: true` to make a skill behave like a traditional slash command, where it is only included in context when you explicitly type `/skill-name` in chat.

## [Using a skill as a Custom Mode](#using-a-skill-as-a-custom-mode)

Any skill with a valid frontmatter block can back a [Custom Mode](/docs/agent/prompting#custom-modes), which keeps the skill in context for the whole session. An active mode shows a badge in the chat input. Style it with the optional `icon` and `color` frontmatter fields:

```
---name: tdddescription: Test-driven development playbook for this repo.icon: beakercolor: green ---
```

Icons come from Cursor's icon set, with names like `code`, `terminal`, `bug`, `git-branch`, `book-open`, `beaker`, `shield`, and `rocket`. Unrecognized icons or colors fall back to the default badge, a lightning icon.

## [Including scripts in skills](#including-scripts-in-skills)

Skills can include a `scripts/` directory containing executable code that agents can run. Reference scripts in your `SKILL.md` using relative paths from the skill root.

```
---name: deploy-appdescription: Deploy the application to staging or production environments. Use when deploying code or when the user mentions deployment, releases, or environments. --- # Deploy App Deploy the application using the provided scripts. ## Usage Run the deployment script: `scripts/deploy.sh ` Where `` is either `staging` or `production`. ## Pre-deployment Validation Before deploying, run the validation script: `python scripts/validate.py`
```

The agent reads these instructions and executes the referenced scripts when the skill is invoked. Scripts can be written in any language—Bash, Python, JavaScript, or any other executable format supported by the agent implementation.

Scripts should be self-contained, include helpful error messages, and handle edge cases gracefully.

## [Optional directories](#optional-directories)

Skills support these optional directories:

| Directory | Purpose |
| --- | --- |
| `scripts/` | Executable code that agents can run |
| `references/` | Additional documentation loaded on demand |
| `assets/` | Static resources like templates, images, or data files |

Keep your main `SKILL.md` focused and move detailed reference material to separate files. This keeps context usage efficient since agents load resources progressively—only when needed.

## [Viewing skills](#viewing-skills)

To view discovered skills, open **Customize** in the sidebar and go to **Skills**. Skills installed from plugins or your project appear alongside rules in the **Agent Decides** section.

## [Use personal skills with Cloud Agents](#use-personal-skills-with-cloud-agents)

Personal skills in `~/.cursor/skills/` run on your local machine. Turn on **Sync Skills for Cloud Agents** to use those same skills with [Cloud Agents](/docs/cloud-agent).

Synced skills stay private to you. They are not shared with your team.

### [Sync your skills](#sync-your-skills)

1. Open **Settings → Agents**.
2. Under **Context and Tools**, turn on **Sync Skills for Cloud Agents**.
3. Confirm. Cursor copies the contents of `~/.cursor/skills/` so your Cloud Agents can use them.

You can also start a sync from **Customize → Skills**.

Only `~/.cursor/skills/` syncs. Project skills, `~/.agents/skills/`, and other files on your machine stay local.

### [Stop syncing](#stop-syncing)

Turn off **Sync Skills for Cloud Agents** under **Settings → Agents**. Cursor moves the synced skills back to your machine.

### [Team admin controls](#team-admin-controls)

On Teams and Enterprise plans, admins decide whether members can sync:

1. Open [Team Settings](https://cursor.com/dashboard/team-settings).
2. Go to **Security & Identity**.
3. Turn **Sync Skills for Cloud Agents** off to disable sync for everyone on the team.

When the team setting is on, each member still chooses whether to sync. When it is off, no one on the team can sync.

To share a skill with teammates, [publish it to your team marketplace](/docs/plugins#publish-a-skill-to-your-team). Publishing is different from sync: teammates can install a published skill, and Cursor stores a copy in a repository hosted for your team.

## [Installing skills from GitHub](#installing-skills-from-github)

You can import skills from GitHub repositories:

1. Open **Customize** in the sidebar
2. Go to **Rules** and click **Add Rule**
3. Select **Remote Rule (Github)**
4. Enter the GitHub repository URL

## [Migrating rules and commands to skills](#migrating-rules-and-commands-to-skills)

Cursor includes a built-in `/migrate-to-skills` skill in 2.4 that helps you convert existing dynamic rules and slash commands to skills.

The migration skill converts:

* **Dynamic rules**: Rules that use the "Apply Intelligently" configuration—rules with `alwaysApply: false` (or undefined) and no `globs` patterns defined. These are converted to standard skills.
* **Slash commands**: Both user-level and workspace-level commands are converted to skills with `disable-model-invocation: true`, preserving their explicit invocation behavior.

To migrate:

1. Type `/migrate-to-skills` in Agent chat
2. The agent will identify eligible rules and commands and convert them to skills
3. Review the generated skills in `.cursor/skills/`

Rules with `alwaysApply: true` or specific `globs` patterns are not migrated, as they have explicit triggering conditions that differ from skill behavior. User rules are also not migrated since they are not stored on the file system.

## [Learn more](#learn-more)

Agent Skills is an open standard. Learn more at [agentskills.io](https://agentskills.io).

## [Related](#related)

* [Skills help](/help/customization/skills)
* [Publish a skill to your team](/docs/plugins#publish-a-skill-to-your-team)
* [Self-Hosted Machines](/docs/cloud-agent/self-hosted/choose-runtime)

Sign in

Download

## Command Palette

Search for a command to run...

## Get Started

[Welcome](/docs)[Quickstart](/docs/get-started/quickstart)[Changelog](https://cursor.com/changelog)

## Agent

[Overview](/docs/agent/overview)[Agents Window](/docs/agent/agents-window)[Agent Review](/docs/agent/agent-review)[Planning](/docs/agent/plan-mode)[Prompting](/docs/agent/prompting)[Debugging](/docs/agent/debug-mode)[Design Mode](/docs/agent/design-mode)

## Grok Bot

[Overview](/docs/grok-bot)[Get Started](/docs/grok-bot/get-started)[Use Cases](/docs/grok-bot/use-cases)[Work with Grok Bot](/docs/grok-bot/work)[Settings](/docs/grok-bot/settings)

## Customize

[Overview](/docs/customize-cursor)[Plugins](/docs/plugins)[Rules](/docs/rules)[Skills](/docs/skills)[Subagents](/docs/subagents)[Hooks](/docs/hooks)[MCP](/docs/mcp)

## Cloud Agents

[Overview](/docs/cloud-agent)[Setup](/docs/cloud-agent/setup)[Builds](/docs/cloud-agent/builds)[Best Practices](/docs/cloud-agent/best-practices)[Choose Where Cloud Agents Run](/docs/cloud-agent/self-hosted/choose-runtime)[Automations](/docs/cloud-agent/automations)[Bugbot](/docs/bugbot)[Security Agents](/docs/security-agents)[PR Routing & Approval](/docs/approval-agents)[Mobile](/docs/cloud-agent/mobile)[Settings](/docs/cloud-agent/settings)[API](/docs/cloud-agent/api/endpoints)

## Origin

[Overview](/docs/origin)[Create a repository](/docs/origin/create-repository)[Clone, Push & Pull](/docs/origin/git)[Mirror GitHub](/docs/origin/mirror-github)[Pull requests](/docs/origin/pull-requests)[Browse & Search](/docs/origin/browse)[Settings](/docs/origin/settings)[Codebase settings](/docs/origin/codebase-settings)[Integrations](/docs/origin/integrations)

## Integrations

[Slack](/docs/integrations/slack)[Microsoft Teams](/docs/integrations/microsoft-teams)[Jira](/docs/integrations/jira)[Linear](/docs/integrations/linear)[Notion](/docs/integrations/notion)[GitHub](/docs/integrations/github)[GitLab](/docs/integrations/gitlab)[Azure DevOps](/docs/integrations/azure-devops)[Bitbucket](/docs/integrations/bitbucket)[JetBrains](/docs/integrations/jetbrains)[Xcode](/docs/integrations/xcode)[Deeplinks](/docs/reference/deeplinks)

## SDK

[TypeScript](/docs/sdk/typescript)[Python](/docs/sdk/python)[Bridge](/docs/sdk/bridge)[Changelog](/docs/sdk/changelog)

## CLI

[Overview](/docs/cli/overview)[Installation](/docs/cli/installation)[Capabilities](/docs/cli/using)[Changelog](/docs/cli/changelog)[Shell Mode](/docs/cli/shell-mode)[ACP](/docs/cli/acp)[Headless / CI](/docs/cli/headless)

## Teams & Enterprise

Customize

# Agent Skills

Agent Skills is an open standard for extending AI agents with specialized capabilities. Skills package domain-specific knowledge and workflows that agents can use to perform specific tasks.

## [What are skills?](#what-are-skills)

A skill is a portable, version-controlled package that teaches agents how to perform domain-specific tasks. Skills can include scripts, templates, and references that agents may act on using their tools.

Portable

Skills work across any agent that supports the Agent Skills standard.

Version-controlled

Skills are stored as files and can be tracked in your repository, or installed via GitHub repository links.

Actionable

Skills can include scripts, templates, and references that agents act on using their tools.

Progressive

Skills load resources on demand, keeping context usage efficient.

## [How skills work](#how-skills-work)

When Cursor starts, it automatically discovers skills from skill directories and makes them available to Agent. The agent is presented with available skills and decides when they are relevant based on context.

Skills can also be manually invoked by typing `/` in Agent chat and searching for the skill name. A skill invoked this way attaches to one message. To keep a skill on for the whole session, use it as a Custom Mode with `Option+EnterAlt+Enter` (Mac) or `Alt+Enter` (Windows). See [Custom Modes](/docs/agent/prompting#custom-modes).

## [Built-in Cursor skills](#built-in-cursor-skills)

Cursor includes a small set of built in skills to improve your general workflows. These skills are managed by Cursor and appear alongside the skills you add yourself.

| Skill | What it does |
| --- | --- |
| `/automate` | Creates Cursor Automations triggered by schedules, Slack messages, GitHub events, and other sources. |
| `/autopilot` | Monitors a pull request and addresses feedback, conflicts, failing checks, and follow-up work. |
| `/canvas` | Creates interactive React artifacts that render alongside the conversation. |
| `/create-hook` | Creates Cursor hooks and updates `hooks.json` for agent lifecycle events. |
| `/create-rule` | Creates Cursor rules with the appropriate scope and instructions. |
| `/create-skill` | Creates Agent Skills, including their structure and `SKILL.md` files. |
| `/create-subagent` | Creates custom subagents with focused roles and delegation instructions. |
| `/cursor-blame` | Investigates AI-authored changes and the prompts that produced them. |
| `/loop` | Runs a prompt or skill repeatedly at a specified interval. |
| `/migrate-to-skills` | Converts eligible dynamic rules and slash commands into Agent Skills. |
| `/review` | Selects and runs the appropriate code-review agent. |
| `/review-bugbot` | Reviews code for likely bugs and regressions with Bugbot. |
| `/review-security` | Reviews code for security vulnerabilities with Security Review. |
| `/sdk` | Helps you build applications and integrations with the Cursor SDK. |
| `/shell` | Runs the provided text as a literal shell command. |
| `/split-to-prs` | Splits large changes into smaller pull requests. |
| `/statusline` | Configures the Cursor CLI status line. |
| `/update-cli-config` | Updates Cursor CLI settings in `~/.cursor/cli-config.json`. |
| `/update-cursor-settings` | Finds and updates the appropriate Cursor or VS Code setting. |

You can run any built-in skill by typing `/` in Agent chat and selecting its name. Agent may also use some built-in skills automatically when your request clearly matches their purpose.

## [Skill directories](#skill-directories)

Skills are automatically loaded from these locations:

| Location | Scope |
| --- | --- |
| `.agents/skills/` | Project-level |
| `.cursor/skills/` | Project-level |
| `~/.agents/skills/` | User-level (global) on the local machine |
| `~/.cursor/skills/` | User-level (global) on the local machine |

User-level skills in `~/.cursor/skills/` stay on your machine until you [sync them for Cloud Agents](#use-personal-skills-with-cloud-agents) or [publish them to your team](/docs/plugins#publish-a-skill-to-your-team). Cursor does not copy `~/.agents/skills/` or unsynced local skills to Cloud Agents, Agents Window remote SSH sessions, or [self-hosted workers](/docs/cloud-agent/self-hosted/pool). On self-hosted workers, use project skills from the repo or bake skills into the worker image.

For compatibility, Cursor also loads skills from Claude and Codex directories: `.claude/skills/`, `.codex/skills/`, `~/.claude/skills/`, and `~/.codex/skills/`.

Each skill should be a folder containing a `SKILL.md` file:

```
.agents/└── skills/ └── my-skill/ └── SKILL.md
```

Skills can also include optional directories for scripts, references, and assets:

```
.agents/└── skills/ └── deploy-app/ ├── SKILL.md ├── scripts/ │ ├── deploy.sh │ └── validate.py ├── references/ │ └── REFERENCE.md └── assets/ └── config-template.json
```

### [Nested skill directories](#nested-skill-directories)

Skill directories can be organized into subdirectories. This is useful for grouping related skills by category, team, or domain. Cursor walks the skills root recursively and picks up any `SKILL.md` it finds:

```
.cursor/└── skills/ ├── shipping/ │ ├── land-it/ │ │ └── SKILL.md │ └── careful-merge-conflicts/ │ └── SKILL.md ├── debugging/ │ └── using-datadog-mcp/ │ └── SKILL.md └── workflow/ └── tdd/ └── SKILL.md
```

The category folder is purely organizational. The skill's identity comes from the folder containing `SKILL.md` (here `land-it`, `tdd`, etc.), not the parent category.

Cursor also discovers skills inside nested project subdirectories. A `.cursor/skills/` (or `.agents/skills/`) folder anywhere inside your repository is picked up, so monorepos can colocate skills with the package they apply to:

```
my-monorepo/├── .cursor/skills/ # repo-wide skills│ └── land-it/SKILL.md└── apps/ └── web/ └── .cursor/skills/ # app-specific skills └── deploy-web/SKILL.md
```

Skills in nested project directories are automatically scoped to files inside that directory. In the example above, `deploy-web` is only surfaced when the agent works with files under `apps/web/`, while skills in the repo-wide `.cursor/skills/` are available everywhere. This is similar to the [`paths` frontmatter field](#scoping-a-skill-to-specific-files) — you don't need to set `paths` on a nested skill to scope it to its directory.

## [SKILL.md file format](#skillmd-file-format)

Each skill is defined in a `SKILL.md` file with YAML frontmatter:

```
---name: my-skilldescription: Short description of what this skill does and when to use it. --- # My Skill Detailed instructions for the agent. ## When to Use - Use this skill when... - This skill is helpful for... ## Instructions - Step-by-step guidance for the agent - Domain-specific conventions - Best practices and patterns - Use the ask questions tool if you need to clarify requirements with the user
```

### [Frontmatter fields](#frontmatter-fields)

| Field | Required | Description |
| --- | --- | --- |
| `name` | Yes | Skill identifier. Lowercase letters, numbers, and hyphens only. Must match the parent folder name. |
| `description` | Yes | Describes what the skill does and when to use it. Used by the agent to determine relevance. |
| `paths` | No | Glob patterns that scope the skill to matching files. Accepts a comma-separated string or a list. When set, the skill is only surfaced when the agent works with files that match. |
| `disable-model-invocation` | No | When `true`, the skill is only included when explicitly invoked via `/skill-name`. The agent will not automatically apply it based on context. |
| `icon` | No | Icon shown on the badge when the skill is used as a [Custom Mode](/docs/agent/prompting#custom-modes). Defaults to a lightning icon. |
| `color` | No | Badge color when the skill is used as a Custom Mode. One of `default`, `green`, `cyan`, `blue`, `purple`, `magenta`, `orange`, `yellow`, `red`, or `brand`. |
| `metadata` | No | Arbitrary key-value mapping for additional metadata. |

## [Scoping a skill to specific files](#scoping-a-skill-to-specific-files)

Use the `paths` field to limit a skill to files that match one or more glob patterns. The skill is then only surfaced to the agent when it is reading or editing matching files. This keeps file-specific guidance out of context for unrelated work.

```
---name: react-component-patternsdescription: Conventions for writing React components in this codebase.paths: - "**/*.tsx" - "packages/ui/**/*.ts" --- # React component patterns ...
```

You can also pass a single comma-separated string:

```
---name: python-styledescription: Style rules for Python files.paths: "**/*.py, scripts/**/*.py" ---
```

Patterns follow standard glob syntax. Leave `paths` unset for a skill that should be available regardless of which files are open.

The legacy `globs` field is still accepted as a fallback for older skills, but new skills should use `paths`.

## [Disabling automatic invocation](#disabling-automatic-invocation)

By default, skills are automatically applied when the agent determines they are relevant. Set `disable-model-invocation: true` to make a skill behave like a traditional slash command, where it is only included in context when you explicitly type `/skill-name` in chat.

## [Using a skill as a Custom Mode](#using-a-skill-as-a-custom-mode)

Any skill with a valid frontmatter block can back a [Custom Mode](/docs/agent/prompting#custom-modes), which keeps the skill in context for the whole session. An active mode shows a badge in the chat input. Style it with the optional `icon` and `color` frontmatter fields:

```
---name: tdddescription: Test-driven development playbook for this repo.icon: beakercolor: green ---
```

Icons come from Cursor's icon set, with names like `code`, `terminal`, `bug`, `git-branch`, `book-open`, `beaker`, `shield`, and `rocket`. Unrecognized icons or colors fall back to the default badge, a lightning icon.

## [Including scripts in skills](#including-scripts-in-skills)

Skills can include a `scripts/` directory containing executable code that agents can run. Reference scripts in your `SKILL.md` using relative paths from the skill root.

```
---name: deploy-appdescription: Deploy the application to staging or production environments. Use when deploying code or when the user mentions deployment, releases, or environments. --- # Deploy App Deploy the application using the provided scripts. ## Usage Run the deployment script: `scripts/deploy.sh ` Where `` is either `staging` or `production`. ## Pre-deployment Validation Before deploying, run the validation script: `python scripts/validate.py`
```

The agent reads these instructions and executes the referenced scripts when the skill is invoked. Scripts can be written in any language—Bash, Python, JavaScript, or any other executable format supported by the agent implementation.

Scripts should be self-contained, include helpful error messages, and handle edge cases gracefully.

## [Optional directories](#optional-directories)

Skills support these optional directories:

| Directory | Purpose |
| --- | --- |
| `scripts/` | Executable code that agents can run |
| `references/` | Additional documentation loaded on demand |
| `assets/` | Static resources like templates, images, or data files |

Keep your main `SKILL.md` focused and move detailed reference material to separate files. This keeps context usage efficient since agents load resources progressively—only when needed.

## [Viewing skills](#viewing-skills)

To view discovered skills, open **Customize** in the sidebar and go to **Skills**. Skills installed from plugins or your project appear alongside rules in the **Agent Decides** section.

## [Use personal skills with Cloud Agents](#use-personal-skills-with-cloud-agents)

Personal skills in `~/.cursor/skills/` run on your local machine. Turn on **Sync Skills for Cloud Agents** to use those same skills with [Cloud Agents](/docs/cloud-agent).

Synced skills stay private to you. They are not shared with your team.

### [Sync your skills](#sync-your-skills)

1. Open **Settings → Agents**.
2. Under **Context and Tools**, turn on **Sync Skills for Cloud Agents**.
3. Confirm. Cursor copies the contents of `~/.cursor/skills/` so your Cloud Agents can use them.

You can also start a sync from **Customize → Skills**.

Only `~/.cursor/skills/` syncs. Project skills, `~/.agents/skills/`, and other files on your machine stay local.

### [Stop syncing](#stop-syncing)

Turn off **Sync Skills for Cloud Agents** under **Settings → Agents**. Cursor moves the synced skills back to your machine.

### [Team admin controls](#team-admin-controls)

On Teams and Enterprise plans, admins decide whether members can sync:

1. Open [Team Settings](https://cursor.com/dashboard/team-settings).
2. Go to **Security & Identity**.
3. Turn **Sync Skills for Cloud Agents** off to disable sync for everyone on the team.

When the team setting is on, each member still chooses whether to sync. When it is off, no one on the team can sync.

To share a skill with teammates, [publish it to your team marketplace](/docs/plugins#publish-a-skill-to-your-team). Publishing is different from sync: teammates can install a published skill, and Cursor stores a copy in a repository hosted for your team.

## [Installing skills from GitHub](#installing-skills-from-github)

You can import skills from GitHub repositories:

1. Open **Customize** in the sidebar
2. Go to **Rules** and click **Add Rule**
3. Select **Remote Rule (Github)**
4. Enter the GitHub repository URL

## [Migrating rules and commands to skills](#migrating-rules-and-commands-to-skills)

Cursor includes a built-in `/migrate-to-skills` skill in 2.4 that helps you convert existing dynamic rules and slash commands to skills.

The migration skill converts:

* **Dynamic rules**: Rules that use the "Apply Intelligently" configuration—rules with `alwaysApply: false` (or undefined) and no `globs` patterns defined. These are converted to standard skills.
* **Slash commands**: Both user-level and workspace-level commands are converted to skills with `disable-model-invocation: true`, preserving their explicit invocation behavior.

To migrate:

1. Type `/migrate-to-skills` in Agent chat
2. The agent will identify eligible rules and commands and convert them to skills
3. Review the generated skills in `.cursor/skills/`

Rules with `alwaysApply: true` or specific `globs` patterns are not migrated, as they have explicit triggering conditions that differ from skill behavior. User rules are also not migrated since they are not stored on the file system.

## [Learn more](#learn-more)

Agent Skills is an open standard. Learn more at [agentskills.io](https://agentskills.io).

## [Related](#related)

* [Skills help](/help/customization/skills)
* [Publish a skill to your team](/docs/plugins#publish-a-skill-to-your-team)
* [Self-Hosted Machines](/docs/cloud-agent/self-hosted/choose-runtime)
