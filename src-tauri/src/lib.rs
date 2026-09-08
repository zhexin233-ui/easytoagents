use std::{
    fs,
    path::{Path, PathBuf},
};

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
pub mod hooks;
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
        .typ::<domain::HookEvent>()
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
        .typ::<mcp::ReadoptMcpTargetInput>()
        .typ::<mcp::ReadoptMcpTargetResultDto>()
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
        .typ::<skills::PrepareSkillTakeoverInput>()
        .typ::<skills::SkillTakeoverPreviewResultDto>()
        .typ::<skills::ImportSkillInput>()
        .typ::<skills::ImportGithubSkillInput>()
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
        .typ::<hooks::CreateHookInput>()
        .typ::<hooks::UpdateHookInput>()
        .typ::<hooks::VersionedHookInput>()
        .typ::<hooks::DeleteHookResultDto>()
        .typ::<hooks::HookDto>()
        .typ::<hooks::SetGlobalHookAssignmentInput>()
        .typ::<hooks::SetProjectHookAssignmentInput>()
        .typ::<hooks::HookProjectSelectionState>()
        .typ::<hooks::HookProjectOptionDto>()
        .typ::<hooks::HookProjectDto>()
        .typ::<hooks::HookProjectOptionsInput>()
        .typ::<hooks::PreviewHookSyncInput>()
        .typ::<hooks::ApplyHookPreviewInput>()
        .typ::<hooks::ReadoptHookTargetInput>()
        .typ::<hooks::ReadoptHookTargetResultDto>()
        .typ::<hooks::HookTargetStatusDto>()
        .typ::<hooks::HookImportCandidateStatus>()
        .typ::<hooks::HookImportCandidateDto>()
        .typ::<hooks::HookImportPreviewDto>()
        .typ::<hooks::DiscoverHookImportInput>()
        .typ::<hooks::ConfirmHookImportInput>()
        .typ::<hooks::HookImportResultDto>()
        .typ::<projects::ProjectPathStatus>()
        .typ::<projects::GitRepositoryStatus>()
        .typ::<projects::ProjectTargetStatusDto>()
        .typ::<projects::ProjectDto>()
        .typ::<projects::RegisterProjectInput>()
        .typ::<projects::VersionedProjectInput>()
        .typ::<projects::RemoveProjectResultDto>()
        .typ::<projects::ProjectNativeResourceKind>()
        .typ::<projects::ProjectNativeResourceState>()
        .typ::<projects::ProjectNativeEntryType>()
        .typ::<projects::ProjectNativeResourceAction>()
        .typ::<projects::ProjectNativeResourceSummaryDto>()
        .typ::<projects::ProjectNativeResourceDto>()
        .typ::<projects::ProjectNativeResourceQueryInput>()
        .typ::<projects::PreviewProjectNativeResourceActionInput>()
        .typ::<projects::ApplyProjectNativeResourcePreviewInput>()
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
            commands::overview::delete_snapshots,
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
            commands::projects::list_project_native_resources,
            commands::projects::preview_project_native_resource_action,
            commands::projects::apply_project_native_resource_preview,
            commands::profiles::list_provider_profiles,
            commands::profiles::create_provider_profile,
            commands::profiles::update_provider_profile,
            commands::profiles::copy_provider_profile,
            commands::profiles::set_active_provider_profile,
            commands::profiles::delete_provider_profile,
            commands::profiles::list_prompt_profiles,
            commands::profiles::create_prompt_profile,
            commands::profiles::update_prompt_profile,
            commands::profiles::set_global_prompt_assignment,
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
            commands::mcp::readopt_mcp_target,
            commands::mcp::discover_mcp_import,
            commands::mcp::confirm_mcp_import,
            commands::skills::list_skills,
            commands::skills::get_skill,
            commands::skills::import_skill,
            commands::skills::import_github_skill,
            commands::skills::discover_skill_import,
            commands::skills::confirm_skill_import,
            commands::skills::prepare_skill_takeover,
            commands::skills::preview_skill_content,
            commands::skills::adopt_skill_content,
            commands::skills::delete_skill,
            commands::skills::set_global_skill_assignment,
            commands::skills::set_project_skill_assignment,
            commands::skills::list_skill_projects,
            commands::skills::list_skill_project_options,
            commands::skills::list_global_skill_target_statuses,
            commands::skills::preview_skill_sync,
            commands::skills::apply_skill_preview,
            commands::hooks::list_hooks,
            commands::hooks::get_hook,
            commands::hooks::create_hook,
            commands::hooks::update_hook,
            commands::hooks::set_hook_enabled,
            commands::hooks::delete_hook,
            commands::hooks::set_global_hook_assignment,
            commands::hooks::set_project_hook_assignment,
            commands::hooks::list_hook_projects,
            commands::hooks::list_hook_project_options,
            commands::hooks::list_global_hook_target_statuses,
            commands::hooks::preview_hook_sync,
            commands::hooks::apply_hook_preview,
            commands::hooks::readopt_hook_target,
            commands::hooks::discover_hook_import,
            commands::hooks::confirm_hook_import,
        ])
}

pub fn export_typescript_bindings(path: &Path) {
    create_command_builder::<tauri::Wry>()
        .export(Typescript::default(), path)
        .unwrap_or_else(|error| panic!("生成 TypeScript 命令绑定失败：{error}"));
    // specta 在拆分较长 union 类型时可能留下尾随空格；规范化生成产物，
    // 让 `git diff --check` 与绑定一致性检查同时保持可用。
    let generated = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("读取生成的 TypeScript 命令绑定失败：{error}"));
    let normalized = generated
        .split_inclusive('\n')
        .map(|line| {
            let (content, newline) = line
                .strip_suffix('\n')
                .map_or((line, ""), |content| (content, "\n"));
            format!("{}{}", content.trim_end_matches([' ', '\t']), newline)
        })
        .collect::<String>();
    if generated != normalized {
        fs::write(path, normalized)
            .unwrap_or_else(|error| panic!("写入规范化 TypeScript 命令绑定失败：{error}"));
    }
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
            let mut probe_input = app::tool_probe::ReleaseToolProbeInput::for_macos_release(
                home,
                environment_path("CLAUDE_CONFIG_DIR"),
                environment_path("CODEX_HOME"),
                std::env::var_os("PATH").unwrap_or_default(),
            );
            let opencode_config_dir = environment_path("OPENCODE_CONFIG_DIR").or_else(|| {
                environment_path("XDG_CONFIG_HOME")
                    .map(|xdg_config_home| xdg_config_home.join("opencode"))
            });
            if let Some(opencode_config_dir) = opencode_config_dir {
                probe_input = probe_input.with_opencode_config_dir(Some(opencode_config_dir));
            }
            probe_input = probe_input
                .with_opencode_config_path(environment_path("OPENCODE_CONFIG"))
                .with_opencode_config_content(std::env::var("OPENCODE_CONFIG_CONTENT").ok())
                .with_opencode_disabled(std::env::var("OPENCODE_DISABLE").is_ok());
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
