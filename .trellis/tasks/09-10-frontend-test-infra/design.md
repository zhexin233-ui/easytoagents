# 技术设计

## render helper
```ts
export function renderWithProviders(ui: ReactNode, options?: { route?: string; initialEntries?: string[]; queryClient?: QueryClient; path?: string })
```
内部 `new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } })`；用 `MemoryRouter` + `Routes` 包一层以支持 `useParams` 的页面（`path` 参数）；返回 `{ ...renderResult, queryClient, user: userEvent.setup() }`（若项目已有 `@testing-library/user-event`，否则沿用 `fireEvent`，不新增依赖）。

## commands mock
```ts
vi.mock("@/bindings/commands", async (importOriginal) => { const actual = await importOriginal<typeof import("@/bindings/commands")>(); return { ...actual, commands: mockCommands(actual.commands) }; });
```
`mockCommands` 返回 `Record<keyof typeof commands, Mock>`；测试通过 `vi.mocked(commands.listSkills).mockResolvedValue(okResult([...]))`。`okResult/errResult` 类型直接取自绑定的 `Result<T, AppError>` 联合，不引入断言。

## fixtures
`makePreviewPlan` 返回完整合法 `PreviewPlan`，默认一个无警告的 `create` 目标；`makeTarget` 生成 `PreviewTargetPlan` 含完整 `descriptor`。DTO 工厂对每个字段给出与后端一致的默认值，来源以 `bindings/commands.ts` 类型为准。

## 迁移与拆分
逐文件迁移并立即运行该文件测试；拆分 `skills-page.test.tsx` 时按 `describe` 块移动，共享 setup 放到同目录 `skills-page.test-helpers.ts`（非测试文件，不被 vitest 收集）。

## 回滚
helper 新增与文件迁移分开提交；拆分每个大文件单独提交。
