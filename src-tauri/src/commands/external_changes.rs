//! 被动外部变化的双向动作 RPC。
//!
//! 状态卡不会直接写文件：先生成并持久化一份带 observation/hash/row-version
//! 证据的普通 Preview，再由同一命令消费精确 `preview_id`。这样中央覆盖动作
//! 与常规同步共享 claim、快照、journal 和回滚边界。

use tauri::State;

use crate::{
    agents,
    app::AppState,
    commands::with_db_and_redactor,
    error::AppError,
    hooks, mcp, profiles, skills,
    sync::{
        claim_preview, load_persisted_preview, ApplyExternalChangePlanInput, ApplyResult,
        ExternalChangeAction, ExternalChangePlanDto, ExternalChangePlanInput, PersistedPreview,
        PersistedPreviewItem,
    },
};

#[tauri::command(async)]
#[specta::specta]
pub fn prepare_external_change_plan(
    state: State<'_, AppState>,
    input: ExternalChangePlanInput,
) -> Result<ExternalChangePlanDto, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        let plan = match input.artifact_kind {
            crate::domain::ArtifactKind::Provider => {
                if input.project_id.is_some() {
                    return Err(AppError::invalid_input(
                        "projectId",
                        "Provider 外部变化只支持全局目标",
                    ));
                }
                profiles::preview_provider_sync(
                    database,
                    &*state.environment()?,
                    redactor,
                    input.tool,
                )?
            }
            crate::domain::ArtifactKind::Prompt => {
                if input.project_id.is_some() {
                    return Err(AppError::invalid_input(
                        "projectId",
                        "Prompt 外部变化只支持全局目标",
                    ));
                }
                profiles::preview_prompt_sync(
                    database,
                    &*state.environment()?,
                    redactor,
                    input.tool,
                )?
            }
            crate::domain::ArtifactKind::Mcp => mcp::preview_mcp_sync(
                database,
                &*state.environment()?,
                redactor,
                &mcp::PreviewMcpSyncInput {
                    tool: input.tool,
                    project_id: input.project_id.clone(),
                    exclude_from_git: false,
                },
            )?,
            crate::domain::ArtifactKind::Skill => skills::preview_skill_sync(
                database,
                state.paths(),
                &*state.environment()?,
                redactor,
                &skills::PreviewSkillSyncInput {
                    tool: input.tool,
                    project_id: input.project_id.clone(),
                    exclude_from_git: false,
                },
            )?,
            crate::domain::ArtifactKind::Hook => hooks::preview_hook_sync(
                database,
                &*state.environment()?,
                redactor,
                &hooks::PreviewHookSyncInput {
                    tool: input.tool,
                    project_id: input.project_id.clone(),
                    exclude_from_git: false,
                },
            )?,
            crate::domain::ArtifactKind::Agent => agents::preview_agent_sync(
                database,
                &*state.environment()?,
                redactor,
                &agents::PreviewAgentSyncInput {
                    tool: input.tool,
                    project_id: input.project_id.clone(),
                    exclude_from_git: false,
                },
            )?,
        };
        Ok(ExternalChangePlanDto::from_preview(
            &plan,
            input.artifact_kind,
            input.tool,
        ))
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn apply_external_change_plan(
    state: State<'_, AppState>,
    input: ApplyExternalChangePlanInput,
) -> Result<ApplyResult, AppError> {
    with_db_and_redactor(&state, |database, redactor| {
        if matches!(
            input.artifact_kind,
            crate::domain::ArtifactKind::Provider | crate::domain::ArtifactKind::Prompt
        ) && input.project_id.is_some()
        {
            return Err(AppError::invalid_input(
                "projectId",
                "Provider/Prompt 外部变化只支持全局目标",
            ));
        }
        if input.action == ExternalChangeAction::AdoptNative {
            let _write_guard = state
                .write_operations()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let persisted = load_persisted_preview(database, &input.preview_id)?;
            let items = validate_native_adoption_plan(&persisted, &input)?;
            let journal_path = state
                .paths()
                .journals()
                .join(format!("{}.json", input.preview_id));
            claim_preview(database, &input.preview_id, &journal_path)?;

            let result = adopt_native_items(&state, database, redactor, &input, items);
            let applied_targets = match result {
                Ok(count) => count,
                Err(error) => {
                    settle_external_change_error(database, &input.preview_id, &error)?;
                    return Err(error);
                }
            };
            let target_ids = items
                .iter()
                .map(|item| item.target_id.as_str())
                .collect::<Vec<_>>();
            if let Err(error) = finish_external_change_run(database, &input.preview_id, &target_ids)
            {
                // 资源采纳已经完成但 run finalize 失败时，不能把 applying writer
                // 永久留在数据库里；按统一 apply 生命周期沉淀 stale/失败状态。
                settle_external_change_error(database, &input.preview_id, &error)?;
                return Err(error);
            }
            return Ok(ApplyResult {
                run_id: input.preview_id,
                status: "succeeded".to_owned(),
                applied_targets,
                snapshot_count: 0,
            });
        }

        match input.artifact_kind {
            crate::domain::ArtifactKind::Provider | crate::domain::ArtifactKind::Prompt => {
                profiles::apply_profile_preview(
                    state.write_operations(),
                    database,
                    state.paths(),
                    &*state.environment()?,
                    redactor,
                    &input.preview_id,
                    input.tool,
                    input.artifact_kind,
                )
            }
            crate::domain::ArtifactKind::Mcp => mcp::apply_mcp_preview(
                state.write_operations(),
                database,
                state.paths(),
                &*state.environment()?,
                redactor,
                &mcp::ApplyMcpPreviewInput {
                    preview_id: input.preview_id,
                    tool: input.tool,
                    project_id: input.project_id,
                },
            ),
            crate::domain::ArtifactKind::Skill => skills::apply_skill_preview(
                state.write_operations(),
                database,
                state.paths(),
                &*state.environment()?,
                redactor,
                &skills::ApplySkillPreviewInput {
                    preview_id: input.preview_id,
                    tool: input.tool,
                    project_id: input.project_id,
                },
            ),
            crate::domain::ArtifactKind::Hook => hooks::apply_hook_preview(
                state.write_operations(),
                database,
                state.paths(),
                &*state.environment()?,
                &hooks::ApplyHookPreviewInput {
                    preview_id: input.preview_id,
                    tool: input.tool,
                    project_id: input.project_id,
                },
            ),
            crate::domain::ArtifactKind::Agent => agents::apply_agent_preview(
                state.write_operations(),
                database,
                state.paths(),
                &*state.environment()?,
                &agents::ApplyAgentPreviewInput {
                    preview_id: input.preview_id,
                    tool: input.tool,
                    project_id: input.project_id,
                },
            ),
        }
    })
}

