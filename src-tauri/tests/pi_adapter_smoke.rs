//! Pi 隔离 smoke 回归 fixture（design/implement 阶段 4.8）。
//!
//! `research/pi-mcp-adapter.md` §7 的 smoke 已在本机实跑；本文件把它固化为可复现
//! 回归：在 `HOME=<tmp>/home` + `PI_CODING_AGENT_DIR=<tmp>/agent` 的隔离环境里直接
//! import 适配器库，验证两件本任务的实际合同：
//!
//! 1. `getPiGlobalConfigPath()` 尊重 `PI_CODING_AGENT_DIR`（否则 EasyToAgents 会写
//!    到一个 Pi 不读的位置，整个 MCP 能力前提失效）；
//! 2. 我们写入的 `<agent_dir>/mcp.json` 能被适配器解析，且项目 `<root>/.pi/mcp.json`
//!    的同名条目优先级更高（遮蔽检测 `PI_MCP_SHADOWED_BY_PROJECT_PI` 的语义基础）。
//!
//! 没有安装 `pi-mcp-adapter` 或没有 `node` 时测试跳过并打印原因：这是**环境依赖**
//! 的 smoke，不是产品逻辑断言；CI 缺失该环境时不得因此变红。

use std::{fs, path::PathBuf, process::Command};

use serde_json::Value;

/// 本机 `pi-mcp-adapter` 安装目录；未安装时返回 `None`。
fn installed_adapter_package() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let package = PathBuf::from(home).join(".pi/agent/npm/node_modules/pi-mcp-adapter");
    package.join("package.json").is_file().then_some(package)
}

#[test]
fn pi_mcp_adapter_honours_isolated_agent_dir_and_project_precedence() {
    let Some(package) = installed_adapter_package() else {
        eprintln!("跳过：本机未安装 pi-mcp-adapter（~/.pi/agent/npm/node_modules/pi-mcp-adapter）");
        return;
    };
    let Some(node) = node_executable() else {
        eprintln!("跳过：未找到 node 可执行文件");
        return;
    };

    let temporary = tempfile::tempdir().expect("创建隔离 smoke 根失败");
    let root = fs::canonicalize(temporary.path()).expect("规范化隔离 smoke 根失败");
    let home = root.join("home");
    let agent_dir = root.join("agent");
    let project = root.join("project");
    for directory in [
        home.join(".config/mcp"),
        agent_dir.clone(),
        project.join(".pi"),
    ] {
        fs::create_dir_all(directory).expect("创建隔离 smoke 目录失败");
    }

    // 受管全局条目 + 项目同名条目：项目 `.pi/mcp.json` 必须胜出。
    fs::write(
        agent_dir.join("mcp.json"),
        br#"{"mcpServers":{"pi-only":{"url":"https://global.example.com/mcp"},"dup":{"url":"https://global.example.com/dup"}}}"#,
    )
    .expect("写入隔离 agent mcp.json 失败");
    fs::write(
        project.join(".pi/mcp.json"),
        br#"{"mcpServers":{"dup":{"url":"https://project.example.com/dup"}}}"#,
    )
    .expect("写入隔离项目 mcp.json 失败");

    let script = format!(
        r#"import {{ getPiGlobalConfigPath, loadMcpConfig }} from "{config}";
const config = loadMcpConfig(undefined, "{project}");
console.log(JSON.stringify({{
  global: getPiGlobalConfigPath(),
  dup: config.mcpServers.dup,
  servers: Object.keys(config.mcpServers).sort(),
}}));"#,
        config = package.join("dist/config.js").display(),
        project = project.display(),
    );
    let output = Command::new(node)
        .arg("--input-type=module")
        .arg("-e")
        .arg(&script)
        .env("HOME", &home)
        .env("PI_CODING_AGENT_DIR", &agent_dir)
        .output()
        .expect("无法运行 node 隔离 smoke");
    assert!(
        output.status.success(),
        "隔离 smoke 退出码非零：{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(stdout.trim()).expect("隔离 smoke 输出不是 JSON");
    assert_eq!(
        report["global"],
        Value::String(agent_dir.join("mcp.json").to_string_lossy().into_owned()),
        "getPiGlobalConfigPath 必须尊重 PI_CODING_AGENT_DIR"
    );
    assert_eq!(
        report["servers"],
        serde_json::json!(["dup", "pi-only"]),
        "我们写入的两条 mcpServers 条目都必须被适配器解析到"
    );
    assert_eq!(
        report["dup"]["url"],
        Value::String("https://project.example.com/dup".to_owned()),
        "项目 .pi/mcp.json 同名条目必须覆盖 <agent_dir>/mcp.json"
    );
}

fn node_executable() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("node"))
        .find(|candidate| candidate.is_file())
}
