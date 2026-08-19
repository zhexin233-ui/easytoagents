//! 项目 Git 状态与本机排除规则的只读检测。

use std::{
    io::Read,
    path::{Component, Path},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{domain::ProjectRoot, error::AppError};

const GIT_READ_TIMEOUT: Duration = Duration::from_secs(5);
const GIT_REDIRECT_ENVIRONMENT: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    "GIT_PREFIX",
];

struct GitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GitPathStatus {
    pub is_repository: bool,
    pub tracked: bool,
    pub ignored: bool,
    pub ignored_by_local_exclude: bool,
}

impl GitPathStatus {
    fn not_repository() -> Self {
        Self {
            is_repository: false,
            tracked: false,
            ignored: false,
            ignored_by_local_exclude: false,
        }
    }
}

/// 只执行 `rev-parse`、`ls-files` 与 `check-ignore`；不会修改 index 或排除文件。
pub fn inspect_path(
    project_root: &ProjectRoot,
    target_path: &Path,
) -> Result<GitPathStatus, AppError> {
    if !target_path.is_absolute()
        || target_path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::invalid_input(
            "targetPath",
            "Git 目标必须是规范绝对路径",
        ));
    }
    let project_path = Path::new(project_root.as_str());
    target_path
        .strip_prefix(project_path)
        .map_err(|_| AppError::invalid_input("targetPath", "Git 目标不在登记项目内"))?;
    let repository_output = run_git(project_path, &["rev-parse", "--show-toplevel"])?;
    if !repository_output.status.success() {
        return Ok(GitPathStatus::not_repository());
    }
    let repository_text = command_path_text(&repository_output.stdout)?;
    let repository_root = std::fs::canonicalize(repository_text)
        .map_err(|_| AppError::not_found("gitRepository", repository_text))?;
    let relative = target_path
        .strip_prefix(&repository_root)
        .map_err(|_| AppError::invalid_input("targetPath", "Git 目标不在登记项目仓库内"))?;
    let relative_text = relative
        .to_str()
        .ok_or_else(|| AppError::invalid_input("targetPath", "Git 目标路径不是 UTF-8"))?;

    let tracked_output = run_git(
        &repository_root,
        &["ls-files", "--error-unmatch", "--", relative_text],
    )?;
    let tracked = match tracked_output.status.code() {
        Some(0) => true,
        Some(1) => false,
        _ => {
            return Err(AppError::invalid_input(
                "gitStatus",
                "Git ls-files 读取失败",
            ));
        }
    };

    let ignored_output = run_git(
        &repository_root,
        &["check-ignore", "--no-index", "-v", "--", relative_text],
    )?;
    let ignored = match ignored_output.status.code() {
        Some(0) => true,
        Some(1) => false,
        _ => {
            return Err(AppError::invalid_input(
                "gitStatus",
                "Git check-ignore 读取失败",
            ));
        }
    };
    let ignored_by_local_exclude = ignored
        && std::str::from_utf8(&ignored_output.stdout)
            .ok()
            .and_then(|output| output.split_once('\t').map(|(source, _)| source))
            .and_then(|source| source.rsplitn(3, ':').nth(2))
            .is_some_and(|source| source.ends_with(".git/info/exclude"));

    Ok(GitPathStatus {
        is_repository: true,
        tracked,
        ignored,
        ignored_by_local_exclude,
    })
}

fn run_git(current_dir: &Path, arguments: &[&str]) -> Result<GitOutput, AppError> {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ])
        .args(arguments)
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    Command::env(&mut command, "GIT_OPTIONAL_LOCKS", "0");
    Command::env(&mut command, "GIT_TERMINAL_PROMPT", "0");
    for variable in GIT_REDIRECT_ENVIRONMENT {
        Command::env_remove(&mut command, variable);
    }
    let mut child = command
        .spawn()
        .map_err(|_| AppError::not_found("git", &current_dir.to_string_lossy()))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < GIT_READ_TIMEOUT => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::invalid_input("gitStatus", "Git 只读检测超时"));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::invalid_input("gitStatus", "Git 只读检测失败"));
            }
        }
    };
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .ok_or_else(|| AppError::invalid_input("gitStatus", "Git 输出管道不可用"))?
        .read_to_end(&mut stdout)
        .map_err(|_| AppError::invalid_input("gitStatus", "Git 输出读取失败"))?;
    Ok(GitOutput { status, stdout })
}

fn command_path_text(output: &[u8]) -> Result<&str, AppError> {
    let output = output.strip_suffix(b"\n").unwrap_or(output);
    let output = output.strip_suffix(b"\r").unwrap_or(output);
    let text = std::str::from_utf8(output)
        .map_err(|_| AppError::invalid_input("gitRoot", "Git 根路径不是 UTF-8"))?;
    if text.is_empty() {
        return Err(AppError::invalid_input("gitRoot", "Git 根路径为空"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    use tempfile::tempdir;

    use super::inspect_path;
    use crate::adapters::canonicalize_project_root;

    #[test]
    fn reads_tracked_and_local_exclude_status_without_modifying_git_files() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let repository = root.join("repository");
        fs::create_dir(&repository).unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());

        let tracked_path = repository.join(".mcp.json");
        fs::write(&tracked_path, "{}\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "--", ".mcp.json"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());
        let ignored_path = repository.join(".codex/config.toml");
        fs::create_dir(repository.join(".codex")).unwrap();
        fs::write(&ignored_path, "model = \"fixture\"\n").unwrap();
        let exclude_path = repository.join(".git/info/exclude");
        let original_exclude = fs::read_to_string(&exclude_path).unwrap();
        fs::write(
            &exclude_path,
            format!("{original_exclude}\n.codex/config.toml\n"),
        )
        .unwrap();
        let expected_exclude = fs::read_to_string(&exclude_path).unwrap();

        let project = canonicalize_project_root(&repository).unwrap();
        let tracked = inspect_path(&project, &tracked_path).unwrap();
        assert!(tracked.is_repository);
        assert!(tracked.tracked);

        let ignored = inspect_path(&project, &ignored_path).unwrap();
        assert!(ignored.ignored);
        assert!(ignored.ignored_by_local_exclude);
        assert_eq!(fs::read_to_string(&exclude_path).unwrap(), expected_exclude);
        assert!(!repository.join(".gitignore").exists());
    }

    #[test]
    fn repository_fsmonitor_hook_is_not_executed_by_read_only_inspection() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let repository = root.join("repository");
        fs::create_dir(&repository).unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());
        let target = repository.join(".mcp.json");
        fs::write(&target, "{}\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "--", ".mcp.json"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());

        let marker = root.join("fsmonitor-was-executed");
        let hook = root.join("fixture-fsmonitor.sh");
        fs::write(
            &hook,
            format!("#!/bin/sh\n/usr/bin/touch '{}'\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(Command::new("git")
            .args(["config", "core.fsmonitor", hook.to_str().unwrap()])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());

        let project = canonicalize_project_root(&repository).unwrap();
        assert!(inspect_path(&project, &target).unwrap().tracked);
        assert!(!marker.exists(), "只读检查不得执行仓库 fsmonitor hook");
    }

    #[test]
    fn reports_non_repository_without_creating_git_metadata() {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let project_path = root.join("plain-project");
        fs::create_dir(&project_path).unwrap();
        let project = canonicalize_project_root(&project_path).unwrap();
        let status = inspect_path(&project, &project_path.join(".mcp.json")).unwrap();
        assert!(!status.is_repository);
        assert!(!project_path.join(".git").exists());
    }
}
