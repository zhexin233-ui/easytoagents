//! SQLite 初始化、前向迁移与事务边界。

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use uuid::Uuid;

use crate::{
    app::AppPaths,
    domain::Tool,
    error::AppError,
    security::{create_private_file, ensure_private_directory, ensure_private_file},
};

pub mod hooks;
pub mod mcp;
pub(crate) mod mcp_imports;
pub(crate) mod native_resources;
pub mod profiles;
pub mod projects;
pub(crate) mod skill_imports;
pub mod skills;
pub(crate) mod sync;

/// 数据库列 → `Tool` 的唯一解码入口。
///
/// 字符串到工具枚举的映射只允许经 `Tool::from_stable_str`，新增工具时
/// 只需改 `domain/mod.rs` 的 `string_enum!` 定义；非法列值沿用 db 层既有的
/// `InvalidQuery` 约定，由调用方包成 `AppError::database(...)`。
pub(crate) fn column_tool(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Tool> {
    let value: String = row.get(index)?;
    Tool::from_stable_str(&value).ok_or(rusqlite::Error::InvalidQuery)
}

pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        sql: include_str!("migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "snapshot_target_identity",
        sql: include_str!("migrations/0002_snapshot_target_identity.sql"),
    },
    Migration {
        version: 3,
        name: "profile_import_previews",
        sql: include_str!("migrations/0003_profile_import_previews.sql"),
    },
    Migration {
        version: 4,
        name: "project_registration",
        sql: include_str!("migrations/0004_project_registration.sql"),
    },
    Migration {
        version: 5,
        name: "mcp_import_previews",
        sql: include_str!("migrations/0005_mcp_import_previews.sql"),
    },
    Migration {
        version: 6,
        name: "skill_import_previews",
        sql: include_str!("migrations/0006_skill_import_previews.sql"),
    },
    Migration {
        version: 7,
        name: "app_settings",
        sql: include_str!("migrations/0007_app_settings.sql"),
    },
    Migration {
        version: 8,
        name: "prompt_project_assignments",
        sql: include_str!("migrations/0008_prompt_project_assignments.sql"),
    },
    Migration {
        version: 9,
        name: "prompt_tool_active_flags",
        sql: include_str!("migrations/0009_prompt_tool_active_flags.sql"),
    },
    Migration {
        version: 10,
        name: "cursor_tool_support",
        sql: include_str!("migrations/0010_cursor_tool_support.sql"),
    },
    Migration {
        version: 11,
        name: "snapshot_storage_kind",
        sql: include_str!("migrations/0011_snapshot_storage_kind.sql"),
    },
    Migration {
        version: 12,
        name: "project_native_resources",
        sql: include_str!("migrations/0012_project_native_resources.sql"),
    },
    Migration {
        version: 13,
        name: "zcode_tool_support",
        sql: include_str!("migrations/0013_zcode_tool_support.sql"),
    },
    Migration {
        version: 14,
        name: "hooks",
        sql: include_str!("migrations/0014_hooks.sql"),
    },
    Migration {
        version: 15,
        name: "hooks_scripts",
        sql: include_str!("migrations/0015_hooks_scripts.sql"),
    },
    Migration {
        version: 16,
        name: "hook_assignment_events",
        sql: include_str!("migrations/0016_hook_assignment_events.sql"),
    },
    Migration {
        version: 17,
        name: "cursor_prompt_support",
        sql: include_str!("migrations/0017_cursor_prompt_support.sql"),
    },
    Migration {
        version: 18,
        name: "remove_project_prompts",
        sql: include_str!("migrations/0018_remove_project_prompts.sql"),
    },
    Migration {
        version: 19,
        name: "opencode_tool_support",
        sql: include_str!("migrations/0019_opencode_tool_support.sql"),
    },
    Migration {
        version: 20,
        name: "github_skill_sources",
        sql: include_str!("migrations/0020_github_skill_sources.sql"),
    },
    Migration {
        version: 21,
        name: "project_native_hook_entries",
        sql: include_str!("migrations/0021_project_native_hook_entries.sql"),
    },
];

pub(crate) struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseBackup {
    pub directory: PathBuf,
    pub files: Vec<PathBuf>,
}

