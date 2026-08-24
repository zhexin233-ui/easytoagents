//! SQLite 初始化、前向迁移与事务边界。

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, TransactionBehavior};

use crate::{
    app::AppPaths,
    error::AppError,
    security::{create_private_file, ensure_private_directory, ensure_private_file},
};

pub mod mcp;
pub mod profiles;
pub mod projects;
pub mod skills;

const MIGRATIONS: &[Migration] = &[
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
];

struct Migration {
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
    /// 每次打开已有数据库时先备份主文件及存在的 WAL/SHM，再运行前向迁移。
    pub fn open(paths: &AppPaths) -> Result<Self, AppError> {
        paths.initialize()?;
        let startup_backup = backup_database_before_migrations(paths)?;
        prepare_database_file(paths.database())?;

        let mut connection = Connection::open(paths.database())
            .map_err(|_| AppError::database(&paths.database().to_string_lossy(), "open"))?;
        configure_connection(&connection, paths.database())?;
        run_migrations(&mut connection, paths.database())?;
        configure_connection(&connection, paths.database())?;

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
            .prepare("SELECT version, name FROM schema_migrations ORDER BY version")
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
    }
    Ok(())
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
        assert_eq!(database.schema_version().unwrap(), 4);
        let foreign_key_violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(foreign_key_violations, 0);

        let tables = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
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
            "onboarding_state",
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

        connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body, is_active) VALUES ('00000000-0000-4000-8000-000000000014', 'codex', 'One', '', 1)",
                [],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body, is_active) VALUES ('00000000-0000-4000-8000-000000000015', 'codex', 'Two', '', 1)",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO prompt_profiles(id, tool, name, body) VALUES ('not-a-uuid', 'claude', 'Invalid ID', '')",
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
        {
            let database = Database::open(&paths).unwrap();
            database
                .connection()
                .execute(
                    "INSERT INTO projects(id, display_name, root_path) VALUES (?1, 'Kept', '/fixture/kept')",
                    [PROJECT_TWO_ID],
                )
                .unwrap();
        }

        fs::set_permissions(paths.database(), fs::Permissions::from_mode(0o644)).unwrap();
        let reopened = Database::open(&paths).unwrap();
        let backup = reopened.startup_backup().expect("已有数据库必须先备份");
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
    fn startup_backup_preserves_rows_that_are_still_in_an_active_wal() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("active-wal-data")).unwrap();
        let first = Database::open(&paths).unwrap();
        first
            .connection()
            .execute_batch("PRAGMA wal_autocheckpoint = 0;")
            .unwrap();
        first
            .connection()
            .execute(
                "INSERT INTO projects(id, display_name, root_path) VALUES (?1, 'In WAL', '/fixture/in-wal')",
                [PROJECT_TWO_ID],
            )
            .unwrap();
        assert!(paths.database_wal().is_file());

        let second = Database::open(&paths).unwrap();
        let backup = second
            .startup_backup()
            .expect("活动 WAL 必须随主数据库备份");
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
}
