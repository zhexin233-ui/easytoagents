# Phase 8 端到端质量门与 macOS 打包证据

记录日期：2026-08-24

执行环境：macOS 15.7.7（arm64）、Rust 1.92.0、Node.js 22.17.0、pnpm 10.13.1。

## 安全边界

- 新增 `src-tauri/tests/phase8_e2e.rs`，所有原生目标、Skill 来源、项目、应用数据目录均位于单一 `tempfile` 隔离根。
- 测试通过 `ExplicitEnvironment` 显式传入 `HOME`、非默认 `CLAUDE_CONFIG_DIR`、非默认 `CODEX_HOME`，通过 `AppPaths::from_data_root` 显式传入应用数据根。
- 非默认 Claude 配置根的用户 MCP 使用与配置根、安装版本和目标路径绑定的 `VerifiedClaudeUserMcpEvidence`，没有读取或猜测开发者真实配置。
- Claude MCP/Skills 使用与 fixture 安装版本绑定的 `VerifiedClaudeCustomizationPolicyEvidence`；该证据由测试显式注入，不来自开发者机器。
- 测试与构建均未启动真实 Claude/Codex，也未读取、扫描或写入开发者 HOME 下的 Claude/Codex 配置。
- 恢复阶段通过生产 `snapshot_restore_context` 按受管目标身份推导允许根，并逐项断言结果仍是上述 tempfile `HOME`、工具根或项目根；测试不自行复制恢复路由算法。

## 隔离端到端链路

`isolated_full_chain_restores_exact_fixture_and_leaks_no_secret` 覆盖：

1. 中央创建并分配 MCP：Claude/Codex 全局各 1 个目标，Claude/Codex 项目各 1 个目标。
2. 从两个隔离来源目录导入 Skill 中央副本，Claude/Codex 全局各 1 个链接目标，Claude/Codex 项目各 1 个链接目标；来源目录逐字节保持不变。
3. 无损导入原始 Claude Markdown 提示词，切换中央档案并应用 1 个文件目标。
4. 每个目标均执行持久化 Preview → Apply → 原生 JSON/TOML/Markdown/链接验证。
5. Claude JSON 顶层未知字段、外部 MCP 和嵌套未知字段保留；Codex TOML 顶层/项目注释、未知表和未知字段保留；Markdown 写入与恢复逐字节相等。
6. 外部非受管 JSON 字段变化被判定为 `external_non_owned_change` + warning；外部受管 TOML MCP command 变化被判定为 `external_owned_change` + conflict，Apply 返回稳定 `CONFLICT` 且不覆盖外部值。
7. 对 9 个写入前目标快照逐一执行恢复预览与恢复；最终 HOME、Claude/Codex 配置根、项目根、Skill 来源及外部 Skill 的组合 SHA-256 与初始 fixture hash 一致。
8. 初始未知普通 Skill 目录、外部 symlink 和断链在 Apply/Restore 后仍保持原身份。

实际目标矩阵：

| 资源 | Claude 全局 | Codex 全局 | Claude 项目 | Codex 项目 |
| --- | --- | --- | --- | --- |
| MCP | `$HOME/.claude.json` | `$CODEX_HOME/config.toml` | `<project>/.mcp.json` | `<project>/.codex/config.toml` |
| Skill | `$CLAUDE_CONFIG_DIR/skills/<name>` | `$HOME/.agents/skills/<name>` | `<project>/.claude/skills/<name>` | `<project>/.agents/skills/<name>` |

Markdown 另覆盖 `$CLAUDE_CONFIG_DIR/CLAUDE.md`。上述每个目标均调用生产 Service/Adapter 的可注入 probe 入口、持久化 Preview、Apply、原生解析/链接验证、恢复 Preview 与 Restore；没有测试专用写入引擎或恢复捷径。这里证明的是生产算法在显式证据下的链路，不等同于证明 release Tauri command 已接入真实工具/策略探针。

## 自动证据边界与发布命令阻塞

本轮复核确认，Phase 8 E2E 为了保持完全隔离，显式传入了 `ToolAvailability::all_installed()`、fixture Claude 版本，以及版本绑定的用户 MCP/Customization policy 证据。相对地，当前 release `run()` 边界仍把两个工具固定为 installed，未提供 Claude 安装版本；公开 MCP/Skills command 路径固定使用 `ConservativeClaudeCustomizationPolicyProbe`，因此 Claude MCP/Skills 会安全地保持 policy unknown/blocked，非默认 `CLAUDE_CONFIG_DIR` 的用户 MCP 也无法取得版本绑定 capability。

