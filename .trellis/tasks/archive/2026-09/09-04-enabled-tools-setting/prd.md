# 设置中新增启用的工具配置（默认启用 Claude/Codex，Cursor 可选）

## Goal

在设置对话框中新增「启用的工具」配置，让用户选择哪些 AI 工具在应用界面中可见。
被关闭的工具不再出现在界面上：顶部工具入口（右上角）、中央列表（提示词 / MCP /
Skills 页的工具图标列）、项目详情页的工具图标，以及其余以图标形式展示工具的位置。

## Background

当前应用对三个工具（Claude、Codex、Cursor）一律全量展示。Cursor 能力较少
（无 Provider / 提示词能力），对不使用 Cursor 的用户是噪音。需要一个全局开关，
默认只展示 Claude 和 Codex，Cursor 作为可选项由用户自行开启。

## Requirements

### R1 设置项

- 设置对话框（应用偏好）新增「启用的工具」区块，提供 Claude / Codex / Cursor
  三个独立开关。
- 默认值：Claude 启用、Codex 启用、Cursor 关闭。
- 设置立即生效并持久化在本机应用数据库（与 `apply_mode` 同级、同一存储机制）。
- 关闭全部工具在 UI 上不禁止（允许空集），界面需优雅降级为空态，不报错。

### R2 关闭后不再显示（显示层过滤）

被关闭的工具从以下位置消失（以图标或入口形式出现的地方）：

- 顶部工具入口（右上角 Claude/Codex 链接胶囊）。
- 中央列表卡片底部的工具图标列（提示词、MCP、Skills 三个页面的
  PlatformAssignmentButton 组）。
- 项目详情页的平台图标选择列与「工具配置状态」中的目标卡片。
- 总览页（Dashboard）的工具摘要卡片。
- 提示词 / MCP / Skills 页的「全局目标状态」状态卡片。
- 新手引导（Onboarding）中的工具检测步骤。
- Provider 面板的「复制到另一工具」入口，当目标工具被关闭时不再提供。

### R3 行为边界（明确不做）

- 仅做显示层过滤：不删除、不隐藏后端已有数据；已存在的分配关系
  （globalTools）与同步行为保持不变——向已关闭工具的既有同步仍会执行。
- 被关闭工具的 profile 路由（/claude、/codex）不做路由守卫，直接输入 URL
  仍可访问（MVP 接受）。
- 不提供「至少启用一个工具」的强制校验。

## Acceptance Criteria

- [ ] 设置对话框出现「启用的工具」区块，含三个带图标与名称的开关，默认状态为
      Claude ✓、Codex ✓、Cursor ✗。
- [ ] 切换开关后立即保存（无需“保存”按钮），界面相关位置即时增减对应工具图标。
- [ ] 重启应用后设置保持。
- [ ] 关闭 Cursor：顶部入口（本就只含 Claude/Codex）不变；MCP / Skills /
      项目详情的图标列与状态卡片中 Cursor 消失；总览页 Cursor 卡片消失。
- [ ] 关闭 Claude 或 Codex：顶部工具入口、提示词页图标列与状态卡片、MCP /
      Skills 图标列与状态卡片、项目详情图标列与工具配置状态、总览页对应
      卡片全部消失。
- [ ] 项目详情页当前选中的工具被关闭时，选中视图自动回落到第一个启用工具，
      不会出现“无选中图标”的中间态。
- [ ] 后端：`app_settings` 新增 `enabled_tools` 键（JSON 数组），缺省返回
      `["claude","codex"]`，非法取值按数据损坏报错（与 apply_mode 约定一致）。
- [ ] 生成的 TS 绑定与 Rust DTO 同步（bindings:check 通过）。
- [ ] `pnpm check` 全绿（format / lint / typecheck / vitest / cargo fmt+clippy+test）。

## Notes

- 语言：界面文案为中文，与现有设置对话框一致。
- 相关代码入口：`src/features/settings/settings-dialog.tsx`、
  `src-tauri/src/settings.rs`、`src/lib/tool-metadata.ts`。