pub struct Database {
    connection: Connection,
    path: PathBuf,
    startup_backup: Option<DatabaseBackup>,
}

impl Database {
    /// 打开数据库；只有存在待执行迁移时才在迁移前备份主文件及 WAL/SHM。
    /// 备份前先做 WAL checkpoint，让备份出的单个主文件自洽可恢复；最多保留
    /// 最近 `STARTUP_BACKUP_RETENTION` 份，旧备份自动裁剪（裁剪失败不阻断启动）。
    pub fn open(paths: &AppPaths) -> Result<Self, AppError> {
        paths.ensure_directories()?;
        prepare_database_file(paths.database())?;

        let mut connection = Connection::open(paths.database()).map_err(|error| {
            AppError::database(&paths.database().to_string_lossy(), "open").with_source(error)
        })?;
        // PRAGMA 是连接级状态，迁移事务提交不会重置它们；只需在打开时配置一次。
        configure_connection(&connection, paths.database())?;
        let applied_migrations = count_applied_migrations(&connection, paths.database())?;
        let startup_backup = if applied_migrations > 0 && applied_migrations < MIGRATIONS.len() {
            checkpoint_wal(&connection, paths.database())?;
            let backup = backup_database_before_migrations(paths)?;
            prune_startup_backups(paths.database_backups(), backup.as_ref());
            backup
        } else {
            None
        };
        run_migrations(&mut connection, paths.database())?;
        process_retired_snapshot_cleanup(&connection, paths)?;

        for sensitive_file in [
            paths.database().to_owned(),
            paths.database_wal(),
            paths.database_shm(),
        ] {
            if path_entry_exists(&sensitive_file)? {
                ensure_private_file(&sensitive_file)?;
            }
        }

        Ok(Self {
            connection,
            path: paths.database().to_owned(),
            startup_backup,
        })
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    /// 仅供 crate-level 集成测试读取隔离数据库；生产代码使用 db 子模块查询。
    #[cfg(debug_assertions)]
    pub fn with_connection_for_tests<T>(&self, operation: impl FnOnce(&Connection) -> T) -> T {
        operation(&self.connection)
    }

    /// 在同一连接上执行一个 IMMEDIATE 事务，统一 begin/commit 错误边界。
    pub(crate) fn with_immediate_transaction<T, F>(&mut self, operation: F) -> Result<T, AppError>
    where
        F: FnOnce(&Transaction<'_>) -> Result<T, AppError>,
    {
        let database_path = self.path.to_string_lossy().into_owned();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                AppError::database(&database_path, "begin_immediate_transaction").with_source(error)
            })?;
        let value = operation(&transaction)?;
        transaction.commit().map_err(|error| {
            AppError::database(&database_path, "commit_immediate_transaction").with_source(error)
        })?;
        Ok(value)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn startup_backup(&self) -> Option<&DatabaseBackup> {
        self.startup_backup.as_ref()
    }

    pub fn schema_version(&self) -> Result<i64, AppError> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(|error| {
                AppError::database(&self.path.to_string_lossy(), "read_schema_version")
                    .with_source(error)
            })
    }
}

fn prepare_database_file(path: &Path) -> Result<(), AppError> {
    if path_entry_exists(path)? {
        ensure_private_file(path)
    } else {
        create_private_file(path).map(drop)
    }
}

fn configure_connection(connection: &Connection, path: &Path) -> Result<(), AppError> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA recursive_triggers = OFF;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|error| {
            AppError::database(&path.to_string_lossy(), "configure_pragmas").with_source(error)
        })?;
    let journal_mode = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| {
            AppError::database(&path.to_string_lossy(), "enable_wal").with_source(error)
        })?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(AppError::database(&path.to_string_lossy(), "verify_wal"));
    }
    let foreign_keys = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
        .map_err(|error| {
            AppError::database(&path.to_string_lossy(), "verify_foreign_keys").with_source(error)
        })?;
    if foreign_keys != 1 {
        return Err(AppError::database(
            &path.to_string_lossy(),
            "verify_foreign_keys",
        ));
    }
    Ok(())
}

