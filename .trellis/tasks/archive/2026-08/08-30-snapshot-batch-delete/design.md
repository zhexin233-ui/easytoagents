# Design: 私有快照批量删除与选中删除

## 1. 架构与边界

跨层最小改动，全部落在既有模块，不新建包：

```
src-tauri/src/sync/apply.rs        # delete_snapshots 核心逻辑 + DTO + 单测（快照域函数所在地）
src-tauri/src/sync/mod.rs          # re-export delete_snapshots / DTO
src-tauri/src/commands/overview.rs # #[tauri::command] delete_snapshots（持写锁）
src-tauri/src/lib.rs               # collect_commands! 注册
src/bindings/commands.ts           # pnpm bindings:generate 重新生成
src/components/snapshot-restore-dialog.tsx        # 多选 UI + 删除入口 + 确认步骤
src/components/snapshot-restore-dialog.test.tsx   # 前端测试
```

不做 schema 迁移（`snapshots` 表不变），不改恢复/预览/应用流程。

## 2. 后端契约

### DTO（定义在 `sync/apply.rs`，与 `SnapshotSummary` 同处）

```rust
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSnapshotsInput {
    pub snapshot_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDeleteFailureDto {
    pub snapshot_id: String,
    pub code: String,   // ErrorCode::as_str()
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSnapshotsResultDto {
    pub deleted_ids: Vec<String>,
    pub failures: Vec<SnapshotDeleteFailureDto>,
}
```

单删/多删共用一个命令：前端"删除选中"传 N 个 id，"全部删除"传列表全量 id。
输入去重；空列表返回空结果（不报错）。命令层不做整单失败——单项失败进 `failures`，
只有基础设施级错误（锁不可用、权限审计失败、DB 提交失败）才整体 `Err(AppError)`。

### 执行流程（`sync::delete_snapshots`）

签名对齐 `restore_snapshot`：`pub fn delete_snapshots(write_operations: &Mutex<()>, database: &mut Database, paths: &AppPaths, input: &DeleteSnapshotsInput) -> Result<DeleteSnapshotsResultDto, AppError>`

1. 获取 `write_operations` 互斥锁（与 apply/restore 互斥，天然避免与恢复流程竞态）；
   `paths.audit_permissions()?`（对齐 `restore_snapshot` 命令前置）。
2. 对每个 id 预检（逐项，失败记入 `failures` 并跳过该项）：
   - 查 `snapshots` 行（run_id, snapshot_path）；缺失 → `NOT_FOUND`。
   - 活动引用阻断：`SELECT 1 FROM sync_runs WHERE id = snapshot.run_id AND status IN ('applying','restoring','rollback_failed')` → 命中即 `CONFLICT`（"快照仍被活动 run 引用，需等待恢复完成后删除"）。
   - 存储路径校验（防越权删除）：`snapshot_path` 必须等于
     `paths.snapshots().join(run_id).join(format!("{snapshot_id}.snapshot"))`，
     且父目录 canonicalize 后仍在 snapshots root 内。**从 `load_snapshot_record`
     （apply.rs:3721）中抽取共享 helper `validate_snapshot_storage_path(paths, run_id, snapshot_id, snapshot_path)`，
     两处复用**（code-reuse 规则），delete 侧不做内容 hash 校验（删除无需读全文）。
3. 逐项删除：先 `fs::remove_file(snapshot_path)`（文件已不存在视为成功，属自愈）；
   失败 → 该项 `ATOMIC_WRITE_FAILED` 记入 failures（沿用 `AppError::atomic_write(path, "remove_snapshot")`）。
4. 文件移除成功的项，在**一个** Immediate 事务中 `DELETE FROM snapshots WHERE id = ?` 逐条执行并提交；
   提交失败 → 整单 `Err(AppError::database(...))`（此时个别文件已删、DB 行保留，
   恢复这些快照会 fail closed 报错——与 create_snapshot 的镜像风险对称，接受此权衡）。
5. 返回 `{ deleted_ids, failures }`。活动 run 引用阻断意味着 `detect_interrupted_run`
   的输入不变，无需刷新 interrupted 缓存。

### 错误码选择

