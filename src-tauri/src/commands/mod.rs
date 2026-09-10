use serde::{Deserialize, Serialize};
use specta::Type;

pub mod environment;
pub mod hooks;
pub mod mcp;
pub mod overview;
pub mod profiles;
pub mod projects;
pub mod settings;
pub mod skills;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoDto {
    pub name: String,
    pub version: String,
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_app_info() -> AppInfoDto {
    AppInfoDto {
        name: "EasyToAgents".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{get_app_info, AppInfoDto};

    /// 每个命令源文件；新增命令文件时必须加进来，否则线程模型检查会漏掉它。
    const COMMAND_SOURCES: &[(&str, &str)] = &[
        ("commands/mod.rs", include_str!("mod.rs")),
        ("commands/environment.rs", include_str!("environment.rs")),
        ("commands/hooks.rs", include_str!("hooks.rs")),
        ("commands/mcp.rs", include_str!("mcp.rs")),
        ("commands/overview.rs", include_str!("overview.rs")),
        ("commands/profiles.rs", include_str!("profiles.rs")),
        ("commands/projects.rs", include_str!("projects.rs")),
        ("commands/settings.rs", include_str!("settings.rs")),
        ("commands/skills.rs", include_str!("skills.rs")),
    ];

    /// Tauri 2 的同步命令在主线程执行；任何触碰数据库锁、文件系统、子进程或
    /// 网络的命令都会卡住窗口。约定：所有命令要么 `#[tauri::command(async)]`
    /// （同步签名，线程池执行），要么本身是 `async fn`。
    #[test]
    fn every_command_runs_off_the_main_thread() {
        let mut violations = Vec::new();
        let mut command_count = 0;
        for (file, source) in COMMAND_SOURCES {
            let lines: Vec<&str> = source.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                let attribute = line.trim();
                if !attribute.starts_with("#[tauri::command") {
                    continue;
                }
                command_count += 1;
                let signature = lines[index + 1..]
                    .iter()
                    .map(|line| line.trim())
                    .find(|line| line.starts_with("pub "))
                    .unwrap_or_default();
                let off_main_thread =
                    attribute.contains("(async)") || signature.starts_with("pub async fn");
                if !off_main_thread {
                    violations.push(format!("{file}:{} {signature}", index + 1));
                }
            }
        }
        assert!(command_count >= 80, "命令扫描数量异常：{command_count}");
        assert!(violations.is_empty(), "同步主线程命令：{violations:#?}");
    }

    #[test]
    fn command_returns_application_metadata() {
        assert_eq!(
            get_app_info(),
            AppInfoDto {
                name: "EasyToAgents".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            }
        );
    }
}
