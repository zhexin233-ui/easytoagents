# 全局 Skills 检测与复制导入

## 1. 适用范围

修改全局 Skills 来源、入口链接、批量导入、导入 RPC/DTO、证据表或首次状态提示时，必须遵守本规范。中央库和原生同步的一般约束仍见 [Quality Guidelines](./quality-guidelines.md)。

导入只创建私有中央副本，不自动创建 assignment、managed target/item、同步历史或原生链接。原文件、目录、链接文本和权限不变；导入成功不能声称原安装已受管。之后用户主动同步时，同名外部安装仍受原有冲突保护。

## 2. 命令与存储签名

```rust
discover_skill_import(state: State<'_, AppState>, tool: Tool)
    -> Result<SkillImportPreviewDto, AppError>;
confirm_skill_import(state: State<'_, AppState>, input: ConfirmSkillImportInput)
    -> Result<SkillImportResultDto, AppError>;
```

领域实现位于 `src-tauri/src/skills/import.rs`；命令只取得显式环境、私有路径和数据库锁，不读取进程环境或执行技能脚本。Rust/Specta 是类型唯一来源：

```ts
commands.discoverSkillImport(tool);
commands.confirmSkillImport({ previewId, candidateIds });
```

迁移 `0006_skill_import_previews.sql` 新建表：

| 列                           | 约束 / 用途                                                                                        |
| ---------------------------- | -------------------------------------------------------------------------------------------------- |
| `id`                         | 小写 UUID 文本主键                                                                                 |
| `tool`                       | `claude` / `codex`                                                                                 |
| `context_json`               | 合法 object JSON；版本、来源环境指纹、全部中央 Skill ID/row_version 指纹、候选来源链/目录身份/hash |
| `redacted_preview_json`      | 合法 object JSON；安全的展示 DTO                                                                   |
| `status`                     | `previewed` / `consumed`，默认 `previewed`                                                         |
| `created_at` / `consumed_at` | 创建时间默认由 SQLite 生成；消费时更新后者                                                         |

保留 `(status, created_at)` 索引。表不存正文或任意 frontmatter；确认用 `WHERE status = 'previewed'` 条件消费，并与全部 Skill 插入共同提交。不得修改历史迁移、借用 MCP 接管表或自动降级真实数据库。

## 3. 跨层合同

### 显式来源与内置排除

| 工具   | 导入来源                                                               | 正式同步目标                 |
| ------ | ---------------------------------------------------------------------- | ---------------------------- |
| Claude | `environment.claude_config_dir()/skills`                               | 同一 Claude 来源根           |
| Codex  | `environment.home()/.agents/skills`、`environment.codex_home()/skills` | 始终为 `HOME/.agents/skills` |

默认兼容来源为 `HOME/.codex/skills`。`CLAUDE_CONFIG_DIR` / `CODEX_HOME` 由启动时的 `ExplicitEnvironment` 提供；前端不得传任意路径，服务不得重新读环境。能力不支持或策略非 Allowed 时，不读取候选内容。

- 只枚举来源根直属目录/目录链接，不递归搜索集合，不主动扫描插件缓存。
- Codex 两个已知来源根的 `.system` 词法路径及可证明的真实目录树都排除。**排除不依赖当前扫描工具**：Claude 链接指向 Codex 内置树也必须排除。
- `.system` 集合不需要有 `SKILL.md`。`resolve_skill_source` 只验证目录/入口身份；不要为了解析内置集合而调用读取正文的 inspect。
- 在读取候选正文前排除内置；确认开始、每项复制前后、事务重验和提交前重新计算排除集合。`.system` 在检测后或复制期间变成链接，不能使已选用户候选绕过范围检查。
- 最多 256 个候选入口、每次确认 32 个合并候选、一次检测或确认累计读取/复制 128 MiB。目录枚举和单技能还受 `library.rs` 的文件数/深度/大小限制。预算包含复制及多次重验，不是允许原始文件总量达到 128 MiB；超限不得声称完整成功。

