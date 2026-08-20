# Claude 与 Codex 配置管理桌面端：实施计划

## 执行原则

- 本任务按共享同步内核的一条端到端链路实施，不拆成独立子任务。
- 每一阶段先补 fixture/测试，再接 UI；任何外部写入能力都必须经过 Preview/Apply。
- 阶段完成条件是对应验证命令通过并形成可回滚检查点，不以“页面能打开”替代数据与文件验证。
- 所有前端依赖使用 pnpm；所有代码注释、提交信息和用户文案使用简体中文。

## Phase 0：项目脚手架与质量基线

- [x] 使用 pnpm 创建 React + TypeScript + Vite 项目并接入 Tauri 2，配置 macOS 13+ bundle target。
- [x] 建立 `src/` 与 `src-tauri/src/` 的 feature/layer 目录；配置路径别名。
- [x] 接入 ESLint、Prettier、Vitest、React Testing Library、Rust fmt/clippy/test。
- [x] 在 `package.json` 明确定义 `test: vitest`、`tauri: tauri`、`lint`、`typecheck`、`check`，并先验证 pnpm 能正确把 `--run`、`build --debug` 参数透传给脚本。
- [x] 配置 Tailwind CSS 与最小 shadcn/ui/Radix 基础组件，不批量安装未使用组件。
- [x] 建立 Tauri command smoke test 与 Rust→TypeScript DTO 生成；验证前端不使用手工 `as` 解析 RPC。
- [x] 增加 CI 等价的本地 `pnpm check` 脚本。

验证：

```bash
pnpm install
pnpm lint
pnpm typecheck
pnpm test --run
pnpm tauri build --debug
```

回滚点：只包含可启动空壳、质量配置和目录结构。

## Phase 1：领域模型、数据库与应用私有路径

- [x] 实现 Tool、Scope、ArtifactKind、SyncStatus、ChangeKind、AppError 等领域枚举。
- [x] 建立 SQLite 初始化、WAL、foreign_keys、migration runner 和启动前数据库备份。
- [x] 创建 Provider/Prompt/MCP/Skill/Project、四类 assignment、managed target/item、sync run/item、snapshot 表及索引。
- [x] 为 active profile、唯一名称、项目路径和 assignment 不变量添加数据库/领域双层验证。
- [x] 实现 macOS Application Support、中央 Skills、snapshots、staging 路径解析；目录设为 `0700`，数据库/WAL/SHM/journal/快照等敏感文件限制为当前用户读写。
- [x] 实现统一 secret redactor：覆盖 API Key、Authorization、token、MCP header/env、扩展 JSON 的敏感 selector/键名和值登记表；日志、RPC error、journal、崩溃上下文只允许脱敏结构。

验证：

```bash
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

回滚点：数据库迁移文件和空数据库 fixture；不接触真实 Claude/Codex 文件。

## Phase 2：Adapter 读取、基线和 Preview 内核

- [x] 实现目标矩阵路径解析：`CLAUDE_CONFIG_DIR` 影响 Claude settings/提示词/Skills，`CODEX_HOME` 影响 Codex config/提示词，Codex Skills 固定使用 `$HOME/.agents/skills`，项目根 canonicalization。
- [x] 为非默认 `CLAUDE_CONFIG_DIR` 实现 Claude 用户 MCP capability probe；无法证明实际位置时返回 `unsupported`，不得回退猜写。
- [x] 实现 Claude/Codex TargetDescriptor 和 capability/policy/trust discovery。
- [x] 实现 JSON、TOML、Markdown 解析与 managed projection；TOML 使用 `toml_edit` 保留非受管表和注释。
- [x] 建立脱敏 fixture：Claude settings/用户 MCP/项目 MCP、Codex config/项目 config、提示词、Skills 链接。
- [x] 实现 full hash + managed hash、外部漂移分类和只允许非受管变化自动合并。
- [x] 实现 PreviewPlan、稳定 warning/error code、DB row version 与 preview 持久化。
- [x] 实现 Git tracked/check-ignore 状态读取；本阶段不写 `.git/info/exclude`。

关键测试：

- [x] 未安装、缺失文件、空文件、无权限、损坏 JSON/TOML。
- [x] 非受管字段改变不冲突；受管字段改变返回 conflict。
- [x] Codex untrusted 项目、Claude policy blocked。
- [x] 预览 JSON 中搜索所有 fixture secrets 均无命中。
- [x] 默认及非默认 `CLAUDE_CONFIG_DIR`/`CODEX_HOME` fixture 命中目标矩阵，Codex Skills 不随 `CODEX_HOME` 迁移。

验收映射：AC1、AC9、AC10、AC13。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
pnpm test --run
```

