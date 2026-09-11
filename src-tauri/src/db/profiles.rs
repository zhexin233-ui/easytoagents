//! Provider 与全局提示词档案的 SQLite 仓储。
//!
//! 本模块只维护应用中央意图，不读取或写入 Claude/Codex 原生配置。

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::{
    db::{column_tool, Database},
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
    pub name: String,
    pub body: String,
    pub is_active_claude: bool,
    pub is_active_codex: bool,
    pub is_active_zcode: bool,
    pub is_active_cursor: bool,
    pub is_active_opencode: bool,
    pub imported_from_path: Option<String>,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPromptProfileRecord {
    pub id: String,
    pub name: String,
    pub body: String,
    pub is_active_claude: bool,
    pub is_active_codex: bool,
    pub is_active_zcode: bool,
    pub is_active_cursor: bool,
    pub is_active_opencode: bool,
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
        .map_err(|error| {
            AppError::database(&database_path, "persist_profile_import_preview").with_source(error)
        })?;
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
                    tool: column_tool(row, 1)?,
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
        .map_err(|error| {
            AppError::database(&database_path, "get_profile_import_preview").with_source(error)
        })?
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
        .map_err(|error| {
            AppError::database(&database_path, "begin_adopt_provider_import").with_source(error)
        })?;
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
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_adopt_provider_import").with_source(error)
    })?;
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
        .map_err(|error| {
            AppError::database(&database_path, "begin_adopt_prompt_import").with_source(error)
        })?;
    validate_import_preview(&transaction, preview, ArtifactKind::Prompt, &database_path)?;
    reject_prompt_import_blocked(
        &transaction,
        preview.tool,
        &preview.target_path,
        &database_path,
    )?;
    reject_prompt_name_conflict(&transaction, &profile.name, None, &database_path)?;
    // 遗留列 tool 统一写 'central'（见迁移 0009）；启用位按导入来源工具设置，
    // 保证导入后同步呈 in_sync 而不是清空刚接管的文件。
    transaction
        .execute(
            "INSERT INTO prompt_profiles(id, tool, name, body, is_active_claude,
                                        is_active_codex, is_active_zcode, is_active_cursor,
                                        is_active_opencode, imported_from_path)
             VALUES (?1, 'central', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                profile.id,
                profile.name,
                profile.body,
                i64::from(profile.is_active_claude),
                i64::from(profile.is_active_codex),
                i64::from(profile.is_active_zcode),
                i64::from(profile.is_active_cursor),
                i64::from(profile.is_active_opencode),
                profile.imported_from_path,
            ],
        )
        .map_err(|error| map_profile_write_error(error, &database_path, "adopt_prompt_profile"))?;
    adopt_baseline(
        &transaction,
        preview.tool,
        ArtifactKind::Prompt,
        baseline,
        &database_path,
    )?;
    consume_import_preview(&transaction, &preview.id, &database_path)?;
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_adopt_prompt_import").with_source(error)
    })?;
    get_prompt_profile(database, &profile.id)
}

pub fn list_provider_profiles(
    database: &Database,
    tool: Tool,
) -> Result<Vec<ProviderProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT id, tool, name, api_base_url, api_key, default_model, config_json,
                    is_active, row_version
             FROM provider_profiles WHERE tool = ?1
             ORDER BY name COLLATE NOCASE, id",
        )
        .map_err(|error| {
            AppError::database(&database_path, "prepare_list_provider_profiles").with_source(error)
        })?;
    let rows = statement
        .query_map([tool.as_str()], provider_from_row)
        .map_err(|error| {
            AppError::database(&database_path, "query_list_provider_profiles").with_source(error)
        })?;
    rows.map(|row| {
        row.map_err(|error| {
            AppError::database(&database_path, "read_provider_profile").with_source(error)
        })
    })
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
        .map_err(|error| {
            AppError::database(&database_path, "find_active_provider_profile").with_source(error)
        })
}

pub fn insert_provider_profile(
    database: &mut Database,
    record: &NewProviderProfileRecord,
) -> Result<ProviderProfileRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_insert_provider_profile").with_source(error)
        })?;
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
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_insert_provider_profile").with_source(error)
    })?;
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
        .map_err(|error| {
            AppError::database(&database_path, "begin_update_provider_profile").with_source(error)
        })?;
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
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_update_provider_profile").with_source(error)
    })?;
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
        .map_err(|error| {
            AppError::database(&database_path, "begin_activate_provider_profile").with_source(error)
        })?;
    deactivate_provider_profiles(&transaction, tool, Some(id), &database_path)?;
    let updated = transaction
        .execute(
            "UPDATE provider_profiles SET is_active = 1
             WHERE id = ?1 AND tool = ?2 AND row_version = ?3",
            params![id, tool.as_str(), expected_row_version],
        )
        .map_err(|error| {
            AppError::database(&database_path, "activate_provider_profile").with_source(error)
        })?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "Provider 档案已被其他操作更新",
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_activate_provider_profile").with_source(error)
    })?;
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