### 来源链接与中央副本

普通单目录 `prepare_skill_import` 仍拒绝根链接。全局发现使用专门入口证据：来源根、直属入口、每段链接文本/身份、真实目录身份及祖先身份/权限。有限解析最多 32 跳；逐段 no-follow，拒绝循环、不安全祖先、广泛根和未知应用私有目录。

后续读取绑定已打开目录描述符；复制前后重验入口链和完整树 hash。祖先只绑定身份/权限，不把无关兄弟目录新建造成的 size/nlink 变化误判为技能漂移。内部资源链接仍只允许安全的树内普通文件链接；目录链接、逃逸、硬链接、特殊文件等拒绝。不得只 `canonicalize` 一次后按路径盲读。

中央已有项只有精确 name/hash、记录状态及磁盘副本均有效才识别为 `already_imported`。已指向中央目录的原生链接必须匹配已知记录的直接私有子目录，不能重新复制中央目录到自身。

### DTO 与选择

- 预览：`previewId: string | null`、`tool`、`sources`、`candidates`、`message`。无可新增候选时没有确认令牌。
- 来源：`kind`（`claude_global` / `codex_agents` / `codex_compatibility`）、`path`、`status`（`ready` / `missing` / `empty` / `unavailable`）、`diagnosticCode`、`message`。一个来源失败不能隐藏另一个来源结果。
- 候选：`candidateId`、`name`、`description`、`sourcePaths`、`status`、`reason`、`existingSkillId`。状态为 `importable` / `already_imported` / `name_conflict` / `invalid`。
- 确认输入仅 `previewId` 与 1–32 个非重复 `candidateIds`。路径、名称、hash、工具均从私有证据读取，不信任客户端重建值。
- 结果仅 `tool`、`createdCount`；不返回暗示自动分配的数量。同目录或精确 name/hash 相同来源合并并保留来源路径；同名不同内容和 NOCASE 名称碰撞不可选，不覆盖或自动改名。

### 事务与清理

确认顺序：重验令牌/来源/中央指纹 → 准备所选 staging → 单个 SQLite IMMEDIATE 事务 → 重验 → 排他 finalize 与事务内插入全部记录 → 重验来源/正式副本 → 消费令牌 → commit。拒绝 `applying/restoring/rollback_failed` writer。使用 `insert_skill_in_transaction`，不循环调用各自提交的旧单项接口。

finalize 使用原子不覆盖 rename（macOS `RENAME_EXCL`，Linux `RENAME_NOREPLACE`）。确定未提交时仅清理本次可证明 ID、直接父目录、目录身份和完整 hash 的副本；被替换或改变的目录必须保留。提交结果不确定时，重读令牌及整批中央记录/副本：已提交则保留并成功；确认未提交才清理；无法判定保留并报错。

SQLite 和文件系统没有跨资源原子事务。进程在 finalize 后、commit 前崩溃可能留下私有无记录副本；本流程不显示它们为已导入，不自动猜测清扫。

### 前端生命周期与首次状态

`SkillImportDialog` 复用共享模态焦点机制。查询键为 `['skill-import', tool, requestId]`；每次显式打开/重扫换 requestId，`retry:false`、`staleTime:Infinity`、`gcTime:0`，禁止 focus/reconnect 自动重扫。

默认不勾选，只有 importable 可选。确认同步上锁，禁关闭/取消/重扫/双提交。失败后必须新扫描；重扫清空旧选择和错误。成功仅失效 `skillKeys.all`，等待列表刷新再关闭并恢复焦点。若确认成功但列表刷新失败，明确显示已复制，不能再次提交旧令牌。不得隐式调用 assignment、同步 preview 或 Apply。

未分配 Skills 的初始诊断必须同时满足：通用状态 `external_non_owned_change`、两个 baseline hash 都空、existing managed items 和 desired assignments 都空、成功扫描为 `ObservedDocument::SymlinkDirectory`。

