# 技术设计：项目原生资源发现、禁用与恢复

## 1. 问题与边界

当前系统有两套相邻但不同的概念：

1. 中央资源及 assignment 表达“应用希望同步什么”。
2. managed_targets / managed_items 表达“应用已经证明自己拥有目标中的哪些部分”。

项目原本自带的资源在被发现时既不是中央资源，也不属于应用 ownership。新功能必须增加第三个概念：项目原生资源观察与可恢复状态。它只记录来源身份、观察证据和禁用恢复材料，不自动创建中央资源、assignment 或 managed item。

## 2. 支持矩阵与逻辑资源单位

| 平台 | MCP | Skill | Prompt |
| --- | --- | --- | --- |
| Claude | .mcp.json 中单个 mcpServers 条目 | .claude/skills 的直属入口 | 整个 CLAUDE.md |
| Codex | .codex/config.toml 中单个 mcp_servers 条目 | .codex/skills 的直属入口 | 整个 AGENTS.md |
| Cursor | .cursor/mcp.json 中单个 mcpServers 条目 | .cursor/skills 的直属入口 | 不支持 |

适配器 descriptor 仍是路径、格式、能力与敏感 selector 的唯一来源。前端和服务层不得另写路径映射。

## 3. 数据流

发现：

项目登记/重新扫描
→ adapter descriptor
→ 原生目标 scan
→ 逐项安全投影与 hash
→ 与 managed_items 对照分类
→ project_native_resources 对账
→ 脱敏 DTO
→ 项目详情“项目原生资源”分区

写入：

用户选择单项动作
→ 服务端生成并持久化 PreviewPlan 与私有证据
→ 用户审阅
→ Apply claim + writer lock
→ 重扫、hash/CAS 重验
→ 创建快照
→ 原子禁用或恢复
→ 写后重扫验证
→ 单事务更新状态、baseline 与 run
→ 刷新项目、原生资源、中央资源和恢复查询

失败：

任一步失败
→ 按 journal 与 snapshot 逆序回滚
→ 回滚成功：run failed，资源状态不变
→ 回滚无法证明安全：run rollback_failed，保留快照并阻止新 writer

## 4. 持久化模型

追加数据库迁移 v12，新建 project_native_resources，建议字段：

| 字段 | 用途 |
| --- | --- |
| id | UUID，前端动作只引用此稳定 ID |
| target_id | 关联 managed_targets，复用规范化目标身份与 row version；项目通过目标行关联 |
| external_key | MCP 名称、Skill 入口名或 Prompt 固定文档键 |
| entry_type | mcp_entry、directory、symlink、prompt_file |
| state | active、disabled、missing、conflict |
| observed_item_hash | 最近一次可信观察的条目/树/文件 hash |
| disabled_snapshot_id | 禁用成功后关联的私有 snapshot |
| disabled_at | 禁用时间 |
| last_seen_at | 最近一次扫描时间 |
| row_version | 资源级 CAS |

唯一约束为 target_id + external_key。目标层的 tool、artifact_kind、scope、project_id 与 target_path 仍由 managed_targets 唯一约束，不在多层重复定义。

managed_targets 在这里仅作为规范化“目标身份行”复用，不代表 ownership。逐项发现允许通过现有 target identity upsert 建立一条 baseline_full_hash、baseline_managed_hash、baseline_projection_json 均为空且没有 managed_items 的项目目标行；只有 managed_items 与有效 baseline 才能证明 ownership。PRD 中“只读识别”指不改写项目原生内容，允许在应用私有数据库中持久化观察状态。

约束：

- disabled 必须有 disabled_snapshot_id；active/missing 不得引用禁用快照。
- 被 project_native_resources 引用的 snapshot 不允许删除。
- 项目存在 disabled 资源时，soft_remove_project 返回 CONFLICT。
- managed_items 对同一 target_id + external_key 仍有有效 ownership 证据时，该项分类为中央托管，不得同时作为可禁用原生项返回。
- 名称改变按旧项 missing + 新项 active 处理；MVP 不猜测重命名。

迁移和 repository 测试必须证明：空 baseline/无 managed item 的目标身份行不会被同步或展示层解释为 ownership，也不会放宽普通 Apply。

## 5. 后端领域边界

新增 projects/native_resources 模块，负责：

- 从项目 descriptor 枚举受支持目标。
- 将 ObservedDocument 转成逐项私有观察。
- 复用 Skill library 对直属入口做完整树/链接身份检查。
- 对照 managed_items 分类原生与托管项。
- 对账 project_native_resources，但不在只读扫描中建立 ownership。
- 生成脱敏 ProjectNativeResourceDto。

不要把原生发现塞入 MCP import 或 Skill global import；它们的来源、scope、接管资格与产品语义不同。可以抽取共享的私有解析/hash helper，但不能放宽全局导入合同。

