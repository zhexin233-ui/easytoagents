# 技术设计

## 顺序
R3 Dialog 与 R4 小组件（纯 UI 原语）→ R5 通知 context → R1 预览流程 hook → R2 项目详情拆分 → R6 性能 → R7 主题 token → R8 元数据单一来源（需后端改动与绑定重生成）。

## useSyncPreviewFlow
```ts
interface SyncPreviewFlowOptions<TReadopt = never> {
  artifactKind: ArtifactKind;
  preview: (tool: Tool) => Promise<Result<PreviewPlan, AppError>>;
  apply: (input: { previewId: string; tool: Tool }) => Promise<Result<ApplyResult, AppError>>;
  readopt?: (tool: Tool) => Promise<Result<TReadopt, AppError>>;
  invalidate: () => Promise<void>;
  messages: { previewFailed: string; applyFailed: string; applied: (result: ApplyResult) => string };
  directApply: boolean;
}
```
内部实现现有四页共同逻辑：`requestPreview(tool, autoApply)` → 若 `directApply && autoApply && canAutoApplyPreview(plan)` 则直接 `applyMutation.mutate`，否则 `setOpenPreview({plan, tool})`；`applyMutation.onSuccess` 统一 `closePreview + invalidate + notify`。`canAutoApplyPreview` 沿用现有实现并移入该文件。项目详情页的 preview 输入多一个 `projectId`，通过闭包捕获。Skill 接管与项目原生资源禁用/恢复按规范永不自动应用，传 `directApply: false`。

## Dialog 原语
基于 `form-dialog.tsx:41-120` 抽出 `DialogOverlay`（遮罩 + 点击外部关闭可选）与 `DialogContent`（`role="dialog"`、`aria-modal`、`aria-labelledby` 自动关联 `DialogHeader` 的 `useId`、`useDialogFocus` 焦点陷阱与恢复、Escape）。`FormDialog` 改为组合这些原语。

## 通知
`NotifyProvider` 维护 `Notification[]` 队列，每条自带 id 与自动消失计时；`useNotify()` 返回 `notify`；`<NotifyViewport>` 在 `AppShell` 渲染一次，堆叠展示。测试 helper（`renderWithProviders`）包含 `NotifyProvider`。

## 项目详情拆分
`ProjectAssignmentsSection<TItem>` 接收 `{ title, items, optionsQuery, assign, preview, renderItem }`；三种资源通过配置对象实例化。`invalidateProjectScope(queryClient, kinds: ("mcp"|"skill"|"hook"|"project")[])` 按 kind 映射 key 集合，行为以当前"最全"的集合为准并在测试中固定。

## 懒加载
`const McpPage = lazy(() => import("@/features/mcp/mcp-page").then(m => ({ default: m.McpPage })))`；`AppShell` 的 `<Outlet>` 外包 `<Suspense fallback={<PageLoading />}>`。测试中直接渲染页面组件不受影响。

## 主题 token
`@theme { --color-destructive: ...; --color-destructive-foreground; --color-warning; --color-success; --color-info; }`，`.dark` 覆盖；`toneClass(tone: "neutral"|"info"|"success"|"warning"|"destructive")` 返回 `border/bg/text` 组合，`SyncStatusBadge`、`OptionTag`、`BlockingState`、`ChangePreviewDialog` 共用。

## 元数据单一来源
后端 `domain/mod.rs` 增加 `#[derive(Type, Serialize)] pub struct ToolCapabilities { ... }` 与 `pub fn tool_capabilities() -> BTreeMap<Tool, ToolCapabilities>`、`hook_event_support()`；以 specta `constant` 导出或新增 `list_tool_capabilities` 命令（选前者，避免运行时请求；若 tauri-specta 当前版本常量导出不可用则退回命令 + 启动时一次查询并缓存）。前端 `tool-metadata.ts` 保留 label/icon 等纯展示信息，能力字段全部派生。

## 风险
- 通知 context 改动影响所有页面测试，需在 test-infra 的 helper 中提前预留 Provider。
- 元数据常量导出可能改变 `commands.ts`，需核对 `bindings:check`。

## 回滚
八段按顺序独立提交。
