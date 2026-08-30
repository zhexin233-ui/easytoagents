# 项目页项目追加改为按钮式并状态标签化

## Goal

项目详情页（`src/features/projects/project-detail-page.tsx`）的 MCP / Skill
"项目追加" 选择列表目前每行使用原生 checkbox 表示追加意图，状态以纯文字
（"全局继承（只读）"、"项目追加"、"可追加"）拼接在名称后面。改为按钮式控件与
tag 式状态展示，行为保持不变。

## Requirements

1. **项目追加控件改为按钮式**
   - 不再使用勾选框（checkbox）。
   - 改为按钮：未追加时显示"启用"，点击后追加；已追加时显示"禁用"，点击后取消追加。
   - 按钮沿用页面既有 Button 组件与 `size="sm"` 风格，与 MCP 中央库列表的
     "启用/停用"按钮（`mcp-page.tsx`）一致。
   - 无障碍语义保留：可寻址名称仍为 `${名称} MCP 项目追加` / `${名称} Skill 项目追加`，
     并携带 `aria-pressed` 表达当前是否已追加。
   - 禁用规则不变：`inherited` 状态、不可选择（`selectable=false`）的 available 项、
     mutation pending 时按钮禁用。
2. **状态文字改为 tag**
   - "全局继承" 与 "只读" 不再拼在名称文字里，改为独立 tag（圆角胶囊样式，
     参考 `sync-status-badge.tsx` 的既有样式）。
   - "项目追加"、"可追加" 状态同样以 tag 展示，保持行内一致。
   - MCP 的 "已停用"（`enabled=false`）提示同样改为 tag，不再拼接文字。
   - Skill 的非 ready `status` 提示同样改为 tag。
3. **行为不变**
   - mutation 调用参数、查询失效、消息提示、预览/应用流程均不变。
   - 仅呈现层改动，不涉及后端命令。

## Acceptance Criteria

- [ ] 项目页 MCP/Skill 列表中不再出现"项目追加" checkbox；每行有文字为
      "启用"或"禁用"的按钮（inherited 行按钮禁用）。
- [ ] inherited 行展示"全局继承"与"只读"两个独立 tag；selected 行展示
      "项目追加" tag；available 行展示"可追加" tag。
- [ ] 点击按钮触发的 `setProjectMcpAssignment` / `setProjectSkillAssignment`
      调用参数与改造前完全一致（assigned 取反逻辑保持）。
- [ ] `pnpm lint`、`pnpm type-check`、相关 vitest 测试通过。

## Notes

- 仅呈现层改动：`project-detail-page.tsx` 与其测试文件。
- 不修改 bindings、后端命令与其他页面。
