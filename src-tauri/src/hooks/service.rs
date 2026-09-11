//! Hooks 领域服务、原生投影与持久化 Preview/Apply 编排。
//!
//! 各工具原生合同（2026-09-05 官方文档核验，见任务 09-05-add-hooks-management）：
//! - Claude：`settings.json` 的 `hooks` 键，`{"<Event>": [{matcher?, hooks: [...]}]}`；
//! - Codex：独立 `hooks.json`，顶层 `{"description"?, "hooks": {...同 Claude...}}`；
//! - Cursor：独立 `hooks.json`，`{"version": 1, "hooks": {"<event>": [扁平条目]}}`（camelCase，
//!   matcher 属于条目本身，command 型条目省略 type）；
//! - ZCode：config.json 的 `hooks` 键，`{"enabled"?, "events": {...}}`，
//!   配置文件 hooks 必须 `enabled: true` 才会运行。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{
    models::validate_hook_definition, ApplyHookPreviewInput, CreateHookInput, DeleteHookResultDto,
    HookDto, HookProjectDto, HookProjectOptionDto, HookProjectOptionsInput,
    HookProjectSelectionState, HookTargetStatusDto, PreviewHookSyncInput, ReadoptHookTargetInput,
    ReadoptHookTargetResultDto, SetGlobalHookAssignmentInput, SetProjectHookAssignmentInput,
    UpdateHookInput, VersionedHookInput,
};
use crate::{
    adapters::{
        canonicalize_project_root, descriptor_allowed_root, find_descriptor, projection_value_at,
        DiscoveryContext, ManagedOwnership, TargetDescriptor, ASSIGNABLE_HOOK_TOOLS,
    },
    app::AppPaths,
    db::{
        hooks::{self as repository, HookRecord, ManagedHookItemRecord},
        mcp::{self as mcp_repository, McpProjectRecord},
        Database,
    },
    domain::{ArtifactKind, EntityId, HookEvent, ProjectRoot, Scope, SyncStatus, Tool},
    error::AppError,
    git::inspect_path,
    security::{create_private_file, ensure_private_directory, SecretRedactor},
    sync::hash_bytes,
    sync::{
        apply_persisted_preview, assess_drift, build_preview_plan, hash_json,
        load_managed_target_baseline, load_persisted_preview, persist_preview, safe_row_version,
        scan_target, ApplyResult, ApplyTargetInput, DatabaseRowVersion, ManagedItemApply,
        ManagedTargetBaseline, NoApplyFault, ObservedTarget, PreviewPlan, PreviewTargetRequest,
        TargetScan,
    },
};

// ---------------------------------------------------------------------------
// 中央库 CRUD 与分配
// ---------------------------------------------------------------------------

include!("service_core.rs");
include!("service_native.rs");
#[cfg(test)]
include!("service_tests.rs");
