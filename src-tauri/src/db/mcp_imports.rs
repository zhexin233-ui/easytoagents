//! MCP 导入证据与单来源原子接管，绝不写入原生配置。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    db::{column_tool, mcp, Database},
    domain::{ArtifactKind, EntityId, Scope, SyncScopeDto, Tool},
    error::AppError,
    mcp::{McpImportResultDto, ValidatedMcpConfiguration},
    sync::{hash_json, DatabaseEntityType, DatabaseRowVersion, ManagedTargetBaseline},
};

pub(crate) struct McpImportPreviewRecord {
    pub id: String,
    pub tool: Tool,
    pub target_path: String,
    pub observed_full_hash: String,
    pub context_json: String,
    pub redacted_preview_json: String,
    pub status: String,
}

pub(crate) struct ImportedMcpItem {
    pub configuration: ValidatedMcpConfiguration,
    pub reuse_id: Option<String>,
    pub item_hash: String,
}

/// 外部 MCP 采纳的目标级证据。服务层先完成只读原生扫描和无损映射，仓储层
/// 再在一个 IMMEDIATE 事务中重新校验这些值并更新中央记录、managed item 和
/// 目标基线。`preview_id` 只用于把当前已 claim 的 apply writer 排除在外。
pub(crate) struct NativeMcpAdoptionTarget {
    pub preview_id: String,
    pub tool: Tool,
    pub scope: Scope,
    pub project_id: Option<String>,
    pub target_id: String,
    pub target_path: String,
    pub target_row_version: u32,
    pub observed_full_hash: String,
    pub observed_managed_hash: String,
    pub baseline_projection: Value,
    pub expected_row_versions: Vec<DatabaseRowVersion>,
}

/// 一个已经由服务层按 `resource_id` + `external_key` 唯一配对的 MCP 条目。
/// `configuration` 只存在于应用私有内存/中央数据库，不会作为 RPC 结果返回。
pub(crate) struct NativeMcpAdoptionItem {
    pub id: String,
    pub resource_id: String,
    pub external_key: String,
    pub expected_item_row_version: u32,
    pub expected_resource_row_version: u32,
    pub item_hash: String,
    pub configuration: ValidatedMcpConfiguration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeMcpAdoptionResult {
    pub adopted_item_count: u32,
    pub updated_server_count: u32,
}

pub(crate) fn persist_preview(
    database: &Database,
    record: &McpImportPreviewRecord,
) -> Result<(), AppError> {
    database
        .connection()
        .execute(
            "INSERT INTO mcp_import_previews(id, tool, target_path, observed_full_hash,
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
            AppError::database(&database.path().to_string_lossy(), "persist_mcp_import")
                .with_source(error)
        })?;
    Ok(())
}

pub(crate) fn get_preview(
    database: &Database,
    id: &str,
) -> Result<McpImportPreviewRecord, AppError> {
    EntityId::parse(id)?;
    database.connection().query_row(
        "SELECT id, tool, target_path, observed_full_hash, context_json, redacted_preview_json, status
         FROM mcp_import_previews WHERE id = ?1", [id], |row| {
            Ok(McpImportPreviewRecord {
                id: row.get(0)?,
                tool: column_tool(row, 1)?,
                target_path: row.get(2)?, observed_full_hash: row.get(3)?,
                context_json: row.get(4)?, redacted_preview_json: row.get(5)?, status: row.get(6)?,
            })
        },
    ).optional().map_err(|error| AppError::database(&database.path().to_string_lossy(), "get_mcp_import").with_source(error))?
        .ok_or_else(|| AppError::not_found("mcpImportPreview", id))
}

