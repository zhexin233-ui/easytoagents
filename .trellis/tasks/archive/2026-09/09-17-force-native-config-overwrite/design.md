# 技术设计：原生配置变更直接强制覆盖

前置阅读：`research/current-behavior.md`、`.trellis/spec/backend/quality-guidelines.md`、`.trellis/spec/backend/error-handling.md`、`.trellis/spec/frontend/quality-guidelines.md`、`.trellis/spec/frontend/hook-guidelines.md`。

## 1. 边界与核心决策

- 所有应用拥有的原生写入统一采用直接执行语义，删除 `preview_confirm` / `direct` 产品模式和二次确认 UI。
- 用户的主操作仍是授权边界：中央配置增删改/分配、点击项目资源禁用或恢复、选择 Skill 条目后点击接管。应用不得在没有这些主操作时主动改写任意原生文件。
- 保留持久化 Preview、一次性 claim、行版本、磁盘 hash、单写者、快照、journal、回滚和恢复内核；UI 不再展示 Preview，但 Preview 继续作为事务证据。
- “强制覆盖”只适用于可安全观察、目标类型符合 descriptor、且 ownership 明确的受管内容。解析、权限、策略、信任、危险路径、类型变化、恢复身份等错误继续 fail closed。
- 中央 mutation 与被动外部变化采用不同默认：中央 mutation 自动覆盖；被动扫描只报告并提供“采纳原生 / 中央覆盖”两个应用内动作。
- Provider、Prompt、MCP、Skill、Hook、Agent 使用同一“有效意图变化即同步”结果合同，不能再由各页面根据 `directApply` 和不完整 DTO 决定是否同步。
- 不拆父子任务：强制覆盖、受影响 scope 协议、六类资源触发、生成 bindings 和前端生命周期必须在同一兼容单元内落地；拆分会制造不可验证的中间协议状态。

## 2. 统一数据流

```text
用户主操作
  → 后端提交中央意图并返回受影响 SyncScope[] / 选择原生动作
  → scope 去重、稳定排序、串行执行
  → 每个 scope 生成持久化 Preview（descriptor + ownership + row versions + observed hashes）
  → 分类：no-op / 可覆盖 / 硬阻断
  → 可覆盖：原子 claim → 写前快照/journal → mutation → 校验 → finalize
  → STALE_PREVIEW：当前 scope 重新生成一次并重试
  → 硬阻断或第二次 stale：记录该 scope 错误，继续完成其余独立 scope
  → invalidate 查询并通知结果
```

原始 mutation 必须等待所有 scope 尝试结束后再报告最终结果；成功、部分失败和完全失败应区分。每条写入链都消费精确 `previewId`，不得增加绕过 `apply_persisted_preview` 的文件写入口。

## 3. 受影响范围合同

### 3.1 共享类型与所有者

后端定义生成绑定的稳定值类型（命名可按现有模块风格调整）：

```text
SyncScopeDto {
  artifactKind: provider | prompt | mcp | skill | hook | agent
  tool: Tool
  projectId: string | null
}
```

- 中央 mutation 的结果返回 `affectedSyncScopes`；scope 由服务层在变更事务中根据变更前后 selection/assignment 与待清理 managed state 的并集计算。
- 结果稳定去重并排序；前端只负责调用 artifact 对应的 typed preview/apply command，不再自行从 `globalTools` / `globalAssignments` 猜测项目范围。
- assignment mutation 的 scope 由输入天然确定，但仍走同一结果字段，避免页面形成第二套规则。
- 未分配 create/import 返回空集合；删除被引用约束拒绝时 mutation 整体失败且不返回可执行 scope。
- 允许删除 active Provider/Prompt 时，后端在删除前捕获其 tool scope 并随成功结果返回，用于清理旧投影。

### 3.2 六类资源的范围规则

| 资源/操作 | 受影响范围 |
| --- | --- |
| Provider 创建 active、编辑 active、切换 active、删除 active | 对应 Provider tool 全局 scope；Pi 等多 Provider 工具的任一参与档案变化均返回其 tool scope |
| Prompt 编辑/删除已分配档案、当前选择启停/替换 | 档案变更前后 `globalTools` 并集或精确目标 tool |
| MCP/Hook/Agent 通用字段编辑、启停 | 全部全局 assignment 与显式 project assignment scope |
| Agent 工具特有设置 | 至少返回对应工具的全局/项目 scopes；通用字段返回全部 scopes |
| Hook assignment event 或 matcher/command 等有效内容变化 | 受影响 assignment scopes；多算的 scope 允许生成 no-op Preview，但不得写无关文件 |
| Skill assignment / takeover | 精确全局或项目 scope；中央内容经既有 symlink 已即时生效时可返回空/no-op |
| 任意全局/项目 assignment | 精确 `(artifact, tool, projectId|null)` |

