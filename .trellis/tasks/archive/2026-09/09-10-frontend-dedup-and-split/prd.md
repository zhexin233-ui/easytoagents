# 前端去重与页面拆分

父任务：`09-10-codebase-optimization`。覆盖审阅条目 C7、D7、E8、E9、E10、E11、E12、E13。

## 目标
把五个中央页面与项目详情页共用的预览应用流程、对话框壳、表单字段与工具切换按钮各收敛为一份；拆分项目详情页；通知提升为应用级；路由懒加载；状态色进入主题 token；工具元数据由后端单一来源导出。用户可见行为与 RPC 合同不变。

## 需求
- R1 / E8：新增 `src/features/sync/use-sync-preview-flow.ts`：`useSyncPreviewFlow({ artifactKind, preview, apply, readopt?, invalidateKeys, messages })` 返回 `{ openPreview, requestPreview(tool, autoApply), applyMutation, readoptMutation, closePreview }`；`useImportDialogState()` 管理 `{tool, requestId}` 与 rescan；`useSubmitGuard()` 替代 9 处 `saveInFlight` ref。五个中央页面与项目详情页全部改用；`OpenXxxPreview`、`XxxPreviewRequest/ApplyRequest` 只剩公共定义。`prompts` 页 apply 后刷新的行为由公共 hook 保证。
- R2 / E9：`project-detail-page.tsx` 拆为 `features/projects/detail/{page,native-resources,assignments/{mcp,hook,skill},assignment-card,option-row}.tsx`；三个 Assignments 组件共用一个泛型 `ProjectAssignmentsSection`；`src/lib/projects-api.ts` 提供 `invalidateProjectScope(queryClient, kinds)` 统一决定失效集合，五处手写 `Promise.all` 删除。
- R3 / E10：新增 `src/components/ui/dialog.tsx`（`DialogOverlay/DialogContent/DialogHeader/DialogFooter`，封装 `useDialogFocus`、`useId` aria 关联、Escape 关闭）；`FormDialog`、`ChangePreviewDialog`、`SettingsDialog`、四个 import/picker dialog、`skills-page.tsx` 两个内联对话框全部改用；遮罩色统一。
- R4 / E11：`components/ui/field.tsx` 与 `components/tool-icon-toggle.tsx`；三份 `Field` 与三份图标切换按钮删除，`PlatformAssignmentButton` 变为薄包装。
- R5 / D7：`NotifyProvider` 挂在 `AppShell`，`useNotify()` 从 context 取；支持多条堆叠与自动消失；页面内 `<Notify>` 渲染删除。
- R6 / C7：`router.tsx` 全部页面 `lazy` + 路由级 `Suspense`（`role="status"` 占位）；`OnboardingWizard`、`SnapshotRestoreDialog` 在 open 时懒挂载；`prompts-page` 改用 `useQueries` 只探测启用工具；`useEnabledTools` 返回稳定引用；`McpFormDialog`/`HookFormDialog` 拆为自持 state 的独立组件；hooks 页事件分组用 `useMemo`。
- R7 / E12：`styles.css @theme` 增加 `destructive/warning/success/info` 及 `-foreground` token 并在 `.dark` 覆盖；`toneClass(tone)` 共享；`text-red-700 dark:text-red-300` 等手写组合替换为 token 类。
- R8 / E13：后端通过 specta 导出 `TOOL_CAPABILITIES` 与 `HOOK_EVENT_SUPPORT` 常量（或一个 `list_tool_capabilities` 命令）；前端 `PROFILE_TOOLS/MCP_TOOLS/SKILL_TOOLS/HOOK_TOOLS` 由该来源派生，`hook-events.ts` 的手抄矩阵删除；`tool-metadata.test.ts` 改为前后端一致性断言。

## 验收条件
- A1：`rg "previewMutation = useMutation" src/features` 只命中公共 hook；`rg "saveInFlight" src` 为零。
- A2：`src/features/projects/detail/page.tsx` 不超过 500 行；`rg "Promise.all\(\[" src/features/projects` 为零。
- A3：`rg 'role="dialog"' src --glob '!*.test.*'` 只命中 `components/ui/dialog.tsx`；`rg "bg-black/40|bg-slate-950/40" src` 只命中一处。
- A4：`rg "function Field\(" src` 一处；`rg "opacity-25 grayscale" src` 一处。
- A5：`rg "<Notify" src/features` 为零；通知堆叠有测试。
- A6：`rg "lazy\(" src/app/router.tsx` 覆盖全部页面；构建产物出现按页 chunk。
- A7：`rg "dark:text-red-300|dark:text-amber-300" src` 为零。
- A8：`rg "hookEventSupportedByTool" src/features/hooks/hook-events.ts` 的实现来自绑定常量；`bindings:check` 通过。
- A9：所有页面测试通过且用例数不减少；`pnpm check` 全绿；真实 Tauri 应用走通五个页面的导入、分配、预览、应用。

## 范围外
新功能、后端行为改动。必须在 `09-10-fix-frontend-review-defects` 与 `09-10-frontend-test-infra` 之后执行。
