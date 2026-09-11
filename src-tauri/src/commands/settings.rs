//! 应用级全局设置 RPC。

use tauri::State;

use crate::{
    app::AppState,
    commands::with_db,
    error::AppError,
    settings::{self, AppSettingsDto, UpdateAppSettingsInput},
};

#[tauri::command(async)]
#[specta::specta]
pub fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, AppError> {
    with_db(&state, |database| settings::load_app_settings(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_app_settings(
    state: State<'_, AppState>,
    input: UpdateAppSettingsInput,
) -> Result<AppSettingsDto, AppError> {
    with_db(&state, |database| {
        settings::save_app_settings(database, &input)
    })
}
