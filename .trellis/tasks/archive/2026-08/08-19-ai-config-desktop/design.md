# Claude 与 Codex 配置管理桌面端：技术设计

## 1. 设计目标

本应用是 macOS 本地配置控制面：SQLite 和应用私有中央 Skill 库记录用户意图，Claude/Codex 原生文件与目录是可预览、可恢复的同步目标。

核心不变量：

1. 前端不直接读写外部路径；所有路径解析、格式解析、合并、快照和写入都由 Rust 后端完成。
2. 只修改应用明确拥有的字段或链接；非受管内容语义保持不变。
3. Preview 与 Apply 使用同一份计划和基线；目标变化后旧预览失效。
4. 受管字段发生外部变化时默认保留外部版本并阻止覆盖。
5. 项目配置只追加全局配置；应用自身不生成会遮蔽全局项的同名项目项。
6. 任一修改可通过写入前快照恢复；未知普通目录、文件或链接绝不递归删除。

## 2. 技术栈

### 2.1 选择

- 桌面壳：Tauri 2。
- 前端：React 19、TypeScript、Vite、React Router、TanStack Query。
- UI：Tailwind CSS + shadcn/ui/Radix primitives；表格、表单、对话框和 diff 状态使用可访问的组合组件。
- 前端测试：Vitest + React Testing Library。
- 后端：Rust stable、Tauri commands、Serde。
- 数据库：SQLite（`rusqlite` bundled），WAL、外键、显式迁移。
- 配置解析：`serde_json` 处理 JSON，`toml_edit` 处理 TOML 并尽量保留注释，Markdown 按原始文本处理。
- hash：SHA-256；ID：UUID。
- Git 检测：优先调用系统 `git` 的只读命令，避免为 MVP 引入完整 libgit2。
- 跨层类型：Rust DTO 为唯一 RPC 合同，通过 Specta/Tauri Specta 生成 TypeScript 类型；若兼容性阻塞，退回受测试保护的手工 DTO，不允许组件自行断言 payload。
- 包管理：前端统一使用 pnpm。

### 2.2 取舍

- 选择 Tauri 而非 Electron：需要安全的本地文件操作、较小包体和 Rust 原子写；Electron 调试更成熟但资源和权限面更大。
- 选择 SQLite 而非 JSON 数据库：本任务包含关系、唯一约束、迁移、同步历史和多目标事务状态。
- Skills 只用符号链接，不提供 Copy 模式：符合仅当前 Mac 生效的范围，并避免副本漂移。
- 不引入通用三方合并编辑器：MVP 只自动合并确定无冲突的非受管字段，受管冲突直接阻断。
- 不启动后台文件监听：启动、页面刷新、Preview 和 Apply 前重新扫描；FSEvents 后续再做。

## 3. 工程边界

```text
src/
├── app/                 # router、providers、shell
├── features/
│   ├── dashboard/
│   ├── providers/
│   ├── prompts/
│   ├── mcp/
│   ├── skills/
│   ├── projects/
│   └── sync/
├── components/          # 共享 UI 组合件
├── lib/                 # RPC client、query keys、格式化与脱敏展示
└── bindings/            # Rust 生成的 DTO 类型

src-tauri/src/
├── app/                 # 初始化、路径、状态容器
├── domain/              # 领域类型、枚举、不变量
├── db/                  # migrations、repositories、transactions
├── adapters/
│   ├── claude/
│   └── codex/
├── sync/                # discover、preview、apply、rollback、drift
├── skills/              # 中央库导入、hash、symlink
├── git/                 # tracked/exclude 检测
├── commands/            # 窄 Tauri RPC
├── security/            # redaction、权限、路径校验
└── error.rs              # 稳定错误码
```

领域层不知道 React、Tauri command 或具体文件路径；Adapter 知道工具格式但不自行执行任意 I/O；Sync Engine 是唯一外部写入编排者。

## 4. 数据模型

所有主表使用 UUID 文本主键、`created_at`、`updated_at` 和递增 `row_version`。`tool` 枚举仅允许 `claude|codex`。

