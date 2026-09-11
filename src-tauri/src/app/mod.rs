//! 应用初始化、私有路径与共享状态容器。

pub mod tool_probe;

use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use serde::Serialize;
use specta::Type;

use crate::{
    adapters::{ExplicitEnvironment, PolicyState, ToolAvailabilityState, PROFILE_TOOLS},
    db::Database,
    domain::Tool,
    error::{AppError, ErrorCode},
    security::{
        audit_private_tree, ensure_private_directory, reject_symlink_components, SecretRedactor,
    },
    skills::{migrate_legacy_central_skill_directories, reconcile_skill_target_baselines},
    sync::detect_interrupted_run,
};

const APPLICATION_SUPPORT_DIRECTORY: &str = "EasyToAgents";

/// 应用拥有的路径集合。构造过程不读取 HOME 或任何工具环境变量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    data_root: PathBuf,
    database: PathBuf,
    central_skills: PathBuf,
    central_hooks: PathBuf,
    snapshots: PathBuf,
    staging: PathBuf,
    journals: PathBuf,
    database_backups: PathBuf,
    logs: PathBuf,
}

impl AppPaths {
    pub fn from_data_root(data_root: impl Into<PathBuf>) -> Result<Self, AppError> {
        let data_root = data_root.into();
        validate_private_data_root(&data_root)?;
        reject_symlink_components(&data_root)?;
        if fs::symlink_metadata(&data_root).is_ok_and(|metadata| !metadata.is_dir()) {
            return Err(AppError::invalid_input(
                "privatePath",
                "应用私有路径必须是目录或尚未创建的路径",
            ));
        }
        Ok(Self {
            database: data_root.join("easytoagents.sqlite3"),
            central_skills: data_root.join("skills"),
            central_hooks: data_root.join("hooks"),
            snapshots: data_root.join("snapshots"),
            staging: data_root.join("staging"),
            journals: data_root.join("journals"),
            database_backups: data_root.join("database-backups"),
            logs: data_root.join("logs"),
            data_root,
        })
    }

    /// 纯路径解析，仅供运行时显式传入 home；测试必须传入隔离目录。
    pub fn for_macos_home(home: &Path) -> Result<Self, AppError> {
        validate_absolute_path(home)?;
        Self::from_data_root(
            home.join("Library")
                .join("Application Support")
                .join(APPLICATION_SUPPORT_DIRECTORY),
        )
    }

    /// 启动期一次性：建齐私有目录并对整棵数据树做权限审计。全树审计会递归
    /// `chmod` + `canonicalize` 每个快照与 Skill 文件，只应在 `AppState` 初始化时跑一次。
    pub fn initialize(&self) -> Result<(), AppError> {
        self.ensure_directories()?;
        audit_private_tree(&self.data_root)?;
        Ok(())
    }

    /// 只保证私有目录存在（幂等、廉价），不做全树审计；供 `Database::open` 等复用。
    pub fn ensure_directories(&self) -> Result<(), AppError> {
        for directory in self.private_directories() {
            ensure_private_directory(directory)?;
        }
        Ok(())
    }

    /// 写路径的作用域审计：只覆盖 `journals/` 与给定 run 的 `snapshots/<run_id>/`，
    /// 不再每次 apply/restore 都遍历全部历史快照与中央 Skill 库。
    pub fn audit_run_scope<'a>(
        &self,
        run_ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), AppError> {
        let run_ids = run_ids.into_iter().collect::<Vec<_>>();
        for run_id in &run_ids {
            validate_run_id(run_id)?;
        }
        audit_private_tree(&self.journals)?;
        for run_id in run_ids {
            let run_directory = self.snapshots.join(run_id);
            match fs::symlink_metadata(&run_directory) {
                Ok(_) => {
                    audit_private_tree(&run_directory)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(AppError::permission(
                        &run_directory.to_string_lossy(),
                        "lstat_snapshot_run",
                    )
                    .with_source(error));
                }
            }
        }
        Ok(())
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn database(&self) -> &Path {
        &self.database
    }

    pub fn database_wal(&self) -> PathBuf {
        companion_path(&self.database, "-wal")
    }

    pub fn database_shm(&self) -> PathBuf {
        companion_path(&self.database, "-shm")
    }

    pub fn central_skills(&self) -> &Path {
        &self.central_skills
    }

    pub fn central_hooks(&self) -> &Path {
        &self.central_hooks
    }

    pub fn snapshots(&self) -> &Path {
        &self.snapshots
    }

    pub fn staging(&self) -> &Path {
        &self.staging
    }

    pub fn journals(&self) -> &Path {
        &self.journals
    }

    pub fn database_backups(&self) -> &Path {
        &self.database_backups
    }

    pub fn logs(&self) -> &Path {
        &self.logs
    }

    fn private_directories(&self) -> [&Path; 8] {
        [
            &self.data_root,
            &self.central_skills,
            &self.central_hooks,
            &self.snapshots,
            &self.staging,
            &self.journals,
            &self.database_backups,
            &self.logs,
        ]
    }
}

