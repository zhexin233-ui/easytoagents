# 直接应用模式下隐藏全局同步手动按钮

## Goal

`applyMode === "direct"`（直接应用）时，MCP / Skills / 提示词页面不再展示
「直接应用全局同步」类手动同步按钮；原本依赖该按钮作为唯一同步入口的中央操作
（MCP 保存/删除/导入、提示词保存/删除）在该模式下自动触发同步并按既有规则应用，
保证隐藏按钮后没有任何流程变成死路。

## Background

08-30 两个任务已让直接应用模式覆盖中央分配、启停、项目追加与 Provider/提示词
激活的自动同步，但三处「全局目标状态」卡片仍保留手动同步按钮：

- MCP 页状态卡：`直接应用全局同步` / `生成全局预览`（mcp-page.tsx ~611）
- Skills 页状态卡：`直接应用全局同步` / `预览全局同步`（skills-page.tsx ~549）
- 提示词页工具卡：`直接应用 X 全局同步` / `预览 X 全局同步`（prompts-page.tsx ~566）

且 MCP 保存/删除/导入、提示词保存/删除的成功通知仍指引「点击直接应用全局同步」。
用户要求：直接应用模式下这些按钮应该隐藏——该模式下同步应全自动，手动按钮冗余。

隐藏按钮的约束：spec（direct-apply scenario）现约定 "Save/delete/import stay
manual; their success notifications still identify the required preview or sync
action"。按钮隐藏后通知指向的操作不存在，必须同时把这些流程接入自动同步，并同步
修订 spec 与相关文案。

## Requirements

- **R1 隐藏按钮（仅 direct 模式）**：三处全局同步按钮在 `directApply` 时不再渲染；
  `preview_confirm`（默认）模式下按钮与行为完全不变。状态卡上的「检测并导入」
  按钮不受影响。
- **R2 自动同步补全（仅 direct 模式，复用 previewMutation + autoApply 路径）**：
  - MCP：保存成功后同步该 server 已分配的 `globalTools`（更新场景传递编辑前
    分配列表；新建场景无分配无需同步）；删除成功后同步删除前 `globalTools`
    （清理旧受管条目）；导入成功后同步导入的目标工具。逐工具顺序同步，沿用
    启停自动同步的既有写法。
  - 提示词：保存成功后同步编辑档案的 `globalTools`；删除成功后同步删除档案的
    `globalTools`。导入为无损接管原生文件，无需同步。
  - Skills：无新增自动同步（后端拒绝删除已分配条目；目录导入自带确认流程；
    全局分配已自动同步）。
- **R3 文案与说明**：direct 模式下所有引导「点击直接应用全局同步/预览全局同步」
  的成功通知改为自动同步语义（保存/删除/导入消息）；Skills 中央列表脚注文案、
  `global-target-status-ui` 中引用「预览全局同步」的暂态描述按模式区分；设置页
  「直接应用」说明补充保存/删除等中央操作也会自动同步。
- **R4 安全性**：不新增任何写入路径；自动同步仍先生成持久化预览，仅
  `canAutoApplyPreview` 通过时自动 Apply，冲突/错误/受阻回退预览对话框并禁用
  Apply；快照与回滚机制不变。

## Non-goals

- `preview_confirm` 模式的任何行为变化。
- Provider 面板（渠道同步）与项目详情页（项目同步）按钮：不属「全局同步」按钮。
- Skills 目录导入 / MCP 导入对话框内部流程的改变。
- 批量操作合并同步。

## Acceptance Criteria

- [ ] direct 模式下三页不出现「直接应用全局同步 / 直接应用 X 全局同步」按钮；
      preview_confirm 模式按钮仍在且行为不变。
- [ ] direct 模式下：MCP 保存/删除/导入、提示词保存/删除自动生成预览并 Apply
      （断言 preview 与 apply 载荷及 previewId）；冲突预览回退对话框且 Apply 禁用；
      预览/Apply 失败仅使用共享失败通知。
- [ ] direct 模式下无任何通知/说明再引导用户点击已隐藏的全局同步按钮。
- [ ] 设置页说明与实际行为一致。
- [ ] spec direct-apply scenario 更新（save/delete/import 自动同步 + 按钮隐藏契约）。
- [ ] `pnpm check` 全绿。
