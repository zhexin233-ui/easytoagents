# 前端测试基建

父任务：`09-10-codebase-optimization`。覆盖审阅条目 E14。

## 目标
为前端测试提供统一的渲染、命令 mock 与夹具 helper，消除 13 个测试文件的复制脚手架，并把超大测试文件按场景拆分，为随后的前端重构提供安全网。

## 需求
- R1：新增 `src/test/render.tsx`，导出 `renderWithProviders(ui, { route?, initialEntries?, queryClient? })`，内部创建带 `retry: false`（queries 与 mutations）的 `QueryClient`、`MemoryRouter` 与应用 Provider；返回 `queryClient` 便于断言失效。
- R2：新增 `src/test/commands-mock.ts`，提供 `mockCommands()` 工厂：基于 `Object.keys(actual.commands)` 自动为每个命令生成 `vi.fn()`，新增命令时无需手改列表；提供 `okResult(value)` / `errResult(error)` 构造生成绑定的 `Result` 联合；`eslint-disable unbound-method` 只出现在该文件一处。
- R3：新增 `src/test/fixtures/`：`preview-plan.ts`（`makePreviewPlan(overrides)`、`makeTarget(overrides)`）、`dtos.ts`（各列表 DTO 的最小合法对象工厂）；所有现有内联 `PreviewPlan` 夹具改用工厂。
- R4：13 个测试文件全部迁移到上述 helper；`src/test/setup.ts` 集中注册 jest-dom 与通用 `afterEach(cleanup)`。
- R5：`skills-page.test.tsx` 按"列表与分配 / 目录导入 / GitHub 导入 / 接管 / 同步预览"拆为 4-5 个文件，每个不超过 700 行；`mcp-page.test.tsx`、`project-detail-page.test.tsx` 同理拆分到不超过 900 行。
- R6：测试用例数与断言覆盖不减少；测试总时长不显著上升（基线约 6 秒）。

## 验收条件
- A1：`rg "new QueryClient" src --glob '*.test.*'` 为零。
- A2：`rg "eslint-disable.*unbound-method" src` 只命中 `src/test/commands-mock.ts`。
- A3：`rg "vi.mock\(\"@/bindings/commands\"" src` 的每处都使用 `mockCommands()` 工厂，不再手写命令列表。
- A4：`wc -l` 校验任一测试文件不超过 900 行。
- A5：`pnpm test --run` 用例数不少于 268；`pnpm check` 全绿。

## 范围外
产品代码改动。若某个测试因迁移暴露出产品缺陷，记录到父任务研究文件并由对应子任务处理。
