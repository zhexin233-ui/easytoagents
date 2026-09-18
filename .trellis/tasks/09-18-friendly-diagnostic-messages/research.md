# 诊断码用户展示研究

## 当前入口

- 目标状态共享入口：`src/lib/global-target-status-ui.ts:11-195`。
- RPC 错误文本：`src/lib/rpc.ts:22-47`，当前会拼接 `${code}：${reason}`。
- Skills/MCP/Hooks/Agents/Profile 页面会直接渲染 `diagnosticCode`；Projects 详情还会
  直接渲染 `diagnosticCodes`。
- Dashboard 最近同步记录直接显示 `run.errorCode`；Onboarding Preview 直接显示
  `target.errorCode` 与 `warningCodes`。

## 已有文案与缺口

- Agents、Pi MCP、部分 Skills 初始状态和 Claude policy 已有共享中文 presentation。
- Skills/MCP/Hooks/Agents/Projects 的大量目标码、导入码、原生资源码仍只显示原始码或
  通用“检测失败”。
- 后端 `AppError` 已提供稳定 `ErrorCode`、静态中文 `message` 和受限 `details.reason`；
  无需修改后端错误契约即可在前端统一解释。
- Codex 的 `CODEX_INSTALLATION_PROBE_UNSUPPORTED` 在用户场景中重启应用即可恢复，首屏
  应提示重新检测/重启，而不是暴露探针实现细节。

## 主要码类

- 能力/安装：`TOOL_NOT_INSTALLED`、各工具 `*_INSTALLATION_PROBE_UNSUPPORTED`、
  `PI_AGENT_DIR_OVERRIDE_UNMAPPED`、配置覆盖/禁用和证据过期。
- 同步/目标：解析、权限、读取失败、类型变化、基线不完整、冲突、外部变化、信任/策略。
- 导入/中央库：`SKILL_IMPORT_*`、`CENTRAL_SKILL_*`、Agent/Hook 候选失败。
- 项目/原生资源：`PROJECT_ROOT_*`、`NATIVE_CONFIGURATION_*`、
  `PROJECT_NATIVE_RESOURCE_*`。
- RPC/运行记录：`NOT_FOUND`、`PERMISSION_DENIED`、`STALE_PREVIEW`、写入/回滚/数据库/
  迁移失败及 `ENVIRONMENT_PROBING`。

## 设计结论

内部码继续作为稳定协议和日志信息；前端新增单一 presentation registry，已知码给出
中文 label/description/nextStep，未知码按上下文安全降级。页面不得再把机器码作为普通
用户主文案，也不得改变现有动作阻断、Preview claim、查询失效或脱敏行为。
