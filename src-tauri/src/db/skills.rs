//! Skill 中央记录、分配与逐目标 managed item 仓储。

use crate::{
    db::{column_tool, sync::reject_active_writer as reject_active_writer_on, Database},
    domain::{
        stable_sync_scopes, validate_global_assignment, validate_project_assignment, ArtifactKind,
        EntityId, SkillStatus, SyncScopeDto, Tool, TrustStatus,
    },
    error::AppError,
    skills::PreparedSkillRecord,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::Value;

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

/// 按原生目标路径定位的 Skill managed target。原生采纳还会核对输入携带的
/// target id、tool、scope/project 与目标行版本，不能只按工具名或路径猜测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillManagedTargetRecord {
    pub id: String,
    pub tool: Tool,
    pub scope: crate::domain::Scope,
    pub project_id: Option<String>,
    pub target_path: String,
    pub baseline_full_hash: Option<String>,
    pub baseline_managed_hash: Option<String>,
    pub baseline_projection_json: Option<String>,
    pub row_version: i64,
}

/// 已完成文件树校验后的中央 Skill 更新；内容、item 与 target 必须由同一个
/// SQLite IMMEDIATE 事务提交，避免只刷新 baseline 冒充原生采纳。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NativeSkillContentUpdate {
    pub id: String,
    pub row_version: u32,
    pub content_hash: String,
    pub frontmatter: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeSkillItemUpdate {
    pub id: String,
    pub target_id: String,
    pub resource_id: String,
    pub external_key: String,
    pub row_version: u32,
    pub item_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NativeSkillTargetUpdate {
    pub id: String,
    pub tool: Tool,
    pub scope: crate::domain::Scope,
    pub project_id: Option<String>,
    pub target_path: String,
    pub row_version: u32,
    pub full_hash: String,
    pub managed_hash: String,
    pub projection: Value,
    /// 在同一事务中复核的中央/项目/managed 行版本；目标行版本单独绑定在
    /// `row_version`，但若 Preview 携带目标行也必须指向同一目标。
    pub row_versions: Vec<crate::sync::DatabaseRowVersion>,
}

pub fn list_skills(database: &Database) -> Result<Vec<SkillRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT id, name, source_path, central_path, content_hash,
                    frontmatter_json, status, row_version
             FROM skills ORDER BY name COLLATE NOCASE, id",
        )
        .map_err(|error| AppError::database(&path, "prepare_list_skills").with_source(error))?;
    let records = statement
        .query_map([], skill_from_row)
        .map_err(|error| AppError::database(&path, "query_list_skills").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::database(&path, "decode_list_skills").with_source(error))?;
    Ok(records)
}

pub fn get_skill(database: &Database, id: &str) -> Result<SkillRecord, AppError> {
    get_skill_from_connection(
        database.connection(),
        &database.path().to_string_lossy(),
        id,
    )
}

pub(crate) fn get_skill_from_connection(
    connection: &rusqlite::Connection,
    database_path: &str,
    id: &str,
) -> Result<SkillRecord, AppError> {
    EntityId::parse(id)?;
    connection
        .query_row(
            "SELECT id, name, source_path, central_path, content_hash,
                    frontmatter_json, status, row_version
             FROM skills WHERE id = ?1",
            [id],
            skill_from_row,
        )
        .optional()
        .map_err(|error| AppError::database(database_path, "get_skill").with_source(error))?
        .ok_or_else(|| AppError::not_found("skill", id))
}

pub(crate) fn insert_skill(
    database: &mut Database,
    value: &PreparedSkillRecord,
) -> Result<SkillRecord, AppError> {
    let path = database.path().to_string_lossy().into_owned();
    database.with_immediate_transaction(|transaction| {
        insert_skill_in_transaction(transaction, &path, value)
    })
}

