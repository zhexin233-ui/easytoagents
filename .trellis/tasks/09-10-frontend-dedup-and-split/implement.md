# 实施计划

- [x] 加载前端规范（component、hook、state、quality、type-safety）与代码复用指南；确认缺陷修复与测试基建任务已归档。
- [x] R3：`components/ui/dialog.tsx`，迁移全部 dialog；遮罩统一；测试。
- [x] R4：`Field`、`ToolIconToggle`，删除三份副本。
- [x] R5：`NotifyProvider` 与堆叠视口；删除页面内 `<Notify>`；测试 helper 接入。
- [x] R1：`useSyncPreviewFlow`、`useImportDialogState`、`useSubmitGuard`；五页 + 项目详情迁移；删除重复类型。
- [x] R2：项目详情目录化、`ProjectAssignmentsSection`、`invalidateProjectScope`。
- [x] R6：懒加载、`useQueries`、稳定引用、表单组件拆分、`useMemo`。
- [x] R7：主题 token 与 `toneClass`；全量替换手写状态色。
- [x] R8：后端能力常量导出、`pnpm bindings:generate`、前端派生与一致性测试。
- [x] `pnpm bindings:check`、`pnpm check`。
- [x] `pnpm build`、`git diff --check`；最终自动化检查通过（32 个测试文件、282 项前端测试；Rust 318 passed、2 ignored）。
- [ ] 真实 Tauri 应用走查五页。
- [x] 更新 `.trellis/spec/frontend/{component-guidelines,hook-guidelines,state-management,quality-guidelines}.md`；记录共享预览 Hook、通知队列、项目失效集合和后端能力常量合同。

## 未完成 / 外部验证

- 真实 Tauri 应用五页面导入、分配、预览、应用走查需要隔离 fixture 与可用桌面窗口；本次仅完成自动化验证，未将其冒充为手工通过。
- 项目原生资源因请求以 `resourceId + rowVersion + action` 为主键，保留专用 `nativePreview` mutation；它遵守共享预览对话框、精确 preview ID、禁止 direct auto-apply 和写后失效合同，详见前端质量规范。

## 风险文件
`src/features/**/*-page.tsx`、`src/features/projects/**`、`src/components/**`、`src/app/{router,app-shell}.tsx`、`src/lib/{tool-metadata,projects-api}.ts`、`src/styles.css`、`src-tauri/src/domain/mod.rs`、`src-tauri/src/lib.rs`。

## 回滚点
八段独立提交，见 design.md。
