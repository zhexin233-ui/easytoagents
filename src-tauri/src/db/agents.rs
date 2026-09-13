//! Agents 中央记录、全局/项目分配与受管目标查询仓储（镜像 db/hooks.rs）。
//!
//! Agent 是整文件目标（WholeDocument），不使用 managed_items 基线；
//! 受管目标按「一个受管文件 = 一行 managed_targets」登记。

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::Value;

use crate::{
    db::{
        column_tool,
        mcp::{touch_versioned_row, verify_row_version},
        Database,
    },
    domain::{
        tool_capabilities, validate_global_assignment, validate_project_assignment, EntityId, Tool,
    },
    error::AppError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub enabled: bool,
    pub row_version: i64,
}

/// 按查询键定位的 agent 受管目标行（managed_targets.artifact_kind = 'agent'）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentManagedTargetRecord {
    pub id: String,
    pub target_path: String,
    pub baseline_full_hash: Option<String>,
    pub baseline_managed_hash: Option<String>,
    pub last_status: String,
    pub row_version: i64,
}

const AGENT_COLUMNS: &str = "id, name, description, prompt, enabled, row_version";

pub fn list_agents(database: &Database) -> Result<Vec<AgentRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(&format!(
            "SELECT {AGENT_COLUMNS} FROM agents ORDER BY name COLLATE NOCASE, id"
        ))
        .map_err(|error| AppError::database(&path, "prepare_list_agents").with_source(error))?;
    let records = statement
        .query_map([], agent_from_row)
        .map_err(|error| AppError::database(&path, "query_list_agents").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database(&path, "decode_list_agents").with_source(error))?;
    Ok(records)
}

pub fn get_agent(database: &Database, id: &str) -> Result<AgentRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            &format!("SELECT {AGENT_COLUMNS} FROM agents WHERE id = ?1"),
            [id],
            agent_from_row,
        )
        .optional()
        .map_err(|error| AppError::database(&path, "get_agent").with_source(error))?
        .ok_or_else(|| AppError::not_found("agent", id))
}

/// 校验后的中央 Agent 定义；由服务层构造，仓储层不重复业务校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedAgentDefinition {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub enabled: bool,
}

/// 插入中央 Agent。`id` 由服务层生成，保持与 hooks 一致的插入合同。
pub(crate) fn insert_agent(
    database: &mut Database,
    id: &str,
    value: &ValidatedAgentDefinition,
) -> Result<AgentRecord, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    EntityId::parse(id)?;
    database
        .connection_mut()
        .execute(
            "INSERT INTO agents(id, name, description, prompt, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                value.name,
                value.description,
                value.prompt,
                value.enabled
            ],
        )
        .map_err(|error| map_agent_write_error(error, &path, "insert_agent"))?;
    get_agent(database, id)
}

pub(crate) fn update_agent(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
    value: &ValidatedAgentDefinition,
) -> Result<AgentRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE agents
             SET name = ?2, description = ?3, prompt = ?4, enabled = ?5
             WHERE id = ?1 AND row_version = ?6",
            params![
                id,
                value.name,
                value.description,
                value.prompt,
                value.enabled,
                expected_row_version
            ],
        )
        .map_err(|error| map_agent_write_error(error, &path, "update_agent"))?;
    if updated != 1 {
        return stale_or_missing_agent(database, id);
    }
    get_agent(database, id)
}

pub fn set_agent_enabled(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
    enabled: bool,
) -> Result<AgentRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE agents SET enabled = ?2 WHERE id = ?1 AND row_version = ?3",
            params![id, enabled, expected_row_version],
        )
        .map_err(|error| map_agent_write_error(error, &path, "set_agent_enabled"))?;
    if updated != 1 {
        return stale_or_missing_agent(database, id);
    }
    get_agent(database, id)
}

pub fn delete_agent(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
) -> Result<(), AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let deleted = database
        .connection_mut()
        .execute(
            "DELETE FROM agents WHERE id = ?1 AND row_version = ?2",
            params![id, expected_row_version],
        )
        .map_err(|error| map_agent_write_error(error, &path, "delete_agent"))?;
    if deleted != 1 {
        return stale_or_missing_agent(database, id).map(drop);
    }
    Ok(())
}