/// 调用方持有同一个写事务；批量导入不能逐项提交。
pub(crate) fn insert_skill_in_transaction(
    connection: &rusqlite::Connection,
    path: &str,
    value: &PreparedSkillRecord,
) -> Result<SkillRecord, AppError> {
    EntityId::parse(&value.id)?;
    let frontmatter = serde_json::to_string(&value.frontmatter).map_err(|error| {
        AppError::invalid_input("frontmatter", "Skill frontmatter 无法序列化").with_source(error)
    })?;
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
        .map_err(|error| AppError::database(path, "read_inserted_skill").with_source(error))?;
    Ok(record)
}

pub(crate) fn reject_active_writer(database: &Database) -> Result<(), AppError> {
    reject_active_writer_on(database.connection(), &database.path().to_string_lossy())
}

pub(crate) fn adopt_skill_content(
    database: &mut Database,
    id: &str,
    expected_row_version: u32,
    content_hash: &str,
    frontmatter: &Value,
) -> Result<SkillRecord, AppError> {
    EntityId::parse(id)?;
    let path = database.path().to_string_lossy().into_owned();
    let frontmatter_json = serde_json::to_string(frontmatter).map_err(|error| {
        AppError::invalid_input("frontmatter", "Skill frontmatter 无法序列化").with_source(error)
    })?;
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&path, "begin_adopt_skill_content").with_source(error)
        })?;
    reject_active_writer_on(&transaction, &path)?;
    let updated = transaction
        .execute(
            "UPDATE skills
             SET content_hash = ?2, frontmatter_json = ?3, status = 'ready'
             WHERE id = ?1 AND row_version = ?4",
            params![id, content_hash, frontmatter_json, expected_row_version],
        )
        .map_err(|error| map_skill_write_error(error, &path, "adopt_skill_content"))?;
    if updated != 1 {
        return match get_skill_from_connection(&transaction, &path, id) {
            Ok(_) => Err(AppError::conflict("rowVersion", "Skill 已被其他操作修改")),
            Err(error) => Err(error),
        };
    }
    let record = get_skill_from_connection(&transaction, &path, id)?;
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_adopt_skill_content").with_source(error)
    })?;
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
        .map_err(|error| AppError::database(&path, "count_skill_assignments").with_source(error))?;
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
        .map_err(|error| {
            AppError::database(&path, "count_applied_skill_links").with_source(error)
        })?;
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
        .map_err(|error| AppError::database(&path, "begin_delete_skill").with_source(error))?;
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
        .map_err(|error| {
            AppError::database(&path, "recheck_skill_delete_blockers").with_source(error)
        })?;
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
        .map_err(|error| AppError::database(&path, "commit_delete_skill").with_source(error))?;
    Ok(())
}

/// 一条语句取回全部 Skill 的全局分配，供列表接口一次组装，避免逐条 N+1 查询。
pub fn global_tools_for_all_skills(
    database: &Database,
) -> Result<std::collections::BTreeMap<String, Vec<Tool>>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT skill_id, tool FROM skill_global_assignments ORDER BY skill_id, tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_all_skill_global_tools").with_source(error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, column_tool(row, 1)?))
        })
        .map_err(|error| {
            AppError::database(&path, "query_all_skill_global_tools").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_all_skill_global_tools").with_source(error)
        })?;
    let mut grouped = std::collections::BTreeMap::<String, Vec<Tool>>::new();
    for (skill_id, tool) in rows {
        grouped.entry(skill_id).or_default().push(tool);
    }
    Ok(grouped)
}

/// 返回 Skill 当前所有显式 assignment 对应的同步 scope。
pub fn sync_scopes_for_skill(
    database: &Database,
    skill_id: &str,
) -> Result<Vec<SyncScopeDto>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT tool, NULL FROM skill_global_assignments WHERE skill_id = ?1
             UNION ALL
             SELECT tool, project_id FROM skill_project_assignments WHERE skill_id = ?1
             UNION ALL
             SELECT target.tool, target.project_id
             FROM managed_items AS item
             JOIN managed_targets AS target ON target.id = item.target_id
             WHERE item.resource_kind = 'skill' AND item.resource_id = ?1
             ORDER BY tool, project_id",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_skill_sync_scopes").with_source(error)
        })?;
    let scopes = statement
        .query_map([skill_id], |row| {
            Ok(SyncScopeDto::new(
                ArtifactKind::Skill,
                column_tool(row, 0)?,
                row.get(1)?,
            ))
        })
        .map_err(|error| AppError::database(&path, "query_skill_sync_scopes").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_skill_sync_scopes").with_source(error)
        })?;
    Ok(stable_sync_scopes(scopes))
}