/// 名称匹配可能涉及整个中央列表；同时绑定分配和管理元数据，兼顾其它进程。
/// 指纹仅含身份/版本，原始配置和秘密值不进入导入证据。
pub(crate) fn state_fingerprint(
    connection: &Connection,
    tool: Tool,
    target_path: &str,
) -> Result<String, AppError> {
    let read_error =
        |error| AppError::database(target_path, "read_mcp_import_state").with_source(error);
    let mut state = Vec::new();
    let mut servers = connection
        .prepare_cached("SELECT id, row_version FROM mcp_servers ORDER BY id")
        .map_err(read_error)?;
    let rows = servers
        .query_map([], |row| {
            Ok(json!([row.get::<_, String>(0)?, row.get::<_, i64>(1)?]))
        })
        .map_err(read_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_error)?;
    state.push(json!(rows));
    for sql in [
        "SELECT mcp_id, '' FROM mcp_global_assignments WHERE tool = ?1 ORDER BY mcp_id",
        "SELECT mcp_id, project_id FROM mcp_project_assignments WHERE tool = ?1 ORDER BY mcp_id, project_id",
    ] {
        let mut statement = connection.prepare_cached(sql).map_err(read_error)?;
        let rows = statement.query_map([tool.as_str()], |row| Ok(json!([
            row.get::<_, String>(0)?, row.get::<_, String>(1)?
        ]))).map_err(read_error)?.collect::<Result<Vec<_>, _>>().map_err(read_error)?;
        state.push(json!(rows));
    }
    for sql in [
        "SELECT id, row_version FROM managed_targets WHERE tool = ?1 AND artifact_kind = 'mcp'
         AND scope = 'global' AND project_id IS NULL AND target_path = ?2 ORDER BY id",
        "SELECT item.id, item.row_version FROM managed_items item JOIN managed_targets target
         ON target.id = item.target_id WHERE target.tool = ?1 AND target.artifact_kind = 'mcp'
         AND target.scope = 'global' AND target.project_id IS NULL AND target.target_path = ?2 ORDER BY item.id",
    ] {
        let mut statement = connection.prepare_cached(sql).map_err(read_error)?;
        let rows = statement.query_map(params![tool.as_str(), target_path], |row| Ok(json!([
            row.get::<_, String>(0)?, row.get::<_, i64>(1)?
        ]))).map_err(read_error)?.collect::<Result<Vec<_>, _>>().map_err(read_error)?;
        state.push(json!(rows));
    }
    Ok(hash_json(&json!(state)))
}

pub(crate) fn has_project_assignment(
    database: &Database,
    tool: Tool,
    mcp_id: &str,
) -> Result<bool, AppError> {
    database
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM mcp_project_assignments WHERE tool = ?1 AND mcp_id = ?2)",
            params![tool.as_str(), mcp_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            AppError::database(
                &database.path().to_string_lossy(),
                "read_import_project_assignment",
            )
            .with_source(error)
        })
}

