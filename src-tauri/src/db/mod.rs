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

        let mut connection = Connection::open(paths.database())
            .map_err(|_| AppError::database(&paths.database().to_string_lossy(), "open"))?;
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

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
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
            .map_err(|_| AppError::database(&self.path.to_string_lossy(), "read_schema_version"))
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
        .map_err(|_| AppError::database(&path.to_string_lossy(), "configure_pragmas"))?;
    let journal_mode = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| AppError::database(&path.to_string_lossy(), "enable_wal"))?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(AppError::database(&path.to_string_lossy(), "verify_wal"));
    }
    let foreign_keys = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
        .map_err(|_| AppError::database(&path.to_string_lossy(), "verify_foreign_keys"))?;
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
        .map_err(|_| AppError::database(&path.to_string_lossy(), "read_schema_migrations"))?;
    if table_exists == 0 {
        return Ok(0);
    }
    connection
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| usize::try_from(count).unwrap_or_default())
        .map_err(|_| AppError::database(&path.to_string_lossy(), "read_schema_migrations"))
}

/// 把 WAL 中的页写回主文件并截断 WAL，使随后复制出的主文件不依赖 WAL 即可打开。
fn checkpoint_wal(connection: &Connection, path: &Path) -> Result<(), AppError> {
    connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(drop)
        .map_err(|_| AppError::database(&path.to_string_lossy(), "checkpoint_wal"))
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
        .map_err(|_| AppError::migration(&path.to_string_lossy(), 0))?;

    let applied_migrations = {
        let mut statement = connection
            .prepare_cached("SELECT version, name FROM schema_migrations ORDER BY version")
            .map_err(|_| AppError::migration(&path.to_string_lossy(), 0))?;
        let applied = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| AppError::migration(&path.to_string_lossy(), 0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::migration(&path.to_string_lossy(), 0))?;
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
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
        validate_migration_preconditions(&transaction, migration, path)?;
        transaction
            .execute_batch(migration.sql)
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                params![migration.version, migration.name],
            )
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
        transaction
            .commit()
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
        // 迁移可能原地修订 sqlite_schema 文本（writable_schema）；这类修订不会推进
        // schema cookie，本连接会继续持有陈旧 schema 缓存。每次迁移提交后显式推进
        // cookie，强制该连接与所有缓存语句重新解析最新 schema。
        let schema_version: i64 = connection
            .query_row("PRAGMA schema_version", [], |row| row.get(0))
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
        connection
            .pragma_update(None, "schema_version", schema_version + 1)
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
                .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
                .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
                .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
                .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
                .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
            .map_err(|_| AppError::migration(&path.to_string_lossy(), migration.version))?;
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
        .map_err(|_| AppError::database(&database_path, "read_retired_snapshot_cleanup"))?;
    let entries = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|_| AppError::database(&database_path, "read_retired_snapshot_cleanup"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&database_path, "read_retired_snapshot_cleanup"))?;
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
                .map_err(|_| {
                    AppError::database(&database_path, "delete_retired_snapshot_cleanup")
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
        Err(_) => Err(AppError::database(&path.to_string_lossy(), "lstat")),
    }
}