/// 通用的 `SELECT id, row_version FROM <table> WHERE id IN (...)`。
pub(crate) fn row_versions_by_id(
    connection: &rusqlite::Connection,
    database_path: &str,
    table: &'static str,
    ids: &[&str],
    operation: &'static str,
) -> Result<std::collections::BTreeMap<String, i64>, AppError> {
    if ids.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!("SELECT id, row_version FROM {table} WHERE id IN ({placeholders})");
    let mut statement = connection
        .prepare_cached(&sql)
        .map_err(|error| AppError::database(database_path, operation).with_source(error))?;
    let params: Vec<&dyn rusqlite::ToSql> =
        ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    let rows = statement
        .query_map(params.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| AppError::database(database_path, operation).with_source(error))?
        .collect::<Result<std::collections::BTreeMap<_, _>, _>>()
        .map_err(|error| AppError::database(database_path, operation).with_source(error))?;
    Ok(rows)
}

pub fn global_tools_for_skill(database: &Database, skill_id: &str) -> Result<Vec<Tool>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT tool FROM skill_global_assignments
             WHERE skill_id = ?1 ORDER BY tool",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_skill_global_tools").with_source(error)
        })?;
    let tools = statement
        .query_map([skill_id], |row| column_tool(row, 0))
        .map_err(|error| AppError::database(&path, "query_skill_global_tools").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_skill_global_tools").with_source(error)
        })?;
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
        .map_err(|error| {
            AppError::database(&path, "begin_set_skill_global_assignment").with_source(error)
        })?;
    set_global_assignment_in_connection(
        &transaction,
        &path,
        tool,
        skill_id,
        assigned,
        expected_row_version,
    )?;
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_set_skill_global_assignment").with_source(error)
    })?;
    get_skill(database, skill_id)
}

pub(crate) fn set_global_assignment_in_connection(
    connection: &rusqlite::Connection,
    database_path: &str,
    tool: Tool,
    skill_id: &str,
    assigned: bool,
    expected_row_version: u32,
) -> Result<bool, AppError> {
    EntityId::parse(skill_id)?;
    verify_row_version(
        connection,
        "skills",
        skill_id,
        expected_row_version,
        "skill",
        database_path,
    )?;
    let changed = if assigned {
        let project_count = connection
            .query_row(
                "SELECT COUNT(*) FROM skill_project_assignments
                 WHERE tool = ?1 AND skill_id = ?2",
                params![tool.as_str(), skill_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                AppError::database(database_path, "count_skill_project_assignments")
                    .with_source(error)
            })?;
        validate_global_assignment(project_count > 0)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO skill_global_assignments(tool, skill_id) VALUES (?1, ?2)",
                params![tool.as_str(), skill_id],
            )
            .map_err(|error| {
                map_skill_write_error(error, database_path, "insert_skill_global_assignment")
            })?
    } else {
        connection
            .execute(
                "DELETE FROM skill_global_assignments WHERE tool = ?1 AND skill_id = ?2",
                params![tool.as_str(), skill_id],
            )
            .map_err(|error| {
                AppError::database(database_path, "delete_skill_global_assignment")
                    .with_source(error)
            })?
    };
    if changed == 1 {
        touch_versioned_row(
            connection,
            "skills",
            skill_id,
            expected_row_version,
            database_path,
        )?;
    }
    Ok(changed == 1)
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
        .map_err(|error| {
            AppError::database(&path, "begin_set_skill_project_assignment").with_source(error)
        })?;
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
        .map_err(|error| {
            AppError::database(&path, "read_skill_global_assignment").with_source(error)
        })?;
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
            .map_err(|error| {
                AppError::database(&path, "delete_skill_project_assignment").with_source(error)
            })?
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
    transaction.commit().map_err(|error| {
        AppError::database(&path, "commit_set_skill_project_assignment").with_source(error)
    })?;
    get_skill(database, skill_id)
}

