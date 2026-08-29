# Implement: 布局外壳重构

## Steps

1. [ ] 重写 `src/app/app-shell.tsx`：TopBar（右上角 Claude/Codex）+ Sidebar（总览/MCP/Skills/项目 + 项目子项）+ 滚动内容区。
2. [ ] 页面容器统一：dashboard / projects / project-detail / tool-profiles / mcp / skills 的 `<main>` 去掉 `min-h-screen`、统一 padding。
3. [ ] `styles.css` 按需微调（不做主题重构）。
4. [ ] 自测：`pnpm lint`、`pnpm typecheck`、`pnpm test --run`、`pnpm format:check`。
5. [ ] 视觉验证：`pnpm dev` + 截图确认布局符合 PRD。
6. [ ] 质量检查 + 提交。

## Validation Commands

```bash
pnpm lint
pnpm typecheck
pnpm test --run
pnpm format:check
```

## Rollback

- revert 单个 commit。
