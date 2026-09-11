# 实施计划

- [x] 加载前端规范（quality、hook、type-safety）。
- [x] 建 `src/test/render.tsx`、`src/test/commands-mock.ts`、`src/test/fixtures/*`，完善 `setup.ts`。
- [x] 逐文件迁移 13 个测试到 helper，每迁移一个跑一次该文件测试。
- [x] 拆分 `skills-page.test.tsx`、`mcp-page.test.tsx`、`project-detail-page.test.tsx`，并将超 900 行的 Prompt/Provider 测试按场景拆分。
- [x] 校验用例数、行数与耗时；`pnpm check` 通过（32 文件、280 用例，前端约 7 秒）。
- [x] 更新 `.trellis/spec/frontend/quality-guidelines.md` 的测试要求节，指向新 helper。

## 风险文件
`src/test/**`、`src/**/*.test.tsx`。

## 回滚点
helper、迁移、拆分三段提交。