pub fn list_assigned_skills(
    database: &Database,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<Vec<SkillRecord>, AppError> {
    list_assigned_skills_from_connection(
        database.connection(),
        &database.path().to_string_lossy(),
        tool,
        project_id,
    )
}

pub(crate) fn list_assigned_skills_from_connection(
    connection: &rusqlite::Connection,
    database_path: &str,
    tool: Tool,
    project_id: Option<&str>,
) -> Result<Vec<SkillRecord>, AppError> {
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
    let mut statement = connection.prepare_cached(sql).map_err(|error| {
        AppError::database(database_path, "prepare_list_assigned_skills").with_source(error)
    })?;
    let records = statement
        .query_map(params![project_parameter, tool.as_str()], skill_from_row)
        .map_err(|error| {
            AppError::database(database_path, "query_list_assigned_skills").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(database_path, "decode_list_assigned_skills").with_source(error)
        })?;
    Ok(records)
}

pub fn list_projects(database: &Database) -> Result<Vec<SkillProjectRecord>, AppError> {
    let path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT id, display_name, root_path, codex_trust_status, row_version
             FROM projects
             WHERE removed_at IS NULL
             ORDER BY display_name COLLATE NOCASE, root_path",
        )
        .map_err(|error| {
            AppError::database(&path, "prepare_list_skill_projects").with_source(error)
        })?;
    let projects = statement
        .query_map([], project_from_row)
        .map_err(|error| AppError::database(&path, "query_list_skill_projects").with_source(error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&path, "decode_list_skill_projects").with_source(error)
        })?;
    Ok(projects)
}

pub fn get_project(database: &Database, id: &str) -> Result<SkillProjectRecord, AppError> {
    get_project_from_connection(
        database.connection(),
        &database.path().to_string_lossy(),
        id,
    )
}

pub(crate) fn get_project_from_connection(
    connection: &rusqlite::Connection,
    database_path: &str,
    id: &str,
) -> Result<SkillProjectRecord, AppError> {
    EntityId::parse(id)?;
    connection
        .query_row(
            "SELECT id, display_name, root_path, codex_trust_status, row_version
             FROM projects WHERE id = ?1 AND removed_at IS NULL",
            [id],
            project_from_row,
        )
        .optional()
        .map_err(|error| AppError::database(database_path, "get_skill_project").with_source(error))?
        .ok_or_else(|| AppError::not_found("project", id))
}

pub fn list_managed_skill_items(
    database: &Database,
    target_id: &str,
) -> Result<Vec<ManagedSkillItemRecord>, AppError> {
    list_managed_skill_items_from_connection(
        database.connection(),
        &database.path().to_string_lossy(),
        target_id,
    )
}

pub(crate) fn list_managed_skill_items_from_connection(
    connection: &rusqlite::Connection,
    database_path: &str,
    target_id: &str,
) -> Result<Vec<ManagedSkillItemRecord>, AppError> {
    let mut statement = connection
        .prepare_cached(
            "SELECT id, resource_id, external_key, last_applied_item_hash, row_version
             FROM managed_items
             WHERE target_id = ?1 AND resource_kind = 'skill'
             ORDER BY external_key COLLATE NOCASE, id",
        )
        .map_err(|error| {
            AppError::database(database_path, "prepare_list_managed_skill_items").with_source(error)
        })?;
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
        .map_err(|error| {
            AppError::database(database_path, "query_list_managed_skill_items").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(database_path, "decode_list_managed_skill_items").with_source(error)
        })?;
    Ok(items)
}

