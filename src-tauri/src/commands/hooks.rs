//! Hooks 中央意图、分配与同步的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    error::{AppError, ErrorCode},
    hooks::{self, *},
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command(async)]
#[specta::specta]
pub fn list_hooks(state: State<'_, AppState>) -> Result<Vec<HookDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::list_hooks(&database)
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_hook(state: State<'_, AppState>, id: String) -> Result<HookDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::get_hook(&database, &id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_hook(
    state: State<'_, AppState>,
    input: CreateHookInput,
) -> Result<HookDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::create_hook(&mut database, state.paths(), &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_hook(
    state: State<'_, AppState>,
    input: UpdateHookInput,
) -> Result<HookDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::update_hook(&mut database, &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_hook_enabled(
    state: State<'_, AppState>,
    input: VersionedHookInput,
    enabled: bool,
) -> Result<HookDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::set_hook_enabled(&mut database, &input, enabled)
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_hook(
    state: State<'_, AppState>,
    input: VersionedHookInput,
) -> Result<DeleteHookResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::delete_hook(&mut database, state.paths(), &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_global_hook_assignment(
    state: State<'_, AppState>,
    input: SetGlobalHookAssignmentInput,
) -> Result<HookDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::set_global_hook_assignment(&mut database, &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_project_hook_assignment(
    state: State<'_, AppState>,
    input: SetProjectHookAssignmentInput,
) -> Result<HookDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::set_project_hook_assignment(&mut database, &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_hook_projects(state: State<'_, AppState>) -> Result<Vec<HookProjectDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::list_hook_projects(&database)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_hook_project_options(
    state: State<'_, AppState>,
    input: HookProjectOptionsInput,
) -> Result<Vec<HookProjectOptionDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::list_hook_project_options(&database, &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_global_hook_target_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<HookTargetStatusDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::list_global_hook_target_statuses(&database, &*state.environment()?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_hook_sync(
    state: State<'_, AppState>,
    input: PreviewHookSyncInput,
) -> Result<PreviewPlan, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    hooks::preview_hook_sync(&mut database, &*state.environment()?, &mut redactor, &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn apply_hook_preview(
    state: State<'_, AppState>,
    input: ApplyHookPreviewInput,
) -> Result<ApplyResult, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::apply_hook_preview(
        state.write_operations(),
        &mut database,
        state.paths(),
        &*state.environment()?,
        &input,
    )
}

#[tauri::command(async)]
#[specta::specta]
pub fn readopt_hook_target(
    state: State<'_, AppState>,
    input: ReadoptHookTargetInput,
) -> Result<ReadoptHookTargetResultDto, AppError> {
    // 与 apply 互斥：接管期间不允许在途 apply 同时改写基线。
    let _write_guard = state
        .write_operations()
        .lock()
        .map_err(|_| state_lock_error())?;
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::readopt_hook_target(&mut database, &*state.environment()?, &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn discover_hook_import(
    state: State<'_, AppState>,
    input: DiscoverHookImportInput,
) -> Result<HookImportPreviewDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::discover_hook_import(&mut database, &*state.environment()?, &input)
}

#[tauri::command(async)]
#[specta::specta]
pub fn confirm_hook_import(
    state: State<'_, AppState>,
    input: ConfirmHookImportInput,
) -> Result<HookImportResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    hooks::confirm_hook_import(&mut database, state.paths(), &*state.environment()?, &input)
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
