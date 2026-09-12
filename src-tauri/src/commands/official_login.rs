//! 官方账号登录的窄 Tauri RPC：状态探测、启动与取消都委托官方 CLI 子进程。

use tauri::State;

use crate::{app::AppState, domain::Tool, error::AppError, official_login::OfficialLoginStatusDto};

/// 探测会同步启动一个几秒内结束的 CLI 子进程；`command(async)` 让它离开主线程。
#[tauri::command(async)]
#[specta::specta]
pub fn get_official_login_status(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<OfficialLoginStatusDto, AppError> {
    let context = state.official_login_context()?;
    let redactor = state.redactor_read();
    state.official_logins().status(&context, &redactor, tool)
}

#[tauri::command(async)]
#[specta::specta]
pub fn start_official_login(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<OfficialLoginStatusDto, AppError> {
    let context = state.official_login_context()?;
    state.official_logins().start(&context, tool)?;
    let redactor = state.redactor_read();
    state.official_logins().status(&context, &redactor, tool)
}

#[tauri::command(async)]
#[specta::specta]
pub fn cancel_official_login(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<OfficialLoginStatusDto, AppError> {
    let context = state.official_login_context()?;
    state.official_logins().cancel(tool)?;
    let redactor = state.redactor_read();
    state.official_logins().status(&context, &redactor, tool)
}