/// 校验状态卡动作携带的持久化 Preview 身份。采纳动作只消费已经观察到受管
/// 漂移的目标；解析、权限、策略、类型或 row-version 错误在这里 fail closed，
/// 不给资源执行器任何“猜测另一份目标”的机会。
fn validate_native_adoption_plan<'a>(
    persisted: &'a PersistedPreview,
    input: &ApplyExternalChangePlanInput,
) -> Result<&'a [PersistedPreviewItem], AppError> {
    if persisted.items.is_empty()
        || persisted.project_id != input.project_id
        || persisted.items.iter().any(|item| {
            item.envelope.descriptor.tool != input.tool
                || item.envelope.descriptor.artifact_kind != input.artifact_kind
                || item.status != crate::domain::SyncStatus::ExternalOwnedChange
                || item.change_kind == crate::domain::ChangeKind::Conflict
                || item.error_code.is_some()
                || item.envelope.descriptor.path.is_none()
                || item.envelope.descriptor.path.as_deref() != Some(item.target_path.as_str())
                || item.envelope.current_full_hash.is_none()
                || item.envelope.current_managed_hash.is_none()
        })
    {
        return Err(AppError::stale_preview(
            &input.preview_id,
            "externalChangePlan",
        ));
    }
    let expected_scope = if input.project_id.is_some() {
        crate::domain::Scope::Project
    } else {
        crate::domain::Scope::Global
    };
    if persisted.scope != expected_scope
        || persisted.items.iter().any(|item| {
            item.envelope.descriptor.scope != expected_scope
                || (item.envelope.descriptor.project_root.is_none()
                    && expected_scope == crate::domain::Scope::Project)
        })
    {
        return Err(AppError::stale_preview(
            &input.preview_id,
            "externalChangeScope",
        ));
    }
    Ok(&persisted.items)
}

