# 集中管理项目 MCP 与 Skill — 实施计划

## 实施步骤

1. 精简 `src/features/mcp/mcp-page.tsx`：移除项目级查询、状态、mutation、UI 与错误聚合，保留并收敛全局 preview/apply。
2. 精简 `src/features/skills/skills-page.tsx`：执行同构调整，保留 Skill 内容预览、导入、全局分配和全局同步。
3. 更新 `src/features/projects/project-detail-page.tsx`：加入 MCP/Skill accessible 切换控件，并按当前视图在 Claude/Codex 两列中渲染对应管理卡。
4. 更新 MCP、Skills、项目详情页面测试：删除中央页项目入口断言，补充入口不存在、默认 MCP、双向切换、Skill assignment/preview/apply，以及既有 MCP 流程回归断言。
5. 运行格式化、lint、类型检查与相关 Vitest；再执行完整前端测试。

## 验证命令

- `pnpm exec prettier --check src/features/mcp/mcp-page.tsx src/features/skills/skills-page.tsx src/features/projects/project-detail-page.tsx src/features/mcp/mcp-page.test.tsx src/features/skills/skills-page.test.tsx src/features/projects/project-detail-page.test.tsx`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm test -- src/features/mcp/mcp-page.test.tsx src/features/skills/skills-page.test.tsx src/features/projects/project-detail-page.test.tsx`
- `pnpm test`

## 风险与复核点

- 删除中央页项目逻辑时不能误删全局 preview/apply 所需的 mutation、类型或状态。
- 项目 assignment 后必须继续联合刷新 project/MCP/Skill key family，避免共享项目行版本冲突。
- 切换控件必须可被 role/name 查询，并准确维护 `aria-pressed`。
- MCP 与 Skill 的零目标、信任/策略阻断、Git exclude 与显式 Apply 行为不得退化。