pub(crate) fn adopt_import(
    database: &mut Database,
    preview: &McpImportPreviewRecord,
    expected_state: &str,
    baseline: Option<&ManagedTargetBaseline>,
    projection: &Value,
    items: &[ImportedMcpItem],
    validate_source: impl Fn() -> Result<(), AppError>,
) -> Result<McpImportResultDto, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| AppError::database(&path, "begin_mcp_import").with_source(error))?;
    let actual = transaction.query_row(
        "SELECT tool, target_path, observed_full_hash, context_json, status FROM mcp_import_previews WHERE id = ?1",
        [&preview.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
    ).optional().map_err(|error| AppError::database(&path, "validate_mcp_import").with_source(error))?
        .ok_or_else(|| AppError::not_found("mcpImportPreview", &preview.id))?;
    if actual.4 != "previewed" {
        return Err(AppError::preview_already_consumed(&preview.id, &actual.4));
    }
    if actual.0 != preview.tool.as_str()
        || actual.1 != preview.target_path
        || actual.2 != preview.observed_full_hash
        || actual.3 != preview.context_json
        || state_fingerprint(&transaction, preview.tool, &preview.target_path)? != expected_state
    {
        return Err(AppError::stale_preview(&preview.id, &preview.target_path));
    }
    let writer = transaction
        .query_row(
            "SELECT id, status FROM sync_runs WHERE status IN ('applying', 'restoring', 'rollback_failed') LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| AppError::database(&path, "check_mcp_import_writer").with_source(error))?;
    if let Some((id, status)) = writer {
        return Err(AppError::write_in_progress(&id, &status));
    }
    // 取得写锁可能等待其它连接，不能沿用等待前的原生读取结果。
    validate_source()?;
    let target_id = baseline
        .map(|value| value.target_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let projection_json = serde_json::to_string(projection).map_err(|error| {
        AppError::invalid_input("import", "导入基线无法序列化").with_source(error)
    })?;
    let managed_hash = hash_json(projection);
    if let Some(baseline) = baseline {
        let updated = transaction.execute(
            "UPDATE managed_targets SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
             baseline_projection_json = ?4, last_status = 'in_sync' WHERE id = ?1 AND row_version = ?5",
            params![target_id, preview.observed_full_hash, managed_hash, projection_json, baseline.target_row_version],
        ).map_err(|error| AppError::database(&path, "extend_mcp_import_baseline").with_source(error))?;
        if updated != 1 {
            return Err(AppError::stale_preview(&preview.id, &preview.target_path));
        }
    } else {
        transaction
            .execute(
                "INSERT INTO managed_targets(id, tool, artifact_kind, scope, target_path,
             baseline_full_hash, baseline_managed_hash, baseline_projection_json, last_status)
             VALUES (?1, ?2, 'mcp', 'global', ?3, ?4, ?5, ?6, 'in_sync')",
                params![
                    target_id,
                    preview.tool.as_str(),
                    preview.target_path,
                    preview.observed_full_hash,
                    managed_hash,
                    projection_json
                ],
            )
            .map_err(|error| {
                AppError::database(&path, "adopt_mcp_import_baseline").with_source(error)
            })?;
    }
    let mut result = McpImportResultDto {
        tool: preview.tool,
        created_count: 0,
        reused_count: 0,
        assigned_count: 0,
        affected_sync_scopes: None,
    };
    for item in items {
        let id = if let Some(id) = &item.reuse_id {
            result.reused_count += 1;
            id.clone()
        } else {
            result.created_count += 1;
            mcp::insert_mcp_configuration(&transaction, &item.configuration, &path)?
        };
        let assigned = transaction
            .execute(
                "INSERT OR IGNORE INTO mcp_global_assignments(tool, mcp_id) VALUES (?1, ?2)",
                params![preview.tool.as_str(), id],
            )
            .map_err(|error| mcp::map_mcp_write_error(error, &path, "assign_mcp_import"))?;
        if assigned > 0 {
            result.assigned_count += 1;
            transaction
                .execute(
                    "UPDATE mcp_servers SET updated_at = updated_at WHERE id = ?1",
                    [&id],
                )
                .map_err(|error| {
                    AppError::database(&path, "touch_imported_mcp").with_source(error)
                })?;
        }
        transaction.execute(
            "INSERT INTO managed_items(id, target_id, resource_kind, resource_id, external_key, last_applied_item_hash)
             VALUES (?1, ?2, 'mcp', ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), target_id, id, item.configuration.name, item.item_hash],
        ).map_err(|error| AppError::conflict("import", "原生 MCP 的管理关系已变化").with_source(error))?;
    }
    // 文件不受 SQLite 锁保护，入库期间源文件变化必须让整批回滚。
    validate_source()?;
    let consumed = transaction.execute(
        "UPDATE mcp_import_previews SET status = 'consumed', consumed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ?1 AND status = 'previewed'", [&preview.id],
    ).map_err(|error| AppError::database(&path, "consume_mcp_import").with_source(error))?;
    if consumed != 1 {
        return Err(AppError::preview_already_consumed(&preview.id, "consumed"));
    }
    transaction
        .commit()
        .map_err(|error| AppError::database(&path, "commit_mcp_import").with_source(error))?;
    result.affected_sync_scopes = Some(if result.assigned_count > 0 {
        vec![SyncScopeDto::global(ArtifactKind::Mcp, result.tool)]
    } else {
        Vec::new()
    });
    Ok(result)
}