/// 一条语句取回全部 Agent 的全局分配工具，供列表接口一次组装。
pub fn global_assignments_for_all_agents(
    database: &Database,
) -> Result<std::collections::BTreeMap<String, Vec<Tool>>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT agent_id, tool FROM agent_global_assignments ORDER BY agent_id, tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_all_agent_global_tools").with_source(error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, agent_tool_from_row(row, 1)?))
        })
        .map_err(|error| {
            AppError::database(&path, "query_all_agent_global_tools").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_all_agent_global_tools").with_source(error)
        })?;
    let mut grouped = std::collections::BTreeMap::<String, Vec<Tool>>::new();
    for (agent_id, tool) in rows {
        grouped.entry(agent_id).or_default().push(tool);
    }
    Ok(grouped)
}

pub fn global_assignments_for_agent(
    database: &Database,
    agent_id: &str,
) -> Result<Vec<Tool>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT tool FROM agent_global_assignments WHERE agent_id = ?1 ORDER BY tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_agent_global_tools").with_source(error)
        })?;
    let assignments = statement
        .query_map([agent_id], |row| agent_tool_from_row(row, 0))
        .map_err(|error| AppError::database(&path, "query_agent_global_tools").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_agent_global_tools").with_source(error)
        })?;
    Ok(assignments)
}

/// 返回单个 Agent 的全部工具覆盖层。数据库内容已经由迁移的 JSON CHECK
/// 约束保证为对象，但读取仍需显式解析，损坏数据不得静默降级为空设置。
pub fn tool_settings_for_agent(
    database: &Database,
    agent_id: &str,
) -> Result<BTreeMap<Tool, Value>, AppError> {
    EntityId::parse(agent_id)?;
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT tool, settings_json FROM agent_tool_settings
             WHERE agent_id = ?1 ORDER BY tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_agent_tool_settings").with_source(error)
        })?;
    let rows = statement
        .query_map([agent_id], |row| {
            let tool = column_tool(row, 0)?;
            let json = row.get::<_, String>(1)?;
            let value =
                serde_json::from_str::<Value>(&json).map_err(|_| rusqlite::Error::InvalidQuery)?;
            if !value.is_object() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok((tool, value))
        })
        .map_err(|error| {
            AppError::database(&path, "query_agent_tool_settings").with_source(error)
        })?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|error| AppError::database(&path, "decode_agent_tool_settings").with_source(error))
}

/// 列表接口一次获取所有 Agent 的覆盖层，避免在 DTO map 中逐 id 查询。
pub fn tool_settings_for_all_agents(
    database: &Database,
) -> Result<BTreeMap<String, BTreeMap<Tool, Value>>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT agent_id, tool, settings_json FROM agent_tool_settings
             ORDER BY agent_id, tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_all_agent_tool_settings").with_source(error)
        })?;
    let rows = statement
        .query_map([], |row| {
            let agent_id = row.get::<_, String>(0)?;
            let tool = column_tool(row, 1)?;
            let json = row.get::<_, String>(2)?;
            let value =
                serde_json::from_str::<Value>(&json).map_err(|_| rusqlite::Error::InvalidQuery)?;
            if !value.is_object() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok((agent_id, tool, value))
        })
        .map_err(|error| {
            AppError::database(&path, "query_all_agent_tool_settings").with_source(error)
        })?;
    let mut grouped = BTreeMap::new();
    for row in rows {
        let (agent_id, tool, value) = row.map_err(|error| {
            AppError::database(&path, "decode_all_agent_tool_settings").with_source(error)
        })?;
        grouped
            .entry(agent_id)
            .or_insert_with(BTreeMap::new)
            .insert(tool, value);
    }
    Ok(grouped)
}

pub fn upsert_tool_settings(
    database: &mut Database,
    agent_id: &str,
    tool: Tool,
    settings_json: &str,
    expected_row_version: u32,
) -> Result<AgentRecord, AppError> {
    EntityId::parse(agent_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&path, "begin_upsert_agent_tool_settings").with_source(error)
        })?;
    verify_row_version(
        &transaction,
        "agents",
        agent_id,
        expected_row_version,
        "agent",
        &path,
    )?;
    transaction
        .execute(
            "INSERT INTO agent_tool_settings(agent_id, tool, settings_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(agent_id, tool) DO UPDATE SET
               settings_json = excluded.settings_json,
               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![agent_id, tool.as_str(), settings_json],
        )
        .map_err(|error| map_agent_write_error(error, &path, "upsert_agent_tool_settings"))?;
    touch_versioned_row(
        &transaction,
        "agents",
        agent_id,
        expected_row_version,
        &path,
    )?;
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_upsert_agent_tool_settings").with_source(error)
    })?;
    get_agent(database, agent_id)
}

