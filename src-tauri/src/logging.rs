//! 结构化日志：写入应用私有目录的按日滚动文件（目录 0700、文件 0600）。
//!
//! 只记录稳定错误码、operation、路径与脱敏后的底层原因（`AppError::with_source`），
//! 以及 apply/restore 的阶段转换；任何进入日志的文本都先经过 `SecretRedactor`。
//! 级别默认 `info`，可用环境变量 `EASYTOAGENTS_LOG`（`tracing_subscriber::EnvFilter`
//! 语法）调整。日志本身失败绝不影响业务命令。

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tracing_subscriber::{fmt::MakeWriter, EnvFilter};

use crate::{app::AppPaths, security::PRIVATE_FILE_MODE};

pub const LOG_LEVEL_ENVIRONMENT_VARIABLE: &str = "EASYTOAGENTS_LOG";
const LOG_FILE_PREFIX: &str = "app-";
const LOG_FILE_SUFFIX: &str = ".log";
/// 保留最近 7 天的日志文件；更早的在启动时删除。
const LOG_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// 每次写入都以 append + 0600 打开当天的文件；日志量很低，不值得为此常驻句柄。
#[derive(Clone)]
pub struct PrivateLogWriter {
    directory: PathBuf,
}

impl PrivateLogWriter {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    fn open_today(&self) -> io::Result<File> {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(PRIVATE_FILE_MODE)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.directory.join(log_file_name(SystemTime::now())))?;
        // `mode` 只约束新建文件；已有日志可能是旧版本创建的，启动后也要
        // 收紧权限，避免把历史日志暴露给同机其他用户。
        file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
        Ok(file)
    }
}

/// 打不开日志文件时退化为丢弃输出：日志永远不能让命令失败。
pub enum LogSink {
    File(File),
    Discard,
}

impl io::Write for LogSink {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.write(buffer),
            Self::Discard => Ok(buffer.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::File(file) => file.flush(),
            Self::Discard => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for PrivateLogWriter {
    type Writer = LogSink;

    fn make_writer(&'a self) -> Self::Writer {
        self.open_today().map_or(LogSink::Discard, LogSink::File)
    }
}

pub fn log_file_name(now: SystemTime) -> String {
    let days = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{LOG_FILE_PREFIX}{year:04}-{month:02}-{day:02}{LOG_FILE_SUFFIX}")
}

/// Howard Hinnant 的 days → (y, m, d) 公历算法，避免为此引入 chrono。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn environment_filter() -> EnvFilter {
    EnvFilter::try_from_env(LOG_LEVEL_ENVIRONMENT_VARIABLE)
        .unwrap_or_else(|_| EnvFilter::new("info"))
}

/// 构造订阅者：测试用 `tracing::subscriber::with_default` 局部安装。
pub fn build_subscriber(directory: &Path) -> impl tracing::Subscriber + Send + Sync + 'static {
    tracing_subscriber::fmt()
        .with_env_filter(environment_filter())
        .with_writer(PrivateLogWriter::new(directory))
        .with_ansi(false)
        .with_target(false)
        .finish()
}

/// 发布进程在 setup 里调用一次；重复安装（如同进程多次 setup）静默忽略。
pub fn init(paths: &AppPaths) {
    prune_old_logs(paths.logs(), SystemTime::now());
    let _ = tracing::subscriber::set_global_default(build_subscriber(paths.logs()));
}

/// 删除超过保留期的 `app-YYYY-MM-DD.log`；只按文件名判断，不读内容。
pub fn prune_old_logs(directory: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let cutoff = now.checked_sub(LOG_RETENTION).unwrap_or(UNIX_EPOCH);
    let cutoff_name = log_file_name(cutoff);
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_log = name.starts_with(LOG_FILE_PREFIX)
            && name.ends_with(LOG_FILE_SUFFIX)
            && entry.file_type().is_ok_and(|kind| kind.is_file());
        // 文件名按日期排序即时间序；早于截止日期的删除。
        if is_log && name < cutoff_name {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    use tempfile::tempdir;

    use super::{build_subscriber, civil_from_days, log_file_name, prune_old_logs};
    use crate::{
        error::AppError,
        security::{SecretRedactor, PRIVATE_FILE_MODE},
    };

    #[test]
    fn log_file_names_follow_the_civil_calendar() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_707), (2026, 9, 11));
        assert_eq!(
            log_file_name(std::time::UNIX_EPOCH + Duration::from_secs(20_707 * 86_400 + 3_600)),
            "app-2026-09-11.log"
        );
    }

    #[test]
    fn log_file_is_private_and_sources_are_redacted() {
        let temporary = tempdir().unwrap();
        let directory = fs::canonicalize(temporary.path()).unwrap().join("logs");
        fs::create_dir(&directory).unwrap();
        let existing_log = directory.join(log_file_name(std::time::SystemTime::now()));
        fs::write(&existing_log, "previous log\n").unwrap();
        fs::set_permissions(&existing_log, fs::Permissions::from_mode(0o644)).unwrap();
        tracing::subscriber::with_default(build_subscriber(&directory), || {
            let mut redactor = SecretRedactor::default();
            redactor.register_secret("fixture-log-secret");
            let error = AppError::database(
                "/isolated/fixture-log-secret.sqlite3",
                "open_fixture-log-secret",
            )
            .with_source_redacted(
                "disk I/O error; Authorization: Bearer fixture-log-secret",
                &redactor,
            );
            assert!(error
                .source()
                .is_some_and(|source| !source.contains("fixture-log-secret")));
            tracing::info!(run_id = "run-1", phase = "applying", "journal phase");
        });
        let mut files = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1, "应只产生当天一个日志文件");
        let log = files.pop().unwrap();
        assert_eq!(
            fs::metadata(&log).unwrap().permissions().mode() & 0o777,
            PRIVATE_FILE_MODE
        );
        let text = fs::read_to_string(&log).unwrap();
        assert!(text.contains("open_[REDACTED]"));
        assert!(text.contains("DATABASE_ERROR"));
        assert!(text.contains("journal phase"));
        assert!(
            !text.contains("fixture-log-secret"),
            "日志不得包含注入的凭据：{text}"
        );
    }

    #[test]
    fn stale_log_files_are_pruned_by_name() {
        let temporary = tempdir().unwrap();
        let directory = temporary.path().to_path_buf();
        for name in ["app-2026-08-01.log", "app-2026-09-10.log", "notes.txt"] {
            fs::write(directory.join(name), "x").unwrap();
        }
        prune_old_logs(
            &directory,
            std::time::UNIX_EPOCH + Duration::from_secs(20_708 * 86_400),
        );
        assert!(!directory.join("app-2026-08-01.log").exists());
        assert!(directory.join("app-2026-09-10.log").exists());
        assert!(directory.join("notes.txt").exists());
    }
}
