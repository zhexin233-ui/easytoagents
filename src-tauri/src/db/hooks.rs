//! Hooks 中央记录、全局/项目分配与 managed item 基线仓储。

use std::collections::BTreeSet;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::Value;

use crate::{
    db::{
        column_tool,
        mcp::{touch_versioned_row, verify_row_version},
        Database,
    },
    domain::{
        stable_sync_scopes, tool_capabilities, validate_global_assignment,
        validate_project_assignment, ArtifactKind, EntityId, HookEvent, SyncScopeDto, Tool,
    },
    error::AppError,
    sync::{DatabaseEntityType, DatabaseRowVersion},
};

#[derive(Debug, Clone, PartialEq)]
pub struct HookRecord {
    pub id: String,
    pub name: String,
    pub event: HookEvent,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_seconds: Option<i32>,
    pub enabled: bool,
    /// 接管脚本在中央目录内的文件名；NULL = inline 命令。
    pub script_name: Option<String>,
    /// 中央脚本内容的 SHA-256；与 script_name 同置同空。
    pub script_hash: Option<String>,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedHookItemRecord {
    pub id: String,
    pub resource_id: String,
    pub external_key: String,
    pub last_applied_item_hash: String,
    pub row_version: i64,
}

/// 已由服务层完成原生事件/条目唯一配对与字段校验的中央 Hook 更新。
///
/// `event` 是目标 assignment 的生效事件，而不是客户端可以随意改写的中央
/// 建议事件；数据库事务会再次核对 assignment，防止调用方绕过服务层静默改变
/// Hook 的生效含义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeHookAdoption {
    pub id: String,
    pub row_version: u32,
    pub event: HookEvent,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_seconds: Option<i32>,
    pub script_name: Option<String>,
    pub script_hash: Option<String>,
}

/// 已由服务层配对的 managed item 更新。`resource_id` 必须仍指向原来的中央
/// Hook；采纳不允许借机把一个匿名原生条目重绑到另一个中央资源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeHookItemAdoption {
    pub id: String,
    pub target_id: String,
    pub row_version: u32,
    pub resource_id: String,
    pub external_key: String,
    pub last_applied_item_hash: String,
}

/// Hook 原生采纳的目标身份与 observation 证据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeHookTargetAdoption {
    pub target_id: String,
    pub target_row_version: u32,
    pub target_path: String,
    pub tool: Tool,
    pub scope: crate::domain::Scope,
    pub project_id: Option<String>,
    pub observed_full_hash: String,
    pub observed_managed_hash: String,
    pub baseline_projection_json: String,
    pub row_versions: Vec<DatabaseRowVersion>,
}

const HOOK_COLUMNS: &str = "id, name, event, matcher, command, timeout_seconds, enabled, script_name, script_hash, row_version";

pub fn list_hooks(database: &Database) -> Result<Vec<HookRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(&format!(
            "SELECT {HOOK_COLUMNS} FROM hooks ORDER BY name COLLATE NOCASE, id"
        ))
        .map_err(|error| AppError::database(&path, "prepare_list_hooks").with_source(error))?;
    let records = statement
        .query_map([], hook_from_row)
        .map_err(|error| AppError::database(&path, "query_list_hooks").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database(&path, "decode_list_hooks").with_source(error))?;
    Ok(records)
}

pub fn get_hook(database: &Database, id: &str) -> Result<HookRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            &format!("SELECT {HOOK_COLUMNS} FROM hooks WHERE id = ?1"),
            [id],
            hook_from_row,
        )
        .optional()
        .map_err(|error| AppError::database(&path, "get_hook").with_source(error))?
        .ok_or_else(|| AppError::not_found("hook", id))
}

/// 校验后的中央 Hook 定义；由服务层构造，仓储层不重复业务校验。
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedHookDefinition {
    pub name: String,
    pub event: HookEvent,
    pub matcher: Option<String>,
    pub command: String,
    pub timeout_seconds: Option<i32>,
    pub enabled: bool,
    pub script_name: Option<String>,
    pub script_hash: Option<String>,
}

/// 插入中央 Hook。`id` 由服务层生成（脚本中央目录需要先于 DB 插入确定路径）。
pub(crate) fn insert_hook(
    database: &mut Database,
    id: &str,
    value: &ValidatedHookDefinition,
) -> Result<HookRecord, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    EntityId::parse(id)?;
    database
        .connection_mut()
        .execute(
            "INSERT INTO hooks(
                id, name, event, matcher, command, timeout_seconds, enabled,
                script_name, script_hash
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                value.name,
                value.event.as_str(),
                value.matcher,
                value.command,
                value.timeout_seconds,
                value.enabled,
                value.script_name,
                value.script_hash,
            ],
        )
        .map_err(|error| map_hook_write_error(error, &path, "insert_hook"))?;
    get_hook(database, id)
}

