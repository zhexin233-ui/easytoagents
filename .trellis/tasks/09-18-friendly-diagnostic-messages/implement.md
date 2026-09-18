# 统一诊断码用户提示：实施计划

## 实施顺序

1. **建立码表与类型边界**
   - 盘点生产路径中所有目标诊断码、`ErrorCode`、Preview warning/error、Dashboard
     和 Onboarding code。
   - 新建共享 presentation registry，定义 label/description/nextStep/tone 与未知码
     fallback；为 Codex 重启提示建立回归测试。

2. **迁移共享目标状态**
   - 将 `global-target-status-ui.ts` 的现有 Agents、Pi MCP、Skills 初始状态、Claude
     policy 映射迁入或委托给 registry。
   - 补齐同步、能力门禁、安装探针、Pi、Hook、Project/native resource 等已知码。
   - 保持 `SyncStatusBadge`、`ExternalChangeActions`、blocked/previewBlocked 行为不变。

3. **迁移 RPC 与 Preview 展示**
   - 改造 `profileErrorText`，移除普通用户文案中的原始 ErrorCode 前缀，保留已有特殊
     resource 文案和后端静态 message。
   - 迁移 Onboarding、工具 Profile、Dashboard 最近同步记录及各 Preview target/warning
     的错误展示。

4. **迁移资源页面与导入对话框**
   - 迁移 Skills/MCP/Hooks/Agents/Projects 的状态卡、导入来源/候选、项目 native
     resources；删除直接渲染机器码作为主说明的分支。
   - 对未知码统一展示安全 fallback，避免新后端 code 造成空白。

5. **补齐测试与审计**
   - 增加 registry 单测：Codex 重启、已知错误、warning、未知 code、ErrorCode 映射。
   - 更新跨页面组件测试，断言用户看到中文提示且不把内部码作为主文案。
   - 用 `rg` 审计生产 TSX/TS 中剩余的 `diagnosticCode`、`errorCode`、`warningCodes` 原样
     插值；仅允许日志/开发详情保留原码。

## 验证命令

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test --run
pnpm bindings:check
pnpm check
git diff --check
```

## 风险文件与回滚点

- 主要文件：`src/lib/diagnostic-presentations.ts`、`src/lib/global-target-status-ui.ts`、
  `src/lib/rpc.ts`，以及 Skills/MCP/Hooks/Agents/Projects/Profiles/Dashboard/Onboarding
  页面和对应测试。
- 高风险点：错误文案迁移不能改变按钮禁用、Preview claim、query invalidation 或脱敏。
- 回滚点：先回滚页面引用，再保留或删除 registry；后端稳定码和数据库无需回滚。

## 开始实现前的门禁

- PRD、设计和本实施计划经用户确认；
- `implement.jsonl` 与 `check.jsonl` 已包含真实规格/研究条目；
- 再运行 `task.py start`，进入实现阶段。
