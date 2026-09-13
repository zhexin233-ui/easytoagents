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
        assert_eq!(database.schema_version().unwrap(), 24);
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
        assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
        assert_eq!(database.schema_version().unwrap(), 24);
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
        assert_eq!(database.schema_version().unwrap(), 24);
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
        assert_eq!(reopened.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            24
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
        assert_eq!(database.schema_version().unwrap(), 24);
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
        assert_eq!(reopened.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
            assert_eq!(database.schema_version().unwrap(), 24);
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
    fn project_native_hook_entries_migration_preserves_rows_and_widens_check() {
        const RUN_ID: &str = "00000000-0000-4000-8000-000000000421";
        const SNAPSHOT_ID: &str = "00000000-0000-4000-8000-000000000422";
        const TARGET_ID: &str = "00000000-0000-4000-8000-000000000423";
        const RESOURCE_MCP: &str = "00000000-0000-4000-8000-000000000424";
        const RESOURCE_DIRECTORY: &str = "00000000-0000-4000-8000-000000000425";
        const RESOURCE_SYMLINK: &str = "00000000-0000-4000-8000-000000000426";
        const RESOURCE_HOOK: &str = "00000000-0000-4000-8000-000000000427";
        const SNAPSHOT_ID_NEW: &str = "00000000-0000-4000-8000-000000000428";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v20-native-hook-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            // 先建到 0020 的旧库，插入手持禁用快照的三种旧类型行，再交给真实迁移。
            for migration in &super::MIGRATIONS[..20] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/hook-project");
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'claude', 'hook', 'project', ?2, '/fixture/hook-project/.claude/settings.json')",
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
                     VALUES (?1, ?2, ?3, '/fixture/hook-project/.claude/settings.json', '/fixture/snapshot/hook.snapshot', 'file', 'payload_file')",
                    params![SNAPSHOT_ID, RUN_ID, TARGET_ID],
                )
                .unwrap();
            let disabled_at = "2026-09-01T00:00:00.000Z";
            for (id, external_key, entry_type, state, snapshot, hash) in [
                (RESOURCE_MCP, "mcp|native", "mcp_entry", "active", None, Some("a".repeat(64))),
                (RESOURCE_DIRECTORY, "native-dir", "directory", "active", None, Some("b".repeat(64))),
                (RESOURCE_SYMLINK, "native-link", "symlink", "disabled", Some(SNAPSHOT_ID), None),
            ] {
                connection
                    .execute(
                        "INSERT INTO project_native_resources(
                            id, target_id, external_key, entry_type, state, observed_item_hash,
                            disabled_snapshot_id, disabled_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            id,
                            TARGET_ID,
                            external_key,
                            entry_type,
                            state,
                            hash,
                            snapshot,
                            if snapshot.is_some() { Some(disabled_at) } else { None },
                        ],
                    )
                    .unwrap();
            }
        }

        for _ in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 24);
            let connection = database.connection();
            type PreservedRow = (
                String,
                String,
                String,
                Option<String>,
                Option<String>,
            );
            let rows: Vec<PreservedRow> = connection
                .prepare(
                    "SELECT id, external_key, entry_type, disabled_snapshot_id, disabled_at
                     FROM project_native_resources ORDER BY external_key",
                )
                .unwrap()
                .query_map([], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(
                rows,
                vec![
                    (
                        RESOURCE_MCP.to_owned(),
                        "mcp|native".to_owned(),
                        "mcp_entry".to_owned(),
                        None,
                        None,
                    ),
                    (
                        RESOURCE_DIRECTORY.to_owned(),
                        "native-dir".to_owned(),
                        "directory".to_owned(),
                        None,
                        None,
                    ),
                    (
                        RESOURCE_SYMLINK.to_owned(),
                        "native-link".to_owned(),
                        "symlink".to_owned(),
                        Some(SNAPSHOT_ID.to_owned()),
                        Some("2026-09-01T00:00:00.000Z".to_owned()),
                    ),
                ],
                "旧类型行与禁用快照引用必须逐字保留"
            );

            // hook_entry 可插入，prompt_file 仍被拒绝。
            connection
                .execute(
                    "INSERT INTO project_native_resources(
                        id, target_id, external_key, entry_type, state, observed_item_hash
                     ) VALUES (?1, ?2, 'UserPromptSubmit||abc123', 'hook_entry', 'active', ?3)",
                    params![RESOURCE_HOOK, TARGET_ID, "c".repeat(64)],
                )
                .unwrap();
            assert!(connection
                .execute(
                    "INSERT INTO project_native_resources(
                        id, target_id, external_key, entry_type, state, observed_item_hash
                     ) VALUES (?1, ?2, 'prompt-file', 'prompt_file', 'active', ?3)",
                    params![RESOURCE_HOOK, TARGET_ID, "d".repeat(64)],
                )
                .is_err());

            // row_version bump 触发器在重建后仍然生效。
            let before_version: i64 = connection
                .query_row(
                    "SELECT row_version FROM project_native_resources WHERE id = ?1",
                    [RESOURCE_HOOK],
                    |row| row.get(0),
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE project_native_resources SET last_seen_at = last_seen_at WHERE id = ?1",
                    [RESOURCE_HOOK],
                )
                .unwrap();
            let after_version: i64 = connection
                .query_row(
                    "SELECT row_version FROM project_native_resources WHERE id = ?1",
                    [RESOURCE_HOOK],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(after_version, before_version + 1);

            // snapshots 交叉保护触发器在重建后仍然拒绝改 id。
            let rename = connection.execute(
                "UPDATE snapshots SET id = ?1 WHERE id = ?2",
                params![SNAPSHOT_ID_NEW, SNAPSHOT_ID],
            );
            assert!(
                rename.is_err(),
                "被原生禁用记录引用的快照必须继续被触发器保护"
            );
            connection
                .execute(
                    "DELETE FROM project_native_resources WHERE id = ?1",
                    [RESOURCE_HOOK],
                )
                .unwrap();
        }
    }

    #[test]
    fn project_native_agent_files_migration_preserves_rows_and_widens_check() {
        const RUN_ID: &str = "00000000-0000-4000-8000-000000000441";
        const SNAPSHOT_ID: &str = "00000000-0000-4000-8000-000000000442";
        const TARGET_ID: &str = "00000000-0000-4000-8000-000000000443";
        const RESOURCE_MCP: &str = "00000000-0000-4000-8000-000000000444";
        const RESOURCE_DIRECTORY: &str = "00000000-0000-4000-8000-000000000445";
        const RESOURCE_SYMLINK: &str = "00000000-0000-4000-8000-000000000446";
        const RESOURCE_HOOK: &str = "00000000-0000-4000-8000-000000000447";
        const RESOURCE_AGENT: &str = "00000000-0000-4000-8000-000000000448";
        const RESOURCE_PROMPT: &str = "00000000-0000-4000-8000-000000000449";
        const SNAPSHOT_ID_NEW: &str = "00000000-0000-4000-8000-00000000044a";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v23-native-agent-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            for migration in &super::MIGRATIONS[..23] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/agent-project");
        }

        // 0022 通过 writable_schema 放宽 managed_targets 的 CHECK；关闭并
        // 重新打开连接，确保 SQLite 丢弃旧 schema cache 后再写入 Agent 目标。
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'claude', 'agent', 'project', ?2, '/fixture/agent-project/.claude/agents')",
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
                     VALUES (?1, ?2, ?3, '/fixture/agent-project/.claude/agents/link.md', '/fixture/snapshot/agent.snapshot', 'file', 'payload_file')",
                    params![SNAPSHOT_ID, RUN_ID, TARGET_ID],
                )
                .unwrap();
            let disabled_at = "2026-09-01T00:00:00.000Z";
            for (id, external_key, entry_type, state, observed_hash, snapshot) in [
                (RESOURCE_MCP, "mcp-native", "mcp_entry", "active", Some("a".repeat(64)), None),
                (RESOURCE_DIRECTORY, "native-dir", "directory", "active", Some("b".repeat(64)), None),
                (RESOURCE_SYMLINK, "native-link", "symlink", "disabled", None, Some(SNAPSHOT_ID)),
                (RESOURCE_HOOK, "PreToolUse||abc123", "hook_entry", "active", Some("c".repeat(64)), None),
            ] {
                connection
                    .execute(
                        "INSERT INTO project_native_resources(
                            id, target_id, external_key, entry_type, state, observed_item_hash,
                            disabled_snapshot_id, disabled_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            id,
                            TARGET_ID,
                            external_key,
                            entry_type,
                            state,
                            observed_hash,
                            snapshot,
                            if snapshot.is_some() { Some(disabled_at) } else { None },
                        ],
                    )
                    .unwrap();
            }
        }

        let database = Database::open(&paths).unwrap();
        assert_eq!(database.schema_version().unwrap(), 24);
        let connection = database.connection();
        type PreservedRow = (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<PreservedRow> = connection
            .prepare(
                "SELECT id, external_key, entry_type, state, observed_item_hash,
                        disabled_snapshot_id, disabled_at
                 FROM project_native_resources ORDER BY external_key",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().any(|row| {
            row.0 == RESOURCE_SYMLINK
                && row.2 == "symlink"
                && row.3 == "disabled"
                && row.5.as_deref() == Some(SNAPSHOT_ID)
        }));
        connection
            .execute(
                "INSERT INTO project_native_resources(
                    id, target_id, external_key, entry_type, state, observed_item_hash
                 ) VALUES (?1, ?2, 'reviewer.md', 'agent_file', 'active', ?3)",
                params![RESOURCE_AGENT, TARGET_ID, "d".repeat(64)],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO project_native_resources(
                    id, target_id, external_key, entry_type, state, observed_item_hash
                 ) VALUES (?1, ?2, 'prompt-file', 'prompt_file', 'active', ?3)",
                params![RESOURCE_PROMPT, TARGET_ID, "e".repeat(64)],
            )
            .is_err());

        let before_version: i64 = connection
            .query_row(
                "SELECT row_version FROM project_native_resources WHERE id = ?1",
                [RESOURCE_AGENT],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE project_native_resources SET last_seen_at = last_seen_at WHERE id = ?1",
                [RESOURCE_AGENT],
            )
            .unwrap();
        let after_version: i64 = connection
            .query_row(
                "SELECT row_version FROM project_native_resources WHERE id = ?1",
                [RESOURCE_AGENT],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after_version, before_version + 1);
        assert!(connection
            .execute(
                "UPDATE snapshots SET id = ?1 WHERE id = ?2",
                params![SNAPSHOT_ID_NEW, SNAPSHOT_ID],
            )
            .is_err());
        drop(database);

        let reopened = Database::open(&paths).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), 24);
        let (count, agent_type): (i64, String) = reopened
            .connection()
            .query_row(
                "SELECT COUNT(*), (SELECT entry_type FROM project_native_resources
                 WHERE target_id = ?1 AND entry_type = 'agent_file' LIMIT 1)
                 FROM project_native_resources WHERE target_id = ?1",
                [TARGET_ID],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 5);
        assert_eq!(agent_type, "agent_file");
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

    #[test]
    fn agents_migration_preserves_rows_and_widens_managed_target_checks() {
        const HOOK_ID: &str = "00000000-0000-4000-8000-000000000501";
        const HOOK_TARGET_ID: &str = "00000000-0000-4000-8000-000000000502";
        const AGENT_ONE_ID: &str = "00000000-0000-4000-8000-000000000503";
        // 金丝雀插入专用 ID（0..=11 位按序编号，避免相互冲突）。
        const CANARY_IDS: [&str; 12] = [
            "00000000-0000-4000-8000-000000000510",
            "00000000-0000-4000-8000-000000000511",
            "00000000-0000-4000-8000-000000000512",
            "00000000-0000-4000-8000-000000000513",
            "00000000-0000-4000-8000-000000000514",
            "00000000-0000-4000-8000-000000000515",
            "00000000-0000-4000-8000-000000000516",
            "00000000-0000-4000-8000-000000000517",
            "00000000-0000-4000-8000-000000000518",
            "00000000-0000-4000-8000-000000000519",
            "00000000-0000-4000-8000-00000000051a",
            "00000000-0000-4000-8000-00000000051b",
        ];
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v21-agents-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
        {
            let connection = Connection::open(paths.database()).unwrap();
            super::configure_connection(&connection, paths.database()).unwrap();
            connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))").unwrap();
            // 先建到 0021 的旧库，插入既有 hook、分配与受管目标行，再交给真实迁移。
            for migration in &super::MIGRATIONS[..21] {
                connection.execute_batch(migration.sql).unwrap();
                connection
                    .execute(
                        "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )
                    .unwrap();
            }
            insert_project(&connection, PROJECT_ONE_ID, "/fixture/agents-project");
            connection
                .execute(
                    "INSERT INTO hooks(id, name, event, command) VALUES (?1, 'legacy-hook', 'PreToolUse', 'echo legacy')",
                    params![HOOK_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO hook_global_assignments(tool, hook_id, event) VALUES ('claude', ?1, 'PreToolUse')",
                    params![HOOK_ID],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                     VALUES (?1, 'claude', 'hook', 'global', '/fixture/home/.claude/settings.json')",
                    params![HOOK_TARGET_ID],
                )
                .unwrap();
            // v21 的 CHECK 尚不接受 agent 目标：升级前证明旧边界存在。
            assert!(
                connection
                    .execute(
                        "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                         VALUES (?1, 'claude', 'agent', 'global', '/fixture/home/.claude/agents/x.md')",
                        params![AGENT_ONE_ID],
                    )
                    .is_err(),
                "v21 必须拒绝 agent 受管目标"
            );
        }

        for _ in 0..2 {
            let database = Database::open(&paths).unwrap();
            assert_eq!(database.schema_version().unwrap(), 24);
            let connection = database.connection();

            // 旧行保留：hook、分配与受管目标在 writable_schema 改写后逐字保留。
            let hook: (String, String, String) = connection
                .query_row(
                    "SELECT name, event, command FROM hooks WHERE id = ?1",
                    [HOOK_ID],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                hook,
                (
                    "legacy-hook".to_owned(),
                    "PreToolUse".to_owned(),
                    "echo legacy".to_owned()
                )
            );
            let assignment_count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM hook_global_assignments WHERE hook_id = ?1",
                    [HOOK_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(assignment_count, 1);
            let hook_target_kind: String = connection
                .query_row(
                    "SELECT artifact_kind FROM managed_targets WHERE id = ?1",
                    [HOOK_TARGET_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(hook_target_kind, "hook");

            // 金丝雀 1：全局 agent 目标（claude）被接受。
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                     VALUES (?1, 'claude', 'agent', 'global', '/fixture/home/.claude/agents/reviewer.md')",
                    params![AGENT_ONE_ID],
                )
                .unwrap();
            // 金丝雀 2：项目级 agent 目标（claude + project）被接受。
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'claude', 'agent', 'project', ?2, '/fixture/agents-project/.claude/agents/reviewer.md')",
                    params![CANARY_IDS[0], PROJECT_ONE_ID],
                )
                .unwrap();
            // 金丝雀 3：cursor / opencode 的全局 agent 目标被接受。
            for (index, (tool, target)) in [
                ("cursor", "/fixture/home/.cursor/agents/reviewer.md"),
                (
                    "opencode",
                    "/fixture/home/.config/opencode/agents/reviewer.md",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                connection
                    .execute(
                        "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                         VALUES (?1, ?2, 'agent', 'global', ?3)",
                        params![CANARY_IDS[index + 1], tool, target],
                    )
                    .unwrap_or_else(|error| panic!("{tool} 全局 agent 目标应被接受: {error}"));
            }
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                     VALUES (?1, 'opencode', 'agent', 'project', ?2, '/fixture/agents-project/.opencode/agents/reviewer.md')",
                    params![CANARY_IDS[3], PROJECT_ONE_ID],
                )
                .unwrap();
            // 金丝雀 4（负例）：ZCode 项目级 agent 目标被新作用域约束拒绝，
            // 全局 agent 目标仍被接受。
            assert!(
                connection
                    .execute(
                        "INSERT INTO managed_targets(id, tool, artifact_kind, scope, project_id, target_path)
                         VALUES (?1, 'zcode', 'agent', 'project', ?2, '/fixture/agents-project/.zcode/agents/reviewer.md')",
                        params![CANARY_IDS[4], PROJECT_ONE_ID],
                    )
                    .is_err(),
                "ZCode 项目级 agent 目标必须被作用域约束拒绝"
            );
            connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                     VALUES (?1, 'zcode', 'agent', 'global', '/fixture/home/.zcode/agents/reviewer.md')",
                    params![CANARY_IDS[5]],
                )
                .unwrap();
            // 金丝雀 5（负例）：既有边界未放宽——opencode hook 目标仍被拒绝。
            assert!(connection
                .execute(
                    "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path)
                     VALUES (?1, 'opencode', 'hook', 'global', '/fixture/home/.config/opencode/hooks.json')",
                    params![CANARY_IDS[6]],
                )
                .is_err());

            // agents 表自身 CHECK：合法记录可插入，非法名称/空描述被拒。
            connection
                .execute(
                    "INSERT INTO agents(id, name, description, prompt) VALUES (?1, 'code-reviewer', '审查代码', '正文')",
                    params![AGENT_ONE_ID],
                )
                .unwrap();
            for (index, bad_name) in ["Bad_Name", "-lead", "a b", "名前"].into_iter().enumerate() {
                assert!(
                    connection
                        .execute(
                            "INSERT INTO agents(id, name, description, prompt)
                             VALUES (?1, ?2, '描述', '正文')",
                            params![CANARY_IDS[index + 7], bad_name],
                        )
                        .is_err(),
                        "非法名称 {bad_name} 必须被 agents.name CHECK 拒绝"
                );
            }
            assert!(connection
                .execute(
                    "INSERT INTO agents(id, name, description, prompt)
                     VALUES (?1, 'valid-name', '', '正文')",
                    params![CANARY_IDS[11]],
                )
                .is_err(), "空描述必须被拒绝");

            // 分配 + 互斥触发器：全局分配后项目分配被拒；删除被 RESTRICT 保护。
            connection
                .execute(
                    "INSERT INTO agent_global_assignments(tool, agent_id) VALUES ('claude', ?1)",
                    params![AGENT_ONE_ID],
                )
                .unwrap();
            assert!(
                connection
                    .execute(
                        "INSERT INTO agent_project_assignments(project_id, tool, agent_id)
                         VALUES (?1, 'claude', ?2)",
                        params![PROJECT_ONE_ID, AGENT_ONE_ID],
                    )
                    .is_err(),
                "全局继承与项目分配必须互斥"
            );
            assert!(
                connection
                    .execute("DELETE FROM agents WHERE id = ?1", [AGENT_ONE_ID])
                    .is_err(),
                "仍有分配的 agent 必须被 RESTRICT 保护"
            );

            // row_version bump 触发器生效。
            connection
                .execute(
                    "UPDATE agents SET description = description WHERE id = ?1",
                    [AGENT_ONE_ID],
                )
                .unwrap();
            let version: i64 = connection
                .query_row(
                    "SELECT row_version FROM agents WHERE id = ?1",
                    [AGENT_ONE_ID],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version, 2);

            // 索引与外键完整性。
            let index_count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'index' AND name = 'idx_agent_project_assignments_tool_agent'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(index_count, 1);
            let fk_violations: i64 = connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(fk_violations, 0);

            // 清理本轮金丝雀行，保证重开（第二次循环）能重复全部断言。
            connection
                .execute(
                    "DELETE FROM agent_global_assignments WHERE agent_id = ?1",
                    [AGENT_ONE_ID],
                )
                .unwrap();
            connection
                .execute("DELETE FROM agents WHERE id = ?1", [AGENT_ONE_ID])
                .unwrap();
            connection
                .execute(
                    "DELETE FROM managed_targets WHERE artifact_kind = 'agent'",
                    [],
                )
                .unwrap();
        }
    }

    #[test]
    fn agent_tool_settings_migration_upgrades_v22_and_preserves_agents() {
        const AGENT_ID: &str = "00000000-0000-4000-8000-000000000298";
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("v22-agent-settings-data")).unwrap();
        paths.initialize().unwrap();
        super::prepare_database_file(paths.database()).unwrap();
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
            for migration in &super::MIGRATIONS[..22] {
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
                    "INSERT INTO agents(id, name, description, prompt)
                     VALUES (?1, 'legacy-agent', '旧记录', '旧正文')",
                    [AGENT_ID],
                )
                .unwrap();
        }

        let database = Database::open(&paths).unwrap();
        assert_eq!(database.schema_version().unwrap(), 24);
        let record: (String, String) = database
            .connection()
            .query_row(
                "SELECT name, prompt FROM agents WHERE id = ?1",
                [AGENT_ID],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(record, ("legacy-agent".to_owned(), "旧正文".to_owned()));
        let settings: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM agent_tool_settings WHERE agent_id = ?1",
                [AGENT_ID],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(settings, 0);
    }

    #[test]
    fn agent_tool_settings_table_enforces_json_and_cascades_on_delete() {
        let (_temporary, paths, mut database) = open_isolated_database();
        let agent_id = "00000000-0000-4000-8000-000000000299";
        {
            let connection = database.connection_mut();
            connection
                .execute(
                    "INSERT INTO agents(id, name, description, prompt) VALUES (?1, 'settings-agent', '描述', '正文')",
                    [agent_id],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO agent_tool_settings(agent_id, tool, settings_json)
                     VALUES (?1, 'claude', '{\"color\":\"cyan\"}')",
                    [agent_id],
                )
                .unwrap();
            assert!(connection
                .execute(
                    "INSERT INTO agent_tool_settings(agent_id, tool, settings_json)
                     VALUES (?1, 'cursor', '{}')",
                    [agent_id],
                )
                .is_err());
            assert!(connection
                .execute(
                    "INSERT INTO agent_tool_settings(agent_id, tool, settings_json)
                     VALUES (?1, 'codex', '[]')",
                    [agent_id],
                )
                .is_err());
            connection
                .execute("DELETE FROM agents WHERE id = ?1", [agent_id])
                .unwrap();
            let remaining: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM agent_tool_settings WHERE agent_id = ?1",
                    [agent_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(remaining, 0);
        }
        let reopened = Database::open(&paths).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), 24);
    }
}
