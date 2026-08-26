# 实施计划

## 0. 进入实现前

- [x] 用户在最终规划摘要之后明确回复“可以”，批准实现。
- [x] 确认 `prd.md`、`design.md` 已收敛，两个 JSONL 均有真实有效的规范/研究条目。
- [x] 运行 `task.py validate` 并确认仅本任务规划文件变更；满足 `task.py start` 门槛。
- [x] 按当前 Codex/Trellis 流程派发实现与检查，派发正文以活动任务路径开头；不得自动提交或修改用户真实全局目录。

## 1. 来源解析与安全扫描

- [x] 在 Skills 领域内新增导入模块，从显式环境生成 Claude、Codex 正式/兼容来源，正式同步 descriptor 不变。
- [x] 从 `library.rs` 提取只读 inspect/hash/frontmatter helper，默认单目录导入仍拒绝根链接；新来源边界显式允许有证据的入口链接。
- [x] 实现有限的直属枚举、来源根/入口/链/最终目录身份绑定、32 跳循环保护和有限资源预算；排除 `.system` 及其入口别名，不扫描插件缓存。
- [x] 分类来源缺失/空/不可读与逐候选错误，聚合同目录/同内容入口，检测同名不同内容冲突；已指向中央库的链接只能在证明现有记录有效后识别为已导入。
- [x] 扩展现有 library/service fixture：默认/自定义根、两处 Codex 来源、普通目录/链接、断链/循环/不安全祖先、内部链接/硬链接拒绝、技能文件非法、预算超限、内置集合/别名排除、检测零 staging/零原生写入。

检查点：只读发现可证明来源和候选资格，原文件字节/链接文本/权限不变；任何有争议的链接放行先停下复核，不弱化既有导入校验。

## 2. 私有证据与批量确认

- [x] 新增并注册 `0006_skill_import_previews.sql` 与窄 repository，JSON shape、状态、UUID、时间和索引有约束；不改历史迁移。
- [x] 保存只含安全 metadata/identity/hash/version 的证据，绑定来源环境、候选集合和中央快照，不存正文或任意 frontmatter。
- [x] 提取现有 `insert_skill` 的事务内插入 helper；旧单目录导入保留事务包装与行为。
- [x] 实现仅提交 previewId/candidateIds 的确认路径：重验 → 专有 staging → Immediate 事务 → 重验 → finalize/全部插入 → 重验 → 条件消费令牌 → commit。
- [x] 实现提交前已知失败的批量补偿；提交不确定时先核对令牌与整批记录，无法判定则保留目录，不删除可能已提交的数据。
- [x] 保持 assignment、managed target/item、sync runs 与所有原生文件完全不变；不能直接套用 MCP 的接管事务。
- [x] 用隔离 DB/目录覆盖：选择子集、同批去重、跨工具已导入识别、同名冲突、来源/中央 stale、重复/伪造/空选择、活动 writer、两个独立连接确认、第二项复制/rename/SQL 失败和提交不确定分支。
- [x] 升级 v5 fixture 到 v6，保留已有记录，重复打开幂等。审计非空 RPC/证据/错误载体，确保 fixture 正文和私有 frontmatter 未泄漏。

检查点：正常返回失败不出现中央列表部分成功；崩溃不声称跨文件/DB 原子性，未知私有残留不自动清扫；不增加来源工具分配。

## 3. RPC 与初始状态

- [x] 在 `skills/models.rs` 定义候选/来源/预览/确认结果 DTO，注册两个新命令并从模块导出；输入不得接受客户端重建的来源路径或哈希。
- [x] 使用稳定 `AppError`、固定原因文本和既有权限/策略证据；保留缺失、错误和受阻区别。
- [x] 在 Skills 全局状态服务中按 `design.md` 条件产生首次空目录/首次未管理诊断；保留通用 `SyncStatus` 和 drift 算法。
- [x] 扩展服务回归：无目录、空目录、仅元文件、已有外部目录、无基线的 preview target 行、复制后仍未接管、desired 已存在、完整/半个基线、managed item 漂移、中央损坏、策略/权限异常。
- [x] 生成并核对 bindings，禁止手写 TypeScript RPC 类型。

