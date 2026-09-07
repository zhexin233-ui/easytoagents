//! 项目原生资源观察记录、CAS 与快照引用保护。

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::{
    db::Database,
    domain::{ArtifactKind, EntityId, Tool},
    error::AppError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIdentityRecord {
    pub target_id: String,
    pub target_row_version: i64,
    pub full_hash: Option<String>,
    pub managed_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeResourceRecord {
    pub id: String,
    pub target_id: String,
    pub project_id: String,
    pub tool: String,
    pub artifact_kind: String,
    pub target_path: String,
    pub target_row_version: i64,
    pub external_key: String,
    pub entry_type: String,
    pub state: String,
    pub observed_item_hash: Option<String>,
    pub disabled_snapshot_id: Option<String>,
    pub disabled_at: Option<String>,
    pub last_seen_at: String,
    pub row_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub struct NativeResourceCounts {
    pub active: u32,
    pub disabled: u32,
    pub missing: u32,
    pub conflict: u32,
}

pub fn snapshot_is_referenced(
    connection: &rusqlite::Connection,
    snapshot_id: &str,
    database_path: &str,
) -> Result<bool, AppError> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM project_native_resources WHERE disabled_snapshot_id = ?1
             )",
            [snapshot_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::database(database_path, "check_native_snapshot_reference"))
}

pub fn count_blocking_native_resources(
    transaction: &Transaction<'_>,
    project_id: &str,
    database_path: &str,
) -> Result<u32, AppError> {
    transaction
        .query_row(
            "SELECT COUNT(*)
             FROM project_native_resources AS resource
             JOIN managed_targets AS target ON target.id = resource.target_id
             WHERE target.project_id = ?1
               AND resource.state IN ('disabled', 'conflict')",
            [project_id],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|_| AppError::database(database_path, "count_blocking_native_resources"))
}

#[allow(dead_code)]
pub fn count_for_project(
    database: &Database,
    project_id: &str,
) -> Result<NativeResourceCounts, AppError> {
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN resource.state = 'active' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN resource.state = 'disabled' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN resource.state = 'missing' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN resource.state = 'conflict' THEN 1 ELSE 0 END), 0)
             FROM project_native_resources AS resource
             JOIN managed_targets AS target ON target.id = resource.target_id
             WHERE target.project_id = ?1",
            [project_id],
            |row| {
                Ok(NativeResourceCounts {
                    active: row.get(0)?,
                    disabled: row.get(1)?,
                    missing: row.get(2)?,
                    conflict: row.get(3)?,
                })
            },
        )
        .map_err(|_| AppError::database(&path, "count_project_native_resources"))
}

pub fn list_for_project(
    database: &Database,
    project_id: &str,
    tool: Option<Tool>,
    artifact_kind: Option<ArtifactKind>,
) -> Result<Vec<NativeResourceRecord>, AppError> {
    EntityId::parse(project_id)?;
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT resource.id, resource.target_id, target.project_id, target.tool,
                    target.artifact_kind, target.target_path, target.row_version,
                    resource.external_key, resource.entry_type, resource.state,
                    resource.observed_item_hash, resource.disabled_snapshot_id,
                    resource.disabled_at, resource.last_seen_at, resource.row_version
             FROM project_native_resources AS resource
             JOIN managed_targets AS target ON target.id = resource.target_id
             WHERE target.project_id = ?1
               AND (?2 IS NULL OR target.tool = ?2)
               AND (?3 IS NULL OR target.artifact_kind = ?3)
             ORDER BY target.tool, target.artifact_kind, resource.external_key COLLATE NOCASE, resource.id",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_native_resources"))?;
    let records = statement
        .query_map(
            params![
                project_id,
                tool.map(Tool::as_str),
                artifact_kind.map(ArtifactKind::as_str),
            ],
            native_from_row,
        )
        .map_err(|_| AppError::database(&path, "query_list_native_resources"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_native_resources"))?;
    Ok(records)
}

pub fn get_by_id(database: &Database, id: &str) -> Result<NativeResourceRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT resource.id, resource.target_id, target.project_id, target.tool,
                    target.artifact_kind, target.target_path, target.row_version,
                    resource.external_key, resource.entry_type, resource.state,
                    resource.observed_item_hash, resource.disabled_snapshot_id,
                    resource.disabled_at, resource.last_seen_at, resource.row_version
             FROM project_native_resources AS resource
             JOIN managed_targets AS target ON target.id = resource.target_id
             WHERE resource.id = ?1",
            [id],
            native_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&path, "get_native_resource"))?
        .ok_or_else(|| AppError::not_found("projectNativeResource", id))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSnapshotRecord {
    pub id: String,
    pub snapshot_path: String,
    pub content_hash: Option<String>,
    pub file_mode: Option<i64>,
    pub target_type: String,
    pub link_target: Option<String>,
    pub storage_kind: String,
}