### 4.1 核心实体

- `provider_profiles`
  - `tool`, `name`, `api_base_url`, `api_key`, `default_model`, `config_json`, `is_active`。
  - 唯一 `(tool, name)`；部分唯一索引保证每个工具最多一个 active。
- `prompt_profiles`
  - `tool`, `name`, `body`, `is_active`, `imported_from_path`。
  - 唯一 `(tool, name)`；每工具最多一个 active。
- `mcp_servers`
  - `name` 全局唯一，`transport`、`command`、`args_json`、`url`、`headers_json`、`env_json`、`extra_json`、`enabled`。
  - 已知字段结构化；`extra_json` 仅保存无法统一但已验证的扩展字段。
- `skills`
  - `name`、`source_path`、`central_path`、`content_hash`、`frontmatter_json`、`status`。
  - 中央目录由应用拥有，原来源只做溯源，不作为同步目标。
- `projects`
  - `display_name`、规范化绝对 `root_path`、`is_git_repo`、`codex_trust_status`、`last_scanned_at`。
  - `root_path` 唯一。

### 4.2 分配表

使用显式表保持外键完整性，不用无法建立资源外键的多态 `resource_type/resource_id`：

- `mcp_global_assignments(tool, mcp_id)`
- `skill_global_assignments(tool, skill_id)`
- `mcp_project_assignments(project_id, tool, mcp_id)`
- `skill_project_assignments(project_id, tool, skill_id)`

项目写入前由领域服务拒绝：

- 为已全局分配给同一工具的资源创建项目 assignment；此时 UI 仅显示只读的“全局继承”，数据库不保存重复项目 assignment；
- 与全局项或外部项目项同名但不是同一受管资源；
- 会触发工具原生遮蔽规则的组合。

`config_json`、`headers_json`、`env_json` 和 `extra_json` 都按“可能含秘密”处理。Schema 对已知秘密字段声明敏感 selector；导入扩展字段还按键名与值形态检测 `authorization|api[_-]?key|token|secret|password|cookie` 等秘密。原值只能进入受限数据库、必要的原生目标和私有快照，不能进入同步 journal、RPC、日志或崩溃上下文。

### 4.3 同步与基线

- `managed_targets`
  - `tool`, `artifact_kind`, `scope`, `project_id`, `target_path`、`baseline_full_hash`、`baseline_managed_hash`、`baseline_projection_json`、`last_status`。
  - 唯一 `(tool, artifact_kind, scope, project_id, target_path)`。
- `managed_items`
  - `target_id`, `resource_kind`, `resource_id`, `external_key`, `last_applied_item_hash`。
  - 支持安全重命名和只删除仍匹配基线的旧项。
- `sync_runs`
  - `kind=preview|apply|restore`, `status`, `scope`, `project_id`, `db_version`, `started_at`, `finished_at`, `error_code`。
  - 部分唯一索引保证全库最多一个 `status in (applying, restoring)` 的活动写入；崩溃遗留 run 会继续占用该约束，直到显式恢复或判定完成。
- `sync_items`
  - `run_id`, `target_id`, `change_kind`, `status`, `redacted_diff_json`, `warning_codes`。
- `snapshots`
  - `run_id`, `target_path`, `snapshot_path`, `content_hash`, `file_mode`, `target_type`, `link_target`。
  - 内容保存为应用私有文件，不把大文件 blob 放进 SQLite。

## 5. Tool Adapter 合同

```rust
trait ToolAdapter {
    fn tool(&self) -> Tool;
    fn discover(&self, ctx: &DiscoveryContext) -> Result<Vec<TargetDescriptor>>;
    fn parse(&self, target: &TargetDescriptor, raw: ObservedRaw) -> Result<ObservedDocument>;
    fn project_managed(&self, desired: &DesiredState, target: &TargetDescriptor)
        -> Result<ManagedProjection>;
    fn plan(&self, input: PlanInput) -> Result<TargetPlan>;
    fn render(&self, input: RenderInput) -> Result<RenderedTarget>;
    fn verify(&self, rendered: &RenderedTarget, observed: ObservedRaw) -> Result<()>;
}
```

