//! Skill 中央记录、分配与逐目标 managed item 仓储。

use crate::{
    db::Database,
    domain::{
        validate_global_assignment, validate_project_assignment, EntityId, SkillStatus, Tool,
        TrustStatus,
    },
    error::AppError,
    skills::PreparedSkillRecord,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

#[derive(Debug, Clone, PartialEq)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub central_path: String,
    pub content_hash: String,
    pub frontmatter_json: String,
    pub status: SkillStatus,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillProjectRecord {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub codex_trust_status: TrustStatus,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedSkillItemRecord {
    pub id: String,
    pub resource_id: String,
    pub external_key: String,
    pub last_applied_item_hash: String,
    pub row_version: i64,
}

pub fn list_skills(database: &Database) -> Result<Vec<SkillRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, name, source_path, central_path, content_hash,
                    frontmatter_json, status, row_version
             FROM skills ORDER BY name COLLATE NOCASE, id",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_skills"))?;
    let records = statement
        .query_map([], skill_from_row)
        .map_err(|_| AppError::database(&path, "query_list_skills"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_skills"))?;
    Ok(records)
}

pub fn get_skill(database: &Database, id: &str) -> Result<SkillRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT id, name, source_path, central_path, content_hash,
                    frontmatter_json, status, row_version
             FROM skills WHERE id = ?1",
            [id],
            skill_from_row,
        )
        .optional()
        .map_err(|_| AppError::database(&path, "get_skill"))?
        .ok_or_else(|| AppError::not_found("skill", id))
}

pub(crate) fn insert_skill(
    database: &mut Database,
    value: &PreparedSkillRecord,
) -> Result<SkillRecord, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&path, "begin_insert_skill"))?;
    let record = insert_skill_in_transaction(&transaction, &path, value)?;
    transaction
        .commit()
        .map_err(|_| AppError::database(&path, "commit_insert_skill"))?;
    Ok(record)
}

/// 调用方持有同一个写事务；批量导入不能逐项提交。
pub(crate) fn insert_skill_in_transaction(
    connection: &rusqlite::Connection,
    path: &str,
    value: &PreparedSkillRecord,
) -> Result<SkillRecord, AppError> {
    EntityId::parse(&value.id)?;
    let frontmatter = serde_json::to_string(&value.frontmatter)
        .map_err(|_| AppError::invalid_input("frontmatter", "Skill frontmatter 无法序列化"))?;
    connection
        .execute(
            "INSERT INTO skills(
                id, name, source_path, central_path, content_hash,
                frontmatter_json, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready')",
            params![
                value.id,
                value.name,
                value.source_path,
                value.central_path,
                value.content_hash,
                frontmatter,
            ],
        )
        .map_err(|error| map_skill_write_error(error, path, "insert_skill"))?;
    let record = connection
        .query_row(
            "SELECT id, name, source_path, central_path, content_hash,
                    frontmatter_json, status, row_version
             FROM skills WHERE id = ?1",
            [&value.id],
            skill_from_row,
        )
        .map_err(|_| AppError::database(path, "read_inserted_skill"))?;
    Ok(record)
}

pub fn ensure_skill_deletable(
    database: &Database,
    id: &str,
    expected_row_version: u32,
) -> Result<SkillRecord, AppError> {
    let record = get_skill(database, id)?;
    if u32::try_from(record.row_version).ok() != Some(expected_row_version) {
        return Err(AppError::conflict("rowVersion", "Skill 已被其他操作修改"));
    }
    let path = database.path().to_string_lossy();
    let assignments = database
        .connection()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM skill_global_assignments WHERE skill_id = ?1) +
                (SELECT COUNT(*) FROM skill_project_assignments WHERE skill_id = ?1)",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| AppError::database(&path, "count_skill_assignments"))?;
    if assignments > 0 {
        return Err(AppError::conflict(
            "assignment",
            "Skill 仍有全局或项目分配，不能移出中央库",
        ));
    }
    let managed_items = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM managed_items
             WHERE resource_kind = 'skill' AND resource_id = ?1",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| AppError::database(&path, "count_applied_skill_links"))?;
    if managed_items > 0 {
        return Err(AppError::conflict(
            "managedItems",
            "Skill 仍有已应用链接，请先 Preview 并 Apply 清理目标",
        ));
    }
    Ok(record)
}

pub(crate) fn delete_skill_record(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
) -> Result<(), AppError> {
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&path, "begin_delete_skill"))?;
    verify_row_version(
        &transaction,
        "skills",
        id,
        expected_row_version,
        "skill",
        &path,
    )?;
    let blockers = transaction
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM skill_global_assignments WHERE skill_id = ?1) +
                (SELECT COUNT(*) FROM skill_project_assignments WHERE skill_id = ?1) +
                (SELECT COUNT(*) FROM managed_items
                 WHERE resource_kind = 'skill' AND resource_id = ?1)",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| AppError::database(&path, "recheck_skill_delete_blockers"))?;
    if blockers > 0 {
        return Err(AppError::conflict(
            "assignment",
            "Skill 仍有分配或已应用链接，不能移出中央库",
        ));
    }
    let deleted = transaction
        .execute(
            "DELETE FROM skills WHERE id = ?1 AND row_version = ?2",
            params![id, expected_row_version],
        )
        .map_err(|error| map_skill_write_error(error, &path, "delete_skill"))?;
    if deleted != 1 {
        return Err(AppError::conflict("rowVersion", "Skill 已被其他操作修改"));
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&path, "commit_delete_skill"))?;
    Ok(())
}