- `NOT_FOUND`：快照不存在。
- `CONFLICT`：仍被活动 run（applying/restoring/rollback_failed）journal 引用。
- `ATOMIC_WRITE_FAILED`：快照文件删除失败。
- 命令级：`WRITE_IN_PROGRESS`（锁不可用）、`PERMISSION_DENIED`（审计失败）、`DATABASE_ERROR`。

### 已知行为（记录为设计决策，非 bug）

- 已持久化但未执行的恢复预览（kind='restore', status='previewed'，envelope 存于
  `sync_items.redacted_diff_json`）引用的快照被删除后，该预览执行时会在
  `load_snapshot_record` 报 `NOT_FOUND` fail closed。预览本就一次性且强校验指纹，
  不为它加 JSON 解析级阻断查询（成本高、收益低）。

### 命令层（commands/overview.rs）

```rust
#[tauri::command]
#[specta::specta]
pub fn delete_snapshots(state: State<'_, AppState>, input: DeleteSnapshotsInput)
    -> Result<DeleteSnapshotsResultDto, AppError>
{
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    sync::delete_snapshots(state.write_operations(), &mut database, state.paths(), &input)
}
```

`lib.rs` `collect_commands!` 增加 `commands::overview::delete_snapshots`，然后
`pnpm bindings:generate`。

## 3. 前端设计（SnapshotRestoreDialog）

状态（组件内新增）：

- `selectedIds: Set<string>` — 复选框勾选集合；列表变化时清理已不存在的 id。
- `deleteConfirm: { mode: "selected" | "all"; ids: string[] } | null` — 二次确认步骤。
- `deleteSummary: { deleted: number; failed: number } | null` — 结果反馈（沿用现有区块风格）。

UI 结构：

- 列表上方工具条（快照非空时显示）：`删除选中 (N)`（N=0 禁用）、`全部删除`，均 `variant="outline" size="sm"`。
- 每条快照卡片左侧加 `<input type="checkbox">`，`aria-label` 含 targetPath，勾选切换 selectedIds。
- 点删除按钮 → 进入 `deleteConfirm` 确认区块（复用预览区块的 section 样式）：
  文案"将永久删除 N 个恢复点，删除后无法再回滚到这些快照"，按钮 `取消` / `确认删除`。
- `deleteSnapshots` mutation：`unwrapResult(await commands.deleteSnapshots({ snapshotIds: ids }))`；
  onSuccess：写 deleteSummary（含 failures 计数）、清空 selectedIds 与 deleteConfirm、
  `invalidateQueries({ queryKey: syncKeys.all })` **和** `invalidateQueries({ queryKey: dashboardKeys.all })`
  （Dashboard `snapshotCount` 在另一个 query root，现有恢复流程只失效 syncKeys——删除必须两个都失效，AC4）。
- `handleClose` 重置全部新增状态；确认区块与预览区块互斥显示（同屏只出现一个 section）。
- 不改 `prioritizeSnapshot` / 对话框打开逻辑；无 destructive 按钮变体（button.tsx 只有 default/outline），确认按钮用 default 变体 + 文案强调。

## 4. 兼容与回滚

- 纯增量：无迁移、无既有命令签名变化；回滚 = revert 提交即可。
- bindings 变更由 `bindings:check` 测试守护（`generated_bindings_are_current`）。
- 风险文件：`apply.rs`（超大文件，需精准插入 + 从 `load_snapshot_record` 抽 helper 时保持既有测试全绿）。

## 5. 测试策略

- Rust（apply.rs tests 模块，沿用隔离临时 home fixture 模式）：
  1. 批删成功：DB 行 + 文件均消失，`deleted_ids` 完整。
  2. 单删（列表含 1 个 id）成功。
  3. 活动 run（applying/restoring/rollback_failed 三态至少覆盖其一 + 其余状态可删）引用 → CONFLICT，文件保留。
  4. 不存在 id → NOT_FOUND；混合批次中它不影响其他项（best-effort）。
  5. DB snapshot_path 与预期模式不一致（模拟越权路径）→ CONFLICT/相应错误，文件不被删除。
  6. 去重：重复 id 只删一次。
- 前端（snapshot-restore-dialog.test.tsx，mock commands 增 `deleteSnapshots`）：
  勾选 → 删除选中 → 确认 → 调用参数正确、列表刷新、选择清空；
  全部删除同理；failures 返回时展示计数；未确认不发起调用。
