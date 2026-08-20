//! Provider 与全局提示词档案的 SQLite 仓储。
//!
//! 本模块只维护应用中央意图，不读取或写入 Claude/Codex 原生配置。

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::{
    db::Database,
    domain::{ArtifactKind, Tool},
    error::AppError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfileRecord {
    pub id: String,
    pub tool: Tool,
    pub name: String,
    pub api_base_url: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
    pub config_json: String,
    pub is_active: bool,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProviderProfileRecord {
    pub id: String,
    pub tool: Tool,
    pub name: String,
    pub api_base_url: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
    pub config_json: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProfileRecord {
    pub id: String,
    pub tool: Tool,
    pub name: String,
    pub body: String,
    pub is_active: bool,
    pub imported_from_path: Option<String>,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPromptProfileRecord {
    pub id: String,
    pub tool: Tool,
    pub name: String,
    pub body: String,
    pub is_active: bool,
    pub imported_from_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreviewRecord {
    pub id: String,
    pub tool: Tool,
    pub artifact_kind: ArtifactKind,
    pub target_path: String,
    pub observed_full_hash: String,
    pub suggested_name: String,
    pub redacted_preview_json: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedBaselineRecord {
    pub target_id: String,
    pub target_path: String,
    pub full_hash: String,
    pub managed_hash: String,
    pub projection_json: String,
}

pub fn persist_import_preview(
    database: &mut Database,
    preview: &ImportPreviewRecord,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    database
        .connection_mut()
        .execute(
            "INSERT INTO profile_import_previews(
                id, tool, artifact_kind, target_path, observed_full_hash,
                suggested_name, redacted_preview_json, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'previewed')",
            params![
                preview.id,
                preview.tool.as_str(),
                preview.artifact_kind.as_str(),
                preview.target_path,
                preview.observed_full_hash,
                preview.suggested_name,
                preview.redacted_preview_json,
            ],
        )
        .map_err(|_| AppError::database(&database_path, "persist_profile_import_preview"))?;
    Ok(())
}

pub fn get_import_preview(
    database: &Database,
    preview_id: &str,
) -> Result<ImportPreviewRecord, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, tool, artifact_kind, target_path, observed_full_hash,
                    suggested_name, redacted_preview_json, status
             FROM profile_import_previews WHERE id = ?1",
            [preview_id],
            |row| {
                Ok(ImportPreviewRecord {
                    id: row.get(0)?,
                    tool: tool_from_database(row.get(1)?)?,
                    artifact_kind: artifact_kind_from_database(row.get(2)?)?,
                    target_path: row.get(3)?,
                    observed_full_hash: row.get(4)?,
                    suggested_name: row.get(5)?,
                    redacted_preview_json: row.get(6)?,
                    status: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "get_profile_import_preview"))?
        .ok_or_else(|| AppError::not_found("profileImportPreview", preview_id))
}

pub fn adopt_imported_provider(
    database: &mut Database,
    preview: &ImportPreviewRecord,
    profile: &NewProviderProfileRecord,
    baseline: &ImportedBaselineRecord,
) -> Result<ProviderProfileRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_adopt_provider_import"))?;
    validate_import_preview(
        &transaction,
        preview,
        ArtifactKind::Provider,
        &database_path,
    )?;
    reject_existing_profiles(
        &transaction,
        "provider_profiles",
        profile.tool,
        &database_path,
    )?;
    reject_provider_name_conflict(
        &transaction,
        profile.tool,
        &profile.name,
        None,
        &database_path,
    )?;
    transaction
        .execute(
            "INSERT INTO provider_profiles(
                id, tool, name, api_base_url, api_key, default_model, config_json, is_active
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                profile.id,
                profile.tool.as_str(),
                profile.name,
                profile.api_base_url,
                profile.api_key,
                profile.default_model,
                profile.config_json,
            ],
        )
        .map_err(|error| {
            map_profile_write_error(error, &database_path, "adopt_provider_profile")
        })?;
    adopt_baseline(
        &transaction,
        profile.tool,
        ArtifactKind::Provider,
        baseline,
        &database_path,
    )?;
    consume_import_preview(&transaction, &preview.id, &database_path)?;
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_adopt_provider_import"))?;
    get_provider_profile(database, &profile.id)
}

pub fn adopt_imported_prompt(
    database: &mut Database,
    preview: &ImportPreviewRecord,
    profile: &NewPromptProfileRecord,
    baseline: &ImportedBaselineRecord,
) -> Result<PromptProfileRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_adopt_prompt_import"))?;
    validate_import_preview(&transaction, preview, ArtifactKind::Prompt, &database_path)?;
    reject_existing_profiles(
        &transaction,
        "prompt_profiles",
        profile.tool,
        &database_path,
    )?;
    reject_prompt_name_conflict(
        &transaction,
        profile.tool,
        &profile.name,
        None,
        &database_path,
    )?;
    transaction
        .execute(
            "INSERT INTO prompt_profiles(id, tool, name, body, is_active, imported_from_path)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![
                profile.id,
                profile.tool.as_str(),
                profile.name,
                profile.body,
                profile.imported_from_path,
            ],
        )
        .map_err(|error| map_profile_write_error(error, &database_path, "adopt_prompt_profile"))?;
    adopt_baseline(
        &transaction,
        profile.tool,
        ArtifactKind::Prompt,
        baseline,
        &database_path,
    )?;
    consume_import_preview(&transaction, &preview.id, &database_path)?;
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_adopt_prompt_import"))?;
    get_prompt_profile(database, &profile.id)
}

pub fn list_provider_profiles(
    database: &Database,
    tool: Tool,
) -> Result<Vec<ProviderProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, tool, name, api_base_url, api_key, default_model, config_json,
                    is_active, row_version
             FROM provider_profiles WHERE tool = ?1
             ORDER BY is_active DESC, name COLLATE NOCASE, id",
        )
        .map_err(|_| AppError::database(&database_path, "prepare_list_provider_profiles"))?;
    let rows = statement
        .query_map([tool.as_str()], provider_from_row)
        .map_err(|_| AppError::database(&database_path, "query_list_provider_profiles"))?;
    rows.map(|row| row.map_err(|_| AppError::database(&database_path, "read_provider_profile")))
        .collect()
}