/// 按完整 `(tool, artifact_kind, target_path)` 查询 Skill 目标。
///
/// 数据库的唯一索引还包含 scope/project；这里显式拒绝同一工具下存在多个
/// 同路径 Skill target 的异常状态，避免把一个计划误应用到另一条 scope。
pub(crate) fn find_skill_managed_target(
    database: &Database,
    tool: Tool,
    target_path: &str,
) -> Result<Option<SkillManagedTargetRecord>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare_cached(
            "SELECT id, tool, scope, project_id, target_path,
                    baseline_full_hash, baseline_managed_hash,
                    baseline_projection_json, row_version
             FROM managed_targets
             WHERE tool = ?1 AND artifact_kind = 'skill' AND target_path = ?2
             ORDER BY scope, project_id, id",
        )
        .map_err(|error| {
            AppError::database(&database_path, "prepare_find_skill_managed_target")
                .with_source(error)
        })?;
    let rows = statement
        .query_map(params![tool.as_str(), target_path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })
        .map_err(|error| {
            AppError::database(&database_path, "query_find_skill_managed_target").with_source(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::database(&database_path, "decode_find_skill_managed_target")
                .with_source(error)
        })?;
    let row = match rows.as_slice() {
        [] => return Ok(None),
        [row] => row,
        _ => {
            return Err(AppError::conflict(
                "targetPath",
                "Skill 原生目标路径对应多个 managed target，不能安全采纳",
            ))
        }
    };
    let parsed_tool = Tool::from_stable_str(&row.1)
        .ok_or_else(|| AppError::invalid_input("targetTool", "数据库中的 Skill 目标工具无效"))?;
    let scope = crate::domain::Scope::from_stable_str(&row.2).ok_or_else(|| {
        AppError::invalid_input("targetScope", "数据库中的 Skill 目标 scope 无效")
    })?;
    if parsed_tool != tool {
        return Err(AppError::conflict(
            "targetTool",
            "Skill 目标工具与采纳输入不一致",
        ));
    }
    Ok(Some(SkillManagedTargetRecord {
        id: row.0.clone(),
        tool: parsed_tool,
        scope,
        project_id: row.3.clone(),
        target_path: row.4.clone(),
        baseline_full_hash: row.5.clone(),
        baseline_managed_hash: row.6.clone(),
        baseline_projection_json: row.7.clone(),
        row_version: row.8,
    }))
}

