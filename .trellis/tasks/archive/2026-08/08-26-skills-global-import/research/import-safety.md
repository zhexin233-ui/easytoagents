# 全局导入安全研究

## 事实：可复用的目录扫描与复制安全边界

- `prepare_skill_import`（`src-tauri/src/skills/library.rs:126-174`）目前把校验、扫描、复制、二次扫描和 staging hash 校验绑在一起；其中只读扫描的核心是 `digest_tree_with_root_identity(source, None, ...)`（`library.rs:462-520`），而 `walk_directory` 在 `destination=None` 时只读目录并计算稳定 hash（`library.rs:523-645`）。它仍会拒绝特殊文件、硬链接，限制深度/数量，并对根目录做身份复核。
- `walk_directory` 的复制分支与只读分支共享同一套 `lstat_at`、`open_*_nofollow`、身份复核和 symlink 校验（`library.rs:564-645`）。因此批量复制可按“先只读扫描并收集每项，再逐项调用现有 staging/复制路径”的方式复用安全 helper；事实层面没有现成的批量 API。
- hash 对目录、文件、链接分别编码；文件 hash 还包含可执行位（`hash_record`/`hash_file_record`，`library.rs:1143-1159`）。`prepare_skill_import` 的“复制中 hash、来源二次 hash、staging hash”三者相等才继续（`library.rs:145-153`）。
- 根目录链接在 `validate_source_root` 的 `symlink_metadata` 处被实际拒绝（`library.rs:402-414`，错误原文：`Skill 来源必须是真实目录，不能是符号链接`）；随后 `canonical_source_directory` 仍要求 canonical 结果为真实目录（`library.rs:447-459`）。已有测试锚点：`source_root_symlink_and_missing_skill_md_are_rejected`（`library.rs:1427-1434`）。
- 内部链接安全由 `validate_source_symlink`（`library.rs:728-786`）和 `normalize_internal_link`（`library.rs:789-814`）保证：只接受相对目标、禁止 `..` 逃逸、用 fd + `O_NOFOLLOW` 逐段打开、末段必须是目录内普通非硬链接文件。复制时保留原始相对链接文本（`library.rs:628-639`）。

## 推断：根目录链接的最小扩展

若产品明确允许“来源入口本身是链接”，最小改动位置是 `validate_source_root`：对根入口先 `canonicalize`，记录 canonical 根的 `FileIdentity`，再把 canonical 路径交给现有 `canonical_source_directory`/`digest_tree_with_root_identity`。内部遍历仍必须从 canonical 根 fd 开始并保持 `O_NOFOLLOW`，不能把根链接解析逻辑扩展到内部链接；否则会改变现有“内部链接不得指向目录/外部路径”的边界。需要额外保留入口原始路径用于 `source_path` 展示，或明确数据库存 canonical 路径（这是产品取舍，当前实现会存 canonical 后的 `source_text`，`library.rs:131-132`）。

## 事实：名称、哈希与事务约束

- `skills.name` 是 `COLLATE NOCASE UNIQUE`；`source_path`、`central_path` 有路径 CHECK，`central_path` 另有 UNIQUE；`content_hash` 必须是 64 位小写 hex（`src-tauri/src/db/migrations/0001_initial.sql:105-140`）。插入通过 `insert_skill` 的 `TransactionBehavior::Immediate` 事务完成（`db/skills.rs:79-119`）。
- 写入错误把名称冲突映射为 `name`/“Skill 名称已存在（不区分大小写）”，中央路径冲突映射为 `centralPath`（`db/skills.rs:576-595`）。因此“同名+同内容 hash 复用”不能只依赖 hash：必须先按 NOCASE 精确名称查找，再比较 `content_hash`，并核验现有记录状态、中央目录存在且再次扫描 hash 相等；同名不同 hash 必须报告冲突。数据库当前没有 content_hash 唯一约束。
- `set_global_assignment` 和 `set_project_assignment` 都是 Immediate 事务并校验 row version（`db/skills.rs:233-292`, `296-377`）。全局 assignment 前若同工具同 skill 存在项目 assignment 会被 `validate_global_assignment` 拒绝（`db/skills.rs:254-264`; 规则 `domain/mod.rs:386-390`）；项目 assignment 若已全局继承也拒绝（`db/skills.rs:328-345`）。

## 事实/推断：外部目录接管与 preview/apply

