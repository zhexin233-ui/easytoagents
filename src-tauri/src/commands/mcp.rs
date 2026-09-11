//! MCP 中央意图、分配与同步的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    commands::{with_db, with_db_and_redactor},
    error::AppError,
    mcp::{self, *},
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command(async)]
#[specta::specta]
pub fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerDto>, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::list_mcp_servers(database, redactor)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_mcp_server(state: State<'_, AppState>, id: String) -> Result<McpServerDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::get_mcp_server(database, redactor, &id)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_mcp_server(
    state: State<'_, AppState>,
    input: McpServerInput,
) -> Result<McpServerDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::create_mcp_server(database, redactor, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn update_mcp_server(
    state: State<'_, AppState>,
    input: UpdateMcpServerInput,
) -> Result<McpServerDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::update_mcp_server(database, redactor, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_mcp_enabled(
    state: State<'_, AppState>,
    input: VersionedMcpInput,
    enabled: bool,
) -> Result<McpServerDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::set_mcp_enabled(database, redactor, &input, enabled)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_mcp_server(
    state: State<'_, AppState>,
    input: VersionedMcpInput,
) -> Result<DeleteMcpResultDto, AppError> {
    with_db(&state, |database| mcp::delete_mcp_server(database, &input))
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_global_mcp_assignment(
    state: State<'_, AppState>,
    input: SetGlobalMcpAssignmentInput,
) -> Result<McpServerDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::set_global_mcp_assignment(database, redactor, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_project_mcp_assignment(
    state: State<'_, AppState>,
    input: SetProjectMcpAssignmentInput,
) -> Result<McpServerDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::set_project_mcp_assignment(database, redactor, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_mcp_projects(state: State<'_, AppState>) -> Result<Vec<McpProjectDto>, AppError> {
    with_db(&state, |database| mcp::list_mcp_projects(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_mcp_project_options(
    state: State<'_, AppState>,
    input: McpProjectOptionsInput,
) -> Result<Vec<McpProjectOptionDto>, AppError> {
    with_db(&state, |database| {
        mcp::list_mcp_project_options(database, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_global_mcp_target_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<McpTargetStatusDto>, AppError> {
    with_db(&state, |database| {
        mcp::list_global_mcp_target_statuses(database, &*state.environment()?)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_mcp_sync(
    state: State<'_, AppState>,
    input: PreviewMcpSyncInput,
) -> Result<PreviewPlan, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::preview_mcp_sync(database, &*state.environment()?, redactor, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn apply_mcp_preview(
    state: State<'_, AppState>,
    input: ApplyMcpPreviewInput,
) -> Result<ApplyResult, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::apply_mcp_preview(
            state.write_operations(),
            database,
            state.paths(),
            &*state.environment()?,
            redactor,
            &input,
        )
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn readopt_mcp_target(
    state: State<'_, AppState>,
    input: ReadoptMcpTargetInput,
) -> Result<ReadoptMcpTargetResultDto, AppError> {
    with_db(&state, |database| {
        // 与 apply 互斥：接管期间不允许在途 apply 同时改写基线。
        let _write_guard = state.lock_write_operations();
        mcp::readopt_mcp_target(database, &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn discover_mcp_import(
    state: State<'_, AppState>,
    tool: crate::domain::Tool,
) -> Result<McpImportPreviewDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::discover_mcp_import(database, &*state.environment()?, redactor, tool)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn confirm_mcp_import(
    state: State<'_, AppState>,
    input: ConfirmMcpImportInput,
) -> Result<McpImportResultDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        mcp::confirm_mcp_import(database, &*state.environment()?, redactor, &input)
    })
}