每个 `TargetDescriptor` 声明 canonical path、格式、作用域、受管 selector、敏感字段 selector、trust/policy 要求和 symlink policy。Adapter 只生成语义计划与渲染结果；文件锁、快照、临时文件、fsync、rename 和回滚集中在 Sync Engine。

## 6. 原生目标矩阵

| 资源 | Claude | Codex |
|---|---|---|
| Provider | `$CLAUDE_CONFIG_DIR/settings.json` 的受管 `env` keys | `$CODEX_HOME/config.toml` 的 `model`、`model_provider`、受管 `model_providers.<id>` |
| 全局提示词 | `$CLAUDE_CONFIG_DIR/CLAUDE.md` | `$CODEX_HOME/AGENTS.md`；检测同目录 `AGENTS.override.md` |
| 全局 MCP | `$HOME/.claude.json` 的受管 MCP 条目 | `$CODEX_HOME/config.toml` 的 `[mcp_servers.*]` |
| 项目 MCP | `<project>/.mcp.json` | `<project>/.codex/config.toml`，仅 trusted |
| 全局 Skills | `$CLAUDE_CONFIG_DIR/skills/<name>` symlink | `$HOME/.agents/skills/<name>` symlink |
| 项目 Skills | `<project>/.claude/skills/<name>` symlink | `<project>/.agents/skills/<name>` symlink |

`CLAUDE_CONFIG_DIR` 默认 `$HOME/.claude`，`CODEX_HOME` 默认 `$HOME/.codex`。`CLAUDE_CONFIG_DIR` 用于 Claude settings、全局提示词和用户 Skills；官方文档仍单独把用户 MCP 指向 `$HOME/.claude.json`。若设置非默认 `CLAUDE_CONFIG_DIR`，Adapter 必须通过安装版本 capability probe 确认用户 MCP 实际目标，无法确认时把该目标标为 `unsupported` 并禁止写入。Codex 用户 Skills 按官方规则固定从 `$HOME/.agents/skills` 加载，不随 `CODEX_HOME` 变化。

Provider 永不写项目配置。Codex 项目 trust 从用户配置的 `projects.<path>.trust_level` 读取；untrusted 或 unknown 时预览为 blocked，不伪造信任设置。

Claude 若存在 `strictPluginOnlyCustomization` 等策略禁止 user/project MCP 或 Skills，对应目标为 `policy_blocked`。

## 7. Provider 与提示词细节

### 7.1 Claude Provider

