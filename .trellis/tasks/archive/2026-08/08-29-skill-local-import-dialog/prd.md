# Skills 本地目录导入改为按钮加弹窗

## Goal

Skills 页面的"从本地目录导入"不再作为常驻卡片平铺在页面顶部，改为中央列表头部的入口按钮，
点击后弹出弹窗完成选择目录与导入。

## Requirements

- 移除页面常驻导入卡片（含只读路径输入、选择目录、复制到中央库）。
- 中央列表标题行新增"从本地目录导入"按钮，与 MCP 页"新增 MCP"入口一致。
- 新建 `SkillDirectoryImportDialog`：弹窗持有 sourcePath、选择失败错误与 importSkill mutation；
  页面只通过 `onImported` 回调失效查询并展示成功消息。
- 弹窗遵循既有约定：role=dialog + aria-modal、useDialogFocus 焦点陷阱与恢复、Escape/取消/关闭，
  导入进行中禁止关闭与重复提交，未选目录时禁用提交。
- 关闭后重开时弹窗状态清零（回到"尚未选择"）。

## Acceptance Criteria

- [x] 默认无弹窗；点击按钮打开；取消/Escape 关闭且焦点回到触发按钮。
- [x] 未选目录时提交禁用；选择目录失败在弹窗内展示 role=alert 且不调用 importSkill。
- [x] 导入失败保留弹窗、已选目录与重试入口，不刷新列表。
- [x] 导入成功关闭弹窗、刷新中央列表并展示既有成功消息。
- [x] pnpm format:check / lint / typecheck / test（134 项）全部通过；浏览器实测弹窗交互正常。
