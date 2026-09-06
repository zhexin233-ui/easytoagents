# 技术设计：事件随分配

## 1. 迁移 0016_hook_assignment_events.sql

- 重建两张分配表（无外部表引用它们，重建安全）：
  `hook_global_assignments_new(tool, hook_id FK, event NOT NULL CHECK(IN 13), created_at, PK(tool, hook_id))`，
  `INSERT … SELECT a.tool, a.hook_id, h.event, a.created_at … JOIN hooks`；
  DROP 旧表（级联删其触发器）→ RENAME → 重建 4 个互斥触发器（不含 event）。
  项目表同型（PK(project_id, tool, hook_id)）。
- `hooks.event` 保留为建议事件（导入回填、UI 预选），投影不再使用它。
- schema_version 15 → 16。

## 2. 服务与查询

- `list_assigned_hooks`：`SELECT h.*, a.event AS event FROM … JOIN 分配表`，
  HookRecord.event = 生效事件；投影/外部键（event|身份|matcher）/认领判定/
  native_event 键全部自然使用生效事件，无需改动。
- `set_global_assignment(tool, hook_id, event, assigned, row_version)`：
  assigned 时校验 `hook_event_supported(tool, event)`；INSERT … ON
  CONFLICT(tool, hook_id) DO UPDATE SET event=excluded.event（切换事件）；
  取消分配忽略 event。项目侧同型。
- `global_assignments_for_hook` → `Vec<(Tool, HookEvent)>` 替换
  global_tools_for_hook；`HookDto.globalAssignments`。
- 导入与中央 CRUD 不变（event 为建议值；导入确认仍校验来源工具支持）。

## 3. 前端

- Hooks 页：工具页签（filterEnabledTools）→ 工具卡（状态徽章 + 预览/导入 +
  事件分组）。事件分组常量表（分类标题 + 事件 + 中文名），仅渲染
  `hookEventSupportedByTool(tool, event)` 为真的分组；分组行 =
  已分配 Hook（globalAssignments 含 (tool, event)）+ 移除 +「添加」→
  `HookAssignmentPickerDialog`（中央列表选择；若该 Hook 已在本工具其他
  事件，确认文案说明将切换）。
- 中央列表表单字段改标「默认事件（分配时预选）」；每行徽章显示各工具生效事件。
- 项目 Hooks 页签：选项行内事件下拉（默认建议事件）+ 添加；已分配行显示生效事件。
- 移除 Hooks 页对 PlatformAssignmentButton 的使用（MCP/Skills 仍用）。

## 4. 已知限制

- 一工具一 Hook 一事件：需要同 Hook 双事件时复制中央记录（PRD 非目标）。
