//! `pi-mcp-adapter` 就绪探测（只读文件探测，不执行 `pi`）。
//!
//! Pi 核心不含内置 MCP；`<pi_agent_dir>/mcp.json` 与 `<root>/.pi/mcp.json`
//! 只在第三方适配器加载时才有意义。适配器缺失、被 `pi config` 过滤或版本低于
//! 最低支持版本时，写这两个文件属于**必然无效**的写入，因此 descriptor 必须
//! fail closed。本模块只做只读判定，不读取任何凭据文件，也不执行外部命令。
//!
//! 判定依据（`research/pi-mcp-adapter.md` §4.1，2026-09-14 / 2.33.0）：
//! - `<pi_agent_dir>/settings.json` 的 `packages[]`（字符串或 `{ source }` 对象）；
//! - 对象形条目的 `extensions: []` 表示被 `pi config` 过滤未加载；
//! - `<pi_agent_dir>/npm/node_modules/pi-mcp-adapter/package.json` 的存在与版本；
//! - `<project>/.pi/settings.json` 与 `<project>/.pi/npm/...` 的项目级声明/安装。

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

/// 本次核验基线（2.33.0）。低于该版本时行为未知，必须 fail closed。
pub const PI_MCP_ADAPTER_MIN_SUPPORTED_VERSION: &str = "2.33.0";

/// 适配器未声明且未安装：写入无人读。
pub const PI_MCP_ADAPTER_MISSING: &str = "PI_MCP_ADAPTER_MISSING";
/// 已声明/已安装但被 `pi config` 过滤（`extensions: []`）或项目级启用未受信任。
pub const PI_MCP_ADAPTER_NOT_LOADED: &str = "PI_MCP_ADAPTER_NOT_LOADED";
/// 适配器版本低于最低支持版本。
pub const PI_MCP_ADAPTER_VERSION_UNSUPPORTED: &str = "PI_MCP_ADAPTER_VERSION_UNSUPPORTED";

const ADAPTER_PACKAGE_NAME: &str = "pi-mcp-adapter";
const ADAPTER_PACKAGE_SPEC: &str = "npm:pi-mcp-adapter";
const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiMcpAdapterState {
    /// 适配器已声明、已安装且版本满足要求。
    Ready,
    /// 未声明且未安装。
    Missing,
    /// 已声明/已安装但未加载（被过滤或项目级未受信任）。
    NotLoaded,
    /// 安装版本低于最低支持版本。
    VersionUnsupported,
}

impl PiMcpAdapterState {
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// 非 Ready 状态对应的稳定诊断码；`Ready` 返回 `None`。
    pub const fn diagnostic_code(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Missing => Some(PI_MCP_ADAPTER_MISSING),
            Self::NotLoaded => Some(PI_MCP_ADAPTER_NOT_LOADED),
            Self::VersionUnsupported => Some(PI_MCP_ADAPTER_VERSION_UNSUPPORTED),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiMcpAdapterProbe {
    pub state: PiMcpAdapterState,
    /// 安装目录 `package.json` 中读到的版本（若存在）。
    pub version: Option<String>,
    /// 是否在某个作用域的 `packages[]` 中找到适配器声明。
    pub declared: bool,
}

impl PiMcpAdapterProbe {
    pub fn missing() -> Self {
        Self {
            state: PiMcpAdapterState::Missing,
            version: None,
            declared: false,
        }
    }
}

/// 显式探测输入。调用方只传路径与 trust/exclusive 事实，不传进程环境。
#[derive(Debug, Clone, Copy)]
pub struct PiMcpAdapterProbeInput<'a> {
    pub pi_agent_dir: &'a Path,
    /// 项目根；`None` 时只探测全局作用域。
    pub project_root: Option<&'a Path>,
    /// 项目是否受 Pi 信任。项目级 `packages` 未受信任时视为未加载。
    pub project_trusted: bool,
    /// `PI_MCP_CONFIG_MODE=exclusive` 时项目配置被忽略。
    pub exclusive_mode: bool,
}

/// 只读探测适配器就绪状态。任何读取/解析失败都按最保守状态处理。
pub fn probe_mcp_adapter(input: &PiMcpAdapterProbeInput<'_>) -> PiMcpAdapterProbe {
    let global = evaluate_scope(
        &input.pi_agent_dir.join("settings.json"),
        &input
            .pi_agent_dir
            .join("npm/node_modules")
            .join(ADAPTER_PACKAGE_NAME)
            .join("package.json"),
        true,
    );
    let project = input
        .project_root
        .filter(|_| !input.exclusive_mode)
        .map(|root| {
            evaluate_scope(
                &root.join(".pi/settings.json"),
                &root
                    .join(".pi/npm/node_modules")
                    .join(ADAPTER_PACKAGE_NAME)
                    .join("package.json"),
                input.project_trusted,
            )
        })
        .unwrap_or(ScopeReadiness::NotDeclared);
    let declared = !matches!(&global, ScopeReadiness::NotDeclared)
        || !matches!(&project, ScopeReadiness::NotDeclared);

    // Pi 在解析项目 trust 前已经加载 user/global extensions。因而全局适配器
    // 一旦 Ready，项目 package 被过滤或项目未受信任都不能撤销全局加载结果。
    if let ScopeReadiness::Installed(version) = &global {
        if version_meets_minimum(version) {
            return PiMcpAdapterProbe {
                state: PiMcpAdapterState::Ready,
                version: Some(version.clone()),
                declared,
            };
        }
    }
    let scopes = [global, project];
    // 全局没有可用安装时，项目 scope 才决定当前项目上下文是否能加载扩展。
    // 过滤/未受信任仍优先于项目安装目录，保持 fail closed。
    if scopes
        .iter()
        .any(|scope| matches!(scope, ScopeReadiness::Filtered))
    {
        return PiMcpAdapterProbe {
            state: PiMcpAdapterState::NotLoaded,
            version: installed_version(&scopes),
            declared,
        };
    }
    if let Some(version) = installed_version(&scopes) {
        let state = if version_meets_minimum(&version) {
            PiMcpAdapterState::Ready
        } else {
            PiMcpAdapterState::VersionUnsupported
        };
        return PiMcpAdapterProbe {
            state,
            version: Some(version),
            declared,
        };
    }
    PiMcpAdapterProbe {
        state: PiMcpAdapterState::Missing,
        version: None,
        declared,
    }
}

/// 单个作用域（全局或项目）的只读判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ScopeReadiness {
    NotDeclared,
    /// `packages[]` 声明了适配器，但被 `extensions: []` 过滤或未受信任。
    Filtered,
    /// 声明且安装目录存在（携带版本）。
    Installed(String),
    /// 声明但安装目录缺失（Pi 会自动安装；期间写入无效）。
    DeclaredNotInstalled,
}

