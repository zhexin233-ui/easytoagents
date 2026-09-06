# Hook 事件移到分配维度并按工具事件分组管理

## Goal

导入中央列表后，Hook 原来作用于哪个事件不应该限定它在其他工具上的事件。
事件从中央记录的**固定属性**改为**分配属性**：同一中央 Hook 在 Claude 可以
分配为 SessionStart、在 Codex 分配为 PreToolUse。Hooks 页面按工具提供事件
分组视图（如「会话开始」「工具调用前」），从中央列表往分组里添加。

## Requirements

- R1 迁移 0016：`hook_global_assignments` / `hook_project_assignments`
  增加 `event NOT NULL` 列（CHECK 限定 13 个 canonical 事件），重建表并回填
  现有分配的事件（取自 hooks.event）；互斥触发器重建；`hooks.event` 保留为
  「默认/建议事件」（导入来源、UI 预选）。
- R2 分配 RPC：`set_global_hook_assignment` / `set_project_hook_assignment`
  增加 `event` 入参；分配时按工具校验事件支持（fail-closed）；同一
  (tool, hook) 已分配时改为更新事件（PK 不变，仍是一工具一事件；同一 Hook
  想在两个事件生效需复制中央记录，记为已知限制）。
- R3 查询：`HookDto.globalTools: Tool[]` 改为 `globalAssignments:
  {tool, event}[]`；`list_assigned_hooks` 的事件取自分配行（投影、外部键、
  认领判定全部使用生效事件）。
- R4 Hooks 页面重构：工具页签（Claude/Codex/Cursor/ZCode，按启用过滤）+
  事件分组卡（会话生命周期 / 提示词与通知 / 工具调用 / 子代理 / 上下文压缩，
  仅展示该工具支持的事件），每个分组列出已分配 Hook（含移除）+「添加」
  打开中央选择器；工具卡合并展示全局目标状态与预览/导入入口。
- R5 项目详情 Hooks 页签：添加时选择事件（默认取中央建议事件），已分配
  行展示生效事件。
- R6 原生导入流程不变（候选事件作为建议事件写入中央）。

## 非目标

- 同一 (tool, hook) 的多事件并存（需要 managed item 身份模型改造，另行任务）。
- 事件级别中文重命名写入原生文件（原生键仍为官方 PascalCase/camelCase）。

## Acceptance Criteria

- [ ] 同一中央 Hook 可在 A 工具分配为事件 X、在 B 工具分配为事件 Y；切换
      事件后同步预览/Apply 原生文件落入对应事件组。
- [ ] 不支持的事件组合仍被拒绝（fail-closed）；旧库升级回填后行为不变。
- [ ] Hooks 页面工具页签 + 事件分组可用；项目页签添加时可选事件。
- [ ] 迁移 0016 升级测试通过；schema_version=16；pnpm check 全绿。
