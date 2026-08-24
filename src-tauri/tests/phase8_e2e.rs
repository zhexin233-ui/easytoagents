use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Mutex,
};

use easytoagents_lib::{
    adapters::{
        ExplicitEnvironment, ToolAvailability, VerifiedClaudeCustomizationPolicyEvidence,
        VerifiedClaudeUserMcpEvidence,
    },
    app::AppPaths,
    db::Database,
    domain::{ArtifactKind, ChangeKind, McpTransport, SyncStatus, Tool},
    error::ErrorCode,
    mcp::{
        apply_mcp_preview_with_probes, create_mcp_server, preview_mcp_sync_with_probes,
        set_global_mcp_assignment, set_project_mcp_assignment, ApplyMcpPreviewInput,
        McpServerInput, PreviewMcpSyncInput, SetGlobalMcpAssignmentInput,
        SetProjectMcpAssignmentInput,
    },
    overview::{dashboard_summary, snapshot_restore_context},
    profiles::{
        apply_profile_preview, confirm_prompt_import, create_prompt_profile,
        discover_prompt_import, preview_prompt_sync, set_active_prompt_profile, ConfirmImportInput,
        PromptProfileInput, VersionedProfileInput,
    },
    projects::{register_project, RegisterProjectInput},
    security::SecretRedactor,
    skills::{
        apply_skill_preview_with_policy_probe, import_skill, preview_skill_sync_with_policy_probe,
        set_global_skill_assignment, set_project_skill_assignment, ApplySkillPreviewInput,
        ImportSkillInput, PreviewSkillSyncInput, SetGlobalSkillAssignmentInput,
        SetProjectSkillAssignmentInput,
    },
    sync::{list_snapshots, preview_restore, restore_snapshot, WARNING_EXTERNAL_NON_OWNED_CHANGE},
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const HEADER_SECRET: &str = "phase8-header-secret";
const ENV_SECRET: &str = "phase8-env-secret";
const EXTRA_SECRET: &str = "phase8-extra-secret";
const SKILL_PRIVATE_MARKER: &str = "phase8-skill-private-secret";
const CLAUDE_VERSION: &str = "phase8-fixture-1.0.0";
const REPLACEMENT_PROMPT: &str = "# 新提示词\n\n精确保留末尾空格  \n最后一行无换行";

struct Fixture {
    _temporary: TempDir,
    home: PathBuf,
    claude_config: PathBuf,
    codex_home: PathBuf,
    project: PathBuf,
    sources: PathBuf,
    external_skill: PathBuf,
    paths: AppPaths,
    database: Database,
    environment: ExplicitEnvironment,
    project_id: String,
    write_operations: Mutex<()>,
    redactor: SecretRedactor,
    user_mcp_evidence: VerifiedClaudeUserMcpEvidence,
    policy_evidence: VerifiedClaudeCustomizationPolicyEvidence,
}

#[derive(Clone)]
struct RestoreCase {
    snapshot_id: String,
    expected_allowed_root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("创建 Phase 8 隔离根失败");
        let root = fs::canonicalize(temporary.path()).expect("规范化 Phase 8 隔离根失败");
        let home = root.join("home");
        let claude_config = root.join("claude-config");
        let codex_home = root.join("codex-home");
        let project = root.join("project");
        let sources = root.join("sources");
        let external_skill = root.join("external-skill");
        for directory in [
            &home,
            &claude_config,
            &codex_home,
            &project,
            &sources,
            &external_skill,
        ] {
            fs::create_dir(directory).expect("创建 Phase 8 fixture 目录失败");
        }
        for directory in [
            claude_config.join("skills"),
            home.join(".agents/skills"),
            project.join(".claude/skills"),
            project.join(".agents/skills"),
            project.join(".codex"),
        ] {
            fs::create_dir_all(directory).expect("创建隔离原生目标目录失败");
        }

        fs::write(external_skill.join("KEEP.md"), "外部 Skill 不得被修改\n")
            .expect("写入外部 Skill fixture 失败");
        fs::create_dir(claude_config.join("skills/external-directory"))
            .expect("创建未知普通目录失败");
        fs::write(
            claude_config.join("skills/external-directory/KEEP.md"),
            "保留未知普通目录\n",
        )
        .expect("写入未知普通目录失败");
        symlink(
            &external_skill,
            home.join(".agents/skills/external-symlink"),
        )
        .expect("创建未知外部链接失败");
        symlink(
            root.join("missing-external-skill"),
            project.join(".agents/skills/broken-external-symlink"),
        )
        .expect("创建未知断链失败");

        let claude_user_mcp = home.join(".claude.json");
        fs::write(
            &claude_user_mcp,
            br#"{
  "theme": "dark",
  "unknownTop": {"preserve": true},
  "mcpServers": {
    "external-global": {"command": "keep", "unknown": {"value": 1}}
  }
}
"#,
        )
        .expect("写入 Claude 用户 MCP fixture 失败");
        fs::write(
            project.join(".mcp.json"),
            br#"{
  "unknownProject": {"preserve": true},
  "mcpServers": {"external-project": {"command": "keep"}}
}
"#,
        )
        .expect("写入 Claude 项目 MCP fixture 失败");
        fs::write(
            codex_home.join("config.toml"),
            format!(
                r#"# Phase 8 Codex 顶层注释必须保留
model = "external-model"

[projects."{}"]
trust_level = "trusted"

[mcp_servers.external_global]
command = "keep"
unknown = "preserve"

[plugins.phase8]
enabled = true
"#,
                project.to_string_lossy()
            ),
        )
        .expect("写入 Codex 用户配置 fixture 失败");
        fs::write(
            project.join(".codex/config.toml"),
            r#"# Phase 8 Codex 项目注释必须保留
[features]
phase8 = true

[mcp_servers.external_project]
command = "keep"
unknown = "preserve"
"#,
        )
        .expect("写入 Codex 项目配置 fixture 失败");
        fs::write(
            claude_config.join("CLAUDE.md"),
            "# 原始提示词\n\n保留末尾空格  \n最后一行无换行",
        )
        .expect("写入 Markdown fixture 失败");

        let environment = ExplicitEnvironment::new(
            &home,
            Some(claude_config.clone()),
            Some(codex_home.clone()),
            ToolAvailability::all_installed(),
        )
        .expect("创建显式隔离环境失败")
        .with_claude_installation_version(CLAUDE_VERSION)
        .expect("绑定 Claude fixture 版本失败");
        let user_mcp_evidence =
            VerifiedClaudeUserMcpEvidence::new(CLAUDE_VERSION, &claude_config, &claude_user_mcp)
                .expect("创建 Claude MCP capability fixture 失败");
        let policy_evidence =
            VerifiedClaudeCustomizationPolicyEvidence::from_effective_setting(CLAUDE_VERSION, None)
                .expect("创建 Claude policy fixture 失败");
        let paths =
            AppPaths::from_data_root(root.join("app-data")).expect("创建显式隔离应用数据根失败");
        let mut database = Database::open(&paths).expect("打开 Phase 8 隔离数据库失败");
        let project_dto = register_project(
            &mut database,
            &environment,
            &RegisterProjectInput {
                display_name: "Phase 8 隔离项目".to_owned(),
                root_path: project.to_string_lossy().into_owned(),
            },
        )
        .expect("登记 Phase 8 隔离项目失败");

        Self {
            _temporary: temporary,
            home,
            claude_config,
            codex_home,
            project,
            sources,
            external_skill,
            paths,
            database,
            environment,
            project_id: project_dto.id,
            write_operations: Mutex::new(()),
            redactor: SecretRedactor::default(),
            user_mcp_evidence,
            policy_evidence,
        }
    }

    fn project_row_version(&self) -> u32 {
        self.database
            .connection()
            .query_row(
                "SELECT row_version FROM projects WHERE id = ?1",
                [&self.project_id],
                |row| row.get(0),
            )
            .expect("读取项目 row_version 失败")
    }

    fn create_skill_source(&self, name: &str) -> (PathBuf, Vec<u8>) {
        let source = self.sources.join(name);
        fs::create_dir(&source).expect("创建 Skill 来源目录失败");
        let body = format!(
            "---\nname: {name}\ndescription: Phase 8 隔离 Skill\nmetadata:\n  private: {SKILL_PRIVATE_MARKER}\n---\n\n# Phase 8 Skill\n\n{SKILL_PRIVATE_MARKER}\n"
        )
        .into_bytes();
        fs::write(source.join("SKILL.md"), &body).expect("写入 Skill fixture 失败");
        fs::write(source.join("asset.txt"), "原始来源保持不变\n")
            .expect("写入 Skill 资源 fixture 失败");
        (source, body)
    }

    fn apply_mcp(&mut self, tool: Tool, project_id: Option<String>) -> RestoreCase {
        let preview = preview_mcp_sync_with_probes(
            &mut self.database,
            &self.environment,
            &mut self.redactor,
            &PreviewMcpSyncInput {
                tool,
                project_id: project_id.clone(),
                exclude_from_git: false,
            },
            &self.user_mcp_evidence,
            &self.policy_evidence,
        )
        .expect("生成 MCP 持久化预览失败");
        assert_eq!(preview.targets.len(), 1);
        assert_ne!(preview.targets[0].change_kind, ChangeKind::Conflict);
        assert_secrets_absent("MCP 预览 DTO", &serde_json::to_string(&preview).unwrap());
        let target_path = PathBuf::from(
            preview.targets[0]
                .descriptor
                .path
                .as_deref()
                .expect("MCP 预览缺少目标路径"),
        );
        let result = apply_mcp_preview_with_probes(
            &self.write_operations,
            &mut self.database,
            &self.paths,
            &self.environment,
            &mut self.redactor,
            &ApplyMcpPreviewInput {
                preview_id: preview.preview_id,
                tool,
                project_id: project_id.clone(),
            },
            &self.user_mcp_evidence,
            &self.policy_evidence,
        )
        .expect("应用 MCP 持久化预览失败");
        assert_serialized_secrets_absent("MCP Apply RPC DTO", &result);
        assert_eq!(result.applied_targets, 1);
        let allowed_root = if project_id.is_some() {
            self.project.clone()
        } else if tool == Tool::Claude {
            self.home.clone()
        } else {
            self.codex_home.clone()
        };
        self.restore_case(&result.run_id, &target_path, allowed_root)
    }

    fn apply_skill(
        &mut self,
        tool: Tool,
        project_id: Option<String>,
        skill_name: &str,
    ) -> RestoreCase {
        let preview = preview_skill_sync_with_policy_probe(
            &mut self.database,
            &self.paths,
            &self.environment,
            &self.redactor,
            &PreviewSkillSyncInput {
                tool,
                project_id: project_id.clone(),
                exclude_from_git: false,
            },
            &self.policy_evidence,
        )
        .expect("生成 Skill 持久化预览失败");
        assert_eq!(preview.targets.len(), 1);
        assert_ne!(preview.targets[0].change_kind, ChangeKind::Conflict);
        assert_secrets_absent("Skill 预览 DTO", &serde_json::to_string(&preview).unwrap());
        let target_directory = PathBuf::from(
            preview.targets[0]
                .descriptor
                .path
                .as_deref()
                .expect("Skill 预览缺少目标路径"),
        );
        let result = apply_skill_preview_with_policy_probe(
            &self.write_operations,
            &mut self.database,
            &self.paths,
            &self.environment,
            &self.redactor,
            &ApplySkillPreviewInput {
                preview_id: preview.preview_id,
                tool,
                project_id: project_id.clone(),
            },
            &self.policy_evidence,
        )
        .expect("应用 Skill 持久化预览失败");
        assert_serialized_secrets_absent("Skill Apply RPC DTO", &result);
        assert_eq!(result.applied_targets, 1);
        let allowed_root = if project_id.is_some() {
            self.project.clone()
        } else if tool == Tool::Claude {
            self.claude_config.clone()
        } else {
            self.home.clone()
        };
        self.restore_case(
            &result.run_id,
            &target_directory.join(skill_name),
            allowed_root,
        )
    }

    fn restore_case(
        &self,
        run_id: &str,
        target_path: &Path,
        expected_allowed_root: PathBuf,
    ) -> RestoreCase {
        let snapshot = list_snapshots(&self.database)
            .expect("读取快照索引失败")
            .into_iter()
            .find(|snapshot| {
                snapshot.run_id == run_id && snapshot.target_path == target_path.to_string_lossy()
            })
            .expect("未找到写入前目标快照");
        RestoreCase {
            snapshot_id: snapshot.snapshot_id,
            expected_allowed_root,
        }
    }

    fn restore(&mut self, restore_case: &RestoreCase) {
        let context =
            snapshot_restore_context(&self.database, &self.environment, &restore_case.snapshot_id)
                .expect("按生产目标矩阵推导恢复根失败");
        assert_eq!(
            context.allowed_root, restore_case.expected_allowed_root,
            "生产恢复入口必须按工具、资源和作用域推导正确的隔离根"
        );
        let preview = preview_restore(
            &mut self.database,
            &self.paths,
            &restore_case.snapshot_id,
            &context.allowed_root,
        )
        .expect("生成快照恢复预览失败");
        assert_serialized_secrets_absent("恢复预览 RPC DTO", &preview);
        let result = restore_snapshot(
            &self.write_operations,
            &mut self.database,
            &self.paths,
            &preview.preview_id,
            &context.allowed_root,
            Some(self.paths.central_skills()),
        )
        .expect("恢复写入前快照失败");
        assert_serialized_secrets_absent("恢复 Apply RPC DTO", &result);
    }
}