pub(crate) fn update_hook(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
    value: &ValidatedHookDefinition,
) -> Result<HookRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE hooks
             SET name = ?2, event = ?3, matcher = ?4, command = ?5,
                 timeout_seconds = ?6, enabled = ?7
             WHERE id = ?1 AND row_version = ?8",
            params![
                id,
                value.name,
                value.event.as_str(),
                value.matcher,
                value.command,
                value.timeout_seconds,
                value.enabled,
                expected_row_version,
            ],
        )
        .map_err(|error| map_hook_write_error(error, &path, "update_hook"))?;
    if updated != 1 {
        return stale_or_missing_hook(database, id);
    }
    get_hook(database, id)
}

pub fn set_hook_enabled(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
    enabled: bool,
) -> Result<HookRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE hooks SET enabled = ?2 WHERE id = ?1 AND row_version = ?3",
            params![id, enabled, expected_row_version],
        )
        .map_err(|error| map_hook_write_error(error, &path, "set_hook_enabled"))?;
    if updated != 1 {
        return stale_or_missing_hook(database, id);
    }
    get_hook(database, id)
}

pub fn delete_hook(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
) -> Result<(), AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let deleted = database
        .connection_mut()
        .execute(
            "DELETE FROM hooks WHERE id = ?1 AND row_version = ?2",
            params![id, expected_row_version],
        )
        .map_err(|error| map_hook_write_error(error, &path, "delete_hook"))?;
    if deleted != 1 {
        return stale_or_missing_hook(database, id).map(drop);
    }
    Ok(())
}

/// 该 Hook 的全局分配列表：(工具, 生效事件)。事件随分配存储（迁移 0016）。
/// 一条语句取回全部 Hook 的全局分配（工具 + 生效事件），供列表接口一次组装。
pub fn global_assignments_for_all_hooks(
    database: &Database,
) -> Result<std::collections::BTreeMap<String, Vec<(Tool, HookEvent)>>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT hook_id, tool, event FROM hook_global_assignments ORDER BY hook_id, tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_all_hook_global_tools").with_source(error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                hook_tool_from_row(row, 1)?,
                event_from_database(row.get(2)?)?,
            ))
        })
        .map_err(|error| {
            AppError::database(&path, "query_all_hook_global_tools").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_all_hook_global_tools").with_source(error)
        })?;
    let mut grouped = std::collections::BTreeMap::<String, Vec<(Tool, HookEvent)>>::new();
    for (hook_id, tool, event) in rows {
        grouped.entry(hook_id).or_default().push((tool, event));
    }
    Ok(grouped)
}

pub fn global_assignments_for_hook(
    database: &Database,
    hook_id: &str,
) -> Result<Vec<(Tool, HookEvent)>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT tool, event FROM hook_global_assignments
             WHERE hook_id = ?1 ORDER BY tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_hook_global_tools").with_source(error)
        })?;
    let assignments = statement
        .query_map([hook_id], |row| {
            Ok((
                hook_tool_from_row(row, 0)?,
                event_from_database(row.get(1)?)?,
            ))
        })
        .map_err(|error| AppError::database(&path, "query_hook_global_tools").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_hook_global_tools").with_source(error)
        })?;
    Ok(assignments)
}

/// 返回 Hook 当前所有显式 assignment 对应的同步 scope。
pub fn sync_scopes_for_hook(
    database: &Database,
    hook_id: &str,
) -> Result<Vec<SyncScopeDto>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT tool, NULL FROM hook_global_assignments WHERE hook_id = ?1
             UNION ALL
             SELECT tool, project_id FROM hook_project_assignments WHERE hook_id = ?1
             UNION ALL
             SELECT target.tool, target.project_id
             FROM managed_items AS item
             JOIN managed_targets AS target ON target.id = item.target_id
             WHERE item.resource_kind = 'hook' AND item.resource_id = ?1
             ORDER BY tool, project_id",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_hook_sync_scopes").with_source(error)
        })?;
    let scopes = statement
        .query_map([hook_id], |row| {
            Ok(SyncScopeDto::new(
                ArtifactKind::Hook,
                hook_tool_from_row(row, 0)?,
                row.get(1)?,
            ))
        })
        .map_err(|error| AppError::database(&path, "query_hook_sync_scopes").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database(&path, "decode_hook_sync_scopes").with_source(error))?;
    Ok(stable_sync_scopes(scopes))
}

