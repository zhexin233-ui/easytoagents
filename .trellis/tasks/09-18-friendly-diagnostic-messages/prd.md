# 统一诊断码用户提示

## Goal

让用户看到诊断状态时，首先得到通俗、可执行的中文说明，而不是
`CODEX_INSTALLATION_PROBE_UNSUPPORTED`、`TARGET_PARSE_ERROR` 等内部机器码。
内部诊断码仍保留给日志、支持排查和测试；用户主界面应告诉用户下一步该做什么。

## Background / Confirmed Facts

- Codex 安装探针失败时，`src-tauri/src/adapters/codex/mod.rs:39-46` 将状态映射为
  `CODEX_INSTALLATION_PROBE_UNSUPPORTED`；同步目标随后进入 `failed`，Skills 页面会显示
  通用失败说明并直接渲染原始诊断码（`src/features/skills/skills-page.tsx:448-479`）。
- 当前共享状态入口是
  `src/lib/global-target-status-ui.ts:11-195`，但只覆盖 Agents、Pi MCP、部分 Skills
  初始状态与 Claude 策略；大量同步、导入、项目、安装探针诊断码仍原样显示。
- 项目已有可复用的后端中文 reason/action，也有 `BlockingState`、`SyncStatusBadge` 和
  共享目标状态 helper；因此首选在前端建立统一的诊断 presentation registry，而不是改写
  每个后端稳定码。
- “重启后恢复”的 Codex 场景应向用户提示“重新检测/重启应用”，不应把 PATH、权限和
  版本解析细节作为首屏文案。

## Requirements

- 为面向用户的诊断状态提供统一的中文 label、原因说明和可执行下一步；文案不能只显示
  内部码或“检测失败”。
- 统一覆盖 Skills、MCP、Hooks、Agents、Profiles/Prompts、Projects 以及工具安装/能力
  门禁中实际会进入 UI 的诊断码；同时覆盖 RPC `ErrorCode`、Dashboard/Onboarding 的运行
  错误码以及其他用户界面会展示的稳定内部码。未知码必须有安全的状态级 fallback，不能
  出现空白。
- 内部稳定码继续保留在 DTO、日志和测试中；用户主文案不直接依赖码名。
- 对可恢复状态（例如 Codex 探测快照失效）给出重启/重新检测提示；对权限、格式、信任、
  冲突、未安装、接管和导入等状态给出对应动作提示，并保持现有阻断语义。
- 同一诊断码在不同页面使用同一份 presentation，避免页面各自编写判断和文案。
- 保留无障碍要求：阻断状态有文字说明，loading/失败继续使用现有 `role` 约束。

## Acceptance Criteria

- [ ] Skills 全局目标不再把 `CODEX_INSTALLATION_PROBE_UNSUPPORTED` 作为主要用户文案，
      而显示“Codex 需要重新检测/请重启应用后重试”类提示；内部码仍可供支持排查。
- [ ] 共享 presentation registry 覆盖当前生产路径中所有已知目标诊断码、RPC `ErrorCode`
      和运行历史错误码，或为未覆盖码提供可理解且安全的 fallback；未知码不会导致空文案
      或直接暴露机器码作为主说明。
- [ ] Skills、MCP、Hooks、Agents、Profiles/Prompts、Projects 的状态卡、导入提示和阻断
      错误使用统一 presentation，动作提示与现有可用操作一致。
- [ ] 为 registry、Codex 重启提示、通用 RPC 错误、运行历史错误、未知码 fallback 以及至少
      一个跨页面复用场景补充前端测试。
- [ ] 不改变后端诊断码稳定性、同步 fail-closed 规则、敏感信息脱敏和现有写入/回滚行为。

## Scope Boundary

- In scope：面向用户的诊断码 presentation、统一 fallback、跨页面复用、相关测试和文案。
- Out of scope：重新设计探针判定逻辑、绕过能力门禁、修改数据库诊断码历史值、暴露原始
  操作系统错误或敏感路径。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