fn evaluate_scope(settings_path: &Path, package_json_path: &Path, trusted: bool) -> ScopeReadiness {
    let declaration = read_package_declaration(settings_path);
    let installed = read_package_version(package_json_path);
    match declaration {
        PackageDeclaration::Absent => {
            // 未声明即不会被加载：即使目录存在也只按缺失处理（保守）。
            if installed.is_some() {
                ScopeReadiness::DeclaredNotInstalled
            } else {
                ScopeReadiness::NotDeclared
            }
        }
        PackageDeclaration::Filtered => ScopeReadiness::Filtered,
        PackageDeclaration::Present if !trusted => ScopeReadiness::Filtered,
        PackageDeclaration::Present => match installed {
            Some(version) => ScopeReadiness::Installed(version),
            None => ScopeReadiness::DeclaredNotInstalled,
        },
    }
}

fn installed_version(scopes: &[ScopeReadiness; 2]) -> Option<String> {
    scopes.iter().find_map(|scope| match scope {
        ScopeReadiness::Installed(version) => Some(version.clone()),
        _ => None,
    })
}

enum PackageDeclaration {
    Absent,
    Present,
    Filtered,
}

/// 解析 `settings.json` 的 `packages[]`，判断适配器是否被声明、是否被过滤。
/// 任何解析失败都按 `Absent` 处理（fail closed）。
fn read_package_declaration(settings_path: &Path) -> PackageDeclaration {
    let Some(value) = read_json_file(settings_path) else {
        return PackageDeclaration::Absent;
    };
    let Some(packages) = value.get("packages").and_then(Value::as_array) else {
        return PackageDeclaration::Absent;
    };
    let mut declared = false;
    let mut filtered = false;
    for entry in packages {
        match entry {
            Value::String(source) if source == ADAPTER_PACKAGE_SPEC => declared = true,
            Value::Object(object) => {
                let source = object
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if source != ADAPTER_PACKAGE_SPEC {
                    continue;
                }
                declared = true;
                // `extensions: []` 表示 `pi config` 关闭了该包的全部扩展。
                if object
                    .get("extensions")
                    .and_then(Value::as_array)
                    .is_some_and(|extensions| extensions.is_empty())
                {
                    filtered = true;
                }
            }
            _ => {}
        }
    }
    if filtered {
        PackageDeclaration::Filtered
    } else if declared {
        PackageDeclaration::Present
    } else {
        PackageDeclaration::Absent
    }
}

fn read_package_version(package_json_path: &Path) -> Option<String> {
    let value = read_json_file(package_json_path)?;
    if value.get("name").and_then(Value::as_str) != Some(ADAPTER_PACKAGE_NAME) {
        return None;
    }
    let version = value.get("version").and_then(Value::as_str)?;
    // 只接受合法 semver 前缀，异常输出视为未知版本（fail closed）。
    parse_semver(version).map(|_| version.to_owned())
}

/// 只读、无跟随的 JSON 文件读取：拒绝符号链接与非普通文件，限制大小。
pub(crate) fn read_json_file(path: &Path) -> Option<Value> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn version_meets_minimum(version: &str) -> bool {
    match (
        parse_semver(version),
        parse_semver(PI_MCP_ADAPTER_MIN_SUPPORTED_VERSION),
    ) {
        (Some(actual), Some(minimum)) => actual >= minimum,
        _ => false,
    }
}

/// 解析 `major.minor.patch`（忽略 pre-release/build 元数据）。
fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 便捷函数：给定 `<pi_agent_dir>`，返回全局适配器安装 package.json 路径。
pub fn global_adapter_package_json(pi_agent_dir: &Path) -> PathBuf {
    pi_agent_dir
        .join("npm/node_modules")
        .join(ADAPTER_PACKAGE_NAME)
        .join("package.json")
}
