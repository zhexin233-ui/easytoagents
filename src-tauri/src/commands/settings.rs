//! 应用级全局设置 RPC。

use tauri::State;

use crate::{
    app::AppState,
    error::{AppError, ErrorCode},
    settings::{self, AppSettingsDto, UpdateAppSettingsInput},
};

#[tauri::command]
#[specta::specta]
pub fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    settings::load_app_settings(&database)
}

#[tauri::command]
#[specta::specta]
pub fn update_app_settings(
    state: State<'_, AppState>,
    input: UpdateAppSettingsInput,
) -> Result<AppSettingsDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    settings::save_app_settings(&mut database, &input)
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
