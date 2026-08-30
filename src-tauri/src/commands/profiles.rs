//! Provider/Prompt 的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    domain::Tool,
    error::{AppError, ErrorCode},
    profiles::{
        self, ApplyProfilePreviewInput, ConfirmImportInput, CopyProviderProfileInput,
        DeleteProfileResultDto, PromptImportPreviewDto, PromptProfileDto, PromptProfileInput,
        PromptProjectAssignmentDto, ProviderImportPreviewDto, ProviderProfileDto,
        ProviderProfileInput, SetGlobalPromptAssignmentInput, SetPromptProjectAssignmentInput,
        ToolProfileStatusDto, UpdatePromptProfileInput, UpdateProviderProfileInput,
        VersionedProfileInput,
    },
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command]
#[specta::specta]
pub fn list_provider_profiles(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<Vec<ProviderProfileDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::list_provider_profiles(&database, tool)
}

#[tauri::command]
#[specta::specta]
pub fn create_provider_profile(
    state: State<'_, AppState>,
    input: ProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    profiles::create_provider_profile(&mut database, &mut redactor, input)
}

#[tauri::command]
#[specta::specta]
pub fn update_provider_profile(
    state: State<'_, AppState>,
    input: UpdateProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    profiles::update_provider_profile(&mut database, &mut redactor, input)
}

#[tauri::command]
#[specta::specta]
pub fn copy_provider_profile(
    state: State<'_, AppState>,
    input: CopyProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    profiles::copy_provider_profile(&mut database, &mut redactor, input)
}

#[tauri::command]
#[specta::specta]
pub fn set_active_provider_profile(
    state: State<'_, AppState>,
    tool: Tool,
    input: VersionedProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::set_active_provider_profile(&mut database, tool, &input)
}

#[tauri::command]
#[specta::specta]
pub fn delete_provider_profile(
    state: State<'_, AppState>,
    input: VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::delete_provider_profile(&mut database, &input)
}

#[tauri::command]
#[specta::specta]
pub fn list_prompt_profiles(state: State<'_, AppState>) -> Result<Vec<PromptProfileDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::list_prompt_profiles(&database)
}

#[tauri::command]
#[specta::specta]
pub fn create_prompt_profile(
    state: State<'_, AppState>,
    input: PromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::create_prompt_profile(&mut database, input)
}

#[tauri::command]
#[specta::specta]
pub fn update_prompt_profile(
    state: State<'_, AppState>,
    input: UpdatePromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::update_prompt_profile(&mut database, input)
}

#[tauri::command]
#[specta::specta]
pub fn set_global_prompt_assignment(
    state: State<'_, AppState>,
    input: SetGlobalPromptAssignmentInput,
) -> Result<PromptProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::set_global_prompt_assignment(&mut database, &input)
}

#[tauri::command]
#[specta::specta]
pub fn delete_prompt_profile(
    state: State<'_, AppState>,
    input: VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::delete_prompt_profile(&mut database, &input)
}

#[tauri::command]
#[specta::specta]
pub fn get_tool_profile_status(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<ToolProfileStatusDto, AppError> {
    profiles::get_tool_profile_status(state.environment()?, tool)
}

#[tauri::command]
#[specta::specta]
pub fn discover_provider_import(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<Option<ProviderImportPreviewDto>, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    profiles::discover_provider_import(&mut database, state.environment()?, &redactor, tool)
}

#[tauri::command]
#[specta::specta]
pub fn confirm_provider_import(
    state: State<'_, AppState>,
    input: ConfirmImportInput,
) -> Result<ProviderProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    profiles::confirm_provider_import(&mut database, state.environment()?, &mut redactor, input)
}

#[tauri::command]
#[specta::specta]
pub fn discover_prompt_import(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<Option<PromptImportPreviewDto>, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::discover_prompt_import(&mut database, state.environment()?, tool)
}

#[tauri::command]
#[specta::specta]
pub fn confirm_prompt_import(
    state: State<'_, AppState>,
    input: ConfirmImportInput,
) -> Result<PromptProfileDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::confirm_prompt_import(&mut database, state.environment()?, input)
}

#[tauri::command]
#[specta::specta]
pub fn preview_provider_sync(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<PreviewPlan, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    profiles::preview_provider_sync(&mut database, state.environment()?, &mut redactor, tool)
}

#[tauri::command]
#[specta::specta]
pub fn preview_prompt_sync(
    state: State<'_, AppState>,
    tool: Tool,
    project_id: Option<String>,
) -> Result<PreviewPlan, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    profiles::preview_prompt_sync(
        &mut database,
        state.environment()?,
        &redactor,
        tool,
        project_id,
    )
}

#[tauri::command]
#[specta::specta]
pub fn set_prompt_project_assignment(
    state: State<'_, AppState>,
    input: SetPromptProjectAssignmentInput,
) -> Result<PromptProjectAssignmentDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::set_prompt_project_assignment(&mut database, state.environment()?, &input)
}

#[tauri::command]
#[specta::specta]
pub fn get_prompt_project_assignment(
    state: State<'_, AppState>,
    project_id: String,
    tool: Tool,
) -> Result<PromptProjectAssignmentDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    profiles::get_prompt_project_assignment(&database, &project_id, tool)
}

#[tauri::command]
#[specta::specta]
pub fn apply_profile_preview(
    state: State<'_, AppState>,
    input: ApplyProfilePreviewInput,
) -> Result<ApplyResult, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    profiles::apply_profile_preview(
        state.write_operations(),
        &mut database,
        state.paths(),
        state.environment()?,
        &mut redactor,
        &input.preview_id,
        input.tool,
        input.artifact_kind,
        input.project_id.as_deref(),
    )
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
