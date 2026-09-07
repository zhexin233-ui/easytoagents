# Current Documentation Audit

Date: 2026-09-07

The audit covers the current documentation surfaces changed by this task:

- `README.md`
- `src/assets/brand/README.md` (checked; no matching residuals)
- `docs/maintainers/adding-tool-adapter.md`
- `.trellis/spec/backend/database-guidelines.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/frontend/quality-guidelines.md`

The search used:

```bash
rg -n -i \
  'project.?prompt|prompt.?project|项目.?prompt|项目级提示词|项目提示词|promptfile|prompt_file|project.?rules|项目级 rules' \
  README.md src/assets/brand/README.md docs/maintainers/adding-tool-adapter.md \
  .trellis/spec/backend/database-guidelines.md \
  .trellis/spec/backend/quality-guidelines.md \
  .trellis/spec/frontend/quality-guidelines.md
```

All remaining matches in these files are intentional negative or compatibility
statements: Prompt is global-only, project Prompt/Rules files are not observed or
written, and v18 rejects/cleans historical project Prompt state. No current
capability matrix, UI contract, project-resource contract, or adapter contract
claims project Prompt assignment, PromptFile management, or project Prompt
preview/apply support.

Allowed repository-wide exceptions are:

1. Immutable migrations 0001–0017 and their migration names/comments, which
   document the historical schema that v18 upgrades from.
2. Migration 0018 and its upgrade tests, which must name the retired project
   Prompt rows and prove that project files are not touched.
3. Archived Trellis tasks, which are historical records and are not current
   product documentation.
4. Global Prompt concepts and tests (`ArtifactKind::Prompt`, prompt profiles,
   global targets/import/preview/apply), which remain supported by design.
