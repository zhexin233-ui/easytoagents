# 当前行为与改动证据

## 前端状态机

- `src/features/sync/use-sync-preview-flow.ts:144-185` 仅在 `directApply && autoApply && canAutoApplyPreview(plan)` 时自动 Apply，其余情况打开 `ChangePreviewDialog`。
- `src/lib/settings-api.ts:17-25` 的 `canAutoApplyPreview` 拒绝任何 `changeKind === "conflict"` 或带 `errorCode` 的目标。
- `src/components/change-preview-dialog.tsx:61-207` 统一展示“确认原生配置变更”、冲突阻断、readopt/adopt-native 与“应用这份预览”。
- `src/features/settings/settings-dialog.tsx:161-175` 明确把 direct 描述为“无冲突时跳过确认”；设置 RPC 由 `src-tauri/src/settings.rs:14-146` 持久化，但后端同步逻辑不读取该模式。
- `src/features/projects/detail/page.tsx:115-154,433-445` 对项目原生资源禁用/恢复无条件打开确认 Preview。
- `src/features/skills/skills-page.tsx:655-665` 对 `prepareSkillTakeover` 返回的 Preview 无条件打开确认页。

## 后端阻断点

- `src-tauri/src/sync/mod.rs:355-465` 的 `assess_drift` 将受管内容漂移、条目 baseline mismatch、解析/权限/类型/策略/信任问题都标成不可合并，但这些原因必须在新行为中分流处理。
- `src-tauri/src/sync/mod.rs:648-701` 将不可合并目标转换为 `ChangeKind::Conflict`；`src-tauri/src/sync/apply/validate.rs:1-15` 再次拒绝所有 conflict/error Preview。
- MCP、Hooks、Skills 条目 baseline mismatch 分别在 `src-tauri/src/mcp/service.rs:894-941`、`src-tauri/src/hooks/service_native.rs:95-130`、`src-tauri/src/skills/service.rs:904-931` 丢失真实 observation；新设计必须保留当时磁盘 hash，不能跳过并发复核。
- `src-tauri/src/sync/apply/validate.rs:414-460` 已提供 Preview 后磁盘 hash 精确复核，应保留并允许一次有界重新预览。
- `src-tauri/src/sync/apply/core.rs:710-735,791-856,1052-1110` 的持久化 claim、快照、journal、回滚与单写者逻辑应继续作为唯一写入口。

## Skill 接管与项目原生资源

- `src-tauri/src/sync/apply/plan.rs:440-524`、`mutation.rs:669-771`、`finalize.rs:358-420` 已实现 Skill 普通目录/外部链接的证据绑定、隔离替换、目录树快照和安全清理，应复用而不是增加递归删除路径。
- `src-tauri/src/sync/apply/fs_ops.rs:147-166` 与 `mutation.rs:376-395` 会阻止普通同步直接覆盖目录/链接，因此首次 Skill 接管必须显式进入现有 takeover mutation。
- `src-tauri/src/projects/native_resources.rs:181-283` 是项目原生资源 Preview/Apply 业务链；`src-tauri/src/sync/mod.rs:53-55,720-722` 目前附加“需要确认”警告。
- 恢复入口被当前目标占用时仍应按 `src-tauri/src/projects/native_resources.rs:1144-1164` 与 `src-tauri/src/sync/apply/plan.rs:256-263` fail closed，不能由“直接执行”推导为无条件删除未知占用项。

## 现行规范冲突

- `.trellis/spec/backend/quality-guidelines.md:157-170,754-769,875-898` 当前规定受管漂移和 Skill 未知入口必须 conflict；实现完成后需要更新为“已观察且所有权合法时直接覆盖，真实安全错误继续阻断”。
- `.trellis/spec/frontend/quality-guidelines.md:631-720,754-787` 当前规定 direct 仅自动应用 clean Preview，并为 Skill takeover、项目原生资源保留硬确认例外；实现完成后需要整体改写。
- `.trellis/spec/backend/error-handling.md` 的稳定错误、恢复动作和敏感信息合同仍适用，不应因移除 UI 确认而放宽。

