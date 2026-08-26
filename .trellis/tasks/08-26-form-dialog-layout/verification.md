# 弹窗布局验证记录

## 改动

- 共用 `src/components/form-dialog.tsx`，三类新增/编辑默认隐藏，按钮触发打开。
- MCP 中央列表使用完整内容宽度；工具档案列表保留原布局，新增入口与空状态指向弹窗。
- 保存失败保留输入，关闭重置草稿与错误，保存与查询刷新期间阻止重复提交/关闭。
- 保留原命令 payload、行版本、敏感字段 keep、OAuth 限制和无隐式 Apply 合同。

## 自动化检查

2026-08-26，最终焦点修复后主代理执行：

| 命令 | 结果 |
| --- | --- |
| `pnpm test --run` | 8 个文件、62 个测试通过，exit 0 |
| `pnpm lint` | 通过，exit 0 |
| `pnpm typecheck` | 通过，exit 0 |
| `pnpm build` | 通过，exit 0 |

独立检查代理复核本任务完整改动，无阻断项；末轮再次通过 lint、typecheck、7 个前端文件格式检查、`git diff --check`，以及 5 个相关测试文件的 57 个用例（MCP、工具档案、Skills、快照恢复、引导向导）。主代理收尾另执行完整格式与差异检查。

## 隔离浏览器验证

- 通过临时 Vite 入口加载真实 `App`、样式与页面组件；命令模块完全替换为内存 fixture，保存使用延迟模拟响应。未连接 Tauri RPC，不读取/写入用户配置。
- MCP 默认没有表单；新增弹窗、模拟保存后自动关闭/刷新、关闭后恢复新增按钮焦点已实测。
- Claude/Codex 渠道与提示词的新增/编辑弹窗、当前值填充、取消/关闭/Escape、草稿隔离均已实测；截图检查三类弹窗布局。
- 默认 1280×720 及 390×700 视口下检查 MCP 长表单。窄视口弹窗宽 358、高 668，内容区域可滚动，实际 `scrollTop` 达到 235.5，底部按钮始终在视口内，无弹窗横向溢出。已恢复默认视口。
- 复现并修复保存按钮禁用导致焦点落到 `body` 的问题：提交前聚焦容器，hook 支持容器起点的双向 Tab；修复后保存中焦点保持 `SECTION[role=dialog]`，Tab 到输入框，Shift+Tab 到最后可用 checkbox，Escape 不关闭。现有 pending 测试补齐回归。
- 浏览器警告/错误日志为空。该验证不等同于真实 Tauri WebView 或真实配置写入验证；本任务未改后端，未重跑 Rust 全套。

## 收尾状态

组件规范已记录表单弹窗生命周期和禁用按钮焦点约束。用户已明确要求“提交并推送到 git”，以下清单已获授权；按工作提交、归档、会话记录的顺序完成后推送 `origin/main`。

## 已确认的工作提交

提交信息：`feat: 将新增与编辑表单改为弹窗`。

文件清单：

- `src/components/form-dialog.tsx`
- `src/components/use-dialog-focus.ts`
- `src/features/mcp/mcp-page.tsx`
- `src/features/mcp/mcp-page.test.tsx`
- `src/features/tool-profiles/provider-panel.tsx`
- `src/features/tool-profiles/prompt-panel.tsx`
- `src/features/tool-profiles/tool-profiles-page.test.tsx`
- `.trellis/spec/frontend/component-guidelines.md`
- `.trellis/tasks/08-26-form-dialog-layout/task.json`
- `.trellis/tasks/08-26-form-dialog-layout/prd.md`
- `.trellis/tasks/08-26-form-dialog-layout/implement.jsonl`
- `.trellis/tasks/08-26-form-dialog-layout/check.jsonl`
- `.trellis/tasks/08-26-form-dialog-layout/verification.md`

确认时没有未识别的工作区改动。归档与 journal 在工作提交之后按收尾流程处理，最终按用户授权推送远程；不强制推送。
