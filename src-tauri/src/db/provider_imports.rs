//! Provider 导入预览证据与多候选原子接管，绝不写入原生配置。
//!
//! 一次检测一行：`context_json` 只承载「哪些原生 provider 条目是候选」这一身份证据，
//! `redacted_preview_json` 只承载前端展示 DTO。确认时重新扫描原生文件并与证据求交，
//! 保证候选身份来自服务端持久化证据而不是客户端回传或脱敏投影。

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::Value;

use crate::{
    db::{
        column_tool,
        profiles::{
            adopt_baseline, deactivate_provider_profiles, map_profile_write_error,
            reject_provider_name_conflict, ImportedBaselineRecord, NewProviderProfileRecord,
            ProviderProfileRecord,
        },
        Database,
    },
    domain::{EntityId, Tool},
    error::AppError,
    sync::hash_json,
};

pub(crate) struct ProviderImportPreviewRecord {
    pub id: String,
    pub tool: Tool,
    pub target_path: String,
    pub observed_full_hash: String,
    pub context_json: String,
    pub redacted_preview_json: String,
    pub status: String,
}

pub(crate) fn persist_preview(
    database: &Database,
    record: &ProviderImportPreviewRecord,
) -> Result<(), AppError> {
    database
        .connection()
        .execute(
            "INSERT INTO provider_import_previews(id, tool, target_path, observed_full_hash,
             context_json, redacted_preview_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id,
                record.tool.as_str(),
                record.target_path,
                record.observed_full_hash,
                record.context_json,
                record.redacted_preview_json
            ],
        )
        .map_err(|error| {
            AppError::database(
                &database.path().to_string_lossy(),
                "persist_provider_import",
            )
            .with_source(error)
        })?;
    Ok(())
}

pub(crate) fn get_preview(
    database: &Database,
    id: &str,
) -> Result<ProviderImportPreviewRecord, AppError> {
    EntityId::parse(id)?;
    database
        .connection()
        .query_row(
            "SELECT id, tool, target_path, observed_full_hash, context_json,
                    redacted_preview_json, status
             FROM provider_import_previews WHERE id = ?1",
            [id],
            |row| {
                Ok(ProviderImportPreviewRecord {
                    id: row.get(0)?,
                    tool: column_tool(row, 1)?,
                    target_path: row.get(2)?,
                    observed_full_hash: row.get(3)?,
                    context_json: row.get(4)?,
                    redacted_preview_json: row.get(5)?,
                    status: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database.path().to_string_lossy(), "get_provider_import")
                .with_source(error)
        })?
        .ok_or_else(|| AppError::not_found("providerImportPreview", id))
}

/// 单个目标文件的原子批量接管：插入 N 份 Provider 档案、把**并集**投影写成唯一一份
/// 目标级基线、条件消费预览，任一失败整体回滚。
///
/// `validate_source` 在取到写锁之后重新扫描原生文件：锁等待期间原生内容可能已变化，
/// 不能沿用等待前的观察结果。
pub(crate) fn adopt_imported_providers(
    database: &mut Database,
    preview: &ProviderImportPreviewRecord,
    profiles: &[NewProviderProfileRecord],
    baseline_projection: &Value,
    validate_source: impl Fn() -> Result<(), AppError>,
) -> Result<Vec<ProviderProfileRecord>, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| AppError::database(&path, "begin_provider_import").with_source(error))?;
    let actual = transaction
        .query_row(
            "SELECT tool, target_path, observed_full_hash, context_json, status
             FROM provider_import_previews WHERE id = ?1",
            [&preview.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| AppError::database(&path, "validate_provider_import").with_source(error))?
        .ok_or_else(|| AppError::not_found("providerImportPreview", &preview.id))?;
    if actual.4 != "previewed" {
        return Err(AppError::preview_already_consumed(&preview.id, &actual.4));
    }
    if actual.0 != preview.tool.as_str()
        || actual.1 != preview.target_path
        || actual.2 != preview.observed_full_hash
        || actual.3 != preview.context_json
    {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let writer = transaction
        .query_row(
            "SELECT id, status FROM sync_runs
             WHERE status IN ('applying', 'restoring', 'rollback_failed') LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| {
            AppError::database(&path, "check_provider_import_writer").with_source(error)
        })?;
    if let Some((id, status)) = writer {
        return Err(AppError::write_in_progress(&id, &status));
    }
    validate_source()?;

    // 同一批次内允许只有一份生效档案；服务层保证 `is_active` 至多一个为真。
    if profiles.iter().any(|profile| profile.is_active) {
        deactivate_provider_profiles(&transaction, preview.tool, None, &path)?;
    }
    for profile in profiles {
        reject_provider_name_conflict(&transaction, profile.tool, &profile.name, None, &path)?;
        transaction
            .execute(
                "INSERT INTO provider_profiles(
                    id, tool, name, api_base_url, api_key, default_model, config_json, is_active
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    profile.id,
                    profile.tool.as_str(),
                    profile.name,
                    profile.api_base_url,
                    profile.api_key,
                    profile.default_model,
                    profile.config_json,
                    profile.is_active,
                ],
            )
            .map_err(|error| map_profile_write_error(error, &path, "adopt_provider_profiles"))?;
    }

    let projection_json = serde_json::to_string(baseline_projection).map_err(|error| {
        AppError::invalid_input("managedBaseline", "Provider 导入基线无法序列化").with_source(error)
    })?;
    let baseline = ImportedBaselineRecord {
        target_id: uuid::Uuid::new_v4().to_string(),
        target_path: preview.target_path.clone(),
        full_hash: preview.observed_full_hash.clone(),
        managed_hash: hash_json(baseline_projection),
        projection_json,
    };
    adopt_baseline(
        &transaction,
        preview.tool,
        crate::domain::ArtifactKind::Provider,
        &baseline,
        &path,
    )?;

    let consumed = transaction
        .execute(
            "UPDATE provider_import_previews
             SET status = 'consumed', consumed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status = 'previewed'",
            [&preview.id],
        )
        .map_err(|error| AppError::database(&path, "consume_provider_import").with_source(error))?;
    if consumed != 1 {
        return Err(AppError::preview_already_consumed(
            &preview.id,
            "not_previewed",
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_adopt_provider_import").with_source(error)
    })?;

    profiles
        .iter()
        .map(|profile| crate::db::profiles::get_provider_profile(database, &profile.id))
        .collect()
}