回滚点：只读 discovery/preview 可用，仍无外部写入 RPC。

## Phase 3：Apply、原子写、快照与恢复

- [x] 实现 Tauri 单实例入口、进程内 Apply/Restore mutex、全库单活动写入部分唯一索引、SQLite 条件事务原子认领 preview token、重复/并发消费错误与 `STALE_PREVIEW` 校验。
- [x] 实现 snapshot journal、权限/类型/link target 记录。
- [x] 实现同目录临时文件、flush/fsync、atomic rename 和临时 symlink rename。
- [x] 实现多目标顺序 apply、写后解析验证、逆序 rollback、`ROLLBACK_FAILED`。
- [x] 实现崩溃遗留 applying/restoring run 检测和写入阻断。
- [x] 实现 restore 前二次快照和恢复预览。
- [x] 实现 `.git/info/exclude` 应用标记区块的显式、幂等写入；绝不修改 `.gitignore`。

关键测试：

- [x] 在第 N 个目标注入失败，前 N-1 个目标恢复且无半写文件。
- [x] 同一 preview 的两个并发 apply 只有一个能认领；两个不同 preview 并发时也只有一个活动写入，失败方不产生外部修改。
- [x] 在 atomic rename 前后及第 N 个目标注入崩溃；重启后阻止新写入并能按 journal 生成恢复计划。
- [x] preview 后外部改动导致 apply 拒绝。
- [x] 普通目录、未知 symlink、中央库外 link target 不被删除。
- [x] tracked 文件只警告；untracked exclude 仅确认后写入。

验收映射：AC9、AC10、AC11、AC12。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

回滚点：通过 tempfile fixtures 验证完整写入与恢复，但尚未默认指向用户真实配置。

## Phase 4：Provider 与全局提示词纵向功能

- [x] 实现 Provider CRUD、每工具单 active、跨工具复制与字段重新校验。
- [x] Claude Adapter 只管理 `$CLAUDE_CONFIG_DIR/settings.json` 选定 env keys。
- [x] Codex Adapter 只管理 model/provider table，并以脱敏方式处理 `experimental_bearer_token`。
- [x] 实现首次 Provider discovery/import preview。
- [x] 实现 Prompt CRUD、首次无损导入、`$CLAUDE_CONFIG_DIR/CLAUDE.md` 与 `$CODEX_HOME/AGENTS.md` 应用。
- [x] 检测 Codex `AGENTS.override.md` 遮蔽并提示新会话生效。
- [x] 完成 Claude/Codex 渠道与提示词页面、表单、遮罩输入和统一预览。

验收映射：AC2、AC3。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
pnpm test --run
```

回滚点：Provider/Prompt 可单独恢复最近快照。

## Phase 5：MCP 中央列表与全局/项目分配

- [ ] 实现 MCP 结构化验证、stdio/streamable_http 表单与扩展字段保留。
- [ ] 实现中央 CRUD、名称唯一和敏感 header/env 脱敏。
- [ ] 实现 Claude `$HOME/.claude.json`、`.mcp.json` 的受管条目合并；非默认 Claude 配置根必须先通过 capability probe。
- [ ] 实现 Codex `$CODEX_HOME/config.toml` 与项目 `[mcp_servers.*]` 合并并保留其他 TOML 表。
- [ ] 实现全局与项目 assignment：全局项在项目中只读继承、不保存重复 assignment，项目只能追加其他项；外部同名冲突阻断。
- [ ] 更新/重命名/删除仅清理由 managed_items 基线确认的旧条目。
- [ ] 完成 MCP 中央列表、详情、目标状态和项目选择器。

验收映射：AC4、AC6、AC7、AC10。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
pnpm test --run
```

回滚点：MCP 所有写入均有逐目标 snapshot；不影响 Provider/Prompt owned projection。

