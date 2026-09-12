//! 同步运行记录的持久化查询。

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedTargetBaselineRow {
    pub row_version: i64,
    pub baseline_full_hash: Option<String>,
    pub baseline_managed_hash: Option<String>,
}

pub(crate) fn load_managed_target_baseline(
    connection: &Connection,
    target_id: &str,
    database_path: &str,
) -> Result<Option<ManagedTargetBaselineRow>, AppError> {
    connection
        .query_row(
            "SELECT row_version, baseline_full_hash, baseline_managed_hash
             FROM managed_targets WHERE id = ?1",
            [target_id],
            |row| {
                Ok(ManagedTargetBaselineRow {
                    row_version: row.get(0)?,
                    baseline_full_hash: row.get(1)?,
                    baseline_managed_hash: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(database_path, "load_managed_target_baseline").with_source(error)
        })
}

pub(crate) struct SnapshotInsert<'a> {
    pub id: &'a str,
    pub run_id: &'a str,
    pub target_id: Option<&'a str>,
    pub target_path: &'a Path,
    pub snapshot_path: &'a Path,
    pub content_hash: Option<&'a str>,
    pub file_mode: Option<u32>,
    pub target_type: &'a str,
    pub link_target: Option<&'a Path>,
    pub storage_kind: &'a str,
}

pub(crate) fn insert_snapshot(
    connection: &Connection,
    database_path: &str,
    row: SnapshotInsert<'_>,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT INTO snapshots(
                id, run_id, target_id, target_path, snapshot_path, content_hash,
                file_mode, target_type, link_target, storage_kind
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                row.run_id,
                row.target_id,
                row.target_path.to_string_lossy(),
                row.snapshot_path.to_string_lossy(),
                row.content_hash,
                row.file_mode.map(i64::from),
                row.target_type,
                row.link_target.map(|path| path.to_string_lossy()),
                row.storage_kind,
            ],
        )
        .map(|_| ())
        .map_err(|error| AppError::database(database_path, "insert_snapshot").with_source(error))
}

/// 返回当前阻塞外部写入的最早同步运行。
pub(crate) fn active_writer(
    connection: &Connection,
    except_run: Option<&str>,
    database_path: &str,
) -> Result<Option<(String, String)>, AppError> {
    connection
        .query_row(
            "SELECT id, status FROM sync_runs
             WHERE status IN ('applying', 'restoring', 'rollback_failed')
               AND (?1 IS NULL OR id != ?1)
             ORDER BY CASE WHEN status IN ('applying', 'restoring') THEN 0 ELSE 1 END,
                      started_at
             LIMIT 1",
            [except_run],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database(database_path, "read_active_writer").with_source(error))
}

/// Skill 接管等短事务使用的写入者检查，复用 active-writer 查询并保留原错误语义。
pub(crate) fn reject_active_writer(
    connection: &Connection,
    database_path: &str,
) -> Result<(), AppError> {
    if let Some((id, status)) = active_writer(connection, None, database_path)? {
        return Err(AppError::write_in_progress(&id, &status));
    }
    Ok(())
}

/// 受管目标的稳定身份；同步 preview/apply/restore 都只能通过这个查询读取。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedTargetIdentityRow {
    pub row_version: i64,
    pub tool: String,
    pub artifact_kind: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub target_path: String,
    pub project_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestoreTargetIdentityRow {
    pub identity: ManagedTargetIdentityRow,
    pub redacted_diff_json: String,
}

