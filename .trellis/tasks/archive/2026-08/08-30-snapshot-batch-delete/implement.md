# Implement: 私有快照批量删除与选中删除

## 执行清单（按序）

### Step 1: 后端核心（src-tauri）

- [ ] 1.1 `src-tauri/src/sync/apply.rs`：从 `load_snapshot_record`（约 :3721）抽取
      `validate_snapshot_storage_path(paths, run_id, snapshot_id, snapshot_path)` 共享校验 helper，
      `load_snapshot_record` 改为调用它；行为不变，既有测试必须全绿。
- [ ] 1.2 `apply.rs`：新增 `DeleteSnapshotsInput` / `SnapshotDeleteFailureDto` /
      `DeleteSnapshotsResultDto`（derive 见 design.md §2）与 `pub fn delete_snapshots(...)`，
      实现按 design.md §2 流程（预检 → 移文件 → 单事务删行 → 组装结果）。
- [ ] 1.3 `src-tauri/src/sync/mod.rs`：`pub use apply::{...}` 增加 `delete_snapshots` 与三个 DTO。
- [ ] 1.4 `src-tauri/src/commands/overview.rs`：新增 `delete_snapshots` 命令（持锁模式对齐 `restore_snapshot`）。
- [ ] 1.5 `src-tauri/src/lib.rs`：`collect_commands!` 注册 `commands::overview::delete_snapshots`。
- [ ] 1.6 Rust 单测（apply.rs tests 模块，design.md §5 的 6 组用例）。

### Step 2: Bindings

- [ ] 2.1 `pnpm bindings:generate` 重新生成 `src/bindings/commands.ts`；
      `pnpm bindings:check` 通过。

### Step 3: 前端（src）

- [ ] 3.1 `src/lib/sync-api.ts` 无需改动（删除走 mutation，不新增 query）。
- [ ] 3.2 `src/components/snapshot-restore-dialog.tsx`：按 design.md §3 实现
      selectedIds / deleteConfirm / deleteSummary 状态、工具条、复选框、确认区块、
      deleteSnapshots mutation（失效 syncKeys.all + dashboardKeys.all）。
- [ ] 3.3 `src/components/snapshot-restore-dialog.test.tsx`：mock 增加 deleteSnapshots，
      覆盖多选删除、全部删除、确认前不调用、失败计数展示。

### Step 4: 全量验证与收尾

- [ ] 4.1 `pnpm check`（format:check + lint + typecheck + vitest + rust fmt/clippy/test）全绿。
- [ ] 4.2 对照 PRD AC1–AC6 逐条核对。

## 验证命令

```bash
pnpm bindings:generate && pnpm bindings:check
pnpm check
# Rust 单测（可先行单独跑）
cargo test --manifest-path src-tauri/Cargo.toml
# 前端单测
pnpm test --run
```

## 风险点与回滚

- 高风险文件：`src-tauri/src/sync/apply.rs`（5888 行，含大量测试）——Step 1.1 的 helper
  抽取必须先独立完成并跑通既有测试，再做 1.2 新增，两步分开提交点，便于二分回滚。
- 回滚点：Step 1（Rust 侧）、Step 2-3（bindings+前端）各自可独立 revert；
  无迁移/数据变更，revert 即完全回滚。

## Review Gates

- Gate A（Step 1 完成后）：Rust 既有测试全绿，helper 抽取无行为变化。
- Gate B（Step 3 完成后）：`pnpm check` 全绿后再进入收尾。
