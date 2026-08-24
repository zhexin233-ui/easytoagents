//! 总览、快照与恢复 RPC。恢复仍由持久化预览和 Phase 3 引擎执行。

use tauri::State;

use crate::{
    app::AppState,
    error::{AppError, ErrorCode},
    overview::{self, *},
    sync::{self, ApplyResult, InterruptedRunPlan, RestorePreview, SnapshotSummary},
};

#[tauri::command]
#[specta::specta]
pub fn get_dashboard_summary(state: State<'_, AppState>) -> Result<DashboardSummaryDto, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    overview::dashboard_summary(&database, state.paths())
}

#[tauri::command]
#[specta::specta]
pub fn complete_onboarding(
    state: State<'_, AppState>,
) -> Result<CompleteOnboardingResultDto, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    overview::complete_onboarding(&mut database)
}

#[tauri::command]
#[specta::specta]
pub fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotSummary>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    sync::list_snapshots(&database)
}

#[tauri::command]
#[specta::specta]
pub fn get_interrupted_run(
    state: State<'_, AppState>,
) -> Result<Option<InterruptedRunPlan>, AppError> {
    let database = state.database().lock().map_err(|_| state_lock_error())?;
    let interrupted = sync::detect_interrupted_run(&database, state.paths())?;
    let mut cached = state
        .interrupted_run()
        .write()
        .map_err(|_| state_lock_error())?;
    *cached = interrupted.clone();
    Ok(interrupted)
}

#[tauri::command]
#[specta::specta]
pub fn preview_snapshot_restore(
    state: State<'_, AppState>,
    input: SnapshotRestoreInput,
) -> Result<RestorePreview, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let context =
        overview::snapshot_restore_context(&database, state.environment()?, &input.snapshot_id)?;
    sync::preview_restore(
        &mut database,
        state.paths(),
        &input.snapshot_id,
        &context.allowed_root,
    )
}

#[tauri::command]
#[specta::specta]
pub fn restore_snapshot(
    state: State<'_, AppState>,
    input: ApplySnapshotRestoreInput,
) -> Result<ApplyResult, AppError> {
    let mut database = state.database().lock().map_err(|_| state_lock_error())?;
    let context =
        overview::snapshot_restore_context(&database, state.environment()?, &input.snapshot_id)?;
    let result = sync::restore_snapshot(
        state.write_operations(),
        &mut database,
        state.paths(),
        &input.preview_id,
        &context.allowed_root,
        Some(state.paths().central_skills()),
    )?;
    let interrupted = sync::detect_interrupted_run(&database, state.paths())?;
    *state
        .interrupted_run()
        .write()
        .map_err(|_| state_lock_error())? = interrupted;
    Ok(result)
}

fn state_lock_error() -> AppError {
    AppError::new(ErrorCode::WriteInProgress, "应用状态锁不可用", false)
}