pub(crate) fn load_managed_target_identity(
    connection: &Connection,
    target_id: &str,
    database_path: &str,
    operation: &str,
) -> Result<Option<ManagedTargetIdentityRow>, AppError> {
    connection
        .query_row(
            "SELECT target.row_version, target.tool, target.artifact_kind, target.scope,
                    target.project_id, target.target_path, project.root_path
             FROM managed_targets AS target
             LEFT JOIN projects AS project ON project.id = target.project_id
             WHERE target.id = ?1",
            [target_id],
            |row| {
                Ok(ManagedTargetIdentityRow {
                    row_version: row.get(0)?,
                    tool: row.get(1)?,
                    artifact_kind: row.get(2)?,
                    scope: row.get(3)?,
                    project_id: row.get(4)?,
                    target_path: row.get(5)?,
                    project_root: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|error| AppError::database(database_path, operation).with_source(error))
}

pub(crate) fn load_restore_target_identity(
    connection: &Connection,
    target_id: &str,
    run_id: &str,
    database_path: &str,
    operation: &str,
) -> Result<Option<RestoreTargetIdentityRow>, AppError> {
    connection
        .query_row(
            "SELECT target.row_version, target.tool, target.artifact_kind, target.scope,
                    target.project_id, target.target_path, project.root_path,
                    item.redacted_diff_json
             FROM managed_targets AS target
             JOIN sync_items AS item ON item.target_id = target.id AND item.run_id = ?2
             LEFT JOIN projects AS project ON project.id = target.project_id
             WHERE target.id = ?1",
            params![target_id, run_id],
            |row| {
                Ok(RestoreTargetIdentityRow {
                    identity: ManagedTargetIdentityRow {
                        row_version: row.get(0)?,
                        tool: row.get(1)?,
                        artifact_kind: row.get(2)?,
                        scope: row.get(3)?,
                        project_id: row.get(4)?,
                        target_path: row.get(5)?,
                        project_root: row.get(6)?,
                    },
                    redacted_diff_json: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|error| AppError::database(database_path, operation).with_source(error))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotSummaryRow {
    pub id: String,
    pub run_id: String,
    pub target_id: Option<String>,
    pub target_path: String,
    pub target_type: String,
    pub storage_kind: String,
    pub created_at: String,
}

pub(crate) fn list_snapshot_summaries(
    connection: &Connection,
    database_path: &str,
) -> Result<Vec<SnapshotSummaryRow>, AppError> {
    let mut statement = connection
        .prepare_cached(
            "SELECT id, run_id, target_id, target_path, target_type, storage_kind, created_at
             FROM snapshots ORDER BY created_at DESC, id DESC",
        )
        .map_err(|error| {
            AppError::database(database_path, "prepare_list_snapshots").with_source(error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok(SnapshotSummaryRow {
                id: row.get(0)?,
                run_id: row.get(1)?,
                target_id: row.get(2)?,
                target_path: row.get(3)?,
                target_type: row.get(4)?,
                storage_kind: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| {
            AppError::database(database_path, "query_list_snapshots").with_source(error)
        })?;
    rows.map(|row| {
        row.map_err(|error| {
            AppError::database(database_path, "read_snapshot_summary").with_source(error)
        })
    })
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotForDeleteRow {
    pub run_id: String,
    pub snapshot_path: String,
    pub storage_kind: String,
    pub content_hash: Option<String>,
}

pub(crate) fn load_snapshot_for_delete(
    connection: &Connection,
    snapshot_id: &str,
    database_path: &str,
) -> Result<Option<SnapshotForDeleteRow>, AppError> {
    connection
        .query_row(
            "SELECT run_id, snapshot_path, storage_kind, content_hash
             FROM snapshots WHERE id = ?1",
            [snapshot_id],
            |row| {
                Ok(SnapshotForDeleteRow {
                    run_id: row.get(0)?,
                    snapshot_path: row.get(1)?,
                    storage_kind: row.get(2)?,
                    content_hash: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(database_path, "load_snapshot_for_delete").with_source(error)
        })
}

pub(crate) fn has_active_sync_run(
    connection: &Connection,
    run_id: &str,
    database_path: &str,
) -> Result<bool, AppError> {
    connection
        .query_row(
            "SELECT 1 FROM sync_runs WHERE id = ?1
             AND status IN ('applying', 'restoring', 'rollback_failed')",
            [run_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|error| {
            AppError::database(database_path, "check_snapshot_run_active").with_source(error)
        })
}

/// 事务内登记私有快照清理并退役快照行；事务由调用方负责 begin/commit。
pub(crate) fn queue_and_delete_snapshot(
    transaction: &Transaction<'_>,
    snapshot_id: &str,
    run_id: &str,
    snapshot_path: &Path,
    storage_kind: &str,
    content_hash: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT OR REPLACE INTO retired_snapshot_cleanup(
                snapshot_id, run_id, snapshot_path, storage_kind, content_hash
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                snapshot_id,
                run_id,
                snapshot_path.to_string_lossy(),
                storage_kind,
                content_hash,
            ],
        )
        .map_err(|error| {
            AppError::database(database_path, "queue_snapshot_cleanup").with_source(error)
        })?;
    transaction
        .execute("DELETE FROM snapshots WHERE id = ?1", [snapshot_id])
        .map_err(|error| {
            AppError::database(database_path, "delete_snapshot_row").with_source(error)
        })?;
    Ok(())
}

pub(crate) fn dequeue_snapshot_cleanup(
    connection: &Connection,
    snapshot_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "DELETE FROM retired_snapshot_cleanup WHERE snapshot_id = ?1",
            [snapshot_id],
        )
        .map(|_| ())
        .map_err(|error| {
            AppError::database(database_path, "dequeue_snapshot_cleanup").with_source(error)
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveSyncRunRow {
    pub id: String,
    pub status: String,
    pub journal_path: Option<String>,
}

pub(crate) fn load_active_sync_run(
    connection: &Connection,
    database_path: &str,
) -> Result<Option<ActiveSyncRunRow>, AppError> {
    connection
        .query_row(
            "SELECT id, status, journal_path FROM sync_runs
             WHERE status IN ('applying', 'restoring', 'rollback_failed')
             ORDER BY CASE WHEN status IN ('applying', 'restoring') THEN 0 ELSE 1 END,
                      started_at
             LIMIT 1",
            [],
            |row| {
                Ok(ActiveSyncRunRow {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    journal_path: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(database_path, "detect_interrupted_run").with_source(error)
        })
}

pub(crate) fn managed_target_exists(
    connection: &Connection,
    target_id: &str,
    database_path: &str,
) -> Result<bool, AppError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM managed_targets WHERE id = ?1)",
            [target_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            AppError::database(database_path, "check_interrupted_target").with_source(error)
        })
}

pub(crate) fn update_readopt_target_baseline(
    connection: &Connection,
    target_id: &str,
    baseline_full_hash: &str,
    baseline_managed_hash: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "UPDATE managed_targets
             SET baseline_full_hash = ?2, baseline_managed_hash = ?3
             WHERE id = ?1",
            params![target_id, baseline_full_hash, baseline_managed_hash],
        )
        .map(|_| ())
        .map_err(|error| {
            AppError::database(database_path, "readopt_target_baseline").with_source(error)
        })
}

pub(crate) fn update_readopt_item_baseline(
    connection: &Connection,
    item_id: &str,
    target_id: &str,
    item_hash: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "UPDATE managed_items SET last_applied_item_hash = ?2
             WHERE id = ?1 AND target_id = ?3",
            params![item_id, item_hash, target_id],
        )
        .map(|_| ())
        .map_err(|error| {
            AppError::database(database_path, "readopt_item_baseline").with_source(error)
        })
}

pub(crate) fn delete_readopt_item(
    connection: &Connection,
    item_id: &str,
    target_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "DELETE FROM managed_items WHERE id = ?1 AND target_id = ?2",
            params![item_id, target_id],
        )
        .map(|_| ())
        .map_err(|error| {
            AppError::database(database_path, "readopt_remove_item").with_source(error)
        })
}

pub(crate) fn clear_readopt_items(
    connection: &Connection,
    target_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "DELETE FROM managed_items WHERE target_id = ?1",
            [target_id],
        )
        .map(|_| ())
        .map_err(|error| {
            AppError::database(database_path, "readopt_clear_items").with_source(error)
        })
}

pub(crate) fn clear_readopt_target_baseline(
    connection: &Connection,
    target_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "UPDATE managed_targets
             SET baseline_full_hash = NULL, baseline_managed_hash = NULL
             WHERE id = ?1",
            [target_id],
        )
        .map(|_| ())
        .map_err(|error| {
            AppError::database(database_path, "readopt_clear_baseline").with_source(error)
        })
}

pub(crate) struct SyncRunInsert<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub status: &'a str,
    pub scope: &'a str,
    pub project_id: Option<&'a str>,
    pub db_version: i64,
    pub database_path: &'a str,
    pub operation: &'a str,
}

pub(crate) fn insert_sync_run(
    connection: &Connection,
    row: SyncRunInsert<'_>,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.id,
                row.kind,
                row.status,
                row.scope,
                row.project_id,
                row.db_version
            ],
        )
        .map(|_| ())
        .map_err(|error| AppError::database(row.database_path, row.operation).with_source(error))
}

pub(crate) struct SyncItemInsert<'a> {
    pub id: &'a str,
    pub run_id: &'a str,
    pub target_id: &'a str,
    pub change_kind: &'a str,
    pub status: &'a str,
    pub redacted_diff_json: &'a str,
    pub warning_codes_json: &'a str,
    pub error_code: Option<&'a str>,
    pub target_order: i64,
    pub database_path: &'a str,
    pub operation: &'a str,
}

pub(crate) fn insert_sync_item(
    connection: &Connection,
    row: SyncItemInsert<'_>,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT INTO sync_items(
                id, run_id, target_id, change_kind, status,
                redacted_diff_json, warning_codes_json, error_code, target_order
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.id,
                row.run_id,
                row.target_id,
                row.change_kind,
                row.status,
                row.redacted_diff_json,
                row.warning_codes_json,
                row.error_code,
                row.target_order,
            ],
        )
        .map(|_| ())
        .map_err(|error| AppError::database(row.database_path, row.operation).with_source(error))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PersistedPreviewRunRow {
    pub scope: String,
    pub project_id: Option<String>,
    pub db_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PersistedPreviewItemRow {
    pub target_id: String,
    pub target_path: String,
    pub change_kind: String,
    pub status: String,
    pub redacted_diff_json: String,
    pub warning_codes_json: String,
    pub error_code: Option<String>,
}

pub(crate) fn load_persisted_preview_run(
    connection: &Connection,
    preview_id: &str,
    database_path: &str,
) -> Result<Option<PersistedPreviewRunRow>, AppError> {
    connection
        .query_row(
            "SELECT scope, project_id, db_version
             FROM sync_runs WHERE id = ?1",
            [preview_id],
            |row| {
                Ok(PersistedPreviewRunRow {
                    scope: row.get(0)?,
                    project_id: row.get(1)?,
                    db_version: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| AppError::database(database_path, "load_preview_run").with_source(error))
}

pub(crate) fn load_persisted_preview_items(
    connection: &Connection,
    preview_id: &str,
    database_path: &str,
) -> Result<Vec<PersistedPreviewItemRow>, AppError> {
    let mut statement = connection
        .prepare_cached(
            "SELECT item.target_id, target.target_path, item.change_kind, item.status,
                    item.redacted_diff_json, item.warning_codes_json, item.error_code
             FROM sync_items AS item
             JOIN managed_targets AS target ON target.id = item.target_id
             WHERE item.run_id = ?1 ORDER BY item.target_order, item.id",
        )
        .map_err(|error| {
            AppError::database(database_path, "prepare_preview_items").with_source(error)
        })?;
    let rows = statement
        .query_map([preview_id], |row| {
            Ok(PersistedPreviewItemRow {
                target_id: row.get(0)?,
                target_path: row.get(1)?,
                change_kind: row.get(2)?,
                status: row.get(3)?,
                redacted_diff_json: row.get(4)?,
                warning_codes_json: row.get(5)?,
                error_code: row.get(6)?,
            })
        })
        .map_err(|error| {
            AppError::database(database_path, "query_preview_items").with_source(error)
        })?;
    rows.map(|row| {
        row.map_err(|error| {
            AppError::database(database_path, "read_preview_item").with_source(error)
        })
    })
    .collect()
}

pub(crate) fn load_sync_run_kind_status(
    connection: &Connection,
    run_id: &str,
    database_path: &str,
    operation: &str,
) -> Result<Option<(String, String)>, AppError> {
    connection
        .query_row(
            "SELECT kind, status FROM sync_runs WHERE id = ?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database(database_path, operation).with_source(error))
}

pub(crate) fn load_sync_run_status(
    connection: &Connection,
    run_id: &str,
    kind: &str,
    database_path: &str,
    operation: &str,
) -> Result<Option<String>, AppError> {
    connection
        .query_row(
            "SELECT status FROM sync_runs WHERE id = ?1 AND kind = ?2",
            params![run_id, kind],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| AppError::database(database_path, operation).with_source(error))
}

pub(crate) fn update_sync_run_for_apply(
    connection: &Connection,
    run_id: &str,
    journal_path: &Path,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE sync_runs
             SET kind = 'apply', status = 'applying', journal_path = ?2
             WHERE id = ?1 AND kind = 'preview' AND status = 'previewed'",
            params![run_id, journal_path.to_string_lossy()],
        )
        .map_err(|error| AppError::write_in_progress(run_id, "applying").with_source(error))
}

pub(crate) fn mark_sync_run_stale(
    connection: &Connection,
    run_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "UPDATE sync_runs
             SET status = 'stale', error_code = 'STALE_PREVIEW',
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status = 'applying'",
            [run_id],
        )
        .map(|_| ())
        .map_err(|error| AppError::database(database_path, "mark_stale_preview").with_source(error))
}

pub(crate) fn settle_sync_run_error(
    connection: &Connection,
    run_id: &str,
    error_code: &str,
    database_path: &str,
) -> Result<(), AppError> {
    connection
        .execute(
            "UPDATE sync_runs
             SET status = 'failed', error_code = ?2,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status IN ('applying', 'restoring')",
            params![run_id, error_code],
        )
        .map(|_| ())
        .map_err(|error| AppError::database(database_path, "settle_apply_error").with_source(error))
}

pub(crate) fn has_snapshots_for_run(
    connection: &Connection,
    run_id: &str,
    database_path: &str,
) -> Result<bool, AppError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM snapshots WHERE run_id = ?1)",
            [run_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            AppError::database(database_path, "detect_run_snapshots").with_source(error)
        })
}

pub(crate) fn update_managed_target_baseline(
    connection: &Connection,
    target_id: &str,
    full_hash: Option<&str>,
    managed_hash: Option<&str>,
    projection_json: &str,
    expected_row_version: u32,
    database_path: &str,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE managed_targets
             SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                 baseline_projection_json = ?4, last_status = 'in_sync'
             WHERE id = ?1 AND row_version = ?5",
            params![
                target_id,
                full_hash,
                managed_hash,
                projection_json,
                expected_row_version
            ],
        )
        .map_err(|error| {
            AppError::database(database_path, "update_managed_baseline").with_source(error)
        })
}

pub(crate) fn mark_sync_item_in_sync(
    connection: &Connection,
    run_id: &str,
    target_id: &str,
    database_path: &str,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE sync_items SET status = 'in_sync', error_code = NULL
             WHERE run_id = ?1 AND target_id = ?2",
            params![run_id, target_id],
        )
        .map_err(|error| AppError::database(database_path, "finish_sync_item").with_source(error))
}

pub(crate) fn find_latest_snapshot_id(
    connection: &Connection,
    run_id: &str,
    target_id: &str,
    target_path: &str,
    database_path: &str,
) -> Result<String, AppError> {
    connection
        .query_row(
            "SELECT id FROM snapshots
             WHERE run_id = ?1 AND target_id = ?2 AND target_path = ?3
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            params![run_id, target_id, target_path],
            |row| row.get(0),
        )
        .map_err(|error| {
            AppError::database(database_path, "load_native_disable_snapshot").with_source(error)
        })
}

pub(crate) fn finish_sync_run(
    connection: &Connection,
    run_id: &str,
    expected_status: &str,
    database_path: &str,
    operation: &str,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE sync_runs
             SET status = 'succeeded', error_code = NULL,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status = ?2",
            params![run_id, expected_status],
        )
        .map_err(|error| AppError::database(database_path, operation).with_source(error))
}

pub(crate) fn update_sync_run_status(
    connection: &Connection,
    run_id: &str,
    status: &str,
    error_code: &str,
    database_path: &str,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE sync_runs
             SET status = ?2, error_code = ?3,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status IN ('applying', 'restoring')",
            params![run_id, status, error_code],
        )
        .map_err(|error| AppError::database(database_path, "update_failed_run").with_source(error))
}

pub(crate) fn mark_restored_target(
    connection: &Connection,
    target_id: &str,
    expected_row_version: u32,
    database_path: &str,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE managed_targets SET last_status = 'external_owned_change'
             WHERE id = ?1 AND row_version = ?2",
            params![target_id, expected_row_version],
        )
        .map_err(|error| {
            AppError::database(database_path, "mark_restored_target").with_source(error)
        })
}

