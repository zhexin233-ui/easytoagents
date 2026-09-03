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
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::list_projects(&mut database, state.environment()?)
}

#[tauri::command]
#[specta::specta]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<ProjectDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::get_project(&mut database, state.environment()?, &id)
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

#[tauri::command]
#[specta::specta]
pub fn list_project_native_resources(
    state: State<'_, AppState>,
    input: ProjectNativeResourceQueryInput,
) -> Result<Vec<ProjectNativeResourceDto>, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::list_project_native_resources(&mut database, state.environment()?, &input)
}

#[tauri::command]
#[specta::specta]
pub fn preview_project_native_resource_action(
    state: State<'_, AppState>,
    input: PreviewProjectNativeResourceActionInput,
) -> Result<crate::sync::PreviewPlan, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    projects::preview_project_native_resource_action(
        &mut database,
        state.environment()?,
        &mut redactor,
        &input,
    )
}

#[tauri::command]
#[specta::specta]
pub fn apply_project_native_resource_preview(
    state: State<'_, AppState>,
    input: ApplyProjectNativeResourcePreviewInput,
) -> Result<crate::sync::ApplyResult, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    projects::apply_project_native_resource_preview(
        state.write_operations(),
        &mut database,
        state.paths(),
        state.environment()?,
        &input,
    )
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