/// run ID 只允许作为快照目录下的单一普通分量；在数据库 claim 前也不能让它
/// 通过 `Path::join` 影响应用私有目录之外的权限审计范围。
fn validate_run_id(run_id: &str) -> Result<(), AppError> {
    let path = Path::new(run_id);
    if run_id.is_empty()
        || run_id.as_bytes().contains(&0)
        || path.components().count() != 1
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(AppError::invalid_input(
            "runId",
            "同步 run 标识必须是单一安全路径段",
        ));
    }
    Ok(())
}

/// 后台/刷新探测所需的全部显式输入；不读进程环境（那只在 setup 里做一次）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentProbeConfig {
    pub input: tool_probe::ReleaseToolProbeInput,
    pub claude_provider_policy: PolicyState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallationDto {
    pub tool: Tool,
    pub availability: ToolAvailabilityState,
    pub installation_version: Option<String>,
    pub installation_probe_diagnostic: Option<String>,
}

/// 前端可读的环境探测状态：`probing == true` 时 `tools` 为空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStateDto {
    pub probing: bool,
    pub tools: Vec<ToolInstallationDto>,
}

/// 探测完成（含刷新）后的通知回调；发布进程用它向前端广播 Tauri 事件。
pub type EnvironmentReadyNotifier = Box<dyn Fn(bool) + Send + Sync>;

pub struct AppState {
    /// `Arc` 让 async 命令能把数据库句柄移进阻塞线程池，而不必持有 `State` 借用。
    database: Arc<Mutex<Database>>,
    write_operations: Mutex<()>,
    paths: AppPaths,
    redactor: RwLock<SecretRedactor>,
    /// `None` 表示探测仍在后台进行；命令层据此返回 `ENVIRONMENT_PROBING`。
    /// 用 `Arc` 让命令在线程池里拿到一份不受后续刷新影响的快照。
    environment: RwLock<Option<Arc<ExplicitEnvironment>>>,
    probe: Option<EnvironmentProbeConfig>,
    environment_ready: Option<EnvironmentReadyNotifier>,
    github_proxy: Option<String>,
}

impl AppState {
    pub fn initialize(paths: AppPaths) -> Result<Self, AppError> {
        Self::initialize_internal(paths, None, None, None, None)
    }

    pub fn initialize_with_environment(
        paths: AppPaths,
        environment: ExplicitEnvironment,
    ) -> Result<Self, AppError> {
        Self::initialize_internal(paths, Some(environment), None, None, None)
    }

    /// 发布启动路径：数据库先就绪、主窗口先显示，环境由 `probe_environment` 在后台补上。
    pub fn initialize_probing(
        paths: AppPaths,
        probe: EnvironmentProbeConfig,
        github_proxy: Option<String>,
        environment_ready: EnvironmentReadyNotifier,
    ) -> Result<Self, AppError> {
        Self::initialize_internal(
            paths,
            None,
            Some(probe),
            Some(environment_ready),
            github_proxy,
        )
    }

    fn initialize_internal(
        paths: AppPaths,
        environment: Option<ExplicitEnvironment>,
        probe: Option<EnvironmentProbeConfig>,
        environment_ready: Option<EnvironmentReadyNotifier>,
        github_proxy: Option<String>,
    ) -> Result<Self, AppError> {
        // 唯一的全树审计：`Database::open` 与写路径都只做作用域内审计。
        paths.initialize()?;
        let mut database = Database::open(&paths)?;
        migrate_legacy_central_skill_directories(&mut database, &paths)?;
        // 中断的同步等待用户显式回滚；对账只处理无活动写入者的记账漂移。
        // 中断状态不缓存在 AppState 里：`get_interrupted_run` 与 `restore_snapshot`
        // 每次都从 sync_runs 重新检测，缓存副本从未被读取，只会随时间陈旧。
        if detect_interrupted_run(&database, &paths)?.is_none() {
            reconcile_skill_target_baselines(&database);
        }
        Ok(Self {
            database: Arc::new(Mutex::new(database)),
            write_operations: Mutex::new(()),
            paths,
            redactor: RwLock::new(SecretRedactor::default()),
            environment: RwLock::new(environment.map(Arc::new)),
            probe,
            environment_ready,
            github_proxy,
        })
    }

    pub fn database(&self) -> &Mutex<Database> {
        &self.database
    }

