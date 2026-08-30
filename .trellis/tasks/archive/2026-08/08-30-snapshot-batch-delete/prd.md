# 私有快照批量删除与选中删除

## Goal

为"私有恢复点"（原生目标快照）提供删除能力：用户可以在恢复点列表中清理不再需要的快照，
释放磁盘空间并管理恢复点数量。涉及 Rust 后端删除命令 + 前端 SnapshotRestoreDialog 的多选删除 UI。

## Background（代码证据）

- 私有快照唯一列表入口是 `SnapshotRestoreDialog`（`src/components/snapshot-restore-dialog.tsx`），
  由 Dashboard 页打开（`src/features/dashboard/dashboard-page.tsx:183`）。每条快照目前只有"预览恢复"按钮，无删除。
- 后端现有命令仅 `list_snapshots` / `preview_snapshot_restore` / `restore_snapshot`
  （`src-tauri/src/commands/overview.rs`），无任何删除命令。
- 快照双重存储：
  - DB `snapshots` 表（`src-tauri/src/db/migrations/0001_initial.sql:519`）：
    `id`(uuid)、`run_id`(FK sync_runs)、`target_path`、`snapshot_path`(绝对路径)、`content_hash`、
    `file_mode`、`target_type`、`link_target`、`row_version`、`created_at/updated_at`。
  - 文件系统：`<snapshots_root>/<run_id>/<snapshot_id>.snapshot` 私有文件
    （`create_snapshot`，`src-tauri/src/sync/apply.rs:1723`）。
- 快照被以下机制消费（删除的安全边界）：
  - 中断 run 恢复：run journal JSON 按 target 记录 `snapshot_id`
    （`RunJournal`/`JournalTarget`，`apply.rs:154-167`），活动 run 状态为
    `applying` / `restoring` / `rollback_failed`；`detect_interrupted_run` 会引用这些快照。
  - `restore_snapshot` → `load_snapshot_record`（`apply.rs:3721`）：校验
    `snapshot_path == snapshots_root/<run_id>/<snapshot_id>.snapshot`、父目录 canonicalize
    后必须在 snapshots root 内、私有权限、内容 hash 匹配。
  - 持久化恢复预览 envelope 携带 `restore_snapshot_id`（kind='restore', status='previewed' 的 run）。
- 既有删除命令模式：`delete_skill` 等（`src-tauri/src/commands/skills.rs:49`）使用
  `Versioned*Input { id, rowVersion }` + blockers 检查 + Immediate 事务 + `{ id, deleted }` 结果 DTO；
  全仓目前没有批量删除先例。
- 原生写入互斥：`restore_snapshot` 持有 `state.write_operations()` 锁并执行
  `paths.audit_permissions()`（`src-tauri/src/commands/overview.rs:69`）；快照删除同样涉及文件删除，需遵循。
- `SnapshotSummary` DTO 目前不含 `rowVersion`（`src/bindings/commands.ts:631`）。
- 前端模式：TanStack Query mutation + `invalidateQueries({ queryKey: syncKeys.all })`；
  弹窗用 `useDialogFocus`；`skills-page` 的删除是直接 mutation + message 反馈，无确认弹窗。
- 快照是不可变记录（创建后不 update 内容），但表有 `row_version` 乐观锁与 bump 触发器。

## Requirements

- R1 后端提供删除快照的命令（接受快照 ID 列表，单删/多删/全删共用同一命令），同步删除 DB 记录与快照文件。
- R2 删除必须遵循既有安全模式：写入互斥锁、路径校验（防逃逸/链接）、私有文件权限、审计。
- R3 不得破坏崩溃恢复：仍被活动 run（applying/restoring/rollback_failed）journal 引用的快照必须拒绝删除并给出明确错误。
- R4 前端在 SnapshotRestoreDialog 中提供（用户已确认）：
  - 每条快照前加复选框，支持多选；
  - "删除选中"按钮（选中数 > 0 时可用）；
  - "全部删除"按钮（列表非空时可用）；
  - 两种删除均需二次确认（弹窗内确认步骤，非 window.confirm）。
- R5 删除后刷新快照列表与 Dashboard 汇总（snapshotCount），清除选择状态。
- R6 全部操作显式触发，无隐式/自动删除。
- R7 批量删除对单项失败采用逐项 best-effort：被拒绝（不存在/被活动 run 引用）的项不执行删除，成功项正常删除，结果按项反馈给用户。

## Acceptance Criteria

- [ ] AC1 在恢复点列表可勾选若干快照并"删除选中"，也可"全部删除"；两者均有二次确认步骤；确认后 DB 记录与对应 `<snapshots_root>/<run_id>/<snapshot_id>.snapshot` 文件均被移除。
- [ ] AC2 删除引用了不存在/不匹配文件路径的记录时按既有错误语义失败（fail closed），不留下半删除状态或越权路径访问。
- [ ] AC3 属于活动 run（applying/restoring/rollback_failed）的快照删除被拒绝，返回明确错误码/文案；同批其他不受影响的快照仍按 best-effort 处理。
- [ ] AC4 删除成功后列表与 Dashboard 的 snapshotCount 即时更新，选择状态被清除。
- [ ] AC5 Rust 单测覆盖：单删/批删成功、活动 run 引用拒绝、文件缺失行为、批删部分失败语义；前端测试覆盖多选、删除选中、全部删除与确认交互（沿用 snapshot-restore-dialog.test.tsx 既有模式）。
- [ ] AC6 `pnpm check` 全绿（含 bindings:check 一致性）。

## Out of Scope

- 不做快照内容查看/导出（弹窗本身不展示快照内容）。
- 不做自动清理/保留策略（retention）。
- 不改动恢复/预览/应用流程本身。