pub fn get_snapshot(database: &Database, id: &str) -> Result<NativeSnapshotRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, snapshot_path, content_hash, file_mode, target_type, link_target, storage_kind
             FROM snapshots WHERE id = ?1",
            [id],
            |row| {
                Ok(NativeSnapshotRecord {
                    id: row.get(0)?,
                    snapshot_path: row.get(1)?,
                    content_hash: row.get(2)?,
                    file_mode: row.get(3)?,
                    target_type: row.get(4)?,
                    link_target: row.get(5)?,
                    storage_kind: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|_| AppError::database(&path, "get_native_resource_snapshot"))?
        .ok_or_else(|| AppError::not_found("snapshot", id))
}

pub fn find_by_target_key(
    database: &Database,
    target_id: &str,
    external_key: &str,
) -> Result<Option<NativeResourceRecord>, AppError> {
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT resource.id, resource.target_id, target.project_id, target.tool,
                    target.artifact_kind, target.target_path, target.row_version,
                    resource.external_key, resource.entry_type, resource.state,
                    resource.observed_item_hash, resource.disabled_snapshot_id,
                    resource.disabled_at, resource.last_seen_at, resource.row_version
             FROM project_native_resources AS resource
             JOIN managed_targets AS target ON target.id = resource.target_id
             WHERE resource.target_id = ?1 AND resource.external_key = ?2",
            params![target_id, external_key],
            native_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&path, "find_native_resource"))
}

#[allow(dead_code)]
pub fn list_for_target(
    database: &Database,
    target_id: &str,
) -> Result<Vec<NativeResourceRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT resource.id, resource.target_id, target.project_id, target.tool,
                    target.artifact_kind, target.target_path, target.row_version,
                    resource.external_key, resource.entry_type, resource.state,
                    resource.observed_item_hash, resource.disabled_snapshot_id,
                    resource.disabled_at, resource.last_seen_at, resource.row_version
             FROM project_native_resources AS resource
             JOIN managed_targets AS target ON target.id = resource.target_id
             WHERE resource.target_id = ?1
             ORDER BY resource.external_key COLLATE NOCASE, resource.id",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_target_native_resources"))?;
    let records = statement
        .query_map([target_id], native_from_row)
        .map_err(|_| AppError::database(&path, "query_list_target_native_resources"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_target_native_resources"))?;
    Ok(records)
}

pub fn find_project_target_identity(
    database: &Database,
    project_id: &str,
    tool: Tool,
    artifact_kind: ArtifactKind,
    target_path: &str,
) -> Result<Option<TargetIdentityRecord>, AppError> {
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, row_version, baseline_full_hash, baseline_managed_hash
             FROM managed_targets
             WHERE project_id = ?1 AND tool = ?2 AND artifact_kind = ?3
               AND scope = 'project' AND target_path = ?4",
            params![
                project_id,
                tool.as_str(),
                artifact_kind.as_str(),
                target_path
            ],
            |row| {
                Ok(TargetIdentityRecord {
                    target_id: row.get(0)?,
                    target_row_version: row.get(1)?,
                    full_hash: row.get(2)?,
                    managed_hash: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|_| AppError::database(&path, "find_project_target_identity"))
}

pub fn insert_project_target_identity(
    database: &mut Database,
    project_id: &str,
    tool: Tool,
    artifact_kind: ArtifactKind,
    target_path: &str,
) -> Result<TargetIdentityRecord, AppError> {
    if let Some(existing) =
        find_project_target_identity(database, project_id, tool, artifact_kind, target_path)?
    {
        return Ok(existing);
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let id = EntityId::new().to_string();
    database
        .connection_mut()
        .execute(
            "INSERT INTO managed_targets(
                id, tool, artifact_kind, scope, project_id, target_path
             ) VALUES (?1, ?2, ?3, 'project', ?4, ?5)",
            params![
                id,
                tool.as_str(),
                artifact_kind.as_str(),
                project_id,
                target_path,
            ],
        )
        .map_err(|_| AppError::database(&database_path, "insert_project_target_identity"))?;
    find_project_target_identity(database, project_id, tool, artifact_kind, target_path)?
        .ok_or_else(|| AppError::database(&database_path, "reload_project_target_identity"))
}

pub fn upsert_observed_active(
    database: &mut Database,
    target_id: &str,
    external_key: &str,
    entry_type: &str,
    observed_item_hash: &str,
) -> Result<NativeResourceRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_upsert_native_resource"))?;
    let existing = transaction
        .query_row(
            "SELECT id, state FROM project_native_resources
             WHERE target_id = ?1 AND external_key = ?2",
            params![target_id, external_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "read_native_resource_for_upsert"))?;
    let id = match existing {
        Some((id, state)) if state == "disabled" => {
            transaction
                .execute(
                    "UPDATE project_native_resources
                     SET state = 'conflict', entry_type = ?2, last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?1 AND state = 'disabled'",
                    params![id, entry_type],
                )
                .map_err(|_| AppError::database(&database_path, "mark_native_resource_conflict"))?;
            id
        }
        Some((id, state)) if state == "conflict" => {
            transaction
                .execute(
                    "UPDATE project_native_resources
                     SET entry_type = ?2, last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?1 AND state = 'conflict'",
                    params![id, entry_type],
                )
                .map_err(|_| {
                    AppError::database(&database_path, "touch_native_resource_conflict")
                })?;
            id
        }
        Some((id, _)) => {
            transaction
                .execute(
                    "UPDATE project_native_resources
                     SET state = 'active', entry_type = ?2, observed_item_hash = ?3,
                         disabled_snapshot_id = NULL, disabled_at = NULL,
                         last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?1 AND state IN ('active', 'missing')",
                    params![id, entry_type, observed_item_hash],
                )
                .map_err(|_| AppError::database(&database_path, "update_native_resource_active"))?;
            id
        }
        None => {
            let id = EntityId::new().to_string();
            transaction
                .execute(
                    "INSERT INTO project_native_resources(
                        id, target_id, external_key, entry_type, state, observed_item_hash,
                        last_seen_at
                     ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    params![id, target_id, external_key, entry_type, observed_item_hash],
                )
                .map_err(|_| AppError::database(&database_path, "insert_native_resource"))?;
            id
        }
    };
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_upsert_native_resource"))?;
    get_by_id(database, &id)
}

pub fn mark_active_missing(
    database: &mut Database,
    target_id: &str,
    remaining_keys: &[String],
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let placeholders = remaining_keys
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = if remaining_keys.is_empty() {
        "UPDATE project_native_resources
         SET state = 'missing', observed_item_hash = NULL,
             last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE target_id = ?1 AND state = 'active'"
            .to_owned()
    } else {
        format!(
            "UPDATE project_native_resources
             SET state = 'missing', observed_item_hash = NULL,
                 last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE target_id = ?1 AND state = 'active'
               AND external_key NOT IN ({placeholders})"
        )
    };
    let mut statement = database
        .connection_mut()
        .prepare(&sql)
        .map_err(|_| AppError::database(&database_path, "prepare_mark_native_missing"))?;
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&target_id];
    for key in remaining_keys {
        params.push(key);
    }
    statement
        .execute(params.as_slice())
        .map_err(|_| AppError::database(&database_path, "mark_native_missing"))?;
    Ok(())
}

pub fn restore_conflict_when_vacant(
    database: &mut Database,
    target_id: &str,
    occupied_keys: &[String],
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let placeholders = occupied_keys
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = if occupied_keys.is_empty() {
        "UPDATE project_native_resources
         SET state = 'disabled', last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE target_id = ?1 AND state = 'conflict'"
            .to_owned()
    } else {
        format!(
            "UPDATE project_native_resources
             SET state = 'disabled', last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE target_id = ?1 AND state = 'conflict'
               AND external_key NOT IN ({placeholders})"
        )
    };
    let mut statement = database
        .connection_mut()
        .prepare(&sql)
        .map_err(|_| AppError::database(&database_path, "prepare_restore_native_conflict"))?;
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&target_id];
    for key in occupied_keys {
        params.push(key);
    }
    statement
        .execute(params.as_slice())
        .map_err(|_| AppError::database(&database_path, "restore_native_conflict"))?;
    Ok(())
}