全局继承项不等于项目文件 ownership：若项目只有继承而没有显式 assignment，不为其创建或改写项目文件。

### 3.3 前端执行器

- 将现有 `requestPreview(): void` 改造成可等待的统一执行器，输入 `SyncScopeDto[]`，按 scope 串行 Preview → Apply。
- 空 Preview 是成功 no-op；可覆盖 Preview 立即 Apply；硬阻断记录结构化错误；stale 仅重建当前 scope 一次。
- mutation 的查询失效在中央提交后执行，原生 scope 全部完成后再发最终通知；部分失败通知必须列出失败 tool/project 并提供应用内重试，不要求打开文件夹。
- Provider、Prompt、MCP、Skill、Hook、Agent 页面及项目 assignment 子组件不得再保留私有 `if (directApply)` 触发分支。

## 4. 被动扫描与双向处理

### 4.1 扫描触发与统一状态

- 不新增常驻 watcher。应用在资源页面进入、窗口重新获得焦点、`environment-ready` 后以及显式“重新扫描”时刷新相关目标状态。
- 将 Provider、Prompt、MCP、Skill、Hook、Agent 的全局与项目目标统一到现场 `scan_target → assess_drift` 结果；不得让 MCP/Hooks 继续只读过期 `last_status`，也不得让 Provider/Prompt 状态完全缺失 drift。
- 焦点恢复扫描需要节流/去重；仅失效当前可见资源查询，不遍历未打开的所有项目目录。
- 被动扫描不得写原生文件、中央实体或 baseline；发现 `ExternalOwnedChange` 后返回可操作状态。

### 4.2 外部变化计划

新增持久化 `ExternalChangePlan`，或在现有 persisted Preview envelope 上增加等价 action evidence。计划至少绑定：

```text
artifact kind + tool + projectId + target descriptor/path
ownership + observed full/managed hash + redacted diff
matched central resource/assignment/managed-item identities
all participating row versions
canAdoptNative + adoptBlockedReason
canOverwriteCentral + overwriteBlockedReason
```

- 状态卡先请求该计划，再展示动作；不能只凭 status DTO 直接写入。
- 用户点击“采纳原生更改”或“以中央配置覆盖”即构成授权，不再弹第二层确认。
- 动作消费前重新核对 observed hash、path/type、row versions 与能力/策略；变化则返回 stale，刷新计划一次，不猜测执行。
- “以中央配置覆盖”转换/复用普通 persisted Preview → Apply；写前快照和回滚合同不变。
- 仅更新 baseline 的 readopt 不再作为产品动作；内部若仍需 baseline helper，也不得暴露为“采纳”。

### 4.3 六类资源的采纳映射

| 资源 | 可直接采纳条件 | 应用内匹配/导入条件 |
| --- | --- | --- |
| Provider | 稳定 provider id，或唯一中央/原生候选；复用 `adopt_provider_native` | 多候选、官方登录/不可导入 auth、工具归属不明 |
| Prompt | 唯一 active profile，正文可解析；Cursor 只采纳剥离 frontmatter 后正文 | 无唯一 active、空/非法正文、override/fallback 遮蔽 |
| MCP | managed item 的 resource id/external key 精确配对，native transport 可无损映射 | 重命名/新增条目、未知 transport、disabled/unsupported 字段、同名歧义 |
| Skill | 已管理名称精确配对，完整 source tree 可安全解析、复制和 hash 复核 | name 变化、路径逃逸、失效链接、未知/超限树 |
| Hook | managed item/assignment 唯一配对，event/matcher/command/timeout 与脚本可表示 | 匿名项歧义、复杂 shell、事件或 assignment 含义不明确、脚本不可安全复制 |
| Agent | managed path/resource id 唯一配对，Markdown/Codex 字段可无损映射 | 文件重命名、同 stem 冲突、未知 dropped fields、enabled/assignment 意义不明 |

