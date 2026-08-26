//! MCP 导入证据与单来源原子接管，绝不写入原生配置。

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    db::{mcp, Database},
    domain::{EntityId, Tool},
    error::AppError,
    mcp::{McpImportResultDto, ValidatedMcpConfiguration},
    sync::{hash_json, ManagedTargetBaseline},
};

pub(crate) struct McpImportPreviewRecord {
    pub id: String,
    pub tool: Tool,
    pub target_path: String,
    pub observed_full_hash: String,
    pub context_json: String,
    pub redacted_preview_json: String,
    pub status: String,
}

pub(crate) struct ImportedMcpItem {
    pub configuration: ValidatedMcpConfiguration,
    pub reuse_id: Option<String>,
    pub item_hash: String,
}

pub(crate) fn persist_preview(
    database: &Database,
    record: &McpImportPreviewRecord,
) -> Result<(), AppError> {
    database
        .connection()
        .execute(
            "INSERT INTO mcp_import_previews(id, tool, target_path, observed_full_hash,
             context_json, redacted_preview_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id,
                record.tool.as_str(),
                record.target_path,
                record.observed_full_hash,
                record.context_json,
                record.redacted_preview_json
            ],
        )
        .map_err(|_| {
            AppError::database(&database.path().to_string_lossy(), "persist_mcp_import")
        })?;
    Ok(())
}

pub(crate) fn get_preview(
    database: &Database,
    id: &str,
) -> Result<McpImportPreviewRecord, AppError> {
    EntityId::parse(id)?;
    database.connection().query_row(
        "SELECT id, tool, target_path, observed_full_hash, context_json, redacted_preview_json, status
         FROM mcp_import_previews WHERE id = ?1", [id], |row| {
            let tool: String = row.get(1)?;
            Ok(McpImportPreviewRecord {
                id: row.get(0)?,
                tool: match tool.as_str() {
                    "claude" => Tool::Claude,
                    "codex" => Tool::Codex,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                },
                target_path: row.get(2)?, observed_full_hash: row.get(3)?,
                context_json: row.get(4)?, redacted_preview_json: row.get(5)?, status: row.get(6)?,
            })
        },
    ).optional().map_err(|_| AppError::database(&database.path().to_string_lossy(), "get_mcp_import"))?
        .ok_or_else(|| AppError::not_found("mcpImportPreview", id))
}

/// 名称匹配可能涉及整个中央列表；同时绑定分配和管理元数据，兼顾其它进程。
/// 指纹仅含身份/版本，原始配置和秘密值不进入导入证据。
pub(crate) fn state_fingerprint(
    connection: &Connection,
    tool: Tool,
    target_path: &str,
) -> Result<String, AppError> {
    let read_error = |_| AppError::database(target_path, "read_mcp_import_state");
    let mut state = Vec::new();
    let mut servers = connection
        .prepare("SELECT id, row_version FROM mcp_servers ORDER BY id")
        .map_err(read_error)?;
    let rows = servers
        .query_map([], |row| {
            Ok(json!([row.get::<_, String>(0)?, row.get::<_, i64>(1)?]))
        })
        .map_err(read_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_error)?;
    state.push(json!(rows));
    for sql in [
        "SELECT mcp_id, '' FROM mcp_global_assignments WHERE tool = ?1 ORDER BY mcp_id",
        "SELECT mcp_id, project_id FROM mcp_project_assignments WHERE tool = ?1 ORDER BY mcp_id, project_id",
    ] {
        let mut statement = connection.prepare(sql).map_err(read_error)?;
        let rows = statement.query_map([tool.as_str()], |row| Ok(json!([
            row.get::<_, String>(0)?, row.get::<_, String>(1)?
        ]))).map_err(read_error)?.collect::<Result<Vec<_>, _>>().map_err(read_error)?;
        state.push(json!(rows));
    }
    for sql in [
        "SELECT id, row_version FROM managed_targets WHERE tool = ?1 AND artifact_kind = 'mcp'
         AND scope = 'global' AND project_id IS NULL AND target_path = ?2 ORDER BY id",
        "SELECT item.id, item.row_version FROM managed_items item JOIN managed_targets target
         ON target.id = item.target_id WHERE target.tool = ?1 AND target.artifact_kind = 'mcp'
         AND target.scope = 'global' AND target.project_id IS NULL AND target.target_path = ?2 ORDER BY item.id",
    ] {
        let mut statement = connection.prepare(sql).map_err(read_error)?;
        let rows = statement.query_map(params![tool.as_str(), target_path], |row| Ok(json!([
            row.get::<_, String>(0)?, row.get::<_, i64>(1)?
        ]))).map_err(read_error)?.collect::<Result<Vec<_>, _>>().map_err(read_error)?;
        state.push(json!(rows));
    }
    Ok(hash_json(&json!(state)))
}

