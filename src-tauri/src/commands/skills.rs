//! Skills 中央库、分配和同步的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    error::{AppError, ErrorCode},
    skills::{self, *},
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command]
#[specta::specta]
pub fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::list_skills(&database, state.paths())
}

#[tauri::command]
#[specta::specta]
pub fn get_skill(state: State<'_, AppState>, id: String) -> Result<SkillDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::get_skill(&database, state.paths(), &id)
}

#[tauri::command]
#[specta::specta]
pub fn import_skill(
    state: State<'_, AppState>,
    input: ImportSkillInput,
) -> Result<SkillDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::import_skill(&mut database, state.paths(), &input)
}

#[tauri::command]
#[specta::specta]
pub fn preview_skill_content(
    state: State<'_, AppState>,
    id: String,
) -> Result<SkillContentPreviewDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::preview_skill_content(&database, state.paths(), &id)
}

#[tauri::command]
#[specta::specta]
pub fn delete_skill(
    state: State<'_, AppState>,
    input: VersionedSkillInput,
) -> Result<DeleteSkillResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::delete_skill(&mut database, state.paths(), &input)
}

#[tauri::command]
#[specta::specta]
pub fn set_global_skill_assignment(
    state: State<'_, AppState>,
    input: SetGlobalSkillAssignmentInput,
) -> Result<SkillDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::set_global_skill_assignment(&mut database, state.paths(), &input)
}

#[tauri::command]
#[specta::specta]
pub fn set_project_skill_assignment(
    state: State<'_, AppState>,
    input: SetProjectSkillAssignmentInput,
) -> Result<SkillDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::set_project_skill_assignment(&mut database, state.paths(), &input)
}

#[tauri::command]
#[specta::specta]
pub fn list_skill_projects(state: State<'_, AppState>) -> Result<Vec<SkillProjectDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::list_skill_projects(&database)
}

#[tauri::command]
#[specta::specta]
pub fn list_skill_project_options(
    state: State<'_, AppState>,
    input: SkillProjectOptionsInput,
) -> Result<Vec<SkillProjectOptionDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::list_skill_project_options(&database, state.paths(), &input)
}

#[tauri::command]
#[specta::specta]
pub fn list_global_skill_target_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<SkillTargetStatusDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    skills::list_global_skill_target_statuses(&database, state.paths(), state.environment()?)
}

#[tauri::command]
#[specta::specta]
pub fn preview_skill_sync(
    state: State<'_, AppState>,
    input: PreviewSkillSyncInput,
) -> Result<PreviewPlan, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    skills::preview_skill_sync(
        &mut database,
        state.paths(),
        state.environment()?,
        &redactor,
        &input,
    )
}

#[tauri::command]
#[specta::specta]
pub fn apply_skill_preview(
    state: State<'_, AppState>,
    input: ApplySkillPreviewInput,
) -> Result<ApplyResult, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    skills::apply_skill_preview(
        state.write_operations(),
        &mut database,
        state.paths(),
        state.environment()?,
        &redactor,
        &input,
    )
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
