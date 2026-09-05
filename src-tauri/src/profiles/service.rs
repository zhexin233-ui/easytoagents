//! Provider/Prompt 中央意图、首次接管与 Preview/Apply 编排。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::models::{
    validate_prompt_fields, validate_provider_fields, validate_provider_fields_with_optional_key,
    ClaudeCredentialEnvKey, ConfirmImportInput, CopyProviderProfileInput, DeleteProfileResultDto,
    PromptImportPreviewDto, PromptProfileDto, PromptProfileInput, PromptProjectAssignmentDto,
    ProviderImportPreviewDto, ProviderOptionsInput, ProviderProfileDto, ProviderProfileInput,
    SecretUpdate, SetGlobalPromptAssignmentInput, SetPromptProjectAssignmentInput,
    StoredProviderConfig, ToolProfileStatusDto, UpdatePromptProfileInput,
    UpdateProviderProfileInput, VersionedProfileInput, CODEX_BEARER_TOKEN_WARNING,
    NEW_SESSION_NOTICE,
};
use crate::{
    adapters::{
        canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
        cursor::CursorAdapter, zcode::ZcodeAdapter, DiscoveryContext, ExplicitEnvironment,
        ManagedOwnership, PolicyState, TargetDescriptor, ToolAdapter,
    },
    app::AppPaths,
    db::{
        profiles::{
            self as repository, ImportPreviewRecord, ImportedBaselineRecord,
            NewPromptProfileRecord, NewProviderProfileRecord, PromptProfileRecord,
            ProviderProfileRecord,
        },
        projects::get_registered_project,
        Database,
    },
    domain::{ArtifactKind, ArtifactName, ProjectRoot, Scope, Tool},
    error::{AppError, ErrorCode},
    git::inspect_path,
    git::GitPathStatus,
    security::SecretRedactor,
    sync::{
        apply_persisted_preview, build_preview_plan, hash_json, load_persisted_preview,
        persist_preview, scan_target, ApplyResult, ApplyTargetInput, DatabaseEntityType,
        DatabaseRowVersion, ManagedTargetBaseline, NoApplyFault, PreviewPlan, PreviewTargetRequest,
        TargetScan,
    },
};

const CLAUDE_BASE_URL_KEY: &str = "ANTHROPIC_BASE_URL";
const CLAUDE_MODEL_KEY: &str = "ANTHROPIC_MODEL";
const CLAUDE_DEFAULT_MODEL_KEYS: &[&str] = &[
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
];
const CLAUDE_PROVIDER_MANAGED_BY_HOST_KEY: &str = "CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST";
const CODEX_OPENAI_PROVIDER_ID: &str = "openai";
const CODEX_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const CODEX_RESERVED_PROVIDER_IDS: &[&str] = &["openai", "ollama", "lmstudio"];

pub fn list_provider_profiles(
    database: &Database,
    tool: Tool,
) -> Result<Vec<ProviderProfileDto>, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Provider)?;
    repository::list_provider_profiles(database, tool)?
        .iter()
        .map(provider_dto)
        .collect()
}

pub fn create_provider_profile(
    database: &mut Database,
    redactor: &mut SecretRedactor,
    input: ProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Provider)?;
    validate_provider_fields(
        &input.name,
        &input.api_base_url,
        &input.api_key,
        &input.default_model,
    )?;
    let id = Uuid::new_v4().to_string();
    let provider_id = generated_codex_provider_id(&id);
    let config =
        StoredProviderConfig::from_input(input.tool, &provider_id, input.options, BTreeMap::new())?;
    let config_json = serde_json::to_string(&config)
        .map_err(|_| AppError::invalid_input("providerOptions", "Provider 选项无法序列化"))?;
    redactor.register_secret(input.api_key.clone());
    let record = repository::insert_provider_profile(
        database,
        &NewProviderProfileRecord {
            id,
            tool: input.tool,
            name: input.name,
            api_base_url: Some(input.api_base_url),
            api_key: Some(input.api_key),
            default_model: Some(input.default_model),
            config_json,
            is_active: input.activate,
        },
    )?;
    provider_dto(&record)
}

pub fn update_provider_profile(
    database: &mut Database,
    redactor: &mut SecretRedactor,
    input: UpdateProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    let current = repository::get_provider_profile(database, &input.id)?;
    let current_config = parse_stored_provider_config(&current)?;
    let provider_id = match current.tool {
        Tool::Claude => generated_codex_provider_id(&current.id),
        Tool::Codex => current_config.provider_id.clone().ok_or_else(|| {
            AppError::invalid_input("providerOptions", "Codex Provider 缺少稳定 provider id")
        })?,
        Tool::Zcode => current_config.provider_id.clone().ok_or_else(|| {
            AppError::invalid_input("providerOptions", "ZCode Provider 缺少稳定 provider id")
        })?,
        Tool::Cursor => return Err(cursor_unsupported(ArtifactKind::Provider)),
    };
    let allow_missing_api_key =
        codex_provider_allows_missing_api_key(current.tool, current_config.provider_id.as_deref());
    let config = StoredProviderConfig::from_input(
        current.tool,
        &provider_id,
        input.options,
        current_config.extra_provider_fields,
    )?;
    let api_key = match input.api_key {
        SecretUpdate::Keep => current.api_key.clone(),
        SecretUpdate::Clear => None,
        SecretUpdate::Replace(value) if value.is_empty() => None,
        SecretUpdate::Replace(value) => Some(value),
    };
    if allow_missing_api_key && api_key.is_some() {
        return Err(AppError::invalid_input(
            "apiKey",
            "Codex OAuth Provider 使用官方登录凭据，不能保存本地 API Key",
        ));
    }
    validate_provider_fields_with_optional_key(
        &input.name,
        &input.api_base_url,
        api_key.as_deref(),
        &input.default_model,
        allow_missing_api_key,
    )?;
    if let Some(api_key) = &api_key {
        redactor.register_secret(api_key.clone());
    }
    let row_version = i64::from(input.row_version);
    let record = repository::update_provider_profile(
        database,
        &input.id,
        &input.name,
        Some(&input.api_base_url),
        api_key.as_deref(),
        Some(&input.default_model),
        &serde_json::to_string(&config)
            .map_err(|_| AppError::invalid_input("providerOptions", "Provider 选项无法序列化"))?,
        row_version,
    )?;
    provider_dto(&record)
}

pub fn copy_provider_profile(
    database: &mut Database,
    redactor: &mut SecretRedactor,
    input: CopyProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    ensure_profile_capability(input.target_tool, ArtifactKind::Provider)?;
    let source = repository::get_provider_profile(database, &input.source_id)?;
    if source.tool == input.target_tool {
        return Err(AppError::invalid_input(
            "targetTool",
            "跨工具复制的目标必须与来源工具不同",
        ));
    }
    let api_base_url = source.api_base_url.clone().unwrap_or_default();
    let api_key = source.api_key.clone().unwrap_or_default();
    let default_model = source.default_model.clone().unwrap_or_default();
    validate_provider_fields(&input.target_name, &api_base_url, &api_key, &default_model)?;
    let target_id = Uuid::new_v4().to_string();
    let target_options = match input.target_tool {
        Tool::Claude => ProviderOptionsInput {
            credential_env_key: Some(ClaudeCredentialEnvKey::ApiKey),
            extra_env: BTreeMap::new(),
            wire_api: None,
            zcode_kind: None,
        },
        Tool::Codex => ProviderOptionsInput::default(),
        Tool::Zcode => ProviderOptionsInput {
            credential_env_key: None,
            extra_env: BTreeMap::new(),
            wire_api: None,
            zcode_kind: Some("anthropic".to_owned()),
        },
        Tool::Cursor => return Err(cursor_unsupported(ArtifactKind::Provider)),
    };
    let config = StoredProviderConfig::from_input(
        input.target_tool,
        &generated_codex_provider_id(&target_id),
        target_options,
        BTreeMap::new(),
    )?;
    redactor.register_secret(api_key.clone());
    let copied = repository::insert_provider_profile(
        database,
        &NewProviderProfileRecord {
            id: target_id,
            tool: input.target_tool,
            name: input.target_name,
            api_base_url: Some(api_base_url),
            api_key: Some(api_key),
            default_model: Some(default_model),
            config_json: serde_json::to_string(&config).map_err(|_| {
                AppError::invalid_input("providerOptions", "目标 Provider 选项无法序列化")
            })?,
            is_active: input.activate,
        },
    )?;
    provider_dto(&copied)
}

pub fn set_active_provider_profile(
    database: &mut Database,
    tool: Tool,
    input: &VersionedProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Provider)?;
    provider_dto(&repository::set_active_provider_profile(
        database,
        tool,
        &input.id,
        i64::from(input.row_version),
    )?)
}

pub fn delete_provider_profile(
    database: &mut Database,
    input: &VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    repository::delete_provider_profile(database, &input.id, i64::from(input.row_version))?;
    Ok(DeleteProfileResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

pub fn list_prompt_profiles(database: &Database) -> Result<Vec<PromptProfileDto>, AppError> {
    repository::list_prompt_profiles(database)?
        .iter()
        .map(prompt_dto)
        .collect()
}

pub fn create_prompt_profile(
    database: &mut Database,
    input: PromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    validate_prompt_fields(&input.name, &input.body)?;
    // 新建档案不绑定工具也不自动启用；用图标按工具启用（导入路径除外，见 confirm_prompt_import）。
    prompt_dto(&repository::insert_prompt_profile(
        database,
        &NewPromptProfileRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            body: input.body,
            is_active_claude: false,
            is_active_codex: false,
            is_active_zcode: false,
            imported_from_path: None,
        },
    )?)
}

pub fn update_prompt_profile(
    database: &mut Database,
    input: UpdatePromptProfileInput,
) -> Result<PromptProfileDto, AppError> {
    validate_prompt_fields(&input.name, &input.body)?;
    prompt_dto(&repository::update_prompt_profile(
        database,
        &input.id,
        &input.name,
        &input.body,
        i64::from(input.row_version),
    )?)
}

pub fn set_global_prompt_assignment(
    database: &mut Database,
    input: &SetGlobalPromptAssignmentInput,
) -> Result<PromptProfileDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Prompt)?;
    prompt_dto(&repository::set_global_prompt_assignment(
        database,
        input.tool,
        &input.prompt_profile_id,
        input.assigned,
        i64::from(input.row_version),
    )?)
}

pub fn delete_prompt_profile(
    database: &mut Database,
    input: &VersionedProfileInput,
) -> Result<DeleteProfileResultDto, AppError> {
    let assigned_projects = repository::count_prompt_project_assignments(database, &input.id)?;
    if assigned_projects > 0 {
        return Err(AppError::conflict(
            "promptProfile",
            "该提示词档案仍被项目分配使用，请先在项目中解除分配",
        ));
    }
    repository::delete_prompt_profile(database, &input.id, i64::from(input.row_version))?;
    Ok(DeleteProfileResultDto {
        id: input.id.clone(),
        deleted: true,
    })
}

pub fn set_prompt_project_assignment(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &SetPromptProjectAssignmentInput,
) -> Result<PromptProjectAssignmentDto, AppError> {
    ensure_profile_capability(input.tool, ArtifactKind::Prompt)?;
    let project = get_registered_project(database, &input.project_id)?;
    let tool = input.tool;
    if input.prompt_profile_id.is_some() {
        // 分配前确认目标描述符存在（工具可用、信任/策略状态在预览阶段仍会校验）。
        // 档案存在性、「对该工具全局生效不可分配」守卫与幂等例外
        // 在 repository 事务内统一执行。
        let project_root = canonical_project_root(&project.root_path)?;
        descriptor_for_scope(environment, tool, ArtifactKind::Prompt, Some(&project_root))?;
    }
    repository::set_prompt_project_assignment(
        database,
        &project.id,
        tool,
        input.prompt_profile_id.as_deref(),
        input.project_row_version,
    )?;
    get_prompt_project_assignment(database, &project.id, tool)
}

pub fn get_prompt_project_assignment(
    database: &Database,
    project_id: &str,
    tool: Tool,
) -> Result<PromptProjectAssignmentDto, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Prompt)?;
    let assignment = repository::find_prompt_project_assignment(database, project_id, tool)?;
    Ok(PromptProjectAssignmentDto {
        project_id: project_id.to_owned(),
        tool,
        profile_id: assignment.map(|record| record.id),
    })
}

fn canonical_project_root(path: &str) -> Result<ProjectRoot, AppError> {
    let canonical = canonicalize_project_root(Path::new(path))?;
    if canonical.as_str() != path {
        return Err(AppError::conflict(
            "projectRoot",
            "登记项目根与当前 canonical 路径不一致",
        ));
    }
    Ok(canonical)
}

