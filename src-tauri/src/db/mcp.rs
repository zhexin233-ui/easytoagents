//! MCP 中央记录、全局/项目分配与 managed item 基线仓储。

use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::{
    db::Database,
    domain::{
        validate_global_assignment, validate_project_assignment, EntityId, McpTransport, Tool,
        TrustStatus,
    },
    error::AppError,
    mcp::ValidatedMcpConfiguration,
};

#[derive(Debug, Clone, PartialEq)]
pub struct McpServerRecord {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args_json: String,
    pub url: Option<String>,
    pub headers_json: String,
    pub env_json: String,
    pub extra_json: String,
    pub enabled: bool,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProjectRecord {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub codex_trust_status: TrustStatus,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedMcpItemRecord {
    pub id: String,
    pub resource_id: String,
    pub external_key: String,
    pub last_applied_item_hash: String,
    pub row_version: i64,
}

pub fn list_mcp_servers(database: &Database) -> Result<Vec<McpServerRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, name, transport, command, args_json, url, headers_json,
                    env_json, extra_json, enabled, row_version
             FROM mcp_servers ORDER BY name COLLATE NOCASE, id",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_mcp_servers"))?;
    let records = statement
        .query_map([], mcp_from_row)
        .map_err(|_| AppError::database(&path, "query_list_mcp_servers"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_mcp_servers"))?;
    Ok(records)
}

pub fn get_mcp_server(database: &Database, id: &str) -> Result<McpServerRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, name, transport, command, args_json, url, headers_json,
                    env_json, extra_json, enabled, row_version
             FROM mcp_servers WHERE id = ?1",
            [id],
            mcp_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&path, "get_mcp_server"))?
        .ok_or_else(|| AppError::not_found("mcpServer", id))
}

pub(crate) fn insert_mcp_server(
    database: &mut Database,
    value: &ValidatedMcpConfiguration,
) -> Result<McpServerRecord, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let id = insert_mcp_configuration(database.connection(), value, &path)?;
    get_mcp_server(database, &id)
}

pub(super) fn insert_mcp_configuration(
    connection: &rusqlite::Connection,
    value: &ValidatedMcpConfiguration,
    path: &str,
) -> Result<String, AppError> {
    let id = EntityId::new().to_string();
    let json = serialize_configuration_json(value)?;
    connection
        .execute(
            "INSERT INTO mcp_servers(
                id, name, transport, command, args_json, url, headers_json,
                env_json, extra_json, enabled
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                value.name,
                value.transport.as_str(),
                value.command,
                json.args,
                value.url,
                json.headers,
                json.env,
                json.extra,
                value.enabled,
            ],
        )
        .map_err(|error| map_mcp_write_error(error, path, "insert_mcp_server"))?;
    Ok(id)
}

pub(crate) fn update_mcp_server(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
    value: &ValidatedMcpConfiguration,
) -> Result<McpServerRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let json = serialize_configuration_json(value)?;
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE mcp_servers
             SET name = ?2, transport = ?3, command = ?4, args_json = ?5,
                 url = ?6, headers_json = ?7, env_json = ?8, extra_json = ?9,
                 enabled = ?10
             WHERE id = ?1 AND row_version = ?11",
            params![
                id,
                value.name,
                value.transport.as_str(),
                value.command,
                json.args,
                value.url,
                json.headers,
                json.env,
                json.extra,
                value.enabled,
                expected_row_version,
            ],
        )
        .map_err(|error| map_mcp_write_error(error, &path, "update_mcp_server"))?;
    if updated != 1 {
        return stale_or_missing_mcp(database, id);
    }
    get_mcp_server(database, id)
}

pub fn set_mcp_enabled(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
    enabled: bool,
) -> Result<McpServerRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let updated = database
        .connection_mut()
        .execute(
            "UPDATE mcp_servers SET enabled = ?2 WHERE id = ?1 AND row_version = ?3",
            params![id, enabled, expected_row_version],
        )
        .map_err(|error| map_mcp_write_error(error, &path, "set_mcp_enabled"))?;
    if updated != 1 {
        return stale_or_missing_mcp(database, id);
    }
    get_mcp_server(database, id)
}

