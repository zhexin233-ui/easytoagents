# 实现计划

1. 更新 `ProjectPromptAssignments` 的 assignment mutation：在查询失效完成后按 `directApply` 分支，直接触发提示词预览；默认模式保留现有提示。
2. 扩展 `src/features/projects/project-detail-page.test.tsx`：
   - 直接模式下提示词分配自动预览并 Apply；
   - 直接模式下 Cursor/另一工具使用正确的 tool 参数（必要时补充工具切换 fixture）；
   - 冲突计划打开对话框且不自动 Apply；
   - 默认模式测试继续断言无隐式预览。
3. 运行定向前端测试与 lint/type-check；如失败只修复本变更引入的问题。
4. 运行项目质量检查，确认未触碰后端目标、硬拷贝或软链接契约。
5. 更新相关 Trellis spec 的 direct-apply 项目提示词条款（若现有条款未明确项目 Prompt assignment），记录“项目提示词分配也遵循 direct mode 自动预览/Apply，冲突仍回退对话框”。

## 验证命令

- `pnpm exec vitest run src/features/projects/project-detail-page.test.tsx`
- `pnpm lint`
- `pnpm typecheck`（若 package scripts 提供）
- `pnpm check`

## 风险与回滚点

- 风险集中在异步 invalidation 与 preview mutation 的竞态；测试需等待精确 RPC 调用并验证顺序语义。
- 若自动预览出现旧分配，回滚 assignment `onSuccess` 的 direct 分支，不动后端。