fn event_from_database(value: String) -> rusqlite::Result<HookEvent> {
    HookEvent::from_stable_str(&value).ok_or(rusqlite::Error::InvalidQuery)
}

/// Hook 分配行只允许支持 Hooks 的工具；写入侧由服务层门禁，读取侧同样 fail-closed，
/// 避免历史脏数据被静默当成合法分配。
fn hook_tool_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Tool> {
    let tool = column_tool(row, index)?;
    if tool_capabilities()
        .iter()
        .any(|capability| capability.tool == tool && capability.hooks)
    {
        Ok(tool)
    } else {
        Err(rusqlite::Error::InvalidQuery)
    }
}

pub fn set_global_assignment(
    database: &mut Database,
    tool: Tool,
    hook_id: &str,
    event: HookEvent,
    assigned: bool,
    expected_row_version: u32,
) -> Result<HookRecord, AppError> {
    EntityId::parse(hook_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&path, "begin_set_hook_global_assignment").with_source(error)
        })?;
    verify_row_version(
        &transaction,
        "hooks",
        hook_id,
        expected_row_version,
        "hook",
        &path,
    )?;
    let changed = if assigned {
        let project_count = transaction
            .query_row(
                "SELECT COUNT(*) FROM hook_project_assignments
                 WHERE tool = ?1 AND hook_id = ?2",
                params![tool.as_str(), hook_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                AppError::database(&path, "count_hook_project_assignments").with_source(error)
            })?;
        validate_global_assignment(project_count > 0)?;
        // 已分配时更新事件（同 (tool, hook) 一工具一事件；切换 = UPDATE）。
        transaction
            .execute(
                "INSERT INTO hook_global_assignments(tool, hook_id, event) VALUES (?1, ?2, ?3)
                 ON CONFLICT(tool, hook_id) DO UPDATE SET event = excluded.event",
                params![tool.as_str(), hook_id, event.as_str()],
            )
            .map_err(|error| map_hook_write_error(error, &path, "insert_hook_global_assignment"))?
    } else {
        transaction
            .execute(
                "DELETE FROM hook_global_assignments WHERE tool = ?1 AND hook_id = ?2",
                params![tool.as_str(), hook_id],
            )
            .map_err(|error| {
                AppError::database(&path, "delete_hook_global_assignment").with_source(error)
            })?
    };
    if changed == 1 {
        touch_versioned_row(&transaction, "hooks", hook_id, expected_row_version, &path)?;
    }
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_set_hook_global_assignment").with_source(error)
    })?;
    get_hook(database, hook_id)
}

#[allow(clippy::too_many_arguments)]
pub fn set_project_assignment(
    database: &mut Database,
    project_id: &str,
    tool: Tool,
    hook_id: &str,
    event: HookEvent,
    assigned: bool,
    expected_hook_row_version: u32,
    expected_project_row_version: u32,
) -> Result<HookRecord, AppError> {
    EntityId::parse(project_id)?;
    EntityId::parse(hook_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&path, "begin_set_hook_project_assignment").with_source(error)
        })?;
    verify_row_version(
        &transaction,
        "hooks",
        hook_id,
        expected_hook_row_version,
        "hook",
        &path,
    )?;
    verify_row_version(
        &transaction,
        "projects",
        project_id,
        expected_project_row_version,
        "project",
        &path,
    )?;
    let globally_assigned = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM hook_global_assignments
                WHERE tool = ?1 AND hook_id = ?2
             )",
            params![tool.as_str(), hook_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            AppError::database(&path, "read_hook_global_assignment").with_source(error)
        })?;
    // 全局项在项目层是只读继承：不仅禁止重复添加，也禁止通过伪造 RPC 请求
    // 把一个不存在的项目 assignment 当作“禁用全局项”移除。
    validate_project_assignment(globally_assigned)?;
    let changed = if assigned {
        transaction
            .execute(
                "INSERT INTO hook_project_assignments(project_id, tool, hook_id, event)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(project_id, tool, hook_id) DO UPDATE SET event = excluded.event",
                params![project_id, tool.as_str(), hook_id, event.as_str()],
            )
            .map_err(|error| map_hook_write_error(error, &path, "insert_hook_project_assignment"))?
    } else {
        transaction
            .execute(
                "DELETE FROM hook_project_assignments
                 WHERE project_id = ?1 AND tool = ?2 AND hook_id = ?3",
                params![project_id, tool.as_str(), hook_id],
            )
            .map_err(|error| {
                AppError::database(&path, "delete_hook_project_assignment").with_source(error)
            })?
    };
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "hooks",
            hook_id,
            expected_hook_row_version,
            &path,
        )?;
        touch_versioned_row(
            &transaction,
            "projects",
            project_id,
            expected_project_row_version,
            &path,
        )?;
    }
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_set_hook_project_assignment").with_source(error)
    })?;
    get_hook(database, hook_id)
}