- 当前导入是复制到中央库，且测试明确断言“不修改来源权限/内容”（`library.rs:1280-1355`）；数据库记录同时保存 `source_path` 与 `central_path`。因此“导入已有外部目录后不改原目录接管”在文件层面可行，但现有 assignment/sync 流程消费的是中央记录及其中央路径，不能自动把外部目录当成中央库所有权目标，除非新增明确的外部来源语义（推断）。
- 自动加入全局 assignment 会触发上述项目 assignment 冲突；需要在变更前检查每个工具的项目分配并向用户报告/要求清理，不能静默覆盖（事实）。
- preview/apply 是持久化、一次性消费事务：`apply_persisted_preview` 先 `claim_preview`（`sync/apply.rs:316-342`），`claim_preview` 只接受 `sync_runs.kind='preview' AND status='previewed'` 并原子更新为 applying（`sync/apply.rs:636-675`）；应用前还会拒绝冲突项并校验 descriptor/ownership/desired hash（`sync/apply.rs:715-760`）。因此 assignment 自动变化若发生在 preview 与 apply 之间，会按 row/version 或目标 envelope 变成 stale/conflict；不能认为已有 preview 仍然有效（推断）。

## 用户可理解的两种导入后行为（取舍）

1. **复制并托管（现有模型）**：外部目录保持不变，中央库得到副本，后续全局 assignment 指向中央副本。优点是现有安全扫描、preview/apply、删除保护都能复用；代价是用户需理解编辑外部原目录不会自动更新中央副本。
2. **链接来源并托管**：允许入口根链接并把 canonical 外部目录作为来源；同步前只读扫描并检测 hash/身份变化，必要时重新导入。优点是不产生副本语义歧义；代价是必须新增来源链接解析/身份与变更状态，且现有 central_path/原子复制清理不能直接证明外部目录所有权（推断）。

## 回归测试锚点

- `library.rs:1427-1434` 根目录链接拒绝；`library.rs:1280-1355` 复制不触碰来源、链接与 hash；内部链接边界覆盖 `library.rs:1400-1423` 附近的 broken/outside/loop/directory cases。
- 新增批量只读扫描应断言：不创建 staging、不改变来源、同一树 hash 稳定，并覆盖根入口链接的显式允许/拒绝策略。
- DB 复用应覆盖 NOCASE 同名同 hash、同名不同 hash、现有 central 目录/hash 不一致；以 `insert_skill`/`map_skill_write_error`（`db/skills.rs:79-119`, `576-595`）为锚点。
- assignment 组合覆盖全局与项目互斥（`db/skills.rs:254-264`, `328-345`）；preview 一次消费、stale/conflict 覆盖 `claim_preview` 与 `validate_preview_inputs`（`sync/apply.rs:636-675`, `715-760`）。

## 未覆盖/存疑

未查到现成“外部目录接管”或“批量 import”后端 API；当前 helper 只提供单目录 `prepare_skill_import`。上述根链接解析方式是最小改动推断，是否把原始入口还是 canonical 路径写入 `source_path` 需产品决定。

## 主代理点验与规划约束

- 已亲自核对 `library.rs:402-444`：入口目录链接被拒绝，`SKILL.md` 本身也不得为链接；不能因为新增全局检测就全局放松这些检查。
- 已亲自核对 `skills/service.rs:1533-1603` 的 `ordinary_directory_unknown_links_stale_preview_and_policy_block_never_overwrite`：中央技能被分配到 Claude 后，同名普通目录和指向中央库外的链接都产生 `Conflict`，且原条目保持不变。
- `skills/service.rs:599-610` 的 desired projection 固定指向 `record.central_path`。因此复制到中央库不等于接管现有全局安装，自动增加 assignment 会制造后续同名冲突。本次沿用原产品合同的“导入中央副本、保留来源”，不自动增加全局 assignment 或写 managed baseline；原安装迁移不在本次请求内。
- 本文“链接来源并托管”仅是探索备选，不是批准方案；它违背当前中央库所有权模型，不纳入本次实现。
- 若允许检测到的来源入口链接，必须记录并复核原入口/链接文本、解析链和最终目录身份，在确认及拷贝边界重新验证。单次 `canonicalize` 不足以保证检测到确认期间来源未变；内部链接、硬链接、特殊文件、深度和大小边界继续沿用现有 no-follow 规则。优先新增显式来源解析边界，而不是弱化所有本地导入调用。
- 精确名称与完整内容哈希相同、且中央目录经复核仍有效才可复用；仅 NOCASE 相同或哈希不同不能视作同一技能。现有 `skills` NOCASE 唯一约束保持不变。
- MCP 导入令牌的参考位置为 `src-tauri/src/mcp/import.rs:214`、`src-tauri/src/db/mcp_imports.rs:157-264`。它的字段接管语义不能直接复制给 Skill；可复用的是选择校验、重新验证和一次性确认的模式。
