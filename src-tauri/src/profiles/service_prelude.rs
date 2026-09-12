
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::models::{
    optional_text, validate_prompt_fields, validate_provider_fields, ClaudeCredentialEnvKey,
    ConfirmImportInput, CopyProviderProfileInput, DeleteProfileResultDto,
    PromptImportPreviewDto, PromptProfileDto, PromptProfileInput, ProviderAuthKind,
    ProviderFieldsInput, ProviderImportPreviewDto, ProviderOptionsInput, ProviderProfileDto,
    ProviderProfileInput, SecretUpdate, SetGlobalPromptAssignmentInput, StoredProviderConfig,
    ToolProfileStatusDto, UpdatePromptProfileInput, UpdateProviderProfileInput,
    VersionedProfileInput, CODEX_BEARER_TOKEN_WARNING, CODEX_OPENAI_PROVIDER_ID,
    NEW_SESSION_NOTICE,
};
use crate::{
    adapters::{
        find_descriptor, DiscoveryContext, ExplicitEnvironment, ManagedOwnership, PolicyState,
        ProviderCodecInput, TargetDescriptor, PROVIDER_AUTH_KIND_API_KEY,
    },
    app::AppPaths,
    db::{
        profiles::{
            self as repository, ImportPreviewRecord, ImportedBaselineRecord,
            NewPromptProfileRecord, NewProviderProfileRecord, PromptProfileRecord,
            ProviderProfileRecord,
        },
        Database,
    },
    domain::{ArtifactKind, ArtifactName, Scope, Tool},
    error::{AppError, ErrorCode},
    git::GitPathStatus,
    security::SecretRedactor,
    sync::{
        apply_persisted_preview, build_preview_plan, hash_json, load_persisted_preview,
        persist_preview, safe_row_version, scan_target, ApplyResult, ApplyTargetInput,
        DatabaseEntityType, DatabaseRowVersion, ManagedTargetBaseline, NoApplyFault, PreviewPlan,
        PreviewTargetRequest, TargetScan,
    },
};

#[cfg(test)]
const CLAUDE_MODEL_KEY: &str = "ANTHROPIC_MODEL";
const CLAUDE_PROVIDER_MANAGED_BY_HOST_KEY: &str = "CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST";
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
    let auth_kind = input.options.auth_kind;
    validate_provider_fields(&ProviderFieldsInput {
        tool: input.tool,
        auth_kind,
        name: &input.name,
        api_base_url: &input.api_base_url,
        api_key: Some(&input.api_key),
        default_model: &input.default_model,
    })?;
    let id = Uuid::new_v4().to_string();
    // Codex 官方登录固定使用内置 openai provider；其余渠道用稳定的生成 id。
    let provider_id = if input.tool == Tool::Codex && auth_kind == ProviderAuthKind::OfficialLogin
    {
        CODEX_OPENAI_PROVIDER_ID.to_owned()
    } else {
        generated_codex_provider_id(&id)
    };
    let config =
        StoredProviderConfig::from_input(input.tool, &provider_id, input.options, BTreeMap::new())?;
    let config_json = serde_json::to_string(&config).map_err(|error| {
        AppError::invalid_input("providerOptions", "Provider 选项无法序列化")
            .with_source_redacted(error, redactor)
    })?;
    let (api_base_url, api_key) = match auth_kind {
        ProviderAuthKind::ApiKey => (
            optional_text(&input.api_base_url),
            Some(input.api_key.clone()),
        ),
        ProviderAuthKind::OfficialLogin => (None, None),
    };
    if let Some(api_key) = &api_key {
        redactor.register_secret(api_key.clone());
    }
    let record = repository::insert_provider_profile(
        database,
        &NewProviderProfileRecord {
            id,
            tool: input.tool,
            name: input.name,
            api_base_url,
            api_key,
            default_model: optional_text(&input.default_model),
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
    let auth_kind = current_config.effective_auth_kind(current.tool);
    if input.options.auth_kind != auth_kind {
        return Err(AppError::invalid_input(
            "providerOptions",
            "渠道的认证方式在创建后不可更改，请新建渠道",
        ));
    }
    let provider_id = match current.tool {
        Tool::Claude => generated_codex_provider_id(&current.id),
        Tool::Codex => current_config.provider_id.clone().ok_or_else(|| {
            AppError::invalid_input("providerOptions", "Codex Provider 缺少稳定 provider id")
        })?,
        Tool::Zcode => current_config.provider_id.clone().ok_or_else(|| {
            AppError::invalid_input("providerOptions", "ZCode Provider 缺少稳定 provider id")
        })?,
        Tool::Cursor => return Err(cursor_unsupported(ArtifactKind::Provider)),
        Tool::Opencode => current_config.provider_id.clone().ok_or_else(|| {
            AppError::invalid_input("providerOptions", "OpenCode Provider 缺少稳定 provider id")
        })?,
    };
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
    validate_provider_fields(&ProviderFieldsInput {
        tool: current.tool,
        auth_kind,
        name: &input.name,
        api_base_url: &input.api_base_url,
        api_key: api_key.as_deref(),
        default_model: &input.default_model,
    })?;
    if let Some(api_key) = &api_key {
        redactor.register_secret(api_key.clone());
    }
    let row_version = i64::from(input.row_version);
    let record = repository::update_provider_profile(
        database,
        &input.id,
        &input.name,
        optional_text(&input.api_base_url).as_deref(),
        api_key.as_deref(),
        optional_text(&input.default_model).as_deref(),
        &serde_json::to_string(&config).map_err(|error| {
            AppError::invalid_input("providerOptions", "Provider 选项无法序列化")
                .with_source_redacted(error, redactor)
        })?,
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
    let source_config = parse_stored_provider_config(&source)?;
    if source_config.effective_auth_kind(source.tool) == ProviderAuthKind::OfficialLogin {
        return Err(AppError::invalid_input(
            "sourceId",
            "官方账号登录渠道绑定工具自身的登录态，不能跨工具复制",
        ));
    }
    let api_base_url = source.api_base_url.clone().unwrap_or_default();
    let api_key = source.api_key.clone().unwrap_or_default();
    let default_model = source.default_model.clone().unwrap_or_default();
    validate_provider_fields(&ProviderFieldsInput {
        tool: input.target_tool,
        auth_kind: ProviderAuthKind::ApiKey,
        name: &input.target_name,
        api_base_url: &api_base_url,
        api_key: Some(&api_key),
        default_model: &default_model,
    })?;
    let target_id = Uuid::new_v4().to_string();
    let extra_env = BTreeMap::new();
    let codec_input = ProviderCodecInput {
        auth_kind: PROVIDER_AUTH_KIND_API_KEY,
        credential_env_key: Some(ClaudeCredentialEnvKey::ApiKey.as_str()),
        extra_env: &extra_env,
        wire_api: None,
        zcode_kind: Some("anthropic"),
        opencode_npm: Some("@ai-sdk/openai-compatible"),
        opencode_api: Some("openai"),
    };
    let target_options = provider_options_from_codec(input.target_tool, &codec_input)?;
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
            api_base_url: optional_text(&api_base_url),
            api_key: Some(api_key),
            default_model: optional_text(&default_model),
            config_json: serde_json::to_string(&config).map_err(|error| {
                AppError::invalid_input("providerOptions", "目标 Provider 选项无法序列化")
                    .with_source_redacted(error, redactor)
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