pub fn get_provider_profile(
    database: &Database,
    id: &str,
) -> Result<ProviderProfileRecord, AppError> {
    find_provider_profile(database, id)?.ok_or_else(|| AppError::not_found("providerProfile", id))
}

pub fn find_active_provider_profile(
    database: &Database,
    tool: Tool,
) -> Result<Option<ProviderProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, tool, name, api_base_url, api_key, default_model, config_json,
                    is_active, row_version
             FROM provider_profiles WHERE tool = ?1 AND is_active = 1",
            [tool.as_str()],
            provider_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "find_active_provider_profile"))
}

pub fn insert_provider_profile(
    database: &mut Database,
    record: &NewProviderProfileRecord,
) -> Result<ProviderProfileRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_insert_provider_profile"))?;
    reject_provider_name_conflict(
        &transaction,
        record.tool,
        &record.name,
        None,
        &database_path,
    )?;
    if record.is_active {
        deactivate_provider_profiles(&transaction, record.tool, None, &database_path)?;
    }
    transaction
        .execute(
            "INSERT INTO provider_profiles(
                id, tool, name, api_base_url, api_key, default_model, config_json, is_active
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                record.tool.as_str(),
                record.name,
                record.api_base_url,
                record.api_key,
                record.default_model,
                record.config_json,
                record.is_active,
            ],
        )
        .map_err(|error| {
            map_profile_write_error(error, &database_path, "insert_provider_profile")
        })?;
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_insert_provider_profile"))?;
    get_provider_profile(database, &record.id)
}