## 4. Skills 检测与选择界面

- [x] 全局卡片增加检测入口；空中央列表指向该入口，保留原单目录导入。官方 Codex 目标缺失不能阻止检测兼容来源。
- [x] 新增独立导入对话框，使用生成类型、requestId 隔离查询和共享焦点机制；每次打开/重扫产生新 requestId，关闭/换工具清空选择，按来源展示结果。
- [x] 默认不选中，只允许 importable；已导入项只读、冲突/无效显示原因，内置技能只提示排除。
- [x] 确认中锁定重复提交/关闭/重扫；过期或失败后需新扫描；旧异步结果不能覆盖新工具/新对话框。
- [x] 成功仅刷新 Skills query family，并明确中央副本/原安装/未分配状态；不得调用 assignment 或 Apply。
- [x] 在共享状态 helper 中识别 Skills 专用诊断，MCP 映射和原有阻断保持不变；明确测试 `SKILL_TARGET_INITIAL_*` 与不匹配 status/普通 MCP 诊断组合不覆盖原展示。
- [x] 扩展既有 Skills page 测试及共享 helper/MCP 回归：两工具精确 payload、无默认选择、loading/error/empty、局部来源失败、内置排除、已导入/冲突、stale→重扫、关闭重开、晚到响应、提交锁、Tab/Escape/焦点恢复和成功刷新。

## 5. 验证与独立复核

实现期间先跑对应 fixture；最终一次质量检查覆盖整个任务，不能只检查最后一个变更。

```bash
pnpm bindings:generate
pnpm bindings:check
pnpm test --run src/features/skills/skills-page.test.tsx src/features/mcp/mcp-page.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml skills::
pnpm check
git diff --check
```

`pnpm check` 已包含格式、lint、typecheck、前端测试、Rust fmt/clippy/test。若新增 source/import 模块测试不在 `skills::` 命名空间，加入相应过滤器或直接使用最终全量 Rust 测试；不得只跑固定过滤器声称全量通过。

- [x] 按 `trellis-check` 全范围复核实际 diff、需求映射、路径安全和失败补偿；主代理核验关键结论，不凭代理摘要宣告完成。
- [x] 浏览器/桌面可视检查优先使用隔离 fixture 或只读页面。原生 RPC 端到端使用临时 home/config/data；不向用户真实目录确认导入或执行 Apply。
- [x] 核对界面：目标卡片文案、两处 Codex 来源、内置排除、选择确认与中央列表更新；jsdom 不等于真实桌面验证，实际不可验证部分明确记录。
- [x] 核对 AC1–AC9 全部有可观察证据，原生来源在检测/确认前后文件、链接和权限一致，全局/项目分配及基线不变。

## 6. 规范、提交和收尾

- [x] 主代理按 `trellis-update-spec` 记录来源入口链接与内部链接的不同边界、复制不接管、内置排除、批量证据/清理和 Skills 初始诊断合同；必要时新增 Skills 导入专用规范并更新索引。
- [x] 记录实际验证结果与未覆盖项，核对本任务与外来变更，不提交未识别文件。
- [x] 展示本任务提交计划并获得一次确认后才提交；不 amend、不 push，不提前归档。
- [ ] 代码提交后按 `trellis-finish-work` 归档和记录会话。

## 风险与退回规划条件

- 不能为了支持入口链接放松来源链身份复核、内部 no-follow 或中央目录所有权证明。
- 不能用全局重命名状态掩盖 MCP/Skills 真实漂移，也不能在导入成功后显示原目标已接管。
- 不能为接管同名外部安装而修改 Apply/恢复模型；如实现必须这样才能满足新要求，先返回规划。
- 迁移回退需兼容版本或私有备份；不自动删新表、降级或恢复真实数据库。
- 若需要改变内置排除、额外目录扫描、失败原子性或来源副本语义，更新 PRD/设计并重新提交最终审阅。
