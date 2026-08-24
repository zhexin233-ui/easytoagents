use serde::{Deserialize, Serialize};
use specta::Type;

pub mod mcp;
pub mod profiles;
pub mod skills;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoDto {
    pub name: String,
    pub version: String,
}

#[tauri::command]
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
