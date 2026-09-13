//! Agents 领域服务、原生投影与持久化 Preview/Apply 编排。
//!
//! 各工具原生合同（2026-09-12 官方文档核验，见任务 09-12-add-agents-management）：
//! - Claude：Markdown + YAML frontmatter（`name`、`description`，以及白名单覆盖层）；
//! - Cursor / ZCode：Markdown + YAML frontmatter（`name`、`description`）；
//! - OpenCode：Markdown + YAML frontmatter（`description` 必填，名称取文件名；
//!   投影固定写 `mode: subagent`，避免中央子代理被当作主代理）；
//! - Codex：TOML（`name`、`description`、`developer_instructions`）。
//!
//! 核心决策：一个受管文件 = 一行 managed_targets，整文件所有权
//! （WholeDocument），不使用 managed_items。ZCode 仅支持全局（官方 Beta，
//! 项目级明示不支持）。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

use super::{
    models::{tool_settings_dto, validate_agent_definition, validate_agent_tool_settings},
    AgentDto, AgentProjectDto, AgentProjectOptionDto, AgentProjectOptionsInput,
    AgentToolTargetStatusDto, ApplyAgentPreviewInput, CreateAgentInput, DeleteAgentResultDto,
    PreviewAgentSyncInput, ReadoptAgentTargetInput, ReadoptAgentTargetResultDto,
    SetAgentToolSettingsInput, SetGlobalAgentAssignmentInput, SetProjectAgentAssignmentInput,
    UpdateAgentInput, VersionedAgentInput,
};
use crate::{
    adapters::{
        agent_file_extension, canonicalize_project_root, descriptor_allowed_root, find_descriptor,
        DiscoveryContext, ManagedOwnership, TargetDescriptor, ASSIGNABLE_AGENT_TOOLS,
        PROJECT_AGENT_TOOLS,
    },
    app::AppPaths,
    db::{
        agents::{self as repository, AgentManagedTargetRecord, AgentRecord},
        mcp::{self as mcp_repository, McpProjectRecord},
        Database,
    },
    domain::{ArtifactKind, ProjectRoot, Scope, SyncStatus, Tool},
    error::AppError,
    git::inspect_path,
    security::SecretRedactor,
    sync::{
        apply_persisted_preview, assess_drift, build_preview_plan, load_managed_target_baseline,
        load_persisted_preview, persist_preview, safe_row_version, scan_target, ApplyResult,
        ApplyTargetInput, DatabaseEntityType, DatabaseRowVersion, ManagedTargetBaseline,
        NoApplyFault, PreviewPlan, PreviewTargetRequest, TargetScan,
    },
};

include!("service_core.rs");
include!("service_native.rs");
#[cfg(test)]
include!("service_tests.rs");
