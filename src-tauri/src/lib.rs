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
pub mod mcp;
pub mod overview;
pub mod profiles;
pub mod projects;
pub mod security;
pub mod settings;
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
        .typ::<adapters::ToolAvailabilityState>()
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
        .typ::<mcp::McpServerInput>()
        .typ::<mcp::SensitiveMapUpdate>()
        .typ::<mcp::SensitiveJsonUpdate>()
        .typ::<mcp::UpdateMcpServerInput>()
        .typ::<mcp::VersionedMcpInput>()
        .typ::<mcp::McpServerDto>()
        .typ::<mcp::DeleteMcpResultDto>()
        .typ::<mcp::SetGlobalMcpAssignmentInput>()
        .typ::<mcp::SetProjectMcpAssignmentInput>()
        .typ::<mcp::McpProjectSelectionState>()
        .typ::<mcp::McpProjectOptionDto>()
        .typ::<mcp::McpProjectDto>()
        .typ::<mcp::McpProjectOptionsInput>()
        .typ::<mcp::PreviewMcpSyncInput>()
        .typ::<mcp::ApplyMcpPreviewInput>()
        .typ::<mcp::McpTargetStatusDto>()
        .typ::<mcp::McpImportCandidateStatus>()
        .typ::<mcp::McpImportAction>()
        .typ::<mcp::McpImportCandidateDto>()
        .typ::<mcp::McpImportPreviewDto>()
        .typ::<mcp::ConfirmMcpImportInput>()
        .typ::<mcp::McpImportResultDto>()
        .typ::<skills::ConfirmSkillImportInput>()
        .typ::<skills::SkillImportPreviewDto>()
        .typ::<skills::SkillImportResultDto>()
        .typ::<skills::ImportSkillInput>()
        .typ::<skills::VersionedSkillInput>()
        .typ::<skills::SkillDto>()
        .typ::<skills::SkillContentPreviewDto>()
        .typ::<skills::DeleteSkillResultDto>()
        .typ::<skills::SetGlobalSkillAssignmentInput>()
        .typ::<skills::SetProjectSkillAssignmentInput>()
        .typ::<skills::SkillProjectSelectionState>()
        .typ::<skills::SkillProjectOptionDto>()
        .typ::<skills::SkillProjectDto>()
        .typ::<skills::SkillProjectOptionsInput>()
        .typ::<skills::PreviewSkillSyncInput>()
        .typ::<skills::ApplySkillPreviewInput>()
        .typ::<skills::SkillTargetStatusDto>()
        .typ::<projects::ProjectPathStatus>()
        .typ::<projects::GitRepositoryStatus>()
        .typ::<projects::ProjectTargetStatusDto>()
        .typ::<projects::ProjectDto>()
        .typ::<projects::RegisterProjectInput>()
        .typ::<projects::VersionedProjectInput>()
        .typ::<projects::RemoveProjectResultDto>()
        .typ::<overview::DashboardToolSummaryDto>()
        .typ::<overview::RecentSyncRunDto>()
        .typ::<overview::DashboardSummaryDto>()
        .typ::<overview::SnapshotRestoreInput>()
        .typ::<overview::ApplySnapshotRestoreInput>()
        .typ::<overview::CompleteOnboardingResultDto>()
        .typ::<settings::ApplyMode>()
        .typ::<settings::AppSettingsDto>()
        .typ::<settings::UpdateAppSettingsInput>()
        .commands(collect_commands![
            commands::get_app_info,
            commands::overview::get_dashboard_summary,
            commands::overview::complete_onboarding,
            commands::overview::list_snapshots,
            commands::overview::get_interrupted_run,
            commands::overview::preview_snapshot_restore,
            commands::overview::restore_snapshot,
            commands::settings::get_app_settings,
            commands::settings::update_app_settings,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::register_project,
            commands::projects::rescan_project,
            commands::projects::remove_project,
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
            commands::mcp::list_mcp_servers,
            commands::mcp::get_mcp_server,
            commands::mcp::create_mcp_server,
            commands::mcp::update_mcp_server,
            commands::mcp::set_mcp_enabled,
            commands::mcp::delete_mcp_server,
            commands::mcp::set_global_mcp_assignment,
            commands::mcp::set_project_mcp_assignment,
            commands::mcp::list_mcp_projects,
            commands::mcp::list_mcp_project_options,
            commands::mcp::list_global_mcp_target_statuses,
            commands::mcp::preview_mcp_sync,
            commands::mcp::apply_mcp_preview,
            commands::mcp::discover_mcp_import,
            commands::mcp::confirm_mcp_import,
            commands::skills::list_skills,
            commands::skills::get_skill,
            commands::skills::import_skill,
            commands::skills::discover_skill_import,
            commands::skills::confirm_skill_import,
            commands::skills::preview_skill_content,
            commands::skills::delete_skill,
            commands::skills::set_global_skill_assignment,
            commands::skills::set_project_skill_assignment,
            commands::skills::list_skill_projects,
            commands::skills::list_skill_project_options,
            commands::skills::list_global_skill_target_statuses,
            commands::skills::preview_skill_sync,
            commands::skills::apply_skill_preview,
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
        .plugin(tauri_plugin_dialog::init())
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
            let probe_input = app::tool_probe::ReleaseToolProbeInput::for_macos_release(
                home,
                environment_path("CLAUDE_CONFIG_DIR"),
                environment_path("CODEX_HOME"),
                std::env::var_os("PATH").unwrap_or_default(),
            );
            let environment = app::tool_probe::probe_release_environment(&probe_input)?
                .environment
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