这不是越权写入风险：所有未知证据都 fail closed。但它意味着现有 E2E 不能证明以下发布行为，且在真实只读 discovery/命令边界接线完成前不得标成通过：

- AC1 的真实安装版本 capability/policy discovery；
- AC13 的工具未安装状态（release 当前固定报告两个工具 installed）；
- AC14 经由实际 Tauri command 边界完成 Claude MCP/Skills 链路。

## Secret audit

Phase 8 fixture secrets：HTTP Authorization、stdio env、MCP 扩展字段秘密及 Skill 私有正文标记。

以下载体逐一序列化/读取并断言四类 fixture secret 零明文命中：

- 生产 RPC DTO 的真实 Serde JSON（MCP/Skill CRUD 与 assignment、所有 Preview、ApplyResult、RestorePreview、Dashboard、SnapshotSummary）；
- `AppError` RPC JSON 与 `Display` 文本；
- SQLite 中实际持久化的 `sync_runs` 状态、错误码和 journal 路径；
- SQLite 中实际持久化的 `sync_items.redacted_diff_json`、warning/error 字段；
- 应用私有 journal 文件树；项目当前没有通用日志管线，源码也没有运行时 `println!`/`eprintln!`/`tracing`/`log` 载体；
- `list_snapshots` RPC 和 snapshots 元数据索引。

快照内容文件允许包含恢复所需的原始秘密，但只位于私有应用数据根且不进入上述索引/预览/日志审计载体。

## Destructive-path audit

完整 Rust 质量门通过了以下已有回归与新增端到端断言：

- `restore_never_deletes_unknown_directory_or_external_symlink`：恢复不得删除未知目录或外部链接。
- `recovery_cleanup_refuses_a_replaced_temporary_file`：临时路径身份被替换后保留。
- `rollback_preserves_a_concurrently_replaced_created_skill_directory`：同轮创建目录被替换后保留。
- `managed_symlink_uses_atomic_rename_and_refuses_directory_or_external_link`：只操作可证明的中央目标链接。
- `late_ancestor_symlink_escape_is_rejected_without_touching_outside`：写入边界再次 canonicalize/lstat，祖先链接替换后拒绝逃逸。
- `changed_quarantine_is_never_recursively_deleted`：中央 Skill quarantine hash/身份变化后不递归删除。
- `managed_link_removal_and_central_delete_are_safe`：只删除匹配 managed item/中央记录的链接和中央 child，来源与未知 sibling 保留。
- Phase 8 E2E：预置未知普通目录、外部 symlink、断链，完整 Apply/Restore 后最终 fixture hash 与初始一致。

因此删除/清理范围保持为：canonical 且可证明的中央直接 child、匹配 managed item、同轮同指纹临时项、同轮创建且身份未变的空目录。未知目录/链接/替换身份均保留。

## 自动质量门