/// 在已完成文件树和 target 观察校验后，一次性提交 Skill 原生采纳。
///
/// 这条仓储入口故意不调用 `reject_active_writer`：外部变化动作在命令层先
/// claim 了同一份 persisted preview，active writer 正是该 preview 本身。所有
/// 行版本仍在本事务中 CAS 校验；因此未 claim 的直接调用也只能在没有其它写者
/// 时成功，而不会绕过单写者或 stale 保护。
pub(crate) fn adopt_native_skill(
    database: &mut Database,
    target: &NativeSkillTargetUpdate,
    contents: &[NativeSkillContentUpdate],
    items: &[NativeSkillItemUpdate],
) -> Result<(), AppError> {
    EntityId::parse(&target.id)?;
    if let Some(project_id) = target.project_id.as_deref() {
        EntityId::parse(project_id)?;
    }
    if !is_sha256(&target.full_hash) || !is_sha256(&target.managed_hash) {
        return Err(AppError::invalid_input(
            "observedHash",
            "Skill 采纳缺少有效的目标 hash",
        ));
    }
    if !target.projection.is_object() {
        return Err(AppError::invalid_input(
            "projection",
            "Skill 采纳目标投影必须是对象",
        ));
    }
    let projection_json = serde_json::to_string(&target.projection).map_err(|error| {
        AppError::invalid_input("projection", "Skill 采纳基线无法序列化").with_source(error)
    })?;
    let database_path = database.path().to_string_lossy().into_owned();
    let transaction = database
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            AppError::database(&database_path, "begin_adopt_native_skill").with_source(error)
        })?;

    let actual_target = transaction
        .query_row(
            "SELECT tool, artifact_kind, scope, project_id, target_path,
                    row_version, baseline_full_hash, baseline_managed_hash
             FROM managed_targets WHERE id = ?1",
            [&target.id],
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
                ))
            },
        )
        .optional()
        .map_err(|error| {
            AppError::database(&database_path, "verify_adopt_native_skill_target")
                .with_source(error)
        })?
        .ok_or_else(|| AppError::stale_preview("adoptSkillNative", &target.target_path))?;
    if actual_target.0 != target.tool.as_str()
        || actual_target.1 != ArtifactKind::Skill.as_str()
        || actual_target.2 != target.scope.as_str()
        || actual_target.3 != target.project_id
        || actual_target.4 != target.target_path
        || u32::try_from(actual_target.5).ok() != Some(target.row_version)
        || actual_target.6.is_none() != actual_target.7.is_none()
    {
        return Err(AppError::stale_preview(
            "adoptSkillNative",
            &target.target_path,
        ));
    }
    if actual_target.6.is_none() {
        return Err(AppError::conflict(
            "skillAdopt",
            "没有完整 Skill managed baseline，不能直接采纳",
        ));
    }
    verify_native_skill_row_versions(&transaction, target, &database_path)?;

    let mut content_ids = std::collections::BTreeSet::new();
    for content in contents {
        EntityId::parse(&content.id)?;
        if !content_ids.insert(content.id.as_str()) {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Skill 采纳包含重复的 Skill 内容更新",
            ));
        }
        if !is_sha256(&content.content_hash) || !content.frontmatter.is_object() {
            return Err(AppError::invalid_input(
                "skillContent",
                "Skill 采纳内容或 frontmatter 无效",
            ));
        }
        let frontmatter_json = serde_json::to_string(&content.frontmatter).map_err(|error| {
            AppError::invalid_input("frontmatter", "Skill frontmatter 无法序列化")
                .with_source(error)
        })?;
        let updated = transaction
            .execute(
                "UPDATE skills
                 SET content_hash = ?2, frontmatter_json = ?3, status = 'ready'
                 WHERE id = ?1 AND row_version = ?4",
                params![
                    content.id,
                    content.content_hash,
                    frontmatter_json,
                    content.row_version
                ],
            )
            .map_err(|error| map_skill_write_error(error, &database_path, "adopt_native_skill"))?;
        if updated != 1 {
            return Err(AppError::stale_preview("adoptSkillNative", &content.id));
        }
    }

    let mut item_ids = std::collections::BTreeSet::new();
    for item in items {
        EntityId::parse(&item.id)?;
        EntityId::parse(&item.target_id)?;
        EntityId::parse(&item.resource_id)?;
        if item.target_id != target.id || !item_ids.insert(item.id.as_str()) {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Skill 采纳包含不匹配或重复的 managed item",
            ));
        }
        if !is_sha256(&item.item_hash) {
            return Err(AppError::invalid_input(
                "itemHash",
                "Skill 采纳缺少有效的 managed item hash",
            ));
        }
        let updated = transaction
            .execute(
                "UPDATE managed_items
                 SET last_applied_item_hash = ?2
                 WHERE id = ?1 AND target_id = ?3 AND resource_kind = 'skill'
                   AND resource_id = ?4 AND external_key = ?5 AND row_version = ?6",
                params![
                    item.id,
                    item.item_hash,
                    item.target_id,
                    item.resource_id,
                    item.external_key,
                    item.row_version
                ],
            )
            .map_err(|error| {
                AppError::database(&database_path, "adopt_native_skill_item").with_source(error)
            })?;
        if updated != 1 {
            return Err(AppError::stale_preview("adoptSkillNative", &item.id));
        }
    }

    let updated = transaction
        .execute(
            "UPDATE managed_targets
             SET baseline_full_hash = ?2, baseline_managed_hash = ?3,
                 baseline_projection_json = ?4, last_status = 'in_sync'
             WHERE id = ?1 AND tool = ?5 AND artifact_kind = 'skill'
               AND scope = ?6 AND ifnull(project_id, '') = ifnull(?7, '')
               AND target_path = ?8 AND row_version = ?9",
            params![
                target.id,
                target.full_hash,
                target.managed_hash,
                projection_json,
                target.tool.as_str(),
                target.scope.as_str(),
                target.project_id,
                target.target_path,
                target.row_version,
            ],
        )
        .map_err(|error| {
            AppError::database(&database_path, "adopt_native_skill_target").with_source(error)
        })?;
    if updated != 1 {
        return Err(AppError::stale_preview(
            "adoptSkillNative",
            &target.target_path,
        ));
    }
    transaction.commit().map_err(|error| {
        AppError::database(&database_path, "commit_adopt_native_skill").with_source(error)
    })?;
    Ok(())
}