fn copy_private_file(source: &Path, destination: &Path) -> Result<(), AppError> {
    let mut input = File::open(source)
        .map_err(|_| AppError::database(&source.to_string_lossy(), "open_backup_source"))?;
    let mut output = create_private_file(destination)?;
    io::copy(&mut input, &mut output)
        .map_err(|_| AppError::database(&destination.to_string_lossy(), "copy_backup"))?;
    output
        .flush()
        .map_err(|_| AppError::database(&destination.to_string_lossy(), "flush_backup"))?;
    output
        .sync_all()
        .map_err(|_| AppError::database(&destination.to_string_lossy(), "sync_backup"))?;
    ensure_private_file(destination)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fs,
        os::unix::fs::{symlink, PermissionsExt},
    };

    use rusqlite::{params, Connection};
    use tempfile::tempdir;

    use super::Database;
    use crate::{
        app::AppPaths,
        error::ErrorCode,
        security::{mode, PRIVATE_DIRECTORY_MODE, PRIVATE_FILE_MODE},
    };

    const PROJECT_ONE_ID: &str = "00000000-0000-4000-8000-000000000001";
    const PROJECT_TWO_ID: &str = "00000000-0000-4000-8000-000000000002";
    const MCP_ID: &str = "00000000-0000-4000-8000-000000000003";
    const SKILL_ID: &str = "00000000-0000-4000-8000-000000000004";
    const TARGET_ONE_ID: &str = "00000000-0000-4000-8000-000000000005";
    const TARGET_TWO_ID: &str = "00000000-0000-4000-8000-000000000006";
    const RUN_ONE_ID: &str = "00000000-0000-4000-8000-000000000007";
    const RUN_TWO_ID: &str = "00000000-0000-4000-8000-000000000008";
    const MCP_TWO_ID: &str = "00000000-0000-4000-8000-000000000009";
    const SKILL_TWO_ID: &str = "00000000-0000-4000-8000-000000000010";

    fn open_isolated_database() -> (tempfile::TempDir, AppPaths, Database) {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("app-data")).unwrap();
        let database = Database::open(&paths).unwrap();
        (temporary, paths, database)
    }

    fn insert_project(connection: &Connection, id: &str, root_path: &str) {
        connection
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES (?1, ?2, ?3)",
                params![id, id, root_path],
            )
            .unwrap();
    }

    fn insert_mcp(connection: &Connection, id: &str, name: &str) {
        connection
            .execute(
                "INSERT INTO mcp_servers(id, name, transport, command) VALUES (?1, ?2, 'stdio', 'fixture-command')",
                params![id, name],
            )
            .unwrap();
    }

    fn insert_skill(connection: &Connection, id: &str, name: &str) {
        connection
            .execute(
                "INSERT INTO skills(id, name, source_path, central_path, content_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    name,
                    format!("/fixture/source/{id}"),
                    format!("/fixture/central/{id}"),
                    "a".repeat(64)
                ],
            )
            .unwrap();
    }

    #[test]
    fn github_skill_source_migration_accepts_only_normalized_source_shape() {
        let (_temporary, _paths, database) = open_isolated_database();
        assert_eq!(database.schema_version().unwrap(), 20);
        database
            .connection()
            .execute(
                "INSERT INTO skills(id, name, source_path, central_path, content_hash)
                 VALUES (?1, 'github-demo', ?2, '/fixture/central/github-demo', ?3)",
                params![
                    "00000000-0000-4000-8000-000000000020",
                    "https://github.com/acme/repo/tree/main/skills/demo",
                    "a".repeat(64),
                ],
            )
            .unwrap();
        assert!(database
            .connection()
            .execute(
                "INSERT INTO skills(id, name, source_path, central_path, content_hash)
                 VALUES (?1, 'github-bad', ?2, '/fixture/central/github-bad', ?3)",
                params![
                    "00000000-0000-4000-8000-000000000021",
                    "https://example.com/acme/repo/tree/main/skills/demo",
                    "b".repeat(64),
                ],
            )
            .is_err());
    }

    #[test]
    fn initializes_wal_foreign_keys_and_all_phase_one_tables() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(foreign_keys, 1);
        assert_eq!(database.schema_version().unwrap(), 20);
        let foreign_key_violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(foreign_key_violations, 0);

        let tables = connection
            .prepare_cached("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<HashSet<_>, _>>()
            .unwrap();
        for table in [
            "provider_profiles",
            "prompt_profiles",
            "mcp_servers",
            "skills",
            "projects",
            "mcp_global_assignments",
            "skill_global_assignments",
            "mcp_project_assignments",
            "skill_project_assignments",
            "managed_targets",
            "managed_items",
            "sync_runs",
            "sync_items",
            "snapshots",
            "profile_import_previews",
            "mcp_import_previews",
            "skill_import_previews",
            "onboarding_state",
            "project_native_resources",
        ] {
            assert!(tables.contains(table), "缺少表：{table}");
        }
    }

    #[test]
    fn database_enforces_active_profiles_and_case_insensitive_names() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        connection
            .execute(
                "INSERT INTO provider_profiles(id, tool, name, is_active) VALUES ('00000000-0000-4000-8000-000000000011', 'claude', 'Primary', 1)",
                [],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO provider_profiles(id, tool, name, is_active) VALUES ('00000000-0000-4000-8000-000000000012', 'claude', 'Second', 1)",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO provider_profiles(id, tool, name) VALUES ('00000000-0000-4000-8000-000000000016', 'claude', ?1)",
                ["invalid\nname"],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO provider_profiles(id, tool, name) VALUES ('00000000-0000-4000-8000-000000000013', 'claude', 'primary')",
                [],
            )
            .is_err());

        // 工具无关化后（迁移 0009）：每工具至多一份生效由 is_active_claude/codex
        // 部分唯一索引强制；遗留 tool 列放宽后新档案可写 'central'。
        connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body, is_active_codex) VALUES ('00000000-0000-4000-8000-000000000014', 'central', 'One', '', 1)",
                [],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body, is_active_codex) VALUES ('00000000-0000-4000-8000-000000000015', 'central', 'Two', '', 1)",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body) VALUES ('00000000-0000-4000-8000-000000000018', 'central', 'Three', '')",
                [],
            )
            .unwrap()
            == 1);
        assert!(connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body) VALUES ('not-a-uuid', 'central', 'Invalid ID', '')",
                [],
            )
            .is_err());
    }

    #[test]
    fn database_enforces_project_paths_and_uniqueness() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        insert_project(connection, PROJECT_ONE_ID, "/fixture/project");
        assert!(connection
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES ('00000000-0000-4000-8000-000000000022', 'Two', '/fixture/project')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES ('00000000-0000-4000-8000-000000000023', 'Three', 'relative/project')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES ('00000000-0000-4000-8000-000000000024', 'Four', '/fixture/../escape')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES ('00000000-0000-4000-8000-000000000025', 'Five', '/fixture/project/')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES ('00000000-0000-4000-8000-000000000026', 'Six', '/fixture//project')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES ('00000000-0000-4000-8000-000000000027', 'Root', '/')",
                [],
            )
            .is_err());
    }

    #[test]
    fn database_rejects_invalid_json_hashes_paths_and_foreign_keys() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        assert!(connection
            .execute(
                "INSERT INTO mcp_servers(id, name, transport, command, args_json) VALUES ('00000000-0000-4000-8000-000000000031', 'Invalid JSON', 'stdio', 'command', '{}')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO skills(id, name, source_path, central_path, content_hash) VALUES ('00000000-0000-4000-8000-000000000032', 'Invalid Hash', '/fixture/source', '/fixture/central', ?1)",
                ["g".repeat(64)],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES ('00000000-0000-4000-8000-000000000033', 'claude', 'provider', 'global', '/fixture/../settings.json')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash
                 ) VALUES (
                    '00000000-0000-4000-8000-000000000036', 'claude', 'provider',
                    'global', '/fixture/settings.json', ?1, NULL
                 )",
                ["a".repeat(64)],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO mcp_global_assignments(tool, mcp_id) VALUES ('claude', '00000000-0000-4000-8000-000000000034')",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, db_version, error_code) VALUES ('00000000-0000-4000-8000-000000000035', 'preview', 'failed', 'global', 1, 'UNSTABLE_ERROR')",
                [],
            )
            .is_err());
    }

    #[test]
    fn database_increments_row_versions_and_rejects_regression() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        insert_project(connection, PROJECT_ONE_ID, "/fixture/project");
        connection
            .execute(
                "UPDATE projects SET display_name = 'Renamed' WHERE id = ?1",
                [PROJECT_ONE_ID],
            )
            .unwrap();
        let row_version: i64 = connection
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [PROJECT_ONE_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(row_version, 2);
        connection
            .execute(
                "UPDATE projects SET display_name = 'Renamed Again', updated_at = '2099-01-01T00:00:00Z' WHERE id = ?1",
                [PROJECT_ONE_ID],
            )
            .unwrap();
        let row_version: i64 = connection
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [PROJECT_ONE_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(row_version, 3);
        assert!(connection
            .execute(
                "UPDATE projects SET row_version = 1 WHERE id = ?1",
                [PROJECT_ONE_ID],
            )
            .is_err());
    }

    #[test]
    fn database_enforces_assignment_inheritance_in_both_directions() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        insert_project(connection, PROJECT_ONE_ID, "/fixture/project");
        insert_mcp(connection, MCP_ID, "Fixture MCP");
        insert_mcp(connection, MCP_TWO_ID, "Fixture MCP Two");
        insert_skill(connection, SKILL_ID, "Fixture Skill");
        insert_skill(connection, SKILL_TWO_ID, "Fixture Skill Two");

        connection
            .execute(
                "INSERT INTO mcp_global_assignments(tool, mcp_id) VALUES ('claude', ?1)",
                [MCP_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO mcp_project_assignments(project_id, tool, mcp_id) VALUES (?1, 'claude', ?2)",
                params![PROJECT_ONE_ID, MCP_ID],
            )
            .is_err());
        connection
            .execute(
                "INSERT INTO mcp_project_assignments(project_id, tool, mcp_id) VALUES (?1, 'claude', ?2)",
                params![PROJECT_ONE_ID, MCP_TWO_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "UPDATE mcp_project_assignments SET mcp_id = ?1 WHERE project_id = ?2 AND tool = 'claude' AND mcp_id = ?3",
                params![MCP_ID, PROJECT_ONE_ID, MCP_TWO_ID],
            )
            .is_err());
        assert!(connection
            .execute(
                "UPDATE mcp_global_assignments SET mcp_id = ?1 WHERE tool = 'claude' AND mcp_id = ?2",
                params![MCP_TWO_ID, MCP_ID],
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO skill_project_assignments(project_id, tool, skill_id) VALUES (?1, 'codex', ?2)",
                params![PROJECT_ONE_ID, SKILL_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO skill_global_assignments(tool, skill_id) VALUES ('codex', ?1)",
                [SKILL_ID],
            )
            .is_err());
        connection
            .execute(
                "INSERT INTO skill_global_assignments(tool, skill_id) VALUES ('codex', ?1)",
                [SKILL_TWO_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "UPDATE skill_project_assignments SET skill_id = ?1 WHERE project_id = ?2 AND tool = 'codex' AND skill_id = ?3",
                params![SKILL_TWO_ID, PROJECT_ONE_ID, SKILL_ID],
            )
            .is_err());
        assert!(connection
            .execute(
                "UPDATE skill_global_assignments SET skill_id = ?1 WHERE tool = 'codex' AND skill_id = ?2",
                params![SKILL_ID, SKILL_TWO_ID],
            )
            .is_err());
    }

    #[test]
    fn managed_targets_and_active_writer_use_null_safe_unique_constraints() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        connection
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'claude', 'provider', 'global', '/fixture/settings.json')",
                [TARGET_ONE_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'claude', 'provider', 'global', '/fixture/settings.json')",
                [TARGET_TWO_ID],
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, db_version) VALUES (?1, 'apply', 'applying', 'global', 1)",
                [RUN_ONE_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, db_version) VALUES (?1, 'restore', 'restoring', 'global', 1)",
                [RUN_TWO_ID],
            )
            .is_err());
    }

    #[test]
    fn managed_items_and_sync_items_must_match_their_parent_contracts() {
        let (_temporary, _paths, database) = open_isolated_database();
        let connection = database.connection();
        insert_project(connection, PROJECT_ONE_ID, "/fixture/project");
        connection
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'claude', 'provider', 'global', '/fixture/settings.json')",
                [TARGET_ONE_ID],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path) VALUES (?1, 'claude', 'mcp', 'project', ?2, '/fixture/project/.mcp.json')",
                params![TARGET_TWO_ID, PROJECT_ONE_ID],
            )
            .unwrap();

        let item_id = "00000000-0000-4000-8000-000000000041";
        assert!(connection
            .execute(
                "INSERT INTO managed_items(id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash) VALUES (?1, ?2, 'mcp', ?3, 'wrong-kind', ?4)",
                params![item_id, TARGET_ONE_ID, MCP_ID, "a".repeat(64)],
            )
            .is_err());
        connection
            .execute(
                "INSERT INTO managed_items(id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash) VALUES (?1, ?2, 'provider', ?3, 'provider-key', ?4)",
                params![item_id, TARGET_ONE_ID, MCP_ID, "a".repeat(64)],
            )
            .unwrap();
        assert!(connection
            .execute(
                "UPDATE managed_items SET resource_kind = 'mcp' WHERE id = ?1",
                [item_id],
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, db_version) VALUES (?1, 'preview', 'previewed', 'global', 1)",
                [RUN_ONE_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO sync_items(id, run_id, target_id, change_kind, status) VALUES ('00000000-0000-4000-8000-000000000042', ?1, ?2, 'update', 'in_sync')",
                params![RUN_ONE_ID, TARGET_TWO_ID],
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version) VALUES (?1, 'preview', 'previewed', 'project', ?2, 1)",
                params![RUN_TWO_ID, PROJECT_ONE_ID],
            )
            .unwrap();
        let sync_item_id = "00000000-0000-4000-8000-000000000043";
        connection
            .execute(
                "INSERT INTO sync_items(id, run_id, target_id, change_kind, status) VALUES (?1, ?2, ?3, 'update', 'in_sync')",
                params![sync_item_id, RUN_TWO_ID, TARGET_TWO_ID],
            )
            .unwrap();
        assert!(connection
            .execute(
                "UPDATE sync_items SET target_id = ?1 WHERE id = ?2",
                params![TARGET_ONE_ID, sync_item_id],
            )
            .is_err());
    }

    #[test]
    fn startup_restricts_database_files_and_creates_a_private_backup() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("app-data")).unwrap();
        // 备份只在有待执行迁移时发生：用只应用前 4 个迁移的旧库来模拟升级。
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..4] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            connection
                .execute(
                    "INSERT INTO projects(id, display_name, root_path) VALUES (?1, 'Kept', '/fixture/kept')",
                    [PROJECT_TWO_ID],
                )
                .unwrap();
        }

        fs::set_permissions(paths.database(), fs::Permissions::from_mode(0o644)).unwrap();
        let reopened = Database::open(&paths).unwrap();
        let backup = reopened
            .startup_backup()
            .expect("有待执行迁移的数据库必须先备份");
        assert_eq!(mode(paths.database()).unwrap(), PRIVATE_FILE_MODE);
        for companion in [paths.database_wal(), paths.database_shm()] {
            if companion.exists() {
                assert_eq!(mode(&companion).unwrap(), PRIVATE_FILE_MODE);
            }
        }
        assert_eq!(mode(&backup.directory).unwrap(), PRIVATE_DIRECTORY_MODE);
        assert!(!backup.files.is_empty());
        for file in &backup.files {
            assert_eq!(mode(file).unwrap(), PRIVATE_FILE_MODE);
        }

        let backup_database_path = backup
            .files
            .iter()
            .find(|path| path.file_name() == paths.database().file_name())
            .unwrap();
        let backup_connection = Connection::open(backup_database_path).unwrap();
        let count: i64 = backup_connection
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE id = ?1",
                [PROJECT_TWO_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn startup_backup_only_runs_when_migrations_are_pending() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("no-pending-data")).unwrap();
        {
            let database = Database::open(&paths).unwrap();
            assert!(
                database.startup_backup().is_none(),
                "全新库没有可备份的旧状态"
            );
        }
        // 已是最新 schema：再次打开不产生任何备份目录。
        let reopened = Database::open(&paths).unwrap();
        assert!(reopened.startup_backup().is_none());
        assert_eq!(fs::read_dir(paths.database_backups()).unwrap().count(), 0);
    }

    #[test]
    fn startup_backup_checkpoints_active_wal_and_prunes_to_three_directories() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("prune-data")).unwrap();
        paths.initialize().unwrap();
        // 预先放四个历史备份目录；只有最新两个 + 本次应被保留。
        for name in ["startup-100", "startup-200", "startup-300", "startup-400"] {
            fs::create_dir_all(paths.database_backups().join(name)).unwrap();
        }
        fs::write(paths.database_backups().join("unrelated.txt"), "keep").unwrap();

        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..4] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            // 关闭自动 checkpoint，让行留在 WAL 里，验证备份前会先 checkpoint。
            connection
                .execute_batch("PRAGMA wal_autocheckpoint = 0;")
                .unwrap();
            insert_mcp(&connection, MCP_ID, "In WAL");
            assert!(paths.database_wal().is_file());
            assert!(fs::metadata(paths.database_wal()).unwrap().len() > 0);
        }

        let database = Database::open(&paths).unwrap();
        let backup = database.startup_backup().expect("有待执行迁移时必须备份");
        let backup_database_path = backup
            .files
            .iter()
            .find(|path| path.file_name() == paths.database().file_name())
            .unwrap();
        // 备份出的单个主文件自洽：不依赖 WAL 即可读到 checkpoint 之前的行。
        let backup_connection = Connection::open_with_flags(
            backup_database_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let name: String = backup_connection
            .query_row(
                "SELECT name FROM mcp_servers WHERE id = ?1",
                [MCP_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "In WAL");

        let mut directories = fs::read_dir(paths.database_backups())
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        directories.sort();
        assert_eq!(directories.len(), 3, "最多保留 3 份启动备份");
        assert!(!directories.iter().any(|name| name == "startup-100"));
        assert!(!directories.iter().any(|name| name == "startup-200"));
        assert!(directories.iter().any(|name| name == "startup-300"));
        assert!(directories.iter().any(|name| name == "startup-400"));
        assert!(directories.contains(
            &backup
                .directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        ));
        assert!(paths.database_backups().join("unrelated.txt").is_file());
        assert_eq!(mode(&backup.directory).unwrap(), PRIVATE_DIRECTORY_MODE);
    }

    #[test]
    fn startup_rejects_broken_or_external_database_sidecar_symlinks() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("symlink-data")).unwrap();
        {
            let _database = Database::open(&paths).unwrap();
        }
        let missing_outside = isolated_root.join("missing-outside-wal");
        symlink(&missing_outside, paths.database_wal()).unwrap();

        assert!(Database::open(&paths).is_err());
        assert!(!missing_outside.exists());
    }

    #[test]
    fn mcp_import_migration_upgrades_v4_and_reopens_without_losing_central_rows() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("upgrade-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE,
                    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
                )
                .unwrap();
            for migration in &super::MIGRATIONS[..4] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_mcp(&connection, MCP_ID, "Existing MCP");
        }
        for (iteration, _) in (0..2).enumerate() {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            // 只有第一次打开有待执行迁移，才产生启动备份。
            assert_eq!(database.startup_backup().is_some(), iteration == 0);
            let (name, previews): (String, i64) = database.connection().query_row(
                "SELECT name, (SELECT COUNT(*) FROM mcp_import_previews) FROM mcp_servers WHERE id = ?1",
                [MCP_ID], |row| Ok((row.get(0)?, row.get(1)?)),
            ).unwrap();
            assert_eq!(name, "Existing MCP");
            assert_eq!(previews, 0);
        }
    }

    #[test]
    fn skill_import_migration_upgrades_v5_and_reopens() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v5-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..5] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_mcp(&connection, MCP_ID, "Preserved MCP");
        }
        for _ in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let (name, previews): (String, i64) = database.connection().query_row("SELECT name, (SELECT COUNT(*) FROM skill_import_previews) FROM mcp_servers WHERE id = ?1", [MCP_ID], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
            assert_eq!(name, "Preserved MCP");
            assert_eq!(previews, 0);
            assert!(database.connection().execute("INSERT INTO skill_import_previews(id, tool, context_json, redacted_preview_json) VALUES (?1, 'codex', '[]', '{}')", [MCP_ID]).is_err());
        }
    }

    #[test]
    fn snapshot_storage_kind_migration_classifies_existing_rows() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v10-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        let run_id = "00000000-0000-4000-8000-000000000301";
        let file_snapshot = "00000000-0000-4000-8000-000000000302";
        let directory_snapshot = "00000000-0000-4000-8000-000000000303";
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..10] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            connection
                .execute(
                    "INSERT INTO sync_runs(id, kind, status, scope, db_version)
                     VALUES (?1, 'apply', 'succeeded', 'global', 0)",
                    [run_id],
                )
                .unwrap();
            for (id, target_type) in [(file_snapshot, "file"), (directory_snapshot, "directory")] {
                connection
                    .execute(
                        "INSERT INTO snapshots(id, run_id, target_path, snapshot_path, target_type)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            id,
                            run_id,
                            format!("/fixture/target/{id}"),
                            format!("/fixture/snapshot/{id}.snapshot"),
                            target_type,
                        ],
                    )
                    .unwrap();
            }
        }
        let database = Database::open(&paths).unwrap();
        assert_eq!(database.schema_version().unwrap(), 20);
        let kinds = database
            .connection()
            .prepare_cached("SELECT id, storage_kind FROM snapshots ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            kinds,
            vec![
                (file_snapshot.to_owned(), "payload_file".to_owned()),
                (directory_snapshot.to_owned(), "metadata_only".to_owned()),
            ]
        );
    }

    #[test]
    fn project_prompt_migration_removes_only_project_prompt_state_and_payloads() {
        const PROMPT_PROFILE_ID: &str = "00000000-0000-4000-8000-000000000501";
        const PROMPT_TARGET_ID: &str = "00000000-0000-4000-8000-000000000502";
        const MCP_TARGET_ID: &str = "00000000-0000-4000-8000-000000000503";
        const GLOBAL_PROMPT_TARGET_ID: &str = "00000000-0000-4000-8000-000000000504";
        const PROMPT_RUN_ID: &str = "00000000-0000-4000-8000-000000000505";
        const MIXED_RUN_ID: &str = "00000000-0000-4000-8000-000000000506";
        const GLOBAL_RUN_ID: &str = "00000000-0000-4000-8000-000000000507";
        const PROMPT_SNAPSHOT_ID: &str = "00000000-0000-4000-8000-000000000508";
        const MIXED_PROMPT_SNAPSHOT_ID: &str = "00000000-0000-4000-8000-000000000509";
        const MIXED_MCP_SNAPSHOT_ID: &str = "00000000-0000-4000-8000-000000000510";
        const GLOBAL_SNAPSHOT_ID: &str = "00000000-0000-4000-8000-000000000511";
        const PROMPT_NATIVE_ID: &str = "00000000-0000-4000-8000-000000000512";
        const PROMPT_ITEM_ID: &str = "00000000-0000-4000-8000-000000000513";
        const PROMPT_RUN_ITEM_ID: &str = "00000000-0000-4000-8000-000000000514";
        const MIXED_PROMPT_ITEM_ID: &str = "00000000-0000-4000-8000-000000000515";
        const MIXED_MCP_ITEM_ID: &str = "00000000-0000-4000-8000-000000000516";
        const GLOBAL_ITEM_ID: &str = "00000000-0000-4000-8000-000000000517";

        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let project_root = root.join("prompt-project");
        fs::create_dir(&project_root).unwrap();
        let project_prompt_path = project_root.join("CLAUDE.md");
        let project_prompt_bytes = b"# keep this user file\nwith exact bytes\n";
        fs::write(&project_prompt_path, project_prompt_bytes).unwrap();

        let paths = AppPaths::from_data_root(root.join("v17-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();

        let snapshot_path = |run_id: &str, snapshot_id: &str| {
            paths
                .snapshots()
                .join(run_id)
                .join(format!("{snapshot_id}.snapshot"))
        };
        let prompt_snapshot_path = snapshot_path(PROMPT_RUN_ID, PROMPT_SNAPSHOT_ID);
        let mixed_prompt_snapshot_path = snapshot_path(MIXED_RUN_ID, MIXED_PROMPT_SNAPSHOT_ID);
        let mixed_mcp_snapshot_path = snapshot_path(MIXED_RUN_ID, MIXED_MCP_SNAPSHOT_ID);
        let global_snapshot_path = snapshot_path(GLOBAL_RUN_ID, GLOBAL_SNAPSHOT_ID);
        for path in [
            &prompt_snapshot_path,
            &mixed_prompt_snapshot_path,
            &mixed_mcp_snapshot_path,
            &global_snapshot_path,
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"retired snapshot payload").unwrap();
        }

        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE schema_migrations(
                        version INTEGER PRIMARY KEY,
                        name TEXT NOT NULL UNIQUE,
                        applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                    )",
                )
                .unwrap();
            for migration in &super::MIGRATIONS[..17] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, project_root.to_str().unwrap());
            connection
                .execute(
                    "INSERT INTO prompt_profiles(id, tool, name, body)
                     VALUES (?1, 'central', 'Retired project prompt', 'body')",
                    [PROMPT_PROFILE_ID],
                )
                .unwrap();
            for (id, artifact_kind, scope, project_id, target_path) in [
                (
                    PROMPT_TARGET_ID,
                    "prompt",
                    "project",
                    Some(PROJECT_ONE_ID),
                    project_prompt_path.to_str().unwrap(),
                ),
                (
                    MCP_TARGET_ID,
                    "mcp",
                    "project",
                    Some(PROJECT_ONE_ID),
                    "/fixture/prompt-project/.mcp.json",
                ),
                (
                    GLOBAL_PROMPT_TARGET_ID,
                    "prompt",
                    "global",
                    None,
                    "/fixture/home/CLAUDE.md",
                ),
            ] {
                connection
                    .execute(
                        "INSERT INTO managed_targets(
                            id, tool, artifact_kind, scope, project_id, target_path
                         ) VALUES (?1, 'claude', ?2, ?3, ?4, ?5)",
                        params![id, artifact_kind, scope, project_id, target_path],
                    )
                    .unwrap();
            }
            connection
                .execute(
                    "INSERT INTO managed_items(
                        id, target_id, resource_kind, resource_id, external_key,
                        last_applied_item_hash
                     ) VALUES (?1, ?2, 'prompt', ?3, 'prompt', ?4)",
                    params![
                        PROMPT_ITEM_ID,
                        PROMPT_TARGET_ID,
                        PROMPT_PROFILE_ID,
                        "a".repeat(64)
                    ],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO prompt_project_assignments(project_id, tool, prompt_profile_id)
                     VALUES (?1, 'claude', ?2)",
                    params![PROJECT_ONE_ID, PROMPT_PROFILE_ID],
                )
                .unwrap();

            for (run_id, project_id) in [
                (PROMPT_RUN_ID, Some(PROJECT_ONE_ID)),
                (MIXED_RUN_ID, Some(PROJECT_ONE_ID)),
                (GLOBAL_RUN_ID, None),
            ] {
                let scope = project_id.map_or("global", |_| "project");
                connection
                    .execute(
                        "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version)
                         VALUES (?1, 'apply', 'succeeded', ?2, ?3, 17)",
                        params![run_id, scope, project_id],
                    )
                    .unwrap();
            }
            for (id, run_id, target_id) in [
                (PROMPT_RUN_ITEM_ID, PROMPT_RUN_ID, PROMPT_TARGET_ID),
                (MIXED_PROMPT_ITEM_ID, MIXED_RUN_ID, PROMPT_TARGET_ID),
                (MIXED_MCP_ITEM_ID, MIXED_RUN_ID, MCP_TARGET_ID),
                (GLOBAL_ITEM_ID, GLOBAL_RUN_ID, GLOBAL_PROMPT_TARGET_ID),
            ] {
                connection
                    .execute(
                        "INSERT INTO sync_items(
                            id, run_id, target_id, change_kind, status
                         ) VALUES (?1, ?2, ?3, 'update', 'in_sync')",
                        params![id, run_id, target_id],
                    )
                    .unwrap();
            }
            for (id, run_id, target_id, target_path, stored_path) in [
                (
                    PROMPT_SNAPSHOT_ID,
                    PROMPT_RUN_ID,
                    PROMPT_TARGET_ID,
                    project_prompt_path.to_str().unwrap(),
                    prompt_snapshot_path.to_str().unwrap(),
                ),
                (
                    MIXED_PROMPT_SNAPSHOT_ID,
                    MIXED_RUN_ID,
                    PROMPT_TARGET_ID,
                    project_prompt_path.to_str().unwrap(),
                    mixed_prompt_snapshot_path.to_str().unwrap(),
                ),
                (
                    MIXED_MCP_SNAPSHOT_ID,
                    MIXED_RUN_ID,
                    MCP_TARGET_ID,
                    "/fixture/prompt-project/.mcp.json",
                    mixed_mcp_snapshot_path.to_str().unwrap(),
                ),
                (
                    GLOBAL_SNAPSHOT_ID,
                    GLOBAL_RUN_ID,
                    GLOBAL_PROMPT_TARGET_ID,
                    "/fixture/home/CLAUDE.md",
                    global_snapshot_path.to_str().unwrap(),
                ),
            ] {
                connection
                    .execute(
                        "INSERT INTO snapshots(
                            id, run_id, target_id, target_path, snapshot_path,
                            target_type, storage_kind
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'file', 'payload_file')",
                        params![id, run_id, target_id, target_path, stored_path],
                    )
                    .unwrap();
            }
            connection
                .execute(
                    "INSERT INTO project_native_resources(
                        id, target_id, external_key, entry_type, state,
                        observed_item_hash, disabled_snapshot_id, disabled_at
                     ) VALUES (?1, ?2, 'prompt', 'prompt_file', 'disabled', ?3, ?4,
                               strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    params![
                        PROMPT_NATIVE_ID,
                        PROMPT_TARGET_ID,
                        "b".repeat(64),
                        PROMPT_SNAPSHOT_ID
                    ],
                )
                .unwrap();
        }

        let database = Database::open(&paths).unwrap();
        assert_eq!(database.schema_version().unwrap(), 20);
        assert_eq!(
            fs::read(&project_prompt_path).unwrap(),
            project_prompt_bytes
        );
        assert!(!prompt_snapshot_path.exists());
        assert!(!mixed_prompt_snapshot_path.exists());
        assert!(mixed_mcp_snapshot_path.is_file());
        assert!(global_snapshot_path.is_file());

        let connection = database.connection();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'prompt_project_assignments'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM managed_targets WHERE id = ?1",
                    [PROMPT_TARGET_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM project_native_resources", [], |row| {
                    row.get::<_, i64>(0)
                },)
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM snapshots WHERE target_id = ?1",
                    [PROMPT_TARGET_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sync_items WHERE target_id = ?1",
                    [PROMPT_TARGET_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM managed_items WHERE target_id = ?1",
                    [PROMPT_TARGET_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sync_runs WHERE id = ?1",
                    [PROMPT_RUN_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sync_items WHERE run_id = ?1",
                    [MIXED_RUN_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM snapshots WHERE run_id = ?1",
                    [MIXED_RUN_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM managed_targets WHERE id = ?1",
                    [GLOBAL_PROMPT_TARGET_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM snapshots WHERE id = ?1",
                    [GLOBAL_SNAPSHOT_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM retired_snapshot_cleanup", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        assert!(connection
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, project_id, target_path
                 ) VALUES ('00000000-0000-4000-8000-000000000518', 'claude', 'prompt',
                           'project', ?1, ?2)",
                params![PROJECT_ONE_ID, project_prompt_path.to_str().unwrap()],
            )
            .is_err());

        drop(database);
        let reopened = Database::open(&paths).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), 20);
        assert_eq!(
            reopened
                .connection()
                .query_row("SELECT COUNT(*) FROM retired_snapshot_cleanup", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }

    #[test]
    fn prompt_project_assignment_migration_rewrites_managed_targets_check() {
        const GLOBAL_TARGET_ID: &str = "00000000-0000-4000-8000-000000000201";
        const CANARY_PROJECT_ID: &str = "00000000-0000-4000-8000-000000000202";
        const CANARY_TARGET_ID: &str = "00000000-0000-4000-8000-000000000203";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v7-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..7] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            // v7 状态下项目作用域不允许 prompt 基线（artifact_kind 受限）。
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                     VALUES (?1, 'claude', 'prompt', 'global', '/fixture/home/.claude/CLAUDE.md')",
                    [GLOBAL_TARGET_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO projects(id, display_name, root_path)
                     VALUES (?1, 'Prompt Canary', '/fixture/prompt-canary')",
                    [CANARY_PROJECT_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'claude', 'prompt', 'project', ?2, '/fixture/prompt-canary/CLAUDE.md')",
                    params![CANARY_TARGET_ID, CANARY_PROJECT_ID],
                )
                .unwrap_err();
        }
        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            // 既有全局 prompt 基线在迁移后原样保留。
            let preserved: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM managed_targets
                     WHERE id = ?1 AND tool = 'claude' AND artifact_kind = 'prompt'
                       AND scope = 'global' AND project_id IS NULL",
                    [GLOBAL_TARGET_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(preserved, 1);
            let connection = database.connection();
            // 迁移执行所用的同一连接必须立即识别收紧后的 CHECK。
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'claude', 'prompt', 'project', ?2, '/fixture/prompt-canary/CLAUDE.md')",
                    params![CANARY_TARGET_ID, CANARY_PROJECT_ID],
                )
                .unwrap_err();
        }
    }

    #[test]
    fn prompt_tool_active_flags_migration_seeds_per_tool_actives() {
        const CLAUDE_PROFILE_ID: &str = "00000000-0000-4000-8000-000000000210";
        const CODEX_PROFILE_ID: &str = "00000000-0000-4000-8000-000000000211";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v8-prompt-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..8] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            // v8 状态：每个工具各一份生效档案（旧 is_active + tool 绑定语义）。
            connection
                .execute(
                    "INSERT INTO prompt_profiles(id, tool, name, body, is_active)
                     VALUES (?1, 'claude', 'Claude 档案', '', 1)",
                    [CLAUDE_PROFILE_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO prompt_profiles(id, tool, name, body, is_active)
                     VALUES (?1, 'codex', 'Codex 档案', '', 1)",
                    [CODEX_PROFILE_ID],
                )
                .unwrap();
        }
        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            // 旧生效档案按工具种子到新启用位；遗留 is_active 清零。
            let (claude_flag, codex_flag, legacy_active): (i64, i64, i64) = connection
                .query_row(
                    "SELECT is_active_claude, is_active_codex, is_active
                     FROM prompt_profiles WHERE id = ?1",
                    [CLAUDE_PROFILE_ID],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!((claude_flag, codex_flag, legacy_active), (1, 0, 0));
            let codex_flag: i64 = connection
                .query_row(
                    "SELECT is_active_codex FROM prompt_profiles WHERE id = ?1",
                    [CODEX_PROFILE_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(codex_flag, 1);
            // 遗留 tool CHECK 放宽：新档案可写 'central'。
            connection
                .execute(
                    "INSERT INTO prompt_profiles(id, tool, name, body) VALUES (?1, 'central', 'Central 档案', '')",
                    ["00000000-0000-4000-8000-000000000212"],
                )
                .unwrap();
            // 每工具唯一启用仍由部分唯一索引强制。
            assert!(connection
                .execute(
                    "INSERT INTO prompt_profiles(id, tool, name, body, is_active_claude) VALUES ('00000000-0000-4000-8000-000000000213', 'central', '冲突档案', '', 1)",
                    [],
                )
                .is_err());
            connection
                .execute(
                    "DELETE FROM prompt_profiles WHERE tool = 'central' AND name = 'Central 档案'",
                    [],
                )
                .unwrap();
        }
    }

    #[test]
    fn cursor_tool_support_migration_opens_mcp_skill_and_prompt_storage() {
        const CURSOR_TARGET_ID: &str = "00000000-0000-4000-8000-000000000220";
        const CURSOR_MCP_IMPORT_ID: &str = "00000000-0000-4000-8000-000000000221";
        const CURSOR_SKILL_IMPORT_ID: &str = "00000000-0000-4000-8000-000000000222";
        const CLAUDE_PROMPT_ID: &str = "00000000-0000-4000-8000-000000000227";
        const CURSOR_PROMPT_IMPORT_ID: &str = "00000000-0000-4000-8000-000000000269";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v9-cursor-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..9] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/cursor-project");
            insert_mcp(&connection, MCP_ID, "Cursor MCP");
            insert_skill(&connection, SKILL_ID, "cursor-skill");
            connection
                .execute(
                    "INSERT INTO prompt_profiles(id, tool, name, body)
                     VALUES (?1, 'claude', '可供外键验证的提示词', 'fixture')",
                    [CLAUDE_PROMPT_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO mcp_global_assignments(tool, mcp_id) VALUES ('claude', ?1)",
                    [MCP_ID],
                )
                .unwrap();
        }

        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            let preserved: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM mcp_global_assignments WHERE tool = 'claude' AND mcp_id = ?1",
                    [MCP_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(preserved, 1);
            let preserved_indexes: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'index' AND name IN (
                         'idx_mcp_project_assignments_tool_mcp',
                         'idx_skill_project_assignments_tool_skill',
                         'uq_managed_targets_identity',
                         'idx_managed_targets_project',
                         'idx_managed_targets_status',
                         'idx_mcp_import_previews_status',
                         'idx_skill_import_previews_status'
                     )",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(preserved_indexes, 7);
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );

            connection
                .execute(
                    "INSERT INTO mcp_global_assignments(tool, mcp_id) VALUES ('cursor', ?1)",
                    [MCP_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO skill_global_assignments(tool, skill_id) VALUES ('cursor', ?1)",
                    [SKILL_ID],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM mcp_global_assignments WHERE tool = 'cursor'",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM skill_global_assignments WHERE tool = 'cursor'",
                    [],
                )
                .unwrap();
            connection.execute("INSERT INTO mcp_project_assignments(project_id, tool, mcp_id) VALUES (?1, 'cursor', ?2)", params![PROJECT_ONE_ID, MCP_ID]).unwrap();
            connection.execute("INSERT INTO skill_project_assignments(project_id, tool, skill_id) VALUES (?1, 'cursor', ?2)", params![PROJECT_ONE_ID, SKILL_ID]).unwrap();
            connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'cursor', 'mcp', 'global', '/fixture/home/.cursor/mcp.json')", [CURSOR_TARGET_ID]).unwrap();
            connection.execute("INSERT INTO mcp_import_previews(id, tool, target_path, observed_full_hash, context_json, redacted_preview_json) VALUES (?1, 'cursor', '/fixture/home/.cursor/mcp.json', ?2, '{}', '{}')", params![CURSOR_MCP_IMPORT_ID, "a".repeat(64)]).unwrap();
            connection.execute("INSERT INTO skill_import_previews(id, tool, context_json, redacted_preview_json) VALUES (?1, 'cursor', '{}', '{}')", [CURSOR_SKILL_IMPORT_ID]).unwrap();

            assert!(connection.execute("INSERT INTO provider_profiles(id, tool, name) VALUES ('00000000-0000-4000-8000-000000000223', 'cursor', 'Cursor Provider')", []).is_err());
            assert!(connection.execute("INSERT INTO prompt_profiles(id, tool, name, body) VALUES ('00000000-0000-4000-8000-000000000224', 'cursor', 'Cursor Prompt', '')", []).is_err());
            // 0017 放宽后的 cursor×prompt 全局受管目标与导入预览在 v18
            // 迁移后仍保留；项目 Prompt 分配及项目受管目标已经被移除。
            connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'cursor', 'prompt', 'global', '/fixture/home/.cursor/rules/easytoagents.mdc')", ["00000000-0000-4000-8000-000000000268"]).unwrap();
            connection.execute("INSERT INTO profile_import_previews(id, tool, artifact_kind, target_path, observed_full_hash, suggested_name, redacted_preview_json) VALUES (?1, 'cursor', 'prompt', '/fixture/home/.cursor/rules/easytoagents.mdc', ?2, 'Cursor', '{}')", params![CURSOR_PROMPT_IMPORT_ID, "b".repeat(64)]).unwrap();
            assert!(connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES ('00000000-0000-4000-8000-000000000226', 'cursor', 'provider', 'global', '/fixture/provider.json')", []).is_err());
            assert!(connection.execute("INSERT INTO profile_import_previews(id, tool, artifact_kind, target_path, observed_full_hash, suggested_name, redacted_preview_json) VALUES ('00000000-0000-4000-8000-000000000225', 'cursor', 'provider', '/fixture/provider.json', ?1, 'Cursor', '{}')", ["b".repeat(64)]).is_err());

            connection
                .execute(
                    "DELETE FROM skill_import_previews WHERE id = ?1",
                    [CURSOR_SKILL_IMPORT_ID],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM profile_import_previews WHERE id = ?1",
                    [CURSOR_PROMPT_IMPORT_ID],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM managed_targets WHERE id = ?1",
                    ["00000000-0000-4000-8000-000000000268"],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM mcp_import_previews WHERE id = ?1",
                    [CURSOR_MCP_IMPORT_ID],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM managed_targets WHERE id = ?1",
                    [CURSOR_TARGET_ID],
                )
                .unwrap();
            connection.execute("DELETE FROM skill_project_assignments WHERE project_id = ?1 AND tool = 'cursor'", [PROJECT_ONE_ID]).unwrap();
            connection
                .execute(
                    "DELETE FROM mcp_project_assignments WHERE project_id = ?1 AND tool = 'cursor'",
                    [PROJECT_ONE_ID],
                )
                .unwrap();
        }
    }

    #[test]
    fn zcode_tool_support_migration_opens_full_capability_storage() {
        const ZCODE_TARGET_PROVIDER: &str = "00000000-0000-4000-8000-000000000230";
        const ZCODE_TARGET_PROMPT: &str = "00000000-0000-4000-8000-000000000231";
        const ZCODE_TARGET_MCP: &str = "00000000-0000-4000-8000-000000000232";
        const ZCODE_TARGET_SKILL: &str = "00000000-0000-4000-8000-000000000233";
        const ZCODE_PROVIDER_ID: &str = "00000000-0000-4000-8000-000000000234";
        const ZCODE_PROFILE_IMPORT: &str = "00000000-0000-4000-8000-000000000235";
        const ZCODE_PROMPT_PROFILE: &str = "00000000-0000-4000-8000-000000000237";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v12-zcode-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..12] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/zcode-project");
            insert_mcp(&connection, MCP_ID, "ZCode MCP");
            insert_skill(&connection, SKILL_ID, "zcode-skill");
            connection
                .execute(
                    "INSERT INTO prompt_profiles(id, tool, name, body) VALUES (?1, 'claude', '可供外键验证的提示词', 'fixture')",
                    ["00000000-0000-4000-8000-000000000227"],
                )
                .unwrap();
        }
        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );

            // ZCode 四类 artifact 都可写入 managed_targets，并走完整的分配与导入链路。
            connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'zcode', 'provider', 'global', '/fixture/home/.zcode/v2/config.json')", [ZCODE_TARGET_PROVIDER]).unwrap();
            connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'zcode', 'prompt', 'global', '/fixture/home/.zcode/AGENTS.md')", [ZCODE_TARGET_PROMPT]).unwrap();
            connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'zcode', 'mcp', 'global', '/fixture/home/.zcode/cli/config.json')", [ZCODE_TARGET_MCP]).unwrap();
            connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'zcode', 'skill', 'global', '/fixture/home/.zcode/skills')", [ZCODE_TARGET_SKILL]).unwrap();
            connection
                .execute(
                    "INSERT INTO mcp_global_assignments(tool, mcp_id) VALUES ('zcode', ?1)",
                    [MCP_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO skill_global_assignments(tool, skill_id) VALUES ('zcode', ?1)",
                    [SKILL_ID],
                )
                .unwrap();
            connection.execute("INSERT INTO provider_profiles(id, tool, name, api_base_url, api_key, default_model, config_json, is_active) VALUES (?1, 'zcode', 'ZCode Provider', 'https://provider.example.com/v1', 'fixture-key', 'GLM-5.3', '{}', 1)", [ZCODE_PROVIDER_ID]).unwrap();
            connection.execute("INSERT INTO profile_import_previews(id, tool, artifact_kind, target_path, observed_full_hash, suggested_name, redacted_preview_json) VALUES (?1, 'zcode', 'provider', '/fixture/home/.zcode/v2/config.json', ?2, 'ZCode', '{}')", params![ZCODE_PROFILE_IMPORT, "a".repeat(64)]).unwrap();
            // 每工具至多一份生效的 ZCode 提示词索引：第二份被拒绝。
            connection.execute("INSERT INTO prompt_profiles(id, tool, name, body, is_active_zcode) VALUES (?1, 'central', 'ZCode 生效提示词', '', 1)", [ZCODE_PROMPT_PROFILE]).unwrap();
            assert!(connection.execute("INSERT INTO prompt_profiles(id, tool, name, body, is_active_zcode) VALUES ('00000000-0000-4000-8000-000000000238', 'central', 'ZCode 第二份生效', '', 1)", []).is_err());

            // 未支持工具的金丝雀仍被所有表拒绝。
            assert!(connection
                .execute(
                    "INSERT INTO mcp_global_assignments(tool, mcp_id) VALUES ('windsurf', ?1)",
                    [MCP_ID]
                )
                .is_err());
            assert!(connection.execute("INSERT INTO provider_profiles(id, tool, name) VALUES ('00000000-0000-4000-8000-000000000239', 'windsurf', 'Windsurf Provider')", []).is_err());
            assert!(connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES ('00000000-0000-4000-8000-000000000240', 'windsurf', 'mcp', 'global', '/fixture/windsurf.json')", []).is_err());
            connection
                .execute(
                    "DELETE FROM profile_import_previews WHERE id = ?1",
                    [ZCODE_PROFILE_IMPORT],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM provider_profiles WHERE id = ?1",
                    [ZCODE_PROVIDER_ID],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM mcp_global_assignments WHERE tool = 'zcode'",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM skill_global_assignments WHERE tool = 'zcode'",
                    [],
                )
                .unwrap();
            for target_id in [
                ZCODE_TARGET_PROVIDER,
                ZCODE_TARGET_PROMPT,
                ZCODE_TARGET_MCP,
                ZCODE_TARGET_SKILL,
            ] {
                connection
                    .execute("DELETE FROM managed_targets WHERE id = ?1", [target_id])
                    .unwrap();
            }
            connection
                .execute(
                    "DELETE FROM prompt_profiles WHERE id = ?1",
                    [ZCODE_PROMPT_PROFILE],
                )
                .unwrap();
        }
    }

    #[test]
    fn cursor_prompt_migration_widens_checks_and_adds_active_flag() {
        const PROMPT_PROFILE_ID: &str = "00000000-0000-4000-8000-000000000261";
        const CURSOR_PROMPT_TARGET: &str = "00000000-0000-4000-8000-000000000262";
        const CURSOR_IMPORT_PREVIEW: &str = "00000000-0000-4000-8000-000000000263";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v16-cursor-prompt-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..16] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(
                &connection,
                PROJECT_ONE_ID,
                "/fixture/cursor-prompt-project",
            );
        }
        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );

            // Cursor×prompt 进入 managed_targets；cursor×provider 仍被 artifact 限制拒绝。
            connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'cursor', 'prompt', 'global', '/fixture/home/.cursor/rules/easytoagents.mdc')", [CURSOR_PROMPT_TARGET]).unwrap();
            assert!(connection.execute("INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES ('00000000-0000-4000-8000-000000000264', 'cursor', 'provider', 'global', '/fixture/home/.cursor/provider.json')", []).is_err());

            // 每工具至多一份生效的 Cursor 索引：第二份被拒绝。
            connection.execute("INSERT INTO prompt_profiles(id, tool, name, body, is_active_cursor) VALUES (?1, 'central', 'Cursor 生效提示词', '', 1)", [PROMPT_PROFILE_ID]).unwrap();
            assert!(connection.execute("INSERT INTO prompt_profiles(id, tool, name, body, is_active_cursor) VALUES ('00000000-0000-4000-8000-000000000265', 'central', 'Cursor 第二份生效', '', 1)", []).is_err());

            // Cursor Prompt 全局导入预览接受；cursor×provider 导入预览仍被组合 CHECK 拒绝。
            connection.execute("INSERT INTO profile_import_previews(id, tool, artifact_kind, target_path, observed_full_hash, suggested_name, redacted_preview_json) VALUES (?1, 'cursor', 'prompt', '/fixture/home/.cursor/rules/easytoagents.mdc', ?2, 'Cursor', '{}')", params![CURSOR_IMPORT_PREVIEW, "b".repeat(64)]).unwrap();
            assert!(connection.execute("INSERT INTO profile_import_previews(id, tool, artifact_kind, target_path, observed_full_hash, suggested_name, redacted_preview_json) VALUES ('00000000-0000-4000-8000-000000000267', 'cursor', 'provider', '/fixture/provider.json', ?1, 'Cursor', '{}')", ["b".repeat(64)]).is_err());

            // Cursor Provider 仍被 provider_profiles 的 tool CHECK 拒绝（0017 刻意不放宽）。
            assert!(connection.execute("INSERT INTO provider_profiles(id, tool, name) VALUES ('00000000-0000-4000-8000-000000000266', 'cursor', 'Cursor Provider')", []).is_err());

            connection
                .execute(
                    "DELETE FROM profile_import_previews WHERE id = ?1",
                    [CURSOR_IMPORT_PREVIEW],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM managed_targets WHERE id = ?1",
                    [CURSOR_PROMPT_TARGET],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM prompt_profiles WHERE id = ?1",
                    [PROMPT_PROFILE_ID],
                )
                .unwrap();
        }
    }

    #[test]
    fn opencode_migration_accepts_legacy_cursor_preview_schema() {
        let mut connection = Connection::open_in_memory().unwrap();
        let path = std::path::Path::new("/fixture/legacy.sqlite3");
        connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE)").unwrap();
        for migration in &super::MIGRATIONS[..18] {
            connection.execute_batch(migration.sql).unwrap();
            connection
                .execute(
                    "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                    params![migration.version, migration.name],
                )
                .unwrap();
        }
        // 重现真实早期 v17/v18 的表约束，不依赖开发者数据库。
        connection
            .execute_batch(
                "PRAGMA writable_schema = ON;
            UPDATE sqlite_master SET sql = replace(sql,
                ' AND (tool != ''cursor'' OR artifact_kind = ''prompt'')', '')
            WHERE name = 'profile_import_previews';
            PRAGMA writable_schema = OFF;",
            )
            .unwrap();
        super::run_migrations(&mut connection, path).unwrap();
        super::run_migrations(&mut connection, path).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            20
        );
        for (tool, artifact, accepted) in [
            ("opencode", "provider", true),
            ("opencode", "prompt", true),
            ("cursor", "prompt", true),
            ("cursor", "provider", false),
        ] {
            let result = connection.execute(
                "INSERT INTO profile_import_previews(id, tool, artifact_kind, target_path, observed_full_hash, suggested_name, redacted_preview_json)
                 VALUES (?1, ?2, ?3, '/fixture/config', ?4, '预览', '{}')",
                params![uuid::Uuid::new_v4().to_string(), tool, artifact, "a".repeat(64)],
            );
            assert_eq!(result.is_ok(), accepted, "{tool}/{artifact}: {result:?}");
        }
    }

    #[test]
    fn opencode_tool_support_migration_opens_only_supported_artifacts() {
        const MCP_ID: &str = "00000000-0000-4000-8000-000000000271";
        const SKILL_ID: &str = "00000000-0000-4000-8000-000000000272";
        const PROMPT_ID: &str = "00000000-0000-4000-8000-000000000273";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v18-opencode-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..18] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_mcp(&connection, MCP_ID, "OpenCode MCP");
            insert_skill(&connection, SKILL_ID, "opencode-skill");
        }

        let database = Database::open(&paths).unwrap();
        assert_eq!(database.schema_version().unwrap(), 20);
        let connection = database.connection();
        connection
            .execute(
                "INSERT INTO provider_profiles(id, tool, name) VALUES ('00000000-0000-4000-8000-000000000274', 'opencode', 'OpenCode')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body, is_active_opencode) VALUES (?1, 'central', 'OpenCode Prompt', '', 1)",
                [PROMPT_ID],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO mcp_global_assignments(tool, mcp_id) VALUES ('opencode', ?1)",
                [MCP_ID],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO skill_global_assignments(tool, skill_id) VALUES ('opencode', ?1)",
                [SKILL_ID],
            )
            .unwrap();
        for (artifact_kind, target_id, suffix) in [
            (
                "provider",
                "00000000-0000-4000-8000-000000000275",
                "provider",
            ),
            ("prompt", "00000000-0000-4000-8000-000000000276", "prompt"),
            ("mcp", "00000000-0000-4000-8000-000000000277", "mcp"),
            ("skill", "00000000-0000-4000-8000-000000000278", "skill"),
        ] {
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES (?1, 'opencode', ?2, 'global', ?3)",
                    params![
                        target_id,
                        artifact_kind,
                        format!("/fixture/opencode/{suffix}")
                    ],
                )
                .unwrap();
        }
        assert!(connection
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path) VALUES ('00000000-0000-4000-8000-000000000289', 'opencode', 'hook', 'global', '/fixture/opencode/hooks')",
                [],
            )
            .is_err());

        drop(database);
        let reopened = Database::open(&paths).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), 20);
        assert_eq!(
            reopened
                .connection()
                .query_row(
                    "SELECT is_active_opencode FROM prompt_profiles WHERE id = ?1",
                    [PROMPT_ID],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn hooks_migration_opens_hook_storage_and_widens_checks() {
        const HOOK_ONE_ID: &str = "00000000-0000-4000-8000-000000000301";
        const HOOK_TARGET_GLOBAL: &str = "00000000-0000-4000-8000-000000000302";
        const HOOK_TARGET_PROJECT: &str = "00000000-0000-4000-8000-000000000303";
        const HOOK_ITEM_ID: &str = "00000000-0000-4000-8000-000000000304";
        const CURSOR_HOOK_TARGET: &str = "00000000-0000-4000-8000-000000000305";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v13-hooks-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..13] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/hooks-project");
        }
        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );

            // 中央 hooks 表：合法记录、大小写不重复、事件与超时 CHECK 生效。
            connection
                .execute(
                    "INSERT INTO hooks(id, name, event, matcher, command, timeout_seconds, enabled)
                     VALUES (?1, 'block-rm', 'PreToolUse', 'Bash', 'bash /fixture/block-rm.sh', 30, 1)",
                    [HOOK_ONE_ID],
                )
                .unwrap();
            assert!(connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command) VALUES ('00000000-0000-4000-8000-000000000306', 'BLOCK-RM', 'PreToolUse', 'true')",
                    [],
                )
                .is_err());
            assert!(connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command) VALUES ('00000000-0000-4000-8000-000000000307', 'bad-event', 'BeforeToolUse', 'true')",
                    [],
                )
                .is_err());
            assert!(connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command, timeout_seconds) VALUES ('00000000-0000-4000-8000-000000000308', 'bad-timeout', 'Stop', 'true', 0)",
                    [],
                )
                .is_err());
            assert!(connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command) VALUES ('00000000-0000-4000-8000-000000000309', 'empty-command', 'Stop', '  ')",
                    [],
                )
                .is_err());

            // managed_targets / managed_items：'hook' 进入 global/project 两类作用域，
            // Cursor 的 artifact 限制同步放宽。
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                     VALUES (?1, 'claude', 'hook', 'global', '/fixture/home/.claude/settings.json')",
                    [HOOK_TARGET_GLOBAL],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'codex', 'hook', 'project', ?2, '/fixture/hooks-project/.codex/hooks.json')",
                    params![HOOK_TARGET_PROJECT, PROJECT_ONE_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                     VALUES (?1, 'cursor', 'hook', 'global', '/fixture/home/.cursor/hooks.json')",
                    [CURSOR_HOOK_TARGET],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO managed_items(id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash)
                     VALUES (?1, ?2, 'hook', ?3, 'PreToolUse|Bash|fixture', ?4)",
                    params![HOOK_ITEM_ID, HOOK_TARGET_GLOBAL, HOOK_ONE_ID, "a".repeat(64)],
                )
                .unwrap();
            assert!(connection
                .execute(
                    "INSERT INTO managed_items(id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash)
                     VALUES ('00000000-0000-4000-8000-000000000310', ?1, 'mcp', ?2, 'fixture', ?3)",
                    params![HOOK_TARGET_GLOBAL, MCP_ID, "a".repeat(64)],
                )
                .is_err());

            // 全局/项目分配互斥触发器与未知工具 CHECK。
            connection
                .execute(
                    "INSERT INTO hook_global_assignments(tool, hook_id, event) VALUES ('claude', ?1, 'PreToolUse')",
                    [HOOK_ONE_ID],
                )
                .unwrap();
            assert!(connection
                .execute(
                    "INSERT INTO hook_project_assignments(project_id, tool, hook_id) VALUES (?1, 'claude', ?2)",
                    params![PROJECT_ONE_ID, HOOK_ONE_ID],
                )
                .is_err());
            assert!(connection
                .execute(
                    "INSERT INTO hook_global_assignments(tool, hook_id) VALUES ('windsurf', ?1)",
                    [HOOK_ONE_ID],
                )
                .is_err());

            if _round == 0 {
                connection
                    .execute("DELETE FROM managed_items WHERE id = ?1", [HOOK_ITEM_ID])
                    .unwrap();
                connection
                    .execute(
                        "DELETE FROM hook_global_assignments WHERE tool = 'claude'",
                        [],
                    )
                    .unwrap();
                for target_id in [HOOK_TARGET_GLOBAL, HOOK_TARGET_PROJECT, CURSOR_HOOK_TARGET] {
                    connection
                        .execute("DELETE FROM managed_targets WHERE id = ?1", [target_id])
                        .unwrap();
                }
                connection
                    .execute("DELETE FROM hooks WHERE id = ?1", [HOOK_ONE_ID])
                    .unwrap();
            }
        }
    }

    #[test]
    fn hooks_scripts_migration_adds_nullable_script_columns() {
        const HOOK_LEGACY_ID: &str = "00000000-0000-4000-8000-000000000320";
        const HOOK_SCRIPT_ID: &str = "00000000-0000-4000-8000-000000000321";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v14-hooks-scripts-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..14] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
        }
        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );

            // 旧行保留：script 列为 NULL（inline 命令）。
            connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command) VALUES (?1, 'legacy-inline', 'Stop', 'echo hi')",
                    [HOOK_LEGACY_ID],
                )
                .unwrap();
            let script_name: Option<String> = connection
                .query_row(
                    "SELECT script_name FROM hooks WHERE id = ?1",
                    [HOOK_LEGACY_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(script_name, None);

            // 接管型插入：两列同置；非法值被列级 CHECK 拒绝。
            connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command, script_name, script_hash)
                     VALUES (?1, 'adopted', 'PreToolUse', 'bash \"/central/x.sh\"', 'x.sh', ?2)",
                    params![HOOK_SCRIPT_ID, "a".repeat(64)],
                )
                .unwrap();
            assert!(connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command, script_name, script_hash)
                     VALUES ('00000000-0000-4000-8000-000000000322', 'bad-name', 'Stop', 'true', 'nested/x.sh', ?1)",
                    ["a".repeat(64)]
                )
                .is_err());
            assert!(connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command, script_name, script_hash)
                     VALUES ('00000000-0000-4000-8000-000000000323', 'bad-hash', 'Stop', 'true', 'x.sh', 'NOTHEX')",
                    [],
                )
                .is_err());

            if _round == 0 {
                connection
                    .execute(
                        "DELETE FROM hooks WHERE id IN (?1, ?2)",
                        params![HOOK_LEGACY_ID, HOOK_SCRIPT_ID],
                    )
                    .unwrap();
            }
        }
    }

    #[test]
    fn hook_assignment_events_migration_backfills_and_allows_event_switch() {
        const HOOK_ID: &str = "00000000-0000-4000-8000-000000000330";
        const PROJECT_HOOK_ID: &str = "00000000-0000-4000-8000-000000000331";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v15-assignment-events-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..15] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/event-project");
            // v15：事件在中央记录上，分配行无事件列。
            connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command) VALUES (?1, 'legacy-event', 'PreToolUse', 'true')",
                    [HOOK_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command) VALUES (?1, 'project-event', 'Stop', 'true')",
                    [PROJECT_HOOK_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO hook_global_assignments(tool, hook_id) VALUES ('claude', ?1)",
                    [HOOK_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO hook_project_assignments(project_id, tool, hook_id) VALUES (?1, 'codex', ?2)",
                    params![PROJECT_ONE_ID, PROJECT_HOOK_ID],
                )
                .unwrap();
        }
        for _round in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );
            // 回填：分配行的 event 取自 hooks.event（仅首轮有 v15 旧数据）。
            if _round == 0 {
                let backfilled: String = connection
                    .query_row(
                        "SELECT event FROM hook_global_assignments WHERE hook_id = ?1",
                        [HOOK_ID],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(backfilled, "PreToolUse");
            }

            // 切换事件：ON CONFLICT 更新；非法事件被 CHECK 拒绝。
            connection
                .execute(
                    "INSERT INTO hook_global_assignments(tool, hook_id, event) VALUES ('claude', ?1, 'SessionStart')
                     ON CONFLICT(tool, hook_id) DO UPDATE SET event = excluded.event",
                    [HOOK_ID],
                )
                .unwrap();
            let switched: String = connection
                .query_row(
                    "SELECT event FROM hook_global_assignments WHERE hook_id = ?1",
                    [HOOK_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(switched, "SessionStart");
            assert!(connection
                .execute(
                    "INSERT INTO hook_global_assignments(tool, hook_id, event) VALUES ('claude', ?1, 'BeforeToolUse')",
                    [HOOK_ID]
                )
                .is_err());

            if _round == 0 {
                // 重开轮保留数据：验证幂等与触发器在既有数据上仍然生效。
                connection
                    .execute(
                        "UPDATE hook_global_assignments SET event = 'Stop' WHERE hook_id = ?1",
                        [HOOK_ID],
                    )
                    .unwrap();
            }
        }
    }

    #[test]
    fn cursor_tool_support_migration_rejects_a_missing_exact_anchor() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v9-anchor-mismatch-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..9] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            connection
                .execute_batch(
                    "PRAGMA writable_schema = ON;
                     UPDATE sqlite_master
                     SET sql = replace(
                         sql,
                         'tool TEXT NOT NULL CHECK(tool IN (''claude'', ''codex''))',
                         'tool TEXT NOT NULL CHECK(tool IN (''codex'', ''claude''))'
                     )
                     WHERE type = 'table' AND name = 'mcp_global_assignments';
                     PRAGMA writable_schema = OFF;",
                )
                .unwrap();
        }

        let error = match Database::open(&paths) {
            Ok(_) => panic!("精确旧锚点缺失时迁移不得成功"),
            Err(error) => error,
        };
        assert_eq!(error.code(), ErrorCode::MigrationFailed);
        let connection = Connection::open(paths.database()).unwrap();
        let version: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 9, "失败迁移不得推进 schema version");
    }

    #[test]
    fn startup_rejects_a_forged_or_out_of_order_migration_history() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("migration-history-data")).unwrap();
        {
            let database = Database::open(&paths).unwrap();
            database
                .connection()
                .execute(
                    "UPDATE schema_migrations SET name = 'forged' WHERE version = 1",
                    [],
                )
                .unwrap();
        }

        assert!(Database::open(&paths).is_err());
    }

    #[test]
    fn project_native_resources_migration_upgrades_v11_and_enforces_constraints() {
        const RUN_ID: &str = "00000000-0000-4000-8000-000000000401";
        const SNAPSHOT_ID: &str = "00000000-0000-4000-8000-000000000402";
        const TARGET_ID: &str = "00000000-0000-4000-8000-000000000403";
        const RESOURCE_ID: &str = "00000000-0000-4000-8000-000000000404";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v11-native-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..11] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/native-project");
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'claude', 'mcp', 'project', ?2, '/fixture/native-project/.mcp.json')",
                    params![TARGET_ID, PROJECT_ONE_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version)
                     VALUES (?1, 'apply', 'succeeded', 'project', ?2, 0)",
                    params![RUN_ID, PROJECT_ONE_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO snapshots(id, run_id, target_id, target_path, snapshot_path, target_type, storage_kind)
                     VALUES (?1, ?2, ?3, '/fixture/native-project/.mcp.json', '/fixture/snapshot/native.snapshot', 'file', 'payload_file')",
                    params![SNAPSHOT_ID, RUN_ID, TARGET_ID],
                )
                .unwrap();
        }
        for _ in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 20);
            let connection = database.connection();
            let preserved: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM managed_targets WHERE id = ?1
                       AND baseline_full_hash IS NULL AND baseline_managed_hash IS NULL
                       AND baseline_projection_json IS NULL",
                    [TARGET_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(preserved, 1);
            assert!(connection
                .execute(
                    "INSERT INTO project_native_resources(
                        id, target_id, external_key, entry_type, state, disabled_snapshot_id, disabled_at
                     ) VALUES (?1, ?2, 'fixture', 'mcp_entry', 'disabled', NULL, NULL)",
                    params![RESOURCE_ID, TARGET_ID],
                )
                .is_err());
            connection
                .execute(
                    "INSERT INTO project_native_resources(
                        id, target_id, external_key, entry_type, state, observed_item_hash
                     ) VALUES (?1, ?2, 'fixture', 'mcp_entry', 'active', ?3)",
                    params![RESOURCE_ID, TARGET_ID, "a".repeat(64)],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE project_native_resources
                     SET state = 'disabled', disabled_snapshot_id = ?2,
                         disabled_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?1",
                    params![RESOURCE_ID, SNAPSHOT_ID],
                )
                .unwrap();
            assert!(
                connection
                    .execute("DELETE FROM snapshots WHERE id = ?1", [SNAPSHOT_ID])
                    .is_err(),
                "被原生禁用记录引用的快照必须 RESTRICT"
            );
            connection
                .execute(
                    "DELETE FROM project_native_resources WHERE id = ?1",
                    [RESOURCE_ID],
                )
                .unwrap();
        }
    }

    #[test]
    fn empty_target_identity_is_not_ownership() {
        let (_temporary, _paths, database) = open_isolated_database();
        insert_project(database.connection(), PROJECT_ONE_ID, "/fixture/identity");
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                 VALUES (?1, 'claude', 'mcp', 'project', ?2, '/fixture/identity/.mcp.json')",
                params![TARGET_ONE_ID, PROJECT_ONE_ID],
            )
            .unwrap();
        let (full, managed, items): (Option<String>, Option<String>, i64) = database
            .connection()
            .query_row(
                "SELECT baseline_full_hash, baseline_managed_hash,
                        (SELECT COUNT(*) FROM managed_items WHERE target_id = ?1)
                 FROM managed_targets WHERE id = ?1",
                [TARGET_ONE_ID],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert!(full.is_none());
        assert!(managed.is_none());
        assert_eq!(items, 0);
        let counts = super::native_resources::count_for_project(&database, PROJECT_ONE_ID).unwrap();
        assert_eq!(counts.active, 0);
        assert_eq!(counts.disabled, 0);
        assert!(
            super::native_resources::list_for_target(&database, TARGET_ONE_ID)
                .unwrap()
                .is_empty()
        );
    }
}
