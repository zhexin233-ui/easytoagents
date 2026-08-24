use std::{collections::BTreeSet, path::Path};

use rusqlite::OptionalExtension;

use crate::{
    adapters::{
        canonicalize_project_root, claude::ClaudeAdapter, codex::CodexAdapter,
        ClaudeCustomizationPolicyProbe, ConservativeClaudeCustomizationPolicyProbe,
        ConservativeClaudeUserMcpProbe, DiscoveryContext, ExplicitEnvironment, ManagedOwnership,
        PolicyState, TargetDescriptor, TargetTrustState, ToolAdapter,
    },
    db::{mcp as mcp_repository, projects as repository, skills as skill_repository, Database},
    domain::{ArtifactKind, ArtifactName, EntityId, SyncStatus, Tool, TrustStatus},
    error::{AppError, ErrorCode},
    git::inspect_path,
    sync::{assess_drift, hash_json, scan_target, ManagedTargetBaseline, TargetScan},
};

use super::{
    GitRepositoryStatus, ProjectDto, ProjectPathStatus, ProjectTargetStatusDto,
    RegisterProjectInput, RemoveProjectResultDto, VersionedProjectInput,
};

struct ProjectObservation {
    git_status: GitRepositoryStatus,
    codex_trust_status: TrustStatus,
    claude_policy_status: PolicyState,
    targets: Vec<ProjectTargetStatusDto>,
}

struct PersistedProjectTarget {
    project_id: String,
    baseline: ManagedTargetBaseline,
}

pub fn list_projects(
    database: &Database,
    environment: &ExplicitEnvironment,
) -> Result<Vec<ProjectDto>, AppError> {
    let policy_probe = ConservativeClaudeCustomizationPolicyProbe;
    repository::list_registered_projects(database)?
        .into_iter()
        .map(|record| project_dto(database, environment, &policy_probe, record))
        .collect()
}

pub fn get_project(
    database: &Database,
    environment: &ExplicitEnvironment,
    id: &str,
) -> Result<ProjectDto, AppError> {
    let policy_probe = ConservativeClaudeCustomizationPolicyProbe;
    let record = repository::get_registered_project(database, id)?;
    project_dto(database, environment, &policy_probe, record)
}

pub fn register_project(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &RegisterProjectInput,
) -> Result<ProjectDto, AppError> {
    let policy_probe = ConservativeClaudeCustomizationPolicyProbe;
    register_project_with_policy_probe(database, environment, input, &policy_probe)
}

fn register_project_with_policy_probe(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &RegisterProjectInput,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
) -> Result<ProjectDto, AppError> {
    let display_name = ArtifactName::parse(&input.display_name)?;
    let project_root = canonicalize_project_root(Path::new(&input.root_path))?;
    let observation = observe_project(database, environment, policy_probe, &project_root)?;
    let scan_update = repository::ProjectScanUpdate {
        is_git_repo: observation.git_status == GitRepositoryStatus::Repository,
        codex_trust_status: observation.codex_trust_status.as_str(),
    };
    let record = match repository::find_project_by_root(database, project_root.as_str())? {
        Some(existing) if !existing.removed => {
            return Err(AppError::conflict("rootPath", "该规范化项目目录已经登记"));
        }
        Some(existing) => repository::reactivate_project(
            database,
            &existing.id,
            display_name.as_str(),
            &scan_update,
            existing.row_version,
        )?,
        None => repository::insert_project(
            database,
            &EntityId::new().to_string(),
            display_name.as_str(),
            project_root.as_str(),
            &scan_update,
        )?,
    };
    project_dto_from_observation(record, ProjectPathStatus::Valid, observation)
}

