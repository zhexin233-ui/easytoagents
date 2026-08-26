//! 全局 Skills 导入的私有证据；不持有原生安装的管理关系。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use crate::{
    domain::{EntityId, Tool},
    error::AppError,
    sync::hash_json,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillImportPreviewRecord {
    pub id: String,
    pub tool: Tool,
    pub context_json: String,
    pub status: String,
}

pub(crate) fn persist_preview(
    connection: &Connection,
    record: &SkillImportPreviewRecord,
    display_json: &str,
) -> Result<(), AppError> {
    connection.execute(
        "INSERT INTO skill_import_previews(id, tool, context_json, redacted_preview_json) VALUES (?1, ?2, ?3, ?4)",
        params![record.id, record.tool.as_str(), record.context_json, display_json],
    ).map_err(|_| AppError::database("skill_import_previews", "persist_skill_import"))?;
    Ok(())
}

pub(crate) fn get_preview(
    connection: &Connection,
    id: &str,
) -> Result<SkillImportPreviewRecord, AppError> {
    EntityId::parse(id)?;
    connection
        .query_row(
            "SELECT tool, context_json, status FROM skill_import_previews WHERE id = ?1",
            [id],
            |row| {
                let tool: String = row.get(0)?;
                Ok(SkillImportPreviewRecord {
                    id: id.to_owned(),
                    tool: match tool.as_str() {
                        "claude" => Tool::Claude,
                        "codex" => Tool::Codex,
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    },
                    context_json: row.get(1)?,
                    status: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|_| AppError::database("skill_import_previews", "get_skill_import"))?
        .ok_or_else(|| AppError::not_found("skillImportPreview", id))
}

pub(crate) fn state_fingerprint(connection: &Connection) -> Result<String, AppError> {
    let mut statement = connection
        .prepare("SELECT id, row_version FROM skills ORDER BY id")
        .map_err(|_| AppError::database("skills", "read_skill_import_state"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!([row.get::<_, String>(0)?, row.get::<_, i64>(1)?]))
        })
        .map_err(|_| AppError::database("skills", "query_skill_import_state"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database("skills", "decode_skill_import_state"))?;
    Ok(hash_json(&json!(rows)))
}

pub(crate) fn validate_preview(
    connection: &Connection,
    expected: &SkillImportPreviewRecord,
    expected_state: &str,
) -> Result<(), AppError> {
    let actual = get_preview(connection, &expected.id)?;
    if actual.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &actual.id,
            &actual.status,
        ));
    }
    if actual != *expected || state_fingerprint(connection)? != expected_state {
        return Err(AppError::stale_preview(&expected.id, "skillImport"));
    }
    let writer = connection.query_row(
        "SELECT id, status FROM sync_runs WHERE status IN ('applying', 'restoring', 'rollback_failed') LIMIT 1", [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional().map_err(|_| AppError::database("sync_runs", "check_skill_import_writer"))?;
    if let Some((id, status)) = writer {
        return Err(AppError::write_in_progress(&id, &status));
    }
    Ok(())
}

pub(crate) fn consume_preview(connection: &Connection, id: &str) -> Result<(), AppError> {
    let changed = connection.execute(
        "UPDATE skill_import_previews SET status = 'consumed', consumed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1 AND status = 'previewed'", [id],
    ).map_err(|_| AppError::database("skill_import_previews", "consume_skill_import"))?;
    if changed != 1 {
        return Err(AppError::preview_already_consumed(id, "consumed"));
    }
    Ok(())
}
