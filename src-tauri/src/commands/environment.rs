//! 工具环境探测状态与显式刷新。启动时探测在后台线程池进行，主窗口不等待。

use tauri::State;

use crate::{
    app::{AppState, EnvironmentStateDto},
    error::AppError,
};

/// 探测完成（含刷新）后向前端广播的 Tauri 事件；payload 为是否成功。
pub const ENVIRONMENT_READY_EVENT: &str = "environment-ready";

#[tauri::command(async)]
#[specta::specta]
pub fn get_environment_state(state: State<'_, AppState>) -> Result<EnvironmentStateDto, AppError> {
    state.environment_state()
}

/// 重新探测五个工具并替换环境快照。探测是同步子进程调用；`command(async)`
/// 让它在线程池执行，完成后 `AppState` 的通知回调会广播 `environment-ready`。
#[tauri::command(async)]
#[specta::specta]
pub fn refresh_environment(state: State<'_, AppState>) -> Result<EnvironmentStateDto, AppError> {
    state.probe_environment()?;
    state.environment_state()
}
