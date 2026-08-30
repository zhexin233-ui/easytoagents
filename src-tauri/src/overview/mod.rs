use std::path::PathBuf;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    adapters::{canonicalize_project_root, ExplicitEnvironment},
    app::AppPaths,
    db::Database,
    domain::{ArtifactKind, Scope, SyncRunKind, SyncRunStatus, Tool},
    error::{AppError, ErrorCode},
    sync::{detect_interrupted_run, list_snapshots, InterruptedRunPlan},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DashboardToolSummaryDto {
    pub tool: Tool,
    pub active_provider_name: Option<String>,
    pub active_prompt_name: Option<String>,
    pub global_mcp_count: u32,
    pub global_skill_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentSyncRunDto {
    pub id: String,
    pub kind: SyncRunKind,
    pub status: SyncRunStatus,
    pub scope: Scope,
    pub project_id: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_code: Option<ErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummaryDto {
    pub tools: Vec<DashboardToolSummaryDto>,
    pub project_count: u32,
    pub conflict_count: u32,
    pub snapshot_count: u32,
    pub recent_sync_runs: Vec<RecentSyncRunDto>,
    pub interrupted_run: Option<InterruptedRunPlan>,
    pub needs_onboarding: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRestoreInput {
    pub snapshot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplySnapshotRestoreInput {
    pub preview_id: String,
    pub snapshot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompleteOnboardingResultDto {
    pub completed: bool,
}

#[derive(Debug)]
pub struct SnapshotRestoreContext {
    pub allowed_root: PathBuf,
}

pub fn dashboard_summary(
    database: &Database,
    paths: &AppPaths,
) -> Result<DashboardSummaryDto, AppError> {
    let tools = [Tool::Claude, Tool::Codex]
        .into_iter()
        .map(|tool| tool_summary(database, tool))
        .collect::<Result<Vec<_>, _>>()?;
    let database_path = database.path().to_string_lossy();
    let project_count = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM projects WHERE removed_at IS NULL",
            [],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|_| AppError::database(&database_path, "count_dashboard_projects"))?;
    let conflict_count = database
        .connection()
        .query_row(
            "SELECT COUNT(*)
             FROM managed_targets AS target
             LEFT JOIN projects AS project ON project.id = target.project_id
             WHERE target.last_status IN (
                'external_owned_change', 'parse_error', 'permission_denied',
                'policy_blocked', 'untrusted', 'target_type_changed', 'failed'
             ) AND (target.project_id IS NULL OR project.removed_at IS NULL)",
            [],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|_| AppError::database(&database_path, "count_dashboard_conflicts"))?;
    let snapshot_count = u32::try_from(list_snapshots(database)?.len())
        .map_err(|_| AppError::database(&database_path, "count_dashboard_snapshots"))?;
    let recent_sync_runs = recent_sync_runs(database)?;
    let interrupted_run = detect_interrupted_run(database, paths)?;
    let needs_onboarding = !onboarding_completed(database)?
        && project_count == 0
        && tools.iter().all(|tool| {
            tool.active_provider_name.is_none()
                && tool.active_prompt_name.is_none()
                && tool.global_mcp_count == 0
                && tool.global_skill_count == 0
        });
    Ok(DashboardSummaryDto {
        tools,
        project_count,
        conflict_count,
        snapshot_count,
        recent_sync_runs,
        interrupted_run,
        needs_onboarding,
    })
}

pub fn complete_onboarding(
    database: &mut Database,
) -> Result<CompleteOnboardingResultDto, AppError> {
    let database_path = database.path().to_string_lossy().into_owned();
    database
        .connection_mut()
        .execute(
            "INSERT INTO onboarding_state(singleton, completed_at)
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(singleton) DO UPDATE SET completed_at = excluded.completed_at",
            [],
        )
        .map_err(|_| AppError::database(&database_path, "complete_onboarding"))?;
    Ok(CompleteOnboardingResultDto { completed: true })
}

fn onboarding_completed(database: &Database) -> Result<bool, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM onboarding_state WHERE singleton = 1)",
            [],
            |row| row.get(0),
        )
        .map_err(|_| AppError::database(&database_path, "read_onboarding_state"))
}

fn tool_summary(database: &Database, tool: Tool) -> Result<DashboardToolSummaryDto, AppError> {
    let database_path = database.path().to_string_lossy();
    let active_provider_name = database
        .connection()
        .query_row(
            "SELECT name FROM provider_profiles WHERE tool = ?1 AND is_active = 1",
            [tool.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "read_dashboard_provider"))?;
    let active_prompt_name = database
        .connection()
        .query_row(
            "SELECT name FROM prompt_profiles
             WHERE (CASE WHEN ?1 = 'claude' THEN is_active_claude ELSE is_active_codex END) = 1",
            [tool.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "read_dashboard_prompt"))?;
    let global_mcp_count = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM mcp_global_assignments WHERE tool = ?1",
            [tool.as_str()],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|_| AppError::database(&database_path, "count_dashboard_mcp"))?;
    let global_skill_count = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM skill_global_assignments WHERE tool = ?1",
            [tool.as_str()],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|_| AppError::database(&database_path, "count_dashboard_skills"))?;
    Ok(DashboardToolSummaryDto {
        tool,
        active_provider_name,
        active_prompt_name,
        global_mcp_count,
        global_skill_count,
    })
}