### 5.1 DTO

新增生成类型：

- ProjectNativeResourceKind：mcp、skill、prompt。
- ProjectNativeResourceState：active、disabled、missing、conflict。
- ProjectNativeEntryType：mcp_entry、directory、symlink、prompt_file。
- ProjectNativeResourceDto：id、projectId、tool、artifactKind、displayName、targetPath、entryType、state、rowVersion、canDisable、canRestore、diagnosticCodes、safeSummary。
- ProjectNativeResourceSummaryDto：各状态计数，挂到 ProjectDto 供登记反馈和项目卡展示。
- ProjectNativeResourceQueryInput：projectId、tool、artifactKind。
- PreviewProjectNativeResourceActionInput：resourceId、rowVersion、action。

MCP safeSummary 只允许传输类型等固定非敏感信息；不得携带 command、args、env、headers、URL 查询或任意原始配置。

### 5.2 命令

- list_project_native_resources(input)：重新扫描并返回当前安全列表。
- preview_project_native_resource_action(input)：只接受资源 ID、row version 与 disable/restore 枚举；服务端持久化路径、hash、snapshot 及 descriptor 证据。
- apply_project_native_resource_preview(preview_id)：复用 sync writer、claim、journal、snapshot、回滚和 run DTO。

即使用户偏好 direct，前端也不能自动调用第三个命令。项目原生资源动作与 Skill takeover 一样始终需要显式审阅。

动作状态矩阵：

| 当前状态 | disable | restore |
| --- | --- | --- |
| active | 允许生成预览 | INVALID_INPUT |
| disabled | INVALID_INPUT | 允许生成预览 |
| missing | CONFLICT | CONFLICT |
| conflict | CONFLICT | CONFLICT，直到重新扫描证明占用已解除并回到 disabled |
| 中央托管或中央漂移 | NOT_FOUND / CONFLICT | NOT_FOUND / CONFLICT |

Preview 与 Apply 都重新检查该矩阵；不能只信任客户端按钮状态。

## 6. 分类与对账

每次登记、get_project、rescan 和 list_project_native_resources 都使用同一领域函数：

1. 从适配器获取 Project descriptor；不支持组合直接跳过。
2. 扫描当前目标并得到逐项私有 hash。
3. 读取目标的 managed_items：
   - 外部键与 item hash 仍匹配：中央托管项，不进入原生列表。
   - 外部键属于 managed item 但 hash 漂移：保留中央漂移诊断，不伪装成原生项。
   - 无 ownership 证据：项目原生项。
4. 新原生项 upsert 为 active。
5. active 记录未再出现时标记 missing；它没有禁用快照，因此不可恢复。
6. disabled 记录只要快照有效且目标键仍缺失，就始终保持 disabled 并可恢复，即使整个父目标文件/目录已被外部删除；恢复可以在 descriptor 允许范围内重建缺失目标。
7. disabled 记录的同键重新出现时标记 conflict，但保留原快照；后续扫描证明占用解除后回到 disabled。

登记响应仅携带 summary，完整列表由详情页专用查询获取，避免项目列表批量返回资源细节。

## 7. 资源操作

### 7.1 MCP 单项

禁用：

1. 重验 target 与目标 selector 的私有 item hash。
2. 对整个配置文件建立 payload_file 快照。
3. clone 当前 JSON/TOML 文档，仅删除目标 selector。
4. 原子替换文件并验证目标 selector 缺失、未知兄弟不变。
5. 记录 disabled、snapshot ID 与写后 target baseline。

恢复：

1. 从私有快照解析原始目标 selector，不把内容写入 DTO 或 journal。
2. 扫描当前文件；同名 selector 存在即 CONFLICT。
3. 对当前文件创建回滚快照。
4. 将原始 selector 合并进当前文档并原子替换。
5. 验证恢复 item hash，与快照证据一致后置 active。

恢复允许无关兄弟在禁用期间变化；只要目标 selector 仍为空且当前文档可安全解析，就不要求 whole-file hash 完全相等。

MCP 的“未知兄弟保持不变”是语义值合同，不要求目标文件整体字节相等。JSON/TOML renderer 只允许改变目标 selector；TOML 目标之外的注释必须保留，键顺序和目标附近的格式化可由 toml_edit 规范化。

### 7.2 Skill 单项

禁用：

- 入口是外部 symlink：保存链接文本与 no-follow 身份，移除链接自身，不触碰目标。
- 入口是普通目录：使用完整 tree digest 创建 directory_tree snapshot，重验后通过新增的受证据约束 mutation 移除目标树。
- 目录链接、逃逸、硬链接、特殊文件、内置集合或超限树拒绝操作。

恢复：

