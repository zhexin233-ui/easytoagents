# 执行计划：项目级 Agent 文件只读纳入项目原生资源

按序执行；步骤 1（迁移）与步骤 2（后端）必须先于步骤 3（前端）。步骤 2.2 的同步守卫与
其测试是安全前提，必须在 2.4 放开描述符之前完成。每步末尾有验证命令。

## 步骤 1：数据库迁移 `0024`

- [x] 1.1 新增 `src-tauri/src/db/migrations/0024_project_native_agent_files.sql`，按
      design.md §2.2 的 12 步表重建（前置校验锚点
      `entry_type IN ('mcp_entry', 'directory', 'symlink', 'hook_entry')` → 建新表 →
      显式列复制 → 行数校验 → 删旧触发器 → 删旧表 / 重命名 → 重建索引与触发器 →
      `foreign_key_check` / `integrity_check` → 清理临时表）。
- [x] 1.2 在 `src-tauri/src/db/mod.rs` 的 `MIGRATIONS` 末尾登记 `version: 24`。
- [x] 1.3 `src-tauri/src/app/mod.rs` 三处 `schema_version` 断言 23 → 24。
- [x] 1.4 `src-tauri/src/db/tests.rs` 新增
      `project_native_agent_files_migration_preserves_rows_and_widens_check`：用
      `MIGRATIONS[..23]` 建旧库并插入四种旧类型行（含持快照的 disabled 行），迁移后断言
      行保留、`agent_file` 可插入、`prompt_file` 仍被拒绝、快照 id 更新仍被触发器拒绝、
      重开幂等。

验证：`cargo test --manifest-path src-tauri/Cargo.toml db::`、
`cargo test --manifest-path src-tauri/Cargo.toml app::`

## 步骤 2：后端守卫、观测与 DTO

- [x] 2.1 `src-tauri/src/agents/`：把 `agent_file_descriptor`、
      `parse_markdown_agent_file`、`parse_codex_agent_file`、`ParsedAgentFile`、诊断码常量
      提升为 `pub(crate)` 并在 `agents/mod.rs` 重导出；把 `import.rs` 的
      `MAX_AGENT_FILE_BYTES` 抽为模块级 `pub(crate)` 常量（import 继续引用）。
- [x] 2.2 `src-tauri/src/agents/service_core.rs::prepare_agents_sync`：在
      `list_agent_managed_targets` 之后 `retain` 通过 `agent_file_descriptor` 校验的行
      （design §3.2），带注释说明目录级身份行来源。
- [x] 2.3 `src-tauri/src/projects/native_resources_tests.rs` 先写
      `agent_directory_identity_row_never_enters_agents_sync`（design §3.3）；此时因描述符
      尚未放开，用例会因"无目录身份行"失败，作为红灯基线。
- [x] 2.4 `src-tauri/src/projects/models.rs`：`ProjectNativeEntryType` 增加 `AgentFile`
      （`"agent_file"`）。
- [x] 2.5 `src-tauri/src/projects/native_resources.rs`：
  - `supported_project_descriptors` 纳入 `ArtifactKind::Agent`；
  - `HookDisplayIndex` 泛化为 `NativeDisplayIndex`，`DescriptorObservation.hook_entries`
    改为 `display_entries`；
  - 新增 `observe_agent_items`（design §4.2 + §5），接入 `observe_items`；
  - `should_hide_centralized` 增加 `"agent"` 按路径 + 非空 baseline 判定（§4.3）；
  - `parse_artifact` 增加 `"agent"`；`safe_summary` Agent 兜底 `{ kind: "agent" }`；
  - `to_dto` 对 `AgentFile` 产出 `display_name` / `safe_summary`，
    `can_disable` / `can_restore` 恒 false；
  - `native_ownership` 对 Agent 返回 `INVALID_INPUT`（文案
    "Agent 文件暂不支持临时禁用与恢复"）；其余 `match entry_type` 补 `AgentFile`
    分支 fail closed（§6）。
- [x] 2.6 `native_resources_tests.rs` 补齐 design §8 其余用例：
      `claude_project_agent_files_are_listed_read_only`、
      `codex_cursor_opencode_agent_files_are_listed`、
      `removed_agent_file_becomes_missing`、
      `centrally_applied_agent_file_is_hidden_from_native_list`、
      `agent_file_action_preview_is_rejected`、
      `agent_description_with_secret_is_redacted_in_summary`、
      `unparseable_agent_files_are_listed_with_diagnostic`；2.3 的用例转绿。
- [x] 2.7 `pnpm bindings:generate` 后确认 `src/bindings/commands.ts` 的
      `ProjectNativeEntryType` 含 `"agent_file"`；`pnpm bindings:check` 通过。