#[allow(clippy::too_many_arguments)]
pub fn update_provider_profile(
    database: &mut Database,
    id: &str,
    name: &str,
    api_base_url: Option<&str>,
    api_key: Option<&str>,
    default_model: Option<&str>,
    config_json: &str,
    expected_row_version: i64,
) -> Result<ProviderProfileRecord, AppError> {
    let current = get_provider_profile(database, id)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_update_provider_profile"))?;
    reject_provider_name_conflict(&transaction, current.tool, name, Some(id), &database_path)?;
    let updated = transaction
        .execute(
            "UPDATE provider_profiles
             SET name = ?2, api_base_url = ?3, api_key = ?4,
                 default_model = ?5, config_json = ?6
             WHERE id = ?1 AND row_version = ?7",
            params![
                id,
                name,
                api_base_url,
                api_key,
                default_model,
                config_json,
                expected_row_version,
            ],
        )
        .map_err(|error| {
            map_profile_write_error(error, &database_path, "update_provider_profile")
        })?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "Provider 档案已被其他操作更新",
        ));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_update_provider_profile"))?;
    get_provider_profile(database, id)
}

pub fn set_active_provider_profile(
    database: &mut Database,
    tool: Tool,
    id: &str,
    expected_row_version: i64,
) -> Result<ProviderProfileRecord, AppError> {
    let current = get_provider_profile(database, id)?;
    if current.tool != tool {
        return Err(AppError::invalid_input(
            "tool",
            "Provider 档案不属于目标工具",
        ));
    }
    if current.row_version != expected_row_version {
        return Err(AppError::conflict(
            "rowVersion",
            "Provider 档案已被其他操作更新",
        ));
    }
    if current.is_active {
        return Ok(current);
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_activate_provider_profile"))?;
    deactivate_provider_profiles(&transaction, tool, Some(id), &database_path)?;
    let updated = transaction
        .execute(
            "UPDATE provider_profiles SET is_active = 1
             WHERE id = ?1 AND tool = ?2 AND row_version = ?3",
            params![id, tool.as_str(), expected_row_version],
        )
        .map_err(|_| AppError::database(&database_path, "activate_provider_profile"))?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "Provider 档案已被其他操作更新",
        ));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_activate_provider_profile"))?;
    get_provider_profile(database, id)
}

pub fn delete_provider_profile(
    database: &mut Database,
    id: &str,
    expected_row_version: i64,
) -> Result<(), AppError> {
    delete_profile_row(
        database,
        "provider_profiles",
        "providerProfile",
        id,
        expected_row_version,
    )
}

pub fn list_prompt_profiles(
    database: &Database,
    tool: Tool,
) -> Result<Vec<PromptProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, tool, name, body, is_active, imported_from_path, row_version
             FROM prompt_profiles WHERE tool = ?1
             ORDER BY is_active DESC, name COLLATE NOCASE, id",
        )
        .map_err(|_| AppError::database(&database_path, "prepare_list_prompt_profiles"))?;
    let rows = statement
        .query_map([tool.as_str()], prompt_from_row)
        .map_err(|_| AppError::database(&database_path, "query_list_prompt_profiles"))?;
    rows.map(|row| row.map_err(|_| AppError::database(&database_path, "read_prompt_profile")))
        .collect()
}

pub fn get_prompt_profile(database: &Database, id: &str) -> Result<PromptProfileRecord, AppError> {
    find_prompt_profile(database, id)?.ok_or_else(|| AppError::not_found("promptProfile", id))
}

pub fn find_active_prompt_profile(
    database: &Database,
    tool: Tool,
) -> Result<Option<PromptProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, tool, name, body, is_active, imported_from_path, row_version
             FROM prompt_profiles WHERE tool = ?1 AND is_active = 1",
            [tool.as_str()],
            prompt_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "find_active_prompt_profile"))
}

pub fn insert_prompt_profile(
    database: &mut Database,
    record: &NewPromptProfileRecord,
) -> Result<PromptProfileRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_insert_prompt_profile"))?;
    reject_prompt_name_conflict(
        &transaction,
        record.tool,
        &record.name,
        None,
        &database_path,
    )?;
    if record.is_active {
        deactivate_prompt_profiles(&transaction, record.tool, None, &database_path)?;
    }
    transaction
        .execute(
            "INSERT INTO prompt_profiles(id, tool, name, body, is_active, imported_from_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id,
                record.tool.as_str(),
                record.name,
                record.body,
                record.is_active,
                record.imported_from_path,
            ],
        )
        .map_err(|error| map_profile_write_error(error, &database_path, "insert_prompt_profile"))?;
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_insert_prompt_profile"))?;
    get_prompt_profile(database, &record.id)
}

