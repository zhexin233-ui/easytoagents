# 执行计划：项目级 Hooks 只读纳入项目原生资源

按序执行；步骤 1（迁移）与步骤 2（观测）必须先于步骤 3（前端）。每步末尾有验证命令。

## 步骤 1：数据库迁移 `0021`

- [x] 1.1 新增 `src-tauri/src/db/migrations/0021_project_native_hook_entries.sql`，
      按 design.md §2.2 的 12 步表重建（前置校验 → 建新表 → 显式列复制 →
      行数校验 → 删旧触发器 → 删旧表/重命名 → 重建索引与触发器 →
      `foreign_key_check` / `integrity_check`）。
- [x] 1.2 在 `src-tauri/src/db/mod.rs` 的 `MIGRATIONS` 末尾登记。
- [x] 1.3 `src-tauri/src/db/tests.rs` 新增迁移测试：旧类型行保留、
      `hook_entry` 可插入、`prompt_file` 仍被拒绝、重开幂等、
      `trg_snapshots_reject_native_resource_id_update` 仍生效。

验证：`cd src-tauri && cargo test db::` 

## 步骤 2：后端观测与 DTO

- [x] 2.1 `src-tauri/src/projects/models.rs`：`ProjectNativeEntryType` 增加
      `HookEntry`（`"hook_entry"`）。
- [x] 2.2 `src-tauri/src/hooks/service_core.rs`：`native_entries` 提升为
      `pub(crate)`；确认 `events_root`、`build_hook_ownership` 已可从
      `projects` 模块调用（必要时在 `hooks/mod.rs` 重导出）。
- [x] 2.3 `src-tauri/src/projects/native_resources.rs`：
  - `supported_project_descriptors` 纳入 `ArtifactKind::Hook`；
  - 新增 `observe_hook_items`（design §3.2），接入 `observe_items`；
  - `should_hide_centralized` 增加 `"hook"` 按哈希判定（§3.4）；
  - 对账函数返回 Hook 展示索引，`to_dto` 产出 `display_name` 与
    `safe_summary`（§4），命令经 `contains_detectable_secret` 脱敏；
  - `can_disable` / `can_restore` 对 `HookEntry` 恒 false；
  - `native_ownership` 对 Hook 保持 `INVALID_INPUT` 并更新文案；
  - `evidence_entry_type` 改 `Result`，其余 `match` 补 `HookEntry`
    分支 fail closed（§5）。
- [x] 2.4 验证目标身份行共用（design §3.3）：写一个测试先通过项目 Hook 分配
      同步，再对账原生资源，断言两者 `target_id` 相同。
- [x] 2.5 `src-tauri/src/projects/native_resources_tests.rs` 新增用例：
  - `claude_project_hooks_are_listed_read_only`（多事件、matcher 组、无 matcher
    组；断言文件字节不变、`canDisable/canRestore=false`、`safeSummary` 字段）；
  - `cursor_flat_and_zcode_nested_hooks_are_listed`；
  - `removed_hook_entry_becomes_missing`；
  - `centrally_synced_hook_entry_is_hidden_from_native_list`；
  - `hook_entry_action_preview_is_rejected`；
  - `hook_command_with_secret_is_redacted_in_summary`。
- [x] 2.6 重新生成前端 bindings（找到项目既有生成入口，如 specta 导出测试或
      `pnpm` 脚本），确认 `src/bindings/commands.ts` 中
      `ProjectNativeEntryType` 含 `"hook_entry"`。

验证：`cd src-tauri && cargo test projects:: hooks::`；
`cargo clippy --all-targets` / `cargo fmt --check`（若项目启用）。

## 步骤 3：前端展示

- [x] 3.1 `src/features/projects/detail/native-resources.tsx`：
      `entryTypeLabel` 增加 `hook_entry`；`NativeResourceRow` 对 hook 条目
      隐藏操作按钮、渲染 safeSummary（事件、matcher、命令或脱敏提示、超时）
      与"Hooks 暂不支持临时禁用与恢复"说明。
- [x] 3.2 `src/test/fixtures/dtos.ts` 增加 hook 条目 fixture；
      在 `src/features/projects/project-detail-page.*.test.tsx`（选与原生资源
      相关的分片）新增用例：Hooks 视图渲染 hook 条目、无操作按钮、
      脱敏条目不显示命令。

验证：`pnpm test -- project-detail-page`；`pnpm lint && pnpm typecheck`。

## 步骤 4：全量质量检查（trellis-check）

- [x] 4.1 按 `.trellis/spec/backend/quality-guidelines.md` 与
      `.trellis/spec/frontend/quality-guidelines.md` 的清单逐项过。
- [x] 4.2 `cargo test`（至少 db、projects、hooks、sync）全绿；
      `pnpm lint`、`pnpm typecheck`、相关 vitest 全绿。
- [x] 4.3 交叉一致性：MCP / Skill 原生资源既有测试不回归；
      中央 Hooks 项目分配同步（`hooks/service_tests.rs`）不回归；
      OpenCode Hooks 仍 fail closed。
- [ ] 4.4 手动验收：登记 `/Users/zhexin/github/picslicer`，Claude / Codex /
      Cursor 的 Hooks 视图列出原生 hooks，MCP / Skill 视图正常。
      （待用户在应用 GUI 中确认）

## 步骤 5：Spec 更新与提交

- [x] 5.1 `.trellis/spec/backend/quality-guidelines.md` 原生资源场景补充
      Hook 只读合同（哈希外部键、按哈希隐藏、动作 fail closed、命令脱敏）。
- [x] 5.2 `.trellis/spec/backend/database-guidelines.md` 记录 0021 作为
      "表重建放宽 CHECK"的首个规范化先例。
- [x] 5.3 分两个 commit：迁移 + 后端观测；前端展示 + 测试。

## 回滚点

- 步骤 1 单独可 revert（迁移未登记前无副作用；登记后本地库需删除重建）。
- 步骤 2、3 各自独立 commit；回滚代码前需清理本地库 `hook_entry` 行
  （design §7）。

## 风险与备注

- `insert_project_target_identity_in` 与 `ensure_hook_target` 若 UNIQUE 键
  不一致会导致中央隐藏失效，2.4 必须先做。
- 条目内容哈希键意味着编辑 hook 命令会产生一条 `missing` + 一条 `active`，
  这是预期语义；`missing` 行随后续对账保留，可在后续任务考虑自动清理。
- ZCode 项目 `.zcode/config.json` 同时承载其他配置，扫描只读，无风险。