/// 已应用的迁移条数；`schema_migrations` 尚不存在（全新库）时为 0。
fn count_applied_migrations(connection: &Connection, path: &Path) -> Result<usize, AppError> {
    let table_exists = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| AppError::database(&path.to_string_lossy(), "read_schema_migrations").with_source(error))?;
    if table_exists == 0 {
        return Ok(0);
    }
    connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| usize::try_from(count).unwrap_or_default())
        .map_err(|error| {
            AppError::database(&path.to_string_lossy(), "read_schema_migrations").with_source(error)
        })
}

/// 把 WAL 中的页写回主文件并截断 WAL，使随后复制出的主文件不依赖 WAL 即可打开。
fn checkpoint_wal(connection: &Connection, path: &Path) -> Result<(), AppError> {
    connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(drop)
        .map_err(|error| {
            AppError::database(&path.to_string_lossy(), "checkpoint_wal").with_source(error)
        })
}

const STARTUP_BACKUP_RETENTION: usize = 3;

/// 只保留最新的 `STARTUP_BACKUP_RETENTION` 个 `startup-*` 备份目录（含本次）。
/// 备份含 Provider 凭据副本，无限累积既占空间也扩大泄露面。裁剪是尽力而为：
/// 任何失败都不影响启动，下次打开会再次尝试。
fn prune_startup_backups(root: &Path, current: Option<&DatabaseBackup>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut directories = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
                && entry.file_name().to_string_lossy().starts_with("startup-")
        })
        .map(|entry| entry.path())
        .filter(|path| current.map_or(true, |backup| backup.directory != *path))
        .collect::<Vec<_>>();
    // 目录名以毫秒时间戳命名，字典序即时间序。
    directories.sort();
    let keep = STARTUP_BACKUP_RETENTION.saturating_sub(usize::from(current.is_some()));
    let excess = directories.len().saturating_sub(keep);
    for stale in directories.into_iter().take(excess) {
        let _ = fs::remove_dir_all(stale);
    }
}

fn run_migrations(connection: &mut Connection, path: &Path) -> Result<(), AppError> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )
        .map_err(|error| AppError::migration(&path.to_string_lossy(), 0).with_source(error))?;

    let applied_migrations = {
        let mut statement = connection
            .prepare_cached("SELECT version, name FROM schema_migrations ORDER BY version")
            .map_err(|error| AppError::migration(&path.to_string_lossy(), 0).with_source(error))?;
        let applied = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| AppError::migration(&path.to_string_lossy(), 0).with_source(error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::migration(&path.to_string_lossy(), 0).with_source(error))?;
        applied
    };
    for (index, (version, name)) in applied_migrations.iter().enumerate() {
        let Some(expected) = MIGRATIONS.get(index) else {
            return Err(AppError::migration(&path.to_string_lossy(), *version));
        };
        if *version != expected.version || name != expected.name {
            return Err(AppError::migration(&path.to_string_lossy(), *version));
        }
    }

    for migration in MIGRATIONS.iter().skip(applied_migrations.len()) {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
            })?;
        validate_migration_preconditions(&transaction, migration, path)?;
        transaction.execute_batch(migration.sql).map_err(|error| {
            AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
        })?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                params![migration.version, migration.name],
            )
            .map_err(|error| {
                AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
            })?;
        transaction.commit().map_err(|error| {
            AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
        })?;
        // 迁移可能原地修订 sqlite_schema 文本（writable_schema）；这类修订不会推进
        // schema cookie，本连接会继续持有陈旧 schema 缓存。每次迁移提交后显式推进
        // cookie，强制该连接与所有缓存语句重新解析最新 schema。
        let schema_version: i64 = connection
            .query_row("PRAGMA schema_version", [], |row| row.get(0))
            .map_err(|error| {
                AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
            })?;
        connection
            .pragma_update(None, "schema_version", schema_version + 1)
            .map_err(|error| {
                AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
            })?;
    }
    Ok(())
}