## 关键结论

1. 不能只删除弹窗；必须改变 Preview 对“安全可覆盖漂移”的分类，并让 Apply 继续验证 Preview 绑定的真实 observation。
2. Preview 是事务与恢复凭据，不是 UI 页面；移除确认不等于移除 Preview。
3. 三条用户流程可共享“生成精确 Preview → 立即消费 → STALE_PREVIEW 最多重建一次”的生命周期，但各自仍使用现有后端业务入口。
4. 任务高度共享同步内核、生成绑定与前端生命周期，拆成并行子任务会造成协议和测试中间态；因此保留为一个集成任务，按后端内核 → 特殊流程 → 绑定/前端 → 全量验证顺序实施。

## 全资源有效意图触发证据

### Provider 与 Prompt

- `src/features/tool-profiles/provider-panel.tsx:96-149` 的新增/编辑成功只刷新和通知；`175-187` 只有切换当前渠道才调用同步，因此编辑 active Provider 后必须切走再切回。
- Provider Preview 会在 `src-tauri/src/profiles/provider.rs:49-86` 重新读取 active profile；根因不是缓存，而是编辑成功后漏触发。Pi 的 desired 是全部相关渠道并集，不能只按 `isActive` 判断范围。
- Prompt 编辑/选择/删除只在 direct 模式同步：`src/features/prompts/prompts-page.tsx:83-121,141-175,202-233`；未分配新建不改变投影。

### MCP 与 Skills

- MCP 编辑、启停只同步 DTO 的 `globalTools`：`src/features/mcp/mcp-page.tsx:74-143`，遗漏 project-only assignments；`McpServerDto` 不提供项目 scopes。
- MCP 精确 desired 来自 `(tool, projectId|null)` 上启用的显式 assignment：`src-tauri/src/mcp/service.rs:520-617`；项目仅继承全局项时不应创建冗余项目文件。
- Skills 全局/项目 assignment 也只在 direct 模式同步：`src/features/skills/skills-page.tsx:157-185`、`src/features/projects/detail/assignments/skill.tsx:74-100`。
- Skill 链接投影只包含 `{name -> central_path}`：`src-tauri/src/skills/service.rs:854-868`。中央内容变化已通过链接即时可见时，Preview 可以 no-op；链接 assignment/takeover 变化仍需同步。

### Hooks 与 Agents

- Hooks 新增/编辑、启停、删除以及列表内移除分配都不自动同步：`src/features/hooks/hooks-page.tsx:108-222`；picker 添加/事件切换却仅在 direct 下同步：`:753-766`，路径明显非对称。
- Agents 中央编辑/启停/删除仅同步 `globalAssignments`：`src/features/agents/agents-page.tsx:110-228`；项目 assignment 只在项目页面 direct 分支同步：`src/features/projects/detail/assignments/agent.tsx:89-117`。
- `AgentDto`、`McpServerDto`、`SkillDto` 等现有 DTO 不足以反查全部项目 scopes，因此统一触发不能继续由前端基于列表 DTO 推导。

### 删除与范围合同

- MCP、Hook、Agent、Skill 的 assignment 外键/服务约束会拒绝删除仍被分配的资源；Skill 还会拒绝删除残留 managed item。被拒绝的删除没有改变中央意图，不应触发同步。
- 对允许删除的 active Provider/Prompt，mutation 必须保留删除前的 tool scopes，才能在资源行消失后清理旧原生投影。
- 推荐后端 mutation 返回稳定的受影响范围值，至少包含 artifact kind、tool、projectId；范围取变更前后有效 assignments/selection 与需要清理的 managed state 并集。
- 多范围执行应去重、稳定排序并串行；现有 `requestPreview(): void` 需要改为可等待 Promise，才能让原始 mutation 在同步完成后再报告最终结果。

## 外部变化双向处理证据

