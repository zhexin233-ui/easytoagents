# 应用方式全局配置：MCP/Skills 可直接应用或保持预览确认

## Goal

为应用新增一个全局设置项「应用方式」，让用户决定 MCP / Skills 原生配置写入的流程：勾选「直接应用」后，无冲突的同步预览自动应用，跳过预览确认对话框；未勾选（默认）保持现有「生成预览 → 人工确认 Apply」流程不变。

## Background

- 当前所有原生写入（全局 MCP 同步、全局 Skills 同步、项目 MCP/Skill 追加、Profiles 同步）都必须经过
  `preview_*_sync` → `ChangePreviewDialog` 手动确认 → `apply_*_preview`，没有任何跳过确认的途径。
- 应用目前没有任何全局设置存储：前端仅主题偏好走 localStorage，后端仅有 `onboarding_state` 单例表。
- 后端安全模型（持久化预览、hash/row_version 复核、快照、journal、单写者）必须原样保留。

## Requirements

- **R1 全局设置存储**：新增 `app_settings` 键值表（迁移 0007）与 `get_app_settings` / `update_app_settings`
  命令；`applyMode` 枚举 `preview_confirm`（默认）| `direct`。
- **R2 设置界面**：新增「设置」页（一级导航 + `/settings` 路由），提供「直接应用（跳过预览确认）」勾选框，
  附行为说明；勾选立即生效并持久化。
- **R3 直接应用行为**，作用域限定为四条流程：全局 MCP 同步、全局 Skills 同步、项目 MCP 追加、项目 Skills 追加：
  - 预览仍照常生成（复用现有 preview 命令与全部服务端校验）；
  - 当预览无冲突（每个 target `changeKind !== "conflict"` 且 `errorCode === null`，与对话框 Apply 按钮可用
    条件完全一致）时，自动调用 apply，不弹出确认对话框；
  - 存在冲突或错误时回退为打开预览对话框（用户可见冲突详情，Apply 仍被禁用）；
  - `targets` 为空时保持现有提示文案，不应用；
  - 警告（如外部非受管修改）不阻止自动应用，与现有对话框允许带警告 Apply 的行为一致。
- **R4 按钮文案**：开启直接应用后，上述四条流程的触发按钮从「生成预览 / 预览同步」切换为「直接应用」语义文案。
- **R5 默认与兼容**：默认 `preview_confirm`，升级后所有页面行为与现状完全一致；Profiles（Claude/Codex
  Provider/提示词）本次不接入，保持预览确认，设置页需说明。
- **R6 安全性**：直接应用不得绕过任何后端校验（stale preview、hash、row_version、快照、journal、单写者）。

## Non-goals

- 不做按资源 / 按项目的差异化设置（全局单一开关）。
- 不做保存、启停、分配变更后的自动触发应用（直接应用仍需用户点击同步按钮）。
- 不接入 Profiles（Provider/提示词）同步流程。
- 不提供应用历史的额外展示（沿用现有消息提示与 sync runs）。

## Acceptance Criteria

- [x] 全新数据库默认 `applyMode = preview_confirm`；更新为 `direct` 后 `get` 返回 `direct`，重启后保持。
- [x] 默认（未勾选）状态下，所有页面行为与现状完全一致。
- [x] 开启后：MCP 页按钮变为「直接应用」语义；无冲突预览时不弹对话框直接写入，并展示应用结果消息。
- [x] 开启后：Skills 页同上。
- [x] 开启后：项目详情页 MCP / Skill 追加按钮直接应用并展示成功消息；冲突时回退弹出预览对话框且 Apply 禁用。
- [x] 冲突回退可验证：预览包含 `conflict` target 或 `errorCode` 时永远不自动应用。
- [x] 设置页勾选后立即生效（无需重启）且持久化到数据库。
- [x] `pnpm check` 全绿（format / lint / typecheck / vitest / cargo fmt + clippy + test / bindings check）。