/// writable_schema 迁移必须在改写前证明每个精确旧锚点都存在；否则 replace
/// 会静默更新 0 行，却仍可能把 schema version 推进到新版本。
fn validate_migration_preconditions(
    transaction: &Transaction<'_>,
    migration: &Migration,
    path: &Path,
) -> Result<(), AppError> {
    if migration.version == 10 {
        const SHARED_TOOL_ANCHOR: &str = "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex'))";
        const MANAGED_TARGET_ANCHOR: &str =
            "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),";
        for table in [
            "mcp_global_assignments",
            "skill_global_assignments",
            "mcp_project_assignments",
            "skill_project_assignments",
            "mcp_import_previews",
            "skill_import_previews",
        ] {
            let matched: i64 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = ?1 AND sql IS NOT NULL
                       AND (length(sql) - length(replace(sql, ?2, ''))) = length(?2)",
                    params![table, SHARED_TOOL_ANCHOR],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    AppError::migration(&path.to_string_lossy(), migration.version)
                        .with_source(error)
                })?;
            if matched != 1 {
                return Err(AppError::migration(
                    &path.to_string_lossy(),
                    migration.version,
                ));
            }
        }

        let managed_target_matched: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'managed_targets' AND sql IS NOT NULL
                   AND (length(sql) - length(replace(sql, ?1, ''))) = length(?1)",
                [MANAGED_TARGET_ANCHOR],
                |row| row.get(0),
            )
            .map_err(|error| {
                AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
            })?;
        if managed_target_matched != 1 {
            return Err(AppError::migration(
                &path.to_string_lossy(),
                migration.version,
            ));
        }
        return Ok(());
    }

    if migration.version == 18 {
        const PROJECT_PROMPT_SCOPE_ANCHOR: &str =
            "artifact_kind IN ('mcp', 'skill', 'prompt', 'hook'))";
        const NATIVE_PROMPT_ENTRY_ANCHOR: &str =
            "entry_type IN ('mcp_entry', 'directory', 'symlink', 'prompt_file')";

        for (table, anchor) in [
            ("managed_targets", PROJECT_PROMPT_SCOPE_ANCHOR),
            ("project_native_resources", NATIVE_PROMPT_ENTRY_ANCHOR),
        ] {
            let matched: i64 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = ?1 AND sql IS NOT NULL
                       AND (length(sql) - length(replace(sql, ?2, ''))) = length(?2)",
                    params![table, anchor],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    AppError::migration(&path.to_string_lossy(), migration.version)
                        .with_source(error)
                })?;
            if matched != 1 {
                return Err(AppError::migration(
                    &path.to_string_lossy(),
                    migration.version,
                ));
            }
        }

        let assignments_table: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'prompt_project_assignments'",
                [],
                |row| row.get(0),
            )
            .map_err(|error| {
                AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
            })?;
        if assignments_table != 1 {
            return Err(AppError::migration(
                &path.to_string_lossy(),
                migration.version,
            ));
        }
    }
    if migration.version == 19 {
        // 早期 v17 已开放 Cursor，但尚未加入 tool×artifact 组合约束。
        // 在同一迁移事务内将这个已知旧锚点规范化，再执行下方的严格校验；
        // 失败时连同 schema 文本一起回滚，不修改已应用的迁移历史。
        transaction
            .execute_batch(
                "PRAGMA writable_schema = ON;
                 UPDATE sqlite_master SET sql = replace(sql,
                   'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'')),',
                   'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'') AND (tool != ''cursor'' OR artifact_kind = ''prompt'')),')
                 WHERE type = 'table' AND name = 'profile_import_previews'
                   AND instr(sql, 'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex'', ''zcode'', ''cursor'')),') > 0;
                 PRAGMA writable_schema = OFF;",
            )
            .map_err(|error| AppError::migration(&path.to_string_lossy(), migration.version).with_source(error))?;
        let anchors = [
            (
                "mcp_global_assignments",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode'))",
            ),
            (
                "skill_global_assignments",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode'))",
            ),
            (
                "mcp_project_assignments",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode'))",
            ),
            (
                "skill_project_assignments",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode'))",
            ),
            (
                "mcp_import_previews",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode'))",
            ),
            (
                "skill_import_previews",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode'))",
            ),
            (
                "managed_targets",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode') AND (tool != 'cursor' OR artifact_kind IN ('mcp', 'skill', 'hook', 'prompt'))),",
            ),
            (
                "profile_import_previews",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'zcode', 'cursor') AND (tool != 'cursor' OR artifact_kind = 'prompt')),",
            ),
            (
                "provider_profiles",
                "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'zcode'))",
            ),
        ];
        for (table, anchor) in anchors {
            let matched: i64 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = ?1 AND sql IS NOT NULL
                       AND instr(sql, ?2) > 0",
                    params![table, anchor],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    AppError::migration(&path.to_string_lossy(), migration.version)
                        .with_source(error)
                })?;
            if matched != 1 {
                return Err(AppError::migration(
                    &path.to_string_lossy(),
                    migration.version,
                ));
            }
        }

        const SHARED_TOOL_ANCHOR: &str =
            "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode'))";
        const MANAGED_TARGET_ANCHOR: &str = "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'cursor', 'zcode') AND (tool != 'cursor' OR artifact_kind IN ('mcp', 'skill', 'hook', 'prompt'))),";
        const PROFILE_IMPORT_ANCHOR: &str = "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'zcode', 'cursor') AND (tool != 'cursor' OR artifact_kind = 'prompt')),";
        const PROVIDER_ANCHOR: &str =
            "tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex', 'zcode'))";

        for table in [
            "mcp_global_assignments",
            "skill_global_assignments",
            "mcp_project_assignments",
            "skill_project_assignments",
            "mcp_import_previews",
            "skill_import_previews",
        ] {
            let matched: i64 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = ?1 AND sql IS NOT NULL
                       AND (length(sql) - length(replace(sql, ?2, ''))) = length(?2)",
                    params![table, SHARED_TOOL_ANCHOR],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    AppError::migration(&path.to_string_lossy(), migration.version)
                        .with_source(error)
                })?;
            if matched != 1 {
                return Err(AppError::migration(
                    &path.to_string_lossy(),
                    migration.version,
                ));
            }
        }

        for (table, anchor) in [
            ("managed_targets", MANAGED_TARGET_ANCHOR),
            ("profile_import_previews", PROFILE_IMPORT_ANCHOR),
            ("provider_profiles", PROVIDER_ANCHOR),
        ] {
            let matched: i64 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = ?1 AND sql IS NOT NULL
                       AND (length(sql) - length(replace(sql, ?2, ''))) = length(?2)",
                    params![table, anchor],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    AppError::migration(&path.to_string_lossy(), migration.version)
                        .with_source(error)
                })?;
            if matched != 1 {
                return Err(AppError::migration(
                    &path.to_string_lossy(),
                    migration.version,
                ));
            }
        }
    }
    if migration.version == 20 {
        const SOURCE_PATH_ANCHOR: &str = "source_path TEXT NOT NULL CHECK(\n        source_path LIKE '/%' AND source_path != '/' AND instr(source_path, '//') = 0\n        AND source_path NOT LIKE '%/../%' AND source_path NOT LIKE '%/./%'\n        AND source_path NOT LIKE '%/..' AND source_path NOT LIKE '%/.'\n        AND substr(source_path, -1) != '/'\n    )";
        let matched: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'skills' AND sql IS NOT NULL
                   AND (length(sql) - length(replace(sql, ?1, ''))) = length(?1)",
                [SOURCE_PATH_ANCHOR],
                |row| row.get(0),
            )
            .map_err(|error| {
                AppError::migration(&path.to_string_lossy(), migration.version).with_source(error)
            })?;
        if matched != 1 {
            return Err(AppError::migration(
                &path.to_string_lossy(),
                migration.version,
            ));
        }
    }
    Ok(())
}