pub fn rescan_project(
    database: &mut Database,
    environment: &ExplicitEnvironment,
    input: &VersionedProjectInput,
) -> Result<ProjectDto, AppError> {
    let policy_probe = ConservativeClaudeCustomizationPolicyProbe;
    let current = repository::get_registered_project(database, &input.id)?;
    if current.row_version != input.row_version {
        return Err(AppError::conflict("rowVersion", "项目已被其他操作更新"));
    }
    let canonical = match canonicalize_project_root(Path::new(&current.root_path)) {
        Ok(root) if root.as_str() == current.root_path => root,
        Ok(_) => {
            return invalid_project_after_rescan(
                database,
                current,
                ProjectPathStatus::Invalid,
                input.row_version,
            );
        }
        Err(error) if error.code() == ErrorCode::NotFound => {
            return invalid_project_after_rescan(
                database,
                current,
                ProjectPathStatus::Missing,
                input.row_version,
            );
        }
        Err(error) if error.code() == ErrorCode::PermissionDenied => {
            return invalid_project_after_rescan(
                database,
                current,
                ProjectPathStatus::PermissionDenied,
                input.row_version,
            );
        }
        Err(_) => {
            return invalid_project_after_rescan(
                database,
                current,
                ProjectPathStatus::Invalid,
                input.row_version,
            );
        }
    };
    let observation = observe_project(database, environment, &policy_probe, &canonical)?;
    let updated = repository::update_project_scan(
        database,
        &input.id,
        None,
        &repository::ProjectScanUpdate {
            is_git_repo: observation.git_status == GitRepositoryStatus::Repository,
            codex_trust_status: observation.codex_trust_status.as_str(),
        },
        input.row_version,
    )?;
    project_dto_from_observation(updated, ProjectPathStatus::Valid, observation)
}

pub fn remove_project(
    database: &mut Database,
    input: &VersionedProjectInput,
) -> Result<RemoveProjectResultDto, AppError> {
    let removed = repository::soft_remove_project(database, &input.id, input.row_version)?;
    Ok(RemoveProjectResultDto {
        id: input.id.clone(),
        removed: true,
        native_configuration_left_unmanaged: removed.managed_target_count > 0,
    })
}

fn invalid_project_after_rescan(
    database: &mut Database,
    current: repository::ProjectRecord,
    path_status: ProjectPathStatus,
    expected_row_version: u32,
) -> Result<ProjectDto, AppError> {
    let updated = repository::update_project_scan(
        database,
        &current.id,
        None,
        &repository::ProjectScanUpdate {
            is_git_repo: false,
            codex_trust_status: TrustStatus::Unknown.as_str(),
        },
        expected_row_version,
    )?;
    let diagnostic = match path_status {
        ProjectPathStatus::Missing => "PROJECT_ROOT_MISSING",
        ProjectPathStatus::PermissionDenied => "PROJECT_ROOT_PERMISSION_DENIED",
        ProjectPathStatus::Invalid => "PROJECT_ROOT_CHANGED",
        ProjectPathStatus::Valid => "PROJECT_ROOT_INVALID",
    };
    project_dto_from_observation(
        updated,
        path_status,
        ProjectObservation {
            git_status: GitRepositoryStatus::Unavailable,
            codex_trust_status: TrustStatus::Unknown,
            claude_policy_status: PolicyState::Unknown,
            targets: blocked_project_targets(diagnostic),
        },
    )
}

fn project_dto(
    database: &Database,
    environment: &ExplicitEnvironment,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
    record: repository::ProjectRecord,
) -> Result<ProjectDto, AppError> {
    let project_root = match canonicalize_project_root(Path::new(&record.root_path)) {
        Ok(root) if root.as_str() == record.root_path => root,
        Ok(_) => {
            return project_dto_from_observation(
                record,
                ProjectPathStatus::Invalid,
                ProjectObservation {
                    git_status: GitRepositoryStatus::Unavailable,
                    codex_trust_status: TrustStatus::Unknown,
                    claude_policy_status: PolicyState::Unknown,
                    targets: blocked_project_targets("PROJECT_ROOT_CHANGED"),
                },
            );
        }
        Err(error) => {
            let (path_status, diagnostic) = match error.code() {
                ErrorCode::NotFound => (ProjectPathStatus::Missing, "PROJECT_ROOT_MISSING"),
                ErrorCode::PermissionDenied => (
                    ProjectPathStatus::PermissionDenied,
                    "PROJECT_ROOT_PERMISSION_DENIED",
                ),
                _ => (ProjectPathStatus::Invalid, "PROJECT_ROOT_INVALID"),
            };
            return project_dto_from_observation(
                record,
                path_status,
                ProjectObservation {
                    git_status: GitRepositoryStatus::Unavailable,
                    codex_trust_status: TrustStatus::Unknown,
                    claude_policy_status: PolicyState::Unknown,
                    targets: blocked_project_targets(diagnostic),
                },
            );
        }
    };
    let observation = observe_project(database, environment, policy_probe, &project_root)?;
    project_dto_from_observation(record, ProjectPathStatus::Valid, observation)
}