pub fn list_prompt_profiles(database: &Database) -> Result<Vec<PromptProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT id, name, body, is_active_claude, is_active_codex, is_active_zcode,
                    is_active_cursor, is_active_opencode, imported_from_path, row_version
             FROM prompt_profiles
             ORDER BY name COLLATE NOCASE, id",
        )
        .map_err(|error| {
            AppError::database(&database_path, "prepare_list_prompt_profiles").with_source(error)
        })?;
    let rows = statement.query_map([], prompt_from_row).map_err(|error| {
        AppError::database(&database_path, "query_list_prompt_profiles").with_source(error)
    })?;
    rows.map(|row| {
        row.map_err(|error| {
            AppError::database(&database_path, "read_prompt_profile").with_source(error)
        })
    })
    .collect()
}

pub fn get_prompt_profile(database: &Database, id: &str) -> Result<PromptProfileRecord, AppError> {
    find_prompt_profile(database, id)?.ok_or_else(|| AppError::not_found("promptProfile", id))
}

/// 读取对该工具全局生效的提示词档案；未启用返回 `None`。
pub fn find_active_prompt_profile(
    database: &Database,
    tool: Tool,
) -> Result<Option<PromptProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, name, body, is_active_claude, is_active_codex, is_active_zcode,
                    is_active_cursor, is_active_opencode, imported_from_path, row_version
             FROM prompt_profiles
             WHERE (CASE WHEN ?1 = 'claude' THEN is_active_claude
                         WHEN ?1 = 'zcode' THEN is_active_zcode
                         WHEN ?1 = 'cursor' THEN is_active_cursor
                         WHEN ?1 = 'opencode' THEN is_active_opencode
                         ELSE is_active_codex END) = 1",
            [tool.as_str()],
            prompt_from_row,
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database_path, "find_active_prompt_profile").with_source(error)
        })
}

pub fn insert_prompt_profile(
    database: &mut Database,
    record: &NewPromptProfileRecord,
) -> Result<PromptProfileRecord, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_insert_prompt_profile").with_source(error)
        })?;
    reject_prompt_name_conflict(&transaction, &record.name, None, &database_path)?;
    // 遗留列 tool 统一写 'central'：档案不再绑定工具（CHECK 已由迁移 0009 放宽）。
    transaction
        .execute(
            "INSERT INTO prompt_profiles(id, tool, name, body, is_active_claude,
                                        is_active_codex, is_active_zcode, is_active_cursor,
                                        is_active_opencode, imported_from_path)
             VALUES (?1, 'central', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id,
                record.name,
                record.body,
                record.is_active_claude,
                record.is_active_codex,
                record.is_active_zcode,
                record.is_active_cursor,
                record.is_active_opencode,
                record.imported_from_path,
            ],
        )
        .map_err(|error| map_profile_write_error(error, &database_path, "insert_prompt_profile"))?;
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_insert_prompt_profile").with_source(error)
    })?;
    get_prompt_profile(database, &record.id)
}

pub fn update_prompt_profile(
    database: &mut Database,
    id: &str,
    name: &str,
    body: &str,
    expected_row_version: i64,
) -> Result<PromptProfileRecord, AppError> {
    get_prompt_profile(database, id)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_update_prompt_profile").with_source(error)
        })?;
    reject_prompt_name_conflict(&transaction, name, Some(id), &database_path)?;
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
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_update_prompt_profile").with_source(error)
    })?;
    get_prompt_profile(database, id)
}