/// 迁移提交后退休历史 Prompt 快照。数据库迁移只记录绝对路径和身份；
/// 这里再次验证路径形状、父目录和文件类型，任何越界、链接或特殊文件都
/// 保留在队列中等待人工/后续版本处理，不触碰 managed_targets.target_path。
fn process_retired_snapshot_cleanup(
    connection: &Connection,
    paths: &AppPaths,
) -> Result<(), AppError> {
    let database_path = paths.database().to_string_lossy().into_owned();
    let mut statement = connection
        .prepare_cached(
            "SELECT snapshot_id, run_id, snapshot_path, storage_kind
             FROM retired_snapshot_cleanup ORDER BY retired_at, snapshot_id",
        )
        .map_err(|error| {
            AppError::database(&database_path, "read_retired_snapshot_cleanup").with_source(error)
        })?;
    let entries = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| {
            AppError::database(&database_path, "read_retired_snapshot_cleanup").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&database_path, "read_retired_snapshot_cleanup").with_source(error)
        })?;
    drop(statement);

    for (snapshot_id, run_id, snapshot_path, storage_kind) in entries {
        let Some(path) =
            retired_snapshot_path(paths, &snapshot_id, &run_id, &snapshot_path, &storage_kind)
        else {
            continue;
        };

        let completed = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => false,
            Ok(_) => fs::remove_file(&path).is_ok(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(_) => false,
        };
        if completed {
            connection
                .execute(
                    "DELETE FROM retired_snapshot_cleanup WHERE snapshot_id = ?1",
                    [&snapshot_id],
                )
                .map_err(|error| {
                    AppError::database(&database_path, "delete_retired_snapshot_cleanup")
                        .with_source(error)
                })?;
        }
    }
    Ok(())
}

