//! 项目登记仓储。项目移除采用软删除，以保留同步历史与恢复证据。

use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};

use crate::{db::Database, domain::EntityId, error::AppError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub is_git_repo: bool,
    pub codex_trust_status: String,
    pub last_scanned_at: Option<String>,
    pub row_version: u32,
    pub removed: bool,
}

pub struct ProjectScanUpdate<'a> {
    pub is_git_repo: bool,
    pub codex_trust_status: &'a str,
}

pub struct RemoveProjectResult {
    pub managed_target_count: u32,
}

pub fn list_registered_projects(database: &Database) -> Result<Vec<ProjectRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, display_name, root_path, is_git_repo, codex_trust_status,
                    last_scanned_at, row_version, removed_at IS NOT NULL
             FROM projects
             WHERE removed_at IS NULL
             ORDER BY display_name COLLATE NOCASE, root_path",
        )
        .map_err(|_| AppError::database(&database_path, "prepare_list_projects"))?;
    let projects = statement
        .query_map([], project_from_row)
        .map_err(|_| AppError::database(&database_path, "query_list_projects"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&database_path, "decode_list_projects"))?;
    Ok(projects)
}

pub fn get_registered_project(database: &Database, id: &str) -> Result<ProjectRecord, AppError> {
    EntityId::parse(id)?;
    get_project_where(database, "id = ?1 AND removed_at IS NULL", id)?
        .ok_or_else(|| AppError::not_found("project", id))
}

pub fn find_project_by_root(
    database: &Database,
    root_path: &str,
) -> Result<Option<ProjectRecord>, AppError> {
    get_project_where(database, "root_path = ?1 COLLATE NOCASE", root_path)
}

fn get_project_where(
    database: &Database,
    predicate: &str,
    value: &str,
) -> Result<Option<ProjectRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    let sql = format!(
        "SELECT id, display_name, root_path, is_git_repo, codex_trust_status,
                last_scanned_at, row_version, removed_at IS NOT NULL
         FROM projects WHERE {predicate}"
    );
    database
        .connection()
        .query_row(&sql, [value], project_from_row)
        .optional()
        .map_err(|_| AppError::database(&database_path, "get_project"))
}

pub fn insert_project(
    database: &mut Database,
    id: &str,
    display_name: &str,
    root_path: &str,
    scan: &ProjectScanUpdate<'_>,
) -> Result<ProjectRecord, AppError> {
    EntityId::parse(id)?;
    let database_path = database.path().to_string_lossy().into_owned();
    database
        .connection_mut()
        .execute(
            "INSERT INTO projects(
                id, display_name, root_path, is_git_repo, codex_trust_status, last_scanned_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![
                id,
                display_name,
                root_path,
                scan.is_git_repo,
                scan.codex_trust_status,
            ],
        )
        .map_err(|error| {
            if error.to_string().contains("projects.root_path") {
                AppError::conflict("rootPath", "该规范化项目目录已经登记")
            } else {
                AppError::database(&database_path, "insert_project")
            }
        })?;
    get_registered_project(database, id)
}

pub fn reactivate_project(
    database: &mut Database,
    id: &str,
    display_name: &str,
    scan: &ProjectScanUpdate<'_>,
    expected_row_version: u32,
) -> Result<ProjectRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE projects
             SET display_name = ?2, is_git_repo = ?3, codex_trust_status = ?4,
                 last_scanned_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), removed_at = NULL
             WHERE id = ?1 AND row_version = ?5 AND removed_at IS NOT NULL",
            params![
                id,
                display_name,
                scan.is_git_repo,
                scan.codex_trust_status,
                expected_row_version,
            ],
        )
        .map_err(|_| AppError::database(&database_path, "reactivate_project"))?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "项目登记状态已被其他操作更新",
        ));
    }
    get_registered_project(database, id)
}

pub fn update_project_scan(
    database: &mut Database,
    id: &str,
    display_name: Option<&str>,
    scan: &ProjectScanUpdate<'_>,
    expected_row_version: u32,
) -> Result<ProjectRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE projects
             SET display_name = COALESCE(?2, display_name), is_git_repo = ?3,
                 codex_trust_status = ?4,
                 last_scanned_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND row_version = ?5 AND removed_at IS NULL",
            params![
                id,
                display_name,
                scan.is_git_repo,
                scan.codex_trust_status,
                expected_row_version,
            ],
        )
        .map_err(|_| AppError::database(&database_path, "update_project_scan"))?;
    if updated != 1 {
        return Err(AppError::conflict("rowVersion", "项目已被其他操作更新"));
    }
    get_registered_project(database, id)
}

pub fn soft_remove_project(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
) -> Result<RemoveProjectResult, AppError> {
    EntityId::parse(id)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_remove_project"))?;
    let blocking_run = transaction
        .query_row(
            "SELECT id, status FROM sync_runs
             WHERE status IN ('applying', 'restoring', 'rollback_failed')
             ORDER BY started_at, id
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "check_project_active_run"))?;
    if let Some((run_id, status)) = blocking_run {
        return Err(AppError::write_in_progress(&run_id, &status));
    }
    let blocking_native = crate::db::native_resources::count_blocking_native_resources(
        &transaction,
        id,
        &database_path,
    )?;
    if blocking_native > 0 {
        return Err(AppError::conflict(
            "projectNativeResource",
            "项目仍有已禁用的原生资源，请先恢复后再移除登记",
        ));
    }
    let managed_target_count = transaction
        .query_row(
            "SELECT COUNT(*) FROM managed_targets WHERE project_id = ?1",
            [id],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|_| AppError::database(&database_path, "count_project_targets"))?;
    transaction
        .execute(
            "DELETE FROM mcp_project_assignments WHERE project_id = ?1",
            [id],
        )
        .map_err(|_| AppError::database(&database_path, "remove_project_mcp_assignments"))?;
    transaction
        .execute(
            "DELETE FROM skill_project_assignments WHERE project_id = ?1",
            [id],
        )
        .map_err(|_| AppError::database(&database_path, "remove_project_skill_assignments"))?;
    let updated = transaction
        .execute(
            "UPDATE projects
             SET removed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND row_version = ?2 AND removed_at IS NULL",
            params![id, expected_row_version],
        )
        .map_err(|_| AppError::database(&database_path, "remove_project"))?;
    if updated != 1 {
        return Err(AppError::conflict("rowVersion", "项目已被其他操作更新"));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_remove_project"))?;
    Ok(RemoveProjectResult {
        managed_target_count,
    })
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectRecord> {
    Ok(ProjectRecord {
        id: row.get(0)?,
        display_name: row.get(1)?,
        root_path: row.get(2)?,
        is_git_repo: row.get(3)?,
        codex_trust_status: row.get(4)?,
        last_scanned_at: row.get(5)?,
        row_version: row.get(6)?,
        removed: row.get(7)?,
    })
}
