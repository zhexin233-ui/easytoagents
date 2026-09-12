//! Agents 中央意图、分配与同步的窄 Tauri RPC。

use tauri::State;

use crate::{
    agents,
    app::AppState,
    commands::{with_db, with_db_and_redactor},
    error::AppError,
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command(async)]
#[specta::specta]
pub fn list_agents(state: State<'_, AppState>) -> Result<Vec<agents::AgentDto>, AppError> {
    with_db(&state, |database| agents::list_agents(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_agent(state: State<'_, AppState>, id: String) -> Result<agents::AgentDto, AppError> {
    with_db(&state, |database| agents::get_agent(database, &id))
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_agent(
    state: State<'_, AppState>,
    input: agents::CreateAgentInput,
) -> Result<agents::AgentDto, AppError> {
    with_db(&state, |database| agents::create_agent(database, &input))
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_agent(
    state: State<'_, AppState>,
    input: agents::UpdateAgentInput,
) -> Result<agents::AgentDto, AppError> {
    with_db(&state, |database| agents::update_agent(database, &input))
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_agent_enabled(
    state: State<'_, AppState>,
    input: agents::VersionedAgentInput,
    enabled: bool,
) -> Result<agents::AgentDto, AppError> {
    with_db(&state, |database| {
        agents::set_agent_enabled(database, &input, enabled)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_agent(
    state: State<'_, AppState>,
    input: agents::VersionedAgentInput,
) -> Result<agents::DeleteAgentResultDto, AppError> {
    with_db(&state, |database| agents::delete_agent(database, &input))
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_global_agent_assignment(
    state: State<'_, AppState>,
    input: agents::SetGlobalAgentAssignmentInput,
) -> Result<agents::AgentDto, AppError> {
    with_db(&state, |database| {
        agents::set_global_agent_assignment(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_project_agent_assignment(
    state: State<'_, AppState>,
    input: agents::SetProjectAgentAssignmentInput,
) -> Result<agents::AgentDto, AppError> {
    with_db(&state, |database| {
        agents::set_project_agent_assignment(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_agent_projects(
    state: State<'_, AppState>,
) -> Result<Vec<agents::AgentProjectDto>, AppError> {
    with_db(&state, |database| agents::list_agent_projects(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_agent_project_options(
    state: State<'_, AppState>,
    input: agents::AgentProjectOptionsInput,
) -> Result<Vec<agents::AgentProjectOptionDto>, AppError> {
    with_db(&state, |database| {
        agents::list_agent_project_options(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_global_agent_target_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<agents::AgentToolTargetStatusDto>, AppError> {
    with_db(&state, |database| {
        agents::list_global_agent_target_statuses(database, &*state.environment()?)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_agent_sync(
    state: State<'_, AppState>,
    input: agents::PreviewAgentSyncInput,
) -> Result<PreviewPlan, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        agents::preview_agent_sync(database, &*state.environment()?, redactor, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn apply_agent_preview(
    state: State<'_, AppState>,
    input: agents::ApplyAgentPreviewInput,
) -> Result<ApplyResult, AppError> {
    with_db(&state, |database| {
        agents::apply_agent_preview(
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
pub fn readopt_agent_target(
    state: State<'_, AppState>,
    input: agents::ReadoptAgentTargetInput,
) -> Result<agents::ReadoptAgentTargetResultDto, AppError> {
    with_db(&state, |database| {
        // 与 apply 互斥：接管期间不允许在途 apply 同时改写基线。
        let _write_guard = state.lock_write_operations();
        agents::readopt_agent_target(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn discover_agent_import(
    state: State<'_, AppState>,
    input: agents::DiscoverAgentImportInput,
) -> Result<agents::AgentImportPreviewDto, AppError> {
    with_db(&state, |database| {
        agents::discover_agent_import(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn confirm_agent_import(
    state: State<'_, AppState>,
    input: agents::ConfirmAgentImportInput,
) -> Result<agents::AgentImportResultDto, AppError> {
    with_db(&state, |database| {
        agents::confirm_agent_import(database, &*state.environment()?, &input)
    })
}
