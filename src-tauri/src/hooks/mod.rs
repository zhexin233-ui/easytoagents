//! Hooks 领域服务、DTO 与原生同步编排。

mod import;
mod models;
mod service;

pub use import::{confirm_hook_import, discover_hook_import};
pub use models::{
    ApplyHookPreviewInput, ConfirmHookImportInput, CreateHookInput, DeleteHookResultDto,
    DiscoverHookImportInput, HookDto, HookGlobalAssignmentDto, HookImportCandidateDto,
    HookImportCandidateStatus, HookImportPreviewDto, HookImportResultDto, HookProjectDto,
    HookProjectOptionDto, HookProjectOptionsInput, HookProjectSelectionState, HookTargetStatusDto,
    PreviewHookSyncInput, ReadoptHookTargetInput, ReadoptHookTargetResultDto,
    SetGlobalHookAssignmentInput, SetProjectHookAssignmentInput, UpdateHookInput,
    VersionedHookInput,
};
pub use service::{
    apply_hook_preview, create_hook, delete_hook, get_hook, hook_event_supported,
    list_global_hook_target_statuses, list_hook_project_options, list_hook_projects, list_hooks,
    preview_hook_sync, readopt_hook_target, set_global_hook_assignment, set_hook_enabled,
    set_project_hook_assignment, update_hook,
};

pub(crate) use service::{build_hook_ownership, events_root, native_entry_hashes};
