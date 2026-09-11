//! Skills 中央库、分配和同步的窄 Tauri RPC。

use tauri::State;

use crate::{
    app::AppState,
    commands::{with_database_handle, with_db, with_db_and_redactor},
    domain::Tool,
    error::{AppError, ErrorCode},
    skills::{self, *},
    sync::{ApplyResult, PreviewPlan},
};

#[tauri::command(async)]
#[specta::specta]
pub fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillDto>, AppError> {
    with_db(&state, |database| {
        skills::list_skills(database, state.paths())
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_skill(state: State<'_, AppState>, id: String) -> Result<SkillDto, AppError> {
    with_db(&state, |database| {
        skills::get_skill(database, state.paths(), &id)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn import_skill(
    state: State<'_, AppState>,
    input: ImportSkillInput,
) -> Result<SkillDto, AppError> {
    with_db(&state, |database| {
        skills::import_skill(database, state.paths(), &input)
    })
}

#[tauri::command]
#[specta::specta]
pub async fn import_github_skill(
    state: State<'_, AppState>,
    input: ImportGithubSkillInput,
) -> Result<SkillDto, AppError> {
    // 代理配置在 setup 里读取一次并注入，这里不再读进程环境。
    // 先拷贝成 owned 值，避免 async 命令的 future 跨 await 借用 `State`。
    let proxy = state.github_proxy().map(str::to_owned);
    let downloaded = skills::download_github_skill(&input.url, proxy.as_deref()).await?;
    // 下载后的文件校验、中央库复制与数据库写入都是同步阻塞操作，
    // 不能留在 tokio worker 上；把数据库句柄与路径移进阻塞线程池。
    let database = state.database_handle();
    let paths = state.paths().clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_database_handle(database, |database| {
            skills::import_downloaded_github_skill(
                database,
                &paths,
                downloaded.path(),
                downloaded.normalized_url(),
            )
        })
    })
    .await
    .map_err(|error| {
        AppError::new(ErrorCode::WriteInProgress, "后台导入任务已中止", true).with_source(error)
    })?
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_skill_content(
    state: State<'_, AppState>,
    id: String,
) -> Result<SkillContentPreviewDto, AppError> {
    with_db(&state, |database| {
        skills::preview_skill_content(database, state.paths(), &id)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn adopt_skill_content(
    state: State<'_, AppState>,
    input: VersionedSkillInput,
) -> Result<SkillDto, AppError> {
    with_db(&state, |database| {
        skills::adopt_skill_content(database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_skill(
    state: State<'_, AppState>,
    input: VersionedSkillInput,
) -> Result<DeleteSkillResultDto, AppError> {
    with_db(&state, |database| {
        skills::delete_skill(database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_global_skill_assignment(
    state: State<'_, AppState>,
    input: SetGlobalSkillAssignmentInput,
) -> Result<SkillDto, AppError> {
    with_db(&state, |database| {
        skills::set_global_skill_assignment(database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_project_skill_assignment(
    state: State<'_, AppState>,
    input: SetProjectSkillAssignmentInput,
) -> Result<SkillDto, AppError> {
    with_db(&state, |database| {
        skills::set_project_skill_assignment(database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_skill_projects(state: State<'_, AppState>) -> Result<Vec<SkillProjectDto>, AppError> {
    with_db(&state, |database| skills::list_skill_projects(database))
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_skill_project_options(
    state: State<'_, AppState>,
    input: SkillProjectOptionsInput,
) -> Result<Vec<SkillProjectOptionDto>, AppError> {
    with_db(&state, |database| {
        skills::list_skill_project_options(database, state.paths(), &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_global_skill_target_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<SkillTargetStatusDto>, AppError> {
    with_db(&state, |database| {
        skills::list_global_skill_target_statuses(database, state.paths(), &*state.environment()?)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn preview_skill_sync(
    state: State<'_, AppState>,
    input: PreviewSkillSyncInput,
) -> Result<PreviewPlan, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        skills::preview_skill_sync(
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
pub fn apply_skill_preview(
    state: State<'_, AppState>,
    input: ApplySkillPreviewInput,
) -> Result<ApplyResult, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        skills::apply_skill_preview(
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
pub fn discover_skill_import(
    state: State<'_, AppState>,
    tool: Tool,
) -> Result<SkillImportPreviewDto, AppError> {
    with_db(&state, |database| {
        skills::discover_skill_import(database, state.paths(), &*state.environment()?, tool)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn confirm_skill_import(
    state: State<'_, AppState>,
    input: ConfirmSkillImportInput,
) -> Result<SkillImportResultDto, AppError> {
    with_db(&state, |database| {
        skills::confirm_skill_import(database, state.paths(), &*state.environment()?, &input)
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn prepare_skill_takeover(
    state: State<'_, AppState>,
    input: PrepareSkillTakeoverInput,
) -> Result<SkillTakeoverPreviewResultDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        skills::prepare_skill_takeover(
            database,
            state.paths(),
            &*state.environment()?,
            redactor,
            &input,
        )
    })
}
