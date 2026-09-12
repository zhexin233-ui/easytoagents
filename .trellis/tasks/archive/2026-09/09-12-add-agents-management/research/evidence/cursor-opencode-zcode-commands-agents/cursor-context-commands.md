Sign in

Download

[← Back](/help)

# Skills

Skills are reusable sets of instructions that teach Agent how to handle specific tasks. They're more detailed than rules and designed for multi-step workflows.

## [What are Skills?](#what-are-skills)

Skills are markdown files that teach Agent specialized workflows. For example, deploying an app to staging or running a security audit across your dependencies. They're reusable across conversations and shareable across your team.

## [How do I create a skill?](#how-do-i-create-a-skill)

Type `/create-skill` in chat and describe the skill you want. Cursor comes with a built-in skill that walks you through naming, structuring, and saving a new skill.

To create one manually, add a `SKILL.md` file in `.cursor/skills/your-skill-name/`:

```
# Deploy to staging 1. Run the test suite2. Build the production bundle3. Deploy to the staging environment4. Verify the deployment health check
```

You can also organize skills into subfolders, like `.cursor/skills/shipping/deploy-staging/SKILL.md`. Cursor walks the skills root recursively, so category folders work for grouping related skills. The skill's name comes from the folder that contains `SKILL.md`, not the category above it.