fn project_dto_from_observation(
    record: repository::ProjectRecord,
    path_status: ProjectPathStatus,
    observation: ProjectObservation,
) -> Result<ProjectDto, AppError> {
    Ok(ProjectDto {
        id: record.id,
        display_name: record.display_name,
        root_path: record.root_path,
        path_status,
        git_status: observation.git_status,
        codex_trust_status: observation.codex_trust_status,
        claude_policy_status: observation.claude_policy_status,
        targets: observation.targets,
        last_scanned_at: record.last_scanned_at,
        row_version: record.row_version,
    })
}

fn observe_project(
    database: &Database,
    environment: &ExplicitEnvironment,
    policy_probe: &dyn ClaudeCustomizationPolicyProbe,
    project_root: &crate::domain::ProjectRoot,
) -> Result<ProjectObservation, AppError> {
    let git_status = match inspect_path(
        project_root,
        &Path::new(project_root.as_str()).join(".mcp.json"),
    ) {
        Ok(status) if status.is_repository => GitRepositoryStatus::Repository,
        Ok(_) => GitRepositoryStatus::NotRepository,
        Err(_) => GitRepositoryStatus::Unavailable,
    };
    let user_probe = ConservativeClaudeUserMcpProbe;
    let context = DiscoveryContext {
        environment,
        project_root: Some(project_root),
        claude_user_mcp_probe: &user_probe,
        claude_customization_policy_probe: policy_probe,
    };
    let claude_targets = ClaudeAdapter.discover(&context)?;
    let codex_targets = CodexAdapter.discover(&context)?;
    let claude_project_targets = project_targets(claude_targets);
    let codex_project_targets = project_targets(codex_targets);
    let claude_policy_status = claude_project_targets
        .iter()
        .map(|target| target.policy)
        .fold(PolicyState::Allowed, merge_policy);
    let codex_trust = codex_project_targets
        .iter()
        .map(|target| target.trust)
        .next()
        .unwrap_or(TargetTrustState::Unknown);
    let codex_trust_status = match codex_trust {
        TargetTrustState::Trusted => TrustStatus::Trusted,
        TargetTrustState::Untrusted => TrustStatus::Untrusted,
        TargetTrustState::Unknown | TargetTrustState::NotRequired => TrustStatus::Unknown,
    };
    let targets = claude_project_targets
        .into_iter()
        .chain(codex_project_targets)
        .map(|descriptor| target_status(database, project_root.as_str(), descriptor))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProjectObservation {
        git_status,
        codex_trust_status,
        claude_policy_status,
        targets,
    })
}

fn project_targets(targets: Vec<TargetDescriptor>) -> Vec<TargetDescriptor> {
    targets
        .into_iter()
        .filter(|target| {
            target.scope == crate::domain::Scope::Project
                && matches!(
                    target.artifact_kind,
                    ArtifactKind::Mcp | ArtifactKind::Skill
                )
        })
        .collect()
}

fn merge_policy(current: PolicyState, next: PolicyState) -> PolicyState {
    match (current, next) {
        (PolicyState::Blocked, _) | (_, PolicyState::Blocked) => PolicyState::Blocked,
        (PolicyState::Unknown, _) | (_, PolicyState::Unknown) => PolicyState::Unknown,
        _ => PolicyState::Allowed,
    }
}