/// 逐目标调用资源专用采纳执行器。每个执行器均在自身的 IMMEDIATE 事务中
/// 更新中央实体、managed item 和 baseline；本层只负责统一 claim/settle/finish
/// 生命周期，不复制资源解析逻辑。
fn adopt_native_items(
    state: &State<'_, AppState>,
    database: &mut crate::db::Database,
    redactor: &mut crate::security::SecretRedactor,
    input: &ApplyExternalChangePlanInput,
    items: &[PersistedPreviewItem],
) -> Result<u32, AppError> {
    let mut applied = 0_u32;
    for item in items {
        let envelope = &item.envelope;
        let target_path = envelope
            .descriptor
            .path
            .clone()
            .ok_or_else(|| AppError::stale_preview(&input.preview_id, "targetPath"))?;
        let observed_full_hash = envelope.current_full_hash.clone();
        let observed_managed_hash = envelope.current_managed_hash.clone();
        let target_id = item.target_id.clone();
        let target_row_version = envelope.target_row_version;
        let row_versions = envelope.row_versions.clone();
        let count = match input.artifact_kind {
            crate::domain::ArtifactKind::Provider => {
                if input.project_id.is_some() || items.len() != 1 {
                    return Err(AppError::conflict(
                        "externalChangePlan",
                        "Provider 原生采纳只支持单个全局目标",
                    ));
                }
                profiles::adopt_provider_native(
                    database,
                    &*state.environment()?,
                    redactor,
                    profiles::AdoptProviderNativeInput {
                        preview_id: Some(input.preview_id.clone()),
                        tool: input.tool,
                        target_id,
                        target_row_version,
                        target_path,
                        row_versions,
                        observed_full_hash,
                    },
                )?
                .adopted
                .len() as u32
            }
            crate::domain::ArtifactKind::Prompt => profiles::adopt_prompt_native(
                database,
                &*state.environment()?,
                profiles::AdoptPromptNativeInput {
                    tool: input.tool,
                    target_id,
                    target_row_version,
                    target_path,
                    row_versions,
                    observed_full_hash,
                    observed_managed_hash,
                },
            )?
            .adopted
            .len() as u32,
            crate::domain::ArtifactKind::Mcp => {
                mcp::adopt_mcp_native(
                    database,
                    &*state.environment()?,
                    redactor,
                    mcp::AdoptMcpNativeInput {
                        preview_id: input.preview_id.clone(),
                        tool: input.tool,
                        project_id: input.project_id.clone(),
                        target_id,
                        target_path,
                        status: item.status,
                        descriptor: envelope.descriptor.clone(),
                        ownership: envelope.ownership.clone(),
                        observed_full_hash,
                        observed_managed_hash,
                        target_row_version,
                        row_versions,
                    },
                )?
                .adopted_item_count
            }
            crate::domain::ArtifactKind::Skill => skills::adopt_skill_native(
                database,
                state.paths(),
                &*state.environment()?,
                &skills::AdoptSkillNativeInput {
                    tool: input.tool,
                    project_id: input.project_id.clone(),
                    target_id,
                    target_row_version,
                    target_path,
                    row_versions,
                    observed_full_hash,
                    observed_managed_hash,
                },
            )?
            .adopted
            .len() as u32,
            crate::domain::ArtifactKind::Hook => hooks::adopt_hook_native(
                database,
                state.paths(),
                &*state.environment()?,
                hooks::AdoptHookNativeInput {
                    tool: input.tool,
                    project_id: input.project_id.clone(),
                    target_id,
                    target_row_version,
                    target_path,
                    row_versions,
                    observed_full_hash,
                    observed_managed_hash,
                },
            )?
            .adopted
            .len() as u32,
            crate::domain::ArtifactKind::Agent => agents::adopt_agent_native(
                database,
                &*state.environment()?,
                redactor,
                agents::AdoptAgentNativeInput {
                    tool: input.tool,
                    project_id: input.project_id.clone(),
                    target_id,
                    target_row_version,
                    target_path,
                    row_versions,
                    observed_full_hash,
                    observed_managed_hash,
                },
            )?
            .adopted
            .len() as u32,
        };
        if count == 0 {
            // ExternalOwnedChange 代表受管内容确实漂移；若资源执行器无法把
            // 它唯一映射到中央实体（例如原生删除/重命名），必须进入应用内
            // 匹配/导入，而不能把未更新的 baseline 伪装成 in_sync。
            return Err(AppError::conflict(
                "externalChangePlan",
                "原生受管变化无法唯一映射到中央实体，请在应用内匹配或导入",
            ));
        }
        applied = applied.saturating_add(count);
    }
    Ok(applied)
}

fn settle_external_change_error(
    database: &mut crate::db::Database,
    preview_id: &str,
    error: &AppError,
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    if error.code() == crate::error::ErrorCode::StalePreview {
        crate::db::sync::mark_sync_run_stale(database.connection(), preview_id, &database_path)?;
    } else {
        crate::db::sync::settle_sync_run_error(
            database.connection(),
            preview_id,
            error.code().persisted().as_str(),
            &database_path,
        )?;
    }
    Ok(())
}

fn finish_external_change_run(
    database: &mut crate::db::Database,
    preview_id: &str,
    target_ids: &[&str],
) -> Result<(), AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_finish_external_change_plan")
                .with_source(error)
        })?;
    for target_id in target_ids {
        let updated = crate::db::sync::mark_sync_item_in_sync(
            &transaction,
            preview_id,
            target_id,
            &database_path,
        )?;
        if updated != 1 {
            return Err(AppError::stale_preview(preview_id, target_id));
        }
    }
    let finished = crate::db::sync::finish_sync_run(
        &transaction,
        preview_id,
        "applying",
        &database_path,
        "finish_external_change_plan",
    )?;
    if finished != 1 {
        return Err(AppError::database(
            &database_path,
            "finish_external_change_plan",
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_finish_external_change_plan").with_source(error)
    })?;
    Ok(())
}
