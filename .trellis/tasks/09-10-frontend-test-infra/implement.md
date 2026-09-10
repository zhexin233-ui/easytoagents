# 实施计划

- [ ] 加载前端规范（quality、hook、type-safety）。
- [ ] 建 `src/test/render.tsx`、`src/test/commands-mock.ts`、`src/test/fixtures/*`，完善 `setup.ts`。
- [ ] 逐文件迁移 13 个测试到 helper，每迁移一个跑一次该文件测试。
- [ ] 拆分 `skills-page.test.tsx`、`mcp-page.test.tsx`、`project-detail-page.test.tsx`。
- [ ] 校验用例数、行数与耗时；`pnpm check`。
- [ ] 更新 `.trellis/spec/frontend/quality-guidelines.md` 的测试要求节，指向新 helper。

## 风险文件
`src/test/**`、`src/**/*.test.tsx`。

## 回滚点
helper、迁移、拆分三段提交。