pub fn delete_mcp_server(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
) -> Result<(), AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let deleted = database
        .connection_mut()
        .execute(
            "DELETE FROM mcp_servers WHERE id = ?1 AND row_version = ?2",
            params![id, expected_row_version],
        )
        .map_err(|error| map_mcp_write_error(error, &path, "delete_mcp_server"))?;
    if deleted != 1 {
        return stale_or_missing_mcp(database, id).map(drop);
    }
    Ok(())
}

pub fn global_tools_for_mcp(database: &Database, mcp_id: &str) -> Result<Vec<Tool>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT tool FROM mcp_global_assignments
             WHERE mcp_id = ?1 ORDER BY tool",
        )
        .map_err(|_| AppError::database(&path, "prepare_mcp_global_tools"))?;
    let tools = statement
        .query_map([mcp_id], |row| tool_from_database(row.get(0)?))
        .map_err(|_| AppError::database(&path, "query_mcp_global_tools"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_mcp_global_tools"))?;
    Ok(tools)
}

pub fn set_global_assignment(
    database: &mut Database,
    tool: Tool,
    mcp_id: &str,
    assigned: bool,
    expected_row_version: u32,
) -> Result<McpServerRecord, AppError> {
    EntityId::parse(mcp_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&path, "begin_set_mcp_global_assignment"))?;
    verify_row_version(
        &transaction,
        "mcp_servers",
        mcp_id,
        expected_row_version,
        "mcpServer",
        &path,
    )?;
    let changed = if assigned {
        let project_count = transaction
            .query_row(
                "SELECT COUNT(*) FROM mcp_project_assignments
                 WHERE tool = ?1 AND mcp_id = ?2",
                params![tool.as_str(), mcp_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| AppError::database(&path, "count_mcp_project_assignments"))?;
        validate_global_assignment(project_count > 0)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO mcp_global_assignments(tool, mcp_id) VALUES (?1, ?2)",
                params![tool.as_str(), mcp_id],
            )
            .map_err(|error| map_mcp_write_error(error, &path, "insert_mcp_global_assignment"))?
    } else {
        transaction
            .execute(
                "DELETE FROM mcp_global_assignments WHERE tool = ?1 AND mcp_id = ?2",
                params![tool.as_str(), mcp_id],
            )
            .map_err(|_| AppError::database(&path, "delete_mcp_global_assignment"))?
    };
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "mcp_servers",
            mcp_id,
            expected_row_version,
            &path,
        )?;
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&path, "commit_set_mcp_global_assignment"))?;
    get_mcp_server(database, mcp_id)
}

#[allow(clippy::too_many_arguments)]
pub fn set_project_assignment(
    database: &mut Database,
    project_id: &str,
    tool: Tool,
    mcp_id: &str,
    assigned: bool,
    expected_mcp_row_version: u32,
    expected_project_row_version: u32,
) -> Result<McpServerRecord, AppError> {
    EntityId::parse(project_id)?;
    EntityId::parse(mcp_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&path, "begin_set_mcp_project_assignment"))?;
    verify_row_version(
        &transaction,
        "mcp_servers",
        mcp_id,
        expected_mcp_row_version,
        "mcpServer",
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
                SELECT 1 FROM mcp_global_assignments
                WHERE tool = ?1 AND mcp_id = ?2
             )",
            params![tool.as_str(), mcp_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::database(&path, "read_mcp_global_assignment"))?;
    // 全局项在项目层是只读继承：不仅禁止重复添加，也禁止通过伪造 RPC 请求
    // 把一个不存在的项目 assignment 当作“禁用全局项”移除。
    validate_project_assignment(globally_assigned)?;
    let changed = if assigned {
        transaction
            .execute(
                "INSERT OR IGNORE INTO mcp_project_assignments(project_id, tool, mcp_id)
                 VALUES (?1, ?2, ?3)",
                params![project_id, tool.as_str(), mcp_id],
            )
            .map_err(|error| map_mcp_write_error(error, &path, "insert_mcp_project_assignment"))?
    } else {
        transaction
            .execute(
                "DELETE FROM mcp_project_assignments
                 WHERE project_id = ?1 AND tool = ?2 AND mcp_id = ?3",
                params![project_id, tool.as_str(), mcp_id],
            )
            .map_err(|_| AppError::database(&path, "delete_mcp_project_assignment"))?
    };
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "mcp_servers",
            mcp_id,
            expected_mcp_row_version,
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
    transaction
        .commit()
        .map_err(|_| AppError::database(&path, "commit_set_mcp_project_assignment"))?;
    get_mcp_server(database, mcp_id)
}

