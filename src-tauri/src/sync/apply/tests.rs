#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::{symlink, MetadataExt},
        path::{Path, PathBuf},
        process::Command,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Barrier, Mutex,
        },
        thread,
    };

    use rusqlite::params;
    use serde_json::{json, Value};
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::{
        apply_persisted_preview, claim_preview, delete_snapshots, detect_interrupted_run,
        journal_failure, list_snapshots, preview_restore, restore_snapshot, ApplyFaultDecision,
        ApplyFaultEvent, ApplyFaultInjector, ApplyTargetInput, DeleteSnapshotsInput,
        JournalOperation, ManagedItemApply, NoApplyFault, SnapshotStorageKind, TargetPhase,
    };
    use crate::{
        adapters::{
            CapabilityState, ManagedOwnership, PolicyState, PromptOverrideState, SymlinkPolicy,
            TargetCapability, TargetDescriptor, TargetFormat, TargetTrustState,
        },
        app::AppPaths,
        db::Database,
        domain::{ArtifactKind, ProjectRoot, Scope, Tool},
        error::{AppError, ErrorCode},
        git::inspect_path,
        security::{mode, SecretRedactor, PRIVATE_DIRECTORY_MODE, PRIVATE_FILE_MODE},
        skills::library as skill_library,
        sync::{
            build_preview_plan, load_managed_target_baseline, persist_preview, scan_target,
            ManagedTargetBaseline, PreviewTargetRequest, SkillTakeoverEntry,
            SkillTakeoverEntryType, TargetScan, WARNING_SKILL_TAKEOVER_CONFIRMATION,
        },
    };

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        targets: PathBuf,
        paths: AppPaths,
        database: Database,
        write_lock: Mutex<()>,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let targets = root.join("targets");
            fs::create_dir(&targets).unwrap();
            let paths = AppPaths::from_data_root(root.join("private/app-data")).unwrap();
            let database = Database::open(&paths).unwrap();
            Self {
                _temporary: temporary,
                root,
                targets,
                paths,
                database,
                write_lock: Mutex::new(()),
            }
        }
    }

    #[test]
    fn journal_enums_preserve_wire_names_and_unknown_values() {
        let phases = [
            (TargetPhase::Applying, "applying"),
            (TargetPhase::Claimed, "claimed"),
            (
                TargetPhase::CrashedAfterDatabaseFinalize,
                "crashed_after_database_finalize",
            ),
            (TargetPhase::CrashedAfterRename, "crashed_after_rename"),
            (
                TargetPhase::CrashedAfterRestoreTree,
                "crashed_after_restore_tree",
            ),
            (TargetPhase::CrashedAfterTakeover, "crashed_after_takeover"),
            (TargetPhase::CrashedAfterTarget, "crashed_after_target"),
            (
                TargetPhase::CrashedBeforeDatabaseFinalize,
                "crashed_before_database_finalize",
            ),
            (
                TargetPhase::CrashedBeforeNativeLink,
                "crashed_before_native_link",
            ),
            (
                TargetPhase::CrashedBeforeNativeRemove,
                "crashed_before_native_remove",
            ),
            (TargetPhase::CrashedBeforeRename, "crashed_before_rename"),
            (
                TargetPhase::CrashedBeforeRestoreTree,
                "crashed_before_restore_tree",
            ),
            (
                TargetPhase::CrashedBeforeTakeover,
                "crashed_before_takeover",
            ),
            (TargetPhase::CrashedBeforeTarget, "crashed_before_target"),
            (
                TargetPhase::CrashedDuringDatabaseFinalize,
                "crashed_during_database_finalize",
            ),
            (
                TargetPhase::DirectoryCreateFailed,
                "directory_create_failed",
            ),
            (
                TargetPhase::DirectoryCreatePending,
                "directory_create_pending",
            ),
            (TargetPhase::DirectoryCreated, "directory_created"),
            (
                TargetPhase::DirectoryRestorePending,
                "directory_restore_pending",
            ),
            (TargetPhase::DirectoryRestored, "directory_restored"),
            (
                TargetPhase::ExternalChangeAfterWrite,
                "external_change_after_write",
            ),
            (TargetPhase::NativeLinkPending, "native_link_pending"),
            (
                TargetPhase::ReadyToFinalizeDatabase,
                "ready_to_finalize_database",
            ),
            (TargetPhase::Removed, "removed"),
            (TargetPhase::RenameFailed, "rename_failed"),
            (TargetPhase::RenamePending, "rename_pending"),
            (TargetPhase::Renamed, "renamed"),
            (TargetPhase::RollbackFailed, "rollback_failed"),
            (TargetPhase::RolledBack, "rolled_back"),
            (TargetPhase::RollingBack, "rolling_back"),
            (TargetPhase::Snapshotted, "snapshotted"),
            (TargetPhase::Snapshotting, "snapshotting"),
            (TargetPhase::Succeeded, "succeeded"),
            (TargetPhase::TakeoverLinkFailed, "takeover_link_failed"),
            (TargetPhase::TakeoverLinked, "takeover_linked"),
            (TargetPhase::TakeoverQuarantined, "takeover_quarantined"),
            (TargetPhase::TakeoverRenameFailed, "takeover_rename_failed"),
            (
                TargetPhase::TakeoverRenamePending,
                "takeover_rename_pending",
            ),
            (TargetPhase::Verified, "verified"),
            (TargetPhase::Writing, "writing"),
            (TargetPhase::Written, "written"),
            (TargetPhase::Unknown, "unknown"),
        ];
        for (phase, expected) in phases {
            assert_eq!(phase.as_str(), expected);
            assert_eq!(serde_json::to_value(phase).unwrap(), json!(expected));
        }

        for (operation, expected) in [
            (JournalOperation::Apply, "apply"),
            (JournalOperation::Restore, "restore"),
            (JournalOperation::Unknown, "unknown"),
        ] {
            assert_eq!(operation.as_str(), expected);
            assert_eq!(serde_json::to_value(operation).unwrap(), json!(expected));
        }

        let legacy = json!({
            "version": 1,
            "run_id": "legacy-run",
            "operation": "operation_added_by_newer_version",
            "phase": "phase_added_by_newer_version",
            "targets": [{
                "target_id": "target-1",
                "target_path": "/tmp/target-1",
                "snapshot_id": null,
                "snapshot_path": null,
                "phase": "target_phase_added_by_newer_version",
                "before_fingerprint": null,
                "after_fingerprint": null,
                "temporary_path": null
            }]
        });
        let parsed: super::RunJournal = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.operation, JournalOperation::Unknown);
        assert_eq!(parsed.phase, TargetPhase::Unknown);
        assert_eq!(parsed.targets[0].phase, TargetPhase::Unknown);
        assert!(parsed.targets[0].phase.may_have_changed_target());
        assert!(!parsed.phase.is_crashed());

        let fixture = Fixture::new();
        let mut unknown_journal = parsed;
        unknown_journal.run_id = "unknown-run".to_owned();
        super::persist_journal(&fixture.paths, &unknown_journal).unwrap();
        assert!(super::journal_reports_crash(&fixture.paths, "unknown-run"));
    }

    #[test]
    fn rollback_journal_failure_always_has_a_stable_operation() {
        let failure = journal_failure(&AppError::conflict("targetPath", "回滚目标已被外部修改"));
        assert_eq!(failure.code, "CONFLICT");
        assert_eq!(failure.operation.as_deref(), Some("rollback"));
        assert!(failure.source.is_none());
    }

    fn file_descriptor(path: &Path, scope: Scope, project_root: Option<&Path>) -> TargetDescriptor {
        TargetDescriptor {
            tool: Tool::Claude,
            artifact_kind: ArtifactKind::Prompt,
            scope,
            project_root: project_root.map(|root| root.to_string_lossy().into_owned()),
            path: Some(path.to_string_lossy().into_owned()),
            allowed_root: None,
            mcp_container: None,
            format: TargetFormat::Markdown,
            managed_selector_roots: vec!["$document".to_owned()],
            sensitive_selectors: Vec::new(),
            capability: TargetCapability {
                state: CapabilityState::Supported,
                diagnostic_code: None,
            },
            policy: PolicyState::Allowed,
            trust: TargetTrustState::NotRequired,
            prompt_override: PromptOverrideState::NotApplicable,
            symlink_policy: SymlinkPolicy::Reject,
        }
    }

    fn insert_target_and_request(
        database: &Database,
        target_id: &str,
        descriptor: TargetDescriptor,
        desired: Value,
        git: Option<crate::git::GitPathStatus>,
        exclude_from_git: bool,
        project_id: Option<&str>,
    ) -> PreviewTargetRequest {
        let ownership = ManagedOwnership::WholeDocument;
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &descriptor,
            &ownership,
        );
        let (full_hash, managed_hash, projection) = match &scan {
            TargetScan::Observed(observed) => (
                Some(observed.full_hash.clone()),
                Some(observed.managed_hash.clone()),
                observed.managed_projection.clone(),
            ),
            TargetScan::Missing => (None, None, Value::Null),
            _ => panic!("测试目标必须是普通文件或缺失目标"),
        };
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, project_id, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'claude', ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    target_id,
                    descriptor.artifact_kind.as_str(),
                    descriptor.scope.as_str(),
                    project_id,
                    descriptor.path.as_deref().unwrap(),
                    full_hash,
                    managed_hash,
                    serde_json::to_string(&projection).unwrap(),
                ],
            )
            .unwrap();
        PreviewTargetRequest {
            hook_initial_adopt: false,
            descriptor,
            ownership,
            baseline: ManagedTargetBaseline {
                target_id: target_id.to_owned(),
                target_row_version: 1,
                full_hash,
                managed_hash,
            },
            scan,
            baseline_mismatched_items: Vec::new(),
            readopt_available: false,
            desired_projection: desired,
            row_versions: Vec::new(),
            git,
            exclude_from_git,
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
        }
    }

    fn persist_requests(
        database: &mut Database,
        scope: Scope,
        project_id: Option<String>,
        requests: Vec<PreviewTargetRequest>,
    ) -> String {
        let plan =
            build_preview_plan(scope, project_id, requests, &SecretRedactor::default()).unwrap();
        let preview_id = plan.preview_id.clone();
        persist_preview(database, &plan).unwrap();
        preview_id
    }

    fn input(
        descriptor: TargetDescriptor,
        desired: Value,
        allowed_root: &Path,
    ) -> ApplyTargetInput {
        ApplyTargetInput {
            descriptor,
            ownership: ManagedOwnership::WholeDocument,
            desired_projection: desired,
            allowed_root: allowed_root.to_path_buf(),
            central_skills_root: None,
            delete_target: false,
            managed_items: Vec::new(),
            remove_managed_item_ids: Vec::new(),
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
        }
    }

    #[derive(Clone, Copy)]
    enum InjectPhase {
        BeforeTarget,
        BeforeRename,
        AfterRename,
        AfterTarget,
        BeforeDatabaseFinalize,
        AfterDatabaseFinalize,
    }

    struct InjectFault {
        target_index: usize,
        phase: InjectPhase,
        decision: ApplyFaultDecision,
        sabotage: Option<PathBuf>,
    }

    impl ApplyFaultInjector for InjectFault {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            let matches = match (self.phase, event) {
                (InjectPhase::BeforeTarget, ApplyFaultEvent::BeforeTarget { index, .. })
                | (InjectPhase::BeforeRename, ApplyFaultEvent::BeforeRename { index, .. })
                | (InjectPhase::AfterRename, ApplyFaultEvent::AfterRename { index, .. })
                | (InjectPhase::AfterTarget, ApplyFaultEvent::AfterTarget { index, .. }) => {
                    *index == self.target_index
                }
                (InjectPhase::BeforeDatabaseFinalize, ApplyFaultEvent::BeforeDatabaseFinalize)
                | (InjectPhase::AfterDatabaseFinalize, ApplyFaultEvent::AfterDatabaseFinalize) => {
                    true
                }
                _ => false,
            };
            if !matches {
                return ApplyFaultDecision::Continue;
            }
            if let Some(path) = &self.sabotage {
                if path.is_dir() {
                    let _ = fs::remove_dir(path);
                } else {
                    let _ = fs::remove_file(path);
                }
                let _ = fs::create_dir(path);
            }
            self.decision
        }
    }

    struct ChangeBeforeWrite {
        target: PathBuf,
        database_path: Option<PathBuf>,
        target_id: Option<String>,
        before_rename: bool,
        after_rename: bool,
        changed: AtomicBool,
    }

    impl ApplyFaultInjector for ChangeBeforeWrite {
        fn decide(&self, event: &ApplyFaultEvent) -> ApplyFaultDecision {
            let should_change = if self.after_rename {
                matches!(event, ApplyFaultEvent::AfterRename { .. })
            } else if self.before_rename {
                matches!(event, ApplyFaultEvent::BeforeRename { .. })
            } else {
                matches!(event, ApplyFaultEvent::BeforeTarget { .. })
            };
            if should_change && !self.changed.swap(true, Ordering::SeqCst) {
                if let (Some(database_path), Some(target_id)) =
                    (&self.database_path, &self.target_id)
                {
                    rusqlite::Connection::open(database_path)
                        .unwrap()
                        .execute(
                            "UPDATE managed_targets SET last_status = 'failed' WHERE id = ?1",
                            [target_id],
                        )
                        .unwrap();
                } else {
                    fs::write(&self.target, "external-race").unwrap();
                }
            }
            ApplyFaultDecision::Continue
        }
    }

    /// 单个 WriteFile 目标一次 apply 的 IO 预算：目标文件完整读取 ≤ 2 次
    /// （规划阶段 1 次 + 写后校验 1 次；快照与 rename 前后的复核走 lstat 签名），
    /// 树审计只做 journals 作用域 1 次。fsync 统计见断言注释。
    #[test]
    fn single_write_file_apply_stays_within_io_budget() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("budget.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        super::TARGET_READS.with(|count| count.set(0));
        super::FSYNC_CALLS.with(|count| count.set(0));
        crate::security::AUDIT_TREE_CALLS.with(|count| count.set(0));
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("new"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(
            super::TARGET_READS.with(std::cell::Cell::get) <= 2,
            "目标文件完整读取次数：{}",
            super::TARGET_READS.with(std::cell::Cell::get)
        );
        assert_eq!(
            crate::security::AUDIT_TREE_CALLS.with(std::cell::Cell::get),
            1
        );
        // journal 10 个阶段（claimed/snapshotted/applying/writing/rename_pending/renamed/
        // written/verified/ready_to_finalize_database/succeeded）各 1 次 + journal 文件首次
        // 创建的目录 fsync 1 + 快照根 1 + 快照文件 1 + run 目录 1 + 临时文件 1 + 目标父目录 1
        // = 16。改动前 journal 每阶段走临时文件 + rename（3 次），合计约 36 次。
        // 每个 journal 阶段都保持各自的 durability——它们是崩溃恢复证据，不能为了
        // 数字再省。
        assert!(
            super::FSYNC_CALLS.with(std::cell::Cell::get) <= 16,
            "fsync 次数：{}",
            super::FSYNC_CALLS.with(std::cell::Cell::get)
        );
    }

    /// 旧版本写的是整文件单个 pretty JSON 对象；升级后必须仍能识别中断 run，
    /// 而恢复阶段追加的新行会成为最新状态。
    #[test]
    fn legacy_whole_object_journal_is_still_recognized_and_appended_to() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("legacy.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor.clone(), json!("new"), &fixture.targets)],
            &InjectFault {
                target_index: 0,
                phase: InjectPhase::AfterRename,
                decision: ApplyFaultDecision::Crash,
                sabotage: None,
            },
        )
        .unwrap_err();
        let journal_path = fixture.paths.journals().join(format!("{preview_id}.json"));
        let appended = fs::read_to_string(&journal_path).unwrap();
        assert!(appended.lines().count() > 1, "新格式应是多行追加记录");
        let latest = super::read_journal(&journal_path).unwrap().unwrap();
        assert_eq!(latest.targets[0].phase, TargetPhase::CrashedAfterRename);

        // 改写成旧格式：整文件一个 pretty-printed 对象。
        fs::write(&journal_path, serde_json::to_vec_pretty(&latest).unwrap()).unwrap();
        assert!(super::journal_reports_crash(&fixture.paths, &preview_id));
        let recovery = detect_interrupted_run(&fixture.database, &fixture.paths)
            .unwrap()
            .expect("旧格式 journal 仍必须识别出中断 run");
        assert_eq!(recovery.run_id, preview_id);
        assert!(recovery.journal_available);
        assert_eq!(recovery.targets.len(), 1);

        // 在旧格式文件上追加新阶段：读取方取最新一行而不是旧对象。
        let mut updated = latest.clone();
        updated.phase = TargetPhase::RolledBack;
        super::persist_journal(&fixture.paths, &updated).unwrap();
        let reread = super::read_journal(&journal_path).unwrap().unwrap();
        assert_eq!(reread.phase, TargetPhase::RolledBack);
    }

    #[test]
    fn truncated_latest_journal_line_falls_back_to_previous_complete_line() {
        let journal = super::RunJournal {
            version: 1,
            run_id: "complete-run".to_owned(),
            operation: JournalOperation::Apply,
            phase: TargetPhase::Claimed,
            targets: Vec::new(),
            failure: None,
        };
        let mut bytes = serde_json::to_vec(&journal).unwrap();
        bytes.extend_from_slice(b"\n{\"version\":1,\"run_id\":\"truncated");

        let parsed = super::parse_journal(&bytes).expect("应回退到上一条完整 journal");
        assert_eq!(parsed.run_id, "complete-run");
        assert_eq!(parsed.phase, TargetPhase::Claimed);
    }

    #[test]
    fn nth_target_failure_restores_all_prior_targets_without_partial_files() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("first.md");
        let second = fixture.targets.join("second.md");
        fs::write(&first, "old-first").unwrap();
        fs::write(&second, "old-second").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                "10000000-0000-4000-8000-000000000001",
                first_descriptor.clone(),
                json!("new-first"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                "10000000-0000-4000-8000-000000000002",
                second_descriptor.clone(),
                json!("new-second"),
                None,
                false,
                None,
            ),
        ];
        let preview_id = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[
                input(first_descriptor, json!("new-first"), &fixture.targets),
                input(second_descriptor, json!("new-second"), &fixture.targets),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Fail,
                sabotage: None,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
        assert_eq!(fs::read_to_string(&first).unwrap(), "old-first");
        assert_eq!(fs::read_to_string(&second).unwrap(), "old-second");
        let status: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT status FROM sync_runs WHERE id = ?1",
                [&preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "rolled_back");
        assert_eq!(list_snapshots(&fixture.database).unwrap().len(), 2);
        assert!(fs::read_dir(&fixture.targets).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".easytoagents-")
        }));
    }

    #[test]
    fn rollback_failure_preserves_journal_snapshots_and_unknown_directory() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("first.md");
        let second = fixture.targets.join("second.md");
        fs::write(&first, "old-first").unwrap();
        fs::write(&second, "old-second").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                "11000000-0000-4000-8000-000000000001",
                first_descriptor.clone(),
                json!("new-first"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                "11000000-0000-4000-8000-000000000002",
                second_descriptor.clone(),
                json!("new-second"),
                None,
                false,
                None,
            ),
        ];
        let preview_id = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[
                input(first_descriptor, json!("new-first"), &fixture.targets),
                input(second_descriptor, json!("new-second"), &fixture.targets),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Fail,
                sabotage: Some(first.clone()),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::RollbackFailed);
        assert!(first.is_dir(), "回滚绝不能递归删除故障注入的未知目录");
        assert!(fixture
            .paths
            .journals()
            .join(format!("{preview_id}.json"))
            .is_file());
        assert_eq!(list_snapshots(&fixture.database).unwrap().len(), 2);
    }

    #[test]
    fn same_and_different_preview_claims_are_atomic_across_connections() {
        fn run_case(same_preview: bool) {
            let mut fixture = Fixture::new();
            let first = fixture.targets.join("one.md");
            let second = fixture.targets.join("two.md");
            fs::write(&first, "one").unwrap();
            fs::write(&second, "two").unwrap();
            let first_descriptor = file_descriptor(&first, Scope::Global, None);
            let first_request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                first_descriptor,
                json!("next-one"),
                None,
                false,
                None,
            );
            let first_preview = persist_requests(
                &mut fixture.database,
                Scope::Global,
                None,
                vec![first_request],
            );
            let second_preview = if same_preview {
                first_preview.clone()
            } else {
                let second_descriptor = file_descriptor(&second, Scope::Global, None);
                let second_request = insert_target_and_request(
                    &fixture.database,
                    &Uuid::new_v4().to_string(),
                    second_descriptor,
                    json!("next-two"),
                    None,
                    false,
                    None,
                );
                persist_requests(
                    &mut fixture.database,
                    Scope::Global,
                    None,
                    vec![second_request],
                )
            };
            let second_database = Database::open(&fixture.paths).unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let paths = fixture.paths.clone();
            let first_id = first_preview.clone();
            let first_barrier = Arc::clone(&barrier);
            let first_thread = thread::spawn(move || {
                let mut database = fixture.database;
                first_barrier.wait();
                let journal = paths.journals().join(format!("{first_id}.json"));
                claim_preview(&mut database, &first_id, &journal)
            });
            let paths = fixture.paths.clone();
            let second_barrier = Arc::clone(&barrier);
            let second_thread = thread::spawn(move || {
                let mut database = second_database;
                second_barrier.wait();
                let journal = paths.journals().join(format!("{second_preview}.json"));
                claim_preview(&mut database, &second_preview, &journal)
            });
            let results = [first_thread.join().unwrap(), second_thread.join().unwrap()];
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
            let error = results.into_iter().find_map(Result::err).unwrap();
            assert_eq!(
                error.code(),
                if same_preview {
                    ErrorCode::PreviewAlreadyConsumed
                } else {
                    ErrorCode::WriteInProgress
                }
            );
            assert_eq!(fs::read_to_string(&first).unwrap(), "one");
            assert_eq!(fs::read_to_string(&second).unwrap(), "two");
        }
        run_case(true);
        run_case(false);
    }

    #[test]
    fn external_change_after_preview_is_stale_and_never_overwritten() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("prompt.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            "12000000-0000-4000-8000-000000000001",
            descriptor.clone(),
            json!("desired"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        fs::write(&target, "external").unwrap();
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("desired"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(target).unwrap(), "external");
    }

    #[test]
    fn database_version_change_after_preview_is_stale_without_external_write() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("prompt.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let target_id = "12500000-0000-4000-8000-000000000001";
        let request = insert_target_and_request(
            &fixture.database,
            target_id,
            descriptor.clone(),
            json!("desired"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        fixture
            .database
            .connection()
            .execute(
                "UPDATE managed_targets SET last_status = 'failed' WHERE id = ?1",
                [target_id],
            )
            .unwrap();
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("desired"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
    }

    #[test]
    fn database_and_file_changes_in_the_write_window_are_never_overwritten() {
        let mut database_fixture = Fixture::new();
        let database_target = database_fixture.targets.join("database-race.md");
        fs::write(&database_target, "old").unwrap();
        let database_descriptor = file_descriptor(&database_target, Scope::Global, None);
        let database_target_id = Uuid::new_v4().to_string();
        let database_request = insert_target_and_request(
            &database_fixture.database,
            &database_target_id,
            database_descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let database_preview = persist_requests(
            &mut database_fixture.database,
            Scope::Global,
            None,
            vec![database_request],
        );
        let database_path = database_fixture.database.path().to_path_buf();
        let error = apply_persisted_preview(
            &database_fixture.write_lock,
            &mut database_fixture.database,
            &database_fixture.paths,
            &database_preview,
            &[input(
                database_descriptor,
                json!("new"),
                &database_fixture.targets,
            )],
            &ChangeBeforeWrite {
                target: database_target.clone(),
                database_path: Some(database_path),
                target_id: Some(database_target_id),
                before_rename: false,
                after_rename: false,
                changed: AtomicBool::new(false),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(database_target).unwrap(), "old");

        let mut file_fixture = Fixture::new();
        let file_target = file_fixture.targets.join("file-race.md");
        fs::write(&file_target, "old").unwrap();
        let race_descriptor = file_descriptor(&file_target, Scope::Global, None);
        let file_request = insert_target_and_request(
            &file_fixture.database,
            &Uuid::new_v4().to_string(),
            race_descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let file_preview = persist_requests(
            &mut file_fixture.database,
            Scope::Global,
            None,
            vec![file_request],
        );
        let error = apply_persisted_preview(
            &file_fixture.write_lock,
            &mut file_fixture.database,
            &file_fixture.paths,
            &file_preview,
            &[input(race_descriptor, json!("new"), &file_fixture.targets)],
            &ChangeBeforeWrite {
                target: file_target.clone(),
                database_path: None,
                target_id: None,
                before_rename: true,
                after_rename: false,
                changed: AtomicBool::new(false),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(file_target).unwrap(), "external-race");
        assert!(fs::read_dir(&file_fixture.targets).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".easytoagents-")
        }));

        let mut after_fixture = Fixture::new();
        let after_target = after_fixture.targets.join("after-rename-race.md");
        fs::write(&after_target, "old").unwrap();
        let after_descriptor = file_descriptor(&after_target, Scope::Global, None);
        let after_request = insert_target_and_request(
            &after_fixture.database,
            &Uuid::new_v4().to_string(),
            after_descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let after_preview = persist_requests(
            &mut after_fixture.database,
            Scope::Global,
            None,
            vec![after_request],
        );
        let error = apply_persisted_preview(
            &after_fixture.write_lock,
            &mut after_fixture.database,
            &after_fixture.paths,
            &after_preview,
            &[input(
                after_descriptor,
                json!("new"),
                &after_fixture.targets,
            )],
            &ChangeBeforeWrite {
                target: after_target.clone(),
                database_path: None,
                target_id: None,
                before_rename: false,
                after_rename: true,
                changed: AtomicBool::new(false),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(after_target).unwrap(), "external-race");
    }

    #[test]
    fn snapshot_refuses_content_changed_after_the_render_preflight() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("snapshot-race.md");
        fs::write(&target, "old").unwrap();
        let expected = super::capture_path_state(&target).unwrap().fingerprint();
        fs::write(&target, "external-before-snapshot").unwrap();

        let error = super::create_snapshot(
            &mut fixture.database,
            &fixture.paths,
            super::SnapshotRequest {
                run_id: &Uuid::new_v4().to_string(),
                target_id: Some("16000000-0000-4000-8000-000000000001"),
                target_path: &target,
                allowed_root: &fixture.targets,
                central_root: None,
                expected_before_fingerprint: &expected,
                directory_tree_hash: None,
                known_state: None,
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(target).unwrap(),
            "external-before-snapshot"
        );
    }

    #[test]
    fn whole_document_delete_is_snapshotted_and_can_be_restored() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("prompt.md");
        fs::write(&target, "old-content").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let target_id = "12600000-0000-4000-8000-000000000001";
        let request = insert_target_and_request(
            &fixture.database,
            target_id,
            descriptor.clone(),
            json!(""),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        let mut delete_input = input(descriptor, json!(""), &fixture.targets);
        delete_input.delete_target = true;
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[delete_input],
            &NoApplyFault,
        )
        .unwrap();
        assert!(!target.exists());
        let hashes: (Option<String>, Option<String>) = fixture
            .database
            .connection()
            .query_row(
                "SELECT baseline_full_hash, baseline_managed_hash
                 FROM managed_targets WHERE id = ?1",
                [target_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(hashes, (None, None));
        let snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.run_id == preview_id)
            .unwrap();
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "old-content");
    }

    #[test]
    fn selector_owned_delete_can_never_remove_the_whole_file() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("settings.json");
        fs::write(&target, r#"{"env":{"OWNED":"old"},"user":"keep"}"#).unwrap();
        let mut descriptor = file_descriptor(&target, Scope::Global, None);
        descriptor.artifact_kind = ArtifactKind::Provider;
        descriptor.format = TargetFormat::Json;
        descriptor.managed_selector_roots = vec!["env".to_owned()];
        let ownership = ManagedOwnership::selectors([["env", "OWNED"]]);
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &descriptor,
            &ownership,
        );
        let TargetScan::Observed(observed) = &scan else {
            panic!("JSON fixture 必须可扫描");
        };
        let target_id = Uuid::new_v4().to_string();
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'claude', 'provider', 'global', ?2, ?3, ?4, ?5)",
                params![
                    target_id,
                    descriptor.path.as_deref().unwrap(),
                    observed.full_hash,
                    observed.managed_hash,
                    serde_json::to_string(&observed.managed_projection).unwrap(),
                ],
            )
            .unwrap();
        let preview_id = persist_requests(
            &mut fixture.database,
            Scope::Global,
            None,
            vec![PreviewTargetRequest {
                hook_initial_adopt: false,
                descriptor: descriptor.clone(),
                ownership: ownership.clone(),
                baseline: ManagedTargetBaseline {
                    target_id,
                    target_row_version: 1,
                    full_hash: Some(observed.full_hash.clone()),
                    managed_hash: Some(observed.managed_hash.clone()),
                },
                scan,
                baseline_mismatched_items: Vec::new(),
                readopt_available: false,
                desired_projection: json!({}),
                row_versions: Vec::new(),
                git: None,
                exclude_from_git: false,
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
        );
        let mut apply_input = input(descriptor, json!({}), &fixture.targets);
        apply_input.ownership = ownership;
        apply_input.delete_target = true;
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[apply_input],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert_eq!(
            fs::read_to_string(target).unwrap(),
            r#"{"env":{"OWNED":"old"},"user":"keep"}"#
        );
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
    }

    #[test]
    fn crashes_before_and_after_rename_block_writes_and_restore_from_a_second_snapshot() {
        for phase in [InjectPhase::BeforeRename, InjectPhase::AfterRename] {
            let mut fixture = Fixture::new();
            let target = fixture.targets.join("prompt.md");
            fs::write(&target, "old").unwrap();
            let descriptor = file_descriptor(&target, Scope::Global, None);
            let request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                descriptor.clone(),
                json!("new"),
                None,
                false,
                None,
            );
            let preview_id =
                persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
            let error = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &preview_id,
                &[input(descriptor.clone(), json!("new"), &fixture.targets)],
                &InjectFault {
                    target_index: 0,
                    phase,
                    decision: ApplyFaultDecision::Crash,
                    sabotage: None,
                },
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::AtomicWriteFailed);
            let recovery = detect_interrupted_run(&fixture.database, &fixture.paths)
                .unwrap()
                .expect("崩溃后必须检测到活动 run");
            assert_eq!(recovery.run_id, preview_id);
            assert!(recovery.journal_available);
            assert_eq!(recovery.targets.len(), 1);
            match phase {
                InjectPhase::BeforeRename => {
                    assert_eq!(fs::read_to_string(&target).unwrap(), "old")
                }
                InjectPhase::AfterRename => assert_eq!(fs::read_to_string(&target).unwrap(), "new"),
                InjectPhase::BeforeTarget
                | InjectPhase::AfterTarget
                | InjectPhase::BeforeDatabaseFinalize
                | InjectPhase::AfterDatabaseFinalize => unreachable!(),
            }

            let blocked_target = fixture.targets.join("blocked.md");
            fs::write(&blocked_target, "blocked-old").unwrap();
            let blocked_descriptor = file_descriptor(&blocked_target, Scope::Global, None);
            let blocked_request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                blocked_descriptor.clone(),
                json!("blocked-new"),
                None,
                false,
                None,
            );
            let blocked_preview = persist_requests(
                &mut fixture.database,
                Scope::Global,
                None,
                vec![blocked_request],
            );
            let blocked = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &blocked_preview,
                &[input(
                    blocked_descriptor,
                    json!("blocked-new"),
                    &fixture.targets,
                )],
                &NoApplyFault,
            )
            .unwrap_err();
            assert_eq!(blocked.code(), ErrorCode::WriteInProgress);
            assert_eq!(fs::read_to_string(blocked_target).unwrap(), "blocked-old");

            let snapshot = list_snapshots(&fixture.database)
                .unwrap()
                .into_iter()
                .find(|snapshot| snapshot.run_id == preview_id)
                .unwrap();
            let restore_preview = preview_restore(
                &mut fixture.database,
                &fixture.paths,
                &snapshot.snapshot_id,
                &fixture.targets,
            )
            .unwrap();
            restore_snapshot(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &restore_preview.preview_id,
                &fixture.targets,
                None,
            )
            .unwrap();
            assert_eq!(fs::read_to_string(&target).unwrap(), "old");
            let restore_snapshot_count: i64 = fixture
                .database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM snapshots WHERE run_id = ?1",
                    [&restore_preview.preview_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(restore_snapshot_count, 1, "恢复前必须创建二次快照");
            assert!(fs::read_dir(&fixture.targets).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".easytoagents-")
            }));
        }
    }

    #[test]
    fn crash_at_nth_target_records_partial_progress_for_recovery() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("first.md");
        let second = fixture.targets.join("second.md");
        fs::write(&first, "first-old").unwrap();
        fs::write(&second, "second-old").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                first_descriptor.clone(),
                json!("first-new"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                second_descriptor.clone(),
                json!("second-new"),
                None,
                false,
                None,
            ),
        ];
        let preview_id = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[
                input(first_descriptor, json!("first-new"), &fixture.targets),
                input(second_descriptor, json!("second-new"), &fixture.targets),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Crash,
                sabotage: None,
            },
        )
        .unwrap_err();
        assert_eq!(fs::read_to_string(first).unwrap(), "first-new");
        assert_eq!(fs::read_to_string(second).unwrap(), "second-old");
        let recovery = detect_interrupted_run(&fixture.database, &fixture.paths)
            .unwrap()
            .unwrap();
        assert_eq!(recovery.targets.len(), 2);
        assert!(recovery
            .targets
            .iter()
            .any(|target| target.phase == TargetPhase::Written.as_str()));
        assert!(recovery
            .targets
            .iter()
            .any(|target| target.phase == TargetPhase::CrashedBeforeTarget.as_str()));
    }

    #[test]
    fn restoring_one_snapshot_keeps_a_partial_multi_target_run_blocking() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("partial-first.md");
        let second = fixture.targets.join("partial-second.md");
        fs::write(&first, "first-old").unwrap();
        fs::write(&second, "second-old").unwrap();
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                first_descriptor,
                json!("first-new"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                second_descriptor,
                json!("second-new"),
                None,
                false,
                None,
            ),
        ];
        let source_run = persist_requests(&mut fixture.database, Scope::Global, None, requests);
        let persisted = super::load_persisted_preview(&fixture.database, &source_run).unwrap();
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &source_run,
            &[
                input(
                    persisted.items[0].envelope.descriptor.clone(),
                    json!("first-new"),
                    &fixture.targets,
                ),
                input(
                    persisted.items[1].envelope.descriptor.clone(),
                    json!("second-new"),
                    &fixture.targets,
                ),
            ],
            &InjectFault {
                target_index: 1,
                phase: InjectPhase::BeforeTarget,
                decision: ApplyFaultDecision::Crash,
                sabotage: None,
            },
        )
        .unwrap_err();
        let snapshots = list_snapshots(&fixture.database).unwrap();
        let second_snapshot = snapshots
            .iter()
            .find(|snapshot| {
                snapshot.run_id == source_run && snapshot.target_path == second.to_string_lossy()
            })
            .unwrap();
        let second_restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &second_snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &second_restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap();
        let source_status: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT status FROM sync_runs WHERE id = ?1",
                [&source_run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_status, "rollback_failed");
        assert_eq!(fs::read_to_string(&first).unwrap(), "first-new");
        assert_eq!(fs::read_to_string(&second).unwrap(), "second-old");

        let first_snapshot = snapshots
            .iter()
            .find(|snapshot| {
                snapshot.run_id == source_run && snapshot.target_path == first.to_string_lossy()
            })
            .unwrap();
        let first_restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &first_snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &first_restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap();
        let source_status: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT status FROM sync_runs WHERE id = ?1",
                [&source_run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_status, "rolled_back");
        assert_eq!(fs::read_to_string(first).unwrap(), "first-old");
    }

    #[test]
    fn crashes_on_both_sides_of_database_finalize_never_guess_rollback() {
        for phase in [
            InjectPhase::BeforeDatabaseFinalize,
            InjectPhase::AfterDatabaseFinalize,
        ] {
            let mut fixture = Fixture::new();
            let target = fixture.targets.join("finalize.md");
            fs::write(&target, "old").unwrap();
            let descriptor = file_descriptor(&target, Scope::Global, None);
            let request = insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                descriptor.clone(),
                json!("new"),
                None,
                false,
                None,
            );
            let preview_id =
                persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
            let error = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &preview_id,
                &[input(descriptor, json!("new"), &fixture.targets)],
                &InjectFault {
                    target_index: 0,
                    phase,
                    decision: ApplyFaultDecision::Crash,
                    sabotage: None,
                },
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::DatabaseError);
            assert_eq!(fs::read_to_string(&target).unwrap(), "new");
            let status: String = fixture
                .database
                .connection()
                .query_row(
                    "SELECT status FROM sync_runs WHERE id = ?1",
                    [&preview_id],
                    |row| row.get(0),
                )
                .unwrap();
            match phase {
                InjectPhase::BeforeDatabaseFinalize => {
                    assert_eq!(status, "applying");
                    assert!(detect_interrupted_run(&fixture.database, &fixture.paths)
                        .unwrap()
                        .is_some());
                }
                InjectPhase::AfterDatabaseFinalize => {
                    assert_eq!(status, "succeeded");
                    assert!(detect_interrupted_run(&fixture.database, &fixture.paths)
                        .unwrap()
                        .is_none());
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn invalid_managed_item_intent_is_rejected_before_snapshot_or_external_write() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("managed-item.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        let mut apply_input = input(descriptor, json!("new"), &fixture.targets);
        apply_input.managed_items.push(ManagedItemApply {
            id: Uuid::new_v4().to_string(),
            resource_kind: ArtifactKind::Prompt,
            resource_id: Uuid::new_v4().to_string(),
            external_key: "active-prompt".to_owned(),
            last_applied_item_hash: "not-a-sha256".to_owned(),
        });
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[apply_input],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
    }

    fn applied_missing_snapshot_fixture() -> (Fixture, PathBuf, String) {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("created.md");
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("created"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("created"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap();
        let snapshot_id = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.run_id == preview_id)
            .unwrap()
            .snapshot_id;
        (fixture, target, snapshot_id)
    }

    /// 通过真实 apply 产生一个已完成 run 下的私有快照，供删除用例使用。
    fn apply_snapshot_for_delete(
        fixture: &mut Fixture,
        file_name: &str,
    ) -> (PathBuf, String, String) {
        let target = fixture.targets.join(file_name);
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("created"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        let applied = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("created"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap();
        let snapshot_id = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.run_id == applied.run_id)
            .unwrap()
            .snapshot_id;
        (target, snapshot_id, applied.run_id)
    }

    fn snapshot_file(fixture: &Fixture, run_id: &str, snapshot_id: &str) -> PathBuf {
        fixture
            .paths
            .snapshots()
            .join(run_id)
            .join(format!("{snapshot_id}.snapshot"))
    }

    #[test]
    fn delete_snapshots_removes_rows_and_files_in_one_batch() {
        let mut fixture = Fixture::new();
        let (first_target, first_snapshot, first_run) =
            apply_snapshot_for_delete(&mut fixture, "delete-a.md");
        let (second_target, second_snapshot, second_run) =
            apply_snapshot_for_delete(&mut fixture, "delete-b.md");
        let first_file = snapshot_file(&fixture, &first_run, &first_snapshot);
        let second_file = snapshot_file(&fixture, &second_run, &second_snapshot);
        assert!(first_file.is_file());
        assert!(second_file.is_file());

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![first_snapshot.clone(), second_snapshot.clone()],
            },
        )
        .unwrap();
        assert_eq!(
            result.deleted_ids,
            vec![first_snapshot.clone(), second_snapshot.clone()]
        );
        assert!(result.failures.is_empty());
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
        assert!(!first_file.exists());
        assert!(!second_file.exists());
        // 删除只针对快照文件，原生目标保持不变。
        assert!(first_target.is_file());
        assert!(second_target.is_file());
    }

    #[test]
    fn delete_snapshots_removes_a_single_selected_snapshot() {
        let mut fixture = Fixture::new();
        let (_kept_target, kept_snapshot, _kept_run) =
            apply_snapshot_for_delete(&mut fixture, "keep.md");
        let (removed_target, removed_snapshot, removed_run) =
            apply_snapshot_for_delete(&mut fixture, "remove.md");
        let removed_file = snapshot_file(&fixture, &removed_run, &removed_snapshot);
        assert!(removed_file.is_file());

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![removed_snapshot.clone()],
            },
        )
        .unwrap();
        assert_eq!(result.deleted_ids, vec![removed_snapshot.clone()]);
        assert!(result.failures.is_empty());
        let remaining = list_snapshots(&fixture.database).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].snapshot_id, kept_snapshot);
        assert!(!removed_file.exists());
        assert!(removed_target.is_file());
    }

    #[test]
    fn delete_snapshots_rejects_snapshots_referenced_by_active_runs() {
        for status in ["applying", "restoring", "rollback_failed"] {
            let mut fixture = Fixture::new();
            let (_blocked_target, blocked_snapshot, blocked_run) =
                apply_snapshot_for_delete(&mut fixture, "blocked.md");
            let (_free_target, free_snapshot, _free_run) =
                apply_snapshot_for_delete(&mut fixture, "free.md");
            fixture
                .database
                .connection()
                .execute(
                    "UPDATE sync_runs SET status = ?1 WHERE id = ?2",
                    params![status, blocked_run],
                )
                .unwrap();
            let blocked_file = snapshot_file(&fixture, &blocked_run, &blocked_snapshot);
            assert!(blocked_file.is_file());

            let result = delete_snapshots(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &DeleteSnapshotsInput {
                    snapshot_ids: vec![blocked_snapshot.clone(), free_snapshot.clone()],
                },
            )
            .unwrap();
            assert_eq!(result.deleted_ids, vec![free_snapshot.clone()]);
            assert_eq!(result.failures.len(), 1);
            assert_eq!(result.failures[0].snapshot_id, blocked_snapshot);
            assert_eq!(result.failures[0].code, ErrorCode::Conflict.as_str());
            // 活动引用的快照保持原状：文件与 DB 行都必须保留。
            assert!(blocked_file.is_file());
            assert!(list_snapshots(&fixture.database)
                .unwrap()
                .iter()
                .any(|snapshot| snapshot.snapshot_id == blocked_snapshot));
        }
    }

    #[test]
    fn delete_snapshots_reports_missing_ids_without_blocking_the_batch() {
        let mut fixture = Fixture::new();
        let (_target, snapshot_id, run_id) = apply_snapshot_for_delete(&mut fixture, "survivor.md");
        let missing = "00000000-0000-4000-8000-0000000000ab";
        let file = snapshot_file(&fixture, &run_id, &snapshot_id);
        assert!(file.is_file());

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![missing.to_owned(), snapshot_id.clone()],
            },
        )
        .unwrap();
        assert_eq!(result.deleted_ids, vec![snapshot_id.clone()]);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].snapshot_id, missing);
        assert_eq!(result.failures[0].code, ErrorCode::NotFound.as_str());
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn delete_snapshots_treats_an_already_missing_file_as_deleted() {
        let mut fixture = Fixture::new();
        let (_target, snapshot_id, run_id) = apply_snapshot_for_delete(&mut fixture, "gone.md");
        let file = snapshot_file(&fixture, &run_id, &snapshot_id);
        assert!(file.is_file());
        // 文件已被外部清走：DB 行仍存在，删除应自愈成功而非报错。
        fs::remove_file(&file).unwrap();

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id.clone()],
            },
        )
        .unwrap();
        assert_eq!(result.deleted_ids, vec![snapshot_id.clone()]);
        assert!(result.failures.is_empty());
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn delete_snapshots_retires_rows_first_and_queues_undeletable_files() {
        let mut fixture = Fixture::new();
        // 目标事先存在，快照才是 payload_file（启动清理队列只处理这一类）。
        fs::write(fixture.targets.join("queued.md"), "before").unwrap();
        let (_target, snapshot_id, run_id) = apply_snapshot_for_delete(&mut fixture, "queued.md");
        let file = snapshot_file(&fixture, &run_id, &snapshot_id);
        assert!(file.is_file());
        // 让文件删除失败：把同名路径换成目录，`remove_file` 必然出错。
        // （权限审计会在删除前把目录权限统一回 0700，所以不能用只读父目录来注入。）
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id.clone()],
            },
        )
        .unwrap();
        // 行已退役：列表中不再出现，不留下指向文件的活行。
        assert_eq!(result.deleted_ids, vec![snapshot_id.clone()]);
        assert!(result.failures.is_empty());
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
        // 磁盘条目仍在，并登记到可重试的清理队列。
        assert!(file.exists());
        let queued: (String, String, String) = fixture
            .database
            .connection()
            .query_row(
                "SELECT run_id, snapshot_path, storage_kind FROM retired_snapshot_cleanup
                 WHERE snapshot_id = ?1",
                [&snapshot_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(queued.0, run_id);
        assert_eq!(queued.1, file.to_string_lossy());
        assert_eq!(queued.2, "payload_file");

        // 队列只清理普通文件：目录留在队列里等待，恢复成普通文件后下次打开即被清理。
        fs::remove_dir(&file).unwrap();
        fs::write(&file, "stale payload").unwrap();
        drop(fixture.database);
        let reopened = Database::open(&fixture.paths).unwrap();
        assert!(!file.exists());
        let remaining: i64 = reopened
            .connection()
            .query_row("SELECT COUNT(*) FROM retired_snapshot_cleanup", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 0);
        fixture.database = reopened;
    }

    #[test]
    fn delete_snapshots_leaves_no_cleanup_queue_entry_after_a_successful_removal() {
        let mut fixture = Fixture::new();
        let (_target, snapshot_id, run_id) = apply_snapshot_for_delete(&mut fixture, "clean.md");
        let file = snapshot_file(&fixture, &run_id, &snapshot_id);
        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id.clone()],
            },
        )
        .unwrap();
        assert_eq!(result.deleted_ids, vec![snapshot_id.clone()]);
        assert!(!file.exists());
        let queued: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM retired_snapshot_cleanup WHERE snapshot_id = ?1",
                [&snapshot_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 0);
    }

    #[test]
    fn apply_rejects_inputs_without_a_target_path_instead_of_collapsing_them() {
        let mut fixture = Fixture::new();
        let first = fixture.targets.join("keyed-a.md");
        let second = fixture.targets.join("keyed-b.md");
        let first_descriptor = file_descriptor(&first, Scope::Global, None);
        let second_descriptor = file_descriptor(&second, Scope::Global, None);
        let requests = vec![
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                first_descriptor.clone(),
                json!("a"),
                None,
                false,
                None,
            ),
            insert_target_and_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                second_descriptor.clone(),
                json!("b"),
                None,
                false,
                None,
            ),
        ];
        let preview_id = persist_requests(&mut fixture.database, Scope::Global, None, requests);

        // 两个没有路径的输入以前会折叠成同一个空字符串键、互相覆盖，然后以
        // 误导性的 stale_preview 失败；现在必须以明确的 unsupportedTarget 拒绝。
        let mut pathless_first = first_descriptor.clone();
        pathless_first.path = None;
        let mut pathless_second = second_descriptor.clone();
        pathless_second.path = None;
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[
                input(pathless_first, json!("a"), &fixture.targets),
                input(pathless_second, json!("b"), &fixture.targets),
            ],
            &NoApplyFault,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(
            error
                .details()
                .and_then(|details| details.get("target"))
                .and_then(Value::as_str),
            Some("unsupportedTarget")
        );
        assert!(!first.exists());
        assert!(!second.exists());
    }

    #[test]
    fn delete_snapshots_fails_closed_on_mismatched_storage_path() {
        let mut fixture = Fixture::new();
        let (target, snapshot_id, run_id) = apply_snapshot_for_delete(&mut fixture, "guard.md");
        let stored_file = snapshot_file(&fixture, &run_id, &snapshot_id);
        assert!(stored_file.is_file());
        // 模拟越权记录：DB 的 snapshot_path 指向快照根之外的目标文件。
        fixture
            .database
            .connection()
            .execute(
                "UPDATE snapshots SET snapshot_path = ?1 WHERE id = ?2",
                params![target.to_string_lossy(), snapshot_id],
            )
            .unwrap();

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id.clone()],
            },
        )
        .unwrap();
        assert!(result.deleted_ids.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].snapshot_id, snapshot_id);
        assert_eq!(result.failures[0].code, ErrorCode::Conflict.as_str());
        // 冒充路径与真实快照文件都未被删除，DB 行保留。
        assert!(target.is_file());
        assert!(stored_file.is_file());
        assert_eq!(list_snapshots(&fixture.database).unwrap().len(), 1);
    }

    #[test]
    fn delete_snapshots_deduplicates_repeated_ids() {
        let mut fixture = Fixture::new();
        let (_target, snapshot_id, run_id) = apply_snapshot_for_delete(&mut fixture, "dup.md");
        let file = snapshot_file(&fixture, &run_id, &snapshot_id);
        assert!(file.is_file());

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: vec![snapshot_id.clone(), snapshot_id.clone()],
            },
        )
        .unwrap();
        assert_eq!(result.deleted_ids, vec![snapshot_id.clone()]);
        assert!(result.failures.is_empty());
        assert!(list_snapshots(&fixture.database).unwrap().is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn delete_snapshots_accepts_an_empty_input() {
        let mut fixture = Fixture::new();
        let (_target, snapshot_id, _run_id) = apply_snapshot_for_delete(&mut fixture, "idle.md");

        let result = delete_snapshots(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &DeleteSnapshotsInput {
                snapshot_ids: Vec::new(),
            },
        )
        .unwrap();
        assert!(result.deleted_ids.is_empty());
        assert!(result.failures.is_empty());
        let remaining = list_snapshots(&fixture.database).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].snapshot_id, snapshot_id);
    }

    #[test]
    fn restore_never_deletes_unknown_directory_or_external_symlink() {
        let (mut directory_fixture, directory_target, directory_snapshot) =
            applied_missing_snapshot_fixture();
        fs::remove_file(&directory_target).unwrap();
        fs::create_dir(&directory_target).unwrap();
        let preview = preview_restore(
            &mut directory_fixture.database,
            &directory_fixture.paths,
            &directory_snapshot,
            &directory_fixture.targets,
        )
        .unwrap();
        let error = restore_snapshot(
            &directory_fixture.write_lock,
            &mut directory_fixture.database,
            &directory_fixture.paths,
            &preview.preview_id,
            &directory_fixture.targets,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert!(directory_target.is_dir());

        let (mut link_fixture, link_target, link_snapshot) = applied_missing_snapshot_fixture();
        fs::remove_file(&link_target).unwrap();
        let outside = link_fixture.root.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, &link_target).unwrap();
        let central = link_fixture.root.join("central");
        fs::create_dir(&central).unwrap();
        let preview = preview_restore(
            &mut link_fixture.database,
            &link_fixture.paths,
            &link_snapshot,
            &link_fixture.targets,
        )
        .unwrap();
        let error = restore_snapshot(
            &link_fixture.write_lock,
            &mut link_fixture.database,
            &link_fixture.paths,
            &preview.preview_id,
            &link_fixture.targets,
            Some(&central),
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(fs::read_link(&link_target).unwrap(), outside);
    }

    #[test]
    fn restore_rechecks_snapshot_row_version_before_any_external_write() {
        let (mut fixture, target, snapshot_id) = applied_missing_snapshot_fixture();
        let preview = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        fixture
            .database
            .connection()
            .execute(
                "UPDATE snapshots SET target_type = target_type WHERE id = ?1",
                [&snapshot_id],
            )
            .unwrap();
        let error = restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::StalePreview);
        assert_eq!(fs::read_to_string(target).unwrap(), "created");
        let second_snapshot_count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE run_id = ?1",
                [&preview.preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(second_snapshot_count, 0);
    }

    #[test]
    fn recovery_cleanup_refuses_a_replaced_temporary_file() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("temporary.md");
        fs::write(&target, "old").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("new"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("new"), &fixture.targets)],
            &InjectFault {
                target_index: 0,
                phase: InjectPhase::BeforeRename,
                decision: ApplyFaultDecision::Crash,
                sabotage: None,
            },
        )
        .unwrap_err();
        let journal =
            super::read_journal(&fixture.paths.journals().join(format!("{preview_id}.json")))
                .unwrap()
                .expect("崩溃后必须留下 journal");
        let temporary = PathBuf::from(
            journal.targets[0]
                .temporary_path
                .as_deref()
                .expect("rename 前崩溃必须记录临时路径"),
        );
        fs::write(&temporary, "unknown replacement").unwrap();
        let snapshot_id = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.run_id == preview_id)
            .unwrap()
            .snapshot_id;
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        let error = restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &fixture.targets,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Conflict);
        assert_eq!(
            fs::read_to_string(&temporary).unwrap(),
            "unknown replacement"
        );
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
    }

    #[test]
    fn late_ancestor_symlink_escape_is_rejected_without_touching_outside() {
        let mut fixture = Fixture::new();
        let parent = fixture.targets.join("nested");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("prompt.md");
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            &Uuid::new_v4().to_string(),
            descriptor.clone(),
            json!("desired"),
            None,
            false,
            None,
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        fs::remove_dir(&parent).unwrap();
        let outside = fixture.root.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, &parent).unwrap();
        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[input(descriptor, json!("desired"), &fixture.targets)],
            &NoApplyFault,
        )
        .unwrap_err();
        assert!(matches!(
            error.code(),
            ErrorCode::Conflict | ErrorCode::StalePreview
        ));
        assert!(!outside.join("prompt.md").exists());
    }

    fn skill_descriptor(path: &Path) -> TargetDescriptor {
        TargetDescriptor {
            tool: Tool::Claude,
            artifact_kind: ArtifactKind::Skill,
            scope: Scope::Global,
            project_root: None,
            path: Some(path.to_string_lossy().into_owned()),
            allowed_root: None,
            mcp_container: None,
            format: TargetFormat::SymlinkDirectory,
            managed_selector_roots: vec!["$children".to_owned()],
            sensitive_selectors: Vec::new(),
            capability: TargetCapability::supported(),
            policy: PolicyState::Allowed,
            trust: TargetTrustState::NotRequired,
            prompt_override: PromptOverrideState::NotApplicable,
            symlink_policy: SymlinkPolicy::ManagedChildrenOnly,
        }
    }

    fn skill_request(
        database: &Database,
        target_id: &str,
        descriptor: TargetDescriptor,
        ownership: ManagedOwnership,
        desired: Value,
    ) -> PreviewTargetRequest {
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &descriptor,
            &ownership,
        );
        let (full_hash, managed_hash, projection) = match &scan {
            TargetScan::Observed(observed) => (
                Some(observed.full_hash.clone()),
                Some(observed.managed_hash.clone()),
                observed.managed_projection.clone(),
            ),
            TargetScan::Missing => (None, None, Value::Null),
            _ => panic!("Skills 目录 fixture 必须存在或缺失"),
        };
        database
            .connection()
            .execute(
                "INSERT INTO managed_targets(
                    id, tool, artifact_kind, scope, target_path,
                    baseline_full_hash, baseline_managed_hash, baseline_projection_json
                 ) VALUES (?1, 'claude', 'skill', 'global', ?2, ?3, ?4, ?5)",
                params![
                    target_id,
                    descriptor.path.as_deref().unwrap(),
                    full_hash,
                    managed_hash,
                    serde_json::to_string(&projection).unwrap(),
                ],
            )
            .unwrap();
        PreviewTargetRequest {
            hook_initial_adopt: false,
            descriptor,
            ownership,
            baseline: ManagedTargetBaseline {
                target_id: target_id.to_owned(),
                target_row_version: 1,
                full_hash,
                managed_hash,
            },
            scan,
            baseline_mismatched_items: Vec::new(),
            readopt_available: false,
            desired_projection: desired,
            row_versions: Vec::new(),
            git: None,
            exclude_from_git: false,
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
        }
    }

    #[test]
    fn rollback_preserves_a_concurrently_replaced_created_skill_directory() {
        let mut fixture = Fixture::new();
        let skills = fixture.targets.join("nested/skills");
        let created_parent = fixture.targets.join("nested");
        let central = fixture.root.join("central-skills");
        let central_skill = central.join("owned-skill");
        fs::create_dir_all(&central_skill).unwrap();
        let descriptor = skill_descriptor(&skills);
        let ownership = ManagedOwnership::SymlinkNames(vec!["owned-skill".to_owned()]);
        let desired = json!({
            "owned-skill": {
                "targetType": "symlink",
                "linkTarget": central_skill.to_string_lossy(),
            }
        });
        let request = skill_request(
            &fixture.database,
            "13000000-0000-4000-8000-000000000099",
            descriptor.clone(),
            ownership.clone(),
            desired.clone(),
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);

        let error = apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[ApplyTargetInput {
                descriptor,
                ownership,
                desired_projection: desired,
                allowed_root: fixture.targets.clone(),
                central_skills_root: Some(central),
                delete_target: false,
                managed_items: Vec::new(),
                remove_managed_item_ids: Vec::new(),
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
            &InjectFault {
                target_index: 0,
                phase: InjectPhase::AfterTarget,
                decision: ApplyFaultDecision::Fail,
                sabotage: Some(created_parent.clone()),
            },
        )
        .unwrap_err();

        assert_eq!(error.code(), ErrorCode::RollbackFailed);
        assert!(created_parent.is_dir());
        assert!(fs::read_dir(&created_parent).unwrap().next().is_none());
        assert!(fixture
            .paths
            .journals()
            .join(format!("{preview_id}.json"))
            .is_file());
    }

    #[test]
    fn managed_symlink_uses_atomic_rename_and_refuses_directory_or_external_link() {
        let mut fixture = Fixture::new();
        let skills = fixture.targets.join("skills");
        let central = fixture.root.join("central-skills");
        let central_skill = central.join("owned-skill");
        fs::create_dir(&skills).unwrap();
        fs::create_dir_all(&central_skill).unwrap();
        let descriptor = skill_descriptor(&skills);
        let ownership = ManagedOwnership::SymlinkNames(vec!["owned-skill".to_owned()]);
        let desired = json!({
            "owned-skill": {
                "targetType": "symlink",
                "linkTarget": central_skill.to_string_lossy(),
            }
        });
        let request = skill_request(
            &fixture.database,
            "13000000-0000-4000-8000-000000000001",
            descriptor.clone(),
            ownership.clone(),
            desired.clone(),
        );
        let preview_id =
            persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[ApplyTargetInput {
                descriptor,
                ownership,
                desired_projection: desired,
                allowed_root: fixture.targets.clone(),
                central_skills_root: Some(central.clone()),
                delete_target: false,
                managed_items: Vec::new(),
                remove_managed_item_ids: Vec::new(),
                skill_takeover_entries: Vec::new(),
                project_native_action: None,
            }],
            &NoApplyFault,
        )
        .unwrap();
        assert_eq!(
            fs::canonicalize(skills.join("owned-skill")).unwrap(),
            central_skill
        );
        assert!(fs::read_dir(&skills).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".easytoagents-")
        }));
        let child_path = skills.join("owned-skill");
        let child_snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| {
                snapshot.run_id == preview_id
                    && snapshot.target_path == child_path.to_string_lossy()
            })
            .unwrap();
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &child_snapshot.snapshot_id,
            &fixture.targets,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &fixture.targets,
            Some(&central),
        )
        .unwrap();
        assert!(fs::symlink_metadata(child_path).is_err());

        for occupied_by_directory in [true, false] {
            let mut fixture = Fixture::new();
            let skills = fixture.targets.join("skills");
            let central = fixture.root.join("central-skills");
            fs::create_dir(&skills).unwrap();
            fs::create_dir(&central).unwrap();
            let child = skills.join("unknown");
            let outside = fixture.root.join("outside-skill");
            fs::create_dir(&outside).unwrap();
            if occupied_by_directory {
                fs::create_dir(&child).unwrap();
            } else {
                symlink(&outside, &child).unwrap();
            }
            let descriptor = skill_descriptor(&skills);
            let ownership = ManagedOwnership::SymlinkNames(vec!["unknown".to_owned()]);
            let request = skill_request(
                &fixture.database,
                &Uuid::new_v4().to_string(),
                descriptor.clone(),
                ownership.clone(),
                json!({}),
            );
            let preview_id =
                persist_requests(&mut fixture.database, Scope::Global, None, vec![request]);
            let error = apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &preview_id,
                &[ApplyTargetInput {
                    descriptor,
                    ownership,
                    desired_projection: json!({}),
                    allowed_root: fixture.targets.clone(),
                    central_skills_root: Some(central),
                    delete_target: false,
                    managed_items: Vec::new(),
                    remove_managed_item_ids: Vec::new(),
                    skill_takeover_entries: Vec::new(),
                    project_native_action: None,
                }],
                &NoApplyFault,
            )
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::Conflict);
            if occupied_by_directory {
                assert!(child.is_dir());
            } else {
                assert_eq!(fs::read_link(child).unwrap(), outside);
            }
        }
    }

    #[test]
    fn explicit_skill_takeover_preserves_external_source_and_restores_directory_tree() {
        for entry_type in [
            SkillTakeoverEntryType::ExternalSymlink,
            SkillTakeoverEntryType::Directory,
        ] {
            let mut fixture = Fixture::new();
            let skills = fixture.targets.join("skills");
            let child = skills.join("owned-skill");
            let external = fixture.root.join("external-skill");
            let central = fixture.root.join("central-skills");
            let central_skill = central.join("owned-skill");
            fs::create_dir(&skills).unwrap();
            fs::create_dir(&central).unwrap();
            fs::create_dir(&external).unwrap();
            fs::write(
                external.join("SKILL.md"),
                "---\nname: owned-skill\ndescription: 接管测试\n---\n正文\n",
            )
            .unwrap();
            fs::write(external.join("asset.txt"), "外部内容").unwrap();
            match entry_type {
                SkillTakeoverEntryType::ExternalSymlink => symlink(&external, &child).unwrap(),
                SkillTakeoverEntryType::Directory => {
                    let source_hash = skill_library::inspect_skill_takeover_entry(&external)
                        .unwrap()
                        .content_hash;
                    skill_library::copy_skill_tree(&external, &child, &source_hash).unwrap();
                }
            }
            let inspection = skill_library::inspect_skill_takeover_entry(&child).unwrap();
            skill_library::copy_skill_tree(&external, &central_skill, &inspection.content_hash)
                .unwrap();
            let external_before = fs::metadata(external.join("SKILL.md")).unwrap();
            let descriptor = skill_descriptor(&skills);
            let ownership = ManagedOwnership::SymlinkNames(vec!["owned-skill".to_owned()]);
            let desired = json!({
                "owned-skill": {
                    "targetType": "symlink",
                    "linkTarget": central_skill.to_string_lossy(),
                }
            });
            let target_id = Uuid::new_v4().to_string();
            fixture
                .database
                .connection()
                .execute(
                    "INSERT INTO managed_targets(
                        id, tool, artifact_kind, scope, target_path,
                        baseline_full_hash, baseline_managed_hash, baseline_projection_json
                     ) VALUES (?1, 'claude', 'skill', 'global', ?2, NULL, NULL, 'null')",
                    params![&target_id, descriptor.path.as_deref().unwrap()],
                )
                .unwrap();
            let scan = scan_target(
                &crate::adapters::claude::ClaudeAdapter,
                &descriptor,
                &ownership,
            );
            let takeover = SkillTakeoverEntry {
                name: "owned-skill".to_owned(),
                entry_path: child.to_string_lossy().into_owned(),
                entry_type,
                expected_fingerprint: inspection.fingerprint,
                content_hash: inspection.content_hash.clone(),
                central_path: central_skill.to_string_lossy().into_owned(),
            };
            let plan = build_preview_plan(
                Scope::Global,
                None,
                vec![PreviewTargetRequest {
                    hook_initial_adopt: false,
                    descriptor: descriptor.clone(),
                    ownership: ownership.clone(),
                    baseline: ManagedTargetBaseline {
                        target_id,
                        target_row_version: 1,
                        full_hash: None,
                        managed_hash: None,
                    },
                    scan,
                    baseline_mismatched_items: Vec::new(),
                    readopt_available: false,
                    desired_projection: desired.clone(),
                    row_versions: Vec::new(),
                    git: None,
                    exclude_from_git: false,
                    skill_takeover_entries: vec![takeover.clone()],
                    project_native_action: None,
                }],
                &SecretRedactor::default(),
            )
            .unwrap();
            assert_eq!(
                plan.targets[0].change_kind,
                crate::domain::ChangeKind::Update
            );
            assert!(plan
                .warning_codes
                .iter()
                .any(|code| code == WARNING_SKILL_TAKEOVER_CONFIRMATION));
            let preview_id = plan.preview_id.clone();
            persist_preview(&mut fixture.database, &plan).unwrap();
            apply_persisted_preview(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &preview_id,
                &[ApplyTargetInput {
                    descriptor,
                    ownership,
                    desired_projection: desired,
                    allowed_root: fixture.targets.clone(),
                    central_skills_root: Some(central.clone()),
                    delete_target: false,
                    managed_items: Vec::new(),
                    remove_managed_item_ids: Vec::new(),
                    skill_takeover_entries: vec![takeover],
                    project_native_action: None,
                }],
                &NoApplyFault,
            )
            .unwrap();

            assert_eq!(fs::canonicalize(&child).unwrap(), central_skill);
            assert_eq!(
                fs::read_to_string(external.join("asset.txt")).unwrap(),
                "外部内容"
            );
            let external_after = fs::metadata(external.join("SKILL.md")).unwrap();
            assert_eq!(external_before.ino(), external_after.ino());
            assert!(fs::read_dir(&skills).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".easytoagents-")
            }));

            let snapshot = list_snapshots(&fixture.database)
                .unwrap()
                .into_iter()
                .find(|snapshot| {
                    snapshot.run_id == preview_id && snapshot.target_path == child.to_string_lossy()
                })
                .unwrap();
            let expected_storage = match entry_type {
                SkillTakeoverEntryType::ExternalSymlink => SnapshotStorageKind::MetadataOnly,
                SkillTakeoverEntryType::Directory => SnapshotStorageKind::DirectoryTree,
            };
            assert_eq!(snapshot.storage_kind, expected_storage);
            assert!(snapshot.restorable);
            let restore = preview_restore(
                &mut fixture.database,
                &fixture.paths,
                &snapshot.snapshot_id,
                &fixture.targets,
            )
            .unwrap();
            assert_eq!(restore.storage_kind, expected_storage);
            restore_snapshot(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &restore.preview_id,
                &fixture.targets,
                Some(&central),
            )
            .unwrap();
            match entry_type {
                SkillTakeoverEntryType::ExternalSymlink => {
                    assert_eq!(fs::read_link(&child).unwrap(), external);
                }
                SkillTakeoverEntryType::Directory => {
                    assert!(fs::symlink_metadata(&child).unwrap().is_dir());
                    skill_library::verify_skill_tree(&child, &inspection.content_hash).unwrap();
                }
            }
            let deletion = delete_snapshots(
                &fixture.write_lock,
                &mut fixture.database,
                &fixture.paths,
                &DeleteSnapshotsInput {
                    snapshot_ids: vec![snapshot.snapshot_id.clone()],
                },
            )
            .unwrap();
            assert_eq!(deletion.deleted_ids, vec![snapshot.snapshot_id]);
            assert!(deletion.failures.is_empty());
        }
    }

    fn git_init(path: &Path) {
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn tracked_is_warning_only_and_untracked_exclude_requires_explicit_confirmation() {
        let mut fixture = Fixture::new();
        let repository = fixture.targets.join("repository");
        fs::create_dir(&repository).unwrap();
        git_init(&repository);
        let project_root = fs::canonicalize(&repository).unwrap();
        let project_id = "14000000-0000-4000-8000-000000000001";
        fixture
            .database
            .connection()
            .execute(
                "INSERT INTO projects(id, display_name, root_path, is_git_repo)
                 VALUES (?1, 'fixture', ?2, 1)",
                params![project_id, project_root.to_string_lossy()],
            )
            .unwrap();
        let project = ProjectRoot::parse(&project_root).unwrap();
        let exclude_path = repository.join(".git/info/exclude");
        let original_exclude = fs::read(&exclude_path).unwrap();

        let tracked_target = repository.join("tracked.md");
        fs::write(&tracked_target, "tracked-old").unwrap();
        assert!(Command::new("git")
            .args(["add", "--", "tracked.md"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());
        let tracked_descriptor =
            file_descriptor(&tracked_target, Scope::Project, Some(&project_root));
        let mut tracked_descriptor = tracked_descriptor;
        tracked_descriptor.artifact_kind = ArtifactKind::Mcp;
        let tracked_git = inspect_path(&project, &tracked_target).unwrap();
        assert!(tracked_git.tracked);
        let tracked_request = insert_target_and_request(
            &fixture.database,
            "14000000-0000-4000-8000-000000000002",
            tracked_descriptor.clone(),
            json!("tracked-new"),
            Some(tracked_git),
            true,
            Some(project_id),
        );
        let tracked_plan = build_preview_plan(
            Scope::Project,
            Some(project_id.to_owned()),
            vec![tracked_request],
            &SecretRedactor::default(),
        )
        .unwrap();
        assert!(tracked_plan.targets[0]
            .warning_codes
            .iter()
            .any(|code| code == "GIT_TRACKED"));
        assert!(!tracked_plan.targets[0].exclude_from_git);
        let tracked_preview = tracked_plan.preview_id.clone();
        persist_preview(&mut fixture.database, &tracked_plan).unwrap();
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &tracked_preview,
            &[input(
                tracked_descriptor,
                json!("tracked-new"),
                &project_root,
            )],
            &NoApplyFault,
        )
        .unwrap();
        assert_eq!(fs::read(&exclude_path).unwrap(), original_exclude);

        let untracked_target = repository.join(".codex/config.toml");
        fs::create_dir(repository.join(".codex")).unwrap();
        let untracked_descriptor =
            file_descriptor(&untracked_target, Scope::Project, Some(&project_root));
        let mut untracked_descriptor = untracked_descriptor;
        untracked_descriptor.artifact_kind = ArtifactKind::Mcp;
        let untracked_git = inspect_path(&project, &untracked_target).unwrap();
        assert!(!untracked_git.tracked);
        let target_id = "14000000-0000-4000-8000-000000000003";
        let untracked_request = insert_target_and_request(
            &fixture.database,
            target_id,
            untracked_descriptor.clone(),
            json!("local-only"),
            Some(untracked_git),
            true,
            Some(project_id),
        );
        let untracked_preview = persist_requests(
            &mut fixture.database,
            Scope::Project,
            Some(project_id.to_owned()),
            vec![untracked_request],
        );
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &untracked_preview,
            &[input(
                untracked_descriptor.clone(),
                json!("local-only"),
                &project_root,
            )],
            &NoApplyFault,
        )
        .unwrap();
        let exclude_once = fs::read_to_string(&exclude_path).unwrap();
        assert_eq!(exclude_once.matches("EasyToAgents managed").count(), 2);
        assert!(exclude_once.contains("/.codex/config.toml"));
        assert!(!repository.join(".gitignore").exists());

        let ownership = ManagedOwnership::WholeDocument;
        let scan = scan_target(
            &crate::adapters::claude::ClaudeAdapter,
            &untracked_descriptor,
            &ownership,
        );
        let baseline = load_managed_target_baseline(&fixture.database, target_id).unwrap();
        let second_request = PreviewTargetRequest {
            hook_initial_adopt: false,
            descriptor: untracked_descriptor.clone(),
            ownership,
            baseline,
            scan,
            baseline_mismatched_items: Vec::new(),
            readopt_available: false,
            desired_projection: json!("local-only"),
            row_versions: Vec::new(),
            git: Some(inspect_path(&project, &untracked_target).unwrap()),
            exclude_from_git: true,
            skill_takeover_entries: Vec::new(),
            project_native_action: None,
        };
        let second_preview = persist_requests(
            &mut fixture.database,
            Scope::Project,
            Some(project_id.to_owned()),
            vec![second_request],
        );
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &second_preview,
            &[input(
                untracked_descriptor,
                json!("local-only"),
                &project_root,
            )],
            &NoApplyFault,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&exclude_path).unwrap(), exclude_once);
        assert!(!repository.join(".gitignore").exists());

        let exclude_snapshot = list_snapshots(&fixture.database)
            .unwrap()
            .into_iter()
            .find(|snapshot| {
                snapshot.run_id == untracked_preview
                    && snapshot.target_path == exclude_path.to_string_lossy()
            })
            .unwrap();
        let restore = preview_restore(
            &mut fixture.database,
            &fixture.paths,
            &exclude_snapshot.snapshot_id,
            &project_root,
        )
        .unwrap();
        restore_snapshot(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &restore.preview_id,
            &project_root,
            None,
        )
        .unwrap();
        assert_eq!(fs::read(&exclude_path).unwrap(), original_exclude);
        assert!(!repository.join(".gitignore").exists());
    }

    #[test]
    fn journal_and_database_preview_never_contain_registered_secret() {
        let mut fixture = Fixture::new();
        let target = fixture.targets.join("secret.md");
        fs::write(&target, "old-safe").unwrap();
        let descriptor = file_descriptor(&target, Scope::Global, None);
        let request = insert_target_and_request(
            &fixture.database,
            "15000000-0000-4000-8000-000000000001",
            descriptor.clone(),
            json!("fixture-phase3-secret"),
            None,
            false,
            None,
        );
        let mut redactor = SecretRedactor::default();
        redactor.register_secret("fixture-phase3-secret");
        let plan = build_preview_plan(Scope::Global, None, vec![request], &redactor).unwrap();
        let preview_id = plan.preview_id.clone();
        persist_preview(&mut fixture.database, &plan).unwrap();
        let managed_item_id = "15000000-0000-4000-8000-000000000002";
        let mut apply_input = input(descriptor, json!("fixture-phase3-secret"), &fixture.targets);
        apply_input.managed_items.push(ManagedItemApply {
            id: managed_item_id.to_owned(),
            resource_kind: ArtifactKind::Prompt,
            resource_id: "15000000-0000-4000-8000-000000000003".to_owned(),
            external_key: "active-prompt".to_owned(),
            last_applied_item_hash: "a".repeat(64),
        });
        apply_persisted_preview(
            &fixture.write_lock,
            &mut fixture.database,
            &fixture.paths,
            &preview_id,
            &[apply_input],
            &NoApplyFault,
        )
        .unwrap();
        let journal =
            fs::read_to_string(fixture.paths.journals().join(format!("{preview_id}.json")))
                .unwrap();
        assert!(!journal.contains("fixture-phase3-secret"));
        assert_eq!(
            mode(&fixture.paths.snapshots().join(&preview_id)).unwrap(),
            PRIVATE_DIRECTORY_MODE
        );
        assert_eq!(
            mode(&fixture.paths.journals().join(format!("{preview_id}.json"))).unwrap(),
            PRIVATE_FILE_MODE
        );
        let snapshot_path: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT snapshot_path FROM snapshots WHERE run_id = ?1 LIMIT 1",
                [&preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mode(Path::new(&snapshot_path)).unwrap(), PRIVATE_FILE_MODE);
        let preview_row: String = fixture
            .database
            .connection()
            .query_row(
                "SELECT redacted_diff_json FROM sync_items WHERE run_id = ?1",
                [&preview_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!preview_row.contains("fixture-phase3-secret"));
        let managed_count: i64 = fixture
            .database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM managed_items WHERE id = ?1",
                [managed_item_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(managed_count, 1);
    }
}