pub fn list_assigned_hooks(
    database: &Database,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<Vec<HookRecord>, AppError> {
    let path = database.path().to_string_lossy();
    // 生效事件取自分配行（迁移 0016）；hooks.event 仅是建议事件。
    let (sql, project_parameter) = match project_id {
        Some(project_id) => (
            "SELECT hook.id, hook.name, hook.matcher, hook.command,
                    hook.timeout_seconds, hook.enabled, hook.script_name,
                    hook.script_hash, hook.row_version, assignment.event
             FROM hooks AS hook
             JOIN hook_project_assignments AS assignment ON assignment.hook_id = hook.id
             WHERE assignment.project_id = ?1 AND assignment.tool = ?2
             ORDER BY hook.name COLLATE NOCASE, hook.id",
            Some(project_id),
        ),
        None => (
            "SELECT hook.id, hook.name, hook.matcher, hook.command,
                    hook.timeout_seconds, hook.enabled, hook.script_name,
                    hook.script_hash, hook.row_version, assignment.event
             FROM hooks AS hook
             JOIN hook_global_assignments AS assignment ON assignment.hook_id = hook.id
             WHERE assignment.tool = ?2
             ORDER BY hook.name COLLATE NOCASE, hook.id",
            None,
        ),
    };
    let mut statement = database.connection().prepare_cached(sql).map_err(|error| {
        AppError::database(&path, "prepare_list_assigned_hooks").with_source(error)
    })?;
    let records = statement
        .query_map(
            params![project_parameter, tool.as_str()],
            assigned_hook_from_row,
        )
        .map_err(|error| AppError::database(&path, "query_list_assigned_hooks").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_list_assigned_hooks").with_source(error)
        })?;
    Ok(records)
}

/// 项目分配的生效事件映射：hook_id -> event（供项目选项 DTO 回填）。
pub fn project_assignment_events(
    database: &Database,
    project_id: &str,
    tool: Tool,
) -> Result<Vec<(String, HookEvent)>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT hook_id, event FROM hook_project_assignments
             WHERE project_id = ?1 AND tool = ?2",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_project_assignment_events").with_source(error)
        })?;
    let rows = statement
        .query_map(params![project_id, tool.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                event_from_database(row.get(1)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
            ))
        })
        .map_err(|error| {
            AppError::database(&path, "query_project_assignment_events").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_project_assignment_events").with_source(error)
        })?;
    Ok(rows)
}

pub fn project_assignment_exists(
    database: &Database,
    project_id: &str,
    tool: Tool,
    hook_id: &str,
) -> Result<bool, AppError> {
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM hook_project_assignments
                WHERE project_id = ?1 AND tool = ?2 AND hook_id = ?3
             )",
            params![project_id, tool.as_str(), hook_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            AppError::database(&path, "read_hook_project_assignment").with_source(error)
        })
}

pub fn list_managed_hook_items(
    database: &Database,
    target_id: &str,
) -> Result<Vec<ManagedHookItemRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT id, resource_id, external_key, last_applied_item_hash, row_version
             FROM managed_items
             WHERE target_id = ?1 AND resource_kind = 'hook'
             ORDER BY external_key, id",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_list_managed_hook_items").with_source(error)
        })?;
    let items = statement
        .query_map([target_id], |row| {
            Ok(ManagedHookItemRecord {
                id: row.get(0)?,
                resource_id: row.get(1)?,
                external_key: row.get(2)?,
                last_applied_item_hash: row.get(3)?,
                row_version: row.get(4)?,
            })
        })
        .map_err(|error| {
            AppError::database(&path, "query_list_managed_hook_items").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_list_managed_hook_items").with_source(error)
        })?;
    Ok(items)
}