fn target_status(
    database: &Database,
    project_id_root: &str,
    descriptor: TargetDescriptor,
) -> Result<ProjectTargetStatusDto, AppError> {
    let persisted = descriptor
        .path
        .as_deref()
        .map(|path| persisted_project_target(database, project_id_root, &descriptor, path))
        .transpose()?
        .flatten();
    let assessment_target = match persisted {
        Some(persisted) => Some(persisted),
        None => repository::find_project_by_root(database, project_id_root)?
            .filter(|project| !project.removed)
            .map(|project| PersistedProjectTarget {
                project_id: project.id,
                baseline: ManagedTargetBaseline {
                    target_id: String::new(),
                    target_row_version: 0,
                    full_hash: None,
                    managed_hash: None,
                },
            }),
    };
    let managed_assessment = assessment_target
        .as_ref()
        .map(|persisted| assess_managed_target(database, &descriptor, persisted))
        .transpose()?
        .flatten();
    let (status, diagnostic_code) = if let Some(assessment) = managed_assessment {
        (
            assessment.status,
            assessment.diagnostic_codes.into_iter().next(),
        )
    } else if descriptor.capability.state != crate::adapters::CapabilityState::Supported {
        (
            SyncStatus::Failed,
            descriptor.capability.diagnostic_code.clone(),
        )
    } else if descriptor.policy != PolicyState::Allowed {
        (
            SyncStatus::PolicyBlocked,
            Some(
                match descriptor.policy {
                    PolicyState::Blocked => "CLAUDE_POLICY_BLOCKED",
                    PolicyState::Unknown => "CLAUDE_POLICY_UNKNOWN",
                    PolicyState::Allowed => "CLAUDE_POLICY_ALLOWED",
                }
                .to_owned(),
            ),
        )
    } else if matches!(
        descriptor.trust,
        TargetTrustState::Untrusted | TargetTrustState::Unknown
    ) {
        (
            SyncStatus::Untrusted,
            Some(
                match descriptor.trust {
                    TargetTrustState::Untrusted => "CODEX_PROJECT_UNTRUSTED",
                    _ => "CODEX_TRUST_UNKNOWN",
                }
                .to_owned(),
            ),
        )
    } else {
        status_from_unmanaged_scan(&descriptor)
    };
    Ok(ProjectTargetStatusDto {
        tool: descriptor.tool,
        artifact_kind: descriptor.artifact_kind,
        target_path: descriptor.path.clone(),
        capability: descriptor.capability.state,
        policy: descriptor.policy,
        trust: descriptor.trust,
        status,
        diagnostic_code,
    })
}

fn persisted_project_target(
    database: &Database,
    project_root: &str,
    descriptor: &TargetDescriptor,
    path: &str,
) -> Result<Option<PersistedProjectTarget>, AppError> {
    let database_path = database.path().to_string_lossy();
    database
        .connection()
        .query_row(
            "SELECT project.id, target.id, target.row_version,
                    target.baseline_full_hash, target.baseline_managed_hash
             FROM managed_targets AS target
             JOIN projects AS project ON project.id = target.project_id
             WHERE project.root_path = ?1 AND target.tool = ?2
               AND target.artifact_kind = ?3 AND target.scope = 'project'
               AND target.target_path = ?4",
            rusqlite::params![
                project_root,
                descriptor.tool.as_str(),
                descriptor.artifact_kind.as_str(),
                path,
            ],
            |row| {
                Ok(PersistedProjectTarget {
                    project_id: row.get(0)?,
                    baseline: ManagedTargetBaseline {
                        target_id: row.get(1)?,
                        target_row_version: row.get(2)?,
                        full_hash: row.get(3)?,
                        managed_hash: row.get(4)?,
                    },
                })
            },
        )
        .optional()
        .map_err(|_| AppError::database(&database_path, "read_project_target_baseline"))
}

