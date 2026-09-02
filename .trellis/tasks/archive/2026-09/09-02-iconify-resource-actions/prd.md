# 提示词 MCP Skills 页面操作按钮图标化

## Goal

将提示词、MCP、Skills 三个中央库页面中的高频行级操作改为紧凑的图标按钮，减少卡片操作区占用，同时保留清晰的键盘与辅助技术语义。

## Confirmed Facts

- 提示词页面的档案行操作是“编辑”和“删除”；工具启用/停用已经使用 `PlatformAssignmentButton` 图标按钮。
- MCP 页面每行有“编辑”、动态“启用/停用”和“删除”三个文字按钮。
- Skills 页面每行有“内容预览”和“移出中央库”两个文字按钮。
- `components.json` 已将图标库配置为 Lucide；当前尚未安装 `lucide-react` 运行时依赖。图标按钮约定是 `Button` + `size="sm"` + `size-8 p-0`，并同时提供 `aria-label` 与 `title`。
- 相关页面和行为测试当前通过按钮 accessible name（例如“编辑”“停用”“内容预览”）定位操作。

## Requirements

1. 在提示词中央列表中，将每条档案的“编辑”和“删除”改为仅显示图标的按钮，点击行为、确认提示和 mutation payload 保持不变。
2. 在 MCP 中央列表中，将每条服务器的“编辑”、动态“启用/停用”和“删除”改为仅显示图标的按钮；启用状态切换仍使用原有动态文案和 mutation，不能改变业务语义。
3. 在 Skills 中央列表中，将每条 Skill 的“内容预览”和“移出中央库”改为仅显示图标的按钮；读取/移出进行中仍保留原有 disabled 行为与可感知状态。
4. 所有新增图标按钮必须保留原操作的可查询 accessible name，并设置对应 `title`；启用/停用切换额外暴露当前 `aria-pressed` 状态。使用 `lucide-react` 图标并通过图标组件的 `aria-hidden` 语义隐藏装饰图标，不能依赖颜色作为唯一状态提示。
5. 三个页面保持列表/网格两种布局下的操作位置、禁用条件、焦点行为和深色主题样式，仅引入已批准的 `lucide-react` 图标依赖，不改变任何后端接口。
6. 更新受影响的前端测试，覆盖图标按钮的 accessible name/title 及原有点击行为；现有业务流程测试继续通过。

## Acceptance Criteria

- [x] 提示词每个档案卡片都以图标按钮呈现“编辑”“删除”，点击后分别打开编辑弹窗和执行原有删除确认。
- [x] MCP 每个服务器卡片都以图标按钮呈现“编辑”“启用/停用”“删除”，启用状态和进行中禁用行为与改动前一致。
- [x] Skills 每个 Skill 卡片都以图标按钮呈现“内容预览”“移出中央库”，内容预览弹窗、移出 mutation 及进行中状态与改动前一致。
- [x] 每个图标按钮具备可查询的 `aria-label`、`title`，Lucide 图标对辅助技术隐藏；列表与网格布局均满足该条件。
- [x] `pnpm format:check`、`pnpm lint`、`pnpm typecheck` 和相关 Vitest 测试通过。

## Out of Scope

- 不修改新增/编辑弹窗中的提交按钮、导入按钮、全局同步按钮或项目详情页操作。
- 不改变中央库 CRUD、同步预览、Apply、缓存失效或后端命令行为。
- 不重绘已有 Claude/Codex/Cursor 品牌图标；不改变品牌图标资产。

## Key Decisions

采用 `lucide-react` 的 `Pencil`、`Trash2`、`Power`/`PowerOff`、`Eye`、`FolderMinus` 等通用图标，现有按钮 accessible name 作为兼容标签保留。