    /// 取得数据库锁；持锁线程 panic 留下的中毒标记直接恢复。数据库的每次写入都在
    /// SQLite 事务里，panic 只会回滚未提交事务，不会留下需要拒绝后续命令的半状态。
    pub fn database_guard(&self) -> MutexGuard<'_, Database> {
        self.database.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn redactor_read(&self) -> RwLockReadGuard<'_, SecretRedactor> {
        self.redactor.read().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn redactor_write(&self) -> RwLockWriteGuard<'_, SecretRedactor> {
        self.redactor
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Apply/Restore/接管共用的写串行锁；同样从中毒中恢复（写入本身有 journal 与快照兜底）。
    pub fn lock_write_operations(&self) -> MutexGuard<'_, ()> {
        self.write_operations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 可移进阻塞线程池的数据库句柄；锁语义与 `database()` 完全相同。
    pub fn database_handle(&self) -> Arc<Mutex<Database>> {
        Arc::clone(&self.database)
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// Apply 与 Restore 共用同一把进程内互斥锁；SQLite 部分唯一索引负责跨实例兜底。
    pub fn write_operations(&self) -> &Mutex<()> {
        &self.write_operations
    }

    pub fn redactor(&self) -> &RwLock<SecretRedactor> {
        &self.redactor
    }

    pub fn environment(&self) -> Result<Arc<ExplicitEnvironment>, AppError> {
        self.environment
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(environment_probing_error)
    }

    pub fn github_proxy(&self) -> Option<&str> {
        self.github_proxy.as_deref()
    }

    /// 同步执行（或重新执行）工具探测并替换环境快照，然后通知监听方。
    /// 探测会启动子进程并等待最多数秒，调用方必须把它放到线程池里。
    pub fn probe_environment(&self) -> Result<Arc<ExplicitEnvironment>, AppError> {
        let outcome = self.probe_environment_inner();
        if let Some(notify) = self.environment_ready.as_ref() {
            notify(outcome.is_ok());
        }
        outcome
    }

    fn probe_environment_inner(&self) -> Result<Arc<ExplicitEnvironment>, AppError> {
        let probe = self.probe.as_ref().ok_or_else(|| {
            AppError::invalid_input("environment", "当前进程没有配置工具探测输入")
        })?;
        let environment = Arc::new(
            tool_probe::probe_release_environment(&probe.input)?
                .environment
                .with_claude_provider_policy(probe.claude_provider_policy),
        );
        *self
            .environment
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&environment));
        Ok(environment)
    }

    pub fn environment_state(&self) -> Result<EnvironmentStateDto, AppError> {
        let environment = self
            .environment
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        Ok(match environment.as_deref() {
            None => EnvironmentStateDto {
                probing: true,
                tools: Vec::new(),
            },
            Some(environment) => EnvironmentStateDto {
                probing: false,
                tools: PROFILE_TOOLS
                    .into_iter()
                    .map(|tool| ToolInstallationDto {
                        tool,
                        availability: environment.tool_availability(tool),
                        installation_version: environment
                            .installation_version(tool)
                            .map(str::to_owned),
                        installation_probe_diagnostic: environment
                            .installation_probe_diagnostic(tool)
                            .map(str::to_owned),
                    })
                    .collect(),
            },
        })
    }
}

fn environment_probing_error() -> AppError {
    AppError::new(
        ErrorCode::EnvironmentProbing,
        "工具环境仍在检测中，请稍后重试",
        true,
    )
}