- 原路径必须不存在，否则 CONFLICT。
- symlink 按原始链接文本创建临时链接并排他 rename。
- directory_tree 复制到同父临时目录，验证完整 hash 后排他 rename。
- 写后重验入口类型、身份和 tree hash。

Skill 恢复基线包括：所有普通文件字节与 mode、相对路径、内部安全链接文本，以及根入口为外部 symlink 时的原始链接文本。目录自身的时间戳不属于内容合同；完整 tree hash 必须覆盖上述可验证内容。

### 7.3 Prompt 整文件

禁用：

- 只允许 CLAUDE.md / AGENTS.md descriptor 的 exact path。
- 建立 payload_file 快照，原子移除整文件，保留原字节与 mode。

恢复：

- 目标必须不存在；若外部重新创建则 CONFLICT。
- 从私有快照原子恢复原字节与 mode。
- Markdown 不做自动合并。

Prompt 恢复基线为 exact file bytes 与 Unix mode；时间戳不作为验收条件。

## 8. 写入状态与全局阻断

- preview 可并发生成，但 Apply、restore、项目移除和 snapshot 删除都必须查询 active writer。
- applying、restoring 或 rollback_failed 任一存在时，新原生资源写入返回 WRITE_IN_PROGRESS。
- 项目原生资源 Apply 的每种资源类型都必须走同一 finish_failed_apply；不能在 MCP/Skill/Prompt 分支各自吞错。
- rollback_failed 时项目详情保留资源与恢复材料，显示全局阻断提示；普通恢复入口不得声称可继续写。
- detect_interrupted_run 仍是启动恢复入口，journal 缺失时不推断成功。

## 9. 前端体验

在现有“资源类型 × 平台”视图中保留“中央追加”，其上新增“项目原生资源”分区：

- active：显示“项目原生 · 已启用”和“临时禁用”按钮。
- disabled：显示“项目原生 · 已禁用”、禁用时间和“恢复”按钮。
- missing：说明资源已被外部移除，无可恢复材料时禁用操作。
- conflict：说明生效位置被重新占用或外部变化，保留恢复材料并要求用户先处理冲突。
- rollback_failed / active writer：显示全局阻断提示，不把它降级成普通资源冲突。

交互规则：

- 禁用/恢复均打开 ChangePreviewDialog；direct 偏好不绕过。
- mutation pending 时按钮、关闭、重复提交和切换受现有模态锁约束。
- 成功后共同失效 project、project-native-resources、MCP、Skill、Prompt 与 recovery 查询。
- 错误展示稳定 code 和可行动文案，不展示私有配置。
- 分区、按钮、状态使用 heading、aria-pressed、aria-live 和焦点恢复的既有模式。

## 10. 兼容、迁移与恢复

- v12 迁移只新增表、索引、trigger 和 snapshot 引用关系，不回写历史项目文件。
- 旧数据库升级后首次扫描懒创建 active 观察记录；迁移本身不读取项目路径。
- 迁移继续走 Database::open 的预迁移备份和 IMMEDIATE transaction。
- 软删除项目重新登记会复用原 project ID；但仍有 disabled 资源时不允许先软删除。
- 被引用 snapshot 在恢复完成前不可删除；恢复完成后可按普通历史快照策略保留或删除。
- 现有 commands/overview.rs → sync::delete_snapshots 显式删除路径必须查询 project_native_resources 引用；任何后续 retention/cleanup 入口也必须复用同一 repository guard，不允许旁路。
- 中断 run 继续由 detect_interrupted_run 处理；没有 journal 或无法证明状态时不得猜测成功。

## 11. 取舍

采用专用 project_native_resources，而非扩展 managed_items：

- 优点：不伪造中央 resource_id/ownership，来源和状态清晰，禁用记录可跨重启恢复。
- 代价：增加一张表和对账逻辑。

采用应用私有 snapshot，而非在项目内创建隐藏 disabled 目录：

- 优点：不会污染仓库，也不会被工具误扫描；MCP 私密内容仍在应用私有存储。
- 代价：恢复依赖当前应用数据；因此项目移除和 snapshot 删除必须在禁用期间受阻。

恢复 MCP 采用 selector 合并，而非整文件回滚：

- 优点：保留禁用期间的无关外部变化。
- 代价：同名位置重建时必须显式冲突，不能自动覆盖。

## 12. 风险与回滚

- 最大风险是把外部内容误判为 ownership。所有分类必须同时检查 managed item 身份与 hash。
- 最大数据风险是 Skill 目录删除和含秘密 MCP 快照。前者必须完整树校验，后者必须保持私有且全链路脱敏。
- 若实现阶段无法安全复用 sync apply，可回滚到“只读发现与展示”阶段；不得发布直接文件写入的半实现。
- 数据库迁移失败由启动备份恢复；产品代码回滚时保留新增表无害，不能修改历史迁移。
