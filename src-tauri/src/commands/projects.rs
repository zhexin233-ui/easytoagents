//! 项目登记与只读扫描 RPC。

use tauri::State;

use crate::{
    app::AppState,
    error::{AppError, ErrorCode},
    projects::{self, *},
};

#[tauri::command]
#[specta::specta]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::list_projects(&database, state.environment()?)
}

#[tauri::command]
#[specta::specta]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<ProjectDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::get_project(&database, state.environment()?, &id)
}

#[tauri::command]
#[specta::specta]
pub fn register_project(
    state: State<'_, AppState>,
    input: RegisterProjectInput,
) -> Result<ProjectDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::register_project(&mut database, state.environment()?, &input)
}

#[tauri::command]
#[specta::specta]
pub fn rescan_project(
    state: State<'_, AppState>,
    input: VersionedProjectInput,
) -> Result<ProjectDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::rescan_project(&mut database, state.environment()?, &input)
}

#[tauri::command]
#[specta::specta]
pub fn remove_project(
    state: State<'_, AppState>,
    input: VersionedProjectInput,
) -> Result<RemoveProjectResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::remove_project(&mut database, &input)
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
