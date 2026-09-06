# Hook 项目追加分组视图对齐全局管理

## Goal

项目详情页「Hook 项目追加」当前是扁平选项列表 + 每行事件下拉，与全局 Hooks
管理的事件分组视图不一致。改为与全局一致的结构：按工具支持的事件分组
（会话生命周期/提示词与通知/工具调用/子代理/上下文压缩），从中央列表往
分组里添加（选择器对话框），分组内移除；全局继承项以只读摘要呈现。

## Requirements

- R1 DTO：`HookProjectOptionDto` 增加 `assigned_event: Option<HookEvent>`
  （state=selected 时为项目分配行上的生效事件，其余为 NULL）；事件仍随分配
  存储，无需迁移。
- R2 前端共享：事件分组常量（分组/中文名/工具支持矩阵）抽取到
  `features/hooks/hook-events.ts`，全局页与项目页共用。
- R3 ProjectHookAssignments 重构：AssignmentCard 外壳保留（blocked/error/
  pending/empty/excludeFromGit/预览按钮），内容改为事件分组卡；每个分组列出
  state=selected 且生效事件匹配的 Hook（可移除）+「从中央列表添加」打开
  项目版选择器；全局继承（inherited）在顶部以只读摘要列出（不分组）。
- R4 项目版选择器对话框：列出 state=available 的中央 Hook，点击按分组
  事件分配（双乐观锁 row_version），成功后失效 hooks + project 查询。

## Acceptance Criteria

- [ ] 项目 Hooks 页签按事件分组展示/添加/移除，行为与全局页一致；
      继承项不出现在分组中、只读可见。
- [ ] `pnpm check` 全绿；bindings 更新；补项目页分组交互测试。