pub fn update_prompt_profile(
    database: &mut Database,
    id: &str,
    name: &str,
    body: &str,
    expected_row_version: i64,
) -> Result<PromptProfileRecord, AppError> {
    let current = get_prompt_profile(database, id)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_update_prompt_profile"))?;
    reject_prompt_name_conflict(&transaction, current.tool, name, Some(id), &database_path)?;
    let updated = transaction
        .execute(
            "UPDATE prompt_profiles SET name = ?2, body = ?3
             WHERE id = ?1 AND row_version = ?4",
            params![id, name, body, expected_row_version],
        )
        .map_err(|error| map_profile_write_error(error, &database_path, "update_prompt_profile"))?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "提示词档案已被其他操作更新",
        ));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_update_prompt_profile"))?;
    get_prompt_profile(database, id)
}

pub fn set_active_prompt_profile(
    database: &mut Database,
    tool: Tool,
    id: &str,
    expected_row_version: i64,
) -> Result<PromptProfileRecord, AppError> {
    let current = get_prompt_profile(database, id)?;
    if current.tool != tool {
        return Err(AppError::invalid_input("tool", "提示词档案不属于目标工具"));
    }
    if current.row_version != expected_row_version {
        return Err(AppError::conflict(
            "rowVersion",
            "提示词档案已被其他操作更新",
        ));
    }
    if current.is_active {
        return Ok(current);
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&database_path, "begin_activate_prompt_profile"))?;
    deactivate_prompt_profiles(&transaction, tool, Some(id), &database_path)?;
    let updated = transaction
        .execute(
            "UPDATE prompt_profiles SET is_active = 1
             WHERE id = ?1 AND tool = ?2 AND row_version = ?3",
            params![id, tool.as_str(), expected_row_version],
        )
        .map_err(|_| AppError::database(&database_path, "activate_prompt_profile"))?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "提示词档案已被其他操作更新",
        ));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&database_path, "commit_activate_prompt_profile"))?;
    get_prompt_profile(database, id)
}

pub fn delete_prompt_profile(
    database: &mut Database,
    id: &str,
    expected_row_version: i64,
) -> Result<(), AppError> {
    delete_profile_row(
        database,
        "prompt_profiles",
        "promptProfile",
        id,
        expected_row_version,
    )
}

fn find_provider_profile(
    database: &Database,
    id: &str,
) -> Result<Option<ProviderProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, tool, name, api_base_url, api_key, default_model, config_json,
                    is_active, row_version
             FROM provider_profiles WHERE id = ?1",
            [id],
            provider_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "find_provider_profile"))
}

fn find_prompt_profile(
    database: &Database,
    id: &str,
) -> Result<Option<PromptProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, tool, name, body, is_active, imported_from_path, row_version
             FROM prompt_profiles WHERE id = ?1",
            [id],
            prompt_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "find_prompt_profile"))
}

fn provider_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderProfileRecord> {
    let tool = tool_from_database(row.get::<_, String>(1)?)?;
    Ok(ProviderProfileRecord {
        id: row.get(0)?,
        tool,
        name: row.get(2)?,
        api_base_url: row.get(3)?,
        api_key: row.get(4)?,
        default_model: row.get(5)?,
        config_json: row.get(6)?,
        is_active: row.get(7)?,
        row_version: row.get(8)?,
    })
}

fn prompt_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PromptProfileRecord> {
    let tool = tool_from_database(row.get::<_, String>(1)?)?;
    Ok(PromptProfileRecord {
        id: row.get(0)?,
        tool,
        name: row.get(2)?,
        body: row.get(3)?,
        is_active: row.get(4)?,
        imported_from_path: row.get(5)?,
        row_version: row.get(6)?,
    })
}