pub fn get_tool_profile_status(
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<ToolProfileStatusDto, AppError> {
    let mut provider = descriptor_for(environment, tool, ArtifactKind::Provider)?;
    refine_claude_provider_policy(&mut provider);
    let prompt = descriptor_for(environment, tool, ArtifactKind::Prompt)?;
    Ok(ToolProfileStatusDto {
        tool,
        availability: environment.tool_availability(tool),
        installation_version: environment.installation_version(tool).map(str::to_owned),
        provider_target_path: provider.path.clone(),
        prompt_target_path: prompt.path.clone(),
        provider_capability: provider.capability,
        prompt_capability: prompt.capability,
        prompt_override: prompt.prompt_override,
        provider_policy: provider.policy,
        new_session_notice: NEW_SESSION_NOTICE.to_owned(),
        bearer_token_warning: (tool == Tool::Codex).then(|| CODEX_BEARER_TOKEN_WARNING.to_owned()),
    })
}

pub fn discover_provider_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &SecretRedactor,
    tool: Tool,
) -> Result<Option<ProviderImportPreviewDto>, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Provider)?;
    if !repository::list_provider_profiles(database, tool)?.is_empty() {
        return Ok(None);
    }
    let discovered = match discover_native_provider(environment, tool)? {
        Some(discovered) => discovered,
        None => return Ok(None),
    };
    let suggested_name = discovered
        .suggested_name
        .as_ref()
        .filter(|name| ArtifactName::parse((*name).clone()).is_ok())
        .cloned()
        .unwrap_or_else(|| "已导入渠道".to_owned());
    validate_provider_fields_with_optional_key(
        &suggested_name,
        &discovered.api_base_url,
        discovered.api_key.as_deref(),
        &discovered.default_model,
        discovered_provider_allows_missing_api_key(tool, &discovered),
    )?;
    validate_discovered_provider_config(tool, &discovered)?;
    let preview_id = Uuid::new_v4().to_string();
    let mut target_redactor = redactor.clone();
    if let Some(api_key) = &discovered.api_key {
        target_redactor.register_secret(api_key.clone());
    }
    let redacted_projection = target_redactor
        .redact_structure(&discovered.projection)
        .into_value();
    repository::persist_import_preview(
        database,
        &ImportPreviewRecord {
            id: preview_id.clone(),
            tool,
            artifact_kind: ArtifactKind::Provider,
            target_path: discovered.target_path.clone(),
            observed_full_hash: discovered.full_hash,
            suggested_name: suggested_name.clone(),
            redacted_preview_json: serde_json::to_string(&json!({
                "projection": redacted_projection,
                "apiKeyConfigured": discovered.api_key.is_some(),
            }))
            .map_err(|_| AppError::invalid_input("importPreview", "导入预览无法序列化"))?,
            status: "previewed".to_owned(),
        },
    )?;
    Ok(Some(ProviderImportPreviewDto {
        preview_id,
        tool,
        target_path: discovered.target_path,
        suggested_name,
        api_base_url: discovered.api_base_url,
        api_key_configured: discovered.api_key.is_some(),
        default_model: discovered.default_model,
        redacted_projection,
    }))
}

pub fn confirm_provider_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    input: ConfirmImportInput,
) -> Result<ProviderProfileDto, AppError> {
    let preview = repository::get_import_preview(database, &input.preview_id)?;
    if preview.artifact_kind != ArtifactKind::Provider || preview.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &input.preview_id,
            &preview.status,
        ));
    }
    ArtifactName::parse(input.name.clone())?;
    let discovered = discover_native_provider(environment, preview.tool)?
        .ok_or_else(|| AppError::stale_preview(&preview.id, &preview.target_path))?;
    if discovered.target_path != preview.target_path
        || discovered.full_hash != preview.observed_full_hash
    {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let id = Uuid::new_v4().to_string();
    let provider_id = discovered
        .provider_id
        .clone()
        .unwrap_or_else(|| generated_codex_provider_id(&id));
    validate_codex_provider_id(preview.tool, &provider_id)?;
    validate_provider_fields_with_optional_key(
        &input.name,
        &discovered.api_base_url,
        discovered.api_key.as_deref(),
        &discovered.default_model,
        codex_provider_allows_missing_api_key(preview.tool, Some(&provider_id)),
    )?;
    let options = match preview.tool {
        Tool::Claude => ProviderOptionsInput {
            credential_env_key: Some(discovered.credential_env_key),
            extra_env: discovered.extra_env,
            wire_api: None,
            zcode_kind: None,
        },
        Tool::Codex => ProviderOptionsInput {
            credential_env_key: None,
            extra_env: BTreeMap::new(),
            wire_api: discovered.wire_api,
            zcode_kind: None,
        },
        Tool::Zcode => ProviderOptionsInput {
            credential_env_key: None,
            extra_env: BTreeMap::new(),
            wire_api: None,
            zcode_kind: Some(
                discovered
                    .zcode_kind
                    .clone()
                    .unwrap_or_else(|| "anthropic".to_owned()),
            ),
        },
        Tool::Cursor => return Err(cursor_unsupported(ArtifactKind::Provider)),
    };
    let config = StoredProviderConfig::from_input(
        preview.tool,
        &provider_id,
        options,
        discovered.extra_provider_fields,
    )?;
    if let Some(api_key) = &discovered.api_key {
        redactor.register_secret(api_key.clone());
    }
    let projection_json = serde_json::to_string(&discovered.projection)
        .map_err(|_| AppError::invalid_input("importPreview", "Provider 基线无法序列化"))?;
    let record = repository::adopt_imported_provider(
        database,
        &preview,
        &NewProviderProfileRecord {
            id,
            tool: preview.tool,
            name: input.name,
            api_base_url: Some(discovered.api_base_url),
            api_key: discovered.api_key,
            default_model: Some(discovered.default_model),
            config_json: serde_json::to_string(&config).map_err(|_| {
                AppError::invalid_input("providerOptions", "导入 Provider 选项无法序列化")
            })?,
            is_active: true,
        },
        &ImportedBaselineRecord {
            target_id: Uuid::new_v4().to_string(),
            target_path: discovered.target_path,
            full_hash: discovered.full_hash,
            managed_hash: hash_json(&discovered.projection),
            projection_json,
        },
    )?;
    provider_dto(&record)
}

pub fn discover_prompt_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Option<PromptImportPreviewDto>, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Prompt)?;
    let descriptor = descriptor_for(environment, tool, ArtifactKind::Prompt)?;
    ensure_tool_is_available(&descriptor)?;
    // 该工具已有生效档案、或档案库已有同源导入（imported_from_path 相同）时不再检测。
    let prompt_path = descriptor_path(&descriptor)?;
    if repository::prompt_import_blocked(database, tool, &prompt_path)? {
        return Ok(None);
    }
    let scan = scan_target(
        tool_adapter(tool),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    );
    let observed = match scan {
        TargetScan::Observed(observed) => observed,
        TargetScan::Missing => return Ok(None),
        TargetScan::ParseError => {
            return Err(AppError::parse(&descriptor_path(&descriptor)?, "markdown"));
        }
        _ => return Err(scan_error(&descriptor, &scan)),
    };
    let prompt_path = descriptor_path(&descriptor)?;
    let body = observed
        .managed_projection
        .as_str()
        .ok_or_else(|| AppError::parse(&prompt_path, "markdown"))?
        .to_owned();
    if body.trim().is_empty() {
        return Ok(None);
    }
    let preview_id = Uuid::new_v4().to_string();
    let target_path = prompt_path;
    let suggested_name = "已导入提示词".to_owned();
    repository::persist_import_preview(
        database,
        &ImportPreviewRecord {
            id: preview_id.clone(),
            tool,
            artifact_kind: ArtifactKind::Prompt,
            target_path: target_path.clone(),
            observed_full_hash: observed.full_hash.clone(),
            suggested_name: suggested_name.clone(),
            redacted_preview_json: "{}".to_owned(),
            status: "previewed".to_owned(),
        },
    )?;
    Ok(Some(PromptImportPreviewDto {
        preview_id,
        tool,
        target_path,
        suggested_name,
        body,
    }))
}

pub fn confirm_prompt_import(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: ConfirmImportInput,
) -> Result<PromptProfileDto, AppError> {
    let preview = repository::get_import_preview(database, &input.preview_id)?;
    if preview.artifact_kind != ArtifactKind::Prompt || preview.status != "previewed" {
        return Err(AppError::preview_already_consumed(
            &input.preview_id,
            &preview.status,
        ));
    }
    let descriptor = descriptor_for(environment, preview.tool, ArtifactKind::Prompt)?;
    ensure_tool_is_available(&descriptor)?;
    if descriptor_path(&descriptor)? != preview.target_path {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let observed = match scan_target(
        tool_adapter(preview.tool),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    ) {
        TargetScan::Observed(observed) if observed.full_hash == preview.observed_full_hash => {
            observed
        }
        _ => return Err(AppError::stale_preview(&preview.id, &preview.target_path)),
    };
    let body = observed
        .managed_projection
        .as_str()
        .ok_or_else(|| AppError::parse(&preview.target_path, "markdown"))?
        .to_owned();
    validate_prompt_fields(&input.name, &body)?;
    let record = repository::adopt_imported_prompt(
        database,
        &preview,
        &NewPromptProfileRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            body: body.clone(),
            is_active_claude: preview.tool == Tool::Claude,
            is_active_codex: preview.tool == Tool::Codex,
            is_active_zcode: preview.tool == Tool::Zcode,
            imported_from_path: Some(preview.target_path.clone()),
        },
        &ImportedBaselineRecord {
            target_id: Uuid::new_v4().to_string(),
            target_path: preview.target_path.clone(),
            full_hash: observed.full_hash,
            managed_hash: observed.managed_hash,
            projection_json: serde_json::to_string(&Value::String(body))
                .map_err(|_| AppError::invalid_input("body", "提示词基线无法序列化"))?,
        },
    )?;
    prompt_dto(&record)
}

pub fn preview_provider_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    tool: Tool,
) -> Result<PreviewPlan, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Provider)?;
    let prepared = prepare_provider_sync(database, environment, redactor, tool)?;
    persist_prepared_preview(database, prepared, redactor)
}

pub fn preview_prompt_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &SecretRedactor,
    tool: Tool,
    project_id: Option<String>,
) -> Result<PreviewPlan, AppError> {
    ensure_profile_capability(tool, ArtifactKind::Prompt)?;
    let prepared = prepare_prompt_sync(database, environment, tool, project_id.as_deref())?;
    persist_prepared_preview(database, prepared, redactor)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_profile_preview(
    write_operations: &Mutex<()>,
    database: &mut Database,
    paths: &AppPaths,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    preview_id: &str,
    tool: Tool,
    artifact_kind: ArtifactKind,
    project_id: Option<&str>,
) -> Result<ApplyResult, AppError> {
    ensure_profile_capability(tool, artifact_kind)?;
    if !matches!(artifact_kind, ArtifactKind::Provider | ArtifactKind::Prompt) {
        return Err(AppError::invalid_input(
            "artifactKind",
            "档案预览只能应用 Provider 或 Prompt",
        ));
    }
    let persisted = load_persisted_preview(database, preview_id)?;
    if persisted.items.len() != 1
        || persisted.items[0].envelope.descriptor.tool != tool
        || persisted.items[0].envelope.descriptor.artifact_kind != artifact_kind
        || persisted.items[0].envelope.descriptor.scope
            != if project_id.is_some() {
                Scope::Project
            } else {
                Scope::Global
            }
    {
        return Err(AppError::stale_preview(preview_id, "profileTarget"));
    }
    let prepared = match artifact_kind {
        ArtifactKind::Provider => prepare_provider_sync(database, environment, redactor, tool)?,
        ArtifactKind::Prompt => prepare_prompt_sync(database, environment, tool, project_id)?,
        ArtifactKind::Mcp | ArtifactKind::Skill => unreachable!("已在入口拒绝"),
    };
    let input = ApplyTargetInput {
        descriptor: prepared.descriptor,
        ownership: prepared.ownership,
        desired_projection: prepared.desired_projection,
        allowed_root: prepared.allowed_root,
        central_skills_root: None,
        delete_target: prepared.delete_target,
        managed_items: Vec::new(),
        remove_managed_item_ids: Vec::new(),
        skill_takeover_entries: Vec::new(),
        project_native_action: None,
    };
    apply_persisted_preview(
        write_operations,
        database,
        paths,
        preview_id,
        &[input],
        &NoApplyFault,
    )
}

struct PreparedProfileSync {
    descriptor: TargetDescriptor,
    ownership: ManagedOwnership,
    baseline: ManagedTargetBaseline,
    scan: TargetScan,
    desired_projection: Value,
    row_versions: Vec<DatabaseRowVersion>,
    allowed_root: PathBuf,
    git: Option<GitPathStatus>,
    project_id: Option<String>,
    delete_target: bool,
}

fn prepare_provider_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    redactor: &mut SecretRedactor,
    tool: Tool,
) -> Result<PreparedProfileSync, AppError> {
    let mut descriptor = descriptor_for(environment, tool, ArtifactKind::Provider)?;
    refine_claude_provider_policy(&mut descriptor);
    ensure_tool_is_available(&descriptor)?;
    let target = ensure_profile_target(database, &descriptor, None)?;
    let active = repository::find_active_provider_profile(database, tool)?;
    if active.is_none() && target.baseline.full_hash.is_none() {
        return Err(AppError::not_found("activeProviderProfile", tool.as_str()));
    }
    if let Some(secret) = active.as_ref().and_then(|profile| profile.api_key.as_ref()) {
        redactor.register_secret(secret.clone());
    }
    let desired_projection = active
        .as_ref()
        .map(provider_projection)
        .transpose()?
        .unwrap_or_else(|| Value::Object(Map::new()));
    let ownership = provider_ownership(tool, target.projection.as_ref(), &desired_projection)?;
    let scan = scan_target(tool_adapter(tool), &descriptor, &ownership);
    let row_versions = active
        .as_ref()
        .map(provider_row_version)
        .transpose()?
        .into_iter()
        .collect();
    Ok(PreparedProfileSync {
        allowed_root: allowed_root(environment, tool),
        descriptor,
        ownership,
        baseline: target.baseline,
        scan,
        desired_projection,
        row_versions,
        git: None,
        project_id: None,
        delete_target: false,
    })
}

