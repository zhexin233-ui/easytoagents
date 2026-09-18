# 统一诊断码用户提示：技术设计

## 目标与边界

本任务只改变前端对稳定内部码的解释和展示，不改变 Rust/SQLite 中的稳定码、同步
fail-closed 门禁、DTO 结构、敏感信息脱敏或原生写入流程。后端已有的静态中文错误消息
继续作为语义来源；前端负责把 code、status 和页面上下文组合成可执行提示。

## 统一展示层

新增一个前端共享 presentation 模块（建议放在 `src/lib/diagnostic-presentations.ts`），
由它拥有以下类型和入口：

- `DiagnosticPresentation`：`label`、`description`、`tone`、`nextStep`，以及必要的
  `previewBlocked` 派生信息。
- `presentTargetDiagnostic(status, code, context)`：处理 Skills/MCP/Hooks/Agents/
  Profiles/Projects 的目标诊断；`globalTargetStatusPresentation` 保持兼容包装，页面
  不再自行比较 code。
- `presentRpcError(error)`：把 `ErrorCode`、白名单 details.reason 和已有后端 message
  转成用户说明；不再把机器码拼进主文案。
- `presentPreviewCode(code, kind)`：处理 Preview 的 `errorCode`、`warningCodes`、
  Dashboard 同步记录和 Onboarding 计划。

已知 code 使用集中式 `Record`/分组表；自由字符串诊断码使用显式字典加安全 fallback。
未知目标诊断统一落到“需要重新检测，请刷新后重试”，未知 RPC 错误落到“操作失败，请
重新扫描后再试”，未知 warning 落到“同步计划包含需要注意的事项，请确认后继续”。

## 文案规则

- `CODEX_INSTALLATION_PROBE_UNSUPPORTED` 显示为“Codex 需要重新检测”，下一步为“请完全
  退出并重新打开 easytoagents，然后重试 Skills 同步”。其他安装探针码使用同一规则，
  仅替换工具名称。
- `TOOL_NOT_INSTALLED` 显示安装提示；权限、格式、信任、冲突、接管、导入和回滚类 code
  分别给出修复文件、信任项目、匹配/导入、恢复/重试等动作提示。
- 主界面不直接渲染原始机器码；内部 code 继续存在于 DTO、日志和测试中。若现有页面
  需要支持排查，可在折叠的开发/详情区域保留脱敏后的 code，但不作为普通用户的首屏文案。
- 颜色不是唯一语义来源，所有 blocked/failed 状态都必须有文字说明，并保留现有
  `role="alert"`、`role="status"` 约束。

## 页面迁移

统一迁移以下直接渲染内部码的入口：

- 全局目标状态：Skills、MCP、Hooks、Agents、Provider/Prompt；
- 导入来源和候选：Skill/Agent/MCP/Hooks；
- 项目详情与 native resources；
- Preview/Onboarding 的 target `errorCode` 与 `warningCodes`；
- Dashboard 最近同步记录；
- `profileErrorText` 和其他 RPC 错误通知。

页面只读取 presentation 的 label/description/nextStep；动作按钮仍由页面现有能力决定，
presentation 不直接发起 RPC，避免把导航、刷新和持久化 Preview 生命周期塞进文案层。

## 兼容性与数据流

数据流保持为：

`Rust stable code/message → generated DTO → shared presentation → page/notification`

不新增绑定字段，不改 ErrorCode union，不改数据库历史记录。后端新增未知 code 时，前端
自动使用安全 fallback；加入已知 code 时只需扩展 registry 和对应测试。

## 风险、回滚与验证

- 风险：一次迁移覆盖多个页面，容易遗漏直接插值；通过全仓搜索 `diagnosticCode`、
  `errorCode`、`warningCodes` 和 UI 测试审计防止漏改。
- 风险：后端 reason 可能包含技术细节；presentation 只接受白名单/静态文案，不展示原始
  OS、SQLite、路径或敏感值。
- 回滚：仅回滚前端 registry、页面引用和测试，不涉及数据库或原生文件恢复。
- 验证：先跑 registry/页面单测，再跑 `pnpm format:check`、`pnpm lint`、`pnpm typecheck`、
  `pnpm test --run`、`pnpm bindings:check`、`pnpm check` 和 `git diff --check`。