执行命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test phase8_e2e -- --nocapture
pnpm check
pnpm tauri build
git diff --check
```

结果：

- Phase 8 E2E：1 passed。
- Vitest：8 个测试文件、33 passed。
- Rust：130 个 lib 单元测试 + bindings、command smoke、Phase 8 E2E 共 133 passed；0 failed/ignored。
- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、Prettier、ESLint、TypeScript typecheck 全部通过。
- `git diff --check` 通过；`dist/` 与 `src-tauri/target/` 均由 `.gitignore` 排除，构建产物未进入版本控制状态。

## macOS 产物

`pnpm tauri build` 成功：

- `.app`：`src-tauri/target/release/bundle/macos/EasyToAgents.app`，14,664 KiB。
- DMG：`src-tauri/target/release/bundle/dmg/EasyToAgents_0.1.0_aarch64.dmg`，5,239,758 bytes。
- 主程序：arm64 Mach-O，15,008,448 bytes。
- Bundle ID：`com.easytoagents.desktop`；版本 `0.1.0`；`LSMinimumSystemVersion=13.0`。
- Release WebView 启用 CSP，仅允许本地资源、Tauri IPC、内联样式与内嵌图片；主窗口 capability 仅含 `core:default` 和文件夹选择所需的 `dialog:allow-open`，未授予 shell/HTTP/文件系统插件权限。
- `hdiutil verify`：VALID。
- DMG SHA-256：`e4557b5ee34bc1d8846ffdb39fc9de42363cd72fe4e6bfa91ee56d30e927dd63`。

产物核验命令：

```bash
stat -f '%N %z bytes' src-tauri/target/release/easytoagents src-tauri/target/release/bundle/dmg/EasyToAgents_0.1.0_aarch64.dmg
du -sk src-tauri/target/release/bundle/macos/EasyToAgents.app
file src-tauri/target/release/easytoagents
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' src-tauri/target/release/bundle/macos/EasyToAgents.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' src-tauri/target/release/bundle/macos/EasyToAgents.app/Contents/Info.plist
/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' src-tauri/target/release/bundle/macos/EasyToAgents.app/Contents/Info.plist
hdiutil verify src-tauri/target/release/bundle/dmg/EasyToAgents_0.1.0_aarch64.dmg
shasum -a 256 src-tauri/target/release/bundle/dmg/EasyToAgents_0.1.0_aarch64.dmg
codesign -dvvv src-tauri/target/release/easytoagents
codesign --verify --deep --strict --verbose=4 src-tauri/target/release/bundle/macos/EasyToAgents.app
```

发布风险：当前未配置 Developer ID 签名和 notarization。Mach-O 只有 ad-hoc linker signature，`.app` 没有 sealed resources，故 `codesign --verify --deep --strict` 不通过。Phase 8 已证明可构建 `.app`/DMG，但面向外部分发前仍需配置签名、公证并在 macOS 13+ 重新做 Gatekeeper 安装验证。

## Smoke 与人工剩余项

未直接启动 release bundle。原因：当前运行时应用数据目录由 Tauri/macOS 解析，没有一个可在 release 启动命令中证明生效的显式应用数据根覆盖入口；仅覆盖 `HOME` 仍不足以满足“HOME、CLAUDE_CONFIG_DIR、CODEX_HOME、应用数据根、项目根全部显式隔离”的安全条件。

仍需在专用临时 macOS 用户或隔离 VM 中人工完成：

- DMG 挂载、拖入 Applications、Gatekeeper/签名/公证验证与首次启动；
- 文件选择器导入 Skill，并确认来源目录不变；
- 权限不足、Claude policy blocked、Codex untrusted 的独立 UI 状态；
- SnapshotRestoreDialog 恢复确认、恢复后原生目标 hash 核对；
- 真实 Claude/Codex 当前安装版本只读 discovery，以及用户确认后的隔离样本写入。

## AC1–AC14 与 Out of Scope 核对

| AC | 自动证据 | 人工剩余 |
| --- | --- | --- |
| AC1 | adapter 路径矩阵、非默认根 capability fail-closed、bundle 最低 macOS 13.0 | release 命令边界接入真实版本与 policy probe；安装/首次启动、真实版本只读 discovery |
| AC2–AC3 | Provider/Prompt service 回归、secret audit、Markdown 无损导入/精确恢复 | 新会话生效提示 smoke |
| AC4–AC8 | MCP/Skills service 回归 + Phase 8 全局/项目 E2E | 文件选择器 smoke |
| AC9–AC11 | Preview/Apply/漂移/故障注入/rollback/restore 回归 + Phase 8 最终 hash | 恢复对话框人工确认 |
| AC12–AC13 | Git exclude、tracked warning、trust/policy/permission 状态回归和前端测试 | 工具 availability 的 release 探针；权限与受阻状态视觉 smoke |
| AC14 | Phase 8 中央变更 → 持久化预览 → Apply → 原生验证 → 漂移 → 快照恢复完整自动链路（显式 fixture probe） | Claude MCP/Skills 的真实 release command probe 接线 |

Out of Scope 关键字搜索覆盖产品源码与依赖清单。命中项逐一分类后仅为：Tauri 生成绑定的 `listen`/`Proxy`、特殊文件拒绝测试使用的 `UnixListener`、禁止“项目禁用全局项”的约束注释，以及脱敏测试中的 `proxy=` 字符串；均不是产品能力。依赖和生产模块中没有市场、云同步、WebDAV、钥匙串、SSH/WSL、代理服务、Copy mode、Preset、项目级全局禁用或 Windows/Linux bundle 实现。

## Git 污染状态

- `dist/` 与 `src-tauri/target/` 均由 `.gitignore` 排除；测试结束后没有 tempfile、真实用户配置或构建产物进入版本控制状态。
- 工作树整体并不干净：本轮 reviewer 修复/规范同步共修改 11 个已跟踪文件；此外仍有仓库既存的未跟踪 Trellis/平台模板与研究文件。未擅自删除、改写或归类这些未跟踪文件，因此实施计划中的“`git status` 无污染”最终人工项继续保持未完成；提交时必须逐文件核对 reviewer 修改，不能使用宽泛的 `git add .`。
