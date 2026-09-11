//! 项目登记与只读扫描 RPC。

use tauri::State;

use crate::{
    app::AppState,
    commands::{with_db, with_db_and_redactor},
    error::AppError,
    projects::{self, *},
};

#[tauri::command(async)]
#[specta::specta]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectDto>, AppError> {
    with_db(&state, |database| {
        projects::list_projects(database, &*state.environment()?)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<ProjectDto, AppError> {
    with_db(&state, |database| {
        projects::get_project(database, &*state.environment()?, &id)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn register_project(
    state: State<'_, AppState>,
    input: RegisterProjectInput,
) -> Result<ProjectDto, AppError> {
    with_db(&state, |database| {
        projects::register_project(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn rename_project(
    state: State<'_, AppState>,
    input: RenameProjectInput,
) -> Result<ProjectDto, AppError> {
    with_db(&state, |database| {
        projects::rename_project(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn rescan_project(
    state: State<'_, AppState>,
    input: VersionedProjectInput,
) -> Result<ProjectDto, AppError> {
    with_db(&state, |database| {
        projects::rescan_project(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn remove_project(
    state: State<'_, AppState>,
    input: VersionedProjectInput,
) -> Result<RemoveProjectResultDto, AppError> {
    with_db(&state, |database| {
        projects::remove_project(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_project_native_resources(
    state: State<'_, AppState>,
    input: ProjectNativeResourceQueryInput,
) -> Result<Vec<ProjectNativeResourceDto>, AppError> {
    with_db(&state, |database| {
        projects::list_project_native_resources(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_project_native_resource_action(
    state: State<'_, AppState>,
    input: PreviewProjectNativeResourceActionInput,
) -> Result<crate::sync::PreviewPlan, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        projects::preview_project_native_resource_action(
            database,
            &*state.environment()?,
            redactor,
            &input,
        )
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn apply_project_native_resource_preview(
    state: State<'_, AppState>,
    input: ApplyProjectNativeResourcePreviewInput,
) -> Result<crate::sync::ApplyResult, AppError> {
    with_db(&state, |database| {
        projects::apply_project_native_resource_preview(
            state.write_operations(),
            database,
            state.paths(),
            &*state.environment()?,
            &input,
        )
    })
}
