# 实施计划

- [ ] 加载前端规范（component、hook、state、quality、type-safety）与代码复用指南；确认缺陷修复与测试基建任务已归档。
- [ ] R3：`components/ui/dialog.tsx`，迁移全部 dialog；遮罩统一；测试。
- [ ] R4：`Field`、`ToolIconToggle`，删除三份副本。
- [ ] R5：`NotifyProvider` 与堆叠视口；删除页面内 `<Notify>`；测试 helper 接入。
- [ ] R1：`useSyncPreviewFlow`、`useImportDialogState`、`useSubmitGuard`；五页 + 项目详情迁移；删除重复类型。
- [ ] R2：项目详情目录化、`ProjectAssignmentsSection`、`invalidateProjectScope`。
- [ ] R6：懒加载、`useQueries`、稳定引用、表单组件拆分、`useMemo`。
- [ ] R7：主题 token 与 `toneClass`；全量替换手写状态色。
- [ ] R8：后端能力常量导出、`pnpm bindings:generate`、前端派生与一致性测试。
- [ ] `pnpm bindings:check`、`pnpm check`；真实 Tauri 应用走查五页。
- [ ] 更新 `.trellis/spec/frontend/{component-guidelines,hook-guidelines,state-management,quality-guidelines}.md`。

## 风险文件
`src/features/**/*-page.tsx`、`src/features/projects/**`、`src/components/**`、`src/app/{router,app-shell}.tsx`、`src/lib/{tool-metadata,projects-api}.ts`、`src/styles.css`、`src-tauri/src/domain/mod.rs`、`src-tauri/src/lib.rs`。

## 回滚点
八段独立提交，见 design.md。