/// 在同一个 `IMMEDIATE` SQLite 事务中提交 Hook 原生采纳：中央 Hook 字段、
/// managed item 身份/条目 hash 与目标 baseline 一起 CAS 更新。
///
/// 服务层负责解析原生文档和完成唯一配对；这里仍然重新验证目标身份、assignment、
/// Hook/item 行版本和完整 baseline，避免任何绕过服务层的调用把匿名条目静默绑定到
/// 另一份中央配置。原生文件本身由服务层在进入本函数前完成安全脚本 staging；若
/// 任一 SQL/CAS/约束失败，事务自动回滚，调用方负责恢复已安装的脚本副本。
pub(crate) fn adopt_native_hooks(
    database: &mut Database,
    target: &NativeHookTargetAdoption,
    hooks: &[NativeHookAdoption],
    items: &[NativeHookItemAdoption],
    validate_sources: impl Fn() -> Result<(), AppError>,
) -> Result<(), AppError> {
    EntityId::parse(&target.target_id)?;
    if target.target_path.trim().is_empty()
        || (target.scope == crate::domain::Scope::Global && target.project_id.is_some())
        || (target.scope == crate::domain::Scope::Project && target.project_id.is_none())
    {
        return Err(AppError::invalid_input(
            "targetPath",
            "Hook 采纳的目标身份不完整",
        ));
    }
    if !is_sha256(&target.observed_full_hash) || !is_sha256(&target.observed_managed_hash) {
        return Err(AppError::invalid_input(
            "observedHash",
            "Hook 采纳缺少有效的目标 hash",
        ));
    }
    let projection =
        serde_json::from_str::<Value>(&target.baseline_projection_json).map_err(|error| {
            AppError::invalid_input("managedBaseline", "Hook 接管基线无法序列化").with_source(error)
        })?;
    if crate::sync::hash_json(&projection) != target.observed_managed_hash {
        return Err(AppError::stale_preview(
            "adoptHookNative",
            &target.target_path,
        ));
    }
    if hooks.is_empty() || hooks.len() != items.len() {
        return Err(AppError::conflict("hookAdopt", "MATCH_OR_IMPORT_REQUIRED"));
    }
    let mut hook_ids = BTreeSet::new();
    for hook in hooks {
        EntityId::parse(&hook.id)?;
        if !hook_ids.insert(hook.id.as_str()) {
            return Err(AppError::conflict("hookAdopt", "同一原生 Hook 被重复映射"));
        }
        if hook.script_name.is_some() != hook.script_hash.is_some()
            || hook
                .script_hash
                .as_deref()
                .is_some_and(|hash| !is_sha256(hash))
        {
            return Err(AppError::invalid_input(
                "scriptHash",
                "Hook 脚本元数据不完整",
            ));
        }
    }
    let mut item_ids = BTreeSet::new();
    for item in items {
        EntityId::parse(&item.id)?;
        if item.target_id != target.target_id
            || !is_sha256(&item.last_applied_item_hash)
            || !item_ids.insert(item.id.as_str())
        {
            return Err(AppError::conflict(
                "hookAdopt",
                "Hook managed item 身份或条目 hash 无法证明",
            ));
        }
        EntityId::parse(&item.resource_id)?;
    }

    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_adopt_native_hooks").with_source(error)
        })?;

    let actual_target = transaction
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
            AppError::database(&database_path, "verify_adopt_native_hook_target").with_source(error)
        })?
        .ok_or_else(|| AppError::stale_preview("adoptHookNative", &target.target_path))?;
    if actual_target.0 != target.tool.as_str()
        || actual_target.1 != ArtifactKind::Hook.as_str()
        || actual_target.2 != target.scope.as_str()
        || actual_target.3 != target.project_id
        || actual_target.4 != target.target_path
        || u32::try_from(actual_target.5).ok() != Some(target.target_row_version)
    {
        return Err(AppError::stale_preview(
            "adoptHookNative",
            &target.target_path,
        ));
    }
    if actual_target.6.is_none() != actual_target.7.is_none()
        || actual_target.7.is_none() != actual_target.8.is_none()
    {
        return Err(AppError::conflict(
            "hookAdopt",
            "受管 Hook baseline 不完整，必须先重新生成计划",
        ));
    }
    let (Some(baseline_full_hash), Some(baseline_managed_hash), Some(baseline_projection)) = (
        actual_target.6.as_deref(),
        actual_target.7.as_deref(),
        actual_target.8.as_deref(),
    ) else {
        return Err(AppError::conflict("hookAdopt", "MATCH_OR_IMPORT_REQUIRED"));
    };
    let baseline_projection = serde_json::from_str::<Value>(baseline_projection)
        .map_err(|_| AppError::conflict("hookAdopt", "受管 Hook baseline 无法解析"))?;
    if !is_sha256(baseline_full_hash)
        || !is_sha256(baseline_managed_hash)
        || crate::sync::hash_json(&baseline_projection) != baseline_managed_hash
    {
        return Err(AppError::conflict(
            "hookAdopt",
            "受管 Hook baseline hash 不一致",
        ));
    }

    let existing_item_count = transaction
        .query_row(
            "SELECT COUNT(*) FROM managed_items
             WHERE target_id = ?1 AND resource_kind = 'hook'",
            [&target.target_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            AppError::database(&database_path, "count_adopt_native_hook_items").with_source(error)
        })?;
    if existing_item_count != i64::try_from(items.len()).unwrap_or(i64::MAX) {
        return Err(AppError::conflict(
            "hookAdopt",
            "Hook managed item 集合已变化，必须进入匹配或导入",
        ));
    }

    let expected_row_versions = expected_hook_row_versions(target, hooks, items)?;
    verify_hook_row_versions(&transaction, target, &expected_row_versions, &database_path)?;
    // 脚本文件位于 SQLite 之外。取得 IMMEDIATE 锁后、修改任何中央行之前重新
    // 验证，避免调用方等待数据库锁期间的源文件编辑被合并进本次采纳。
    validate_sources()?;

    let hook_by_id = hooks
        .iter()
        .map(|hook| (hook.id.as_str(), hook))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut linked_hook_ids = BTreeSet::new();
    for item in items {
        let Some(hook) = hook_by_id.get(item.resource_id.as_str()) else {
            return Err(AppError::conflict(
                "hookAdopt",
                "Hook managed item 不能重绑定到未知中央记录",
            ));
        };
        if !linked_hook_ids.insert(hook.id.as_str()) {
            return Err(AppError::conflict(
                "hookAdopt",
                "多个 Hook managed item 指向同一中央记录",
            ));
        }
        let existing = transaction
            .query_row(
                "SELECT resource_id, row_version FROM managed_items
                 WHERE id = ?1 AND target_id = ?2 AND resource_kind = 'hook'",
                params![item.id, target.target_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|error| {
                AppError::database(&database_path, "verify_adopt_native_hook_item")
                    .with_source(error)
            })?
            .ok_or_else(|| AppError::stale_preview("adoptHookNative", &target.target_path))?;
        if existing.0 != hook.id || u32::try_from(existing.1).ok() != Some(item.row_version) {
            return Err(AppError::stale_preview(
                "adoptHookNative",
                &target.target_path,
            ));
        }
    }
    if linked_hook_ids.len() != hook_ids.len() {
        return Err(AppError::conflict(
            "hookAdopt",
            "Hook managed item 与中央记录集合无法一一对应",
        ));
    }

    for hook in hooks {
        let current = transaction
            .query_row(
                "SELECT enabled, row_version FROM hooks WHERE id = ?1",
                [&hook.id],
                |row| Ok((row.get::<_, bool>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|error| {
                AppError::database(&database_path, "verify_adopt_native_hook").with_source(error)
            })?
            .ok_or_else(|| AppError::stale_preview("adoptHookNative", &hook.id))?;
        if !current.0 || u32::try_from(current.1).ok() != Some(hook.row_version) {
            return Err(AppError::stale_preview("adoptHookNative", &hook.id));
        }

        let assignment_event = match target.scope {
            crate::domain::Scope::Global => transaction
                .query_row(
                    "SELECT event FROM hook_global_assignments
                     WHERE tool = ?1 AND hook_id = ?2",
                    params![target.tool.as_str(), hook.id],
                    |row| row.get::<_, String>(0),
                )
                .optional(),
            crate::domain::Scope::Project => transaction
                .query_row(
                    "SELECT event FROM hook_project_assignments
                     WHERE project_id = ?1 AND tool = ?2 AND hook_id = ?3
                     LIMIT 1",
                    params![target.project_id.as_deref(), target.tool.as_str(), hook.id],
                    |row| row.get::<_, String>(0),
                )
                .optional(),
        }
        .map_err(|error| {
            AppError::database(&database_path, "verify_adopt_native_hook_assignment")
                .with_source(error)
        })?
        .ok_or_else(|| AppError::conflict("assignment", "Hook assignment 已被移除或改变"))?;
        // `hooks.event` 是中央建议事件，effective event 由 assignment 行决定；
        // 一个中央 Hook 可以在不同工具/范围使用不同的生效事件。这里只能
        // 核对当前目标 assignment，不能把建议事件误当成 assignment 事件。
        if assignment_event != hook.event.as_str() {
            return Err(AppError::conflict(
                "assignment",
                "原生 Hook 的事件与当前 assignment 不一致",
            ));
        }

        let changed = transaction
            .execute(
                "UPDATE hooks
                 SET matcher = ?2, command = ?3, timeout_seconds = ?4,
                     script_name = ?5, script_hash = ?6
                 WHERE id = ?1 AND row_version = ?7 AND enabled = 1",
                params![
                    hook.id,
                    hook.matcher,
                    hook.command,
                    hook.timeout_seconds,
                    hook.script_name,
                    hook.script_hash,
                    hook.row_version,
                ],
            )
            .map_err(|error| map_hook_write_error(error, &database_path, "adopt_native_hook"))?;
        if changed != 1 {
            return Err(AppError::stale_preview("adoptHookNative", &hook.id));
        }
    }

    for item in items {
        let changed = transaction
            .execute(
                "UPDATE managed_items
                 SET resource_id = ?2, external_key = ?3, last_applied_item_hash = ?4
                 WHERE id = ?1 AND target_id = ?5 AND resource_kind = 'hook'
                   AND row_version = ?6",
                params![
                    item.id,
                    item.resource_id,
                    item.external_key,
                    item.last_applied_item_hash,
                    target.target_id,
                    item.row_version,
                ],
            )
            .map_err(|error| {
                map_hook_write_error(error, &database_path, "adopt_native_hook_item")
            })?;
        if changed != 1 {
            return Err(AppError::stale_preview(
                "adoptHookNative",
                &target.target_path,
            ));
        }
    }

    // 第二次读取关闭首次源文件检查与中央/item 更新之间的窗口；任何不一致都
    // 会中止整个事务。
    validate_sources()?;

    let changed = crate::db::sync::update_managed_target_baseline(
        &transaction,
        &target.target_id,
        Some(&target.observed_full_hash),
        Some(&target.observed_managed_hash),
        &target.baseline_projection_json,
        target.target_row_version,
        &database_path,
    )?;
    if changed != 1 {
        return Err(AppError::stale_preview(
            "adoptHookNative",
            &target.target_path,
        ));
    }
    // 写入 baseline 后再检查一次，关闭 SQLite 更新目标行期间源文件可能变化的
    // 小窗口；此时事务尚未提交，因此可以安全回滚。
    validate_sources()?;
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_adopt_native_hooks").with_source(error)
    })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn expected_hook_row_versions(
    target: &NativeHookTargetAdoption,
    hooks: &[NativeHookAdoption],
    items: &[NativeHookItemAdoption],
) -> Result<std::collections::BTreeMap<(DatabaseEntityType, String), u32>, AppError> {
    if target.project_id.is_some() {
        if target.scope != crate::domain::Scope::Project {
            return Err(AppError::stale_preview("adoptHookNative", "rowVersions"));
        }
    } else if target.scope != crate::domain::Scope::Global {
        return Err(AppError::stale_preview("adoptHookNative", "rowVersions"));
    }
    let mut supplied = std::collections::BTreeMap::new();
    for row in &target.row_versions {
        if !matches!(
            row.entity_type,
            DatabaseEntityType::Project
                | DatabaseEntityType::Hook
                | DatabaseEntityType::ManagedItem
                | DatabaseEntityType::ManagedTarget
        ) {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Hook 原生采纳包含不支持的数据库行版本",
            ));
        }
        let key = (row.entity_type, row.entity_id.clone());
        if supplied.insert(key, row.row_version).is_some() {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Hook 原生采纳包含重复的行版本证据",
            ));
        }
    }

    // 共享 Preview 层会在持久化 envelope 中携带 ManagedTarget，Hook 服务输入
    // 也会把它作为独立字段传入。两种表示都可接受，但绝不能出现第二个无关的
    // 目标行。
    if let Some(version) =
        supplied.remove(&(DatabaseEntityType::ManagedTarget, target.target_id.clone()))
    {
        if version != target.target_row_version {
            return Err(AppError::stale_preview(
                "adoptHookNative",
                "targetRowVersion",
            ));
        }
    }
    if supplied.keys().any(|(entity_type, entity_id)| {
        *entity_type == DatabaseEntityType::ManagedTarget
            || (*entity_type == DatabaseEntityType::Project
                && target.project_id.as_deref() != Some(entity_id.as_str()))
    }) {
        return Err(AppError::stale_preview("adoptHookNative", "rowVersions"));
    }

    let mut required = std::collections::BTreeMap::new();
    for hook in hooks {
        required.insert(
            (DatabaseEntityType::Hook, hook.id.clone()),
            hook.row_version,
        );
    }
    for item in items {
        required.insert(
            (DatabaseEntityType::ManagedItem, item.id.clone()),
            item.row_version,
        );
    }
    if let Some(project_id) = target.project_id.as_deref() {
        let Some(version) = supplied.get(&(DatabaseEntityType::Project, project_id.to_owned()))
        else {
            return Err(AppError::stale_preview(
                "adoptHookNative",
                "projectRowVersion",
            ));
        };
        // 将 project 条目加入 required map，参与精确集合校验。
        required.insert(
            (DatabaseEntityType::Project, project_id.to_owned()),
            *version,
        );
    }
    if supplied != required {
        return Err(AppError::stale_preview("adoptHookNative", "rowVersions"));
    }
    Ok(supplied)
}

