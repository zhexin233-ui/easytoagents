//! MCP 中央意图、分配与同步的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    error::{AppError, ErrorCode},
    mcp::{self, *},
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command]
#[specta::specta]
pub fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    mcp::list_mcp_servers(&database, &redactor)
}

#[tauri::command]
#[specta::specta]
pub fn get_mcp_server(state: State<'_, AppState>, id: String) -> Result<McpServerDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    mcp::get_mcp_server(&database, &redactor, &id)
}

#[tauri::command]
#[specta::specta]
pub fn create_mcp_server(
    state: State<'_, AppState>,
    input: McpServerInput,
) -> Result<McpServerDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    mcp::create_mcp_server(&mut database, &mut redactor, &input)
}

#[tauri::command]
#[specta::specta]
pub fn update_mcp_server(
    state: State<'_, AppState>,
    input: UpdateMcpServerInput,
) -> Result<McpServerDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    mcp::update_mcp_server(&mut database, &mut redactor, &input)
}

#[tauri::command]
#[specta::specta]
pub fn set_mcp_enabled(
    state: State<'_, AppState>,
    input: VersionedMcpInput,
    enabled: bool,
) -> Result<McpServerDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    mcp::set_mcp_enabled(&mut database, &redactor, &input, enabled)
}

#[tauri::command]
#[specta::specta]
pub fn delete_mcp_server(
    state: State<'_, AppState>,
    input: VersionedMcpInput,
) -> Result<DeleteMcpResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    mcp::delete_mcp_server(&mut database, &input)
}

#[tauri::command]
#[specta::specta]
pub fn set_global_mcp_assignment(
    state: State<'_, AppState>,
    input: SetGlobalMcpAssignmentInput,
) -> Result<McpServerDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    mcp::set_global_mcp_assignment(&mut database, &redactor, &input)
}

#[tauri::command]
#[specta::specta]
pub fn set_project_mcp_assignment(
    state: State<'_, AppState>,
    input: SetProjectMcpAssignmentInput,
) -> Result<McpServerDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let redactor = state.redactor().read().map_err(|_| state_lock_error())?;
    mcp::set_project_mcp_assignment(&mut database, &redactor, &input)
}

#[tauri::command]
#[specta::specta]
pub fn list_mcp_projects(state: State<'_, AppState>) -> Result<Vec<McpProjectDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    mcp::list_mcp_projects(&database)
}

#[tauri::command]
#[specta::specta]
pub fn list_mcp_project_options(
    state: State<'_, AppState>,
    input: McpProjectOptionsInput,
) -> Result<Vec<McpProjectOptionDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    mcp::list_mcp_project_options(&database, &input)
}

#[tauri::command]
#[specta::specta]
pub fn list_global_mcp_target_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<McpTargetStatusDto>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    mcp::list_global_mcp_target_statuses(&database, state.environment()?)
}

#[tauri::command]
#[specta::specta]
pub fn preview_mcp_sync(
    state: State<'_, AppState>,
    input: PreviewMcpSyncInput,
) -> Result<PreviewPlan, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    mcp::preview_mcp_sync(&mut database, state.environment()?, &mut redactor, &input)
}

#[tauri::command]
#[specta::specta]
pub fn apply_mcp_preview(
    state: State<'_, AppState>,
    input: ApplyMcpPreviewInput,
) -> Result<ApplyResult, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let mut redactor = state.redactor().write().map_err(|_| state_lock_error())?;
    mcp::apply_mcp_preview(
        state.write_operations(),
        &mut database,
        state.paths(),
        state.environment()?,
        &mut redactor,
        &input,
    )
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
