//! Agents（子代理）领域服务、DTO 与原生同步编排。

mod import;
pub(crate) mod models;
mod service;

pub use import::{confirm_agent_import, discover_agent_import};
pub use models::{
    AgentDto, AgentFileTargetStatusDto, AgentImportCandidateDto, AgentImportPreviewDto,
    AgentImportResultDto, AgentProjectDto, AgentProjectOptionDto, AgentProjectOptionsInput,
    AgentToolSettingsDto, AgentToolTargetStatusDto, ApplyAgentPreviewInput, ClaudeAgentColor,
    ClaudeAgentSettings, CodexAgentSettings, CodexReasoningEffort, ConfirmAgentImportAgent,
    ConfirmAgentImportInput, CreateAgentInput, DeleteAgentResultDto, DiscoverAgentImportInput,
    PreviewAgentSyncInput, ReadoptAgentTargetInput, ReadoptAgentTargetResultDto,
    SetAgentToolSettingsInput, SetGlobalAgentAssignmentInput, SetProjectAgentAssignmentInput,
    UpdateAgentInput, VersionedAgentInput,
};
pub use service::{
    agent_scope_supported, apply_agent_preview, create_agent, delete_agent, get_agent,
    list_agent_project_options, list_agent_projects, list_agents,
    list_global_agent_target_statuses, preview_agent_sync, readopt_agent_target, set_agent_enabled,
    set_agent_tool_settings, set_global_agent_assignment, set_project_agent_assignment,
    update_agent,
};
