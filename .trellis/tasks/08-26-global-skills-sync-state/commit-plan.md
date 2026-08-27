# 提交计划

## 工作提交

1. `fix: 修复全局 Skills 首次分配与同步状态`
   - `.trellis/spec/backend/skill-import-guidelines.md`
   - `.trellis/tasks/08-26-global-skills-sync-state/check.jsonl`
   - `.trellis/tasks/08-26-global-skills-sync-state/commit-plan.md`
   - `.trellis/tasks/08-26-global-skills-sync-state/design.md`
   - `.trellis/tasks/08-26-global-skills-sync-state/implement.jsonl`
   - `.trellis/tasks/08-26-global-skills-sync-state/implement.md`
   - `.trellis/tasks/08-26-global-skills-sync-state/prd.md`
   - `.trellis/tasks/08-26-global-skills-sync-state/research/bug-analysis.md`
   - `.trellis/tasks/08-26-global-skills-sync-state/research/root-cause.md`
   - `.trellis/tasks/08-26-global-skills-sync-state/task.json`
   - `.trellis/tasks/08-26-global-skills-sync-state/verification.md`
   - `src-tauri/src/skills/service.rs`
   - `src/features/skills/skills-page.test.tsx`
   - `src/features/skills/skills-page.tsx`
   - `src/lib/global-target-status-ui.ts`

## 未识别脏文件

无。当前全部脏文件均由本任务产生。

## 后续边界

- 本提交不 amend、不 push。
- 工作提交完成后再运行 `/trellis:finish-work`，由归档与会话记录分别产生后续 bookkeeping 提交。