fn prepare_prompt_sync(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<PreparedProfileSync, AppError> {
    let project = project_id
        .map(|id| get_registered_project(database, id))
        .transpose()?;
    let project_root = project
        .as_ref()
        .map(|project| canonical_project_root(&project.root_path))
        .transpose()?;
    let descriptor = descriptor_for_scope(
        environment,
        tool,
        ArtifactKind::Prompt,
        project_root.as_ref(),
    )?;
    ensure_tool_is_available(&descriptor)?;
    let target = ensure_profile_target(
        database,
        &descriptor,
        project.as_ref().map(|p| p.id.as_str()),
    )?;
    let assigned = match project.as_ref() {
        Some(project) => repository::find_prompt_project_assignment(database, &project.id, tool)?,
        None => repository::find_active_prompt_profile(database, tool)?,
    };
    let assigned = match assigned {
        Some(record) => Some(record),
        // 全局作用域在「无生效档案且从未建立基线」时报错，与既有合同一致；
        // 项目作用域仅在已建立基线但分配被移除时可能出现同样形状，同样报错。
        None if target.baseline.full_hash.is_none() => {
            return Err(AppError::not_found(
                if project.is_some() {
                    "promptProjectAssignment"
                } else {
                    "activePromptProfile"
                },
                project
                    .as_ref()
                    .map(|p| p.id.as_str())
                    .unwrap_or(tool.as_str()),
            ));
        }
        None => None,
    };
    if project.is_some() && assigned.is_none() {
        // 项目分配与基线状态不一致：不允许把空文档写入项目记忆文件。
        return Err(AppError::conflict(
            "promptProjectAssignment",
            "项目提示词分配缺失但目标基线仍存在",
        ));
    }
    let desired_projection = Value::String(
        assigned
            .as_ref()
            .map(|profile| profile.body.clone())
            .unwrap_or_default(),
    );
    let row_versions = assigned
        .as_ref()
        .map(prompt_row_version)
        .transpose()?
        .into_iter()
        .collect();
    let scan = scan_target(
        tool_adapter(tool),
        &descriptor,
        &ManagedOwnership::WholeDocument,
    );
    // 项目作用域的产品语义允许「本地已修改后覆盖式重新应用」：外部修改不算
    // 阻断冲突，而是把观测到的当前内容作为本次预览的确认基线（不落库）。
    // Apply 端指纹绑定仍保证预览与应用之间目标未被再次改动；全局作用域
    // 保持外部修改必须走接管导入的严格语义。
    let baseline = match (&target.baseline, &scan) {
        (
            ManagedTargetBaseline {
                full_hash: Some(full),
                managed_hash: Some(managed),
                ..
            },
            TargetScan::Observed(observed),
        ) if project.is_some()
            && (full != &observed.full_hash || managed != &observed.managed_hash) =>
        {
            ManagedTargetBaseline {
                full_hash: Some(observed.full_hash.clone()),
                managed_hash: Some(observed.managed_hash.clone()),
                ..target.baseline.clone()
            }
        }
        _ => target.baseline.clone(),
    };
    let allowed_root = match project_root.as_ref() {
        Some(root) => PathBuf::from(root.as_str()),
        None => allowed_root(environment, tool),
    };
    let git = project_root
        .as_ref()
        .zip(descriptor.path.as_deref())
        .map(|(root, path)| inspect_path(root, Path::new(path)))
        .transpose()?;
    let prepared_project_id = project.as_ref().map(|project| project.id.clone());
    let prepared_delete_target = assigned.is_none() && project.is_none();
    Ok(PreparedProfileSync {
        allowed_root,
        git,
        descriptor,
        ownership: ManagedOwnership::WholeDocument,
        baseline,
        scan,
        desired_projection,
        row_versions,
        project_id: prepared_project_id,
        delete_target: prepared_delete_target,
    })
}

fn persist_prepared_preview(
    database: &mut Database,
    prepared: PreparedProfileSync,
    redactor: &SecretRedactor,
) -> Result<PreviewPlan, AppError> {
    let plan = build_preview_plan(
        prepared.descriptor.scope,
        prepared.project_id.clone(),
        vec![PreviewTargetRequest {
            descriptor: prepared.descriptor,
            ownership: prepared.ownership,
            baseline: prepared.baseline,
            scan: prepared.scan,
            baseline_mismatched_items: Vec::new(),
            readopt_available: false,
            desired_projection: prepared.desired_projection,
            row_versions: prepared.row_versions,
            git: prepared.git,
            exclude_from_git: false,
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
        }],
        redactor,
    )?;
    persist_preview(database, &plan)?;
    Ok(plan)
}

struct ManagedProfileTarget {
    baseline: ManagedTargetBaseline,
    projection: Option<Value>,
}

fn ensure_profile_target(
    database: &mut Database,
    descriptor: &TargetDescriptor,
    project_id: Option<&str>,
) -> Result<ManagedProfileTarget, AppError> {
    let target_path = descriptor_path(descriptor)?;
    let database_path = database.path().to_string_lossy().into_owned();
    let existing = database
        .connection()
        .query_row(
            "SELECT id, row_version, baseline_full_hash, baseline_managed_hash,
                    baseline_projection_json
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = ?2 AND scope = ?3
               AND ifnull(project_id, '') = ifnull(?4, '') AND target_path = ?5",
            params![
                descriptor.tool.as_str(),
                descriptor.artifact_kind.as_str(),
                descriptor.scope.as_str(),
                project_id,
                target_path,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "find_profile_managed_target"))?;
    let row = if let Some(row) = existing {
        row
    } else {
        let id = Uuid::new_v4().to_string();
        database
            .connection_mut()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, project_id, target_path
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    descriptor.tool.as_str(),
                    descriptor.artifact_kind.as_str(),
                    descriptor.scope.as_str(),
                    project_id,
                    target_path,
                ],
            )
            .map_err(|_| AppError::database(&database_path, "insert_profile_managed_target"))?;
        (id, 1, None, None, None)
    };
    let projection = row
        .4
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|_| AppError::database(&database_path, "parse_profile_managed_baseline"))
        })
        .transpose()?;
    Ok(ManagedProfileTarget {
        baseline: ManagedTargetBaseline {
            target_id: row.0,
            target_row_version: row.1,
            full_hash: row.2,
            managed_hash: row.3,
        },
        projection,
    })
}

struct DiscoveredProvider {
    target_path: String,
    full_hash: String,
    projection: Value,
    api_base_url: String,
    api_key: Option<String>,
    default_model: String,
    credential_env_key: ClaudeCredentialEnvKey,
    extra_env: BTreeMap<String, String>,
    provider_id: Option<String>,
    wire_api: Option<String>,
    zcode_kind: Option<String>,
    extra_provider_fields: BTreeMap<String, Value>,
    suggested_name: Option<String>,
}

fn validate_discovered_provider_config(
    tool: Tool,
    discovered: &DiscoveredProvider,
) -> Result<(), AppError> {
    let options = match tool {
        Tool::Claude => ProviderOptionsInput {
            credential_env_key: Some(discovered.credential_env_key),
            extra_env: discovered.extra_env.clone(),
            wire_api: None,
            zcode_kind: None,
        },
        Tool::Codex => ProviderOptionsInput {
            credential_env_key: None,
            extra_env: BTreeMap::new(),
            wire_api: discovered.wire_api.clone(),
            zcode_kind: None,
        },
        Tool::Zcode => ProviderOptionsInput {
            credential_env_key: None,
            extra_env: BTreeMap::new(),
            wire_api: None,
            zcode_kind: discovered.zcode_kind.clone(),
        },
        Tool::Cursor => return Err(cursor_unsupported(ArtifactKind::Provider)),
    };
    StoredProviderConfig::from_input(
        tool,
        discovered.provider_id.as_deref().unwrap_or("discovered"),
        options,
        discovered.extra_provider_fields.clone(),
    )?;
    Ok(())
}

fn discover_native_provider(
    environment: &ExplicitEnvironment,
    tool: Tool,
) -> Result<Option<DiscoveredProvider>, AppError> {
    let mut descriptor = descriptor_for(environment, tool, ArtifactKind::Provider)?;
    refine_claude_provider_policy(&mut descriptor);
    ensure_tool_is_available(&descriptor)?;
    if descriptor.policy != crate::adapters::PolicyState::Allowed {
        return Err(AppError::policy_blocked(
            "claude",
            descriptor.path.as_deref().unwrap_or("<unsupported>"),
            match descriptor.policy {
                crate::adapters::PolicyState::Blocked => "provider_managed_by_host",
                crate::adapters::PolicyState::Unknown => "provider_policy_unknown",
                crate::adapters::PolicyState::Allowed => unreachable!("已在条件中排除"),
            },
        ));
    }
    let broad_ownership = match tool {
        Tool::Claude => ManagedOwnership::selectors([["env"]]),
        Tool::Codex => ManagedOwnership::selectors([
            vec!["model"],
            vec!["model_provider"],
            vec!["model_providers"],
        ]),
        Tool::Zcode => ManagedOwnership::selectors([["provider"]]),
        Tool::Cursor => return Err(cursor_unsupported(ArtifactKind::Provider)),
    };
    let scan = scan_target(tool_adapter(tool), &descriptor, &broad_ownership);
    let observed = match scan {
        TargetScan::Observed(observed) => observed,
        TargetScan::Missing => return Ok(None),
        TargetScan::ParseError => {
            return Err(AppError::parse(
                &descriptor_path(&descriptor)?,
                match tool {
                    Tool::Claude | Tool::Zcode => "json",
                    Tool::Codex => "toml",
                    Tool::Cursor => "json",
                },
            ));
        }
        _ => return Err(scan_error(&descriptor, &scan)),
    };
    match tool {
        Tool::Claude => discover_claude_provider(&descriptor, &observed),
        Tool::Codex => discover_codex_provider(&descriptor, &observed),
        Tool::Zcode => discover_zcode_provider(&descriptor, &observed),
        Tool::Cursor => Err(cursor_unsupported(ArtifactKind::Provider)),
    }
}