直接采纳时，中央实体、tool settings/assignment（仅计划明确绑定的字段）、managed item 与 target baseline 在一致的事务边界中更新。Skill tree/Hook script 等文件副本先进入私有 staging，hash 验证后提交；DB 失败清理 staging。采纳不得自动新建/删除中央实体、修改 active/assignment 或重命名 identity，除非用户在匹配/导入界面显式选择。

### 4.4 非模态 UI

- 在各资源状态卡/项目状态卡显示“检测到外部修改”、脱敏摘要和两个主动作：“采纳原生更改”“以中央配置覆盖”。
- `canAdoptNative=false` 时仍提供“在应用内匹配/导入”，并显示不可直接采纳原因；不得显示“去文件夹处理”。
- 成功后失效中央列表、状态与项目查询并重新扫描，目标应回到 `in_sync`。
- 部分失败保持状态卡和重试入口；中央 mutation 的自动覆盖不经过此 UI。

## 5. 后端漂移与 Preview 合同

### 5.1 将“可覆盖漂移”与“硬阻断”分离

当前 `can_merge=false` 同时承载受管内容漂移和真实安全错误。实现时应让 `DriftAssessment`（或等价的共享分类）明确表达：

- `InSync`、`Missing`、`ExternalNonOwnedChange`：沿用现有可 Apply 语义；
- `ExternalOwnedChange`：当 scan 是合法 `ObservedTarget` 且 descriptor/ownership 可写时，生成非 conflict 的 update/delete Preview，并保留诊断为 warning/状态信息；
- managed-item baseline mismatch：保留 mismatch 名单，同时携带完整 observation 和 hashes，允许对已观察的受管条目强制更新；
- parse、permission、policy、trust、unsupported、unsafe path、target type changed、读取失败：继续生成 error/conflict，不可 Apply。

不得全局删除 `ChangeKind::Conflict`、`CONFLICT` 或 `review_conflict`，因为它们仍服务于路径身份、重复 claim、恢复占用、危险链接等真实冲突。

### 5.2 保留 Preview 后并发保护

- Preview 必须绑定生成时的 full/managed hash、descriptor、ownership 与所有行版本。
- Apply 前继续调用现有 hash/row-version/路径复核；任何 Preview 后变化返回 `STALE_PREVIEW` 或对应稳定错误，且零原生写入。
- 前端/共享生命周期最多自动重新 Preview 一次；第二次仍 stale 时停止并通知，避免无限重试或覆盖未观察到的新内容。

### 5.3 所有权语义

- `Selectors`：基于最新可解析文档渲染，只覆盖受管 selector，保留非受管字段和表。
- `WholeDocument`：中央目标覆盖整份受管文档。
- `SymlinkNames`：只操作用户已选择/已分配的受管名称；未知兄弟保持不变。

## 6. Skill 首次接管

- 保留检测、条目选择和 `prepareSkillTakeover` 的证据收集，但准备成功后立即 Apply，不再打开 `ChangePreviewDialog`。
- 对普通目录和外部链接复用 `Mutation::TakeoverSymlink`：原入口先进入应用拥有的 `.takeover` 隔离位置，再安装中央链接。
- 普通目录继续创建 `directory_tree` 快照；外部 symlink 只替换入口，绝不跟随修改或删除外部目标。
- Apply 前重新验证 entry type、fingerprint、目录树 hash、中央路径和行版本；变化则 stale/冲突并停止。
- 用户未选择的条目、未知兄弟、危险链接、特殊文件和超限目录不自动接管。

## 7. 项目原生资源禁用/恢复

- 点击“禁用/恢复”后调用现有 `previewProjectNativeResourceAction`，收到非空可应用计划后立即调用 `applyProjectNativeResourcePreview`。
- 删除 `PROJECT_NATIVE_RESOURCE_REQUIRES_CONFIRMATION` 警告及“始终需要确认预览”文案，但保留专用 evidence、rowVersion 和 observed hash。
- 恢复目标若被未知入口占用、身份/类型已变化或不满足现有安全证明，继续阻断并显示错误；本变更不授权删除未知占用项。

## 8. 设置、RPC 与前端清理

