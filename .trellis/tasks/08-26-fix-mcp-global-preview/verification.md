# MCP 全局导入验证记录

日期：2026-08-26。代码实现和质量门已通过，用户已确认下方工作提交计划；不推送远程。任务保持 in_progress，归档与 journal 另行收尾。

## 修复结果

- 根因：全局同步预览只读取中央库中启用且分配到来源工具的 MCP，原生配置未自动进入中央库；原页面缺少显式导入入口。
- 每工具新增“检测并导入已有 MCP”，默认不勾选；确认仅导入所选项并分配到来源工具，后续单独预览/Apply。
- 精确同名且完整私有配置相同才复用；冲突不覆盖。停用、不支持、非法项保持非受管，不会自动启用或被后续同步删除。
- 令牌、源文件、中央及受管行版本、旧基线和活动写入保护均已加入；单个来源批次的数据库变更原子提交或回滚。

## 实际执行结果

| 检查 | 结果 |
| --- | --- |
| `pnpm bindings:generate` | 成功，绑定由生成器更新 |
| `pnpm test --run src/features/mcp/mcp-page.test.tsx` | 13/13 通过；后续类型安全修正也在全量中通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml mcp` | 中期 21/21 通过；最终新增边界用例在全量中通过 |
| `pnpm check` | 最终 exit 0：Prettier、ESLint、TypeScript、50 个前端测试、cargo fmt、clippy `-D warnings`、163 个 Rust 单元测试及 3 个集成测试全部通过 |
| 生成绑定一致性集成测试 | `generated_bindings_are_current` 通过 |
| Tauri IPC 冒烟集成测试 | `app_info_command_is_available_through_tauri_ipc` 通过 |
| 隔离全链路集成测试 | `isolated_full_chain_restores_exact_fixture_and_leaks_no_secret` 通过 |
| `pnpm build` | exit 0，Vite 生产构建成功 |
| `git diff --check` | 通过 |
| Trellis 上下文验证 | implement 13 条、check 8 条真实路径均通过 |

全量检查曾发现测试 mock 多余 async、数组下标可能为空，以及 AppState 旧 schema 版本断言；均已修正并重跑完整质量门，没有禁用规则或增加类型绕过。

## 验收证据

| 验收项 | 证据 |
| --- | --- |
| AC1、AC8：两工具发现/空/失败/保护 | MCP 页面两工具参数测试；`mcp_import_distinguishes_missing_parse_policy_and_unsafe_paths`；既有 adapter/scan 权限与能力回归 |
| AC2、AC3：所选入库、原文件不变、后续同步 | `mcp_import_selects_extends_and_syncs_without_touching_unselected_entries` 对两工具逐字节比较 discover/confirm/preview，随后真实隔离 Apply 验证未选中/无关项保留 |
| AC4：复用与冲突 | `mcp_import_reuses_identical_cross_tool_records_and_blocks_conflicting_names`；`mcp_import_compares_private_values_and_respects_project_assignments` |
| AC5：分批、重复、旧漂移 | 首个集成测试包含分批/重复令牌/已管理状态；`mcp_import_never_refreshes_drifted_existing_baselines`；主代理核对缺失项也走旧基线拒绝分支 |
| AC6：逐项边界与停用保护 | `mcp_import_rejects_unsupported_and_secret_entries_individually`；两工具导入→Apply 测试保留原生停用项 |
| AC7：过期与事务回滚 | stale file/DB/选择、central/target/item 行版本、项目分配、触发器中途失败及两次事务内源复核失败测试 |
| AC8：写入互斥 | `mcp_import_blocks_active_writers_without_consuming_the_token` 覆盖 applying/restoring/rollback_failed |
| AC9：私有保留与脱敏 | 先断言 RPC/preview 存储/sync_items/journal 载体非空，再排除 fixture header/env/extra/被拒参数密钥；私有配置通过 repository 回读核验 |
| AC10：UI 生命周期 | 默认无选择、不可选状态、精确 token/候选 payload、错误重扫、关闭重开旧响应隔离、焦点循环/恢复、确认在途关闭/重复保护、无 create/Apply 调用 |
| AC11：兼容 | 完整质量门覆盖既有中央 CRUD、全局/项目继承、冲突、绑定和同步；新增 v4→v5 升级/重复打开保留中央行测试 |

## 审查和规范

- 后端独立审查发现写锁等待和入库期间需要重新核验源文件，已在锁后、消费令牌前各复核一次，并用回滚测试固定。
- 格式边界独立审查未发现已证实缺陷；Codex 显式 type、SSE 和不可表达引用继续保守拒绝。
- 前端独立审查覆盖 dialog/page/query/focus 和任务合同，未发现已证实问题；主代理修正后执行最终完整验证。
- 已新增 `.trellis/spec/backend/mcp-import-guidelines.md`，记录命令/表结构、私有等价、增量基线、错误矩阵、秘密边界、UI token 生命周期、反例和测试要求；后端/前端索引均指向该合同。

## 验证边界和发布注意

- **未对真实用户配置执行导入确认、Apply 或任何写入测试**。先前本机核验只读取结构/数量，不输出凭证。
- UI 验证来自 jsdom，没有声称完成真实桌面点击验证；没有重新安装或启动用户的发布应用。新 UI/Rust 命令需在重新运行更新后的应用时使用。
- SQLite 与外部文件不存在跨资源锁；两次事务内复核捕获观察到的源变化，后续同步继续校验实际条目基线，不能声称文件/数据库严格原子提交。
- 第 5 次前向迁移通过现有启动备份机制执行，不能通过删除迁移或旧二进制忽略 schema 来降级。
- 停用、SSE、不可表达引用仍保持外部配置；本次没有扩展中央模型。

## 提交计划（用户已确认）

用户已批准一次提交：`fix: 补齐原生全局 MCP 导入与预览流程`。

文件清单：

- `src-tauri/src/app/mod.rs`
- `src-tauri/src/commands/mcp.rs`
- `src-tauri/src/db/mcp.rs`
- `src-tauri/src/db/mcp_imports.rs`
- `src-tauri/src/db/migrations/0005_mcp_import_previews.sql`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/mcp/import.rs`
- `src-tauri/src/mcp/mod.rs`
- `src-tauri/src/mcp/models.rs`
- `src-tauri/src/mcp/service.rs`
- `src/bindings/commands.ts`
- `src/features/mcp/mcp-import-dialog.tsx`
- `src/features/mcp/mcp-page.test.tsx`
- `src/features/mcp/mcp-page.tsx`
- `src/lib/mcp-api.ts`
- `.trellis/spec/backend/index.md`
- `.trellis/spec/backend/mcp-import-guidelines.md`
- `.trellis/spec/frontend/index.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/check.jsonl`
- `.trellis/tasks/08-26-fix-mcp-global-preview/design.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/implement.jsonl`
- `.trellis/tasks/08-26-fix-mcp-global-preview/implement.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/prd.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/task.json`
- `.trellis/tasks/08-26-fix-mcp-global-preview/verification.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/backend-contracts.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/backend-preview.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/backend-review.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/format-review.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/frontend-preview.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/import-adoption.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/import-formats.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/import-ui.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/local-state.md`
- `.trellis/tasks/08-26-fix-mcp-global-preview/research/planning-decisions.md`

未识别或不相关的脏文件：无。仅执行已批准的工作提交，不推送远程；归档和 journal 不混入本次工作提交。
