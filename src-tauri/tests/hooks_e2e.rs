//! Hooks 跨层链路：中央意图 → 全局分配 → 持久化预览 → Apply → 原生合同
//! 断言 → 外部漂移 → 重新接管。与 phase8 链式测试互补，聚焦 hooks 的
//! 四工具原生投影与选择器共存合同。

use std::{fs, path::PathBuf, sync::Mutex};

use easytoagents_lib::{
    adapters::ExplicitEnvironment,
    app::AppPaths,
    db::Database,
    domain::{HookEvent, SyncStatus, Tool},
    hooks::{
        apply_hook_preview, create_hook, preview_hook_sync, readopt_hook_target,
        set_global_hook_assignment, ApplyHookPreviewInput, PreviewHookSyncInput,
        ReadoptHookTargetInput, SetGlobalHookAssignmentInput,
    },
    security::SecretRedactor,
    sync::list_snapshots,
};
use serde_json::{json, Value};
use tempfile::TempDir;

struct Fixture {
    _temporary: TempDir,
    paths: AppPaths,
    database: Database,
    environment: ExplicitEnvironment,
    write_operations: Mutex<()>,
    redactor: SecretRedactor,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("创建隔离根失败");
        let root = fs::canonicalize(temporary.path()).expect("规范化隔离根失败");
        let home = root.join("home");
        let claude_config = root.join("claude-config");
        let codex_home = root.join("codex-home");
        for directory in [&home, &claude_config, &codex_home] {
            fs::create_dir(directory).expect("创建 fixture 目录失败");
        }
        fs::create_dir_all(home.join(".cursor")).expect("创建 .cursor 失败");
        let paths = AppPaths::from_data_root(root.join("app-data")).expect("初始化数据根失败");
        let database = Database::open(&paths).expect("打开数据库失败");
        let environment = ExplicitEnvironment::new(
            &home,
            Some(claude_config),
            Some(codex_home),
            easytoagents_lib::adapters::ToolAvailability::all_installed(),
        )
        .expect("构造显式环境失败");
        Self {
            _temporary: temporary,
            paths,
            database,
            environment,
            write_operations: Mutex::new(()),
            redactor: SecretRedactor::default(),
        }
    }

    fn claude_settings(&self) -> PathBuf {
        self.environment.claude_config_dir().join("settings.json")
    }

    fn codex_hooks(&self) -> PathBuf {
        self.environment.codex_home().join("hooks.json")
    }

    fn cursor_hooks(&self) -> PathBuf {
        self.environment.home().join(".cursor/hooks.json")
    }

    fn zcode_cli_config(&self) -> PathBuf {
        self.environment.home().join(".zcode/cli/config.json")
    }
}

/// 预置原生共存内容：hooks 之外的字段（provider env、MCP、描述）必须保留。
fn seed_native_files(fixture: &Fixture) {
    fs::create_dir_all(fixture.environment.home().join(".zcode/cli"))
        .expect("创建 .zcode/cli 失败");
    fs::write(
        fixture.claude_settings(),
        r#"{
  "env": {"ANTHROPIC_BASE_URL": "https://keep.example.test"},
  "permissions": {"allow": ["Bash(ls)"]}
}
"#,
    )
    .expect("写入 Claude settings fixture 失败");
    fs::write(
        fixture.codex_hooks(),
        r#"{"description": "保留我", "hooks": {}}"#,
    )
    .expect("写入 Codex hooks fixture 失败");
    fs::write(
        fixture.zcode_cli_config(),
        r#"{"mcp": {"servers": {"keep": {"command": "keep"}}}, "hooks": {"events": {}}}"#,
    )
    .expect("写入 ZCode config fixture 失败");
}

