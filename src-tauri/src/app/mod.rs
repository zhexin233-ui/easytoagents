//! 应用初始化、私有路径与共享状态容器。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, RwLock},
};

use crate::{
    db::Database,
    error::AppError,
    security::{
        audit_private_tree, ensure_private_directory, reject_symlink_components, SecretRedactor,
    },
};

const APPLICATION_SUPPORT_DIRECTORY: &str = "EasyToAgents";

/// 应用拥有的路径集合。构造过程不读取 HOME 或任何工具环境变量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    data_root: PathBuf,
    database: PathBuf,
    central_skills: PathBuf,
    snapshots: PathBuf,
    staging: PathBuf,
    journals: PathBuf,
    database_backups: PathBuf,
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
            snapshots: data_root.join("snapshots"),
            staging: data_root.join("staging"),
            journals: data_root.join("journals"),
            database_backups: data_root.join("database-backups"),
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

    pub fn initialize(&self) -> Result<(), AppError> {
        for directory in self.private_directories() {
            ensure_private_directory(directory)?;
        }
        audit_private_tree(&self.data_root)?;
        Ok(())
    }

    pub fn audit_permissions(&self) -> Result<(), AppError> {
        audit_private_tree(&self.data_root).map(|_| ())
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

    fn private_directories(&self) -> [&Path; 6] {
        [
            &self.data_root,
            &self.central_skills,
            &self.snapshots,
            &self.staging,
            &self.journals,
            &self.database_backups,
        ]
    }
}

pub struct AppState {
    database: Mutex<Database>,
    paths: AppPaths,
    redactor: RwLock<SecretRedactor>,
}

impl AppState {
    pub fn initialize(paths: AppPaths) -> Result<Self, AppError> {
        paths.initialize()?;
        let database = Database::open(&paths)?;
        paths.audit_permissions()?;
        Ok(Self {
            database: Mutex::new(database),
            paths,
            redactor: RwLock::new(SecretRedactor::default()),
        })
    }

    pub fn database(&self) -> &Mutex<Database> {
        &self.database
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn redactor(&self) -> &RwLock<SecretRedactor> {
        &self.redactor
    }
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
    fn app_state_initializes_only_inside_an_isolated_private_root() {
        let temporary = tempdir().unwrap();
        let isolated_root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(isolated_root.join("app-state-data")).unwrap();
        let state = AppState::initialize(paths.clone()).unwrap();

        assert_eq!(state.paths(), &paths);
        assert_eq!(
            state.database().lock().unwrap().schema_version().unwrap(),
            1
        );
        assert_eq!(state.redactor().read().unwrap().redact_text("safe"), "safe");
    }
}
