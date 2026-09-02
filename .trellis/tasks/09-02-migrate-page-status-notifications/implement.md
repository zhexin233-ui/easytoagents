# 实施计划：三个中央页面统一操作通知

## 启动门禁

- 用户批准本规划后才运行 `python3 ./.trellis/scripts/task.py start 09-02-migrate-page-status-notifications`。
- 实施前为当前独立 worktree 建立 `codex/` 前缀分支，不切换或接管其他 Trellis 任务。

## 有序清单

1. **加载上下文**
   - 读取 `prd.md`、`design.md`、本文档、`research/notification-audit.md` 和 `research/test-audit.md`。
   - 通过 Trellis implement/check 上下文载入前端组件、质量与 notify 契约。
2. **MCP 页面**
   - 把所有审计确认的成功/无结果反馈移到 success notify，包含已知重新接管文案。
   - 把页面级 mutation 失败移到 error notify，保留 form/import/query 上下文错误。
   - 删除无用 `message`/derived error/inline DOM；将 preview 本地元数据改为 `autoApply`，Apply 不再传通知布尔值。
3. **Skills 页面**
   - 迁移 delete/assignment/empty-preview/apply/import/takeover 成功反馈。
   - 迁移 content-preview/delete/assignment/preview/apply 失败反馈，保留对话框内错误。
   - 删除无用状态与 inline DOM，保持 takeover 无条件打开预览。
4. **Prompts 页面**
   - 迁移 save/assignment/apply/delete/discover-no-result/import 成功反馈。
   - 迁移 assignment/delete/discover/import/preview/apply 页面级失败，保留表单/query/持久诊断。
   - 删除 `notice`/`applyMessage`/derived error/inline DOM。
5. **测试**
   - 更新直接绑定旧内联文本的断言，改为通过可访问 role 查询 success/error notify。
   - 覆盖已知 MCP readopt 动态计数、三页手动 Apply、零结果、导入/接管以及代表性操作失败；断言同一文案只出现一次。
   - 保留表单/导入对话框错误、精确 preview ID/RPC payload、direct/preview-confirm 分支和 takeover 强制预览断言。
6. **规范与全量检查**
   - 实施完成并评审后，更新 `.trellis/spec/frontend/quality-guidelines.md`：把 notify 范围从 direct global-sync 扩大为三个中央页面的短时操作结果，记录上下文错误保留和 `autoApply` 页面元数据契约。
   - 运行 Trellis 全范围 check，确认不包含 `center-list-three-column-edit-actions` 或其他任务文件。

## 验证命令

```bash
pnpm test --run src/features/mcp/mcp-page.test.tsx src/features/skills/skills-page.test.tsx src/features/prompts/prompts-page.test.tsx
pnpm typecheck
pnpm lint
pnpm check
```

## 风险文件与回滚点

- `src/features/mcp/mcp-page.tsx`：readopt → invalidate → preview 顺序，以及 direct auto-Apply 循环。
- `src/features/skills/skills-page.tsx`：takeover 必须继续强制显式预览；导入刷新失败不得被虚假成功通知覆盖。
- `src/features/prompts/prompts-page.tsx`：表单保存错误必须继续保留输入并留在对话框。
- 如单页迁移引入回归，按该页面及对应测试为最小回滚单元；不需要回滚共享 notify 或后端数据。