/// 全局启用/停用一份提示词档案到指定工具（每工具至多一份生效）。
/// 启用会自动停用该工具的原生效档案；停用仅清自身标志位，幂等。
pub fn set_global_prompt_assignment(
    database: &mut Database,
    tool: Tool,
    id: &str,
    assigned: bool,
    expected_row_version: i64,
) -> Result<PromptProfileRecord, AppError> {
    let current = get_prompt_profile(database, id)?;
    if current.row_version != expected_row_version {
        return Err(AppError::conflict(
            "rowVersion",
            "提示词档案已被其他操作更新",
        ));
    }
    let already_assigned = match tool {
        Tool::Claude => current.is_active_claude,
        Tool::Codex => current.is_active_codex,
        Tool::Zcode => current.is_active_zcode,
        Tool::Cursor => current.is_active_cursor,
        Tool::Opencode => current.is_active_opencode,
    };
    if already_assigned == assigned {
        return Ok(current);
    }
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_set_global_prompt_assignment")
                .with_source(error)
        })?;
    if assigned {
        // 同一工具至多一份生效：先清旧启用，再置目标标志。
        deactivate_prompt_profiles(&transaction, tool, Some(id), &database_path)?;
    }
    let updated = match tool {
        Tool::Claude => transaction
            .execute(
                "UPDATE prompt_profiles SET is_active_claude = ?4
                 WHERE id = ?1 AND row_version = ?3 AND is_active_claude = ?5",
                params![
                    id,
                    tool.as_str(),
                    expected_row_version,
                    i64::from(assigned),
                    i64::from(!assigned),
                ],
            )
            .map_err(|error| {
                map_profile_write_error(error, &database_path, "set_global_prompt_assignment")
            })?,
        Tool::Codex => transaction
            .execute(
                "UPDATE prompt_profiles SET is_active_codex = ?4
                 WHERE id = ?1 AND row_version = ?3 AND is_active_codex = ?5",
                params![
                    id,
                    tool.as_str(),
                    expected_row_version,
                    i64::from(assigned),
                    i64::from(!assigned),
                ],
            )
            .map_err(|error| {
                map_profile_write_error(error, &database_path, "set_global_prompt_assignment")
            })?,
        Tool::Zcode => transaction
            .execute(
                "UPDATE prompt_profiles SET is_active_zcode = ?4
                 WHERE id = ?1 AND row_version = ?3 AND is_active_zcode = ?5",
                params![
                    id,
                    tool.as_str(),
                    expected_row_version,
                    i64::from(assigned),
                    i64::from(!assigned),
                ],
            )
            .map_err(|error| {
                map_profile_write_error(error, &database_path, "set_global_prompt_assignment")
            })?,
        Tool::Cursor => transaction
            .execute(
                "UPDATE prompt_profiles SET is_active_cursor = ?4
                 WHERE id = ?1 AND row_version = ?3 AND is_active_cursor = ?5",
                params![
                    id,
                    tool.as_str(),
                    expected_row_version,
                    i64::from(assigned),
                    i64::from(!assigned),
                ],
            )
            .map_err(|error| {
                map_profile_write_error(error, &database_path, "set_global_prompt_assignment")
            })?,
        Tool::Opencode => transaction
            .execute(
                "UPDATE prompt_profiles SET is_active_opencode = ?4
                 WHERE id = ?1 AND row_version = ?3 AND is_active_opencode = ?5",
                params![
                    id,
                    tool.as_str(),
                    expected_row_version,
                    i64::from(assigned),
                    i64::from(!assigned),
                ],
            )
            .map_err(|error| {
                map_profile_write_error(error, &database_path, "set_global_prompt_assignment")
            })?,
    };
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "提示词档案已被其他操作更新",
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_set_global_prompt_assignment").with_source(error)
    })?;
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
        .map_err(|error| {
            AppError::database(&database_path, "find_provider_profile").with_source(error)
        })
}

fn find_prompt_profile(
    database: &Database,
    id: &str,
) -> Result<Option<PromptProfileRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, name, body, is_active_claude, is_active_codex, is_active_zcode,
                    is_active_cursor, is_active_opencode, imported_from_path, row_version
             FROM prompt_profiles WHERE id = ?1",
            [id],
            prompt_from_row,
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database_path, "find_prompt_profile").with_source(error)
        })
}

fn provider_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderProfileRecord> {
    let tool = column_tool(row, 1)?;
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
    Ok(PromptProfileRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        body: row.get(2)?,
        is_active_claude: row.get(3)?,
        is_active_codex: row.get(4)?,
        is_active_zcode: row.get(5)?,
        is_active_cursor: row.get(6)?,
        is_active_opencode: row.get(7)?,
        imported_from_path: row.get(8)?,
        row_version: row.get(9)?,
    })
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
        .map_err(|error| {
            AppError::database(database_path, "validate_profile_import_preview").with_source(error)
        })?
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
        .map_err(|error| {
            AppError::database(database_path, "check_existing_import_profiles").with_source(error)
        })?;
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
        .map_err(|error| {
            AppError::database(database_path, "find_import_managed_target").with_source(error)
        })?;
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
                .map_err(|error| {
                    AppError::database(database_path, "insert_import_managed_target")
                        .with_source(error)
                })?;
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
        .map_err(|error| {
            AppError::database(database_path, "adopt_import_managed_baseline").with_source(error)
        })?;
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
        .map_err(|error| {
            AppError::database(database_path, "consume_profile_import_preview").with_source(error)
        })?;
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
    let exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM provider_profiles
             WHERE tool = ?1 AND name = ?2 COLLATE NOCASE AND (?3 IS NULL OR id != ?3))",
            params![tool.as_str(), name, except_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            AppError::database(database_path, "check_provider_name_conflict").with_source(error)
        })?;
    if exists {
        Err(AppError::conflict("name", "同一工具内的档案名称必须唯一"))
    } else {
        Ok(())
    }
}

