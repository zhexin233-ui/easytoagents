# MCP 原生全局导入技术设计

## 1. 范围与现有机制

本功能补上原生全局 MCP 到中央意图之间的入口，不改变既有 Preview/Apply 写入流程。Provider 导入已有扫描令牌、重新读取 full hash、Immediate 事务和基线接管模式；MCP 需要多条目选择、跨工具复用和已有目标上的增量接管，不能直接调用只允许首次单档案导入的 Provider repository。

主边界：Rust 解析和校验原生配置，SQLite 保存中央意图和导入证据，React 仅展示生成 DTO 和提交选择。原文件只在后续既有 Apply 中写入。

## 2. 数据流与交互

1. 每张 MCP 全局目标卡片提供“检测并导入已有 MCP”；入口沿用工具能力和策略阻止语义。中央空状态与空同步预览说明该入口的用途。
2. 用户触发某一工具的检测，调用 `discover_mcp_import(tool)`。使用发布命令已有的显式环境和 Claude policy/capability probes，解析正确的全局 descriptor。
3. 后端安全读取目标，逐项转换、校验并与中央记录和既有 managed items 对照；持久化脱敏导入预览及校验证据，返回候选列表。
4. 单工具导入对话框默认不勾选任何项。可导入项可选；已管理、同名冲突、停用和不兼容项展示原因且不可选。复用中央记录的候选明确说明将添加来源工具分配。
5. `confirm_mcp_import({ previewId, candidateIds })` 重新发现并验证，单事务导入所选项、记录所有权和消费令牌。没有跨工具部分成功问题，因为每次确认只针对一个工具。
6. 成功后清理导入状态、失效整个 MCP query family；提示原文件未变，用户另行点击“生成全局预览”。导入界面与 `ChangePreviewDialog` 状态分离，永不隐式 Apply。
7. 关闭、重开或更换来源必须丢弃旧选择并重新检测；异步旧响应不能重新打开已关闭的对话框或覆盖当前工具的结果。

## 3. RPC 合同

Rust/Specta 是唯一类型来源，新增 DTO 统一由绑定生成器导出，不手改 TypeScript 绑定。

- `discover_mcp_import(tool: Tool) -> Result<McpImportPreviewDto, AppError>`。
- `McpImportPreviewDto` 包含 `previewId`（没有可确认候选时可空）、`tool`、`targetPath`、候选及安全诊断；缺失文件和空容器是明确的空结果，文件解析失败/权限/策略异常为稳定错误或目标级阻止，不能伪装为无 MCP。
- `McpImportCandidateDto` 包含不透明 `candidateId`、安全名称、可选 transport、条目状态、拟执行动作（新建或复用）、原因和脱敏投影；拒绝项不回传原始参数、URL、headers、env 或未过滤解析错误。
- 状态至少区分可导入、已管理、同名冲突、原生停用、不支持、非法。UI 只消费状态，不自行比较配置或推断导入资格。
- `ConfirmMcpImportInput` 只含 `previewId` 和非空、不重复的 `candidateIds`；工具、路径、名字、原始配置、复用记录 ID 由后端令牌解析，不接受客户端重建。
- 确认结果仅给出来源工具和创建/复用/分配数量；页面通过失效 MCP 查询获取安全中央 DTO。`STALE_PREVIEW`、`CONFLICT`、`PREVIEW_ALREADY_CONSUMED`、`WRITE_IN_PROGRESS` 和既有路径/策略错误保持稳定。

## 4. 原生转换与保真边界

新增 MCP 导入专用模块，复用 `ValidatedMcpConfiguration`、`hash_json`、既有敏感值注册和安全 scan 机制；不要复制 JSON/TOML 文档读写器。

| 原生来源 | 中央映射 |
| --- | --- |
| Claude 显式 `type=stdio`，或无 type 但只有 command | `stdio`；command/args/env |
| Claude `type=http` / 已兼容的 `streamable_http`，或无 type 但只有 url | `streamable_http`；url/headers |
| Codex command/args/env | `stdio` |
| Codex url/http_headers | `streamable_http`；http_headers 映射 headers |
| 缺省 args/env/headers | 中央空集合；不将非法类型当成缺省 |
| 可移植未知字段 | 放入 extra，经过现有大小、深度、类型和保留键校验，来源工具回写时保留 |
| `enabled=false` 或 `disabled=true` | 只展示“原生已停用，暂不导入”，不创建管理关系 |
| SSE、混合协议、冲突字段、不可表达扩展、敏感普通参数/URL | 逐项拒绝并说明原因，不降级转换 |

`env_http_headers` 不能变成实际 header 值，也不能绕过 extra 保留键限制；本次不支持接管。可被现有 extra 无损保存的引用字段保持引用，不解析为环境变量真实值。不会为了导入放宽现有创建/编辑校验。

接受条目的结构化配置在确认时由原文件重新提取，不使用脱敏 DTO 重建。后续 renderer 可能补充 Claude type、Codex enabled=true 或省略空集合；这些等价规范化需出现在同步预览中，不在导入时写文件。无法说明为保真的转换应拒绝该项。

## 5. 同名和重复规则

- 中央名称仍遵循数据库 NOCASE 唯一约束；不改 schema 的中央命名规则。
- 原生名称与中央名称必须精确一致，且所有规范化字段、启用状态、私有 headers/env 和 extra 完全一致，才可标记为复用；按名称、hash 的脱敏值或 URL 单独比较都不充分。
- 精确同名但配置不同、仅大小写相同、同一原生文件中存在大小写碰撞，都标记冲突，不由遍历顺序任选一项。
- 复用时只新增来源工具的 assignment 和所选原生条目的管理关系，保持中央配置和另一工具不变；既有项目 assignment 互斥仍执行。
- 来源工具已经管理该 external key 时显示已管理；不借重复导入刷新基线、解决漂移或恢复已被用户修改的中央意图。
- 相同确认令牌只成功一次；再次扫描的已管理项不可选。未选中的合法项允许在后续新扫描中继续导入。