fn assess_managed_target(
    database: &Database,
    descriptor: &TargetDescriptor,
    persisted: &PersistedProjectTarget,
) -> Result<Option<crate::sync::DriftAssessment>, AppError> {
    let ownership = match descriptor.artifact_kind {
        ArtifactKind::Mcp => {
            let desired = mcp_repository::list_assigned_mcp_servers(
                database,
                descriptor.tool,
                Some(&persisted.project_id),
            )?
            .into_iter()
            .filter(|record| record.enabled)
            .collect::<Vec<_>>();
            let inherited =
                mcp_repository::list_assigned_mcp_servers(database, descriptor.tool, None)?
                    .into_iter()
                    .filter(|record| record.enabled)
                    .collect::<Vec<_>>();
            let existing =
                mcp_repository::list_managed_mcp_items(database, &persisted.baseline.target_id)?;
            let names = desired
                .iter()
                .chain(inherited.iter())
                .map(|record| record.name.clone())
                .chain(existing.iter().map(|item| item.external_key.clone()))
                .collect::<BTreeSet<_>>();
            if names.is_empty() {
                return Ok(None);
            }
            ManagedOwnership::selectors(
                names
                    .into_iter()
                    .map(|name| vec![native_mcp_container(descriptor.tool).to_owned(), name]),
            )
        }
        ArtifactKind::Skill => {
            let desired = skill_repository::list_assigned_skills(
                database,
                descriptor.tool,
                Some(&persisted.project_id),
            )?;
            let inherited =
                skill_repository::list_assigned_skills(database, descriptor.tool, None)?;
            let existing = skill_repository::list_managed_skill_items(
                database,
                &persisted.baseline.target_id,
            )?;
            let names = desired
                .iter()
                .chain(inherited.iter())
                .map(|record| record.name.clone())
                .chain(existing.iter().map(|item| item.external_key.clone()))
                .collect::<BTreeSet<_>>();
            if names.is_empty() {
                return Ok(None);
            }
            ManagedOwnership::SymlinkNames(names.into_iter().collect())
        }
        ArtifactKind::Provider | ArtifactKind::Prompt => return Ok(None),
    };
    let scan = verify_managed_item_baselines(
        database,
        descriptor,
        &persisted.baseline.target_id,
        scan_target(tool_adapter(descriptor.tool), descriptor, &ownership),
    )?;
    Ok(Some(assess_drift(descriptor, &persisted.baseline, &scan)))
}

fn verify_managed_item_baselines(
    database: &Database,
    descriptor: &TargetDescriptor,
    target_id: &str,
    scan: TargetScan,
) -> Result<TargetScan, AppError> {
    let matches = match descriptor.artifact_kind {
        ArtifactKind::Mcp => {
            let existing = mcp_repository::list_managed_mcp_items(database, target_id)?;
            if existing.is_empty() {
                return Ok(scan);
            }
            match &scan {
                TargetScan::Observed(observed) => observed
                    .managed_projection
                    .get(native_mcp_container(descriptor.tool))
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|items| {
                        existing.iter().all(|item| {
                            items.get(&item.external_key).is_some_and(|value| {
                                hash_json(value) == item.last_applied_item_hash
                            })
                        })
                    }),
                TargetScan::Missing => false,
                _ => return Ok(scan),
            }
        }
        ArtifactKind::Skill => {
            let existing = skill_repository::list_managed_skill_items(database, target_id)?;
            if existing.is_empty() {
                return Ok(scan);
            }
            match &scan {
                TargetScan::Observed(observed) => observed
                    .managed_projection
                    .as_object()
                    .is_some_and(|items| {
                        existing.iter().all(|item| {
                            items.get(&item.external_key).is_some_and(|value| {
                                hash_json(value) == item.last_applied_item_hash
                            })
                        })
                    }),
                TargetScan::Missing => false,
                _ => return Ok(scan),
            }
        }
        ArtifactKind::Provider | ArtifactKind::Prompt => return Ok(scan),
    };
    Ok(if matches {
        scan
    } else {
        TargetScan::ManagedItemBaselineMismatch
    })
}

fn native_mcp_container(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "mcpServers",
        Tool::Codex => "mcp_servers",
    }
}

fn status_from_unmanaged_scan(descriptor: &TargetDescriptor) -> (SyncStatus, Option<String>) {
    let ownership = match descriptor.artifact_kind {
        ArtifactKind::Mcp => ManagedOwnership::selectors([[descriptor
            .managed_selector_roots
            .first()
            .cloned()
            .unwrap_or_default()]]),
        ArtifactKind::Skill => ManagedOwnership::SymlinkNames(Vec::new()),
        ArtifactKind::Provider | ArtifactKind::Prompt => ManagedOwnership::WholeDocument,
    };
    match scan_target(tool_adapter(descriptor.tool), descriptor, &ownership) {
        TargetScan::Observed(_) => (
            SyncStatus::ExternalNonOwnedChange,
            Some("UNMANAGED_NATIVE_CONFIGURATION".to_owned()),
        ),
        TargetScan::Missing => (SyncStatus::Missing, None),
        TargetScan::ParseError => (
            SyncStatus::ParseError,
            Some("NATIVE_CONFIGURATION_PARSE_ERROR".to_owned()),
        ),
        TargetScan::PermissionDenied => (
            SyncStatus::PermissionDenied,
            Some("NATIVE_CONFIGURATION_PERMISSION_DENIED".to_owned()),
        ),
        TargetScan::TargetTypeChanged(_) => (
            SyncStatus::TargetTypeChanged,
            Some("NATIVE_TARGET_TYPE_CHANGED".to_owned()),
        ),
        TargetScan::ManagedItemBaselineMismatch => (
            SyncStatus::ExternalOwnedChange,
            Some("MANAGED_ITEM_BASELINE_MISMATCH".to_owned()),
        ),
        TargetScan::Failed | TargetScan::Unavailable => (
            SyncStatus::Failed,
            Some("NATIVE_CONFIGURATION_UNAVAILABLE".to_owned()),
        ),
    }
}

