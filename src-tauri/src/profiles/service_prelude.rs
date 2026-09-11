
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::models::{
    validate_prompt_fields, validate_provider_fields, validate_provider_fields_with_optional_key,
    ClaudeCredentialEnvKey, ConfirmImportInput, CopyProviderProfileInput, DeleteProfileResultDto,
    PromptImportPreviewDto, PromptProfileDto, PromptProfileInput, ProviderImportPreviewDto,
    ProviderOptionsInput, ProviderProfileDto, ProviderProfileInput, SecretUpdate,
    SetGlobalPromptAssignmentInput, StoredProviderConfig, ToolProfileStatusDto,
    UpdatePromptProfileInput, UpdateProviderProfileInput, VersionedProfileInput,
    CODEX_BEARER_TOKEN_WARNING, NEW_SESSION_NOTICE,
};
use crate::{
    adapters::{
        find_descriptor, DiscoveryContext, ExplicitEnvironment, ManagedOwnership, PolicyState,
        ProviderCodecInput, TargetDescriptor,
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
const CODEX_OPENAI_PROVIDER_ID: &str = "openai";
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
    let config_json = serde_json::to_string(&config).map_err(|error| {
        AppError::invalid_input("providerOptions", "Provider 选项无法序列化")
            .with_source_redacted(error, redactor)
    })?;
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
        Tool::Opencode => current_config.provider_id.clone().ok_or_else(|| {
            AppError::invalid_input("providerOptions", "OpenCode Provider 缺少稳定 provider id")
        })?,
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
    let api_base_url = source.api_base_url.clone().unwrap_or_default();
    let api_key = source.api_key.clone().unwrap_or_default();
    let default_model = source.default_model.clone().unwrap_or_default();
    validate_provider_fields(&input.target_name, &api_base_url, &api_key, &default_model)?;
    let target_id = Uuid::new_v4().to_string();
    let extra_env = BTreeMap::new();
    let codec_input = ProviderCodecInput {
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
            api_base_url: Some(api_base_url),
            api_key: Some(api_key),
            default_model: Some(default_model),
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