pub(crate) fn has_project_assignment(
    database: &Database,
    tool: Tool,
    mcp_id: &str,
) -> Result<bool, AppError> {
    database
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM mcp_project_assignments WHERE tool = ?1 AND mcp_id = ?2)",
            params![tool.as_str(), mcp_id],
            |row| row.get(0),
        )
        .map_err(|_| {
            AppError::database(
                &database.path().to_string_lossy(),
                "read_import_project_assignment",
            )
        })
}

pub(crate) fn adopt_import(
    database: &mut Database,
    preview: &McpImportPreviewRecord,
    expected_state: &str,
    baseline: Option<&ManagedTargetBaseline>,
    projection: &Value,
    items: &[ImportedMcpItem],
    validate_source: impl Fn() -> Result<(), AppError>,
) -> Result<McpImportResultDto, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&path, "begin_mcp_import"))?;
    let actual = transaction.query_row(
        "SELECT tool, target_path, observed_full_hash, context_json, status FROM mcp_import_previews WHERE id = ?1",
        [&preview.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
    ).optional().map_err(|_| AppError::database(&path, "validate_mcp_import"))?
        .ok_or_else(|| AppError::not_found("mcpImportPreview", &preview.id))?;
    if actual.4 != "previewed" {
        return Err(AppError::preview_already_consumed(&preview.id, &actual.4));
    }
    if actual.0 != preview.tool.as_str()
        || actual.1 != preview.target_path
        || actual.2 != preview.observed_full_hash
        || actual.3 != preview.context_json
        || state_fingerprint(&transaction, preview.tool, &preview.target_path)? != expected_state
    {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let writer = transaction
        .query_row(
            "SELECT id, status FROM sync_runs WHERE status IN ('applying', 'restoring', 'rollback_failed') LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| AppError::database(&path, "check_mcp_import_writer"))?;
    if let Some((id, status)) = writer {
        return Err(AppError::write_in_progress(&id, &status));
    }
    // 取得写锁可能等待其它连接，不能沿用等待前的原生读取结果。
    validate_source()?;
    let target_id = baseline
        .map(|value| value.target_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let projection_json = serde_json::to_string(projection)
        .map_err(|_| AppError::invalid_input("import", "导入基线无法序列化"))?;
    let managed_hash = hash_json(projection);
    if let Some(baseline) = baseline {
        let updated = transaction.execute(
            "UPDATE managed_targets SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
             baseline_projection_json = ?4, last_status = 'in_sync' WHERE id = ?1 AND row_version = ?5",
            params![target_id, preview.observed_full_hash, managed_hash, projection_json, baseline.target_row_version],
        ).map_err(|_| AppError::database(&path, "extend_mcp_import_baseline"))?;
        if updated != 1 {
            return Err(AppError::stale_preview(&preview.id, &preview.target_path));
        }
    } else {
        transaction
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path,
             baseline_full_hash, baseline_managed_hash, baseline_projection_json, last_status)
             VALUES (?1, ?2, 'mcp', 'global', ?3, ?4, ?5, ?6, 'in_sync')",
                params![
                    target_id,
                    preview.tool.as_str(),
                    preview.target_path,
                    preview.observed_full_hash,
                    managed_hash,
                    projection_json
                ],
            )
            .map_err(|_| AppError::database(&path, "adopt_mcp_import_baseline"))?;
    }
    let mut result = McpImportResultDto {
        tool: preview.tool,
        created_count: 0,
        reused_count: 0,
        assigned_count: 0,
    };
    for item in items {
        let id = if let Some(id) = &item.reuse_id {
            result.reused_count += 1;
            id.clone()
        } else {
            result.created_count += 1;
            mcp::insert_mcp_configuration(&transaction, &item.configuration, &path)?
        };
        let assigned = transaction
            .execute(
                "INSERT OR IGNORE INTO mcp_global_assignments(tool, mcp_id) VALUES (?1, ?2)",
                params![preview.tool.as_str(), id],
            )
            .map_err(|error| mcp::map_mcp_write_error(error, &path, "assign_mcp_import"))?;
        if assigned > 0 {
            result.assigned_count += 1;
            transaction
                .execute(
                    "UPDATE mcp_servers SET updated_at = updated_at WHERE id = ?1",
                    [&id],
                )
                .map_err(|_| AppError::database(&path, "touch_imported_mcp"))?;
        }
        transaction.execute(
            "INSERT INTO managed_items(id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash)
             VALUES (?1, ?2, 'mcp', ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), target_id, id, item.configuration.name, item.item_hash],
        ).map_err(|_| AppError::conflict("import", "原生 MCP 的管理关系已变化"))?;
    }
    // 文件不受 SQLite 锁保护，入库期间源文件变化必须让整批回滚。
    validate_source()?;
    let consumed = transaction.execute(
        "UPDATE mcp_import_previews SET status = 'consumed', consumed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ?1 AND status = 'previewed'", [&preview.id],
    ).map_err(|_| AppError::database(&path, "consume_mcp_import"))?;
    if consumed != 1 {
        return Err(AppError::preview_already_consumed(&preview.id, "consumed"));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&path, "commit_mcp_import"))?;
    Ok(result)
}