首次已分配但尚未写入原生目标时，允许增加 `SKILL_TARGET_INITIAL_SYNC_PENDING`，但必须同时满足：desired assignments 非空且中央副本全部 Ready、两个 baseline hash 都空、existing managed items 为空、assessment `can_merge`，并且 scan/status 精确为 `Missing/missing` 或成功的 `ObservedDocument::SymlinkDirectory/external_non_owned_change`。只生成过预览的空 target 行仍可满足；不得改写通用 drift status 或 Apply 判断。

| 证据                                                                    | 专用诊断 / 展示                                                 |
| ----------------------------------------------------------------------- | --------------------------------------------------------------- |
| 无 desired、目标缺失                                                    | 保留 missing / 待初始化                                         |
| 无 desired，满足未分配初始条件，目录无条目                              | `SKILL_TARGET_INITIAL_EMPTY` / 空目录，待配置                   |
| 无 desired，满足未分配初始条件，目录有条目                              | `SKILL_TARGET_INITIAL_UNMANAGED` / 未纳入同步管理               |
| 有 desired，满足首次待同步全部条件，目标缺失或有可合并的非受管目录内容 | `SKILL_TARGET_INITIAL_SYNC_PENDING` / 已分配，待预览并确认同步 |
| 完整/半基线、existing managed items、受管漂移、损坏或策略/权限错误      | 不覆盖原状态/诊断/阻断                                          |

仅生成过同步预览的 target 行不等于有 baseline。目录条目数不能当作合法技能数量，`.DS_Store` 等未知兄弟必须保留但不计为 Skill。共享展示 helper 必须同时匹配 status 与专用诊断；pending 只匹配 `missing` / `external_non_owned_change`，不能更改通用漂移算法、预览/Apply 冲突或 MCP 映射。分配成功反馈必须说明只更新中央意图、仍需显式预览和 Apply，且不得隐式调用二者。

## 4. 验证与错误矩阵

| 条件                                     | 必须行为                                                              |
| ---------------------------------------- | --------------------------------------------------------------------- |
| 来源缺失、空、不可读                     | 独立 source 状态；不能吞错成空结果                                    |
| 内置集合或其跨工具别名                   | 无候选、无令牌；显示 `SKILL_IMPORT_BUILTIN_EXCLUDED`（若有排除）      |
| 扫描候选/累计预算超限                    | `SKILL_IMPORT_SCAN_LIMIT` 或候选固定限额原因；不宣称完整成功          |
| 选择为空、重复、未知或超过 32            | `INVALID_INPUT`，无中央写入                                           |
| 令牌不存在 / 已消费                      | `NOT_FOUND` / `PREVIEW_ALREADY_CONSUMED`                              |
| 环境、中央指纹或确认前来源变化           | `STALE_PREVIEW`，重扫后再确认                                         |
| 事务内来源/身份/hash 变化或变为内置      | 固定安全错误（如 `CONFLICT` / `INVALID_INPUT`），整批回滚，保留原来源 |
| 活动 writer                              | `WRITE_IN_PROGRESS`，不消费令牌                                       |
| 第二项复制、rename、SQL 或已知提交前失败 | 无部分中央记录，清理仅限可证明的本次副本                              |
| 提交结果不确定                           | 核验整批，不能盲删已提交副本或猜测成功                                |
| 有 desired、无 baseline/item、目标缺失或仅含不同名外部条目 | `SKILL_TARGET_INITIAL_SYNC_PENDING`，预览仍按真实 assessment 生成 |
| 半基线、managed item 漂移、同名外部项、中央损坏或策略/权限错误 | 保留真实阻断；不能显示首次待同步                                |

## 5. Good / Base / Bad 示例

- Good：只有 `.codex/skills` 中一个管理器链接，检测出用户技能；勾选后中央新增一项，原链接/文件/权限及所有分配、管理基线不变。
- Base：只有 `.system` 或候选均已导入，显示原因，没有可确认令牌，不复制、不分配。
- Bad：Claude 链接绕到 Codex `.system`、来源在确认中变化或第二项 SQL 失败，不能导入内置或留下可见半批次。
- Good：中央 Skill 已分配、Claude 目录只有 `.DS_Store`、Codex 目标缺失时，两卡片显示“已分配，待同步”；用户显式 Preview/Apply 后创建两个受管链接并保留 `.DS_Store`。
- Bad：仅看到 desired assignment 就覆盖半基线、同名外部目录或中央副本损坏的真实诊断，或分配成功后自动 Apply。