/// 将已确认的原生 MCP 条目采纳为中央记录的权威内容。
///
/// 该入口与 `adopt_import` 分开：外部变化采纳不创建中央记录、不猜测名称，也
/// 不消费 import token；它只更新预览证据中已经绑定的 MCP 行，并在同一
/// IMMEDIATE 事务里刷新条目级和目标级基线。`validate_source` 在拿到 SQLite
/// 写锁后执行两次（写入前、提交前），因此等待锁期间原生文件发生变化会让整
/// 批更新回滚。
pub(crate) fn adopt_native_mcp(
    database: &mut Database,
    target: &NativeMcpAdoptionTarget,
    items: &[NativeMcpAdoptionItem],
    validate_source: impl Fn() -> Result<(), AppError>,
) -> Result<NativeMcpAdoptionResult, AppError> {
    if items.is_empty()
        || !is_sha256(&target.observed_full_hash)
        || !is_sha256(&target.observed_managed_hash)
        || items.iter().any(|item| !is_sha256(&item.item_hash))
    {
        return Err(AppError::invalid_input(
            "mcpAdopt",
            "MATCH_OR_IMPORT_REQUIRED",
        ));
    }
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| AppError::database(&path, "begin_adopt_native_mcp").with_source(error))?;

    // `claim_preview` 已经把当前预览标记为 applying；它就是这次采纳持有的写者，
    // 不能把自己误判成并发写入。其它 applying/restoring/rollback_failed 仍然
    // 必须 fail closed。
    let writer = transaction
        .query_row(
            "SELECT id, status FROM sync_runs
             WHERE status IN ('applying', 'restoring', 'rollback_failed')
               AND id <> ?1
             ORDER BY started_at
             LIMIT 1",
            [&target.preview_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| {
            AppError::database(&path, "check_adopt_native_mcp_writer").with_source(error)
        })?;
    if let Some((id, status)) = writer {
        return Err(AppError::write_in_progress(&id, &status));
    }

    let expected = expected_row_versions(&target.expected_row_versions)?;
    verify_target_identity(&transaction, target, &path, &expected)?;
    verify_expected_row_versions(&transaction, target, &expected, &path)?;
    verify_managed_item_identity(&transaction, target, items, &expected, &path)?;
    verify_mcp_assignments(&transaction, target, &expected, &path)?;

    if hash_json(&target.baseline_projection) != target.observed_managed_hash {
        return Err(AppError::stale_preview(
            &target.preview_id,
            "mcpNativeManagedHash",
        ));
    }

    // 取得写锁后重新确认源文件；调用方会把 full hash 与最初预览绑定。
    validate_source()?;

    let mut updated_server_count = 0_u32;
    for item in items {
        let serialized = mcp::serialize_configuration_json(&item.configuration)?;
        let current = transaction
            .query_row(
                "SELECT name, transport, command, args_json, url, headers_json,
                        env_json, extra_json, enabled, row_version
                 FROM mcp_servers WHERE id = ?1",
                [&item.resource_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, bool>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| {
                AppError::database(&path, "read_adopt_native_mcp_server").with_source(error)
            })?
            .ok_or_else(|| AppError::stale_preview(&target.preview_id, "mcpNativeServer"))?;
        if current.0 != item.external_key
            || current.0 != item.configuration.name
            || current.9 != i64::from(item.expected_resource_row_version)
        {
            return Err(AppError::stale_preview(
                &target.preview_id,
                "mcpNativeServerIdentity",
            ));
        }

        let changed = current.1 != item.configuration.transport.as_str()
            || current.2 != item.configuration.command
            || current.3 != serialized.args
            || current.4 != item.configuration.url
            || current.5 != serialized.headers
            || current.6 != serialized.env
            || current.7 != serialized.extra
            || current.8 != item.configuration.enabled;
        if changed {
            let updated = transaction
                .execute(
                    "UPDATE mcp_servers
                     SET name = ?2, transport = ?3, command = ?4, args_json = ?5,
                         url = ?6, headers_json = ?7, env_json = ?8, extra_json = ?9,
                         enabled = ?10
                     WHERE id = ?1 AND row_version = ?11",
                    params![
                        item.resource_id,
                        item.external_key,
                        item.configuration.transport.as_str(),
                        item.configuration.command,
                        serialized.args,
                        item.configuration.url,
                        serialized.headers,
                        serialized.env,
                        serialized.extra,
                        item.configuration.enabled,
                        item.expected_resource_row_version,
                    ],
                )
                .map_err(|error| {
                    mcp::map_mcp_write_error(error, &path, "adopt_native_mcp_server")
                })?;
            if updated != 1 {
                return Err(AppError::stale_preview(
                    &target.preview_id,
                    "mcpNativeServer",
                ));
            }
            updated_server_count = updated_server_count.saturating_add(1);
        }

        let current_item_hash: String = transaction
            .query_row(
                "SELECT last_applied_item_hash FROM managed_items
                 WHERE id = ?1 AND target_id = ?2 AND resource_kind = 'mcp'",
                params![item.id, target.target_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                AppError::database(&path, "read_adopt_native_mcp_item_hash").with_source(error)
            })?;
        if current_item_hash != item.item_hash {
            let updated = transaction
                .execute(
                    "UPDATE managed_items
                     SET last_applied_item_hash = ?2
                     WHERE id = ?1 AND target_id = ?3 AND resource_kind = 'mcp'
                       AND row_version = ?4",
                    params![
                        item.id,
                        item.item_hash,
                        target.target_id,
                        item.expected_item_row_version,
                    ],
                )
                .map_err(|error| {
                    AppError::database(&path, "adopt_native_mcp_item").with_source(error)
                })?;
            if updated != 1 {
                return Err(AppError::stale_preview(
                    &target.preview_id,
                    "mcpNativeManagedItem",
                ));
            }
        }
    }

    // 原生文件不受 SQLite 锁保护；第二次校验即使失败，也会回滚上面所有中央
    // 更新和 item 基线更新，不留下半批采纳。
    validate_source()?;
    let projection_json = serde_json::to_string(&target.baseline_projection).map_err(|error| {
        AppError::invalid_input("mcpAdopt", "MCP 原生采纳基线无法序列化").with_source(error)
    })?;
    let updated_target = transaction
        .execute(
            "UPDATE managed_targets
             SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                 baseline_projection_json = ?4, last_status = 'in_sync'
             WHERE id = ?1 AND tool = ?5 AND artifact_kind = 'mcp'
               AND scope = ?6 AND ifnull(project_id, '') = ifnull(?7, '')
               AND target_path = ?8 AND row_version = ?9",
            params![
                target.target_id,
                target.observed_full_hash,
                target.observed_managed_hash,
                projection_json,
                target.tool.as_str(),
                target.scope.as_str(),
                target.project_id,
                target.target_path,
                target.target_row_version,
            ],
        )
        .map_err(|error| AppError::database(&path, "adopt_native_mcp_target").with_source(error))?;
    if updated_target != 1 {
        return Err(AppError::stale_preview(
            &target.preview_id,
            "mcpNativeTarget",
        ));
    }

    transaction
        .commit()
        .map_err(|error| AppError::database(&path, "commit_adopt_native_mcp").with_source(error))?;
    Ok(NativeMcpAdoptionResult {
        adopted_item_count: items.len().try_into().unwrap_or(u32::MAX),
        updated_server_count,
    })
}

fn expected_row_versions(
    rows: &[DatabaseRowVersion],
) -> Result<BTreeMap<(DatabaseEntityType, String), u32>, AppError> {
    let mut expected = BTreeMap::new();
    for row in rows {
        if !matches!(
            row.entity_type,
            DatabaseEntityType::McpServer
                | DatabaseEntityType::ManagedTarget
                | DatabaseEntityType::ManagedItem
                | DatabaseEntityType::Project
        ) {
            return Err(AppError::invalid_input(
                "rowVersions",
                "MCP 原生采纳包含不支持的数据库行版本",
            ));
        }
        let key = (row.entity_type, row.entity_id.clone());
        if expected
            .insert(key, row.row_version)
            .is_some_and(|value| value != row.row_version)
        {
            return Err(AppError::stale_preview("mcpNative", "rowVersions"));
        }
    }
    Ok(expected)
}

fn verify_target_identity(
    transaction: &rusqlite::Transaction<'_>,
    target: &NativeMcpAdoptionTarget,
    path: &str,
    expected: &BTreeMap<(DatabaseEntityType, String), u32>,
) -> Result<(), AppError> {
    let row = transaction
        .query_row(
            "SELECT tool, artifact_kind, scope, project_id, target_path, row_version,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
             FROM managed_targets WHERE id = ?1",
            [&target.target_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(path, "verify_adopt_native_mcp_target").with_source(error)
        })?
        .ok_or_else(|| AppError::stale_preview(&target.preview_id, "mcpNativeTarget"))?;
    if row.0 != target.tool.as_str()
        || row.1 != ArtifactKind::Mcp.as_str()
        || row.2 != target.scope.as_str()
        || row.3 != target.project_id
        || row.4 != target.target_path
        || u32::try_from(row.5).ok() != Some(target.target_row_version)
    {
        return Err(AppError::stale_preview(
            &target.preview_id,
            "mcpNativeTargetIdentity",
        ));
    }
    let (Some(baseline_full_hash), Some(baseline_managed_hash), Some(projection_json)) =
        (row.6, row.7, row.8)
    else {
        return Err(AppError::invalid_input(
            "mcpAdopt",
            "MATCH_OR_IMPORT_REQUIRED",
        ));
    };
    let projection = serde_json::from_str::<Value>(&projection_json)
        .map_err(|_| AppError::invalid_input("mcpAdopt", "MATCH_OR_IMPORT_REQUIRED"))?;
    if !is_sha256(&baseline_full_hash)
        || !is_sha256(&baseline_managed_hash)
        || hash_json(&projection) != baseline_managed_hash
    {
        return Err(AppError::invalid_input(
            "mcpAdopt",
            "MATCH_OR_IMPORT_REQUIRED",
        ));
    }
    if let Some(version) =
        expected.get(&(DatabaseEntityType::ManagedTarget, target.target_id.clone()))
    {
        if *version != target.target_row_version {
            return Err(AppError::stale_preview(
                &target.preview_id,
                "mcpNativeTargetRowVersion",
            ));
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn verify_expected_row_versions(
    transaction: &rusqlite::Transaction<'_>,
    target: &NativeMcpAdoptionTarget,
    expected: &BTreeMap<(DatabaseEntityType, String), u32>,
    path: &str,
) -> Result<(), AppError> {
    for ((entity_type, entity_id), expected_version) in expected {
        if *entity_type == DatabaseEntityType::ManagedTarget {
            if entity_id != &target.target_id {
                return Err(AppError::stale_preview(
                    &target.preview_id,
                    "mcpNativeRowVersions",
                ));
            }
            continue;
        }
        let (table, resource) = match entity_type {
            DatabaseEntityType::McpServer => ("mcp_servers", "mcpServer"),
            DatabaseEntityType::ManagedItem => ("managed_items", "managedItem"),
            DatabaseEntityType::Project => ("projects", "project"),
            _ => continue,
        };
        let query = format!("SELECT row_version FROM {table} WHERE id = ?1");
        let actual = transaction
            .query_row(&query, [entity_id], |row| row.get::<_, i64>(0))
            .optional()
            .map_err(|error| {
                AppError::database(path, "verify_adopt_native_mcp_row_version").with_source(error)
            })?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(*expected_version) {
            return Err(AppError::stale_preview(&target.preview_id, resource));
        }
    }
    if target.scope == Scope::Project {
        let project_id = target
            .project_id
            .as_deref()
            .ok_or_else(|| AppError::invalid_input("projectId", "项目 MCP 采纳缺少 project_id"))?;
        let Some(version) = expected.get(&(DatabaseEntityType::Project, project_id.to_owned()))
        else {
            return Err(AppError::stale_preview(
                &target.preview_id,
                "mcpNativeProject",
            ));
        };
        let actual = transaction
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1 AND removed_at IS NULL",
                [project_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| {
                AppError::database(path, "verify_adopt_native_mcp_project").with_source(error)
            })?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(*version) {
            return Err(AppError::stale_preview(
                &target.preview_id,
                "mcpNativeProject",
            ));
        }
    } else if expected
        .keys()
        .any(|(entity_type, _)| *entity_type == DatabaseEntityType::Project)
    {
        return Err(AppError::stale_preview(
            &target.preview_id,
            "mcpNativeProject",
        ));
    }
    Ok(())
}

fn verify_managed_item_identity(
    transaction: &rusqlite::Transaction<'_>,
    target: &NativeMcpAdoptionTarget,
    items: &[NativeMcpAdoptionItem],
    expected: &BTreeMap<(DatabaseEntityType, String), u32>,
    path: &str,
) -> Result<(), AppError> {
    let mut ids = BTreeSet::new();
    let mut resources = BTreeSet::new();
    let mut external_keys = BTreeSet::new();
    for item in items {
        if !ids.insert(item.id.clone())
            || !resources.insert(item.resource_id.clone())
            || !external_keys.insert(item.external_key.clone())
        {
            return Err(AppError::invalid_input(
                "mcpAdopt",
                "MCP 原生条目无法唯一匹配，请在应用内匹配或导入",
            ));
        }
        if item.configuration.name != item.external_key {
            return Err(AppError::invalid_input(
                "mcpAdopt",
                "MCP 原生条目名称发生变化，请在应用内匹配或导入",
            ));
        }
        if expected.get(&(DatabaseEntityType::ManagedItem, item.id.clone()))
            != Some(&item.expected_item_row_version)
            || expected.get(&(DatabaseEntityType::McpServer, item.resource_id.clone()))
                != Some(&item.expected_resource_row_version)
        {
            return Err(AppError::stale_preview(
                &target.preview_id,
                "mcpNativeItemRowVersion",
            ));
        }
        let row = transaction
            .query_row(
                "SELECT target_id, resource_kind, resource_id, external_key,
                        last_applied_item_hash, row_version
                 FROM managed_items WHERE id = ?1",
                [&item.id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| {
                AppError::database(path, "verify_adopt_native_mcp_item").with_source(error)
            })?
            .ok_or_else(|| AppError::stale_preview(&target.preview_id, "mcpNativeItem"))?;
        if row.0 != target.target_id
            || row.1 != ArtifactKind::Mcp.as_str()
            || row.2 != item.resource_id
            || row.3 != item.external_key
            || u32::try_from(row.5).ok() != Some(item.expected_item_row_version)
        {
            return Err(AppError::stale_preview(
                &target.preview_id,
                "mcpNativeItemIdentity",
            ));
        }
    }

    let mut statement = transaction
        .prepare(
            "SELECT id FROM managed_items
             WHERE target_id = ?1 AND resource_kind = 'mcp'",
        )
        .map_err(|error| {
            AppError::database(path, "prepare_verify_adopt_native_mcp_items").with_source(error)
        })?;
    let stored_ids = statement
        .query_map([&target.target_id], |row| row.get::<_, String>(0))
        .map_err(|error| {
            AppError::database(path, "query_verify_adopt_native_mcp_items").with_source(error)
        })?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| {
            AppError::database(path, "decode_verify_adopt_native_mcp_items").with_source(error)
        })?;
    if stored_ids != ids {
        return Err(AppError::stale_preview(
            &target.preview_id,
            "mcpNativeManagedItems",
        ));
    }
    Ok(())
}

fn verify_mcp_assignments(
    transaction: &rusqlite::Transaction<'_>,
    target: &NativeMcpAdoptionTarget,
    expected: &BTreeMap<(DatabaseEntityType, String), u32>,
    path: &str,
) -> Result<(), AppError> {
    for (entity_type, mcp_id) in expected.keys() {
        if *entity_type != DatabaseEntityType::McpServer {
            continue;
        }
        let assigned = match target.scope {
            Scope::Global => transaction
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM mcp_global_assignments
                        WHERE tool = ?1 AND mcp_id = ?2
                    )",
                    params![target.tool.as_str(), mcp_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| {
                    AppError::database(path, "verify_adopt_native_mcp_global_assignment")
                        .with_source(error)
                })?,
            Scope::Project => transaction
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM mcp_project_assignments
                        WHERE project_id = ?1 AND tool = ?2 AND mcp_id = ?3
                    ) OR EXISTS(
                        SELECT 1 FROM mcp_global_assignments
                        WHERE tool = ?2 AND mcp_id = ?3
                    )",
                    params![target.project_id, target.tool.as_str(), mcp_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| {
                    AppError::database(path, "verify_adopt_native_mcp_project_assignment")
                        .with_source(error)
                })?,
        };
        if !assigned {
            return Err(AppError::stale_preview(
                &target.preview_id,
                "mcpNativeAssignment",
            ));
        }
    }
    Ok(())
}