### 通用扫描与 readopt 限制

- `src-tauri/src/sync/mod.rs:136-211,355-468` 的 `scan_target` / `assess_drift` 已能计算 full/managed hash 并识别 `ExternalOwnedChange`。
- `src-tauri/src/sync/managed.rs:413-491` 的 `readopt_with_scan` 只更新 target/item baseline，绝不更新中央实体；它不能作为“采纳原生更改”的实现。
- `PreviewTargetPlan` 已携带 descriptor、ownership、observed hashes、row versions 与脱敏 diff，是生成双向动作计划的基础证据。

### 六类资源的原生→中央能力

- Provider 已有真正的 `adopt_provider_native`：`src-tauri/src/profiles/provider_adopt.rs:27-170`，会更新中央 Provider 字段并刷新 baseline；无稳定 provider id 且多候选时拒绝猜配。
- Prompt 现有导入能从 WholeDocument 提取正文：`src-tauri/src/profiles/prompt.rs:70-189`，但没有 adopt RPC；只能在唯一 active profile 条件下更新 body，Cursor frontmatter 必须剥离且不能当正文采纳。
- MCP 的 `parse_native_item` 已能将工具原生条目转换为中央配置：`src-tauri/src/mcp/import.rs:442-617`；adopt 应按 managed item `external_key/resource_id` 精确配对，disabled、未知 transport、Codex `env_http_headers` 等不可无损表示时转入应用内匹配/导入。
- Skill 普通扫描只看到直属链接/目录入口：`src-tauri/src/sync/mod.rs:255-295`；采纳必须额外复用 `inspect_skill_source`、tree hash 与 `copy_skill_tree`，把外部完整树安全复制到中央，name 变化或路径逃逸时拒绝自动配对。
- Hook 可复用导入的 event/matcher/command/timeout 与脚本复制逻辑：`src-tauri/src/hooks/import.rs:28-267`、`hooks/service_native.rs:366-510`；匿名数组、内容派生 item key、复杂 shell 和 assignment event 使部分变化必须进入应用内匹配。
- Agent 可复用 Markdown/Codex TOML parser：`src-tauri/src/agents/service_native.rs:510-665`；按 managed path/resource identity 更新中央三字段与 tool settings，文件重命名、同 stem 冲突和 dropped fields 不能静默采纳。

### 扫描与 UI 现状

- 项目详情、全局 Skills、全局 Agents 已有现场扫描；全局 MCP/Hooks 多数只读持久化 `last_status`，Provider/Prompt 状态不扫描漂移，需要统一现场扫描 RPC/结果。
- 当前没有文件 watcher、轮询或通用外部变化事件；React Query 也关闭 window-focus refetch。用户已选择按使用时机扫描，而非新增 watcher。
- 触发点应覆盖：页面进入、应用重新获得焦点、`environment-ready` 后刷新、显式重新扫描。
- 状态 DTO 目前缺少动作能力、目标身份和 row versions；状态卡不能直接安全执行采纳/覆盖，应先生成绑定当前 observation 的外部变化计划。
- 现有 `ChangePreviewDialog` 虽支持 Provider adopt/readopt，但文案和能力硬编码且属于将删除的确认流；新 UI 应是非模态状态操作，不阻塞中央操作的自动覆盖。

### 协议结论

1. 被动扫描只报告，不自动修改任一侧。
2. 状态卡对 external-owned change 请求持久化 `ExternalChangePlan`（或等价现有 Preview 扩展），返回 `canAdoptNative/canOverwriteCentral`、原因、脱敏 diff、目标/资源/assignment row versions。
3. “采纳原生更改”真正更新中央实体、managed item 与 baseline；“以中央配置覆盖”复用普通 persisted Preview/Apply。
4. 用户点击动作本身就是授权，不再增加第二层确认。
5. 无法唯一/无损映射时，采纳动作进入应用内匹配/导入流程；不得用 readopt 掩盖，也不得要求用户去文件夹手工处理。
