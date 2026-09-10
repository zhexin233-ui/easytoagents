# 后端健壮性与可诊断性

父任务：`09-10-codebase-optimization`。覆盖审阅条目 D1、D2、D3、D4、D5，以及 D6 的规范约束部分。

## 目标
让 panic 不再导致应用永久失效，让线上问题可诊断，让 journal 状态机受编译器保护，并把迁移策略以规范固化。

## 需求
- R1 / D1：`AppState` 的 `Mutex/RwLock` 改为 `parking_lot`（不中毒），或统一 `unwrap_or_else(PoisonError::into_inner)`；`state_lock_error` 及其 `WriteInProgress` 误导映射随之删除或改为准确语义。
- R2 / D2：`AppError` 增加 `#[serde(skip)] source: Option<String>` 与 `with_source(impl Display)` 构造器，`AppError::database_from(path, op, &rusqlite::Error)` 等辅助函数；把 `db/`、`sync/`、`skills/`、`mcp/`、`hooks/`、`profiles/`、`projects/` 中的 `map_err(|_| ...)` 改为保留 source（至少覆盖数据库、文件 IO、序列化三类）。RPC 边界输出不变，脱敏合同不变。
- R3 / D2：引入 `tracing` + `tracing-subscriber`，输出到应用私有目录的滚动日志文件（受 `security` 模块权限管理），级别默认 `info`，可用环境变量 `EASYTOAGENTS_LOG` 调整；在所有 `with_source` 处 `warn!`，在 apply/restore 的阶段转换处 `info!`；journal 的 `rollback_failed` 附带脱敏后的错误码与 operation。日志不得写入任何 secret（复用 `SecretRedactor`）。同步更新 `logging-guidelines.md`。
- R4 / D3：journal 的 `phase`/`operation` 改为 `enum TargetPhase`、`enum JournalOperation`（`#[serde(rename_all = "snake_case")]`，含 `#[serde(other)] Unknown`），`mutation_may_have_changed_target` 与 `journal_reports_crash` 改为无通配的 `match`。序列化字符串与现在相同，旧 journal 可解析。
- R5 / D4：生产路径的 `expect`/`unreachable!`（清单见研究文件）全部替换为返回 `AppError::internal(...)`；`sync/apply.rs` 增加 `fn parent_of(&Path) -> Result<&Path, AppError>`。
- R6 / D5：`state_lock_error` 与加锁样板收敛到 `commands/mod.rs`；`AppState` 提供 `with_db`、`with_db_and_redactor` 闭包接口，命令函数体缩为一次调用。
- R7 / D6：不修改已发布迁移；在 `database-guidelines.md` 写入规则：后续迁移禁止 `writable_schema`，必须用 12 步表重建并在末尾 `PRAGMA foreign_key_check` + `integrity_check`；前置校验写进 `.sql` 而非 Rust 分支。

## 验收条件
- A1：一条测试在持锁线程 panic 后，后续命令仍能成功执行。
- A2：`rg "map_err\(\|_\|" src-tauri/src` 数量较基线（679）下降至少 70%；数据库、IO、序列化错误在日志中带 source。
- A3：日志文件在私有目录生成且权限为 0600；一条测试断言日志中不含注入的假 API key。
- A4：journal 枚举序列化与旧字符串逐一相等的测试；旧格式 journal 解析测试���
- A5：`rg "\.expect\(|unreachable!" src-tauri/src --glob '!**/tests*'` 在非测试代码中仅剩启动期 `lib.rs`。
- A6：`state_lock_error` 全仓库只有一处定义。
- A7：`pnpm rust:check`、`pnpm check` 全绿。

## 范围外
性能项、文件拆分、迁移重写。
