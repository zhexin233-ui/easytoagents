//! 总览、快照与恢复 RPC。恢复仍由持久化预览和 Phase 3 引擎执行。

use tauri::State;

use crate::{
    app::AppState,
    commands::with_db,
    error::AppError,
    overview::{self, *},
    sync::{
        self, ApplyResult, DeleteSnapshotsInput, DeleteSnapshotsResultDto, InterruptedRunPlan,
        RestorePreview, SnapshotSummary,
    },
};

#[tauri::command(async)]
#[specta::specta]
pub fn get_dashboard_summary(state: State<'_, AppState>) -> Result<DashboardSummaryDto, AppError> {
    with_db(&state, |database| {
        overview::dashboard_summary(database, state.paths())
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn complete_onboarding(
    state: State<'_, AppState>,
) -> Result<CompleteOnboardingResultDto, AppError> {
    with_db(&state, overview::complete_onboarding)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotSummary>, AppError> {
    with_db(&state, |database| sync::list_snapshots(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_snapshots(
    state: State<'_, AppState>,
    input: DeleteSnapshotsInput,
) -> Result<DeleteSnapshotsResultDto, AppError> {
    with_db(&state, |database| {
        sync::delete_snapshots(state.write_operations(), database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_interrupted_run(
    state: State<'_, AppState>,
) -> Result<Option<InterruptedRunPlan>, AppError> {
    with_db(&state, |database| {
        sync::detect_interrupted_run(database, state.paths())
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_snapshot_restore(
    state: State<'_, AppState>,
    input: SnapshotRestoreInput,
) -> Result<RestorePreview, AppError> {
    with_db(&state, |database| {
        let context = overview::snapshot_restore_context(
            database,
            &*state.environment()?,
            &input.snapshot_id,
        )?;
        sync::preview_restore(
            database,
            state.paths(),
            &input.snapshot_id,
            &context.allowed_root,
        )
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn restore_snapshot(
    state: State<'_, AppState>,
    input: ApplySnapshotRestoreInput,
) -> Result<ApplyResult, AppError> {
    with_db(&state, |database| {
        let context = overview::snapshot_restore_context(
            database,
            &*state.environment()?,
            &input.snapshot_id,
        )?;
        sync::restore_snapshot(
            state.write_operations(),
            database,
            state.paths(),
            &input.preview_id,
            &context.allowed_root,
            Some(state.paths().central_skills()),
        )
    })
}
