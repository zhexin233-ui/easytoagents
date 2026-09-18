//! 应用级全局设置。与 onboarding 一样属于单例偏好数据，不参与乐观并发控制；
//! 当前设置的未知存储取值按数据损坏处理，旧版本的 apply_mode 键则仅作为历史数据保留。

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    db::Database,
    domain::Tool,
    error::{AppError, ErrorCode},
};

/// 为旧版本保留的历史存储键。当前设置合同不会读取或写入该值。
pub const APPLY_MODE_KEY: &str = "apply_mode";
pub const ENABLED_TOOLS_KEY: &str = "enabled_tools";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub enabled_tools: Vec<Tool>,
}

impl Default for AppSettingsDto {
    fn default() -> Self {
        Self {
            enabled_tools: vec![Tool::Claude, Tool::Codex],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppSettingsInput {
    pub enabled_tools: Vec<Tool>,
}

pub fn load_app_settings(database: &Database) -> Result<AppSettingsDto, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let connection = database.connection();
    let read_setting = |key: &str| -> Result<Option<String>, AppError> {
        connection
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| {
                AppError::database(&database_path, "read_app_setting").with_source(error)
            })
    };
    let enabled_tools = match read_setting(ENABLED_TOOLS_KEY)? {
        Some(stored) => {
            let parsed: Vec<Tool> = serde_json::from_str(&stored).map_err(|error| {
                AppError::new(ErrorCode::DatabaseError, "应用设置包含未知取值", false)
                    .with_source(error)
            })?;
            parsed
        }
        None => vec![Tool::Claude, Tool::Codex],
    };
    Ok(AppSettingsDto { enabled_tools })
}

pub fn save_app_settings(
    database: &mut Database,
    input: &UpdateAppSettingsInput,
) -> Result<AppSettingsDto, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let enabled_tools = serde_json::to_string(&input.enabled_tools).map_err(|error| {
        AppError::database(&database_path, "encode_app_setting").with_source(error)
    })?;
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_write_app_setting").with_source(error)
        })?;
    transaction
        .execute(
            "INSERT INTO app_settings(key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![ENABLED_TOOLS_KEY, enabled_tools],
        )
        .map_err(|error| {
            AppError::database(&database_path, "write_app_setting").with_source(error)
        })?;
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_write_app_setting").with_source(error)
    })?;
    Ok(AppSettingsDto {
        enabled_tools: input.enabled_tools.clone(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        load_app_settings, save_app_settings, AppSettingsDto, UpdateAppSettingsInput,
        APPLY_MODE_KEY, ENABLED_TOOLS_KEY,
    };
    use crate::{app::AppPaths, db::Database, domain::Tool, error::ErrorCode};

    fn open_database() -> (tempfile::TempDir, Database) {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        (temporary, Database::open(&paths).unwrap())
    }

    #[test]
    fn missing_settings_fall_back_to_defaults() {
        let (_temporary, database) = open_database();
        assert_eq!(
            load_app_settings(&database).unwrap(),
            AppSettingsDto {
                enabled_tools: vec![Tool::Claude, Tool::Codex],
            }
        );
    }

    #[test]
    fn saved_settings_round_trip_in_both_directions() {
        let (_temporary, mut database) = open_database();
        let first = save_app_settings(
            &mut database,
            &UpdateAppSettingsInput {
                enabled_tools: vec![Tool::Claude, Tool::Codex],
            },
        )
        .unwrap();
        assert_eq!(first.enabled_tools, vec![Tool::Claude, Tool::Codex]);
        assert_eq!(load_app_settings(&database).unwrap(), first);

        let second = save_app_settings(
            &mut database,
            &UpdateAppSettingsInput {
                enabled_tools: vec![Tool::Claude, Tool::Cursor],
            },
        )
        .unwrap();
        assert_eq!(second.enabled_tools, vec![Tool::Claude, Tool::Cursor]);
        assert_eq!(load_app_settings(&database).unwrap(), second);
    }

    #[test]
    fn saved_settings_survive_reopen() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        {
            let mut database = Database::open(&paths).unwrap();
            save_app_settings(
                &mut database,
                &UpdateAppSettingsInput {
                    enabled_tools: vec![Tool::Claude, Tool::Cursor],
                },
            )
            .unwrap();
        }

        let reopened = Database::open(&paths).unwrap();
        assert_eq!(
            load_app_settings(&reopened).unwrap(),
            AppSettingsDto {
                enabled_tools: vec![Tool::Claude, Tool::Cursor],
            }
        );
    }

    #[test]
    fn legacy_apply_mode_is_ignored_and_preserved() {
        let (_temporary, mut database) = open_database();
        database
            .connection_mut()
            .execute(
                "INSERT INTO app_settings(key, value) VALUES (?1, 'legacy_value')",
                [APPLY_MODE_KEY],
            )
            .unwrap();
        assert_eq!(
            load_app_settings(&database).unwrap(),
            AppSettingsDto {
                enabled_tools: vec![Tool::Claude, Tool::Codex],
            }
        );

        save_app_settings(
            &mut database,
            &UpdateAppSettingsInput {
                enabled_tools: vec![Tool::Claude],
            },
        )
        .unwrap();
        let stored: String = database
            .connection()
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                [APPLY_MODE_KEY],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, "legacy_value");
    }

    #[test]
    fn stored_enabled_tools_bogus_json_is_a_database_error() {
        let (_temporary, mut database) = open_database();
        database
            .connection_mut()
            .execute(
                "INSERT INTO app_settings(key, value) VALUES (?1, 'bogus')",
                [ENABLED_TOOLS_KEY],
            )
            .unwrap();
        let error = load_app_settings(&database).unwrap_err();
        assert_eq!(error.code(), ErrorCode::DatabaseError);
    }

    #[test]
    fn stored_enabled_tools_with_unknown_tool_is_a_database_error() {
        let (_temporary, mut database) = open_database();
        database
            .connection_mut()
            .execute(
                "INSERT INTO app_settings(key, value) VALUES (?1, '[\"claude\",\"windsurf\"]')",
                [ENABLED_TOOLS_KEY],
            )
            .unwrap();
        let error = load_app_settings(&database).unwrap_err();
        assert_eq!(error.code(), ErrorCode::DatabaseError);
    }
}