fn recent_sync_runs(database: &Database) -> Result<Vec<RecentSyncRunDto>, AppError> {
    let database_path = database.path().to_string_lossy();
    let mut statement = database
        .connection()
        .prepare(
            "SELECT id, kind, status, scope, project_id, started_at, finished_at, error_code
             FROM sync_runs
             ORDER BY started_at DESC, id DESC
             LIMIT 5",
        )
        .map_err(|_| AppError::database(&database_path, "prepare_recent_sync_runs"))?;
    let runs = statement
        .query_map([], |row| {
            let scope = match row.get::<_, String>(3)?.as_str() {
                "global" => Scope::Global,
                "project" => Scope::Project,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            let kind = parse_sync_run_kind(&row.get::<_, String>(1)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            let status = parse_sync_run_status(&row.get::<_, String>(2)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            let error_code = row
                .get::<_, Option<String>>(7)?
                .map(|value| parse_error_code(&value))
                .transpose()
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(RecentSyncRunDto {
                id: row.get(0)?,
                kind,
                status,
                scope,
                project_id: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
                error_code,
            })
        })
        .map_err(|_| AppError::database(&database_path, "query_recent_sync_runs"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database(&database_path, "decode_recent_sync_runs"))?;
    Ok(runs)
}

fn parse_sync_run_kind(value: &str) -> Result<SyncRunKind, AppError> {
    SyncRunKind::from_stable_str(value)
        .ok_or_else(|| AppError::invalid_input("syncRunKind", "数据库包含未知同步任务类型"))
}

fn parse_sync_run_status(value: &str) -> Result<SyncRunStatus, AppError> {
    SyncRunStatus::from_stable_str(value)
        .ok_or_else(|| AppError::invalid_input("syncRunStatus", "数据库包含未知同步任务状态"))
}

fn parse_error_code(value: &str) -> Result<ErrorCode, AppError> {
    ErrorCode::from_stable_str(value)
        .ok_or_else(|| AppError::invalid_input("errorCode", "数据库包含未知同步错误码"))
}

pub fn snapshot_restore_context(
    database: &Database,
    environment: &ExplicitEnvironment,
    snapshot_id: &str,
) -> Result<SnapshotRestoreContext, AppError> {
    let database_path = database.path().to_string_lossy();
    let identity = database
        .connection()
        .query_row(
            "SELECT target.scope, target.tool, target.artifact_kind,
                    project.root_path, project.removed_at
             FROM snapshots AS snapshot
             JOIN managed_targets AS target ON target.id = snapshot.target_id
             LEFT JOIN projects AS project ON project.id = target.project_id
             WHERE snapshot.id = ?1",
            params![snapshot_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "load_snapshot_restore_context"))?
        .ok_or_else(|| AppError::not_found("snapshot", snapshot_id))?;
    let allowed_root = match identity.0.as_str() {
        "global" if identity.3.is_none() && identity.4.is_none() => {
            global_allowed_root(environment, &identity.1, &identity.2)?
        }
        "project" => {
            if identity.4.is_some() {
                return Err(AppError::conflict(
                    "snapshot",
                    "项目已移除，快照目标保持非受管",
                ));
            }
            let root = identity
                .3
                .ok_or_else(|| AppError::conflict("snapshot", "项目快照缺少登记项目根"))?;
            let canonical = canonicalize_project_root(PathBuf::from(&root).as_path())?;
            if canonical.as_str() != root {
                return Err(AppError::conflict(
                    "snapshot",
                    "项目根身份已变化，不能恢复快照",
                ));
            }
            PathBuf::from(root)
        }
        _ => {
            return Err(AppError::conflict(
                "snapshot",
                "快照范围与受管目标身份不一致",
            ));
        }
    };
    Ok(SnapshotRestoreContext { allowed_root })
}

fn global_allowed_root(
    environment: &ExplicitEnvironment,
    tool: &str,
    artifact_kind: &str,
) -> Result<PathBuf, AppError> {
    let tool = match tool {
        "claude" => Tool::Claude,
        "codex" => Tool::Codex,
        _ => {
            return Err(AppError::conflict("snapshot", "快照包含未知工具身份"));
        }
    };
    let artifact_kind = match artifact_kind {
        "provider" => ArtifactKind::Provider,
        "prompt" => ArtifactKind::Prompt,
        "mcp" => ArtifactKind::Mcp,
        "skill" => ArtifactKind::Skill,
        _ => {
            return Err(AppError::conflict("snapshot", "快照包含未知资源身份"));
        }
    };
    Ok(match (tool, artifact_kind) {
        (Tool::Claude, ArtifactKind::Mcp) => environment.home().to_path_buf(),
        (Tool::Claude, _) => environment.claude_config_dir().to_path_buf(),
        // Codex 全局 Skills 目标位于 CODEX_HOME/skills，恢复根与同步写入根一致。
        (Tool::Codex, _) => environment.codex_home().to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::params;
    use tempfile::tempdir;

    use super::{complete_onboarding, dashboard_summary, snapshot_restore_context};
    use crate::{
        adapters::{ExplicitEnvironment, ToolAvailability},
        app::AppPaths,
        db::Database,
    };

    #[test]
    fn dashboard_aggregates_central_intent_without_native_reads() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        let database = Database::open(&paths).unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO provider_profiles(id, tool, name, is_active)
                 VALUES ('00000000-0000-4000-8000-000000000741', 'claude', 'Fixture', 1)",
                [],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO projects(id, display_name, root_path)
                 VALUES ('00000000-0000-4000-8000-000000000742', 'Project', ?1)",
                [home.join("project").to_string_lossy().as_ref()],
            )
            .unwrap();

        let summary = dashboard_summary(&database, &paths).unwrap();
        assert_eq!(summary.project_count, 1);
        assert_eq!(
            summary.tools[0].active_provider_name.as_deref(),
            Some("Fixture")
        );
        assert!(!summary.needs_onboarding);
        assert!(summary.recent_sync_runs.is_empty());
    }

    #[test]
    fn explicit_all_skip_completion_persists_normal_dashboard_state() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        let mut database = Database::open(&paths).unwrap();
        assert!(
            dashboard_summary(&database, &paths)
                .unwrap()
                .needs_onboarding
        );
        assert!(complete_onboarding(&mut database).unwrap().completed);
        assert!(
            !dashboard_summary(&database, &paths)
                .unwrap()
                .needs_onboarding
        );
    }

    #[test]
    fn restore_context_derives_project_root_from_managed_identity() {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        fs::create_dir(home.join(".claude")).unwrap();
        fs::create_dir(home.join(".codex")).unwrap();
        let project = home.join("project");
        fs::create_dir(&project).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        let database = Database::open(&paths).unwrap();
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed()).unwrap();
        let project_id = "00000000-0000-4000-8000-000000000751";
        let target_id = "00000000-0000-4000-8000-000000000752";
        let run_id = "00000000-0000-4000-8000-000000000753";
        let snapshot_id = "00000000-0000-4000-8000-000000000754";
        database
            .connection()
            .execute(
                "INSERT INTO projects(id, display_name, root_path)
                 VALUES (?1, 'Project', ?2)",
                params![project_id, project.to_string_lossy()],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, project_id, target_path
                 ) VALUES (?1, 'claude', 'mcp', 'project', ?2, ?3)",
                params![
                    target_id,
                    project_id,
                    project.join(".mcp.json").to_string_lossy()
                ],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, project_id, db_version)
                 VALUES (?1, 'apply', 'succeeded', 'project', ?2, 1)",
                params![run_id, project_id],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO snapshots(
                    id, run_id, target_id, target_path, snapshot_path, target_type
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'missing')",
                params![
                    snapshot_id,
                    run_id,
                    target_id,
                    project.join(".mcp.json").to_string_lossy(),
                    paths.snapshots().join("fixture.snapshot").to_string_lossy(),
                ],
            )
            .unwrap();

        let context = snapshot_restore_context(&database, &environment, snapshot_id).unwrap();
        assert_eq!(context.allowed_root, project);
        database
            .connection()
            .execute(
                "UPDATE projects
                 SET removed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                [project_id],
            )
            .unwrap();
        let removed = snapshot_restore_context(&database, &environment, snapshot_id).unwrap_err();
        assert_eq!(removed.code(), crate::error::ErrorCode::Conflict);
    }

    #[test]
    fn global_restore_context_uses_custom_tool_root_instead_of_assuming_home() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let home = root.join("home");
        let claude_root = root.join("custom-claude");
        let codex_root = root.join("custom-codex");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&claude_root).unwrap();
        fs::create_dir(&codex_root).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        let database = Database::open(&paths).unwrap();
        let environment = ExplicitEnvironment::new(
            &home,
            Some(claude_root.clone()),
            Some(codex_root.clone()),
            ToolAvailability::all_installed(),
        )
        .unwrap();
        let target_id = "00000000-0000-4000-8000-000000000761";
        let run_id = "00000000-0000-4000-8000-000000000762";
        let snapshot_id = "00000000-0000-4000-8000-000000000763";
        let target_path = codex_root.join("config.toml");
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path
                 ) VALUES (?1, 'codex', 'provider', 'global', ?2)",
                params![target_id, target_path.to_string_lossy()],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, db_version)
                 VALUES (?1, 'apply', 'succeeded', 'global', 1)",
                [run_id],
            )
            .unwrap();
        database
            .connection()
            .execute(
                "INSERT INTO snapshots(
                    id, run_id, target_id, target_path, snapshot_path, target_type
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'missing')",
                params![
                    snapshot_id,
                    run_id,
                    target_id,
                    target_path.to_string_lossy(),
                    paths.snapshots().join("fixture.snapshot").to_string_lossy(),
                ],
            )
            .unwrap();

        let context = snapshot_restore_context(&database, &environment, snapshot_id).unwrap();
        assert_eq!(context.allowed_root, codex_root);
    }
}