#[test]
fn isolated_full_chain_restores_exact_fixture_and_leaks_no_secret() {
    let mut fixture = Fixture::new();
    let original_prompt =
        fs::read(fixture.claude_config.join("CLAUDE.md")).expect("读取原始 Markdown fixture 失败");

    let (global_skill_source, global_skill_body) =
        fixture.create_skill_source("phase8-global-skill");
    let (project_skill_source, project_skill_body) =
        fixture.create_skill_source("phase8-project-skill");
    let initial_hash = fixture_hash(&[
        ("home", &fixture.home),
        ("claude-config", &fixture.claude_config),
        ("codex-home", &fixture.codex_home),
        ("project", &fixture.project),
        ("sources", &fixture.sources),
        ("external-skill", &fixture.external_skill),
    ]);

    let global_mcp = create_mcp_server(
        &mut fixture.database,
        &mut fixture.redactor,
        &McpServerInput {
            name: "phase8-global-http".to_owned(),
            transport: McpTransport::StreamableHttp,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/rpc?tenant=phase8".to_owned()),
            headers: BTreeMap::from([(
                "Authorization".to_owned(),
                format!("Bearer {HEADER_SECRET}"),
            )]),
            env: BTreeMap::new(),
            extra: json!({"request_timeout_sec": 30, "auth_token": EXTRA_SECRET}),
            enabled: true,
        },
    )
    .expect("创建全局 MCP 中央意图失败");
    assert_serialized_secrets_absent("创建 MCP RPC DTO", &global_mcp);
    let global_mcp = set_global_mcp_assignment(
        &mut fixture.database,
        &fixture.redactor,
        &SetGlobalMcpAssignmentInput {
            tool: Tool::Claude,
            mcp_id: global_mcp.id,
            assigned: true,
            row_version: global_mcp.row_version,
        },
    )
    .expect("分配 Claude 全局 MCP 失败");
    assert_serialized_secrets_absent("分配 MCP RPC DTO", &global_mcp);
    set_global_mcp_assignment(
        &mut fixture.database,
        &fixture.redactor,
        &SetGlobalMcpAssignmentInput {
            tool: Tool::Codex,
            mcp_id: global_mcp.id,
            assigned: true,
            row_version: global_mcp.row_version,
        },
    )
    .expect("分配 Codex 全局 MCP 失败");

    let project_mcp = create_mcp_server(
        &mut fixture.database,
        &mut fixture.redactor,
        &McpServerInput {
            name: "phase8-project-stdio".to_owned(),
            transport: McpTransport::Stdio,
            command: Some("phase8-server".to_owned()),
            args: vec!["--fixture".to_owned()],
            url: None,
            headers: BTreeMap::new(),
            env: BTreeMap::from([("MCP_TOKEN".to_owned(), ENV_SECRET.to_owned())]),
            extra: json!({"startup_timeout_sec": 10}),
            enabled: true,
        },
    )
    .expect("创建项目 MCP 中央意图失败");
    assert_serialized_secrets_absent("创建项目 MCP RPC DTO", &project_mcp);
    let project_version = fixture.project_row_version();
    let project_mcp = set_project_mcp_assignment(
        &mut fixture.database,
        &fixture.redactor,
        &SetProjectMcpAssignmentInput {
            project_id: fixture.project_id.clone(),
            tool: Tool::Claude,
            mcp_id: project_mcp.id,
            assigned: true,
            mcp_row_version: project_mcp.row_version,
            project_row_version: project_version,
        },
    )
    .expect("分配 Claude 项目 MCP 失败");
    assert_serialized_secrets_absent("分配项目 MCP RPC DTO", &project_mcp);
    let project_version = fixture.project_row_version();
    set_project_mcp_assignment(
        &mut fixture.database,
        &fixture.redactor,
        &SetProjectMcpAssignmentInput {
            project_id: fixture.project_id.clone(),
            tool: Tool::Codex,
            mcp_id: project_mcp.id,
            assigned: true,
            mcp_row_version: project_mcp.row_version,
            project_row_version: project_version,
        },
    )
    .expect("分配 Codex 项目 MCP 失败");

    let global_skill = import_skill(
        &mut fixture.database,
        &fixture.paths,
        &ImportSkillInput {
            source_path: global_skill_source.to_string_lossy().into_owned(),
        },
    )
    .expect("导入全局 Skill 失败");
    assert_serialized_secrets_absent("导入 Skill RPC DTO", &global_skill);
    let global_skill = set_global_skill_assignment(
        &mut fixture.database,
        &fixture.paths,
        &SetGlobalSkillAssignmentInput {
            tool: Tool::Claude,
            skill_id: global_skill.id,
            assigned: true,
            row_version: global_skill.row_version,
        },
    )
    .expect("分配 Claude 全局 Skill 失败");
    assert_serialized_secrets_absent("分配 Skill RPC DTO", &global_skill);
    set_global_skill_assignment(
        &mut fixture.database,
        &fixture.paths,
        &SetGlobalSkillAssignmentInput {
            tool: Tool::Codex,
            skill_id: global_skill.id,
            assigned: true,
            row_version: global_skill.row_version,
        },
    )
    .expect("分配 Codex 全局 Skill 失败");

    let project_skill = import_skill(
        &mut fixture.database,
        &fixture.paths,
        &ImportSkillInput {
            source_path: project_skill_source.to_string_lossy().into_owned(),
        },
    )
    .expect("导入项目 Skill 失败");
    assert_serialized_secrets_absent("导入项目 Skill RPC DTO", &project_skill);
    let project_version = fixture.project_row_version();
    let project_skill = set_project_skill_assignment(
        &mut fixture.database,
        &fixture.paths,
        &SetProjectSkillAssignmentInput {
            project_id: fixture.project_id.clone(),
            tool: Tool::Claude,
            skill_id: project_skill.id,
            assigned: true,
            skill_row_version: project_skill.row_version,
            project_row_version: project_version,
        },
    )
    .expect("分配 Claude 项目 Skill 失败");
    assert_serialized_secrets_absent("分配项目 Skill RPC DTO", &project_skill);
    let project_version = fixture.project_row_version();
    set_project_skill_assignment(
        &mut fixture.database,
        &fixture.paths,
        &SetProjectSkillAssignmentInput {
            project_id: fixture.project_id.clone(),
            tool: Tool::Codex,
            skill_id: project_skill.id,
            assigned: true,
            skill_row_version: project_skill.row_version,
            project_row_version: project_version,
        },
    )
    .expect("分配 Codex 项目 Skill 失败");

    let imported_prompt =
        discover_prompt_import(&mut fixture.database, &fixture.environment, Tool::Claude)
            .expect("发现原始 Markdown 失败")
            .expect("原始 Markdown 未生成导入预览");
    assert_serialized_secrets_absent("提示词导入预览 RPC DTO", &imported_prompt);
    assert_eq!(imported_prompt.body.as_bytes(), original_prompt.as_slice());
    confirm_prompt_import(
        &mut fixture.database,
        &fixture.environment,
        ConfirmImportInput {
            preview_id: imported_prompt.preview_id,
            name: "原始提示词".to_owned(),
        },
    )
    .expect("确认无损导入 Markdown 失败");
    let replacement_prompt = create_prompt_profile(
        &mut fixture.database,
        PromptProfileInput {
            tool: Tool::Claude,
            name: "Phase 8 新提示词".to_owned(),
            body: REPLACEMENT_PROMPT.to_owned(),
            activate: false,
        },
    )
    .expect("创建替换提示词失败");
    set_active_prompt_profile(
        &mut fixture.database,
        Tool::Claude,
        &VersionedProfileInput {
            id: replacement_prompt.id,
            row_version: replacement_prompt.row_version,
        },
    )
    .expect("切换替换提示词失败");

    let mut restore_cases = Vec::new();
    let project_id = fixture.project_id.clone();
    restore_cases.push(fixture.apply_mcp(Tool::Claude, None));
    restore_cases.push(fixture.apply_mcp(Tool::Codex, None));
    restore_cases.push(fixture.apply_mcp(Tool::Claude, Some(project_id.clone())));
    restore_cases.push(fixture.apply_mcp(Tool::Codex, Some(project_id.clone())));
    restore_cases.push(fixture.apply_skill(Tool::Claude, None, "phase8-global-skill"));
    restore_cases.push(fixture.apply_skill(Tool::Codex, None, "phase8-global-skill"));
    restore_cases.push(fixture.apply_skill(
        Tool::Claude,
        Some(project_id.clone()),
        "phase8-project-skill",
    ));
    restore_cases.push(fixture.apply_skill(Tool::Codex, Some(project_id), "phase8-project-skill"));

    let prompt_preview = preview_prompt_sync(
        &mut fixture.database,
        &fixture.environment,
        &fixture.redactor,
        Tool::Claude,
    )
    .expect("生成 Markdown 持久化预览失败");
    assert_serialized_secrets_absent("Markdown 预览 RPC DTO", &prompt_preview);
    let prompt_target = PathBuf::from(
        prompt_preview.targets[0]
            .descriptor
            .path
            .as_deref()
            .expect("Markdown 预览缺少目标路径"),
    );
    let prompt_result = apply_profile_preview(
        &fixture.write_operations,
        &mut fixture.database,
        &fixture.paths,
        &fixture.environment,
        &mut fixture.redactor,
        &prompt_preview.preview_id,
        Tool::Claude,
        ArtifactKind::Prompt,
    )
    .expect("应用 Markdown 持久化预览失败");
    assert_serialized_secrets_absent("Markdown Apply RPC DTO", &prompt_result);
    restore_cases.push(fixture.restore_case(
        &prompt_result.run_id,
        &prompt_target,
        fixture.claude_config.clone(),
    ));
    assert_eq!(
        fs::read(&prompt_target).expect("读取应用后的 Markdown 失败"),
        REPLACEMENT_PROMPT.as_bytes()
    );

    assert_native_round_trip(&fixture);
    assert_eq!(
        fs::read(global_skill_source.join("SKILL.md")).unwrap(),
        global_skill_body
    );
    assert_eq!(
        fs::read(project_skill_source.join("SKILL.md")).unwrap(),
        project_skill_body
    );

    let claude_mcp_path = fixture.home.join(".claude.json");
    let mut unmanaged: Value =
        serde_json::from_slice(&fs::read(&claude_mcp_path).unwrap()).unwrap();
    unmanaged["postApplyUnmanaged"] = json!({"preserve": true});
    fs::write(
        &claude_mcp_path,
        serde_json::to_vec_pretty(&unmanaged).unwrap(),
    )
    .unwrap();
    let unmanaged_preview = preview_mcp_sync_with_probes(
        &mut fixture.database,
        &fixture.environment,
        &mut fixture.redactor,
        &PreviewMcpSyncInput {
            tool: Tool::Claude,
            project_id: None,
            exclude_from_git: false,
        },
        &fixture.user_mcp_evidence,
        &fixture.policy_evidence,
    )
    .expect("检测非受管漂移失败");
    assert_serialized_secrets_absent("非受管漂移预览 RPC DTO", &unmanaged_preview);
    assert_eq!(
        unmanaged_preview.targets[0].status,
        SyncStatus::ExternalNonOwnedChange
    );
    assert_eq!(
        unmanaged_preview.targets[0].change_kind,
        ChangeKind::Warning
    );
    assert!(unmanaged_preview.targets[0]
        .warning_codes
        .contains(&WARNING_EXTERNAL_NON_OWNED_CHANGE.to_owned()));

    let codex_project_path = fixture.project.join(".codex/config.toml");
    let mut managed_drift = fs::read_to_string(&codex_project_path)
        .expect("读取 Codex 项目 MCP 失败")
        .parse::<toml_edit::DocumentMut>()
        .expect("解析 Codex 项目 MCP 失败");
    managed_drift["mcp_servers"]["phase8-project-stdio"]["command"] =
        toml_edit::value("external-command");
    fs::write(&codex_project_path, managed_drift.to_string()).expect("写入受管漂移 fixture 失败");
    let managed_preview = preview_mcp_sync_with_probes(
        &mut fixture.database,
        &fixture.environment,
        &mut fixture.redactor,
        &PreviewMcpSyncInput {
            tool: Tool::Codex,
            project_id: Some(fixture.project_id.clone()),
            exclude_from_git: false,
        },
        &fixture.user_mcp_evidence,
        &fixture.policy_evidence,
    )
    .expect("检测受管漂移失败");
    assert_serialized_secrets_absent("受管漂移预览 RPC DTO", &managed_preview);
    assert_eq!(managed_preview.targets[0].change_kind, ChangeKind::Conflict);
    assert_eq!(
        managed_preview.targets[0].status,
        SyncStatus::ExternalOwnedChange
    );
    let conflict_error = apply_mcp_preview_with_probes(
        &fixture.write_operations,
        &mut fixture.database,
        &fixture.paths,
        &fixture.environment,
        &mut fixture.redactor,
        &ApplyMcpPreviewInput {
            preview_id: managed_preview.preview_id,
            tool: Tool::Codex,
            project_id: Some(fixture.project_id.clone()),
        },
        &fixture.user_mcp_evidence,
        &fixture.policy_evidence,
    )
    .expect_err("包含受管漂移的 Preview 不得应用");
    assert_eq!(conflict_error.code(), ErrorCode::Conflict);
    assert_secrets_absent(
        "RPC error",
        &format!(
            "{}\n{}",
            serde_json::to_string(&conflict_error).unwrap(),
            conflict_error
        ),
    );
    assert_eq!(
        fs::read_to_string(&codex_project_path)
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap()["mcp_servers"]["phase8-project-stdio"]["command"]
            .as_str(),
        Some("external-command")
    );

    for restore_case in restore_cases.iter().rev() {
        fixture.restore(restore_case);
    }
    assert_eq!(
        fs::read(fixture.claude_config.join("CLAUDE.md")).unwrap(),
        original_prompt,
        "Markdown 快照恢复必须逐字节还原"
    );
    assert_eq!(
        fixture_hash(&[
            ("home", &fixture.home),
            ("claude-config", &fixture.claude_config),
            ("codex-home", &fixture.codex_home),
            ("project", &fixture.project),
            ("sources", &fixture.sources),
            ("external-skill", &fixture.external_skill),
        ]),
        initial_hash,
        "快照恢复后原生 fixture 必须与初始 hash 一致"
    );
    audit_secret_surfaces(&fixture, &conflict_error);
}

