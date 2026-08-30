//! 应用级全局设置。与 onboarding 一样属于单例偏好数据，不参与乐观并发控制；
//! 未知存储取值按数据损坏处理，绝不静默回退默认值。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    db::Database,
    error::{AppError, ErrorCode},
};

pub const APPLY_MODE_KEY: &str = "apply_mode";

/// 原生配置写入方式：默认保持预览确认，`Direct` 在预览无冲突时跳过确认对话框。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMode {
    PreviewConfirm,
    Direct,
}

impl ApplyMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreviewConfirm => "preview_confirm",
            Self::Direct => "direct",
        }
    }

    pub fn from_stable_str(value: &str) -> Option<Self> {
        match value {
            "preview_confirm" => Some(Self::PreviewConfirm),
            "direct" => Some(Self::Direct),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDto {
    pub apply_mode: ApplyMode,
}

impl Default for AppSettingsDto {
    fn default() -> Self {
        Self {
            apply_mode: ApplyMode::PreviewConfirm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppSettingsInput {
    pub apply_mode: ApplyMode,
}

pub fn load_app_settings(database: &Database) -> Result<AppSettingsDto, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let stored: Option<String> = database
        .connection()
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [APPLY_MODE_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "read_app_setting"))?;
    let Some(stored) = stored else {
        return Ok(AppSettingsDto::default());
    };
    let apply_mode = ApplyMode::from_stable_str(&stored)
        .ok_or_else(|| AppError::new(ErrorCode::DatabaseError, "应用设置包含未知取值", false))?;
    Ok(AppSettingsDto { apply_mode })
}

pub fn save_app_settings(
    database: &mut Database,
    input: &UpdateAppSettingsInput,
) -> Result<AppSettingsDto, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    database
        .connection_mut()
        .execute(
            "INSERT INTO app_settings(key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![APPLY_MODE_KEY, input.apply_mode.as_str()],
        )
        .map_err(|_| AppError::database(&database_path, "write_app_setting"))?;
    Ok(AppSettingsDto {
        apply_mode: input.apply_mode,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        load_app_settings, save_app_settings, AppSettingsDto, ApplyMode, UpdateAppSettingsInput,
        APPLY_MODE_KEY,
    };
    use crate::{app::AppPaths, db::Database, error::ErrorCode};

    fn open_database() -> (tempfile::TempDir, Database) {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        (temporary, Database::open(&paths).unwrap())
    }

    #[test]
    fn missing_setting_falls_back_to_preview_confirm() {
        let (_temporary, database) = open_database();
        assert_eq!(
            load_app_settings(&database).unwrap(),
            AppSettingsDto {
                apply_mode: ApplyMode::PreviewConfirm
            }
        );
    }

    #[test]
    fn saved_apply_mode_round_trips_in_both_directions() {
        let (_temporary, mut database) = open_database();
        let direct = save_app_settings(
            &mut database,
            &UpdateAppSettingsInput {
                apply_mode: ApplyMode::Direct,
            },
        )
        .unwrap();
        assert_eq!(direct.apply_mode, ApplyMode::Direct);
        assert_eq!(load_app_settings(&database).unwrap(), direct);

        let preview = save_app_settings(
            &mut database,
            &UpdateAppSettingsInput {
                apply_mode: ApplyMode::PreviewConfirm,
            },
        )
        .unwrap();
        assert_eq!(preview.apply_mode, ApplyMode::PreviewConfirm);
        assert_eq!(load_app_settings(&database).unwrap(), preview);
    }

    #[test]
    fn saved_apply_mode_survives_reopen() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        {
            let mut database = Database::open(&paths).unwrap();
            save_app_settings(
                &mut database,
                &UpdateAppSettingsInput {
                    apply_mode: ApplyMode::Direct,
                },
            )
            .unwrap();
        }

        let reopened = Database::open(&paths).unwrap();
        assert_eq!(
            load_app_settings(&reopened).unwrap().apply_mode,
            ApplyMode::Direct
        );
    }

    #[test]
    fn unknown_stored_value_is_a_database_error() {
        let (_temporary, mut database) = open_database();
        database
            .connection_mut()
            .execute(
                "INSERT INTO app_settings(key, value) VALUES (?1, 'bogus')",
                [APPLY_MODE_KEY],
            )
            .unwrap();
        let error = load_app_settings(&database).unwrap_err();
        assert_eq!(error.code(), ErrorCode::DatabaseError);
    }
}