pub fn delete_tool_settings(
    database: &mut Database,
    agent_id: &str,
    tool: Tool,
    expected_row_version: u32,
) -> Result<AgentRecord, AppError> {
    EntityId::parse(agent_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&path, "begin_delete_agent_tool_settings").with_source(error)
        })?;
    verify_row_version(
        &transaction,
        "agents",
        agent_id,
        expected_row_version,
        "agent",
        &path,
    )?;
    let changed = transaction
        .execute(
            "DELETE FROM agent_tool_settings WHERE agent_id = ?1 AND tool = ?2",
            params![agent_id, tool.as_str()],
        )
        .map_err(|error| {
            AppError::database(&path, "delete_agent_tool_settings").with_source(error)
        })?;
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "agents",
            agent_id,
            expected_row_version,
            &path,
        )?;
    }
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_delete_agent_tool_settings").with_source(error)
    })?;
    get_agent(database, agent_id)
}

/// Agent 分配行只允许支持 Agents 的工具；写入侧由服务层门禁，读取侧同样
/// fail-closed，避免历史脏数据被静默当成合法分配。
fn agent_tool_from_row(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Tool> {
    let tool = column_tool(row, index)?;
    if tool_capabilities()
        .iter()
        .any(|capability| capability.tool == tool && capability.agents)
    {
        Ok(tool)
    } else {
        Err(rusqlite::Error::InvalidQuery)
    }
}

pub fn set_global_assignment(
    database: &mut Database,
    tool: Tool,
    agent_id: &str,
    assigned: bool,
    expected_row_version: u32,
) -> Result<AgentRecord, AppError> {
    EntityId::parse(agent_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&path, "begin_set_agent_global_assignment").with_source(error)
        })?;
    verify_row_version(
        &transaction,
        "agents",
        agent_id,
        expected_row_version,
        "agent",
        &path,
    )?;
    let changed = if assigned {
        let project_count = transaction
            .query_row(
                "SELECT COUNT(*) FROM agent_project_assignments
                 WHERE tool = ?1 AND agent_id = ?2",
                params![tool.as_str(), agent_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                AppError::database(&path, "count_agent_project_assignments").with_source(error)
            })?;
        validate_global_assignment(project_count > 0)?;
        transaction
            .execute(
                "INSERT INTO agent_global_assignments(tool, agent_id) VALUES (?1, ?2)
                 ON CONFLICT(tool, agent_id) DO NOTHING",
                params![tool.as_str(), agent_id],
            )
            .map_err(|error| {
                map_agent_write_error(error, &path, "insert_agent_global_assignment")
            })?
    } else {
        transaction
            .execute(
                "DELETE FROM agent_global_assignments WHERE tool = ?1 AND agent_id = ?2",
                params![tool.as_str(), agent_id],
            )
            .map_err(|error| {
                AppError::database(&path, "delete_agent_global_assignment").with_source(error)
            })?
    };
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "agents",
            agent_id,
            expected_row_version,
            &path,
        )?;
    }
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_set_agent_global_assignment").with_source(error)
    })?;
    get_agent(database, agent_id)
}

#[allow(clippy::too_many_arguments)]
pub fn set_project_assignment(
    database: &mut Database,
    project_id: &str,
    tool: Tool,
    agent_id: &str,
    assigned: bool,
    expected_agent_row_version: u32,
    expected_project_row_version: u32,
) -> Result<AgentRecord, AppError> {
    EntityId::parse(project_id)?;
    EntityId::parse(agent_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&path, "begin_set_agent_project_assignment").with_source(error)
        })?;
    verify_row_version(
        &transaction,
        "agents",
        agent_id,
        expected_agent_row_version,
        "agent",
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
                SELECT 1 FROM agent_global_assignments
                WHERE tool = ?1 AND agent_id = ?2
             )",
            params![tool.as_str(), agent_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            AppError::database(&path, "read_agent_global_assignment").with_source(error)
        })?;
    // 全局项在项目层是只读继承：不仅禁止重复添加，也禁止通过伪造 RPC 请求
    // 把一个不存在的项目 assignment 当作"禁用全局项"移除。
    validate_project_assignment(globally_assigned)?;
    let changed = if assigned {
        transaction
            .execute(
                "INSERT INTO agent_project_assignments(project_id, tool, agent_id)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(project_id, tool, agent_id) DO NOTHING",
                params![project_id, tool.as_str(), agent_id],
            )
            .map_err(|error| {
                map_agent_write_error(error, &path, "insert_agent_project_assignment")
            })?
    } else {
        transaction
            .execute(
                "DELETE FROM agent_project_assignments
                 WHERE project_id = ?1 AND tool = ?2 AND agent_id = ?3",
                params![project_id, tool.as_str(), agent_id],
            )
            .map_err(|error| {
                AppError::database(&path, "delete_agent_project_assignment").with_source(error)
            })?
    };
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "agents",
            agent_id,
            expected_agent_row_version,
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
        AppError::database(&path, "commit_set_agent_project_assignment").with_source(error)
    })?;
    get_agent(database, agent_id)
}