#[test]
fn hooks_global_chain_applies_each_tool_contract_and_recovers_from_drift() {
    let mut fixture = Fixture::new();
    seed_native_files(&fixture);

    // 中央意图 + 四工具全局分配（分配不隐式 Apply，原生文件此刻未变）。
    let hook = create_hook(
        &mut fixture.database,
        &fixture.paths,
        &easytoagents_lib::hooks::CreateHookInput {
            name: "block-rm".to_owned(),
            event: HookEvent::PreToolUse,
            matcher: Some("Bash".to_owned()),
            command: "bash .claude/hooks/block-rm.sh".to_owned(),
            timeout_seconds: Some(30),
            enabled: true,
            script_source_path: None,
        },
    )
    .expect("创建中央 Hook 失败");
    let mut assigned = hook;
    for tool in [Tool::Claude, Tool::Codex, Tool::Cursor, Tool::Zcode] {
        assigned = set_global_hook_assignment(
            &mut fixture.database,
            &SetGlobalHookAssignmentInput {
                tool,
                hook_id: assigned.id.clone(),
                event: HookEvent::PreToolUse,
                assigned: true,
                row_version: assigned.row_version,
            },
        )
        .expect("全局分配失败");
    }
    assert!(
        !fixture.claude_settings().exists() || {
            let content = fs::read_to_string(fixture.claude_settings()).unwrap();
            !content.contains("block-rm")
        }
    );

    // 逐工具 Preview → Apply，并断言各自的原生合同。
    for tool in [Tool::Claude, Tool::Codex, Tool::Cursor, Tool::Zcode] {
        let plan = preview_hook_sync(
            &mut fixture.database,
            &fixture.environment,
            &mut fixture.redactor,
            &PreviewHookSyncInput {
                tool,
                project_id: None,
                exclude_from_git: false,
            },
        )
        .expect("生成全局预览失败");
        assert_eq!(plan.targets.len(), 1, "{tool:?} 应恰好有一个受管目标");
        assert!(
            matches!(
                plan.targets[0].change_kind,
                easytoagents_lib::domain::ChangeKind::Add
                    | easytoagents_lib::domain::ChangeKind::Update
            ),
            "{tool:?} 首次同步应为新增或更新，实际为 {:?}（目标文件可能已预置共存内容）",
            plan.targets[0].change_kind
        );
        let result = apply_hook_preview(
            &fixture.write_operations,
            &mut fixture.database,
            &fixture.paths,
            &fixture.environment,
            &ApplyHookPreviewInput {
                preview_id: plan.preview_id.clone(),
                tool,
                project_id: None,
            },
        )
        .expect("应用全局预览失败");
        assert_eq!(result.applied_targets, 1);
        assert!(result.snapshot_count >= 1, "{tool:?} 必须产生快照");
    }

    // Claude：hooks 子树写入且 env/permissions 原样保留。
    let claude: Value =
        serde_json::from_str(&fs::read_to_string(fixture.claude_settings()).unwrap()).unwrap();
    assert_eq!(
        claude["hooks"]["PreToolUse"][0],
        json!({
            "matcher": "Bash",
            "hooks": [{"type": "command", "command": "bash .claude/hooks/block-rm.sh", "timeout": 30}]
        })
    );
    assert_eq!(
        claude["env"]["ANTHROPIC_BASE_URL"],
        "https://keep.example.test"
    );
    assert!(claude.get("permissions").is_some());

    // Codex：独立 hooks.json，顶层 description 保留。
    let codex: Value =
        serde_json::from_str(&fs::read_to_string(fixture.codex_hooks()).unwrap()).unwrap();
    assert_eq!(codex["description"], "保留我");
    assert_eq!(
        codex["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "bash .claude/hooks/block-rm.sh"
    );

    // Cursor：version + hooks，事件键为 camelCase，matcher 属于条目。
    let cursor: Value =
        serde_json::from_str(&fs::read_to_string(fixture.cursor_hooks()).unwrap()).unwrap();
    assert_eq!(cursor["version"], 1);
    assert_eq!(
        cursor["hooks"]["preToolUse"][0],
        json!({"command": "bash .claude/hooks/block-rm.sh", "timeout": 30, "matcher": "Bash"})
    );

    // ZCode：events 嵌套 + runner 级 enabled 恒为 true，mcp.servers 保留。
    let zcode: Value =
        serde_json::from_str(&fs::read_to_string(fixture.zcode_cli_config()).unwrap()).unwrap();
    assert_eq!(zcode["hooks"]["enabled"], true);
    assert_eq!(
        zcode["hooks"]["events"]["PreToolUse"][0]["hooks"][0]["command"],
        "bash .claude/hooks/block-rm.sh"
    );
    assert_eq!(zcode["mcp"]["servers"]["keep"]["command"], "keep");

    // 每工具一份快照进入账本（供恢复链路复用）。
    let snapshots = list_snapshots(&fixture.database).expect("读取快照失败");
    assert_eq!(snapshots.len(), 4);

    // 外部改写受管条目 → 外部拥有内容变更，重新接管后回到一致。
    let mut drifted = claude.clone();
    drifted["hooks"]["PreToolUse"][0]["hooks"][0]["command"] =
        Value::String("bash /malicious/rewrite.sh".to_owned());
    fs::write(
        fixture.claude_settings(),
        serde_json::to_string_pretty(&drifted).unwrap(),
    )
    .expect("写入外部漂移失败");
    let plan = preview_hook_sync(
        &mut fixture.database,
        &fixture.environment,
        &mut fixture.redactor,
        &PreviewHookSyncInput {
            tool: Tool::Claude,
            project_id: None,
            exclude_from_git: false,
        },
    )
    .expect("漂移后生成预览失败");
    assert_eq!(plan.targets[0].status, SyncStatus::ExternalOwnedChange);
    assert!(plan.targets[0].readopt_available);

    let readopt = readopt_hook_target(
        &mut fixture.database,
        &fixture.environment,
        &ReadoptHookTargetInput {
            tool: Tool::Claude,
            project_id: None,
        },
    )
    .expect("重新接管失败");
    assert_eq!(readopt.updated_item_count, 1);
    assert_eq!(readopt.removed_item_count, 0);

    let plan = preview_hook_sync(
        &mut fixture.database,
        &fixture.environment,
        &mut fixture.redactor,
        &PreviewHookSyncInput {
            tool: Tool::Claude,
            project_id: None,
            exclude_from_git: false,
        },
    )
    .expect("接管后生成预览失败");
    assert_eq!(plan.targets[0].status, SyncStatus::InSync);
}

/// 导入接管链路：原生全局配置引用的脚本被复制进中央目录，确认后中央命令
/// 重写为引用中央副本，同步 Apply 后原生配置直接引用中央路径；原文件保留。
#[test]
fn hook_import_adopts_script_into_central_storage_and_native_references_it() {
    use easytoagents_lib::hooks::{
        confirm_hook_import, discover_hook_import, ConfirmHookImportInput, DiscoverHookImportInput,
    };

    let mut fixture = Fixture::new();
    seed_native_files(&fixture);

    // 原生脚本 + 全局 hooks 配置引用它（/usr/bin/env python3 形式，回归
    // 2026-09-05 用户反馈：env 间接层命令此前被误判为不可接管）。
    let script_dir = fixture.environment.claude_config_dir().join("hooks");
    fs::create_dir_all(&script_dir).expect("创建原生脚本目录失败");
    let script_path = script_dir.join("deny_dotenv.py");
    fs::write(&script_path, b"#!/usr/bin/env python3\nprint('deny')\n").expect("写入原脚本失败");
    let claude_settings = fixture.claude_settings();
    let original_settings = fs::read_to_string(&claude_settings).unwrap();
    fs::write(
        &claude_settings,
        format!(
            r#"{{
  "env": {{"ANTHROPIC_BASE_URL": "https://keep.example.test"}},
  "hooks": {{"PreToolUse": [{{"matcher": "Bash", "hooks": [
    {{"type": "command", "command": "/usr/bin/env python3 {script_path}", "timeout": 30}}
  ]}}]}}
}}
"#,
            script_path = script_path.to_string_lossy()
        ),
    )
    .expect("写入引用脚本的 hooks 配置失败");

    // 只读发现：候选标记脚本接管与来源路径。
    let preview = discover_hook_import(
        &mut fixture.database,
        &fixture.environment,
        &DiscoverHookImportInput { tool: Tool::Claude },
    )
    .expect("发现导入候选失败");
    assert_eq!(preview.candidates.len(), 1);
    let candidate = &preview.candidates[0];
    assert_eq!(
        candidate.status,
        easytoagents_lib::hooks::HookImportCandidateStatus::Importable
    );
    assert!(candidate.script_adopted);
    assert_eq!(
        candidate.script_source_path.as_deref(),
        Some(script_path.to_string_lossy().as_ref())
    );

    // 确认导入：脚本复制进中央目录（0600），命令重写为引用中央副本。
    let hook_name = candidate.name.clone();
    let result = confirm_hook_import(
        &mut fixture.database,
        &fixture.paths,
        &fixture.environment,
        &ConfirmHookImportInput {
            tool: Tool::Claude,
            hooks: vec![easytoagents_lib::hooks::CreateHookInput {
                name: hook_name,
                event: HookEvent::PreToolUse,
                matcher: Some("Bash".to_owned()),
                command: candidate.command.clone(),
                timeout_seconds: candidate.timeout_seconds,
                enabled: true,
                script_source_path: candidate.script_source_path.clone(),
            }],
        },
    )
    .expect("确认导入失败");
    assert_eq!(result.created_count, 1);
    // 原脚本文件保持原样（复制不移动）。
    assert!(script_path.exists());

    let hooks = easytoagents_lib::hooks::list_hooks(&fixture.database).unwrap();
    assert_eq!(hooks.len(), 1);
    let imported = &hooks[0];
    let script_name = imported.script_name.clone().expect("接管型必有脚本名");
    assert_eq!(
        imported.command,
        format!(
            "/usr/bin/env python3 \"{}\"",
            fixture
                .paths
                .central_hooks()
                .join(&imported.id)
                .join(&script_name)
                .to_string_lossy()
        )
    );
    use std::os::unix::fs::PermissionsExt;
    let central_script = fixture
        .paths
        .central_hooks()
        .join(&imported.id)
        .join(&script_name);
    let permissions = fs::metadata(&central_script).unwrap().permissions();
    assert_eq!(permissions.mode() & 0o777, 0o600, "中央脚本必须 0600");

    // 分配 + 同步：原生配置直接引用中央副本。
    let assigned = set_global_hook_assignment(
        &mut fixture.database,
        &SetGlobalHookAssignmentInput {
            tool: Tool::Claude,
            hook_id: imported.id.clone(),
            event: HookEvent::PreToolUse,
            assigned: true,
            row_version: imported.row_version,
        },
    )
    .expect("全局分配失败");
    let plan = preview_hook_sync(
        &mut fixture.database,
        &fixture.environment,
        &mut fixture.redactor,
        &PreviewHookSyncInput {
            tool: Tool::Claude,
            project_id: None,
            exclude_from_git: false,
        },
    )
    .expect("生成预览失败");
    apply_hook_preview(
        &fixture.write_operations,
        &mut fixture.database,
        &fixture.paths,
        &fixture.environment,
        &ApplyHookPreviewInput {
            preview_id: plan.preview_id,
            tool: Tool::Claude,
            project_id: None,
        },
    )
    .expect("应用预览失败");

    let synced: Value =
        serde_json::from_str(&fs::read_to_string(&claude_settings).unwrap()).unwrap();
    assert_eq!(
        synced["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        assigned.command
    );
    assert!(assigned.command.contains("hooks/"), "命令必须引用中央目录");
    // 非受管内容（env）仍然保留。
    assert_eq!(
        synced["env"]["ANTHROPIC_BASE_URL"],
        "https://keep.example.test"
    );
    let _ = original_settings;

    // 删除未分配的 hook 会清理中央目录；分配中的删除被外键阻止，
    // 这里先取消分配再删除以验证清理路径。
    let unassigned = set_global_hook_assignment(
        &mut fixture.database,
        &SetGlobalHookAssignmentInput {
            tool: Tool::Claude,
            hook_id: imported.id.clone(),
            event: HookEvent::PreToolUse,
            assigned: false,
            row_version: assigned.row_version,
        },
    )
    .expect("取消分配失败");
    easytoagents_lib::hooks::delete_hook(
        &mut fixture.database,
        &fixture.paths,
        &easytoagents_lib::hooks::VersionedHookInput {
            id: unassigned.id.clone(),
            row_version: unassigned.row_version,
        },
    )
    .expect("删除 Hook 失败");
    assert!(!fixture.paths.central_hooks().join(&imported.id).exists());
}