pub fn global_tools_for_skill(database: &Database, skill_id: &str) -> Result<Vec<Tool>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT tool FROM skill_global_assignments
             WHERE skill_id = ?1 ORDER BY tool",
        )
        .map_err(|_| AppError::database(&path, "prepare_skill_global_tools"))?;
    let tools = statement
        .query_map([skill_id], |row| tool_from_database(row.get(0)?))
        .map_err(|_| AppError::database(&path, "query_skill_global_tools"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_skill_global_tools"))?;
    Ok(tools)
}

pub fn set_global_assignment(
    database: &mut Database,
    tool: Tool,
    skill_id: &str,
    assigned: bool,
    expected_row_version: u32,
) -> Result<SkillRecord, AppError> {
    EntityId::parse(skill_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&path, "begin_set_skill_global_assignment"))?;
    verify_row_version(
        &transaction,
        "skills",
        skill_id,
        expected_row_version,
        "skill",
        &path,
    )?;
    let changed = if assigned {
        let project_count = transaction
            .query_row(
                "SELECT COUNT(*) FROM skill_project_assignments
                 WHERE tool = ?1 AND skill_id = ?2",
                params![tool.as_str(), skill_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| AppError::database(&path, "count_skill_project_assignments"))?;
        validate_global_assignment(project_count > 0)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO skill_global_assignments(tool, skill_id) VALUES (?1, ?2)",
                params![tool.as_str(), skill_id],
            )
            .map_err(|error| {
                map_skill_write_error(error, &path, "insert_skill_global_assignment")
            })?
    } else {
        transaction
            .execute(
                "DELETE FROM skill_global_assignments WHERE tool = ?1 AND skill_id = ?2",
                params![tool.as_str(), skill_id],
            )
            .map_err(|_| AppError::database(&path, "delete_skill_global_assignment"))?
    };
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "skills",
            skill_id,
            expected_row_version,
            &path,
        )?;
    }
    transaction
        .commit()
        .map_err(|_| AppError::database(&path, "commit_set_skill_global_assignment"))?;
    get_skill(database, skill_id)
}

#[allow(clippy::too_many_arguments)]
pub fn set_project_assignment(
    database: &mut Database,
    project_id: &str,
    tool: Tool,
    skill_id: &str,
    assigned: bool,
    expected_skill_row_version: u32,
    expected_project_row_version: u32,
) -> Result<SkillRecord, AppError> {
    EntityId::parse(project_id)?;
    EntityId::parse(skill_id)?;
    let path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| AppError::database(&path, "begin_set_skill_project_assignment"))?;
    verify_row_version(
        &transaction,
        "skills",
        skill_id,
        expected_skill_row_version,
        "skill",
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
                SELECT 1 FROM skill_global_assignments
                WHERE tool = ?1 AND skill_id = ?2
             )",
            params![tool.as_str(), skill_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::database(&path, "read_skill_global_assignment"))?;
    validate_project_assignment(globally_assigned)?;
    let changed = if assigned {
        transaction
            .execute(
                "INSERT OR IGNORE INTO skill_project_assignments(project_id, tool, skill_id)
                 VALUES (?1, ?2, ?3)",
                params![project_id, tool.as_str(), skill_id],
            )
            .map_err(|error| {
                map_skill_write_error(error, &path, "insert_skill_project_assignment")
            })?
    } else {
        transaction
            .execute(
                "DELETE FROM skill_project_assignments
                 WHERE project_id = ?1 AND tool = ?2 AND skill_id = ?3",
                params![project_id, tool.as_str(), skill_id],
            )
            .map_err(|_| AppError::database(&path, "delete_skill_project_assignment"))?
    };
    if changed == 1 {
        touch_versioned_row(
            &transaction,
            "skills",
            skill_id,
            expected_skill_row_version,
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
        .map_err(|_| AppError::database(&path, "commit_set_skill_project_assignment"))?;
    get_skill(database, skill_id)
}

pub fn list_assigned_skills(
    database: &Database,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<Vec<SkillRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let (sql, project_parameter) = match project_id {
        Some(project_id) => (
            "SELECT skill.id, skill.name, skill.source_path, skill.central_path,
                    skill.content_hash, skill.frontmatter_json, skill.status, skill.row_version
             FROM skills AS skill
             JOIN skill_project_assignments AS assignment ON assignment.skill_id = skill.id
             WHERE assignment.project_id = ?1 AND assignment.tool = ?2
             ORDER BY skill.name COLLATE NOCASE, skill.id",
            Some(project_id),
        ),
        None => (
            "SELECT skill.id, skill.name, skill.source_path, skill.central_path,
                    skill.content_hash, skill.frontmatter_json, skill.status, skill.row_version
             FROM skills AS skill
             JOIN skill_global_assignments AS assignment ON assignment.skill_id = skill.id
             WHERE assignment.tool = ?2
             ORDER BY skill.name COLLATE NOCASE, skill.id",
            None,
        ),
    };
    let mut statement = database
        .connection()
        .prepare(sql)
        .map_err(|_| AppError::database(&path, "prepare_list_assigned_skills"))?;
    let records = statement
        .query_map(params![project_parameter, tool.as_str()], skill_from_row)
        .map_err(|_| AppError::database(&path, "query_list_assigned_skills"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_assigned_skills"))?;
    Ok(records)
}

pub fn list_projects(database: &Database) -> Result<Vec<SkillProjectRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, display_name, root_path, codex_trust_status, row_version
             FROM projects
             WHERE removed_at IS NULL
             ORDER BY display_name COLLATE NOCASE, root_path",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_skill_projects"))?;
    let projects = statement
        .query_map([], project_from_row)
        .map_err(|_| AppError::database(&path, "query_list_skill_projects"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_skill_projects"))?;
    Ok(projects)
}

pub fn get_project(database: &Database, id: &str) -> Result<SkillProjectRecord, AppError> {
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
        .map_err(|_| AppError::database(&path, "get_skill_project"))?
        .ok_or_else(|| AppError::not_found("project", id))
}

pub fn list_managed_skill_items(
    database: &Database,
    target_id: &str,
) -> Result<Vec<ManagedSkillItemRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, resource_id, external_key, last_applied_item_hash, row_version
             FROM managed_items
             WHERE target_id = ?1 AND resource_kind = 'skill'
             ORDER BY external_key COLLATE NOCASE, id",
        )
        .map_err(|_| AppError::database(&path, "prepare_list_managed_skill_items"))?;
    let items = statement
        .query_map([target_id], |row| {
            Ok(ManagedSkillItemRecord {
                id: row.get(0)?,
                resource_id: row.get(1)?,
                external_key: row.get(2)?,
                last_applied_item_hash: row.get(3)?,
                row_version: row.get(4)?,
            })
        })
        .map_err(|_| AppError::database(&path, "query_list_managed_skill_items"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&path, "decode_list_managed_skill_items"))?;
    Ok(items)
}