## Phase 6：Skills 中央库与全局/项目链接

- [ ] 实现本地目录选择、`SKILL.md`/frontmatter 校验、staging copy、hash、原子入库。
- [ ] 防止循环/逃逸 symlink、特殊文件和来源目录被修改。
- [ ] 实现中央 Skill CRUD；存在 assignment 时禁止直接删除。
- [ ] 实现 Claude `$CLAUDE_CONFIG_DIR/skills`、Codex `$HOME/.agents/skills` 及两者项目目标的 symlink plan/apply/verify。
- [ ] 实现普通目录占位、未知链接、断链、外部同名及 policy blocked 状态。
- [ ] 实现全局继承显示、项目不可禁用/重复选择。
- [ ] 完成 Skills 列表、内容预览、本地导入、目标状态与项目选择器。

验收映射：AC5、AC6、AC7、AC8。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
pnpm test --run
```

回滚点：只删除应用可证明拥有的 symlink/中央目录；来源目录永不删除。

## Phase 7：项目、总览与首次接管流程

- [ ] 实现项目登记、规范化去重、Git/Codex trust/Claude policy 状态。
- [ ] 完成项目列表与详情双工具组合页，持续显示全局继承与 blocked/conflict。
- [ ] 完成首次启动向导：检测 → 选择导入/接管 → 预览 → 应用；支持跳过工具。
- [ ] 完成总览卡片、最近同步、冲突入口和唯一下一步的空状态。
- [ ] 统一 ChangePreviewDialog、SyncStatusBadge、BlockingState、SnapshotRestoreDialog。
- [ ] 验证键盘导航、焦点管理、表单标签、对话框语义和颜色非唯一状态表达。

验收映射：AC1、AC6、AC9、AC12、AC13。

验证：

```bash
pnpm lint
pnpm typecheck
pnpm test --run
cargo test --manifest-path src-tauri/Cargo.toml
```

回滚点：UI 聚合层可回退而不改变已有 Adapter/数据库契约。

## Phase 8：端到端质量门与 macOS 打包

- [ ] 使用隔离 HOME/CODEX_HOME/CLAUDE_CONFIG_DIR fixture 运行完整链路，不触碰开发者真实配置。
- [ ] 覆盖中央变更 → 预览 → 应用 → 原生验证 → 外部漂移 → 恢复。
- [ ] 运行 secret audit：日志、RPC error、preview JSON、测试快照索引不得泄露 fixture secret。
- [ ] 运行 destructive-path audit：所有删除目标必须 canonicalize 且证明属于中央库/managed item。
- [ ] 运行格式往返测试：Claude JSON 未知字段、Codex TOML 注释/未知表、Markdown 原文。
- [ ] 构建 macOS `.app` 与 DMG，执行首次启动、文件选择、权限失败和恢复人工 smoke test。
- [ ] 核对 Out of Scope：无市场、云同步、代理服务、跨平台、钥匙串、项目全局禁用。

验收映射：AC14；并汇总 AC1–AC13 的自动测试与 macOS smoke 证据。

最终验证：

```bash
pnpm lint
pnpm typecheck
pnpm test --run
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

最终人工检查：

- [ ] AC1–AC14 全部映射到自动测试或记录明确的 macOS smoke 证据。
- [ ] 使用真实 Claude/Codex 安装版本做只读 discovery 与用户确认后的隔离样本写入测试。
- [ ] `git status` 中没有测试污染、真实密钥、构建产物或用户配置。
- [ ] 从最新快照恢复后，原始 fixture 与恢复目标 hash 一致。

## 高风险点与停止条件

- Claude `~/.claude.json` 结构与 fixture 不匹配：停止写入该目标，补 capability/fixture，不猜字段。
- Codex 不接受 `experimental_bearer_token`：标记 Provider unsupported，不改写 shell profile，不擅自引入 helper。
- 目标路径 canonicalize 后超出预期根、入口为未知 symlink 或权限异常：阻止操作。
- 受管字段漂移、preview 过期、Codex untrusted、Claude policy blocked：不得提供强制覆盖快捷路径。
- rollback 失败：停止后续写入，保留全部快照和 journal，优先恢复能力。
