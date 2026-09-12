# 执行计划：渠道导入基线冲突与工具切换状态残留

按序执行，每步有独立验证点；步骤 1 与步骤 2 相互独立，可分别提交回滚。

## 步骤 1：后端 — adopt_baseline 刷新孤儿基线

- [ ] 1.1 修改 `src-tauri/src/db/profiles.rs` `adopt_baseline`：
  - 查询行时多取 `row_version`；
  - `Some((id, _, _))`（含孤儿基线）不再报 CONFLICT，复用 `target_id`；
  - UPDATE 去掉 `baseline_* IS NULL` 守卫，改为 `AND row_version = ?` 乐观锁，
    覆盖 `baseline_full_hash` / `baseline_managed_hash` /
    `baseline_projection_json` / `last_status = 'in_sync'`；
  - `updated != 1` → `AppError::conflict("import", "原生目标受管基线已经变化")`。
- [ ] 1.2 新增后端回归测试（见 design.md 测试设计）：
  - 导入 → 删光档案 → 再导入成功且基线刷新；
  - row_version 被并发篡改时 confirm 报 CONFLICT；
  - 既有用例（无档案冲突守卫、全新库导入）保持通过。

验证：`cargo test -p <crate> profiles`（以 workspace 实际包名为准）；
`cargo clippy` / `cargo fmt --check` 若项目配置了则一并执行。

## 步骤 2：前端 — 工具路由 key 重置显示页

- [ ] 2.1 修改 `src/app/router.tsx`：五条工具路由元素补 `key`（见 design.md）。
- [ ] 2.2 在 `src/features/tool-profiles/tool-profiles-page.test.tsx` 新增
  切换工具回归测试：导入预览/错误/表单状态在切换到 codex、zcode 后不残留。

验证：`pnpm test -- tool-profiles-page`（或项目等价命令）；
`pnpm lint && pnpm typecheck`。

## 步骤 3：全量质量检查（trellis-check）

- [ ] 3.1 运行 `python3 ./.trellis/scripts/get_context.py --mode packages`，
      按 backend / frontend spec index 的 Quality Check 清单全量过一遍。
- [ ] 3.2 前端 `pnpm lint`、`pnpm typecheck`、相关 vitest 用例。
- [ ] 3.3 后端 `cargo test`（至少 profiles 与 db 模块）。
- [ ] 3.4 交叉一致性：确认 `managed_targets` 其它写入方
      （`ensure_profile_target`、mcp_imports、sync readopt）不受影响。

## 回滚点

- 步骤 1、2 各自独立成 commit；任一步骤问题可单独 revert。
- 无 schema 迁移，无数据回填需求。

## 风险与备注

- 乐观锁失败文案沿用「原生目标受管基线已经变化」，前端
  `profileErrorText` 会渲染为 `CONFLICT：原生目标受管基线已经变化`，
  无前端改动。
- 若实现中发现 `adopt_imported_prompt`（prompt 导入走同一 `adopt_baseline`）
  行为随之一并变化——这是预期收益（提示词导入同样受益），需在用例中覆盖或
  至少确认无回归。