fn tool_from_database(value: String) -> rusqlite::Result<Tool> {
    match value.as_str() {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn artifact_kind_from_database(value: String) -> rusqlite::Result<ArtifactKind> {
    match value.as_str() {
        "provider" => Ok(ArtifactKind::Provider),
        "prompt" => Ok(ArtifactKind::Prompt),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn validate_import_preview(
    transaction: &Transaction<'_>,
    expected: &ImportPreviewRecord,
    artifact_kind: ArtifactKind,
    database_path: &str,
) -> Result<(), AppError> {
    let actual = transaction
        .query_row(
            "SELECT tool, artifact_kind, target_path, observed_full_hash, status
             FROM profile_import_previews WHERE id = ?1",
            [&expected.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(database_path, "validate_profile_import_preview"))?
        .ok_or_else(|| AppError::not_found("profileImportPreview", &expected.id))?;
    if actual.0 != expected.tool.as_str()
        || actual.1 != artifact_kind.as_str()
        || actual.2 != expected.target_path
        || actual.3 != expected.observed_full_hash
        || actual.4 != "previewed"
    {
        return Err(AppError::preview_already_consumed(&expected.id, &actual.4));
    }
    Ok(())
}

fn reject_existing_profiles(
    transaction: &Transaction<'_>,
    table: &str,
    tool: Tool,
    database_path: &str,
) -> Result<(), AppError> {
    let query = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE tool = ?1)");
    let exists = transaction
        .query_row(&query, [tool.as_str()], |row| row.get::<_, bool>(0))
        .map_err(|_| AppError::database(database_path, "check_existing_import_profiles"))?;
    if exists {
        Err(AppError::conflict(
            "import",
            "首次导入仅在该工具尚无中央档案时可确认",
        ))
    } else {
        Ok(())
    }
}

fn adopt_baseline(
    transaction: &Transaction<'_>,
    tool: Tool,
    artifact_kind: ArtifactKind,
    baseline: &ImportedBaselineRecord,
    database_path: &str,
) -> Result<(), AppError> {
    let existing = transaction
        .query_row(
            "SELECT id, baseline_full_hash, baseline_managed_hash
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = ?2 AND scope = 'global'
               AND project_id IS NULL AND target_path = ?3",
            params![tool.as_str(), artifact_kind.as_str(), baseline.target_path],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(database_path, "find_import_managed_target"))?;
    let target_id = match existing {
        Some((id, None, None)) => id,
        Some((_id, _, _)) => {
            return Err(AppError::conflict("import", "该原生目标已经建立受管基线"));
        }
        None => {
            transaction
                .execute(
                    "INSERT INTO managed_targets(
                        id, tool, artifact_kind, scope, project_id, target_path
                     ) VALUES (?1, ?2, ?3, 'global', NULL, ?4)",
                    params![
                        baseline.target_id,
                        tool.as_str(),
                        artifact_kind.as_str(),
                        baseline.target_path,
                    ],
                )
                .map_err(|_| AppError::database(database_path, "insert_import_managed_target"))?;
            baseline.target_id.clone()
        }
    };
    let updated = transaction
        .execute(
            "UPDATE managed_targets
             SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                 baseline_projection_json = ?4, last_status = 'in_sync'
             WHERE id = ?1 AND baseline_full_hash IS NULL AND baseline_managed_hash IS NULL",
            params![
                target_id,
                baseline.full_hash,
                baseline.managed_hash,
                baseline.projection_json,
            ],
        )
        .map_err(|_| AppError::database(database_path, "adopt_import_managed_baseline"))?;
    if updated != 1 {
        return Err(AppError::conflict("import", "原生目标受管基线已经变化"));
    }
    Ok(())
}

fn consume_import_preview(
    transaction: &Transaction<'_>,
    preview_id: &str,
    database_path: &str,
) -> Result<(), AppError> {
    let updated = transaction
        .execute(
            "UPDATE profile_import_previews
             SET status = 'consumed', consumed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status = 'previewed'",
            [preview_id],
        )
        .map_err(|_| AppError::database(database_path, "consume_profile_import_preview"))?;
    if updated != 1 {
        return Err(AppError::preview_already_consumed(
            preview_id,
            "not_previewed",
        ));
    }
    Ok(())
}

fn reject_provider_name_conflict(
    transaction: &Transaction<'_>,
    tool: Tool,
    name: &str,
    except_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    reject_name_conflict(
        transaction,
        "provider_profiles",
        tool,
        name,
        except_id,
        database_path,
    )
}

fn reject_prompt_name_conflict(
    transaction: &Transaction<'_>,
    tool: Tool,
    name: &str,
    except_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    reject_name_conflict(
        transaction,
        "prompt_profiles",
        tool,
        name,
        except_id,
        database_path,
    )
}

fn reject_name_conflict(
    transaction: &Transaction<'_>,
    table: &str,
    tool: Tool,
    name: &str,
    except_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    let query = format!(
        "SELECT EXISTS(SELECT 1 FROM {table}
         WHERE tool = ?1 AND name = ?2 COLLATE NOCASE AND (?3 IS NULL OR id != ?3))"
    );
    let exists = transaction
        .query_row(&query, params![tool.as_str(), name, except_id], |row| {
            row.get::<_, bool>(0)
        })
        .map_err(|_| AppError::database(database_path, "check_profile_name_conflict"))?;
    if exists {
        Err(AppError::conflict("name", "同一工具内的档案名称必须唯一"))
    } else {
        Ok(())
    }
}

fn deactivate_provider_profiles(
    transaction: &Transaction<'_>,
    tool: Tool,
    except_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    transaction
        .execute(
            "UPDATE provider_profiles SET is_active = 0
             WHERE tool = ?1 AND is_active = 1 AND (?2 IS NULL OR id != ?2)",
            params![tool.as_str(), except_id],
        )
        .map_err(|_| AppError::database(database_path, "deactivate_provider_profiles"))?;
    Ok(())
}

fn deactivate_prompt_profiles(
    transaction: &Transaction<'_>,
    tool: Tool,
    except_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    transaction
        .execute(
            "UPDATE prompt_profiles SET is_active = 0
             WHERE tool = ?1 AND is_active = 1 AND (?2 IS NULL OR id != ?2)",
            params![tool.as_str(), except_id],
        )
        .map_err(|_| AppError::database(database_path, "deactivate_prompt_profiles"))?;
    Ok(())
}

fn delete_profile_row(
    database: &mut Database,
    table: &str,
    resource: &'static str,
    id: &str,
    expected_row_version: i64,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let query = format!("DELETE FROM {table} WHERE id = ?1 AND row_version = ?2");
    let deleted = database
        .connection_mut()
        .execute(&query, params![id, expected_row_version])
        .map_err(|_| AppError::database(&database_path, "delete_profile"))?;
    if deleted == 1 {
        Ok(())
    } else {
        let exists_query = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
        let exists = database
            .connection()
            .query_row(&exists_query, [id], |row| row.get::<_, bool>(0))
            .map_err(|_| AppError::database(&database_path, "check_deleted_profile"))?;
        if exists {
            Err(AppError::conflict("rowVersion", "档案已被其他操作更新"))
        } else {
            Err(AppError::not_found(resource, id))
        }
    }
}

fn map_profile_write_error(
    error: rusqlite::Error,
    database_path: &str,
    operation: &'static str,
) -> AppError {
    if error
        .sqlite_error_code()
        .is_some_and(|code| code == rusqlite::ErrorCode::ConstraintViolation)
    {
        AppError::conflict("profile", "档案违反名称唯一或单一生效约束")
    } else {
        AppError::database(database_path, operation)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        delete_prompt_profile, delete_provider_profile, insert_prompt_profile,
        insert_provider_profile, list_prompt_profiles, list_provider_profiles,
        set_active_prompt_profile, set_active_provider_profile, update_prompt_profile,
        update_provider_profile, NewPromptProfileRecord, NewProviderProfileRecord,
    };
    use crate::{app::AppPaths, db::Database, domain::Tool};

    fn database() -> (tempfile::TempDir, Database) {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path())
            .unwrap()
            .join("private/data/root");
        let paths = AppPaths::from_data_root(root).unwrap();
        (temporary, Database::open(&paths).unwrap())
    }

    #[test]
    fn repositories_enforce_tool_scoped_names_and_single_active_profile() {
        let (_temporary, mut database) = database();
        let first = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Claude,
                name: "主渠道".to_owned(),
                api_base_url: Some("https://one.example.com".to_owned()),
                api_key: Some("fixture-provider-key-one".to_owned()),
                default_model: Some("claude-one".to_owned()),
                config_json: "{}".to_owned(),
                is_active: true,
            },
        )
        .unwrap();
        let second = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Claude,
                name: "Fallback".to_owned(),
                api_base_url: Some("https://two.example.com".to_owned()),
                api_key: Some("fixture-provider-key-two".to_owned()),
                default_model: Some("claude-two".to_owned()),
                config_json: "{}".to_owned(),
                is_active: false,
            },
        )
        .unwrap();
        set_active_provider_profile(&mut database, Tool::Claude, &second.id, second.row_version)
            .unwrap();
        let providers = list_provider_profiles(&database, Tool::Claude).unwrap();
        assert!(providers
            .iter()
            .any(|item| item.id == second.id && item.is_active));
        assert!(providers
            .iter()
            .any(|item| item.id == first.id && !item.is_active));

        let duplicate = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Claude,
                name: "fallback".to_owned(),
                api_base_url: None,
                api_key: None,
                default_model: None,
                config_json: "{}".to_owned(),
                is_active: false,
            },
        )
        .unwrap_err();
        assert_eq!(duplicate.code(), crate::error::ErrorCode::Conflict);

        let prompt_one = insert_prompt_profile(
            &mut database,
            &NewPromptProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Codex,
                name: "默认提示词".to_owned(),
                body: "第一份".to_owned(),
                is_active: true,
                imported_from_path: None,
            },
        )
        .unwrap();
        let prompt_two = insert_prompt_profile(
            &mut database,
            &NewPromptProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Codex,
                name: "审查提示词".to_owned(),
                body: "第二份".to_owned(),
                is_active: false,
                imported_from_path: None,
            },
        )
        .unwrap();
        set_active_prompt_profile(
            &mut database,
            Tool::Codex,
            &prompt_two.id,
            prompt_two.row_version,
        )
        .unwrap();
        let prompts = list_prompt_profiles(&database, Tool::Codex).unwrap();
        assert!(prompts
            .iter()
            .any(|item| item.id == prompt_two.id && item.is_active));
        assert!(prompts
            .iter()
            .any(|item| item.id == prompt_one.id && !item.is_active));
    }

    #[test]
    fn activate_and_delete_reject_stale_row_versions() {
        let (_temporary, mut database) = database();
        let provider = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Codex,
                name: "待更新渠道".to_owned(),
                api_base_url: Some("https://provider.example.com".to_owned()),
                api_key: Some("fixture-cas-provider-secret".to_owned()),
                default_model: Some("fixture-model".to_owned()),
                config_json: "{}".to_owned(),
                is_active: false,
            },
        )
        .unwrap();
        let updated = update_provider_profile(
            &mut database,
            &provider.id,
            "已更新渠道",
            provider.api_base_url.as_deref(),
            provider.api_key.as_deref(),
            provider.default_model.as_deref(),
            &provider.config_json,
            provider.row_version,
        )
        .unwrap();
        assert_eq!(
            set_active_provider_profile(
                &mut database,
                Tool::Codex,
                &provider.id,
                provider.row_version,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::Conflict
        );
        assert_eq!(
            delete_provider_profile(&mut database, &provider.id, provider.row_version)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::Conflict
        );
        delete_provider_profile(&mut database, &provider.id, updated.row_version).unwrap();

        let prompt = insert_prompt_profile(
            &mut database,
            &NewPromptProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Claude,
                name: "待更新提示词".to_owned(),
                body: "原正文".to_owned(),
                is_active: false,
                imported_from_path: None,
            },
        )
        .unwrap();
        let updated_prompt = update_prompt_profile(
            &mut database,
            &prompt.id,
            "已更新提示词",
            "新正文",
            prompt.row_version,
        )
        .unwrap();
        assert_eq!(
            set_active_prompt_profile(&mut database, Tool::Claude, &prompt.id, prompt.row_version,)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::Conflict
        );
        assert_eq!(
            delete_prompt_profile(&mut database, &prompt.id, prompt.row_version)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::Conflict
        );
        delete_prompt_profile(&mut database, &prompt.id, updated_prompt.row_version).unwrap();
    }
}