fn verify_row_version(
    transaction: &rusqlite::Connection,
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
        .map_err(|error| {
            AppError::database(database_path, "verify_skill_assignment_row_version")
                .with_source(error)
        })?
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
    transaction: &rusqlite::Connection,
    table: &str,
    id: &str,
    expected: u32,
    database_path: &str,
) -> Result<(), AppError> {
    let sql =
        format!("UPDATE {table} SET updated_at = updated_at WHERE id = ?1 AND row_version = ?2");
    let updated = transaction
        .execute(&sql, params![id, expected])
        .map_err(|error| {
            AppError::database(database_path, "touch_skill_assignment_owner").with_source(error)
        })?;
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
    let app_error = if text.contains("UNIQUE constraint failed: skills.name") {
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
    };
    app_error.with_source(error)
}

fn verify_native_skill_row_versions(
    transaction: &rusqlite::Transaction<'_>,
    target: &NativeSkillTargetUpdate,
    database_path: &str,
) -> Result<(), AppError> {
    let mut seen = std::collections::BTreeSet::new();
    let mut project_seen = false;
    for row in &target.row_versions {
        if !matches!(
            row.entity_type,
            crate::sync::DatabaseEntityType::Skill
                | crate::sync::DatabaseEntityType::Project
                | crate::sync::DatabaseEntityType::ManagedTarget
                | crate::sync::DatabaseEntityType::ManagedItem
        ) {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Skill 原生采纳包含不匹配的数据库行版本",
            ));
        }
        EntityId::parse(&row.entity_id)?;
        let key = (row.entity_type, row.entity_id.clone());
        if !seen.insert(key) {
            return Err(AppError::invalid_input(
                "rowVersions",
                "Skill 原生采纳包含重复的数据库行版本",
            ));
        }
        if row.entity_type == crate::sync::DatabaseEntityType::ManagedTarget {
            if row.entity_id != target.id || row.row_version != target.row_version {
                return Err(AppError::stale_preview(
                    "adoptSkillNative",
                    &target.target_path,
                ));
            }
            continue;
        }
        if row.entity_type == crate::sync::DatabaseEntityType::Project {
            if target.project_id.as_deref() != Some(row.entity_id.as_str()) {
                return Err(AppError::stale_preview(
                    "adoptSkillNative",
                    &target.target_path,
                ));
            }
            project_seen = true;
        }
        let table = match row.entity_type {
            crate::sync::DatabaseEntityType::Skill => "skills",
            crate::sync::DatabaseEntityType::Project => "projects",
            crate::sync::DatabaseEntityType::ManagedItem => "managed_items",
            crate::sync::DatabaseEntityType::ManagedTarget => unreachable!(),
            _ => unreachable!(),
        };
        let actual = crate::db::sync::load_row_version(
            transaction,
            table,
            &row.entity_id,
            database_path,
            "verify_adopt_native_skill_row_version",
        )?;
        if actual.and_then(|value| u32::try_from(value).ok()) != Some(row.row_version) {
            return Err(AppError::stale_preview("adoptSkillNative", &row.entity_id));
        }
    }
    if target.project_id.is_some() && !project_seen {
        return Err(AppError::stale_preview(
            "adoptSkillNative",
            &target.target_path,
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
