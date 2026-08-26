# 已确认提交计划

用户于 2026-08-26 明确要求“提交并推送到 git”。按以下清单执行一次工作提交，随后独立归档、记录会话，并推送到 `origin/main`；不 amend、不强制推送。

## 1. `fix: 修复全局 Skills 检测导入与初始状态提示`

这是一个跨层功能修复：代码、生成绑定、回归测试、数据库迁移及对应规范共同提交。
内置技能及其跨工具别名排除；只复制中央库，不接管或改动原安装。
质量检查和隔离浏览器验收见 `verification.md`。

### 后端与迁移

- `src-tauri/src/app/mod.rs`
- `src-tauri/src/commands/skills.rs`
- `src-tauri/src/db/migrations/0006_skill_import_previews.sql`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/skill_imports.rs`
- `src-tauri/src/db/skills.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/skills/import.rs`
- `src-tauri/src/skills/library.rs`
- `src-tauri/src/skills/mod.rs`
- `src-tauri/src/skills/models.rs`
- `src-tauri/src/skills/service.rs`

### 前端与生成绑定

- `src/bindings/commands.ts`
- `src/features/mcp/mcp-page.test.tsx`
- `src/features/skills/skill-import-dialog.tsx`
- `src/features/skills/skills-page.test.tsx`
- `src/features/skills/skills-page.tsx`
- `src/lib/global-target-status-ui.ts`
- `src/lib/skills-api.ts`

### 项目规范

- `.trellis/spec/backend/index.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/backend/skill-import-guidelines.md`
- `.trellis/spec/frontend/index.md`

### 本任务规划、研究与验收材料

- `.trellis/tasks/08-26-skills-global-import/check.jsonl`
- `.trellis/tasks/08-26-skills-global-import/commit-plan.md`
- `.trellis/tasks/08-26-skills-global-import/design.md`
- `.trellis/tasks/08-26-skills-global-import/implement.jsonl`
- `.trellis/tasks/08-26-skills-global-import/implement.md`
- `.trellis/tasks/08-26-skills-global-import/prd.md`
- `.trellis/tasks/08-26-skills-global-import/research/backend-guidelines-context.md`
- `.trellis/tasks/08-26-skills-global-import/research/backend-implementation-report.md`
- `.trellis/tasks/08-26-skills-global-import/research/implementation-review.md`
- `.trellis/tasks/08-26-skills-global-import/research/import-safety.md`
- `.trellis/tasks/08-26-skills-global-import/research/planning-contract-review.md`
- `.trellis/tasks/08-26-skills-global-import/research/planning-safety-review.md`
- `.trellis/tasks/08-26-skills-global-import/research/source-layout.md`
- `.trellis/tasks/08-26-skills-global-import/research/status-and-ui.md`
- `.trellis/tasks/08-26-skills-global-import/research/ui-partial-source.png`
- `.trellis/tasks/08-26-skills-global-import/research/ui-stale.png`
- `.trellis/tasks/08-26-skills-global-import/task.json`
- `.trellis/tasks/08-26-skills-global-import/verification.md`

## 未识别变更

无。以上 41 个文件均属于本任务；临时浏览器 fixture 与 dist 构建产物不纳入提交。

## 工作提交之后

按 Trellis 流程再分别执行任务归档和会话记录；这两步不混入本工作提交。工作提交哈希在后续归档材料与会话中记录。

## 工作提交记录

`0f28c561d6e59b55f17b9a673e846e0c06e3f7c5`：已按以上 41 个文件提交；后续归档和会话记录独立提交。