/// ZCode 的 `~/.zcode/v2/config.json` 可包含多个 provider 条目。导入面向单一
/// 配置：只有「恰好一个 enabled 条目」或「唯一条目且无 enabled 标记」时可发现，
/// 其余情况保持 fail closed。`models`、`source`、`systemDisabledReason` 是 ZCode
/// 自管字段，不进入受管选择器，也不参与写入。
fn discover_zcode_provider(
    descriptor: &TargetDescriptor,
    observed: &crate::sync::ObservedTarget,
) -> Result<Option<DiscoveredProvider>, AppError> {
    const ZCODE_PROVIDER_KINDS: &[&str] = &["anthropic", "openai", "gemini"];
    let entries = observed
        .managed_projection
        .get("provider")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::parse("~/.zcode/v2/config.json", "json"))?;
    let enabled: Vec<&String> = entries
        .iter()
        .filter(|(_, value)| value.get("enabled").and_then(Value::as_bool) == Some(true))
        .map(|(key, _)| key)
        .collect();
    let selected = match (enabled.len(), entries.len()) {
        (1, _) => (enabled[0], &entries[enabled[0]]),
        (0, 1) => entries
            .iter()
            .next()
            .ok_or_else(|| AppError::parse("~/.zcode/v2/config.json", "json"))?,
        _ => return Ok(None),
    };
    let (provider_id, entry) = selected;
    if entry
        .get("kind")
        .and_then(Value::as_str)
        .map_or(true, |kind| !ZCODE_PROVIDER_KINDS.contains(&kind))
    {
        return Ok(None);
    }
    let kind = entry
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("anthropic")
        .to_owned();
    let options = entry.get("options").and_then(Value::as_object);
    let api_base_url = options
        .and_then(|options| options.get("baseURL"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if api_base_url.is_empty() {
        return Ok(None);
    }
    let api_key = options
        .and_then(|options| options.get("apiKey"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let default_model = entry
        .get("models")
        .and_then(Value::as_object)
        .map(|models| {
            models
                .keys()
                .min_by(|a, b| a.cmp(b))
                .cloned()
                .unwrap_or_else(|| "unspecified".to_owned())
        })
        .unwrap_or_else(|| "unspecified".to_owned());
    // 导入基线只记录受管子键（name/kind/options/enabled），与同步阶段的
    // provider_ownership 粒度一致；models/source 等自管字段不进入基线。
    let mut managed_entry = Map::new();
    for leaf in ["name", "kind", "options", "enabled"] {
        if let Some(value) = entry.get(leaf) {
            managed_entry.insert(leaf.to_owned(), value.clone());
        }
    }
    Ok(Some(DiscoveredProvider {
        target_path: descriptor_path(descriptor)?,
        full_hash: observed.full_hash.clone(),
        projection: json!({ "provider": { provider_id: Value::Object(managed_entry) } }),
        api_base_url,
        api_key,
        default_model,
        credential_env_key: ClaudeCredentialEnvKey::ApiKey,
        extra_env: BTreeMap::new(),
        provider_id: Some(provider_id.clone()),
        wire_api: None,
        zcode_kind: Some(kind),
        extra_provider_fields: BTreeMap::new(),
        suggested_name: entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some(provider_id.clone())),
    }))
}

fn ensure_tool_is_available(descriptor: &TargetDescriptor) -> Result<(), AppError> {
    match descriptor.capability.state {
        crate::adapters::CapabilityState::Supported => Ok(()),
        crate::adapters::CapabilityState::ToolNotInstalled => Err(AppError::not_found(
            "toolInstallation",
            descriptor.tool.as_str(),
        )),
        crate::adapters::CapabilityState::Unsupported => Err(AppError::invalid_input(
            "capability",
            match descriptor.capability.diagnostic_code.as_deref() {
                Some("CURSOR_PROVIDER_UNSUPPORTED") => "CURSOR_PROVIDER_UNSUPPORTED",
                Some("CURSOR_PROMPT_UNSUPPORTED") => "CURSOR_PROMPT_UNSUPPORTED",
                _ => "工具安装探针未能安全确认版本",
            },
        )),
    }
}

fn discover_claude_provider(
    descriptor: &TargetDescriptor,
    observed: &crate::sync::ObservedTarget,
) -> Result<Option<DiscoveredProvider>, AppError> {
    let Some(env) = observed
        .managed_projection
        .get("env")
        .and_then(Value::as_object)
    else {
        return Ok(None);
    };
    let api_base_url = env
        .get(CLAUDE_BASE_URL_KEY)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let Some(default_model) = discover_claude_default_model(env) else {
        return Ok(None);
    };
    if api_base_url.is_empty() {
        return Ok(None);
    }
    let auth_token = env
        .get(ClaudeCredentialEnvKey::AuthToken.as_str())
        .and_then(Value::as_str);
    let api_key = env
        .get(ClaudeCredentialEnvKey::ApiKey.as_str())
        .and_then(Value::as_str);
    let credential_env_key = if auth_token.is_some() {
        ClaudeCredentialEnvKey::AuthToken
    } else {
        ClaudeCredentialEnvKey::ApiKey
    };
    let credential = auth_token.or(api_key).map(str::to_owned);
    let extra_env = env
        .iter()
        .filter(|(key, value)| {
            key.starts_with("ANTHROPIC_")
                && !matches!(
                    key.as_str(),
                    CLAUDE_BASE_URL_KEY
                        | CLAUDE_MODEL_KEY
                        | "ANTHROPIC_API_KEY"
                        | "ANTHROPIC_AUTH_TOKEN"
                )
                && value.is_string()
        })
        .map(|(key, value)| (key.clone(), value.as_str().unwrap_or_default().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let mut projection_env = Map::new();
    for key in [
        CLAUDE_BASE_URL_KEY,
        CLAUDE_MODEL_KEY,
        ClaudeCredentialEnvKey::ApiKey.as_str(),
        ClaudeCredentialEnvKey::AuthToken.as_str(),
    ] {
        if let Some(value) = env.get(key) {
            projection_env.insert(key.to_owned(), value.clone());
        }
    }
    for (key, value) in &extra_env {
        projection_env.insert(key.clone(), Value::String(value.clone()));
    }
    Ok(Some(DiscoveredProvider {
        target_path: descriptor_path(descriptor)?,
        full_hash: observed.full_hash.clone(),
        projection: json!({ "env": projection_env }),
        api_base_url,
        api_key: credential,
        default_model,
        credential_env_key,
        extra_env,
        provider_id: None,
        wire_api: None,
        zcode_kind: None,
        extra_provider_fields: BTreeMap::new(),
        suggested_name: None,
    }))
}

fn discover_claude_default_model(env: &Map<String, Value>) -> Option<String> {
    std::iter::once(CLAUDE_MODEL_KEY)
        .chain(CLAUDE_DEFAULT_MODEL_KEYS.iter().copied())
        .find_map(|key| {
            env.get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        })
}

fn discover_codex_provider(
    descriptor: &TargetDescriptor,
    observed: &crate::sync::ObservedTarget,
) -> Result<Option<DiscoveredProvider>, AppError> {
    let projection = &observed.managed_projection;
    let default_model = projection
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default()
        .to_owned();
    if default_model.is_empty() {
        return Ok(None);
    };
    let provider_id = match projection.get("model_provider").and_then(Value::as_str) {
        Some(value) if value.trim().is_empty() => return Ok(None),
        Some(value) => value,
        None => CODEX_OPENAI_PROVIDER_ID,
    };
    if provider_id == CODEX_OPENAI_PROVIDER_ID {
        return discover_codex_openai_provider(
            descriptor,
            projection,
            observed.full_hash.clone(),
            default_model,
        );
    }
    if CODEX_RESERVED_PROVIDER_IDS.contains(&provider_id) {
        return Ok(None);
    }
    validate_codex_custom_provider_id(Tool::Codex, provider_id)?;
    let Some(table) = projection
        .get("model_providers")
        .and_then(Value::as_object)
        .and_then(|providers| providers.get(provider_id))
        .and_then(Value::as_object)
    else {
        return Ok(None);
    };
    let api_base_url = table
        .get("base_url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if api_base_url.is_empty() {
        return Ok(None);
    }
    let api_key = table
        .get("experimental_bearer_token")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if !api_key
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(AppError::invalid_input(
            "experimentalBearerToken",
            "Codex 首次导入仅支持含直接 bearer token 的 Provider",
        ));
    }
    let wire_api = match table.get("wire_api") {
        None => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => {
            return Err(AppError::invalid_input(
                "wireApi",
                "Codex wire_api 必须是字符串",
            ));
        }
    };
    let suggested_name = match table.get("name") {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.clone()),
        _ => {
            return Err(AppError::invalid_input(
                "providerName",
                "Codex Provider name 必须是非空字符串",
            ));
        }
    };
    let managed_projection = Value::Object(Map::from_iter([
        ("model".to_owned(), Value::String(default_model.clone())),
        (
            "model_provider".to_owned(),
            Value::String(provider_id.to_owned()),
        ),
        (
            "model_providers".to_owned(),
            Value::Object(Map::from_iter([(
                provider_id.to_owned(),
                Value::Object(table.clone()),
            )])),
        ),
    ]));
    let extra_provider_fields = table
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "name" | "base_url" | "experimental_bearer_token" | "wire_api"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    Ok(Some(DiscoveredProvider {
        target_path: descriptor_path(descriptor)?,
        full_hash: observed.full_hash.clone(),
        projection: managed_projection,
        api_base_url,
        api_key,
        default_model,
        credential_env_key: ClaudeCredentialEnvKey::ApiKey,
        extra_env: BTreeMap::new(),
        provider_id: Some(provider_id.to_owned()),
        wire_api,
        zcode_kind: None,
        extra_provider_fields,
        suggested_name,
    }))
}

fn discover_codex_openai_provider(
    descriptor: &TargetDescriptor,
    projection: &Value,
    full_hash: String,
    default_model: String,
) -> Result<Option<DiscoveredProvider>, AppError> {
    if !codex_auth_json_has_oauth_tokens(&descriptor_path(descriptor)?) {
        return Ok(None);
    }
    let mut managed_projection = Map::new();
    managed_projection.insert("model".to_owned(), Value::String(default_model.clone()));
    if let Some(value) = projection.get("model_provider") {
        managed_projection.insert("model_provider".to_owned(), value.clone());
    }
    Ok(Some(DiscoveredProvider {
        target_path: descriptor_path(descriptor)?,
        full_hash,
        projection: Value::Object(managed_projection),
        api_base_url: CODEX_OPENAI_BASE_URL.to_owned(),
        api_key: None,
        default_model,
        credential_env_key: ClaudeCredentialEnvKey::ApiKey,
        extra_env: BTreeMap::new(),
        provider_id: Some(CODEX_OPENAI_PROVIDER_ID.to_owned()),
        wire_api: None,
        zcode_kind: None,
        extra_provider_fields: BTreeMap::new(),
        suggested_name: Some("Codex OAuth 登录".to_owned()),
    }))
}

fn codex_auth_json_has_oauth_tokens(config_path: &str) -> bool {
    let Some(codex_home) = Path::new(config_path).parent() else {
        return false;
    };
    let Ok(content) = fs::read_to_string(codex_home.join("auth.json")) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    root.get("tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| {
            ["access_token", "refresh_token", "id_token"]
                .iter()
                .any(|key| {
                    tokens
                        .get(*key)
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty())
                })
        })
}

fn provider_projection(profile: &ProviderProfileRecord) -> Result<Value, AppError> {
    let config = parse_stored_provider_config(profile)?;
    match profile.tool {
        Tool::Claude => {
            let mut env = Map::new();
            if let Some(value) = &profile.api_base_url {
                env.insert(CLAUDE_BASE_URL_KEY.to_owned(), Value::String(value.clone()));
            }
            if let (Some(key), Some(value)) = (config.credential_env_key, &profile.api_key) {
                env.insert(key.as_str().to_owned(), Value::String(value.clone()));
            }
            if let Some(value) = &profile.default_model {
                env.insert(CLAUDE_MODEL_KEY.to_owned(), Value::String(value.clone()));
            }
            env.extend(
                config
                    .extra_env
                    .into_iter()
                    .map(|(key, value)| (key, Value::String(value))),
            );
            Ok(json!({ "env": env }))
        }
        Tool::Codex => {
            let provider_id = config.provider_id.ok_or_else(|| {
                AppError::invalid_input("providerOptions", "Codex Provider 缺少稳定 provider id")
            })?;
            validate_codex_provider_id(Tool::Codex, &provider_id)?;
            if provider_id == CODEX_OPENAI_PROVIDER_ID {
                let mut root = Map::new();
                if let Some(model) = &profile.default_model {
                    root.insert("model".to_owned(), Value::String(model.clone()));
                }
                root.insert(
                    "model_provider".to_owned(),
                    Value::String(CODEX_OPENAI_PROVIDER_ID.to_owned()),
                );
                return Ok(Value::Object(root));
            }
            let mut provider = config
                .extra_provider_fields
                .into_iter()
                .collect::<Map<_, _>>();
            provider.insert("name".to_owned(), Value::String(profile.name.clone()));
            if let Some(value) = &profile.api_base_url {
                provider.insert("base_url".to_owned(), Value::String(value.clone()));
            }
            if let Some(value) = &profile.api_key {
                provider.insert(
                    "experimental_bearer_token".to_owned(),
                    Value::String(value.clone()),
                );
            }
            if let Some(value) = config.wire_api {
                provider.insert("wire_api".to_owned(), Value::String(value));
            }
            let mut root = Map::new();
            if let Some(model) = &profile.default_model {
                root.insert("model".to_owned(), Value::String(model.clone()));
            }
            root.insert(
                "model_provider".to_owned(),
                Value::String(provider_id.clone()),
            );
            root.insert(
                "model_providers".to_owned(),
                Value::Object(Map::from_iter([(provider_id, Value::Object(provider))])),
            );
            Ok(Value::Object(root))
        }
        // 只写有证据的 name/kind/options/enabled；`models`、`source`、
        // `systemDisabledReason` 等 ZCode 自管字段通过子选择器保留在原地。
        Tool::Zcode => {
            let provider_id = config.provider_id.ok_or_else(|| {
                AppError::invalid_input("providerOptions", "ZCode Provider 缺少稳定 provider id")
            })?;
            let mut options = Map::new();
            if let Some(value) = &profile.api_base_url {
                options.insert("baseURL".to_owned(), Value::String(value.clone()));
            }
            if let Some(value) = &profile.api_key {
                options.insert("apiKey".to_owned(), Value::String(value.clone()));
            }
            let entry = json!({
                "name": profile.name,
                "kind": config.zcode_kind.unwrap_or_else(|| "anthropic".to_owned()),
                "options": Value::Object(options),
                "enabled": true,
            });
            Ok(json!({ "provider": { provider_id: entry } }))
        }
        Tool::Cursor => Err(cursor_unsupported(ArtifactKind::Provider)),
    }
}

