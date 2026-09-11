//! Provider/Prompt 的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    commands::{with_db, with_db_and_redactor},
    domain::Tool,
    error::AppError,
    profiles::{
        self, ApplyProfilePreviewInput, ConfirmImportInput, CopyProviderProfileInput,
        DeleteProfileResultDto, PromptImportPreviewDto, PromptProfileDto, PromptProfileInput,
        ProviderImportPreviewDto, ProviderProfileDto, ProviderProfileInput,
        SetGlobalPromptAssignmentInput, ToolProfileStatusDto, UpdatePromptProfileInput,
        UpdateProviderProfileInput, VersionedProfileInput,
    },
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command(async)]
#[specta::specta]
pub fn list_provider_profiles(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<Vec<ProviderProfileDto>, AppError> {
    with_db(&state, |database| {
        profiles::list_provider_profiles(database, tool)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_provider_profile(
    state: State<'_, AppState>,
    input: ProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::create_provider_profile(database, redactor, input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_provider_profile(
    state: State<'_, AppState>,
    input: UpdateProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::update_provider_profile(database, redactor, input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn copy_provider_profile(
    state: State<'_, AppState>,
    input: CopyProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::copy_provider_profile(database, redactor, input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_active_provider_profile(
    state: State<'_, AppState>,
    tool: Tool,
    input: VersionedProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    with_db(&state, |database| {
        profiles::set_active_provider_profile(database, tool, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_provider_profile(
    state: State<'_, AppState>,
    input: VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    with_db(&state, |database| {
        profiles::delete_provider_profile(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_prompt_profiles(state: State<'_, AppState>) -> Result<Vec<PromptProfileDto>, AppError> {
    with_db(&state, |database| profiles::list_prompt_profiles(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_prompt_profile(
    state: State<'_, AppState>,
    input: PromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    with_db(&state, |database| {
        profiles::create_prompt_profile(database, input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_prompt_profile(
    state: State<'_, AppState>,
    input: UpdatePromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    with_db(&state, |database| {
        profiles::update_prompt_profile(database, input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_global_prompt_assignment(
    state: State<'_, AppState>,
    input: SetGlobalPromptAssignmentInput,
) -> Result<PromptProfileDto, AppError> {
    with_db(&state, |database| {
        profiles::set_global_prompt_assignment(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_prompt_profile(
    state: State<'_, AppState>,
    input: VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    with_db(&state, |database| {
        profiles::delete_prompt_profile(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_tool_profile_status(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<ToolProfileStatusDto, AppError> {
    profiles::get_tool_profile_status(&*state.environment()?, tool)
}

#[tauri::command(async)]
#[specta::specta]
pub fn discover_provider_import(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<Option<ProviderImportPreviewDto>, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::discover_provider_import(database, &*state.environment()?, redactor, tool)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn confirm_provider_import(
    state: State<'_, AppState>,
    input: ConfirmImportInput,
) -> Result<ProviderProfileDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::confirm_provider_import(database, &*state.environment()?, redactor, input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn discover_prompt_import(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<Option<PromptImportPreviewDto>, AppError> {
    with_db(&state, |database| {
        profiles::discover_prompt_import(database, &*state.environment()?, tool)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn confirm_prompt_import(
    state: State<'_, AppState>,
    input: ConfirmImportInput,
) -> Result<PromptProfileDto, AppError> {
    with_db(&state, |database| {
        profiles::confirm_prompt_import(database, &*state.environment()?, input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_provider_sync(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<PreviewPlan, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::preview_provider_sync(database, &*state.environment()?, redactor, tool)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_prompt_sync(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<PreviewPlan, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::preview_prompt_sync(database, &*state.environment()?, redactor, tool)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn apply_profile_preview(
    state: State<'_, AppState>,
    input: ApplyProfilePreviewInput,
) -> Result<ApplyResult, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        profiles::apply_profile_preview(
            state.write_operations(),
            database,
            state.paths(),
            &*state.environment()?,
            redactor,
            &input.preview_id,
            input.tool,
            input.artifact_kind,
        )
    })
}