fn verify_hook_row_versions(
    transaction: &rusqlite::Transaction<'_>,
    target: &NativeHookTargetAdoption,
    expected: &std::collections::BTreeMap<(DatabaseEntityType, String), u32>,
    database_path: &str,
) -> Result<(), AppError> {
    if let Some(project_id) = target.project_id.as_deref() {
        let expected = expected
            .get(&(DatabaseEntityType::Project, project_id.to_owned()))
            .copied()
            .ok_or_else(|| AppError::stale_preview("adoptHookNative", "projectRowVersion"))?;
        let actual = transaction
            .query_row(
                "SELECT row_version FROM projects
                 WHERE id = ?1 AND removed_at IS NULL",
                [project_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| {
                AppError::database(database_path, "verify_adopt_native_hook_project")
                    .with_source(error)
            })?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(expected) {
            return Err(AppError::stale_preview(
                "adoptHookNative",
                "projectRowVersion",
            ));
        }
    } else if expected
        .keys()
        .any(|(entity_type, _)| *entity_type == DatabaseEntityType::Project)
    {
        return Err(AppError::stale_preview(
            "adoptHookNative",
            "projectRowVersion",
        ));
    }
    Ok(())
}

fn stale_or_missing_hook(database: &Database, id: &str) -> Result<HookRecord, AppError> {
    match get_hook(database, id) {
        Ok(_) => Err(AppError::conflict("rowVersion", "Hook 已被其他操作修改")),
        Err(error) => Err(error),
    }
}

/// 分配列表行：event 列来自分配表（生效事件）。
fn assigned_hook_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HookRecord> {
    Ok(HookRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        matcher: row.get(2)?,
        command: row.get(3)?,
        timeout_seconds: row
            .get::<_, Option<i64>>(4)?
            .and_then(|value| i32::try_from(value).ok()),
        enabled: row.get(5)?,
        script_name: row.get(6)?,
        script_hash: row.get(7)?,
        row_version: row.get(8)?,
        event: event_from_database(row.get(9)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
    })
}

/// 中央列表行：event 为建议事件（hooks.event）。
fn hook_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HookRecord> {
    Ok(HookRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        event: HookEvent::from_stable_str(&row.get::<_, String>(2)?)
            .ok_or(rusqlite::Error::InvalidQuery)?,
        matcher: row.get(3)?,
        command: row.get(4)?,
        timeout_seconds: row
            .get::<_, Option<i64>>(5)?
            .and_then(|value| i32::try_from(value).ok()),
        enabled: row.get(6)?,
        script_name: row.get(7)?,
        script_hash: row.get(8)?,
        row_version: row.get(9)?,
    })
}

pub(super) fn map_hook_write_error(
    error: rusqlite::Error,
    database_path: &str,
    operation: &'static str,
) -> AppError {
    let text = error.to_string();
    let app_error = if text.contains("UNIQUE constraint failed: hooks.name") {
        AppError::conflict("name", "Hook 名称已存在（不区分大小写）")
    } else if text.contains("FOREIGN KEY constraint failed") {
        AppError::conflict("assignment", "Hook 仍有全局或项目分配，不能删除")
    } else if text.contains("GLOBAL_ASSIGNMENT_INHERITED")
        || text.contains("PROJECT_ASSIGNMENT_EXISTS")
    {
        AppError::conflict("assignment", "全局继承与项目分配不能重复")
    } else {
        AppError::database(database_path, operation)
    };
    app_error.with_source(error)
}
