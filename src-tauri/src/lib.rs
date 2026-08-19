use std::path::Path;

use specta_typescript::Typescript;
use tauri::Manager;
use tauri_specta::{collect_commands, Builder};

pub mod adapters;
pub mod app;
pub mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod git;
pub mod security;
pub mod skills;
pub mod sync;

pub fn create_command_builder<R: tauri::Runtime>() -> Builder<R> {
    Builder::<R>::new()
        .typ::<error::AppError>()
        .typ::<domain::Tool>()
        .typ::<domain::Scope>()
        .typ::<domain::ArtifactKind>()
        .typ::<domain::SyncStatus>()
        .typ::<domain::ChangeKind>()
        .typ::<domain::SyncRunKind>()
        .typ::<domain::SyncRunStatus>()
        .typ::<domain::McpTransport>()
        .typ::<domain::TrustStatus>()
        .typ::<domain::SkillStatus>()
        .typ::<domain::TargetType>()
        .typ::<adapters::TargetFormat>()
        .typ::<adapters::CapabilityState>()
        .typ::<adapters::TargetCapability>()
        .typ::<adapters::PolicyState>()
        .typ::<adapters::TargetTrustState>()
        .typ::<adapters::PromptOverrideState>()
        .typ::<adapters::SymlinkPolicy>()
        .typ::<adapters::TargetDescriptor>()
        .typ::<git::GitPathStatus>()
        .typ::<sync::DatabaseEntityType>()
        .typ::<sync::DatabaseRowVersion>()
        .typ::<sync::PreviewTargetPlan>()
        .typ::<sync::PreviewPlan>()
        .commands(collect_commands![commands::get_app_info])
}

pub fn export_typescript_bindings(path: &Path) {
    create_command_builder::<tauri::Wry>()
        .export(Typescript::default(), path)
        .unwrap_or_else(|error| panic!("生成 TypeScript 命令绑定失败：{error}"));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let command_builder = create_command_builder::<tauri::Wry>();

    tauri::Builder::default()
        .invoke_handler(command_builder.invoke_handler())
        .setup(move |app| {
            command_builder.mount_events(app);
            let paths = app::AppPaths::from_data_root(app.path().app_data_dir()?)?;
            app.manage(app::AppState::initialize(paths)?);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动桌面应用失败");
}
