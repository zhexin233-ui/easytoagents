//! Hooks 中央意图、分配与同步的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    commands::{with_db, with_db_and_redactor},
    error::AppError,
    hooks::{self, *},
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command(async)]
#[specta::specta]
pub fn list_hooks(state: State<'_, AppState>) -> Result<Vec<HookDto>, AppError> {
    with_db(&state, |database| hooks::list_hooks(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_hook(state: State<'_, AppState>, id: String) -> Result<HookDto, AppError> {
    with_db(&state, |database| hooks::get_hook(database, &id))
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_hook(
    state: State<'_, AppState>,
    input: CreateHookInput,
) -> Result<HookDto, AppError> {
    with_db(&state, |database| {
        hooks::create_hook(database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_hook(
    state: State<'_, AppState>,
    input: UpdateHookInput,
) -> Result<HookDto, AppError> {
    with_db(&state, |database| hooks::update_hook(database, &input))
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_hook_enabled(
    state: State<'_, AppState>,
    input: VersionedHookInput,
    enabled: bool,
) -> Result<HookDto, AppError> {
    with_db(&state, |database| {
        hooks::set_hook_enabled(database, &input, enabled)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_hook(
    state: State<'_, AppState>,
    input: VersionedHookInput,
) -> Result<DeleteHookResultDto, AppError> {
    with_db(&state, |database| {
        hooks::delete_hook(database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_global_hook_assignment(
    state: State<'_, AppState>,
    input: SetGlobalHookAssignmentInput,
) -> Result<HookDto, AppError> {
    with_db(&state, |database| {
        hooks::set_global_hook_assignment(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_project_hook_assignment(
    state: State<'_, AppState>,
    input: SetProjectHookAssignmentInput,
) -> Result<HookDto, AppError> {
    with_db(&state, |database| {
        hooks::set_project_hook_assignment(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_hook_projects(state: State<'_, AppState>) -> Result<Vec<HookProjectDto>, AppError> {
    with_db(&state, |database| hooks::list_hook_projects(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_hook_project_options(
    state: State<'_, AppState>,
    input: HookProjectOptionsInput,
) -> Result<Vec<HookProjectOptionDto>, AppError> {
    with_db(&state, |database| {
        hooks::list_hook_project_options(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_global_hook_target_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<HookTargetStatusDto>, AppError> {
    with_db(&state, |database| {
        hooks::list_global_hook_target_statuses(database, &*state.environment()?)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_hook_sync(
    state: State<'_, AppState>,
    input: PreviewHookSyncInput,
) -> Result<PreviewPlan, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        hooks::preview_hook_sync(database, &*state.environment()?, redactor, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn apply_hook_preview(
    state: State<'_, AppState>,
    input: ApplyHookPreviewInput,
) -> Result<ApplyResult, AppError> {
    with_db(&state, |database| {
        hooks::apply_hook_preview(
            state.write_operations(),
            database,
            state.paths(),
            &*state.environment()?,
            &input,
        )
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn readopt_hook_target(
    state: State<'_, AppState>,
    input: ReadoptHookTargetInput,
) -> Result<ReadoptHookTargetResultDto, AppError> {
    with_db(&state, |database| {
        // 与 apply 互斥：接管期间不允许在途 apply 同时改写基线。
        let _write_guard = state.lock_write_operations();
        hooks::readopt_hook_target(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn discover_hook_import(
    state: State<'_, AppState>,
    input: DiscoverHookImportInput,
) -> Result<HookImportPreviewDto, AppError> {
    with_db(&state, |database| {
        hooks::discover_hook_import(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn confirm_hook_import(
    state: State<'_, AppState>,
    input: ConfirmHookImportInput,
) -> Result<HookImportResultDto, AppError> {
    with_db(&state, |database| {
        hooks::confirm_hook_import(database, state.paths(), &*state.environment()?, &input)
    })
}