## 6. 必需测试与断言

使用临时 home/config/data 和 fixture 技能；不向真实用户目录确认导入或 Apply。

- `skills::import`：默认/自定义来源、缺失正式目标、绝对/相对链接、去重/冲突、中央复用、非法路径与限额；检测无 staging，普通 DTO/持久化证据不含 fixture 正文和私有 frontmatter。
- 内置回归同时遍历 Claude/Codex：真实 `.system`、集合链接和真实目标别名；集合无 `SKILL.md` 仍排除；确认前、copy、SQL 阶段新建内置别名时整批拒绝且 token 未消费。
- 确认回归：选择子集、空/重复/未知 ID、来源/中央 stale、活动 writer、两个独立 DB 连接竞争、第二项故障、提交不确定与回滚；原文件 inode/权限/链接文本及全部 assignment/managed/sync 表不变。
- library：排他 rename 不覆盖既有目录；清理保留被替换/更改的目录；原单目录根链接仍拒绝。
- DB：v5→v6 保留已有数据，重复打开幂等，绑定生成检查通过。
- service + UI：初始/空/仅元文件/缺失目标/无基线 target 行/半基线/desired/managed item 漂移/同名普通目录与外部或断裂链接/中央损坏/策略权限类型错误/完整基线后真实非受管变化矩阵；首次双工具 Apply 后 InSync 且原兄弟保留，MCP 回归不变。
- `skills-page.test.tsx`：选择和精确 payload、局部失败、失效重扫、晚到响应、双提交与关闭锁、Tab/Escape/焦点、复制后刷新失败，断言没有 assignment/Apply。
- 分配 UI：断言中央列表与目标状态一起刷新，成功文案说明仍需显式同步；仅 `missing` 或 `external_non_owned_change` 与 pending 诊断组合覆盖徽标，其它组合不覆盖；分配/取消分配均不调用 Preview/Apply。
- 浏览器 fixture 只证明实际组件的交互/布局；不能替代真实 Tauri 或真实安装验收。真实桌面未跑必须明确记载。

## 7. 错误与正确做法

错误：按当前扫描工具排除内置，允许 Claude 别名绕过。

```rust
if tool == Tool::Codex {
    excluded.push(environment.codex_home().join("skills/.system"));
}
```

正确：同一排除边界服务两工具，在各确认检查点重新解析内置目录身份。

```rust
let excluded = builtin_exclusions(environment);
let evidence = library::resolve_skill_source_excluding(root, entry, &excluded)?;
// 确认各阶段调用 validate_sources，再按持久化证据复制所选项。
```

错误：把“复制成功”当成“已同步”，给来源工具自动 assignment 或刷新受管 baseline。

正确：只提交中央 Skill 记录和导入令牌；前端提示原安装未变、尚未分配或同步。

错误：为避免“非受管变更”看起来像故障，只要 `desired` 非空就把目标显示为待同步。

```rust
if !desired.is_empty() {
    diagnostic = Some("SKILL_TARGET_INITIAL_SYNC_PENDING");
}
```

正确：复用完整的中央校验、baseline、managed items、scan 与 assessment 证据，只为可安全合并的首次目标增加展示诊断；通用状态与 Apply 安全边界不变。

```rust
let pending = baseline.full_hash.is_none()
    && baseline.managed_hash.is_none()
    && existing.is_empty()
    && !desired.is_empty()
    && assessment.can_merge
    && matches!(
        (assessment.status, &scan),
        (SyncStatus::Missing, TargetScan::Missing)
            | (SyncStatus::ExternalNonOwnedChange, TargetScan::Observed(_))
    );
```