Skills are automatically loaded from `.agents/skills/`, `.cursor/skills/`, `~/.agents/skills/` (global on the local machine), and `~/.cursor/skills/` (global on the local machine), including nested project subdirectories such as `apps/web/.cursor/skills/` in a monorepo. Skills in a nested project directory are automatically scoped to files inside that directory. For example, skills under `apps/web/.cursor/skills/` are only surfaced when the agent works with files in `apps/web/`, similar to the [`paths` frontmatter field](#how-do-i-scope-a-skill-to-specific-files). For compatibility, Cursor also loads skills from Claude and Codex directories: `.claude/skills/`, `.codex/skills/`, `~/.claude/skills/`, and `~/.codex/skills/`.

## [How do I use personal skills with Cloud Agents?](#how-do-i-use-personal-skills-with-cloud-agents)

Personal skills in `~/.cursor/skills/` stay on your machine until you sync them. Open **Settings → Agents**, then turn on **Sync Skills for Cloud Agents** under **Context and Tools**. Synced skills stay private to you.

Only `~/.cursor/skills/` syncs. To share a skill with teammates, [publish it to your team marketplace](/docs/plugins#publish-a-skill-to-your-team).

On Teams and Enterprise, admins can turn sync off for everyone from [Team Settings](https://cursor.com/dashboard/team-settings) under **Security & Identity**. See [Use personal skills with Cloud Agents](/docs/skills#use-personal-skills-with-cloud-agents).

## [How do I publish a skill to my team?](#how-do-i-publish-a-skill-to-my-team)

Open **Customize → Skills**, open a personal skill, and choose **Publish**. Cursor adds it to your team's Default marketplace. Teammates install it from there. See [Publish a skill to your team](/docs/plugins#publish-a-skill-to-your-team).

## [Are user-level skills available on Cloud Agents and remote workers?](#are-user-level-skills-available-on-cloud-agents-and-remote-workers)

You can [sync](#how-do-i-use-personal-skills-with-cloud-agents) `~/.cursor/skills/` so Cloud Agents can use those skills. `~/.agents/skills/` and unsynced local skills stay on your machine. They are not copied to Cloud Agents, Agents Window remote SSH, or [self-hosted workers](/docs/cloud-agent/self-hosted/pool). On self-hosted workers, use project skills in the repo or bake skills into the worker image.

## [How do I scope a skill to specific files?](#how-do-i-scope-a-skill-to-specific-files)

Add a `paths` field to the skill's frontmatter. The skill only applies when the agent works with files that match:

```
---name: react-component-patternsdescription: Conventions for writing React components.paths: - "**/*.tsx" --- # React component patterns ...
```

`paths` accepts a list or a comma-separated string of glob patterns. Leave it unset for a skill that should be available regardless of which files are open. See the [Skills reference](/docs/skills) for the full frontmatter schema.

## [How do I use a skill?](#how-do-i-use-a-skill)

Type `/` followed by the skill name in chat to run it (e.g., `/write-tests`), or type `@` and select it to attach as context. Agent reads the skill file and follows the instructions.

## [When should I use skills instead of rules?](#when-should-i-use-skills-instead-of-rules)

| Rules | Skills |
| --- | --- |
| **Purpose** | Short coding guidelines and constraints | Multi-step workflows and procedures |
| **Length** | A few lines to a few hundred lines | Often longer, with detailed step-by-step instructions |
| **How applied** | Included as context in every (or matching) conversation | Invoked on demand with `/skill-name` or `@skill-name` |
| **Example** | "Use TypeScript for all new files" | "Deploy to staging: run tests, build, deploy, verify" |

Use a rule when a short instruction is enough. Use a skill when Agent needs a detailed, repeatable process to follow.

## [How do I convert a rule into a skill?](#how-do-i-convert-a-rule-into-a-skill)

1. Type `/create-skill` in chat and tell Agent to turn your rule into a new skill (e.g., "turn `@my-rule` into a skill")
2. Review the generated skill file
3. Delete the original rule file if you no longer need it

## [How do I migrate commands to skills?](#how-do-i-migrate-commands-to-skills)

Type `/migrate-to-skills` in Agent chat. This built-in skill (available in Cursor 2.4+) identifies eligible rules and commands and converts them to skills automatically.

It converts:

* **Dynamic rules**: Rules with `alwaysApply: false` (or undefined) and no `globs` patterns.
* **Slash commands**: Both user-level and workspace-level commands, preserving their explicit invocation behavior.

Rules with `alwaysApply: true` or specific `globs` patterns are not migrated, as they have explicit triggering conditions that differ from skill behavior. User rules are also not migrated since they are not stored on the file system.

## [Related](#related)

* [Skills reference](/docs/skills)
* [Publish a skill to your team](/docs/plugins#publish-a-skill-to-your-team)
* [Rules](/help/customization/rules)
* [Self-Hosted Machines](/docs/cloud-agent/self-hosted/choose-runtime)

## Was this article helpful?

Sign in

Download

[← Back](/help)

# Skills

Skills are reusable sets of instructions that teach Agent how to handle specific tasks. They're more detailed than rules and designed for multi-step workflows.

## [What are Skills?](#what-are-skills)

Skills are markdown files that teach Agent specialized workflows. For example, deploying an app to staging or running a security audit across your dependencies. They're reusable across conversations and shareable across your team.

## [How do I create a skill?](#how-do-i-create-a-skill)

Type `/create-skill` in chat and describe the skill you want. Cursor comes with a built-in skill that walks you through naming, structuring, and saving a new skill.

To create one manually, add a `SKILL.md` file in `.cursor/skills/your-skill-name/`:

```
# Deploy to staging 1. Run the test suite2. Build the production bundle3. Deploy to the staging environment4. Verify the deployment health check
```

You can also organize skills into subfolders, like `.cursor/skills/shipping/deploy-staging/SKILL.md`. Cursor walks the skills root recursively, so category folders work for grouping related skills. The skill's name comes from the folder that contains `SKILL.md`, not the category above it.

Skills are automatically loaded from `.agents/skills/`, `.cursor/skills/`, `~/.agents/skills/` (global on the local machine), and `~/.cursor/skills/` (global on the local machine), including nested project subdirectories such as `apps/web/.cursor/skills/` in a monorepo. Skills in a nested project directory are automatically scoped to files inside that directory. For example, skills under `apps/web/.cursor/skills/` are only surfaced when the agent works with files in `apps/web/`, similar to the [`paths` frontmatter field](#how-do-i-scope-a-skill-to-specific-files). For compatibility, Cursor also loads skills from Claude and Codex directories: `.claude/skills/`, `.codex/skills/`, `~/.claude/skills/`, and `~/.codex/skills/`.

## [How do I use personal skills with Cloud Agents?](#how-do-i-use-personal-skills-with-cloud-agents)

Personal skills in `~/.cursor/skills/` stay on your machine until you sync them. Open **Settings → Agents**, then turn on **Sync Skills for Cloud Agents** under **Context and Tools**. Synced skills stay private to you.

Only `~/.cursor/skills/` syncs. To share a skill with teammates, [publish it to your team marketplace](/docs/plugins#publish-a-skill-to-your-team).

On Teams and Enterprise, admins can turn sync off for everyone from [Team Settings](https://cursor.com/dashboard/team-settings) under **Security & Identity**. See [Use personal skills with Cloud Agents](/docs/skills#use-personal-skills-with-cloud-agents).

## [How do I publish a skill to my team?](#how-do-i-publish-a-skill-to-my-team)

Open **Customize → Skills**, open a personal skill, and choose **Publish**. Cursor adds it to your team's Default marketplace. Teammates install it from there. See [Publish a skill to your team](/docs/plugins#publish-a-skill-to-your-team).

## [Are user-level skills available on Cloud Agents and remote workers?](#are-user-level-skills-available-on-cloud-agents-and-remote-workers)

You can [sync](#how-do-i-use-personal-skills-with-cloud-agents) `~/.cursor/skills/` so Cloud Agents can use those skills. `~/.agents/skills/` and unsynced local skills stay on your machine. They are not copied to Cloud Agents, Agents Window remote SSH, or [self-hosted workers](/docs/cloud-agent/self-hosted/pool). On self-hosted workers, use project skills in the repo or bake skills into the worker image.

## [How do I scope a skill to specific files?](#how-do-i-scope-a-skill-to-specific-files)

Add a `paths` field to the skill's frontmatter. The skill only applies when the agent works with files that match:

```
---name: react-component-patternsdescription: Conventions for writing React components.paths: - "**/*.tsx" --- # React component patterns ...
```

`paths` accepts a list or a comma-separated string of glob patterns. Leave it unset for a skill that should be available regardless of which files are open. See the [Skills reference](/docs/skills) for the full frontmatter schema.

## [How do I use a skill?](#how-do-i-use-a-skill)

Type `/` followed by the skill name in chat to run it (e.g., `/write-tests`), or type `@` and select it to attach as context. Agent reads the skill file and follows the instructions.

## [When should I use skills instead of rules?](#when-should-i-use-skills-instead-of-rules)

| Rules | Skills |
| --- | --- |
| **Purpose** | Short coding guidelines and constraints | Multi-step workflows and procedures |
| **Length** | A few lines to a few hundred lines | Often longer, with detailed step-by-step instructions |
| **How applied** | Included as context in every (or matching) conversation | Invoked on demand with `/skill-name` or `@skill-name` |
| **Example** | "Use TypeScript for all new files" | "Deploy to staging: run tests, build, deploy, verify" |

Use a rule when a short instruction is enough. Use a skill when Agent needs a detailed, repeatable process to follow.

## [How do I convert a rule into a skill?](#how-do-i-convert-a-rule-into-a-skill)

1. Type `/create-skill` in chat and tell Agent to turn your rule into a new skill (e.g., "turn `@my-rule` into a skill")
2. Review the generated skill file
3. Delete the original rule file if you no longer need it

## [How do I migrate commands to skills?](#how-do-i-migrate-commands-to-skills)

Type `/migrate-to-skills` in Agent chat. This built-in skill (available in Cursor 2.4+) identifies eligible rules and commands and converts them to skills automatically.

It converts:

* **Dynamic rules**: Rules with `alwaysApply: false` (or undefined) and no `globs` patterns.
* **Slash commands**: Both user-level and workspace-level commands, preserving their explicit invocation behavior.

Rules with `alwaysApply: true` or specific `globs` patterns are not migrated, as they have explicit triggering conditions that differ from skill behavior. User rules are also not migrated since they are not stored on the file system.

## [Related](#related)

* [Skills reference](/docs/skills)
* [Publish a skill to your team](/docs/plugins#publish-a-skill-to-your-team)
* [Rules](/help/customization/rules)
* [Self-Hosted Machines](/docs/cloud-agent/self-hosted/choose-runtime)

## Was this article helpful?
