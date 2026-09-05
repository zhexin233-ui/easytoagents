# 执行计划：Hooks 全局级与项目级管理

前置：`trellis-before-dev` 已读 `.trellis/spec/backend` 与 `frontend` 索引；实现全程遵循 docs/maintainers/adding-tool-adapter.md 的证据先行与 fail-closed 原则。

## 阶段 A：后端领域与适配器

- [x] A1 `domain/mod.rs`：`ArtifactKind::Hook` + `HookEvent` string_enum（13 canonical 事件）+ `hook_event_supported(tool, event)` + cursor camelCase 映射 `hook_event_native_key`；更新序列化往返测试。
- [x] A2 `adapters/mod.rs`：`ASSIGNABLE_HOOK_TOOLS`；确认 `ArtifactKind::Hook` 在所有 `match artifact_kind` 穷举分支中被显式处理（编译器驱动）。
- [x] A3 四个 adapter 的 `discover()` 各加全局 + 项目 Hook descriptor（design.md §2.2 的表格逐行落地）；zcode 注释既有 hooks 共存约定引用；codex 项目 trust 复用。
  - 验证：`cargo test -p easytoagents --lib adapters`（descriptor 单测，镜像 mcp/skills descriptor 测试）。

## 阶段 B：数据库迁移与仓储

- [x] B1 `db/migrations/0014_hooks.sql`：三张新表 + 互斥触发器 + writable_schema 放宽 `managed_targets`/`managed_items` CHECK（锚点精确、命中唯一）。
- [x] B2 `db/mod.rs` 注册迁移 14；`app/mod.rs` schema_version 断言更新；升级测试（从 v13 升级、旧行保留、约束 canary、重开）。
- [x] B3 `db/hooks.rs`：CRUD + 分配 + `list_assigned_hooks` + managed item 读写，乐观并发镜像 db/mcp.rs。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml db`。

## 阶段 C：服务与命令

- [x] C1 `hooks/service.rs`：投影/ownership/per-item 基线/导入解析（design.md §2.3、§2.4、§4）；`prepare_hooks_sync` 镜像 `prepare_mcp_sync`。
- [x] C2 服务层单测：事件校验拒绝、四工具投影 golden、继承/互斥、空投影不建目标、导入 fail-closed。
- [x] C3 `commands/hooks.rs` + `lib.rs` 注册；`pnpm bindings:generate`；`bindings:check`。
  - 验证：`cargo test --manifest-path src-tauri/Cargo.toml`（含 tests/command_smoke.rs 增加 hooks 冒烟）。

## 阶段 D：前端

- [x] D1 `tool-metadata.ts` capabilities.hooks + HOOK_TOOLS；`use-persisted-central-list-layout.ts` 加 key。
- [x] D2 `hooks-api.ts`（queryOptions/keys/导入查询）。
- [x] D3 `features/hooks/hooks-page.tsx` + 导入对话框 + `hooks-page.test.tsx`（vi.mock commands + Result 夹具 + 角色语义断言）。
- [x] D4 路由 `/hooks`、app-shell 导航、`app-shell.test.tsx` 断言更新。
- [x] D5 `project-detail-page.tsx` Hooks 页签（ProjectHookAssignments）+ 测试。
- [x] D6 `global-target-status-ui.ts`：补充 hooks 相关诊断码映射（如有新增码）。

## 阶段 E：质量门与收尾

- [x] E1 全量质量门：
  ```bash
  pnpm format:check && pnpm lint && pnpm typecheck && pnpm test --run
  pnpm bindings:check && pnpm rust:check
  git diff --check
  ```
- [x] E2 E2E：`src-tauri/tests/phase8_e2e.rs` 增加 hooks 的 Preview → Apply → 漂移 → Restore 跨层用例。
- [x] E3 文档：README 功能清单、docs/maintainers/adding-tool-adapter.md 能力矩阵表追加 Hooks 列。
- [x] E4 spec 更新（trellis-update-spec）：沉淀「hooks 同文件选择器共存」「数组型原生条目哈希匹配」经验。
- [x] E5 提交（feat: 新增 Hooks 全局级与项目级管理）。

## 回滚点

- 阶段 A/B/C/D 各自独立成 commit-able 单元；失败回退到上一阶段末尾。
- 运行时回滚：先关 HOOK_TOOLS → 移除 service/commands → 移除 adapter 分支；迁移保留。
