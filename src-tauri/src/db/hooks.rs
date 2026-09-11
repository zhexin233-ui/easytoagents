//! Hooks 中央记录、全局/项目分配与 managed item 基线仓储。

use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::{
    db::{
        mcp::{touch_versioned_row, verify_row_version},
        Database,
    },
    domain::{validate_global_assignment, validate_project_assignment, EntityId, HookEvent, Tool},
    error::AppError,
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
                tool_from_database(row.get(1)?)?,
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

pub(crate) fn hook_row_versions(
    database: &Database,
    ids: &[&str],
) -> Result<std::collections::BTreeMap<String, i64>, AppError> {
    super::skills::row_versions_by_id(
        database.connection(),
        &database.path().to_string_lossy(),
        "hooks",
        ids,
        "hook_row_versions",
    )
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
                tool_from_database(row.get(0)?)?,
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

fn event_from_database(value: String) -> rusqlite::Result<HookEvent> {
    HookEvent::from_stable_str(&value).ok_or(rusqlite::Error::InvalidQuery)
}

fn tool_from_database(value: String) -> rusqlite::Result<Tool> {
    match value.as_str() {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        "cursor" => Ok(Tool::Cursor),
        "zcode" => Ok(Tool::Zcode),
        _ => Err(rusqlite::Error::InvalidQuery),
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
