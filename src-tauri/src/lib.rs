use std::path::{Path, PathBuf};

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
pub mod profiles;
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
        .typ::<sync::ApplyResult>()
        .typ::<sync::SnapshotSummary>()
        .typ::<sync::InterruptedRunPlan>()
        .typ::<sync::RestorePreview>()
        .typ::<profiles::ClaudeCredentialEnvKey>()
        .typ::<profiles::ProviderOptionsInput>()
        .typ::<profiles::ProviderProfileInput>()
        .typ::<profiles::SecretUpdate>()
        .typ::<profiles::UpdateProviderProfileInput>()
        .typ::<profiles::CopyProviderProfileInput>()
        .typ::<profiles::VersionedProfileInput>()
        .typ::<profiles::ProviderOptionsDto>()
        .typ::<profiles::ProviderProfileDto>()
        .typ::<profiles::PromptProfileInput>()
        .typ::<profiles::UpdatePromptProfileInput>()
        .typ::<profiles::PromptProfileDto>()
        .typ::<profiles::ProviderImportPreviewDto>()
        .typ::<profiles::PromptImportPreviewDto>()
        .typ::<profiles::ConfirmImportInput>()
        .typ::<profiles::ApplyProfilePreviewInput>()
        .typ::<profiles::ToolProfileStatusDto>()
        .typ::<profiles::DeleteProfileResultDto>()
        .commands(collect_commands![
            commands::get_app_info,
            commands::profiles::list_provider_profiles,
            commands::profiles::create_provider_profile,
            commands::profiles::update_provider_profile,
            commands::profiles::copy_provider_profile,
            commands::profiles::set_active_provider_profile,
            commands::profiles::delete_provider_profile,
            commands::profiles::list_prompt_profiles,
            commands::profiles::create_prompt_profile,
            commands::profiles::update_prompt_profile,
            commands::profiles::set_active_prompt_profile,
            commands::profiles::delete_prompt_profile,
            commands::profiles::get_tool_profile_status,
            commands::profiles::discover_provider_import,
            commands::profiles::confirm_provider_import,
            commands::profiles::discover_prompt_import,
            commands::profiles::confirm_prompt_import,
            commands::profiles::preview_provider_sync,
            commands::profiles::preview_prompt_sync,
            commands::profiles::apply_profile_preview,
        ])
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
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _cwd| {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            },
        ))
        .invoke_handler(command_builder.invoke_handler())
        .setup(move |app| {
            command_builder.mount_events(app);
            let paths = app::AppPaths::from_data_root(app.path().app_data_dir()?)?;
            let home = app.path().home_dir()?;
            let environment = adapters::ExplicitEnvironment::new(
                &home,
                environment_path("CLAUDE_CONFIG_DIR"),
                environment_path("CODEX_HOME"),
                adapters::ToolAvailability::all_installed(),
            )?
            .with_claude_provider_policy(claude_provider_policy());
            app.manage(app::AppState::initialize_with_environment(
                paths,
                environment,
            )?);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动桌面应用失败");
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn claude_provider_policy() -> adapters::PolicyState {
    match std::env::var_os("CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST") {
        Some(value) if !value.is_empty() => adapters::PolicyState::Blocked,
        Some(_) | None => adapters::PolicyState::Allowed,
    }
}
