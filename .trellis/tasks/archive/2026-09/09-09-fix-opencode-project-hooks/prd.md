# 修复 OpenCode 项目 Hooks 误导入口

## Goal

让项目资源管理只向当前工具展示受支持的资源类型，避免 OpenCode 用户进入未接入的 Hooks 面板并同时看到能力错误、空库提示和无效写入选项。

## Background

- OpenCode 自身提供插件回调式 hooks，但本应用的统一 Hook 合同是 command-only，当前明确不接入 OpenCode Hooks：`src/lib/tool-metadata.ts:75-86`、`src-tauri/src/adapters/opencode/mod.rs:1-4`。
- 后端对 OpenCode 的 Hooks 查询和写入 fail closed，返回 `INVALID_INPUT / OPENCODE_HOOKS_UNSUPPORTED`：`src-tauri/src/hooks/service.rs:166-174,224-230`。
- 项目页当前无条件展示 Hooks 资源按钮，并在选择后无条件渲染 `ProjectHookAssignments`：`src/features/projects/project-detail-page.tsx:352-403,449-464`。
- Hooks 查询失败时，前端把数据回退为空数组，同时独立渲染错误、空库提示和 `.git/info/exclude` 选项，导致误导性组合状态：`src/features/projects/project-detail-page.tsx:989-1004,1299-1324`。

## Requirements

- 项目资源类型入口必须依据当前工具的能力矩阵展示；OpenCode 不显示 Hooks 入口，支持 Hooks 的工具保持现有行为。
- 当用户从支持 Hooks 的工具切换到 OpenCode 时，当前资源视图必须落到 OpenCode 支持的资源类型，不能保留不可用的 Hooks 视图。
- 不改变 OpenCode MCP、Skill、Provider 或全局 Prompt 能力。
- 不放宽后端 `OPENCODE_HOOKS_UNSUPPORTED` 防线，也不尝试把 OpenCode 插件回调映射为统一 Hook 事件。
- 为资源入口过滤和跨工具切换行为补充前端回归测试。

## Acceptance Criteria

- [x] OpenCode 项目资源管理中不出现 Hooks 按钮，也不发起项目 Hook 选项查询。
- [x] OpenCode 项目页不再显示 `OPENCODE_HOOKS_UNSUPPORTED`、“中央库暂无可追加项”或 Hooks 专用 `.git/info/exclude` 选项。
- [x] 从 Claude、Codex、Cursor 或 ZCode 的 Hooks 视图切换到 OpenCode 后，界面自动显示一个受支持的资源视图。
- [x] 支持 Hooks 的工具仍可进入 Hooks 项目追加面板，现有交互不回归。
- [x] 相关前端测试、类型检查和 lint 通过。

## Out of Scope

- 接入或模拟 OpenCode 原生插件 hooks。
- 修改统一 Hook 事件合同或后端能力矩阵。
- 重构项目资源管理的整体视觉结构。