fn verify_row_version(
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
        .map_err(|_| AppError::database(database_path, "verify_skill_assignment_row_version"))?
        .ok_or_else(|| AppError::not_found(resource, id))?;
    if u32::try_from(actual).ok() != Some(expected) {
        return Err(AppError::conflict(
            "rowVersion",
            "分配依赖的记录已被其他操作修改",
        ));
    }
    Ok(())
}

fn touch_versioned_row(
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
        .map_err(|_| AppError::database(database_path, "touch_skill_assignment_owner"))?;
    if updated != 1 {
        return Err(AppError::conflict(
            "rowVersion",
            "分配依赖的记录已被其他操作修改",
        ));
    }
    Ok(())
}

fn skill_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        source_path: row.get(2)?,
        central_path: row.get(3)?,
        content_hash: row.get(4)?,
        frontmatter_json: row.get(5)?,
        status: status_from_database(row.get(6)?)?,
        row_version: row.get(7)?,
    })
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillProjectRecord> {
    Ok(SkillProjectRecord {
        id: row.get(0)?,
        display_name: row.get(1)?,
        root_path: row.get(2)?,
        codex_trust_status: trust_from_database(row.get(3)?)?,
        row_version: row.get(4)?,
    })
}

fn tool_from_database(value: String) -> rusqlite::Result<Tool> {
    match value.as_str() {
        "claude" => Ok(Tool::Claude),
        "codex" => Ok(Tool::Codex),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn status_from_database(value: String) -> rusqlite::Result<SkillStatus> {
    match value.as_str() {
        "ready" => Ok(SkillStatus::Ready),
        "invalid" => Ok(SkillStatus::Invalid),
        "missing" => Ok(SkillStatus::Missing),
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

fn map_skill_write_error(
    error: rusqlite::Error,
    database_path: &str,
    operation: &'static str,
) -> AppError {
    let text = error.to_string();
    if text.contains("UNIQUE constraint failed: skills.name") {
        AppError::conflict("name", "Skill 名称已存在（不区分大小写）")
    } else if text.contains("UNIQUE constraint failed: skills.central_path") {
        AppError::conflict("centralPath", "Skill 中央目录已被其他记录占用")
    } else if text.contains("FOREIGN KEY constraint failed") {
        AppError::conflict("assignment", "Skill 仍有全局或项目分配，不能删除")
    } else if text.contains("GLOBAL_ASSIGNMENT_INHERITED")
        || text.contains("PROJECT_ASSIGNMENT_EXISTS")
    {
        AppError::conflict("assignment", "全局继承与项目分配不能重复")
    } else {
        AppError::database(database_path, operation)
    }
}
