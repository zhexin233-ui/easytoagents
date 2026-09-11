#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        delete_prompt_profile, delete_provider_profile, insert_prompt_profile,
        insert_provider_profile, list_prompt_profiles, list_provider_profiles,
        set_active_provider_profile, set_global_prompt_assignment, update_prompt_profile,
        update_provider_profile, NewPromptProfileRecord, NewProviderProfileRecord,
    };
    use crate::{app::AppPaths, db::Database, domain::Tool};

    fn database() -> (tempfile::TempDir, Database) {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path())
            .unwrap()
            .join("private/data/root");
        let paths = AppPaths::from_data_root(root).unwrap();
        (temporary, Database::open(&paths).unwrap())
    }

    #[test]
    fn repositories_enforce_tool_scoped_names_and_single_active_profile() {
        let (_temporary, mut database) = database();
        let first = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Claude,
                name: "主渠道".to_owned(),
                api_base_url: Some("https://one.example.com".to_owned()),
                api_key: Some("fixture-provider-key-one".to_owned()),
                default_model: Some("claude-one".to_owned()),
                config_json: "{}".to_owned(),
                is_active: true,
            },
        )
        .unwrap();
        let second = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Claude,
                name: "Fallback".to_owned(),
                api_base_url: Some("https://two.example.com".to_owned()),
                api_key: Some("fixture-provider-key-two".to_owned()),
                default_model: Some("claude-two".to_owned()),
                config_json: "{}".to_owned(),
                is_active: false,
            },
        )
        .unwrap();
        set_active_provider_profile(&mut database, Tool::Claude, &second.id, second.row_version)
            .unwrap();
        let providers = list_provider_profiles(&database, Tool::Claude).unwrap();
        assert_eq!(
            providers
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Fallback", "主渠道"]
        );
        assert!(providers
            .iter()
            .any(|item| item.id == second.id && item.is_active));
        assert!(providers
            .iter()
            .any(|item| item.id == first.id && !item.is_active));

        let duplicate = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Claude,
                name: "fallback".to_owned(),
                api_base_url: None,
                api_key: None,
                default_model: None,
                config_json: "{}".to_owned(),
                is_active: false,
            },
        )
        .unwrap_err();
        assert_eq!(duplicate.code(), crate::error::ErrorCode::Conflict);

        let prompt_one = insert_prompt_profile(
            &mut database,
            &NewPromptProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                name: "默认提示词".to_owned(),
                body: "第一份".to_owned(),
                is_active_claude: false,
                is_active_codex: false,
                is_active_zcode: false,
                is_active_cursor: false,
                is_active_opencode: false,
                imported_from_path: None,
            },
        )
        .unwrap();
        let prompt_two = insert_prompt_profile(
            &mut database,
            &NewPromptProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                name: "审查提示词".to_owned(),
                body: "第二份".to_owned(),
                is_active_claude: false,
                is_active_codex: false,
                is_active_zcode: false,
                is_active_cursor: false,
                is_active_opencode: false,
                imported_from_path: None,
            },
        )
        .unwrap();
        set_global_prompt_assignment(
            &mut database,
            Tool::Codex,
            &prompt_two.id,
            true,
            prompt_two.row_version,
        )
        .unwrap();
        let prompts = list_prompt_profiles(&database).unwrap();
        assert!(prompts
            .iter()
            .any(|item| item.id == prompt_two.id && item.is_active_codex));
        assert!(prompts
            .iter()
            .any(|item| item.id == prompt_one.id && !item.is_active_codex));

        // 停用后该工具回到无生效状态；再次启用走替换语义。
        let disabled = set_global_prompt_assignment(
            &mut database,
            Tool::Codex,
            &prompt_two.id,
            false,
            prompt_two.row_version + 1,
        )
        .unwrap();
        assert!(!disabled.is_active_codex);
        let re_enabled = set_global_prompt_assignment(
            &mut database,
            Tool::Codex,
            &prompt_two.id,
            true,
            disabled.row_version,
        )
        .unwrap();
        assert!(re_enabled.is_active_codex);
    }

    #[test]
    fn activate_and_delete_reject_stale_row_versions() {
        let (_temporary, mut database) = database();
        let provider = insert_provider_profile(
            &mut database,
            &NewProviderProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                tool: Tool::Codex,
                name: "待更新渠道".to_owned(),
                api_base_url: Some("https://provider.example.com".to_owned()),
                api_key: Some("fixture-cas-provider-secret".to_owned()),
                default_model: Some("fixture-model".to_owned()),
                config_json: "{}".to_owned(),
                is_active: false,
            },
        )
        .unwrap();
        let updated = update_provider_profile(
            &mut database,
            &provider.id,
            "已更新渠道",
            provider.api_base_url.as_deref(),
            provider.api_key.as_deref(),
            provider.default_model.as_deref(),
            &provider.config_json,
            provider.row_version,
        )
        .unwrap();
        assert_eq!(
            set_active_provider_profile(
                &mut database,
                Tool::Codex,
                &provider.id,
                provider.row_version,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::Conflict
        );
        assert_eq!(
            delete_provider_profile(&mut database, &provider.id, provider.row_version)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::Conflict
        );
        delete_provider_profile(&mut database, &provider.id, updated.row_version).unwrap();

        let prompt = insert_prompt_profile(
            &mut database,
            &NewPromptProfileRecord {
                id: uuid::Uuid::new_v4().to_string(),
                name: "待更新提示词".to_owned(),
                body: "原正文".to_owned(),
                is_active_claude: false,
                is_active_codex: false,
                is_active_zcode: false,
                is_active_cursor: false,
                is_active_opencode: false,
                imported_from_path: None,
            },
        )
        .unwrap();
        let updated_prompt = update_prompt_profile(
            &mut database,
            &prompt.id,
            "已更新提示词",
            "新正文",
            prompt.row_version,
        )
        .unwrap();
        assert_eq!(
            set_global_prompt_assignment(
                &mut database,
                Tool::Claude,
                &prompt.id,
                true,
                prompt.row_version,
            )
            .unwrap_err()
            .code(),
            crate::error::ErrorCode::Conflict
        );
        assert_eq!(
            delete_prompt_profile(&mut database, &prompt.id, prompt.row_version)
                .unwrap_err()
                .code(),
            crate::error::ErrorCode::Conflict
        );
        delete_prompt_profile(&mut database, &prompt.id, updated_prompt.row_version).unwrap();
    }
}