## 6. 导入证据与事务

新增前向迁移 `0005_mcp_import_previews.sql` 和窄范围 repository，不扩展只允许 provider/prompt 的 `profile_import_previews`，不修改已应用迁移。

导入记录包含 UUID、tool、目标路径、原文件 full hash、脱敏展示 JSON、结构化校验上下文、previewed/consumed 状态及时间。校验上下文保存 descriptor 身份/能力证据指纹、候选 ID 到原生 key/item hash 的对应、相关中央 ID/row version 或预期不存在，以及原 managed target/items 的版本快照；不保存原始配置值。

确认顺序：

1. 读取令牌并确认工具身份和未消费状态；校验选择非空、不重复且每个 ID 都属于令牌中的可导入候选。
2. 从 AppState 取得既有显式环境，通过现有 probe 校验能力/策略证据绑定，再安全重新读取来源；命令不重读进程环境、不重跑工具二进制。校验 descriptor、路径和 full hash，依据所选候选重新转换并检查其原生 key/item hash 和候选资格。
3. 使用 `TransactionBehavior::Immediate`；禁止存在 applying/restoring/rollback_failed run，重新核对令牌、相关中央记录/目标/items 的版本和唯一约束，并在获得写锁后重新读取源文件。不能串联现有“创建”和“分配”两个独立提交的 API。
4. 插入或复用所选中央记录；在同一事务内建立来源工具全局 assignment、目标基线及逐项 `managed_items`。
5. 消费令牌前再次读取源文件核对 descriptor/full hash，再以条件更新消费令牌并 commit；任一步错误整个事务回滚。注册允许导入的敏感值后只返回安全 DTO。SQLite 不锁外部文件，不将这种复核声称为跨文件/数据库原子锁。

file full hash 已覆盖扫描到确认之间的原生内容变化；逐项 ID/哈希用于验证选择确实来自已展示清单，不能以此允许源文件已变化时继续确认。中央和管理状态另用行版本/不存在断言保护。

## 7. 首次与增量基线接管

原生同名条目不能仅靠创建中央记录来接管。确认必须创建 `managed_items(resource_id, external_key, last_applied_item_hash)`，并把目标 baseline 绑定到实际观察内容，而不是预期 renderer 输出。

- 首次接管：目标不存在则创建；只有空基线的既有目标可复用。管理投影仅含本次所选原生项，未选中项仍非受管。
- 增量接管：先使用旧 ownership 对旧受管项做只读扫描，验证 baseline hash 对、原 item 集合及逐项 hash 一致；旧受管项变化、缺失、基线不完整或身份无法验证时阻止导入，不能刷新旧漂移。
- 验证通过才在原 ownership 上追加本次所选项，保存 union 投影/hash 及当前 full hash。旧 item ID/resource 关系保持不变，新增 item 只对应所选项。所有目标/旧 item 版本必须在事务内复核。
- 后续 preview/apply 继续使用既有 `prepare_mcp_sync`、`verify_managed_item_baselines` 和 managed item 版本保护；导入事务引发的版本变化使旧同步预览失效。
- 不接管停用条目：现有中央 enabled=false 表示从 desired set 排除，接管后会产生删除计划；仅改 renderer 的 enabled 值无法解决该语义差异。

## 8. 安全和兼容

- 命令沿用显式环境、release probe 和规范路径，不直接读取进程 HOME，也不猜非默认 Claude MCP 位置。
- scan/confirm 都拒绝不安全链接/祖先、不可读或类型异常；不解析其它工具配置或不相关文件。
- 扫描不得建立中央记录、assignment 或 managed target；允许写入应用私有的脱敏导入预览。
- 原始 headers/env/extra 仅进入允许的私有中央库及私有 baseline；导入预览表、RPC、错误、同步记录和 journal 只保存脱敏内容或 hash。拒绝项的异常不可包含原始输入。
- 同名比较在后端私有值上执行，UI 不接触秘密；未经选择的内容不作为中央凭证保存。
- 前向迁移只加导入证据表和索引，保留原有中央数据及同步表合同；注册迁移并验证旧库升级和重复打开。
- 回滚不能简单让旧二进制忽略新迁移。代码回退需配合现有迁移前私有备份或兼容版本；不得自动删表/降级或恢复真实用户数据库。导入本身未触碰原生配置。

## 9. 测试与影响面

后端主要影响 `mcp/{models,service,mod}.rs`、新增导入模块、`db/mcp.rs` 或新增导入 repository、迁移注册、commands/mcp 和 lib.rs 注册。窄 helper 如需复用只调整可见性，不改造通用同步引擎。

前端影响 `mcp-page.tsx`、可拆出的导入对话框、`mcp-api.ts` 及生成绑定。复用 Button、`useDialogFocus`、现有错误展示与 `ChangePreviewDialog`。现有 `mcp-page.test.tsx`、MCP service fixture、DB migration tests 是优先扩展点。

覆盖两工具映射、只导入所选项、秘密保留与脱敏、原文件字节不变、源工具分配、同名复用/冲突、分批接管、旧漂移保护、令牌/版本过期、重复/伪造选择、DB 事务回滚、活动写入互斥及后续 preview/apply 保留未受管内容。完整执行步骤见 `implement.md`。