验证：`cargo test --manifest-path src-tauri/Cargo.toml projects:: agents:: sync::`；
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`；
`cargo fmt --check --manifest-path src-tauri/Cargo.toml`。

## 步骤 3：前端展示

- [x] 3.1 `src/lib/projects-api.ts`：`ProjectResourceKind` 纳入 `"agent"`，
      `ProjectScopeKind` 相应简化，更新注释。
- [x] 3.2 `src/features/projects/detail/page.tsx`：去掉 Agents 视图对
      `ProjectNativeResources` 的门禁；`applyMutation.onSuccess` 失效列表补 `"agent"`。
- [x] 3.3 `src/features/projects/detail/native-resources.tsx`：`entryTypeLabel` 增加
      `agent_file`；只读行判定扩展到 `agent_file`；新增 `AgentSummary`（文件名、描述或
      脱敏提示、解析诊断、"Agent 文件暂不支持临时禁用与恢复。"）。
- [x] 3.4 `src/features/projects/project-detail-page.test-helpers.tsx` 增加
      `agentNativeResource` / `agentRedactedResource` fixture；
      `src/features/projects/project-detail-page-agents.test.tsx` 新增两条用例
      （design §7.4）。
- [x] 3.5 确认既有用例不回归：`project-detail-page.3.test.tsx` 的原生资源用例、
      `project-detail-page-agents.test.tsx` 的分配用例。

验证：`pnpm test --run project-detail-page`；`pnpm lint && pnpm typecheck && pnpm format:check`。

## 步骤 4：全量质量检查（trellis-check）

- [x] 4.1 按 `.trellis/spec/backend/quality-guidelines.md` 与
      `.trellis/spec/frontend/quality-guidelines.md` 的清单逐项过。
- [x] 4.2 `pnpm check` 全绿（format、lint、typecheck、vitest、cargo fmt / clippy / test）；
      `git diff --check` 无尾随空白。
- [x] 4.3 交叉一致性：MCP / Skill / Hook 原生资源既有测试不回归；中央 Agents 的
      `service_tests.rs` 全部通过；ZCode 项目级仍 Unsupported。
- 验证记录（2026-09-13）：`pnpm check` 全绿（38 个 Vitest 文件 / 326 个测试；
  Rust 379 passed、2 ignored，含 generated bindings、command smoke、Hooks 与 Phase 8
  集成测试）；新增 `projects::native_resources` 祖先符号链接回归测试后，定向测试 28
  passed；`git diff --check`、Rust fmt、clippy（`-D warnings`）均通过。复核期间补上
  Agent 目录祖先的 lstat 安全检查，避免路径组件被替换成符号链接后跟随到项目外。
- [ ] 4.4 手动验收：登记一个含 `.claude/agents/*.md` 与 `.codex/agents/*.toml` 的真实项目，
      Agents 视图下 Claude / Codex 各自列出原生文件；对该项目做一次 Agents 同步预览确认
      无意外删除目标；MCP / Skill / Hooks 视图正常。（待用户在应用 GUI 中确认）

## 步骤 5：Spec 更新与提交

- [x] 5.1 `.trellis/spec/backend/quality-guidelines.md` 项目原生资源场景补充
      `agent_file` 只读合同（目录级身份行 + 同步守卫、按路径与非空 baseline 隐藏、
      动作 fail closed、描述脱敏、不持久化正文）。
- [x] 5.2 `.trellis/spec/frontend/quality-guidelines.md` 把"native-resource views cover
      supported MCP and Skill resources only"更新为含 Hook / Agent 只读行的现状。
- [x] 5.3 `.trellis/spec/backend/database-guidelines.md` 记录 0024 复用 0021 表重建先例
      （一句话即可）。
- [x] 5.4 分三个 commit：迁移；后端守卫 + 观测 + 测试；前端展示 + 测试 + spec。

## 回滚点

- 步骤 1 单独可 revert（迁移未登记前无副作用；登记后本地库需删除重建）。
- 步骤 2.2 守卫独立于其他改动，可单独保留（它对现状无害）。
- 步骤 2、3 各自独立 commit；回滚代码前需清理本地库 `agent_file` 行与目录级 agent
  身份行（design §9）。

## 风险与备注

- 守卫（2.2）若漏掉，用户目录里恰好名为 `agents.<ext>` 的文件会成为同步删除目标；
  2.3 的用例故意使用该文件名，必须保持。
- 描述截断与脱敏只影响展示；哈希基于全文件字节，用户改动描述会让旧行 `missing`？
  不会：外部键是文件名，内容变化只更新 `observed_item_hash`，状态仍 `active`。
- OpenCode 在测试 fixture 中是否被 discovery 门禁排除需在 2.6 实测；若排除则以
  capability 结果为准，测试断言"无目标"而非硬编码路径。