- 只拥有当前激活档案声明的 `env` keys，例如 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`/`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_MODEL` 和默认模型变量。
- 激活新档案时删除上一档案独有的受管 keys，保留其余 `env` 和 settings 字段。
- `CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST` 等宿主管理状态存在时显示 blocked，不声称切换成功。

### 7.2 Codex Provider

- 为应用档案生成稳定且合法的 provider id；激活时更新 `model`、`model_provider` 和该受管 provider table。
- 按用户决策把明文 key 写入 `experimental_bearer_token`；UI 固定显示官方“不推荐”提示，日志和 diff 使用掩码。
- 不覆盖内置保留 provider id，不修改 `mcp_servers`、`plugins`、权限、hooks 等非受管表。

### 7.3 提示词

- Markdown 目标写入的是档案正文，不添加应用 marker，所有权和基线保存在 SQLite。
- 目标受外部修改后进入 managed drift；用户必须先“采用外部内容为新档案”或“恢复中央档案”，后者仍需新预览。
- 切换后提示“新会话生效”；不尝试修改已运行会话的上下文。

## 8. Skills 中央库

中央库位于 macOS Application Support 的应用私有目录，例如：

```text
~/Library/Application Support/EasyToAgents/skills/<skill-id>/
```

导入流程：

1. `lstat/canonicalize` 来源并拒绝不可读、循环链接或缺少 `SKILL.md` 的目录。
2. 复制到中央库 staging 目录，不移动或修改来源。
3. 校验 frontmatter、计算目录内容 hash、限制异常文件类型和逃逸链接。
4. 原子重命名 staging 为正式 `<skill-id>`。
5. 写入 SQLite；如数据库失败则删除本次 staging/正式目录。

应用分配时只创建指向 canonical central path 的 symlink。现有普通目录、非应用链接或指向中央库外的链接均为冲突。移除 Skill 前必须先移除所有分配；只删除已验证位于中央库内且属于该记录的目录。

## 9. Preview / Apply / Rollback

### 9.1 Discovery 与漂移

每次启动、手动刷新、Preview 和 Apply 前读取目标。保存两种 hash：

- `full_hash`：检测任意外部变化；
- `managed_hash`：判断应用拥有的字段/链接是否变化。

状态：`in_sync`、`external_non_owned_change`、`external_owned_change`、`missing`、`parse_error`、`permission_denied`、`policy_blocked`、`untrusted`、`target_type_changed`、`failed`。

仅非受管变化可自动字段级合并；受管变化、格式解析失败和目标类型变化阻止 Apply。

### 9.2 Preview

`preview_sync` 是纯读操作：

1. 重新读取 DB row versions 与目标状态；
2. 生成逐目标 `add/update/delete/unchanged/warning/conflict`；
3. 对敏感字段脱敏；
4. 把基线与计划保存为 `sync_run(status=previewed)`；
5. 返回 `preview_id`、目标分组、警告和 Git 状态。

### 9.3 Apply

`apply_sync(preview_id)`：

1. Tauri 单实例入口与进程内 Apply/Restore mutex 串行化常规请求；SQLite 活动写入唯一约束兜底多实例/异常入口。条件事务仅允许一次把 run 从 `previewed` 原子更新为 `applying`：同一预览重复请求返回 `PREVIEW_ALREADY_CONSUMED`，另一活动 run 存在时返回 `WRITE_IN_PROGRESS`。
2. 校验 DB row versions 未变、目标 full/managed hash 未变；否则把 run 标为 stale 并返回 `STALE_PREVIEW`。
3. 写 durable run journal，并为所有将修改目标创建快照。
4. 文件目标使用同目录临时文件、继承/设置权限、flush、fsync、atomic rename；链接使用临时 symlink + rename，保证单目标不出现截断内容。
5. 每个目标写后重新解析并验证语义。
6. 任一失败时逆序恢复已成功目标；恢复失败则标记 `ROLLBACK_FAILED` 并保留全部快照。
7. 全部成功后在一个 SQLite 事务中更新 managed baselines、items 和 run 状态。

多目标 Apply 不宣称跨文件原子性。应用在每个目标写前、写后持久化 journal 阶段；崩溃注入测试覆盖 rename 前后及第 N 个目标。下次启动扫描 `applying/restoring` run，先阻止新的写入，再依据 journal 与当前 hash 生成恢复计划，不自动猜测覆盖。

### 9.4 Snapshot

- 目录权限 `0700`，快照文件 `0600`；记录原 mode、目标类型和 symlink target。
- 应用私有数据库及其 WAL/SHM、staging 和 journal 文件同样限制为当前用户访问；启动时权限不满足则修复到更严格权限或阻止写入。
- 快照恢复前再创建当前状态快照，避免恢复操作本身不可逆。
- UI 只显示时间、目标和状态，不展示可能含密钥的原文。

## 10. Git 与项目本地性

- 使用 `git rev-parse`、`git ls-files --error-unmatch` 和 `git check-ignore` 判断仓库、tracked 与 exclude。
- tracked 目标显示强警告，`.git/info/exclude` 不会掩盖 tracked 修改。
- untracked 新目标只有在预览勾选后，才幂等追加应用标记区块到 `.git/info/exclude`；不修改 `.gitignore`。
- 应用移除自己最后一个项目目标时，只移除自己创建且仍匹配的 exclude 规则，不清理用户规则。

## 11. RPC 与错误合同

Tauri command 返回生成的 DTO；错误统一为：

```ts
type AppError = {
  code: string;
  message: string;
  details?: Record<string, unknown>; // 已脱敏
  recoverable: boolean;
  action?: "rescan" | "review_conflict" | "restore" | "fix_permissions";
};
```

`details` 采用错误码对应的字段 allowlist，经过敏感 selector 与已登记秘密值双重替换后才可序列化；panic/crash hook 不附带领域 DTO 或原始文件片段。`sync_runs/sync_items` 只持久化路径、hash、状态、错误码和 `redacted_diff_json`。

核心 RPC：

- discovery：`scan_environment`、`scan_tool`、`scan_project`；
- CRUD：Provider、Prompt、MCP、Skill、Project；
- assignments：`set_global_assignment`、`set_project_assignment`；
- sync：`preview_sync`、`apply_sync`、`get_sync_run`；
- recovery：`list_snapshots`、`preview_restore`、`restore_snapshot`。

CRUD 只修改中央意图；所有外部写入必须经过 Preview/Apply RPC。

稳定错误码至少包括：`NOT_FOUND`、`INVALID_INPUT`、`PARSE_ERROR`、`PERMISSION_DENIED`、`POLICY_BLOCKED`、`UNTRUSTED_PROJECT`、`CONFLICT`、`STALE_PREVIEW`、`PREVIEW_ALREADY_CONSUMED`、`WRITE_IN_PROGRESS`、`ATOMIC_WRITE_FAILED`、`ROLLBACK_FAILED`、`SECRET_REDACTED`。

## 12. 前端信息架构

```text
总览
Claude ─ 渠道 / 全局提示词
Codex  ─ 渠道 / 全局提示词
MCP    ─ 中央列表 / 编辑 / 全局目标状态
Skills ─ 中央列表 / 本地导入 / 预览 / 全局目标状态
项目   ─ 项目列表 / 项目详情（Claude 与 Codex 追加项）
```

统一 `ChangePreviewDialog` 按目标展示 diff、敏感字段遮罩、tracked 警告、exclude 选项和冲突。统一 `SyncStatusBadge`、`BlockingState`、`SnapshotRestoreDialog`，避免各页面自造状态语义。

首次启动采用可中断向导：检测 → 展示发现结果 → 用户选择导入/接管 → 预览；跳过的工具保持 unmanaged。

## 13. 兼容、迁移与回滚

- 数据库使用前向迁移；启动迁移前备份 SQLite 文件，迁移失败不启动写入功能。
- macOS 13+ 为 MVP 支持基线；路径解析遵循目标矩阵，尊重 `HOME`、`CLAUDE_CONFIG_DIR`、`CODEX_HOME`，禁止硬编码用户名；非默认 Claude 配置根的用户 MCP 位置未经 capability probe 证明时禁止写入。
- 工具配置结构不支持时显示 unsupported/parse error，不以空配置覆盖。
- 功能回滚以“停止使用 + 从最近快照恢复”为主；卸载应用不自动删除外部配置或中央 Skill 库。
- 版本升级若 Adapter owned projection 变化，必须使旧 preview 失效并要求重新扫描。

## 14. 风险与延后项

- Codex 明文 `experimental_bearer_token` 官方不推荐，未来迁移到环境变量或钥匙串 helper；MVP 明确提示风险。
- JSON 写回可保持语义和字段但不保证原字节格式；预览必须显示格式变化，TOML 使用 `toml_edit` 尽量保留注释。
- Claude 用户 MCP 的内部 JSON 结构需用真实 fixtures 和安装版本做兼容测试，不能整份 `~/.claude.json` 重写。
- 工具版本更新可能改变路径或字段；Adapter capability probe 与 fixture 回归是发布门槛。
- 不实现市场、云同步、启动器、代理服务、跨平台、Copy 模式或项目级全局禁用。