fn provider_ownership(
    tool: Tool,
    baseline: Option<&Value>,
    desired: &Value,
) -> Result<ManagedOwnership, AppError> {
    let mut selectors = BTreeSet::<Vec<String>>::new();
    match tool {
        Tool::Claude => {
            for projection in [baseline, Some(desired)].into_iter().flatten() {
                if let Some(env) = projection.get("env").and_then(Value::as_object) {
                    selectors.extend(env.keys().map(|key| vec!["env".to_owned(), key.clone()]));
                }
            }
        }
        Tool::Codex => {
            for projection in [baseline, Some(desired)].into_iter().flatten() {
                if projection.get("model").is_some() {
                    selectors.insert(vec!["model".to_owned()]);
                }
                if projection.get("model_provider").is_some() {
                    selectors.insert(vec!["model_provider".to_owned()]);
                }
                if let Some(providers) =
                    projection.get("model_providers").and_then(Value::as_object)
                {
                    selectors.extend(
                        providers
                            .keys()
                            .map(|key| vec!["model_providers".to_owned(), key.clone()]),
                    );
                }
            }
        }
        Tool::Zcode => {
            for projection in [baseline, Some(desired)].into_iter().flatten() {
                let Some(providers) = projection.get("provider").and_then(Value::as_object) else {
                    continue;
                };
                for provider_id in providers.keys() {
                    for leaf in ["name", "kind", "options", "enabled"] {
                        selectors.insert(vec![
                            "provider".to_owned(),
                            provider_id.clone(),
                            leaf.to_owned(),
                        ]);
                    }
                }
            }
        }
        Tool::Cursor => return Err(cursor_unsupported(ArtifactKind::Provider)),
    }
    if selectors.is_empty() {
        return Err(AppError::invalid_input(
            "managedOwnership",
            "Provider 同步没有可证明拥有的字段",
        ));
    }
    Ok(ManagedOwnership::Selectors(selectors.into_iter().collect()))
}

fn descriptor_for(
    environment: &ExplicitEnvironment,
    tool: Tool,
    artifact_kind: ArtifactKind,
) -> Result<TargetDescriptor, AppError> {
    descriptor_for_scope(environment, tool, artifact_kind, None)
}

fn descriptor_for_scope(
    environment: &ExplicitEnvironment,
    tool: Tool,
    artifact_kind: ArtifactKind,
    project_root: Option<&ProjectRoot>,
) -> Result<TargetDescriptor, AppError> {
    let adapter = tool_adapter(tool);
    let context = DiscoveryContext {
        environment,
        project_root,
        claude_user_mcp_probe: environment.claude_user_mcp_probe(),
        claude_customization_policy_probe: environment.claude_customization_policy_probe(),
    };
    adapter
        .discover(&context)?
        .into_iter()
        .find(|target| {
            target.artifact_kind == artifact_kind
                && match (target.scope, project_root) {
                    (Scope::Global, None) => true,
                    (Scope::Project, Some(root)) => {
                        target.project_root.as_deref() == Some(root.as_str())
                    }
                    _ => false,
                }
        })
        .ok_or_else(|| AppError::not_found("targetDescriptor", artifact_kind.as_str()))
}

/// 显式运行时证据决定默认状态；原生 settings 中的宿主标记只能把状态收紧。
fn refine_claude_provider_policy(descriptor: &mut TargetDescriptor) {
    if descriptor.tool != Tool::Claude
        || descriptor.artifact_kind != ArtifactKind::Provider
        || descriptor.policy == PolicyState::Blocked
    {
        return;
    }
    let marker_ownership =
        ManagedOwnership::selectors([["env", CLAUDE_PROVIDER_MANAGED_BY_HOST_KEY]]);
    match scan_target(&ClaudeAdapter, descriptor, &marker_ownership) {
        TargetScan::Observed(observed) => {
            let marker = observed
                .managed_projection
                .get("env")
                .and_then(Value::as_object)
                .and_then(|env| env.get(CLAUDE_PROVIDER_MANAGED_BY_HOST_KEY));
            match marker {
                Some(Value::String(value)) if !value.is_empty() => {
                    descriptor.policy = PolicyState::Blocked;
                }
                Some(Value::String(_)) | None => {}
                Some(_) => descriptor.policy = PolicyState::Unknown,
            }
        }
        TargetScan::Missing => {}
        TargetScan::ParseError
        | TargetScan::ManagedItemBaselineMismatch
        | TargetScan::PermissionDenied
        | TargetScan::Unavailable
        | TargetScan::TargetTypeChanged(_)
        | TargetScan::Failed => descriptor.policy = PolicyState::Unknown,
    }
}

fn tool_adapter(tool: Tool) -> &'static dyn ToolAdapter {
    static CLAUDE: ClaudeAdapter = ClaudeAdapter;
    static CODEX: CodexAdapter = CodexAdapter;
    static CURSOR: CursorAdapter = CursorAdapter;
    static ZCODE: ZcodeAdapter = ZcodeAdapter;
    match tool {
        Tool::Claude => &CLAUDE,
        Tool::Codex => &CODEX,
        Tool::Cursor => &CURSOR,
        Tool::Zcode => &ZCODE,
    }
}

fn allowed_root(environment: &ExplicitEnvironment, tool: Tool) -> PathBuf {
    match tool {
        Tool::Claude => environment.claude_config_dir().to_owned(),
        Tool::Codex => environment.codex_home().to_owned(),
        Tool::Cursor => environment.home().join(".cursor"),
        Tool::Zcode => environment.home().join(".zcode"),
    }
}

fn ensure_profile_capability(tool: Tool, artifact_kind: ArtifactKind) -> Result<(), AppError> {
    if tool == Tool::Cursor
        && matches!(artifact_kind, ArtifactKind::Provider | ArtifactKind::Prompt)
    {
        return Err(cursor_unsupported(artifact_kind));
    }
    Ok(())
}

fn cursor_unsupported(artifact_kind: ArtifactKind) -> AppError {
    AppError::invalid_input(
        "capability",
        match artifact_kind {
            ArtifactKind::Provider => "CURSOR_PROVIDER_UNSUPPORTED",
            ArtifactKind::Prompt => "CURSOR_PROMPT_UNSUPPORTED",
            ArtifactKind::Mcp | ArtifactKind::Skill => "Cursor 仅在 MCP/Skills 中受支持",
        },
    )
}

fn descriptor_path(descriptor: &TargetDescriptor) -> Result<String, AppError> {
    descriptor
        .path
        .clone()
        .ok_or_else(|| AppError::invalid_input("targetPath", "目标路径不可用"))
}

fn scan_error(descriptor: &TargetDescriptor, scan: &TargetScan) -> AppError {
    let path = descriptor.path.as_deref().unwrap_or("<unsupported>");
    match scan {
        TargetScan::PermissionDenied => AppError::permission(path, "scan_profile_target"),
        TargetScan::ParseError => AppError::parse(path, descriptor.format.as_str()),
        TargetScan::Unavailable => AppError::not_found("target", path),
        TargetScan::TargetTypeChanged(_) => {
            AppError::conflict("targetPath", "目标类型与配置格式不一致")
        }
        TargetScan::Failed | TargetScan::ManagedItemBaselineMismatch => {
            AppError::new(ErrorCode::Conflict, "目标无法安全读取", true)
        }
        TargetScan::Missing | TargetScan::Observed(_) => {
            AppError::new(ErrorCode::Conflict, "目标状态不符合导入条件", true)
        }
    }
}

fn provider_dto(record: &ProviderProfileRecord) -> Result<ProviderProfileDto, AppError> {
    let config = parse_stored_provider_config(record)?;
    Ok(ProviderProfileDto {
        id: record.id.clone(),
        tool: record.tool,
        name: record.name.clone(),
        api_base_url: record.api_base_url.clone().unwrap_or_default(),
        api_key_configured: record
            .api_key
            .as_ref()
            .is_some_and(|value| !value.is_empty()),
        default_model: record.default_model.clone().unwrap_or_default(),
        options: config.options_dto(),
        is_active: record.is_active,
        row_version: safe_row_version(record.row_version)?,
    })
}

fn prompt_dto(record: &PromptProfileRecord) -> Result<PromptProfileDto, AppError> {
    let mut global_tools = Vec::new();
    if record.is_active_claude {
        global_tools.push(Tool::Claude);
    }
    if record.is_active_codex {
        global_tools.push(Tool::Codex);
    }
    if record.is_active_zcode {
        global_tools.push(Tool::Zcode);
    }
    Ok(PromptProfileDto {
        id: record.id.clone(),
        name: record.name.clone(),
        body: record.body.clone(),
        global_tools,
        imported_from_path: record.imported_from_path.clone(),
        row_version: safe_row_version(record.row_version)?,
    })
}

fn parse_stored_provider_config(
    record: &ProviderProfileRecord,
) -> Result<StoredProviderConfig, AppError> {
    serde_json::from_str(&record.config_json)
        .map_err(|_| AppError::invalid_input("providerOptions", "Provider 中央配置已损坏"))
}

fn provider_row_version(profile: &ProviderProfileRecord) -> Result<DatabaseRowVersion, AppError> {
    Ok(DatabaseRowVersion {
        entity_type: DatabaseEntityType::ProviderProfile,
        entity_id: profile.id.clone(),
        row_version: safe_row_version(profile.row_version)?,
    })
}

fn prompt_row_version(profile: &PromptProfileRecord) -> Result<DatabaseRowVersion, AppError> {
    Ok(DatabaseRowVersion {
        entity_type: DatabaseEntityType::PromptProfile,
        entity_id: profile.id.clone(),
        row_version: safe_row_version(profile.row_version)?,
    })
}

fn safe_row_version(value: i64) -> Result<u32, AppError> {
    u32::try_from(value)
        .map_err(|_| AppError::invalid_input("rowVersion", "档案 row_version 超出安全范围"))
}

fn generated_codex_provider_id(id: &str) -> String {
    format!("easytoagents_{}", id.replace('-', ""))
}

fn discovered_provider_allows_missing_api_key(tool: Tool, discovered: &DiscoveredProvider) -> bool {
    codex_provider_allows_missing_api_key(tool, discovered.provider_id.as_deref())
}

fn codex_provider_allows_missing_api_key(tool: Tool, provider_id: Option<&str>) -> bool {
    tool == Tool::Codex && provider_id == Some(CODEX_OPENAI_PROVIDER_ID)
}

fn validate_codex_provider_id(tool: Tool, provider_id: &str) -> Result<(), AppError> {
    if tool != Tool::Codex {
        return Ok(());
    }
    if provider_id == CODEX_OPENAI_PROVIDER_ID {
        return Ok(());
    }
    validate_codex_custom_provider_id(tool, provider_id)
}