pub(crate) fn retire_interrupted_run(
    connection: &Connection,
    run_id: &str,
    database_path: &str,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE sync_runs
             SET status = 'rollback_failed', error_code = 'ROLLBACK_FAILED',
                 finished_at = NULL
             WHERE id = ?1 AND status IN ('applying', 'restoring', 'rollback_failed')",
            [run_id],
        )
        .map_err(|error| {
            AppError::database(database_path, "retire_interrupted_run").with_source(error)
        })
}

pub(crate) fn claim_restore_run(
    connection: &Connection,
    run_id: &str,
    journal_path: &Path,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE sync_runs SET status = 'restoring', journal_path = ?2
             WHERE id = ?1 AND kind = 'restore' AND status = 'previewed'",
            params![run_id, journal_path.to_string_lossy()],
        )
        .map_err(|error| AppError::write_in_progress(run_id, "restoring").with_source(error))
}

pub(crate) fn finish_source_recovery(
    connection: &Connection,
    run_id: &str,
    resolved: bool,
    database_path: &str,
) -> Result<usize, AppError> {
    connection
        .execute(
            "UPDATE sync_runs
             SET status = CASE WHEN ?2 THEN 'rolled_back' ELSE 'rollback_failed' END,
                 error_code = CASE WHEN ?2 THEN NULL ELSE 'ROLLBACK_FAILED' END,
                 finished_at = CASE
                     WHEN ?2 THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     ELSE NULL
                 END
             WHERE id = ?1 AND status = 'rollback_failed'",
            params![run_id, resolved],
        )
        .map_err(|error| {
            AppError::database(database_path, "finish_source_recovery").with_source(error)
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotRecordRow {
    pub run_id: String,
    pub target_id: Option<String>,
    pub target_path: String,
    pub snapshot_path: String,
    pub content_hash: Option<String>,
    pub file_mode: Option<i64>,
    pub target_type: String,
    pub link_target: Option<String>,
    pub row_version: i64,
    pub storage_kind: String,
}

pub(crate) fn load_snapshot_record(
    connection: &Connection,
    snapshot_id: &str,
    database_path: &str,
) -> Result<Option<SnapshotRecordRow>, AppError> {
    connection
        .query_row(
            "SELECT run_id, target_id, target_path, snapshot_path, content_hash,
                    file_mode, target_type, link_target, row_version, storage_kind
             FROM snapshots WHERE id = ?1",
            [snapshot_id],
            |row| {
                Ok(SnapshotRecordRow {
                    run_id: row.get(0)?,
                    target_id: row.get(1)?,
                    target_path: row.get(2)?,
                    snapshot_path: row.get(3)?,
                    content_hash: row.get(4)?,
                    file_mode: row.get(5)?,
                    target_type: row.get(6)?,
                    link_target: row.get(7)?,
                    row_version: row.get(8)?,
                    storage_kind: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|error| AppError::database(database_path, "load_snapshot").with_source(error))
}

pub(crate) fn load_row_version(
    connection: &Connection,
    table: &str,
    entity_id: &str,
    database_path: &str,
    operation: &str,
) -> Result<Option<i64>, AppError> {
    let table = match table {
        "provider_profiles"
        | "prompt_profiles"
        | "mcp_servers"
        | "skills"
        | "agents"
        | "projects"
        | "hooks"
        | "managed_targets"
        | "managed_items"
        | "project_native_resources" => table,
        _ => {
            return Err(AppError::invalid_input("entityType", "数据库实体类型无效"));
        }
    };
    let query = format!("SELECT row_version FROM {table} WHERE id = ?1");
    connection
        .query_row(&query, [entity_id], |row| row.get::<_, i64>(0))
        .optional()
        .map_err(|error| AppError::database(database_path, operation).with_source(error))
}