/// 提示词档案工具无关化后名称全局唯一（不再按工具分域）。
fn reject_prompt_name_conflict(
    transaction: &Transaction<'_>,
    name: &str,
    except_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    let exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM prompt_profiles
             WHERE name = ?1 COLLATE NOCASE AND (?2 IS NULL OR id != ?2))",
            params![name, except_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            AppError::database(database_path, "check_prompt_name_conflict").with_source(error)
        })?;
    if exists {
        Err(AppError::conflict("name", "提示词档案名称必须唯一"))
    } else {
        Ok(())
    }
}

/// 提示词导入前置（只读版）：该工具已有生效档案、或已有同源导入档案时不可再导入。
pub fn prompt_import_blocked(
    database: &Database,
    tool: Tool,
    target_path: &str,
) -> Result<bool, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM prompt_profiles
             WHERE (CASE WHEN ?1 = 'claude' THEN is_active_claude
                         WHEN ?1 = 'zcode' THEN is_active_zcode
                         WHEN ?1 = 'cursor' THEN is_active_cursor
                         WHEN ?1 = 'opencode' THEN is_active_opencode
                         ELSE is_active_codex END) = 1
                OR imported_from_path = ?2)",
            params![tool.as_str(), target_path],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            AppError::database(&database_path, "check_prompt_import_blocked").with_source(error)
        })
}

/// 提示词导入前置：该工具已有生效档案、或已有同源导入档案时不可再导入。
fn reject_prompt_import_blocked(
    transaction: &Transaction<'_>,
    tool: Tool,
    target_path: &str,
    database_path: &str,
) -> Result<(), AppError> {
    let exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM prompt_profiles
             WHERE (CASE WHEN ?1 = 'claude' THEN is_active_claude
                         WHEN ?1 = 'zcode' THEN is_active_zcode
                         WHEN ?1 = 'cursor' THEN is_active_cursor
                         WHEN ?1 = 'opencode' THEN is_active_opencode
                         ELSE is_active_codex END) = 1
                OR imported_from_path = ?2)",
            params![tool.as_str(), target_path],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            AppError::database(database_path, "check_prompt_import_blocked").with_source(error)
        })?;
    if exists {
        Err(AppError::conflict(
            "import",
            "仅在该工具尚无生效或同源导入的提示词档案时可确认导入",
        ))
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
        .map_err(|error| {
            AppError::database(database_path, "deactivate_provider_profiles").with_source(error)
        })?;
    Ok(())
}

fn deactivate_prompt_profiles(
    transaction: &Transaction<'_>,
    tool: Tool,
    except_id: Option<&str>,
    database_path: &str,
) -> Result<(), AppError> {
    let column = match tool {
        Tool::Claude => "is_active_claude",
        Tool::Codex => "is_active_codex",
        Tool::Zcode => "is_active_zcode",
        Tool::Cursor => "is_active_cursor",
        Tool::Opencode => "is_active_opencode",
    };
    let query = format!(
        "UPDATE prompt_profiles SET {column} = 0
         WHERE {column} = 1 AND (?1 IS NULL OR id != ?1)"
    );
    transaction
        .execute(&query, params![except_id])
        .map_err(|error| {
            AppError::database(database_path, "deactivate_prompt_profiles").with_source(error)
        })?;
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
        .map_err(|error| AppError::database(&database_path, "delete_profile").with_source(error))?;
    if deleted == 1 {
        Ok(())
    } else {
        let exists_query = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
        let exists = database
            .connection()
            .query_row(&exists_query, [id], |row| row.get::<_, bool>(0))
            .map_err(|error| {
                AppError::database(&database_path, "check_deleted_profile").with_source(error)
            })?;
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
    let app_error = if error
        .sqlite_error_code()
        .is_some_and(|code| code == rusqlite::ErrorCode::ConstraintViolation)
    {
        AppError::conflict("profile", "档案违反名称唯一或单一生效约束")
    } else {
        AppError::database(database_path, operation)
    };
    app_error.with_source(error)
}

#[cfg(test)]
include!("profiles_tests.rs");