fn validate_codex_custom_provider_id(tool: Tool, provider_id: &str) -> Result<(), AppError> {
    if tool != Tool::Codex {
        return Ok(());
    }
    if provider_id.is_empty()
        || provider_id.len() > 100
        || CODEX_RESERVED_PROVIDER_IDS.contains(&provider_id)
        || !provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(AppError::invalid_input(
            "providerId",
            "Codex provider id 非法或属于不支持接管的内置项",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Mutex};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        apply_profile_preview, confirm_prompt_import, confirm_provider_import,
        copy_provider_profile, create_prompt_profile, create_provider_profile,
        delete_prompt_profile, discover_prompt_import, discover_provider_import,
        get_tool_profile_status, list_provider_profiles, preview_prompt_sync,
        preview_provider_sync, set_active_provider_profile, set_global_prompt_assignment,
        update_prompt_profile, update_provider_profile, CopyProviderProfileInput, PromptProfileDto,
        PromptProfileInput, ProviderOptionsInput, ProviderProfileInput,
        SetGlobalPromptAssignmentInput, UpdatePromptProfileInput, UpdateProviderProfileInput,
        CLAUDE_MODEL_KEY,
    };
    use crate::{
        adapters::{
            CapabilityState, ExplicitEnvironment, PolicyState, ToolAvailability,
            ToolAvailabilityState,
        },
        app::AppPaths,
        db::Database,
        domain::{ArtifactKind, Tool},
        profiles::{ConfirmImportInput, SecretUpdate},
        security::SecretRedactor,
    };

    struct Fixture {
        _temporary: tempfile::TempDir,
        home: std::path::PathBuf,
        paths: AppPaths,
        database: Database,
        environment: ExplicitEnvironment,
    }

    fn fixture() -> Fixture {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        fs::create_dir(home.join(".claude")).unwrap();
        fs::create_dir(home.join(".codex")).unwrap();
        fs::create_dir_all(home.join(".zcode/v2")).unwrap();
        let paths = AppPaths::from_data_root(home.join("private/app/data")).unwrap();
        let database = Database::open(&paths).unwrap();
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed())
                .unwrap()
                .with_claude_provider_policy(PolicyState::Allowed);
        Fixture {
            _temporary: temporary,
            home,
            paths,
            database,
            environment,
        }
    }

    /// 新建档案并立即对指定工具启用（替代旧 activate 入参的测试夹具）。
    fn create_enabled_prompt(
        fixture: &mut Fixture,
        tool: Tool,
        name: &str,
        body: &str,
    ) -> PromptProfileDto {
        let profile = create_prompt_profile(
            &mut fixture.database,
            PromptProfileInput {
                name: name.to_owned(),
                body: body.to_owned(),
            },
        )
        .unwrap();
        set_global_prompt_assignment(
            &mut fixture.database,
            &SetGlobalPromptAssignmentInput {
                tool,
                prompt_profile_id: profile.id.clone(),
                assigned: true,
                row_version: profile.row_version,
            },
        )
        .unwrap()
    }

    fn provider(tool: Tool, name: &str, key: &str, activate: bool) -> ProviderProfileInput {
        ProviderProfileInput {
            tool,
            name: name.to_owned(),
            api_base_url: "https://provider.example.com/v1".to_owned(),
            api_key: key.to_owned(),
            default_model: "fixture-model".to_owned(),
            options: ProviderOptionsInput::default(),
            activate,
        }
    }

    #[test]
    fn provider_copy_is_independent_and_revalidated_for_target_tool() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let source = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Claude, "Claude 主渠道", "fixture-copy-secret", true),
        )
        .unwrap();
        let copied = copy_provider_profile(
            &mut fixture.database,
            &mut redactor,
            CopyProviderProfileInput {
                source_id: source.id.clone(),
                target_tool: Tool::Codex,
                target_name: "Codex 复制渠道".to_owned(),
                activate: true,
            },
        )
        .unwrap();
        assert_ne!(source.id, copied.id);
        assert_eq!(copied.tool, Tool::Codex);
        assert!(copied
            .options
            .provider_id
            .as_deref()
            .is_some_and(|id| id.starts_with("easytoagents_")));
        assert_eq!(copied.options.credential_env_key, None);
    }

    #[test]
    fn zcode_provider_import_and_sync_preserve_app_owned_fields() {
        let mut fixture = fixture();
        let config = fixture.home.join(".zcode/v2/config.json");
        fs::write(
            &config,
            r#"{
  "provider": {
    "builtin:fixture": {
      "name": "Fixture Plan",
      "kind": "anthropic",
      "options": {
        "apiKey": "fixture-native-secret",
        "baseURL": "https://fixture.invalid/v1"
      },
      "enabled": true,
      "source": "custom",
      "models": {
        "GLM-5.3": {"zcode": {"priority": 1}}
      }
    }
  },
  "unrelated": {"keep": true}
}
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview_dto = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Zcode,
        )
        .unwrap()
        .expect("fixture 配置应包含一个可导入的 provider");
        assert_eq!(preview_dto.suggested_name, "Fixture Plan");
        assert_eq!(preview_dto.api_base_url, "https://fixture.invalid/v1");
        assert!(preview_dto.api_key_configured);
        let serialized = serde_json::to_string(&preview_dto).unwrap();
        assert!(!serialized.contains("fixture-native-secret"));

        let created = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            crate::profiles::ConfirmImportInput {
                preview_id: preview_dto.preview_id,
                name: "Fixture Plan".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(
            created.options.provider_id.as_deref(),
            Some("builtin:fixture")
        );
        assert_eq!(created.options.zcode_kind.as_deref(), Some("anthropic"));

        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Zcode,
        )
        .unwrap();
        assert!(!serde_json::to_string(&sync_preview)
            .unwrap()
            .contains("fixture-native-secret"));
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &sync_preview.preview_id,
            Tool::Zcode,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();

        let document: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        let entry = &document["provider"]["builtin:fixture"];
        assert_eq!(entry["name"], "Fixture Plan");
        assert_eq!(entry["kind"], "anthropic");
        assert_eq!(entry["options"]["apiKey"], "fixture-native-secret");
        assert_eq!(entry["options"]["baseURL"], "https://fixture.invalid/v1");
        assert_eq!(entry["enabled"], true);
        // ZCode 自管字段与无关键必须原样保留。
        assert_eq!(entry["source"], "custom");
        assert_eq!(entry["models"]["GLM-5.3"]["zcode"]["priority"], 1);
        assert_eq!(document["unrelated"]["keep"], true);

        // 切换到新档案后，旧条目的受管子键被移除，models/source 保留。
        let second = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                options: ProviderOptionsInput {
                    zcode_kind: Some("openai".to_owned()),
                    ..ProviderOptionsInput::default()
                },
                ..provider(Tool::Zcode, "第二渠道", "fixture-second-secret", true)
            },
        )
        .unwrap();
        let second_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Zcode,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &second_preview.preview_id,
            Tool::Zcode,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();
        let document: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert!(document["provider"]["builtin:fixture"]
            .get("name")
            .is_none());
        assert_eq!(
            document["provider"]["builtin:fixture"]["models"]["GLM-5.3"]["zcode"]["priority"],
            1
        );
        let new_entry = &document["provider"][second.options.provider_id.unwrap().as_str()];
        assert_eq!(new_entry["name"], "第二渠道");
        assert_eq!(new_entry["kind"], "openai");
        assert_eq!(new_entry["options"]["apiKey"], "fixture-second-secret");
        assert_eq!(document["unrelated"]["keep"], true);
    }

    #[test]
    fn codex_provider_rename_preserves_stable_provider_id_and_delete_removes_profile() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let created = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Codex, "重命名前", "fixture-stable-secret", true),
        )
        .unwrap();
        let provider_id = created.options.provider_id.clone();

        let updated = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: created.id.clone(),
                name: "重命名后".to_owned(),
                api_base_url: created.api_base_url.clone(),
                api_key: SecretUpdate::Keep,
                default_model: "fixture-updated-model".to_owned(),
                options: ProviderOptionsInput {
                    wire_api: Some("responses".to_owned()),
                    ..ProviderOptionsInput::default()
                },
                row_version: created.row_version,
            },
        )
        .unwrap();
        assert_eq!(updated.name, "重命名后");
        assert_eq!(updated.default_model, "fixture-updated-model");
        assert_eq!(updated.options.provider_id, provider_id);

        super::delete_provider_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: created.id.clone(),
                row_version: updated.row_version,
            },
        )
        .unwrap();
        assert!(list_provider_profiles(&fixture.database, Tool::Codex)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn claude_provider_switch_cleans_old_owned_keys_and_preserves_other_settings() {
        let mut fixture = fixture();
        let settings = fixture.home.join(".claude/settings.json");
        fs::write(
            &settings,
            r#"{
  "env": {"UNRELATED_ENV": "keep"},
  "permissions": {"allow": ["Read"]},
  "plugins": {"fixture": true}
}
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let first = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            ProviderProfileInput {
                options: ProviderOptionsInput {
                    credential_env_key: Some(crate::profiles::ClaudeCredentialEnvKey::ApiKey),
                    extra_env: [(
                        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
                        "old-opus".to_owned(),
                    )]
                    .into_iter()
                    .collect(),
                    wire_api: None,
                    zcode_kind: None,
                },
                ..provider(Tool::Claude, "第一档", "fixture-first-secret", true)
            },
        )
        .unwrap();
        let first_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(!serde_json::to_string(&first_preview)
            .unwrap()
            .contains("fixture-first-secret"));
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &first_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();

        let second = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Claude, "第二档", "fixture-second-secret", false),
        )
        .unwrap();
        set_active_provider_profile(
            &mut fixture.database,
            Tool::Claude,
            &crate::profiles::VersionedProfileInput {
                id: second.id.clone(),
                row_version: second.row_version,
            },
        )
        .unwrap();
        let second_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &second_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();
        let written: Value = serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
        assert_eq!(written["env"]["UNRELATED_ENV"], "keep");
        assert!(written["env"].get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
        assert_eq!(written["permissions"]["allow"][0], "Read");
        assert_eq!(written["plugins"]["fixture"], true);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn prompt_import_is_lossless_and_requires_confirmed_persisted_preview() {
        let mut fixture = fixture();
        let prompt_path = fixture.home.join(".codex/AGENTS.md");
        let original = "# 原有指令\n\n保留末尾空格  \n";
        fs::write(&prompt_path, original).unwrap();
        fs::write(
            fixture.home.join(".codex/AGENTS.override.md"),
            "# 覆盖指令\n",
        )
        .unwrap();
        let preview =
            discover_prompt_import(&mut fixture.database, &fixture.environment, Tool::Codex)
                .unwrap()
                .unwrap();
        assert_eq!(preview.body, original);
        assert_eq!(fs::read_to_string(&prompt_path).unwrap(), original);
        let imported = confirm_prompt_import(
            &mut fixture.database,
            &fixture.environment,
            ConfirmImportInput {
                preview_id: preview.preview_id.clone(),
                name: "原有提示词".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(imported.body, original);
        assert_eq!(fs::read_to_string(&prompt_path).unwrap(), original);
        let repeated = confirm_prompt_import(
            &mut fixture.database,
            &fixture.environment,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "重复".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(
            repeated.code(),
            crate::error::ErrorCode::PreviewAlreadyConsumed
        );
    }

    #[test]
    fn provider_import_preview_is_persisted_redacted_and_adopts_without_writing() {
        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        let secret = "fixture-import-provider-secret";
        let original = format!(
            r#"{{
  "env": {{
    "ANTHROPIC_BASE_URL": "https://import.example.com/v1",
    "ANTHROPIC_API_KEY": "{secret}",
    "ANTHROPIC_MODEL": "claude-imported",
    "UNRELATED_ENV": "keep"
  }},
  "permissions": {{"allow": ["Read"]}}
}}
"#,
        );
        fs::write(&settings_path, &original).unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap()
        .unwrap();
        assert!(preview.api_key_configured);
        assert!(!serde_json::to_string(&preview).unwrap().contains(secret));
        let persisted: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_preview_json FROM profile_import_previews WHERE id = ?1",
                [&preview.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!persisted.contains(secret));
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);

        let externally_changed = original.replace(
            r#""permissions": {"allow": ["Read"]}"#,
            r#""permissions": {"allow": ["Read", "Glob"]}"#,
        );
        fs::write(&settings_path, &externally_changed).unwrap();
        let stale = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "过期导入".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(stale.code(), crate::error::ErrorCode::StalePreview);
        assert!(list_provider_profiles(&fixture.database, Tool::Claude)
            .unwrap()
            .is_empty());
        assert_eq!(
            fs::read_to_string(&settings_path).unwrap(),
            externally_changed
        );

        fs::write(&settings_path, &original).unwrap();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap()
        .unwrap();

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "导入渠道".to_owned(),
            },
        )
        .unwrap();
        assert!(imported.api_key_configured);
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert!(matches!(
            sync_preview.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged | crate::domain::ChangeKind::Warning
        ));
    }

    #[test]
    fn claude_provider_import_accepts_default_model_family_without_anthropic_model() {
        let mut fixture = fixture();
        let settings_path = fixture.home.join(".claude/settings.json");
        let secret = "fixture-default-model-secret";
        let original = format!(
            r#"{{
  "env": {{
    "ANTHROPIC_BASE_URL": "https://default-family.example.com/v1",
    "ANTHROPIC_AUTH_TOKEN": "{secret}",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-bg",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus-plan",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-main",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "主 Sonnet",
    "UNRELATED_ENV": "keep"
  }}
}}
"#,
        );
        fs::write(&settings_path, &original).unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
        )
        .unwrap()
        .unwrap();
        assert!(preview.api_key_configured);
        assert_eq!(preview.default_model, "claude-sonnet-main");
        assert!(!serde_json::to_string(&preview).unwrap().contains(secret));
        assert!(preview.redacted_projection["env"]
            .get(CLAUDE_MODEL_KEY)
            .is_none());
        assert_eq!(
            preview.redacted_projection["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            crate::security::REDACTED
        );
        assert_eq!(
            preview.redacted_projection["env"]["ANTHROPIC_AUTH_TOKEN"],
            crate::security::REDACTED
        );

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "导入默认模型族".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(imported.default_model, "claude-sonnet-main");
        assert_eq!(
            imported.options.credential_env_key,
            Some(crate::profiles::ClaudeCredentialEnvKey::AuthToken)
        );
        assert_eq!(
            imported.options.extra_env["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            "claude-sonnet-main"
        );
        assert_eq!(
            imported.options.extra_env["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            "claude-opus-plan"
        );
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
    }

    #[test]
    fn codex_provider_preserves_other_tables_cleans_old_table_and_never_leaks_tokens() {
        let mut fixture = fixture();
        let config_path = fixture.home.join(".codex/config.toml");
        fs::write(
            &config_path,
            r#"# keep this comment
[mcp_servers.fixture]
command = "keep"

[plugins]
fixture = true

[model_providers.external]
name = "External"
base_url = "https://external.example.com/v1"
"#,
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let first_secret = "fixture-codex-secret-first";
        let first = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Codex, "Codex 第一档", first_secret, true),
        )
        .unwrap();
        let first_provider_id = first.options.provider_id.clone().unwrap();
        let first_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        assert!(!serde_json::to_string(&first_preview)
            .unwrap()
            .contains(first_secret));
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &first_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();
        let journal = fs::read_to_string(
            fixture
                .paths
                .journals()
                .join(format!("{}.json", first_preview.preview_id)),
        )
        .unwrap();
        assert!(!journal.contains(first_secret));
        let preview_row: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&first_preview.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!preview_row.contains(first_secret));

        let second_secret = "fixture-codex-secret-second";
        let second = create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(Tool::Codex, "Codex 第二档", second_secret, false),
        )
        .unwrap();
        let second_provider_id = second.options.provider_id.clone().unwrap();
        let activated_second = set_active_provider_profile(
            &mut fixture.database,
            Tool::Codex,
            &crate::profiles::VersionedProfileInput {
                id: second.id.clone(),
                row_version: second.row_version,
            },
        )
        .unwrap();
        let second_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &second_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();

        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("# keep this comment"));
        let parsed: Value = toml_edit::de::from_str(&text).unwrap();
        assert_eq!(parsed["mcp_servers"]["fixture"]["command"], "keep");
        assert_eq!(parsed["plugins"]["fixture"], true);
        assert!(parsed["model_providers"].get("external").is_some());
        assert!(parsed["model_providers"].get(&first_provider_id).is_none());
        assert_eq!(
            parsed["model_providers"][&second_provider_id]["experimental_bearer_token"],
            second_secret
        );
        assert_eq!(parsed["model_provider"], second_provider_id);

        super::delete_provider_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: activated_second.id,
                row_version: activated_second.row_version,
            },
        )
        .unwrap();
        let cleanup_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &cleanup_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();
        let cleaned: Value =
            toml_edit::de::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        assert!(cleaned.get("model").is_none());
        assert!(cleaned.get("model_provider").is_none());
        assert!(cleaned["model_providers"]
            .get(&second_provider_id)
            .is_none());
        assert!(cleaned["model_providers"].get("external").is_some());
        assert_eq!(cleaned["mcp_servers"]["fixture"]["command"], "keep");
    }

    #[test]
    fn codex_status_reports_override_and_new_session_notice() {
        let fixture = fixture();
        fs::write(
            fixture.home.join(".codex/AGENTS.override.md"),
            "# 覆盖指令\n",
        )
        .unwrap();
        let status = get_tool_profile_status(&fixture.environment, Tool::Codex).unwrap();
        assert_eq!(
            status.prompt_override,
            crate::adapters::PromptOverrideState::Present
        );
        assert!(status.new_session_notice.contains("新"));
        assert!(status.bearer_token_warning.is_some());
        assert!(list_provider_profiles(&fixture.database, Tool::Codex)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cursor_profile_and_prompt_capabilities_fail_closed_before_storage() {
        let mut fixture = fixture();
        let status = get_tool_profile_status(&fixture.environment, Tool::Cursor).unwrap();
        assert_eq!(
            status.provider_capability.state,
            CapabilityState::Unsupported
        );
        assert_eq!(status.prompt_capability.state, CapabilityState::Unsupported);
        assert_eq!(
            status.provider_capability.diagnostic_code.as_deref(),
            Some("CURSOR_PROVIDER_UNSUPPORTED")
        );
        assert_eq!(
            status.prompt_capability.diagnostic_code.as_deref(),
            Some("CURSOR_PROMPT_UNSUPPORTED")
        );
        assert!(status.provider_target_path.is_none());
        assert!(status.prompt_target_path.is_none());

        assert_eq!(
            list_provider_profiles(&fixture.database, Tool::Cursor)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            discover_provider_import(
                &mut fixture.database,
                &fixture.environment,
                &SecretRedactor::default(),
                Tool::Cursor,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            discover_prompt_import(&mut fixture.database, &fixture.environment, Tool::Cursor)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            preview_provider_sync(
                &mut fixture.database,
                &fixture.environment,
                &mut SecretRedactor::default(),
                Tool::Cursor,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert_eq!(
            fixture
                .database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM managed_targets WHERE tool = 'cursor'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn tool_status_serializes_release_availability_and_imports_fail_before_native_reads() {
        let mut fixture = fixture();
        fs::write(
            fixture.home.join(".claude/settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://should-not-import.example","ANTHROPIC_MODEL":"blocked","ANTHROPIC_API_KEY":"fixture-secret"}}"#,
        )
        .unwrap();
        fs::write(fixture.home.join(".codex/AGENTS.md"), "# 不应读取的提示词").unwrap();
        let environment = ExplicitEnvironment::new(
            &fixture.home,
            None,
            None,
            ToolAvailability {
                claude: ToolAvailabilityState::Unavailable,
                codex: ToolAvailabilityState::Unsupported,
                cursor: ToolAvailabilityState::Unavailable,
                zcode: ToolAvailabilityState::Unavailable,
            },
        )
        .unwrap()
        .with_claude_provider_policy(PolicyState::Allowed);

        let claude = get_tool_profile_status(&environment, Tool::Claude).unwrap();
        let codex = get_tool_profile_status(&environment, Tool::Codex).unwrap();
        assert_eq!(claude.availability, ToolAvailabilityState::Unavailable);
        assert_eq!(codex.availability, ToolAvailabilityState::Unsupported);
        assert_eq!(claude.installation_version, None);
        assert_eq!(codex.installation_version, None);
        assert!(serde_json::to_string(&claude)
            .unwrap()
            .contains("\"availability\":\"unavailable\""));
        assert_eq!(
            discover_provider_import(
                &mut fixture.database,
                &environment,
                &SecretRedactor::default(),
                Tool::Claude,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::NotFound
        );
        assert_eq!(
            discover_prompt_import(&mut fixture.database, &environment, Tool::Codex)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn claude_provider_unknown_or_host_managed_policy_blocks_preview() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        create_provider_profile(
            &mut fixture.database,
            &mut redactor,
            provider(
                Tool::Claude,
                "受策略保护渠道",
                "fixture-policy-secret",
                true,
            ),
        )
        .unwrap();
        let unknown_environment =
            ExplicitEnvironment::new(&fixture.home, None, None, ToolAvailability::all_installed())
                .unwrap();
        let unknown = preview_provider_sync(
            &mut fixture.database,
            &unknown_environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            unknown.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            unknown.targets[0].error_code,
            Some(crate::error::ErrorCode::PolicyBlocked)
        );
        assert!(!fixture.home.join(".claude/settings.json").exists());

        fs::write(
            fixture.home.join(".claude/settings.json"),
            r#"{"env":{"CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST":"1","UNRELATED":"keep"}}"#,
        )
        .unwrap();
        let blocked = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            blocked.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            get_tool_profile_status(&fixture.environment, Tool::Claude)
                .unwrap()
                .provider_policy,
            PolicyState::Blocked
        );

        fs::write(
            fixture.home.join(".claude/settings.json"),
            "{ invalid settings",
        )
        .unwrap();
        let malformed = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Claude,
        )
        .unwrap();
        assert_eq!(
            malformed.targets[0].status,
            crate::domain::SyncStatus::PolicyBlocked
        );
        assert_eq!(
            get_tool_profile_status(&fixture.environment, Tool::Claude)
                .unwrap()
                .provider_policy,
            PolicyState::Unknown
        );
    }

    #[test]
    fn codex_import_preserves_supported_provider_fields_and_redacts_all_secrets() {
        let mut fixture = fixture();
        let config_path = fixture.home.join(".codex/config.toml");
        let token = "fixture-imported-codex-token";
        let header = "fixture-imported-header-secret";
        fs::write(
            &config_path,
            format!(
                r#"model = "gpt-fixture"
model_provider = "external_fixture"

[model_providers.external_fixture]
name = "External Fixture"
base_url = "https://external.example.com/v1"
experimental_bearer_token = "{token}"
wire_api = "responses"
request_max_retries = 7

[model_providers.external_fixture.http_headers]
Authorization = "Bearer {header}"

[model_providers.external_fixture.query_params]
tenant = "fixture"
"#,
            ),
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap()
        .unwrap();
        assert_eq!(preview.suggested_name, "External Fixture");
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(token));
        assert!(!serialized.contains(header));

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: preview.suggested_name,
            },
        )
        .unwrap();
        assert!(!serde_json::to_string(&imported).unwrap().contains(token));
        let unchanged = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        assert!(matches!(
            unchanged.targets[0].change_kind,
            crate::domain::ChangeKind::Unchanged | crate::domain::ChangeKind::Warning
        ));

        let updated = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: imported.id,
                name: "Renamed Fixture".to_owned(),
                api_base_url: imported.api_base_url,
                api_key: SecretUpdate::Keep,
                default_model: imported.default_model,
                options: ProviderOptionsInput {
                    wire_api: Some("responses".to_owned()),
                    ..ProviderOptionsInput::default()
                },
                row_version: imported.row_version,
            },
        )
        .unwrap();
        let apply_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &apply_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();
        let written: Value =
            toml_edit::de::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        let provider_id = updated.options.provider_id.unwrap();
        assert_eq!(
            written["model_providers"][&provider_id]["request_max_retries"],
            7
        );
        assert_eq!(
            written["model_providers"][&provider_id]["http_headers"]["Authorization"],
            format!("Bearer {header}")
        );
        assert_eq!(
            written["model_providers"][&provider_id]["query_params"]["tenant"],
            "fixture"
        );
    }

    #[test]
    fn codex_oauth_import_adopts_openai_login_without_copying_tokens() {
        let mut fixture = fixture();
        let config_path = fixture.home.join(".codex/config.toml");
        let auth_path = fixture.home.join(".codex/auth.json");
        let access_token = "fixture-codex-oauth-access-token";
        let refresh_token = "fixture-codex-oauth-refresh-token";
        fs::write(
            &config_path,
            r#"model = "gpt-5.5"
"#,
        )
        .unwrap();
        fs::write(
            &auth_path,
            format!(
                r#"{{
  "auth_mode": "chatgpt",
  "tokens": {{
    "access_token": "{access_token}",
    "refresh_token": "{refresh_token}"
  }}
}}
"#
            ),
        )
        .unwrap();
        let mut redactor = SecretRedactor::default();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
        )
        .unwrap()
        .unwrap();
        assert_eq!(preview.suggested_name, "Codex OAuth 登录");
        assert_eq!(preview.default_model, "gpt-5.5");
        assert!(!preview.api_key_configured);
        let serialized_preview = serde_json::to_string(&preview).unwrap();
        assert!(!serialized_preview.contains(access_token));
        assert!(!serialized_preview.contains(refresh_token));
        assert_eq!(preview.redacted_projection["model"], "gpt-5.5");
        assert!(preview.redacted_projection.get("model_provider").is_none());

        let imported = confirm_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            ConfirmImportInput {
                preview_id: preview.preview_id,
                name: "Codex OAuth 登录".to_owned(),
            },
        )
        .unwrap();
        assert!(!imported.api_key_configured);
        assert_eq!(imported.options.provider_id.as_deref(), Some("openai"));
        let edited = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: imported.id.clone(),
                name: "Codex OAuth 编辑".to_owned(),
                api_base_url: imported.api_base_url.clone(),
                api_key: SecretUpdate::Keep,
                default_model: imported.default_model.clone(),
                options: ProviderOptionsInput::default(),
                row_version: imported.row_version,
            },
        )
        .unwrap();
        assert!(!edited.api_key_configured);
        let key_update = update_provider_profile(
            &mut fixture.database,
            &mut redactor,
            UpdateProviderProfileInput {
                id: edited.id.clone(),
                name: edited.name.clone(),
                api_base_url: edited.api_base_url.clone(),
                api_key: SecretUpdate::Replace("fixture-should-not-store".to_owned()),
                default_model: edited.default_model.clone(),
                options: ProviderOptionsInput::default(),
                row_version: edited.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(key_update.code(), crate::error::ErrorCode::InvalidInput);

        let sync_preview = preview_provider_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut redactor,
            Tool::Codex,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut redactor,
            &sync_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Provider,
            None,
        )
        .unwrap();
        let written: Value =
            toml_edit::de::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        assert_eq!(written["model"], "gpt-5.5");
        assert_eq!(written["model_provider"], "openai");
        assert!(written.get("model_providers").is_none());
        let serialized_imported = serde_json::to_string(&imported).unwrap();
        assert!(!serialized_imported.contains(access_token));
        assert!(!serialized_imported.contains(refresh_token));
    }

    #[test]
    fn codex_oauth_import_does_not_report_without_auth_tokens() {
        let mut fixture = fixture();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            r#"model = "gpt-5.5"
"#,
        )
        .unwrap();
        let preview = discover_provider_import(
            &mut fixture.database,
            &fixture.environment,
            &SecretRedactor::default(),
            Tool::Codex,
        )
        .unwrap();
        assert!(preview.is_none());
    }

    #[test]
    fn prompt_apply_is_exact_and_external_change_makes_preview_stale() {
        let mut fixture = fixture();
        let prompt = create_enabled_prompt(
            &mut fixture,
            Tool::Codex,
            "精确提示词",
            "# 第一版\n\n保留末尾空格  \n",
        );
        let redactor = SecretRedactor::default();
        let first_preview = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
            None,
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &first_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Prompt,
            None,
        )
        .unwrap();
        let prompt_path = fixture.home.join(".codex/AGENTS.md");
        assert_eq!(
            fs::read_to_string(&prompt_path).unwrap(),
            "# 第一版\n\n保留末尾空格  \n"
        );

        update_prompt_profile(
            &mut fixture.database,
            UpdatePromptProfileInput {
                id: prompt.id,
                name: prompt.name,
                body: "# 第二版\n".to_owned(),
                row_version: prompt.row_version,
            },
        )
        .unwrap();
        let stale_preview = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Codex,
            None,
        )
        .unwrap();
        fs::write(&prompt_path, "# 外部修改\n").unwrap();
        let error = apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &stale_preview.preview_id,
            Tool::Codex,
            ArtifactKind::Prompt,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(prompt_path).unwrap(), "# 外部修改\n");

        assert_eq!(
            create_prompt_profile(
                &mut fixture.database,
                PromptProfileInput {
                    name: "空提示词".to_owned(),
                    body: String::new(),
                },
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn provider_validation_rejects_url_credentials_and_unsupported_wire_api() {
        let mut fixture = fixture();
        let mut redactor = SecretRedactor::default();
        let mut credential_url = provider(
            Tool::Codex,
            "凭据 URL",
            "fixture-url-provider-secret",
            false,
        );
        credential_url.api_base_url = "https://user:password@provider.example.com/v1".to_owned();
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, credential_url)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );

        let mut unsupported_wire = provider(
            Tool::Codex,
            "旧 wire API",
            "fixture-wire-provider-secret",
            false,
        );
        unsupported_wire.options.wire_api = Some("chat".to_owned());
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, unsupported_wire)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );

        let mut secret_extra_env = provider(
            Tool::Claude,
            "扩展 env 秘密",
            "fixture-extra-env-provider-secret",
            false,
        );
        secret_extra_env.options.extra_env.insert(
            "ANTHROPIC_CUSTOM_HEADERS".to_owned(),
            "Authorization: Bearer fixture-hidden-header-secret".to_owned(),
        );
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, secret_extra_env)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
        assert!(!serde_json::to_string(
            &list_provider_profiles(&fixture.database, Tool::Claude).unwrap()
        )
        .unwrap()
        .contains("fixture-hidden-header-secret"));

        let mut multiline_extra_env = provider(
            Tool::Claude,
            "多行扩展 env",
            "fixture-multiline-provider-secret",
            false,
        );
        multiline_extra_env.options.extra_env.insert(
            "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
            "first-line\nsecond-line".to_owned(),
        );
        assert_eq!(
            create_provider_profile(&mut fixture.database, &mut redactor, multiline_extra_env)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    fn register_demo_project(fixture: &mut Fixture) -> crate::projects::ProjectDto {
        let project_root = fixture.home.join("projects/demo");
        fs::create_dir_all(&project_root).unwrap();
        crate::projects::register_project(
            &mut fixture.database,
            &fixture.environment,
            &crate::projects::RegisterProjectInput {
                display_name: "演示项目".to_owned(),
                root_path: project_root.to_string_lossy().into_owned(),
            },
        )
        .unwrap()
    }

    #[test]
    fn prompt_project_assignment_apply_overwrites_drift_and_unassign_keeps_file() {
        let mut fixture = fixture();
        let project = register_demo_project(&mut fixture);
        let redactor = SecretRedactor::default();
        let profile = create_prompt_profile(
            &mut fixture.database,
            PromptProfileInput {
                name: "项目提示词".to_owned(),
                body: "# 项目指引\n".to_owned(),
            },
        )
        .unwrap();

        // 档案被项目分配时禁止删除。
        crate::profiles::set_prompt_project_assignment(
            &mut fixture.database,
            &fixture.environment,
            &crate::profiles::SetPromptProjectAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                prompt_profile_id: Some(profile.id.clone()),
                project_row_version: project.row_version,
            },
        )
        .unwrap();
        let blocked = delete_prompt_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: profile.id.clone(),
                row_version: profile.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(blocked.code(), crate::error::ErrorCode::Conflict);

        // 分配后预览并应用：项目根出现 CLAUDE.md 硬拷贝。
        let preview = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
            Some(project.id.clone()),
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &preview.preview_id,
            Tool::Claude,
            ArtifactKind::Prompt,
            Some(&project.id),
        )
        .unwrap();
        let project_prompt_path = fixture.home.join("projects/demo/CLAUDE.md");
        assert_eq!(
            fs::read_to_string(&project_prompt_path).unwrap(),
            "# 项目指引\n"
        );

        // 项目文件被外部修改后，预览仍可合并（覆盖式重新应用），apply 后内容回到档案正文。
        fs::write(&project_prompt_path, "# 项目自行修改\n").unwrap();
        let drift_preview = preview_prompt_sync(
            &mut fixture.database,
            &fixture.environment,
            &redactor,
            Tool::Claude,
            Some(project.id.clone()),
        )
        .unwrap();
        apply_profile_preview(
            &Mutex::new(()),
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &mut SecretRedactor::default(),
            &drift_preview.preview_id,
            Tool::Claude,
            ArtifactKind::Prompt,
            Some(&project.id),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(&project_prompt_path).unwrap(),
            "# 项目指引\n"
        );

        // 分配期间项目提示词基线行存在。
        let baseline_count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_targets
                 WHERE tool = 'claude' AND artifact_kind = 'prompt' AND scope = 'project'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // 解除分配：项目文件保留，纳管基线清空（行保留、哈希置空）。
        crate::profiles::set_prompt_project_assignment(
            &mut fixture.database,
            &fixture.environment,
            &crate::profiles::SetPromptProjectAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                prompt_profile_id: None,
                project_row_version: project.row_version + 1,
            },
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(&project_prompt_path).unwrap(),
            "# 项目指引\n"
        );
        let cleared_baseline_count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_targets
                 WHERE tool = 'claude' AND artifact_kind = 'prompt' AND scope = 'project'
                   AND baseline_full_hash IS NULL AND baseline_managed_hash IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let active_baseline_count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_targets
                 WHERE tool = 'claude' AND artifact_kind = 'prompt' AND scope = 'project'
                   AND baseline_full_hash IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(baseline_count, 1);
        assert_eq!(cleared_baseline_count, 1);
        assert_eq!(active_baseline_count, 0);

        // 解除分配后档案可以删除。
        delete_prompt_profile(
            &mut fixture.database,
            &crate::profiles::VersionedProfileInput {
                id: profile.id.clone(),
                row_version: profile.row_version,
            },
        )
        .unwrap();
    }

    #[test]
    fn prompt_project_assignment_is_tool_agnostic_and_bumps_project_version() {
        let mut fixture = fixture();
        let project = register_demo_project(&mut fixture);
        let profile = create_prompt_profile(
            &mut fixture.database,
            PromptProfileInput {
                name: "共享档案".to_owned(),
                body: "# Claude\n".to_owned(),
            },
        )
        .unwrap();

        // 工具无关档案可分配到任意工具的项目记忆文件。
        crate::profiles::set_prompt_project_assignment(
            &mut fixture.database,
            &fixture.environment,
            &crate::profiles::SetPromptProjectAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                prompt_profile_id: Some(profile.id.clone()),
                project_row_version: project.row_version,
            },
        )
        .unwrap();
        let after_assign: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after_assign, i64::from(project.row_version) + 1);

        // 相同分配重复提交是无操作，项目 row_version 不变。
        crate::profiles::set_prompt_project_assignment(
            &mut fixture.database,
            &fixture.environment,
            &crate::profiles::SetPromptProjectAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                prompt_profile_id: Some(profile.id.clone()),
                project_row_version: u32::try_from(after_assign).unwrap(),
            },
        )
        .unwrap();
        let after_repeat: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&project.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(after_repeat, after_assign);
    }

    #[test]
    fn prompt_project_assignment_rejects_globally_active_profile() {
        let mut fixture = fixture();
        let project = register_demo_project(&mut fixture);
        let active = create_enabled_prompt(&mut fixture, Tool::Claude, "全局生效", "# 全局指令\n");

        // 全局生效档案不允许分配到项目，项目 row_version 不变。
        let rejected = crate::profiles::set_prompt_project_assignment(
            &mut fixture.database,
            &fixture.environment,
            &crate::profiles::SetPromptProjectAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                prompt_profile_id: Some(active.id.clone()),
                project_row_version: project.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(rejected.code(), crate::error::ErrorCode::Conflict);
        let reason = rejected
            .details()
            .and_then(|details| details.get("reason"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        assert!(reason.contains("全局生效"));

        // 先分配后启用（对该工具全局生效）的历史分配：重复写入当前分配保持无操作语义。
        let inactive = create_prompt_profile(
            &mut fixture.database,
            PromptProfileInput {
                name: "先分配后启用".to_owned(),
                body: "# 项目指引\n".to_owned(),
            },
        )
        .unwrap();
        crate::profiles::set_prompt_project_assignment(
            &mut fixture.database,
            &fixture.environment,
            &crate::profiles::SetPromptProjectAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                prompt_profile_id: Some(inactive.id.clone()),
                project_row_version: project.row_version,
            },
        )
        .unwrap();
        set_global_prompt_assignment(
            &mut fixture.database,
            &SetGlobalPromptAssignmentInput {
                tool: Tool::Claude,
                prompt_profile_id: inactive.id.clone(),
                assigned: true,
                row_version: inactive.row_version,
            },
        )
        .unwrap();
        crate::profiles::set_prompt_project_assignment(
            &mut fixture.database,
            &fixture.environment,
            &crate::profiles::SetPromptProjectAssignmentInput {
                project_id: project.id.clone(),
                tool: Tool::Claude,
                prompt_profile_id: Some(inactive.id.clone()),
                project_row_version: project.row_version + 1,
            },
        )
        .unwrap();
    }
}
