# 执行计划：Agents（子代理）全局级与项目级管理

前置：实施前运行 `trellis-before-dev` 读 `.trellis/spec/backend` 与 `frontend` 索引；全程遵循
`docs/maintainers/adding-tool-adapter.md` 的证据先行与 fail-closed 原则。证据见
`research/agents-capability-matrix.md` 与 `research/evidence/`。

## 阶段 0：前置核验（实施第一步，任何失败回到设计）

- [x] 0.1 打开当前 schema 的数据库，`SELECT sql FROM sqlite_master WHERE name='managed_targets'`，逐字核对 design.md §3.2 的五个锚点各恰好命中一次。
- [x] 0.2 确认 `build_preview_plan` / `apply_persisted_preview` 支持同一 run 内多个目标（读 `src-tauri/src/sync/mod.rs` 与 `sync/apply/core.rs`，或查 `profiles` 预览是否已有多目标用例）。若不支持，改为每工具每文件独立 run 并在 design.md 记录。
- [x] 0.3 确认 `TargetFormat::Toml + WholeDocument` 的渲染分支能处理多行字符串且输出确定；不满足则在 `document.rs` 补齐。

## 阶段 A：后端领域与适配器

- [x] A1 `domain/mod.rs`：`ArtifactKind::Agent`、`AgentName` 校验（交集正则）、序列化往返测试。
- [x] A2 `adapters/mod.rs`：`ASSIGNABLE_AGENT_TOOLS`、`PROJECT_AGENT_TOOLS`、`agent_file_extension(tool)`；`TargetDescriptor::for_agent_file(name, ext)`。编译器驱动补齐所有 `match ArtifactKind` 穷举分支（不得用 `_`）。
- [x] A3 五个 adapter 的 `discover()` 加目录 descriptor（design.md §2.3 逐行落地）；ZCode 项目级 Unsupported 无路径；Codex 项目级带 trust；Claude 沿用 customization policy 字段。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml adapters`（每工具 global/project descriptor、unsupported、allowed root 单测）。

## 阶段 B：数据库迁移与仓储

- [x] B1 `db/migrations/0022_agents.sql`：三张新表 + 四条互斥触发器 + `managed_targets` 五处 writable_schema 放宽（含 zcode 作用域约束）。
- [x] B2 `db/mod.rs` 注册 22；`app/mod.rs` schema_version 断言 21 → 22；升级测试（从 v21 升级、旧行保留、五处 CHECK 金丝雀、外键 / 索引 / 重开）。
- [x] B3 `db/agents.rs`：CRUD + 分配 + `list_assigned_agents` + 按 `(tool, scope, project_id)` 列出 agent 受管目标；乐观并发镜像 `db/hooks.rs`。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml db`。

## 阶段 C：服务与命令

- [x] C1 `agents/models.rs` + `service_core.rs`：CRUD、分配校验（`agent_scope_supported`）、`prepare_agents_sync`（desired 派生文件级目标 + 删除列表 + 空集不建目标）、状态聚合（§4.3）、preview / apply / readopt。
- [x] C2 `agents/service_native.rs`：五工具投影 golden（YAML frontmatter 由 serde_yaml_ng 生成；OpenCode `mode: subagent`；Codex TOML 三字段）；导入解析（frontmatter / TOML、dropped_fields、诊断码）。
- [x] C3 `agents/import.rs`：`discover_agent_import`（只读直属文件）/ `confirm_agent_import`（名称冲突 conflict）。
- [x] C4 服务层单测：ZCode 项目拒绝、Codex untrusted、停用后删除目标、全局继承 / 互斥、投影幂等、导入 fail-closed 与 dropped_fields。
- [x] C5 `commands/agents.rs` + `lib.rs` 注册；`pnpm bindings:generate`；`pnpm bindings:check`。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml`（含 `tests/command_smoke.rs` 增加 agents 冒烟）。

## 阶段 D：前端

- [x] D1 `tool-metadata.ts`：`capabilities.agents` / `capabilities.projectAgents`、`AGENT_TOOLS`、`PROJECT_AGENT_TOOLS`；`use-persisted-central-list-layout.ts` 加 `agents` key；`tool-metadata.test.ts` 更新。
- [x] D2 `agents-api.ts`。
- [x] D3 `features/agents/agents-page.tsx` + `agent-import-dialog.tsx` + `agents-page.test.tsx`（名称校验、分配、状态聚合展开、预览、导入 droppedFields 展示）。
- [x] D4 路由 `/agents`、`app-shell.tsx` 导航与 `app-shell.test.tsx` 断言。
- [x] D5 `project-detail-page` Agents 页签（`ProjectAgentAssignments`，工具切换排除 ZCode）+ 测试。
- [x] D6 Dashboard 计数与 Supported/Unsupported 文案；`global-target-status-ui.ts` 新增诊断码映射（`ZCODE_PROJECT_AGENTS_UNSUPPORTED`、`AGENT_*`）。
  - 验证：`pnpm test --run`、`pnpm typecheck`、`pnpm lint`。

## 阶段 E：质量门、实机 smoke 与收尾

- [x] E1 全量质量门：
  ```bash
  pnpm format:check && pnpm lint && pnpm typecheck && pnpm test --run
  pnpm bindings:check && pnpm rust:check
  git diff --check
  ```
- [x] E2 `src-tauri/tests/phase8_e2e.rs`：agents 的 Preview → Apply → 漂移 → readopt → 停用删除 → Restore 跨层用例（至少 Claude Markdown 与 Codex TOML 各一）。
- [x] E3 实机 smoke 探针已执行并记录（`research/smoke-2026-09.md`）；当前主机缺少 ZCode 且未启动需要认证/网络的真实会话，未完成项及后续步骤已明确记录。
- [x] E4 文档：README 核心能力表加 Agents 行；`docs/maintainers/adding-tool-adapter.md` 能力矩阵加 Agents 列（证据日期 2026-09-12）与 §10 之后新增 Agents 合同小节。
- [x] E5 spec 更新（`trellis-update-spec`）：沉淀「一个受管文件一个目标、目录级状态聚合」与「导入 dropped_fields 知情丢弃」两条约定。
- [x] E6 提交（`feat: 新增 Agents 子代理全局级与项目级管理`）。

## 回滚点

- 阶段 A / B / C / D 各自独立成 commit-able 单元；失败回退到上一阶段末尾。
- 阶段 0 任一核验失败：停止实施，回到 design.md 修订后再进入 A。
- 运行时回滚：先关 `AGENT_TOOLS` → 移除 service / commands → 移除 adapter 分支；迁移保留。