pub fn mark_disabled_in_transaction(
    transaction: &Transaction<'_>,
    resource_id: &str,
    expected_row_version: u32,
    snapshot_id: &str,
    observed_item_hash: &str,
    database_path: &str,
) -> Result<(), AppError> {
    let updated = transaction
        .execute(
            "UPDATE project_native_resources
             SET state = 'disabled', disabled_snapshot_id = ?2, observed_item_hash = ?3,
                 disabled_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                 last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND row_version = ?4 AND state = 'active'",
            params![
                resource_id,
                snapshot_id,
                observed_item_hash,
                expected_row_version
            ],
        )
        .map_err(|_| AppError::database(database_path, "mark_native_resource_disabled"))?;
    if updated != 1 {
        return Err(AppError::stale_preview("persisted", resource_id));
    }
    Ok(())
}

pub fn mark_restored_in_transaction(
    transaction: &Transaction<'_>,
    resource_id: &str,
    expected_row_version: u32,
    observed_item_hash: &str,
    database_path: &str,
) -> Result<(), AppError> {
    let updated = transaction
        .execute(
            "UPDATE project_native_resources
             SET state = 'active', disabled_snapshot_id = NULL, disabled_at = NULL,
                 observed_item_hash = ?2,
                 last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND row_version = ?3 AND state = 'disabled'",
            params![resource_id, observed_item_hash, expected_row_version],
        )
        .map_err(|_| AppError::database(database_path, "mark_native_resource_restored"))?;
    if updated != 1 {
        return Err(AppError::stale_preview("persisted", resource_id));
    }
    Ok(())
}

fn native_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NativeResourceRecord> {
    Ok(NativeResourceRecord {
        id: row.get(0)?,
        target_id: row.get(1)?,
        project_id: row.get(2)?,
        tool: row.get(3)?,
        artifact_kind: row.get(4)?,
        target_path: row.get(5)?,
        target_row_version: row.get(6)?,
        external_key: row.get(7)?,
        entry_type: row.get(8)?,
        state: row.get(9)?,
        observed_item_hash: row.get(10)?,
        disabled_snapshot_id: row.get(11)?,
        disabled_at: row.get(12)?,
        last_seen_at: row.get(13)?,
        row_version: row.get(14)?,
    })
}