fn retired_snapshot_path(
    paths: &AppPaths,
    snapshot_id: &str,
    run_id: &str,
    snapshot_path: &str,
    storage_kind: &str,
) -> Option<PathBuf> {
    if storage_kind != "payload_file"
        || Uuid::parse_str(snapshot_id).is_err()
        || Uuid::parse_str(run_id).is_err()
    {
        return None;
    }
    let expected = paths
        .snapshots()
        .join(run_id)
        .join(format!("{snapshot_id}.snapshot"));
    if Path::new(snapshot_path) != expected
        || crate::security::reject_symlink_components(&expected).is_err()
    {
        return None;
    }
    Some(expected)
}

fn backup_database_before_migrations(paths: &AppPaths) -> Result<Option<DatabaseBackup>, AppError> {
    if !path_entry_exists(paths.database())? {
        return Ok(None);
    }
    ensure_private_file(paths.database())?;

    let backup_directory = unique_backup_directory(paths.database_backups())?;
    ensure_private_directory(&backup_directory)?;
    let mut files = Vec::new();
    for source in [
        paths.database().to_owned(),
        paths.database_wal(),
        paths.database_shm(),
    ] {
        if !path_entry_exists(&source)? {
            continue;
        }
        ensure_private_file(&source)?;
        let destination =
            backup_directory.join(source.file_name().ok_or_else(|| {
                AppError::database(&source.to_string_lossy(), "backup_file_name")
            })?);
        copy_private_file(&source, &destination)?;
        files.push(destination);
    }

    Ok(Some(DatabaseBackup {
        directory: backup_directory,
        files,
    }))
}

fn unique_backup_directory(root: &Path) -> Result<PathBuf, AppError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for suffix in 0..1000_u16 {
        let name = if suffix == 0 {
            format!("startup-{timestamp}")
        } else {
            format!("startup-{timestamp}-{suffix}")
        };
        let candidate = root.join(name);
        if !path_entry_exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(AppError::database(
        &root.to_string_lossy(),
        "allocate_backup_directory",
    ))
}

fn path_entry_exists(path: &Path) -> Result<bool, AppError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::database(&path.to_string_lossy(), "lstat").with_source(error)),
    }
}

fn copy_private_file(source: &Path, destination: &Path) -> Result<(), AppError> {
    let mut input = File::open(source).map_err(|error| {
        AppError::database(&source.to_string_lossy(), "open_backup_source").with_source(error)
    })?;
    let mut output = create_private_file(destination)?;
    io::copy(&mut input, &mut output).map_err(|error| {
        AppError::database(&destination.to_string_lossy(), "copy_backup").with_source(error)
    })?;
    output.flush().map_err(|error| {
        AppError::database(&destination.to_string_lossy(), "flush_backup").with_source(error)
    })?;
    output.sync_all().map_err(|error| {
        AppError::database(&destination.to_string_lossy(), "sync_backup").with_source(error)
    })?;
    ensure_private_file(destination)
}

#[cfg(test)]
include!("tests.rs");