fn tool_adapter(tool: Tool) -> &'static dyn ToolAdapter {
    match tool {
        Tool::Claude => &ClaudeAdapter,
        Tool::Codex => &CodexAdapter,
    }
}

fn blocked_project_targets(diagnostic_code: &str) -> Vec<ProjectTargetStatusDto> {
    [Tool::Claude, Tool::Codex]
        .into_iter()
        .flat_map(|tool| {
            [ArtifactKind::Mcp, ArtifactKind::Skill]
                .into_iter()
                .map(move |artifact_kind| ProjectTargetStatusDto {
                    tool,
                    artifact_kind,
                    target_path: None,
                    capability: crate::adapters::CapabilityState::Unsupported,
                    policy: PolicyState::Unknown,
                    trust: TargetTrustState::Unknown,
                    status: SyncStatus::Failed,
                    diagnostic_code: Some(diagnostic_code.to_owned()),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use rusqlite::params;
    use tempfile::tempdir;

    use super::{register_project, remove_project, rescan_project};
    use crate::{
        adapters::{
            codex::CodexAdapter, DiscoveryContext, ExplicitEnvironment, ManagedOwnership,
            ToolAdapter, ToolAvailability,
        },
        app::AppPaths,
        db::Database,
        domain::{ArtifactKind, Scope, SyncStatus, Tool},
        error::ErrorCode,
        projects::{ProjectPathStatus, RegisterProjectInput, VersionedProjectInput},
        sync::{hash_json, scan_target, TargetScan},
    };

    struct Fixture {
        _temporary: tempfile::TempDir,
        database: Database,
        environment: ExplicitEnvironment,
        home: std::path::PathBuf,
    }

    fn fixture() -> Fixture {
        let temporary = tempdir().unwrap();
        let home = fs::canonicalize(temporary.path()).unwrap();
        fs::create_dir(home.join(".claude")).unwrap();
        fs::create_dir(home.join(".codex")).unwrap();
        let environment =
            ExplicitEnvironment::new(&home, None, None, ToolAvailability::all_installed()).unwrap();
        let paths = AppPaths::from_data_root(home.join("app-data")).unwrap();
        let database = Database::open(&paths).unwrap();
        Fixture {
            _temporary: temporary,
            database,
            environment,
            home,
        }
    }

    #[test]
    fn registration_canonicalizes_aliases_and_reactivates_removed_project() {
        let mut fixture = fixture();
        let project = fixture.home.join("project");
        fs::create_dir(&project).unwrap();
        let alias = fixture.home.join("project-alias");
        symlink(&project, &alias).unwrap();
        let registered = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Fixture".to_owned(),
                root_path: alias.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(registered.root_path, project.to_string_lossy());
        let duplicate = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Duplicate".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(duplicate.code(), ErrorCode::Conflict);
        remove_project(
            &mut fixture.database,
            &VersionedProjectInput {
                id: registered.id.clone(),
                row_version: registered.row_version,
            },
        )
        .unwrap();
        let reactivated = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Reactivated".to_owned(),
                root_path: alias.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        assert_eq!(reactivated.id, registered.id);
        assert_eq!(reactivated.display_name, "Reactivated");
    }

    #[test]
    fn project_root_identity_is_nocase_in_storage_and_lookup() {
        let mut fixture = fixture();
        let project = fixture.home.join("CaseSensitiveProject");
        fs::create_dir(&project).unwrap();
        let registered = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Fixture".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let folded = registered.root_path.to_ascii_lowercase();
        let found = crate::db::projects::find_project_by_root(&fixture.database, &folded)
            .unwrap()
            .unwrap();
        assert_eq!(found.id, registered.id);
        let duplicate = fixture.database.connection().execute(
            "INSERT INTO projects(id, display_name, root_path)
             VALUES ('00000000-0000-4000-8000-000000000714', 'Duplicate', ?1)",
            [folded],
        );
        assert!(duplicate.is_err());
    }

    #[test]
    fn rescan_reports_missing_root_without_touching_native_configuration() {
        let mut fixture = fixture();
        let project = fixture.home.join("project");
        fs::create_dir(&project).unwrap();
        let native = project.join(".mcp.json");
        fs::write(&native, "{\"fixture\":true}").unwrap();
        let registered = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Fixture".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        fs::remove_file(&native).unwrap();
        fs::remove_dir(&project).unwrap();
        let rescanned = rescan_project(
            &mut fixture.database,
            &fixture.environment,
            &VersionedProjectInput {
                id: registered.id,
                row_version: registered.row_version,
            },
        )
        .unwrap();
        assert_eq!(rescanned.path_status, ProjectPathStatus::Missing);
        assert!(rescanned.last_scanned_at.is_some());
    }

    #[test]
    fn rescan_reassesses_managed_target_instead_of_reusing_persisted_status() {
        let mut fixture = fixture();
        let project = fixture.home.join("project");
        fs::create_dir_all(project.join(".codex")).unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                project.to_string_lossy()
            ),
        )
        .unwrap();
        let native = project.join(".codex/config.toml");
        fs::write(&native, "[mcp_servers.fixture]\ncommand = \"before\"\n").unwrap();
        let registered = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Fixture".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .unwrap();

        let user_probe = crate::adapters::ConservativeClaudeUserMcpProbe;
        let policy_probe = crate::adapters::ConservativeClaudeCustomizationPolicyProbe;
        let project_root = crate::adapters::canonicalize_project_root(&project).unwrap();
        let context = DiscoveryContext {
            environment: &fixture.environment,
            project_root: Some(&project_root),
            claude_user_mcp_probe: &user_probe,
            claude_customization_policy_probe: &policy_probe,
        };
        let descriptor = CodexAdapter
            .discover(&context)
            .unwrap()
            .into_iter()
            .find(|target| {
                target.scope == Scope::Project && target.artifact_kind == ArtifactKind::Mcp
            })
            .unwrap();
        let ownership = ManagedOwnership::selectors([["mcp_servers", "fixture"]]);
        let TargetScan::Observed(observed) = scan_target(&CodexAdapter, &descriptor, &ownership)
        else {
            panic!("fixture 应产生可解析的 Codex MCP 目标");
        };
        let item_hash = hash_json(
            observed.managed_projection["mcp_servers"]
                .get("fixture")
                .unwrap(),
        );
        let mcp_id = "00000000-0000-4000-8000-000000000711";
        let target_id = "00000000-0000-4000-8000-000000000712";
        let item_id = "00000000-0000-4000-8000-000000000713";
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO mcp_servers(id, name, transport, command)
                 VALUES (?1, 'fixture', 'stdio', 'before')",
                [mcp_id],
            )
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO mcp_project_assignments(project_id, tool, mcp_id)
                 VALUES (?1, 'codex', ?2)",
                params![registered.id, mcp_id],
            )
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, project_id, target_path,
                    baseline_full_hash, baseline_managed_hash, last_status
                 ) VALUES (?1, 'codex', 'mcp', 'project', ?2, ?3, ?4, ?5, 'in_sync')",
                params![
                    target_id,
                    registered.id,
                    native.to_string_lossy(),
                    observed.full_hash,
                    observed.managed_hash,
                ],
            )
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_items(
                    id, target_id, resource_kind, resource_id, external_key,
                    last_applied_item_hash
                 ) VALUES (?1, ?2, 'mcp', ?3, 'fixture', ?4)",
                params![item_id, target_id, mcp_id, item_hash],
            )
            .unwrap();

        fs::write(
            &native,
            "[mcp_servers.fixture]\ncommand = \"externally-changed\"\n",
        )
        .unwrap();
        let rescanned = rescan_project(
            &mut fixture.database,
            &fixture.environment,
            &VersionedProjectInput {
                id: registered.id,
                row_version: registered.row_version,
            },
        )
        .unwrap();
        let codex_mcp = rescanned
            .targets
            .iter()
            .find(|target| target.tool == Tool::Codex && target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        assert_eq!(codex_mcp.status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            codex_mcp.diagnostic_code.as_deref(),
            Some("MANAGED_ITEM_BASELINE_MISMATCH")
        );
    }

    #[test]
    fn project_detail_detects_external_same_name_before_first_preview() {
        let mut fixture = fixture();
        let project = fixture.home.join("project");
        fs::create_dir_all(project.join(".codex")).unwrap();
        fs::write(
            fixture.home.join(".codex/config.toml"),
            format!(
                "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
                project.to_string_lossy()
            ),
        )
        .unwrap();
        fs::write(
            project.join(".codex/config.toml"),
            "[mcp_servers.fixture]\ncommand = \"external\"\n",
        )
        .unwrap();
        let registered = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Fixture".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let mcp_id = "00000000-0000-4000-8000-000000000716";
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO mcp_servers(id, name, transport, command)
                 VALUES (?1, 'fixture', 'stdio', 'desired')",
                [mcp_id],
            )
            .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO mcp_project_assignments(project_id, tool, mcp_id)
                 VALUES (?1, 'codex', ?2)",
                params![registered.id, mcp_id],
            )
            .unwrap();

        let detail =
            super::get_project(&fixture.database, &fixture.environment, &registered.id).unwrap();
        let codex_mcp = detail
            .targets
            .iter()
            .find(|target| target.tool == Tool::Codex && target.artifact_kind == ArtifactKind::Mcp)
            .unwrap();
        assert_eq!(codex_mcp.status, SyncStatus::ExternalOwnedChange);
        assert_eq!(
            codex_mcp.diagnostic_code.as_deref(),
            Some("EXTERNAL_OWNED_CHANGE")
        );
    }

    #[test]
    fn removal_only_changes_central_intent_and_preserves_project_files() {
        let mut fixture = fixture();
        let project = fixture.home.join("project");
        fs::create_dir(&project).unwrap();
        let native = project.join(".mcp.json");
        fs::write(&native, "{\"secret\":\"fixture-value\"}").unwrap();
        let before = fs::read(&native).unwrap();
        let registered = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Fixture".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        remove_project(
            &mut fixture.database,
            &VersionedProjectInput {
                id: registered.id,
                row_version: registered.row_version,
            },
        )
        .unwrap();
        assert_eq!(fs::read(&native).unwrap(), before);
    }

    #[test]
    fn removal_uses_cas_and_blocks_while_any_writer_needs_recovery() {
        let mut fixture = fixture();
        let project = fixture.home.join("project");
        fs::create_dir(&project).unwrap();
        let registered = register_project(
            &mut fixture.database,
            &fixture.environment,
            &RegisterProjectInput {
                display_name: "Fixture".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .unwrap();
        let run_id = "00000000-0000-4000-8000-000000000715";
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO sync_runs(id, kind, status, scope, db_version)
                 VALUES (?1, 'apply', 'applying', 'global', 1)",
                [run_id],
            )
            .unwrap();
        let blocked = remove_project(
            &mut fixture.database,
            &VersionedProjectInput {
                id: registered.id.clone(),
                row_version: registered.row_version,
            },
        )
        .unwrap_err();
        assert_eq!(blocked.code(), ErrorCode::WriteInProgress);
        fixture
            .database
            .connection()
            .execute(
                "UPDATE sync_runs SET status = 'succeeded' WHERE id = ?1",
                [run_id],
            )
            .unwrap();
        let stale = remove_project(
            &mut fixture.database,
            &VersionedProjectInput {
                id: registered.id.clone(),
                row_version: registered.row_version + 1,
            },
        )
        .unwrap_err();
        assert_eq!(stale.code(), ErrorCode::Conflict);
        remove_project(
            &mut fixture.database,
            &VersionedProjectInput {
                id: registered.id,
                row_version: registered.row_version,
            },
        )
        .unwrap();
    }
}