/// 列出分配给某工具的 Agent；`project_id` 为 None 时取全局分配，
/// 为 Some 时取该项目自己的分配（全局继承由服务层另行查询）。
pub fn list_assigned_agents(
    database: &Database,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<Vec<AgentRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let (sql, project_parameter) = match project_id {
        Some(project_id) => (
            "SELECT agent.id, agent.name, agent.description, agent.prompt,
                    agent.enabled, agent.row_version
             FROM agents AS agent
             JOIN agent_project_assignments AS assignment ON assignment.agent_id = agent.id
             WHERE assignment.project_id = ?1 AND assignment.tool = ?2
             ORDER BY agent.name COLLATE NOCASE, agent.id",
            Some(project_id),
        ),
        None => (
            "SELECT agent.id, agent.name, agent.description, agent.prompt,
                    agent.enabled, agent.row_version
             FROM agents AS agent
             JOIN agent_global_assignments AS assignment ON assignment.agent_id = agent.id
             WHERE assignment.tool = ?2
             ORDER BY agent.name COLLATE NOCASE, agent.id",
            None,
        ),
    };
    let mut statement = database.connection().prepare_cached(sql).map_err(|error| {
        AppError::database(&path, "prepare_list_assigned_agents").with_source(error)
    })?;
    let records = statement
        .query_map(params![project_parameter, tool.as_str()], agent_from_row)
        .map_err(|error| {
            AppError::database(&path, "query_list_assigned_agents").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_list_assigned_agents").with_source(error)
        })?;
    Ok(records)
}

/// 按 `(tool, scope, project_id)` 列出 agent 受管目标行；`project_id` 为 None
/// 时匹配全局目标行。供删除列表与状态聚合复用。
pub fn list_agent_managed_targets(
    database: &Database,
    tool: Tool,
    scope: crate::domain::Scope,
    project_id: Option<&str>,
) -> Result<Vec<AgentManagedTargetRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT id, target_path, baseline_full_hash, baseline_managed_hash,
                    last_status, row_version
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = 'agent' AND scope = ?2
               AND ifnull(project_id, '') = ifnull(?3, '')
             ORDER BY target_path, id",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_list_agent_managed_targets").with_source(error)
        })?;
    let rows = statement
        .query_map(params![tool.as_str(), scope.as_str(), project_id], |row| {
            Ok(AgentManagedTargetRecord {
                id: row.get(0)?,
                target_path: row.get(1)?,
                baseline_full_hash: row.get(2)?,
                baseline_managed_hash: row.get(3)?,
                last_status: row.get(4)?,
                row_version: row.get(5)?,
            })
        })
        .map_err(|error| {
            AppError::database(&path, "query_list_agent_managed_targets").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_list_agent_managed_targets").with_source(error)
        })?;
    Ok(rows)
}

fn stale_or_missing_agent(database: &Database, id: &str) -> Result<AgentRecord, AppError> {
    match get_agent(database, id) {
        Ok(_) => Err(AppError::conflict("rowVersion", "Agent 已被其他操作修改")),
        Err(error) => Err(error),
    }
}

fn agent_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRecord> {
    Ok(AgentRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        prompt: row.get(3)?,
        enabled: row.get(4)?,
        row_version: row.get(5)?,
    })
}

pub(super) fn map_agent_write_error(
    error: rusqlite::Error,
    database_path: &str,
    operation: &'static str,
) -> AppError {
    let text = error.to_string();
    let app_error = if text.contains("UNIQUE constraint failed: agents.name") {
        AppError::conflict("name", "Agent 名称已存在（不区分大小写）")
    } else if text.contains("FOREIGN KEY constraint failed") {
        AppError::conflict("assignment", "Agent 仍有全局或项目分配，不能删除")
    } else if text.contains("GLOBAL_ASSIGNMENT_INHERITED")
        || text.contains("PROJECT_ASSIGNMENT_EXISTS")
    {
        AppError::conflict("assignment", "全局继承与项目分配不能重复")
    } else if text.contains("CHECK constraint failed: agents.name") {
        AppError::invalid_input("name", "Agent 名称不符合五工具交集规则")
    } else {
        AppError::database(database_path, operation)
    };
    app_error.with_source(error)
}