pub fn list_assigned_mcp_servers(
    database: &Database,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<Vec<McpServerRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let (sql, project_parameter) = match project_id {
        Some(project_id) => (
            "SELECT server.id, server.name, server.transport, server.command,
                    server.args_json, server.url, server.headers_json, server.env_json,
                    server.extra_json, server.enabled, server.row_version
             FROM mcp_servers AS server
             JOIN mcp_project_assignments AS assignment ON assignment.mcp_id = server.id
             WHERE assignment.project_id = ?1 AND assignment.tool = ?2
             ORDER BY server.name COLLATE NOCASE, server.id",
            Some(project_id),
        ),
        None => (
            "SELECT server.id, server.name, server.transport, server.command,
                    server.args_json, server.url, server.headers_json, server.env_json,
                    server.extra_json, server.enabled, server.row_version
             FROM mcp_servers AS server
             JOIN mcp_global_assignments AS assignment ON assignment.mcp_id = server.id
             WHERE assignment.tool = ?2
             ORDER BY server.name COLLATE NOCASE, server.id",
            None,
        ),
    };
    let mut statement = database
        .connection()
        .prepare(sql)
        .map_err(|_| AppError::database(&path, "prepare_list_assigned_mcp"))?;
    let records = statement
        .query_map(params![project_parameter, tool.as_str()], mcp_from_row)
        .map_err(|_| AppError::database(&path, "query_list_assigned_mcp"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_assigned_mcp"))?;
    Ok(records)
}

pub fn list_projects(database: &Database) -> Result<Vec<McpProjectRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, display_name, root_path, codex_trust_status, row_version
             FROM projects
             WHERE removed_at IS NULL
             ORDER BY display_name COLLATE NOCASE, root_path",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_mcp_projects"))?;
    let projects = statement
        .query_map([], project_from_row)
        .map_err(|_| AppError::database(&path, "query_list_mcp_projects"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_mcp_projects"))?;
    Ok(projects)
}

pub fn get_project(database: &Database, id: &str) -> Result<McpProjectRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, display_name, root_path, codex_trust_status, row_version
             FROM projects WHERE id = ?1 AND removed_at IS NULL",
            [id],
            project_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&path, "get_mcp_project"))?
        .ok_or_else(|| AppError::not_found("project", id))
}

pub fn project_assignment_exists(
    database: &Database,
    project_id: &str,
    tool: Tool,
    mcp_id: &str,
) -> Result<bool, AppError> {
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM mcp_project_assignments
                WHERE project_id = ?1 AND tool = ?2 AND mcp_id = ?3
             )",
            params![project_id, tool.as_str(), mcp_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::database(&path, "read_mcp_project_assignment"))
}

