# 技术设计

## 锁
引入 `parking_lot = "0.12"`，`AppState` 字段类型替换；`lock()` 不再返回 `Result`，删除 `state_lock_error` 的中毒分支。`write_operations: Mutex<()>` 保持同名语义（写串行化）。

## 错误 source
`error.rs`：
```rust
pub struct AppError { code, message: &'static str, details: ..., #[serde(skip)] #[specta(skip)] source: Option<String> }
impl AppError { pub fn with_source(mut self, e: impl Display) -> Self; pub fn database_from(path, op, e: &rusqlite::Error) -> Self; pub fn io_from(path, op, e: &std::io::Error) -> Self; }
```
`source` 只用于日志与 journal 的诊断字段；不进入 RPC、不进入用户可见文案。`Display` 实现不包含 source。

## 日志
`lib.rs setup` 初始化 `tracing_subscriber` 的 `fmt` 层写入 `AppPaths::logs()/app.log`（`tracing-appender` 按日滚动，保留 7 天），并用 `EnvFilter` 读取 `EASYTOAGENTS_LOG`。日志目录纳入 `security::audit_private_tree` 权限管理。所有 `with_source` 内部 `tracing::warn!(code, op, source)`；apply/restore 阶段 `info!(run_id, target, phase)`。使用 `SecretRedactor` 的 `redact_text` 处理 source 文本。

## journal 枚举
```rust
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TargetPhase { Pending, Snapshotted, Writing, RenamePending, Renamed, Written, Verified, TakeoverLinked, ..., #[serde(other)] Unknown }
impl TargetPhase { pub fn may_have_changed_target(self) -> bool { match self { /* 逐一列举，无 _ */ } } }
```
`JournalOperation` 同理；`journal_reports_crash` 改为 `matches!(op, JournalOperation::Crashed*)`。旧 journal 中未知字符串落到 `Unknown` 并按"可能已改变目标"保守处理。如 `09-10-backend-perf` 已把 journal 改为 JSONL，此处在其基础上改类型；两任务顺序为 perf 先。

## expect 清理
`parent_of` 返回 `AppError::internal("目标路径缺少父目录")`；`adapters/mod.rs` 的 JSON/TOML 期望改为 `ok_or_else`；`unreachable!` 改为 `return Err(AppError::internal(...))`。

## 命令样板
`commands/mod.rs`：
```rust
pub(crate) fn with_db<R>(state: &AppState, f: impl FnOnce(&mut Database) -> Result<R, AppError>) -> Result<R, AppError>
pub(crate) fn with_db_and_redactor<R>(...)
```
所有命令改为 `with_db(&state, |db| service::xxx(db, ...))`。

## 规范
`database-guidelines.md` 增加"迁移策略"节，明确禁止 `writable_schema`，给出表重建模板与校验 PRAGMA。

## 回滚
锁替换、错误 source、日志、journal 枚举、expect 清理、命令样板六段各自独立提交。