fn validate_absolute_path(path: &Path) -> Result<(), AppError> {
    if !path.is_absolute() {
        return Err(AppError::invalid_input(
            "privatePath",
            "应用私有路径必须是绝对路径",
        ));
    }
    if path == Path::new("/") {
        return Err(AppError::invalid_input(
            "privatePath",
            "应用私有路径不能是文件系统根目录",
        ));
    }
    use std::path::Component;
    if path.components().any(|component| {
        matches!(
            component,
            Component::CurDir | Component::ParentDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::invalid_input(
            "privatePath",
            "应用私有路径不能包含相对片段",
        ));
    }
    Ok(())
}

fn validate_private_data_root(path: &Path) -> Result<(), AppError> {
    validate_absolute_path(path)?;
    let depth = path
        .components()
        .filter(|component| matches!(component, std::path::Component::Normal(_)))
        .count();
    if depth < 3 {
        return Err(AppError::invalid_input(
            "privatePath",
            "应用私有路径不能是系统目录、用户主目录或其他过宽根目录",
        ));
    }
    Ok(())
}

fn companion_path(database: &Path, suffix: &str) -> PathBuf {
    let file_name = database
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    database.with_file_name(format!("{file_name}{suffix}"))
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use tempfile::tempdir;

    use super::{AppPaths, AppState};
    use crate::security::{mode, PRIVATE_DIRECTORY_MODE};

    #[test]
    fn macos_paths_resolve_under_explicit_isolated_home() {
        let temporary = tempdir().unwrap();
        let isolated_home = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::for_macos_home(&isolated_home).unwrap();
        assert_eq!(
            paths.data_root(),
            isolated_home.join("Library/Application Support/EasyToAgents")
        );
        assert_eq!(paths.central_skills(), paths.data_root().join("skills"));
        assert_eq!(paths.snapshots(), paths.data_root().join("snapshots"));
        assert_eq!(paths.staging(), paths.data_root().join("staging"));
    }

    #[test]
    fn initialize_creates_private_directory_layout() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("private-data")).unwrap();
        paths.initialize().unwrap();

        for directory in [
            paths.data_root(),
            paths.central_skills(),
            paths.snapshots(),
            paths.staging(),
            paths.journals(),
            paths.database_backups(),
        ] {
            assert!(directory.is_dir());
            assert_eq!(mode(directory).unwrap(), PRIVATE_DIRECTORY_MODE);
        }
    }

    #[test]
    fn private_root_rejects_broad_or_relative_paths() {
        assert!(AppPaths::from_data_root("relative-data").is_err());
        assert!(AppPaths::from_data_root("/").is_err());
        assert!(AppPaths::from_data_root("/tmp").is_err());
        assert!(AppPaths::from_data_root("/Users/example").is_err());
        assert!(AppPaths::from_data_root("/tmp/../escape").is_err());
    }

    #[test]
    fn private_root_rejects_an_existing_symlink_component() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let outside = isolated_root.join("outside");
        fs::create_dir(&outside).unwrap();
        let linked = isolated_root.join("linked");
        symlink(&outside, &linked).unwrap();

        assert!(AppPaths::from_data_root(linked.join("private-data")).is_err());
    }

    #[test]
    fn audit_run_scope_rejects_path_traversal_and_empty_ids() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("audit-scope-data")).unwrap();
        paths.ensure_directories().unwrap();

        for run_id in ["", ".", "..", "../outside", "nested/run", "/tmp/outside"] {
            assert!(
                paths.audit_run_scope([run_id]).is_err(),
                "不安全 run ID 不应进入权限审计：{run_id:?}"
            );
        }
        paths.audit_run_scope(["safe-run"]).unwrap();
    }

    /// 持锁线程 panic 后锁被标记中毒；后续命令必须仍能拿到锁并正常工作，
    /// 而不是永远返回"应用状态锁不可用"。
    #[test]
    fn a_panic_while_holding_the_database_lock_does_not_poison_later_commands() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("poison-data")).unwrap();
        let state = std::sync::Arc::new(AppState::initialize(paths).unwrap());
        let poisoner = std::sync::Arc::clone(&state);
        let outcome = std::thread::spawn(move || {
            let _database = poisoner.database().lock().unwrap();
            let _redactor = poisoner.redactor().write().unwrap();
            let _write = poisoner.write_operations().lock().unwrap();
            panic!("模拟命令执行中 panic");
        })
        .join();
        assert!(outcome.is_err());
        assert!(state.database().is_poisoned());

        let version =
            crate::commands::with_db(&state, |database| database.schema_version()).unwrap();
        assert_eq!(version, 20);
        assert_eq!(state.redactor_read().redact_text("safe"), "safe");
        drop(state.lock_write_operations());
        let redacted = crate::commands::with_db_and_redactor(&state, |database, redactor| {
            redactor.register_secret("poison-secret");
            Ok((
                database.schema_version()?,
                redactor.redact_text("poison-secret"),
            ))
        })
        .unwrap();
        assert_eq!(redacted.0, 20);
        assert_ne!(redacted.1, "poison-secret");
    }

    #[test]
    fn startup_runs_exactly_one_full_tree_audit() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("audit-budget-data")).unwrap();
        crate::security::AUDIT_TREE_CALLS.with(|count| count.set(0));
        let _state = AppState::initialize(paths).unwrap();
        // `AppPaths::initialize` 一次；`Database::open` 与其它初始化步骤不再重复全树审计。
        assert_eq!(
            crate::security::AUDIT_TREE_CALLS.with(std::cell::Cell::get),
            1
        );
    }

    #[test]
    fn app_state_initializes_only_inside_an_isolated_private_root() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("app-state-data")).unwrap();
        let state = AppState::initialize(paths.clone()).unwrap();

        assert_eq!(state.paths(), &paths);
        assert_eq!(
            state.database().lock().unwrap().schema_version().unwrap(),
            20
        );
        assert_eq!(state.redactor().read().unwrap().redact_text("safe"), "safe");
    }
}