pub fn list_managed_mcp_items(
    database: &Database,
    target_id: &str,
) -> Result<Vec<ManagedMcpItemRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, resource_id, external_key, last_applied_item_hash, row_version
             FROM managed_items
             WHERE target_id = ?1 AND resource_kind = 'mcp'
             ORDER BY external_key COLLATE NOCASE, id",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_managed_mcp_items"))?;
    let items = statement
        .query_map([target_id], |row| {
            Ok(ManagedMcpItemRecord {
                id: row.get(0)?,
                resource_id: row.get(1)?,
                external_key: row.get(2)?,
                last_applied_item_hash: row.get(3)?,
                row_version: row.get(4)?,
            })
        })
        .map_err(|_| AppError::database(&path, "query_list_managed_mcp_items"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_managed_mcp_items"))?;
    Ok(items)
}

fn stale_or_missing_mcp(database: &Database, id: &str) -> Result<McpServerRecord, AppError> {
    match get_mcp_server(database, id) {
        Ok(_) => Err(AppError::conflict("rowVersion", "MCP 已被其他操作修改")),
        Err(error) => Err(error),
    }
}

pub(crate) fn verify_row_version(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
    id: &str,
    expected: u32,
    resource: &'static str,
    database_path: &str,
) -> Result<(), AppError> {
    let sql = format!("SELECT row_version FROM {table} WHERE id = ?1");
    let actual = transaction
        .query_row(&sql, [id], |row| row.get::<_, i64>(0))
        .optional()
        .map_err(|_| AppError::database(database_path, "verify_assignment_row_version"))?
        .ok_or_else(|| AppError::not_found(resource, id))?;
    if u32::try_from(actual).ok() != Some(expected) {
        return Err(AppError::conflict(
            "rowVersion",
            "分配依赖的记录已被其他操作修改",
        ));
    }
    Ok(())
}

pub(crate) fn touch_versioned_row(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
    id: &str,
    expected: u32,
    database_path: &str,
) -> Result<(), AppError> {
    let sql =
        format!("UPDATE {table} SET updated_at = updated_at WHERE id = ?1 AND row_version = ?2");
    let updated = transaction
        .execute(&sql, params![id, expected])
        .map_err(|_| AppError::database(database_path, "touch_assignment_owner"))?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "分配依赖的记录已被其他操作修改",
        ));
    }
    Ok(())
}

struct SerializedConfiguration {
    args: String,
    headers: String,
    env: String,
    extra: String,
}

fn serialize_configuration_json(
    value: &ValidatedMcpConfiguration,
) -> Result<SerializedConfiguration, AppError> {
    Ok(SerializedConfiguration {
        args: serde_json::to_string(&value.args)
            .map_err(|_| AppError::invalid_input("args", "args 无法序列化"))?,
        headers: serde_json::to_string(&value.headers)
            .map_err(|_| AppError::invalid_input("headers", "headers 无法序列化"))?,
        env: serde_json::to_string(&value.env)
            .map_err(|_| AppError::invalid_input("env", "env 无法序列化"))?,
        extra: serde_json::to_string(&value.extra)
            .map_err(|_| AppError::invalid_input("extra", "extra 无法序列化"))?,
    })
}

fn mcp_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpServerRecord> {
    Ok(McpServerRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        transport: transport_from_database(row.get(2)?)?,
        command: row.get(3)?,
        args_json: row.get(4)?,
        url: row.get(5)?,
        headers_json: row.get(6)?,
        env_json: row.get(7)?,
        extra_json: row.get(8)?,
        enabled: row.get(9)?,
        row_version: row.get(10)?,
    })
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpProjectRecord> {
    Ok(McpProjectRecord {
        id: row.get(0)?,
        display_name: row.get(1)?,
        root_path: row.get(2)?,
        codex_trust_status: trust_from_database(row.get(3)?)?,
        row_version: row.get(4)?,
    })
}

fn transport_from_database(value: String) -> rusqlite::Result<McpTransport> {
    match value.as_str() {
        "stdio" => Ok(McpTransport::Stdio),
        "streamable_http" => Ok(McpTransport::StreamableHttp),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
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

fn trust_from_database(value: String) -> rusqlite::Result<TrustStatus> {
    match value.as_str() {
        "unknown" => Ok(TrustStatus::Unknown),
        "trusted" => Ok(TrustStatus::Trusted),
        "untrusted" => Ok(TrustStatus::Untrusted),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

pub(super) fn map_mcp_write_error(
    error: rusqlite::Error,
    database_path: &str,
    operation: &'static str,
) -> AppError {
    let text = error.to_string();
    if text.contains("UNIQUE constraint failed: mcp_servers.name") {
        AppError::conflict("name", "MCP 名称已存在（不区分大小写）")
    } else if text.contains("FOREIGN KEY constraint failed") {
        AppError::conflict("assignment", "MCP 仍有全局或项目分配，不能删除")
    } else if text.contains("GLOBAL_ASSIGNMENT_INHERITED")
        || text.contains("PROJECT_ASSIGNMENT_EXISTS")
    {
        AppError::conflict("assignment", "全局继承与项目分配不能重复")
    } else {
        AppError::database(database_path, operation)
    }
}