fn assert_native_round_trip(fixture: &Fixture) {
    let claude_global: Value =
        serde_json::from_slice(&fs::read(fixture.home.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(claude_global["theme"], "dark");
    assert_eq!(claude_global["unknownTop"]["preserve"], true);
    assert_eq!(
        claude_global["mcpServers"]["external-global"]["unknown"]["value"],
        1
    );
    assert_eq!(
        claude_global["mcpServers"]["phase8-global-http"]["headers"]["Authorization"],
        format!("Bearer {HEADER_SECRET}")
    );

    let claude_project: Value = serde_json::from_slice(
        &fs::read(fixture.project.join(".mcp.json")).expect("读取 Claude 项目 MCP 失败"),
    )
    .unwrap();
    assert_eq!(claude_project["unknownProject"]["preserve"], true);
    assert!(claude_project["mcpServers"]
        .get("external-project")
        .is_some());
    assert!(claude_project["mcpServers"]
        .get("phase8-project-stdio")
        .is_some());
    assert!(claude_project["mcpServers"]
        .get("phase8-global-http")
        .is_none());

    let codex_global = fs::read_to_string(fixture.codex_home.join("config.toml")).unwrap();
    assert!(codex_global.contains("# Phase 8 Codex 顶层注释必须保留"));
    assert!(codex_global.contains("[plugins.phase8]"));
    assert!(codex_global.contains("[mcp_servers.external_global]"));
    assert!(codex_global.contains(HEADER_SECRET));
    let codex_project = fs::read_to_string(fixture.project.join(".codex/config.toml")).unwrap();
    assert!(codex_project.contains("# Phase 8 Codex 项目注释必须保留"));
    assert!(codex_project.contains("[features]"));
    assert!(codex_project.contains("[mcp_servers.external_project]"));
    assert!(codex_project.contains(ENV_SECRET));

    let links = [
        (
            fixture.claude_config.join("skills/phase8-global-skill"),
            "phase8-global-skill",
        ),
        (
            fixture.home.join(".agents/skills/phase8-global-skill"),
            "phase8-global-skill",
        ),
        (
            fixture.project.join(".claude/skills/phase8-project-skill"),
            "phase8-project-skill",
        ),
        (
            fixture.project.join(".agents/skills/phase8-project-skill"),
            "phase8-project-skill",
        ),
    ];
    for (link, expected_name) in links {
        assert!(fs::symlink_metadata(&link)
            .expect("缺少 Skill 目标链接")
            .file_type()
            .is_symlink());
        let canonical = fs::canonicalize(&link).expect("规范化 Skill 目标链接失败");
        assert!(canonical.starts_with(fixture.paths.central_skills()));
        let skill_md = fs::read_to_string(canonical.join("SKILL.md")).unwrap();
        assert!(skill_md.contains(&format!("name: {expected_name}")));
    }
    assert!(fixture
        .claude_config
        .join("skills/external-directory/KEEP.md")
        .is_file());
    assert!(
        fs::symlink_metadata(fixture.home.join(".agents/skills/external-symlink"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(fs::symlink_metadata(
        fixture
            .project
            .join(".agents/skills/broken-external-symlink")
    )
    .unwrap()
    .file_type()
    .is_symlink());
}

fn audit_secret_surfaces(fixture: &Fixture, rpc_error: &easytoagents_lib::error::AppError) {
    let snapshot_index = list_snapshots(&fixture.database).expect("读取真实快照 RPC 索引失败");
    assert!(!snapshot_index.is_empty(), "快照 RPC 审计载体不得为空");
    let sync_runs = query_audit_rows(
        &fixture.database,
        "SELECT id, kind, status, scope, COALESCE(project_id, ''), db_version,
                COALESCE(journal_path, ''), COALESCE(error_code, '')
         FROM sync_runs ORDER BY id",
        8,
    );
    assert!(!sync_runs.is_empty(), "sync_runs 审计载体不得为空");
    let sync_items = query_audit_rows(
        &fixture.database,
        "SELECT id, run_id, target_id, change_kind, status, redacted_diff_json,
                warning_codes_json, COALESCE(error_code, '')
         FROM sync_items ORDER BY id",
        8,
    );
    assert!(!sync_items.is_empty(), "sync_items 审计载体不得为空");
    let snapshot_metadata = query_audit_rows(
        &fixture.database,
        "SELECT id, run_id, COALESCE(target_id, ''), target_path, snapshot_path,
                COALESCE(content_hash, ''), target_type, COALESCE(link_target, '')
         FROM snapshots ORDER BY id",
        8,
    );
    assert!(
        !snapshot_metadata.is_empty(),
        "snapshots 持久化审计载体不得为空"
    );
    let journal_files = read_text_tree(fixture.paths.journals());
    assert!(!journal_files.is_empty(), "journal 审计载体不得为空");

    let mut surfaces = Vec::new();
    surfaces.push((
        "RPC error".to_owned(),
        format!(
            "{}\n{}",
            serde_json::to_string(rpc_error).unwrap(),
            rpc_error
        ),
    ));
    surfaces.push((
        "测试快照索引".to_owned(),
        serde_json::to_string(&snapshot_index).unwrap(),
    ));
    surfaces.push((
        "Dashboard 同步 RPC DTO".to_owned(),
        serde_json::to_string(
            &dashboard_summary(&fixture.database, &fixture.paths)
                .expect("生成 Dashboard 同步 DTO 失败"),
        )
        .expect("序列化 Dashboard 同步 DTO 失败"),
    ));
    surfaces.push(("sync_runs".to_owned(), sync_runs));
    surfaces.push(("sync_items/preview JSON".to_owned(), sync_items));
    surfaces.push(("snapshots metadata".to_owned(), snapshot_metadata));
    surfaces.push(("sync journal".to_owned(), journal_files));

    for (name, content) in surfaces {
        assert_secrets_absent(&name, &content);
    }
}

fn query_audit_rows(database: &Database, sql: &str, columns: usize) -> String {
    let mut statement = database.connection().prepare(sql).unwrap();
    statement
        .query_map([], |row| {
            let mut values = Vec::new();
            for column in 0..columns {
                values.push(
                    row.get::<_, String>(column)
                        .or_else(|_| row.get::<_, i64>(column).map(|value| value.to_string()))?,
                );
            }
            Ok(values.join("|"))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
}

fn assert_secrets_absent(surface: &str, content: &str) {
    for secret in [
        HEADER_SECRET,
        ENV_SECRET,
        EXTRA_SECRET,
        SKILL_PRIVATE_MARKER,
    ] {
        assert!(
            !content.contains(secret),
            "{surface} 泄漏了 Phase 8 fixture secret：{secret}"
        );
    }
}

fn assert_serialized_secrets_absent<T: Serialize>(surface: &str, value: &T) {
    let serialized = serde_json::to_string(value).expect("序列化真实 RPC DTO 失败");
    assert_secrets_absent(surface, &serialized);
}

fn read_text_tree(root: &Path) -> String {
    let mut files = Vec::new();
    collect_entries(root, root, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
        .into_iter()
        .map(|(relative, path)| {
            format!(
                "{}\n{}",
                relative.to_string_lossy(),
                fs::read_to_string(path).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixture_hash(roots: &[(&str, &PathBuf)]) -> String {
    let mut hasher = Sha256::new();
    for (label, root) in roots {
        hasher.update(label.as_bytes());
        hash_path(root, root, &mut hasher);
    }
    format!("{:x}", hasher.finalize())
}

fn hash_path(root: &Path, path: &Path, hasher: &mut Sha256) {
    let metadata = fs::symlink_metadata(path).expect("读取 fixture 元数据失败");
    let relative = path.strip_prefix(root).expect("计算 fixture 相对路径失败");
    hasher.update(relative.as_os_str().as_encoded_bytes());
    if metadata.file_type().is_symlink() {
        hasher.update(b"symlink\0");
        hasher.update(
            fs::read_link(path)
                .expect("读取 fixture 链接失败")
                .as_os_str()
                .as_encoded_bytes(),
        );
    } else if metadata.is_file() {
        hasher.update(b"file\0");
        hasher.update(fs::read(path).expect("读取 fixture 文件失败"));
    } else if metadata.is_dir() {
        hasher.update(b"directory\0");
        let mut entries = fs::read_dir(path)
            .expect("读取 fixture 目录失败")
            .map(|entry| entry.expect("读取 fixture 目录项失败").path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            hash_path(root, &entry, hasher);
        }
    } else {
        panic!("fixture 包含不支持的特殊文件：{}", path.display());
    }
}

fn collect_entries(root: &Path, path: &Path, files: &mut Vec<(PathBuf, PathBuf)>) {
    if !path.exists() {
        return;
    }
    for entry in fs::read_dir(path).expect("读取审计目录失败") {
        let entry = entry.expect("读取审计目录项失败");
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).expect("读取审计目录项元数据失败");
        if metadata.is_dir() {
            collect_entries(root, &path, files);
        } else if metadata.is_file() {
            files.push((
                path.strip_prefix(root)
                    .expect("计算审计文件相对路径失败")
                    .to_path_buf(),
                path,
            ));
        }
    }
}
