# 冲突面板标出不匹配条目并支持受管条目以当前内容重新接管

## Goal

解决「一个受管条目被外部工具重写 → 整个目标永久冲突且应用内无解」的问题（实例：ChatGPT.app
重写 ~/.codex/config.toml 的 node_repl 后，codex 全局 MCP 从 8-29 起无法写入任何变更）。

1. 冲突面板标出具体不匹配的受管条目（现在只有 MANAGED_ITEM_BASELINE_MISMATCH 诊断码）。
2. 对基线类冲突提供「以当前内容重新接管」按钮：把目标基线与受管条目基线对齐到当前磁盘内容，
   解锁目标；随后正常同步会按中央意图重新写入受管内容。

## Background

- `verify_managed_item_baselines` 校验失败只返回 `TargetScan::ManagedItemBaselineMismatch`，
  不携带条目名；`assess_drift` 映射为 ExternalOwnedChange + CONFLICT。
- 目标级基线（full/managed hash）与条目级基线（managed_items.last_applied_item_hash）任一
  不符都判死整个目标；「检测并导入」对已托管条目会拒绝（普通 INSERT）。
- 重新接管必须同时刷新两级基线，只刷条目级仍会被目标级 ExternalOwnedChange 拦住。

## Requirements

- **R1 条目定位**：`PreviewTargetPlan` 新增 `baselineMismatchedItems: string[]`；MCP 预览在
  条目基线不一致时填入外部键列表，对话框展示「内容不一致的受管条目：…」。目标文件整体缺失时
  列出全部受管条目。
- **R2 重新接管**：新增命令 `readopt_mcp_target`（输入 `{ tool, projectId }`，与预览目标一一对应）：
  - 扫描 Observed：刷新目标 full/managed 基线；条目仍在磁盘的刷新条目基线，已消失的删除基线行；
  - 扫描 Missing：清空条目基线行并清空目标基线（下次同步按缺失目标重建）；
  - ParseError/PermissionDenied/TargetTypeChanged/Failed 等拒绝并返回稳定错误；
  - 只动基线表，不改中央记录/分配/原生文件；获取 write_operations 互斥避免与 apply 交错；
  - bumped managed_targets.row_version，使旧持久化预览自然失效。
- **R3 readoptAvailable**：仅当该目标的 drift 判定为 ExternalOwnedChange（含条目基线不匹配与
  目标级受管内容冲突）时为 true；由 MCP 服务端计算下发，Provider/Prompt/Skills 不启用。
- **R4 对话框交互**：readoptAvailable 的冲突目标卡片显示按钮「以当前内容重新接管」；MCP 页点击后
  关闭对话框 → 刷新 → 自动重新生成预览（直接应用模式随之自动应用）；项目详情页点击后关闭对话框
  并提示再次点击同步按钮；pending 期间禁用。
- **R5 兼容**：旧持久化预览反序列化需要 serde default；默认（预览确认）模式行为不变。

## Non-goals

- Skills 符号链接条目的重新接管（后续任务）。
- Provider/Prompt 的 WholeDocument 冲突解法。
- 自动重接管（必须用户显式点击）。

## Acceptance Criteria

- [ ] 条目基线不一致的 MCP 预览目标在对话框中列出具体外部键；文件缺失时列出全部受管条目。
- [ ] 仅 ExternalOwnedChange 冲突类目标显示「以当前内容重新接管」；其他冲突（解析失败、权限、
      策略、信任）不显示。
- [ ] 点击后基线两级全部对齐当前磁盘；再次预览不再 conflict，Apply 解锁；中央记录与原生文件
      在接管动作中不被修改。
- [ ] 文件被删除的场景：接管后下次预览回到「缺失、可创建」状态。
- [ ] `pnpm check` 全绿（含 bindings 校验与新增测试）。