- 从 `AppSettingsDto`、`UpdateAppSettingsInput`、Rust `ApplyMode`、生成 bindings 和设置页删除 apply mode；`enabledTools` 等无关设置保持不变。
- 不需要数据库破坏性迁移；旧 `app_settings.apply_mode` 值保留为未读取历史数据，回滚旧版本时仍可读取。
- 将 `useSyncPreviewFlow` 收敛为始终执行 Preview → Apply 的共享生命周期；空计划保持 no-op，硬阻断直接通知，stale 有界重建一次。
- 为六类中央 mutation 的返回 DTO 加入后端计算的 `affectedSyncScopes`，或使用等价的统一 mutation result wrapper；生成 bindings 后由共享执行器消费。
- 项目原生资源与 Skill takeover 可复用同一个“消费持久化 Preview”helper，但保留各自 typed command 和 query invalidation。
- 删除 `ChangePreviewDialog` 及旧 `openPreview`、readopt mutation、`readoptAvailable` 和 readopt 产品入口；Provider adopt-native 后端逻辑重构为统一 ExternalChangePlan 的 Provider 采纳执行器，不随旧弹窗删除。
- 增加六类资源的现场状态刷新与外部变化计划/动作 bindings；应用窗口 focus 恢复时对当前页面查询做节流失效。
- 删除“确认原生配置变更”“请先处理冲突”“内容不同需先处理差异”“直接应用（跳过预览确认对话框）”等失效文案；真实错误继续使用 notification 或就地诊断。
- 历史快照恢复流程保持独立，不因删除常规同步确认而移除。

## 9. 兼容性与迁移

- 这是有意的产品行为破坏性变更：升级后旧 `preview_confirm` 用户也会直接执行原生写入。
- RPC/DTO 删除要求同一提交重新生成 Specta bindings；前后端必须作为一个版本发布。
- 不新增 schema migration，避免为了删除偏好键改写数据库；代码不再读取旧键即可。
- 既有未消费 Preview 仍受原状态、过期、hash 和行版本校验；部署后不得把旧 conflict Preview 直接强行消费，应由新版本重新 Preview。

## 10. 测试策略

- Rust 漂移矩阵：受管内容漂移与条目 baseline mismatch 生成可 Apply Preview；parse/permission/policy/trust/type/path 仍阻断。
- Rust 并发矩阵：Preview 后文件/行版本变化零写入并 stale；一次重建成功；重复 claim 仍只有一个成功。
- Rust ownership：selector 保留未知字段，whole-document 覆盖，Skill 只替换选中入口。
- Rust Skill takeover：普通目录完整快照/恢复、外部链接目标不变、未知兄弟不变、失败逆序回滚。
- Rust 项目资源：禁用/恢复不再产生 confirmation warning；未知占用和身份变化仍阻断。
- React 页面：各资源中央变更均 Preview → Apply 且从不渲染确认对话框；错误只通知一次；空计划不 Apply。
- Rust/React 触发矩阵：active Provider 编辑立即同步；Prompt 当前选择；MCP/Hook/Agent 的 global-only、project-only 与混合 assignment 编辑/启停；所有全局/项目 assignment；未分配 create/import no-op。
- 范围协议：后端 mutation 返回变更前后 scopes 并集，稳定去重排序；全局继承不制造项目写入，工具特有字段不改写无关工具文件。
- Hooks 对称性：picker 添加、事件切换、分组移除与项目 assignment 都走相同执行器。
- 多 scope：逐个消费精确 Preview，单 scope 失败不并发污染其他 scope，最终结果区分部分失败并可从应用内重试。
- 被动扫描：页面进入、窗口恢复焦点、environment-ready 与显式重扫能发现六类资源漂移；后台静置不轮询、不写任一侧。
- ExternalChangePlan：绑定 observation/hash/row versions，stale 时零写入；按钮动作无二次确认。
- 六类采纳：直接可映射路径更新中央+baseline 后 in-sync；重命名/新增/匿名/未知字段进入应用内匹配导入，不猜测、不伪装 readopt。
- Skill tree/Hook script 采纳覆盖 staging、hash 竞态、DB 失败清理；Provider secret、MCP headers/env、Agent dropped fields 保持脱敏和 fail closed。
- React 特殊流程：Skill 选择后直接接管，项目资源点击后直接 Apply；断言精确 `previewId`、query invalidation 和失败反馈。
- 设置与 bindings：apply mode 类型、开关、文案和测试全部删除，旧后端键不影响 DTO。

## 11. 回滚

- 代码可作为一个兼容单元回滚；旧 `apply_mode` 数据未删除，旧版本仍能恢复此前设置语义。
- 已发生的强制覆盖通过写前 snapshot 和历史恢复流程回退；不得通过代码回滚猜测还原用户文件。
- 若上线后仅某一特殊流程风险过高，应恢复该流程的主操作阻断，而不是绕过/拆除共享 Preview、快照或回滚内核。
