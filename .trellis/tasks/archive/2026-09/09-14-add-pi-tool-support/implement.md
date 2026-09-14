# Implement：新增 Pi 工具支持

前置：`prd.md` 的 AC1–AC7 与 `design.md` 的 §1 不变式是验收基线；权威证据以 `research/verified-contract.md` 为准。**任何阶段发现证据与矩阵冲突，停止实现并回到规划。**

质量门（每阶段结束必跑）：

```bash
pnpm format:check && pnpm lint && pnpm typecheck && pnpm test --run
pnpm bindings:check && pnpm rust:check
git diff --check
```

---

## 阶段 0：动工前基线

- [x] 0.1 确认基线全绿：`pnpm check`（若已有失败，先记录并单独处理，不带病开工）。
      → 基线 `cargo test` / `pnpm typecheck` / `pnpm test --run` / `git diff --check` 全绿；
      `pnpm lint` / `pnpm format:check` 因未跟踪的 `.pi/`（本机工具产物）预先失败，已记录到
      `research/baseline-2026-09-14.md`，未顺手修复。
- [x] 0.2 记录当前 schema 版本（应为 `0024_project_native_agent_files.sql`），确认新增迁移文件名为 `0025_pi_tool_support.sql`。
- [x] 0.3 冻结证据基线：记录 `pi --version` 输出、本机 agent dir 布局与 `pi-mcp-adapter` 版本到任务备注（证据日期 2026-09-14 / Pi 0.85.1 / adapter 2.33.0）。
      → 见 `research/baseline-2026-09-14.md`。

**回滚点 R0**：仅读操作，无代码变更。

---

## 阶段 1：领域层与探针（R1/R8/R11 的骨架）

- [x] 1.1 `domain::Tool` 增加 `Pi => "pi"`，`Tool::ALL` 扩到 6；补 `Tool::Pi` 序列化往返测试。
- [x] 1.2 `tool_capabilities()` 增加 Pi 行（`provider/prompt_global/mcp/skills = true`，`hooks/agents/project_agents/agent_tool_settings = false`），更新既有能力矩阵测试。
- [x] 1.3 `HookEvent::supported_for_tool` 增加显式 `Tool::Pi => false`。
- [x] 1.4 `ExplicitEnvironment` 增加 `pi_agent_dir`（默认 `home/.pi/agent`）+ `with_pi_agent_dir` + `pi_agent_dir()`；`tool_index`、`ToolAvailability` 扩到 6。
- [x] 1.5 `ToolBinary::Pi`（`pi`）与 `parse_version` 分支；`ReleaseToolProbeInput` 增加 `pi_agent_dir`/`with_pi_agent_dir`，探针并行扩到六路；`lib.rs` 边界读取 `PI_CODING_AGENT_DIR`，不可映射时产出 `PI_AGENT_DIR_OVERRIDE_UNMAPPED`。
- [x] 1.6 注册表：`PROFILE_TOOLS` / `ASSIGNABLE_MCP_TOOLS` / `ASSIGNABLE_SKILL_TOOLS` 保持 `Tool::ALL`（6 元素，Pi 均纳入，**不缩小任何集合**）；`ASSIGNABLE_HOOK_TOOLS` / `ASSIGNABLE_AGENT_TOOLS` / `PROJECT_AGENT_TOOLS` 保持不变（Pi 不进入）；补精确成员锁定测试。
- [x] 1.7 `adapter_for`/`global_root_for`/`native_mcp_container`（`Tool::Pi => &["mcpServers"]`）/`agent_file_extension` 的穷举 match 显式处理 Pi（Agent 分支注明服务层已拒，不得靠 `_` 兜底）。
- [x] 1.8 MCP 适配器只读探测（§3.3）：`packages[]` 声明 / `extensions: []` 过滤 / `npm/node_modules/pi-mcp-adapter/package.json` 版本 / 项目级两态；产出 `PI_MCP_ADAPTER_MISSING` / `_NOT_LOADED` / `_VERSION_UNSUPPORTED` 三态，测试不得碰真实 `~/.pi/agent`。

**验证**：`cargo test`（domain/probe/discovery/adapter-probe）；探针矩阵测试覆盖成功/缺失/超时/异常输出/链接路径与两种 `PI_CODING_AGENT_DIR` 状态。

> 阶段 1 偏离记录（仅为让 `Tool::ALL` 扩到 6 后仓库保持可编译/可测，不实现阶段 2+ 能力）：
>
> 1. `adapters/pi/mod.rs` 是**空发现**骨架：`discover()` 返回空集合（阶段 2 产出 6 个 descriptor），
>    仅建立 `PI_AGENT_DIR_OVERRIDE_UNMAPPED` / `PI_INSTALLATION_PROBE_UNSUPPORTED` 能力通路与 `pi::probe`。
> 2. 新增 `adapters/pi/probe.rs` 承载 1.8 的只读适配器探测（最小支持版本常量 2.33.0）。
> 3. 全 crate 的穷举 `match Tool` 显式补齐 Pi 分支：Hooks/Agents 返回设计 §8 的
>    `PI_HOOKS_UNSUPPORTED` / `PI_AGENTS_UNSUPPORTED`；Provider/Prompt/MCP/Skills 的服务与数据库分支
>    暂以 `adapters::pi::pending_implementation()` fail closed（阶段 2–4 替换），不回落任一既有工具。
> 4. `sync::managed::list_global_target_statuses` 暂跳过「无 descriptor」的工具；
>    Pi descriptor 阶段 2 落地后必须移除，让缺失 descriptor 重新暴露。
> 5. 生成 bindings 后 `Tool` 新增 `pi` 触发前端 `Record<Tool, …>` 编译期守卫，因此补齐了最小
>    前端入口（`pi` metadata/图标/`ENABLED_TOOL_ORDER` 默认不勾选、onboarding `Choices`、
>    官方登录文案映射）与官方图标字节级复制 + README 条目；更完整的页面收敛、文档同步仍属阶段 5。

**回滚点 R1**：本阶段只改枚举与探针，可整体 revert 而不影响数据库。

---

## 阶段 2：Pi Adapter（Provider / Prompt / Skills / MCP）

- [x] 2.1 新增 `src-tauri/src/adapters/pi/mod.rs`，实现 §1/§4 的 6 个 descriptor，不含 Hook/Agent descriptor。
- [x] 2.2 Provider codec：`models.json` 条目级局部合并、未知字段保留、内置模型合并语义保留、敏感 selector、`$ENV`/`!command` 不展开不执行。
- [x] 2.3 Prompt descriptor 的 `prompt_override` 三态 + 回退文件诊断（`PI_PROMPT_FALLBACK_PRESENT`）。
- [x] 2.4 Skills descriptor 的 `ManagedChildrenOnly` 与断链/逃逸自检（`PI_SKILL_SYMLINK_BROKEN` / `PI_SKILL_SYMLINK_ESCAPE`）。
- [x] 2.5 MCP descriptor（全局/项目）：`mcpServers` 条目级；适配器未就绪时 `capability = unsupported(...)` 且 `path = None`；`exclusive` 模式下项目 descriptor unsupported；`trust = NotRequired`（项目）+ 安全提示文案。
- [x] 2.6 MCP 遮蔽检测：`<root>/.mcp.json` / 用户手写 `.pi/mcp.json` 同名 server → 硬阻断（`PI_MCP_SHADOWED_BY_PROJECT_SHARED` / `_PROJECT_PI`）；未生成指向共享文件的任何 descriptor。
- [x] 2.7 Adapter 注册进 `adapters/mod.rs`、`adapter_for`、`global_root_for`（`allowed_root` 只能是 `pi_agent_dir` 或 canonicalize 后的项目根）。
- [x] 2.8 Adapter 单测：descriptor 矩阵（6 个）、无 Hook/Agent 目标、ownership、敏感 selector、allowed_root、trust 两态、override 三态、适配器三态、exclusive 模式。

**验证**：`cargo test adapters::pi`；`pnpm rust:check`。

**回滚点 R2**：移除 adapter + 注册点即可；服务层尚未接入。

> 阶段 2 边界记录（未越界，但影响后续阶段验证）：
>
> 1. 本阶段按 `design.md` §1 冻结面产出 6 个 descriptor，并移除了阶段 1 的
>    `sync::managed::list_global_target_statuses` 临时跳过。由于阶段 3 的
>    `0025_pi_tool_support.sql` 尚未应用，`managed_targets` 的 `tool` CHECK
>    仍拒绝 `pi`：当 Pi 被判为 Installed 且项目存在时，`projects` / `agents`
>    的通用项目观测会在 `insert_project_target_identity` 触发 CHECK 失败
>    （40 个既有测试用例）。这不是五工具行为变更，而是「descriptor 已暴露、
>    数据库尚未放宽」的已知阶段依赖；阶段 3 迁移落地后应全部转绿。
> 2. `skills::service::tests::first_global_sync_is_pending_then_preserves_existing_entries_for_all_tools`
>    只对 5 个工具做 Preview，却断言全部 `ASSIGNABLE_SKILL_TOOLS` 状态为
>    pending；Pi 进入集合后断言失败，属阶段 4 服务接线的测试同步项。
> 3. `PI_PROJECT_SKILLS_UNTRUSTED` / `_TRUST_UNKNOWN`、`PI_MCP_SHADOWED_*`、
>    `PI_SKILL_SYMLINK_*`、`PI_PROMPT_OVERRIDE_DETECTED` 等诊断常量与只读
>    探测/自检 helper 已就绪，但 Apply 层接线属阶段 4。
> 4. 2.5 的「安全提示文案」为前端/MCP 页展示，属阶段 5.3；descriptor 的
>    `trust = NotRequired` 已如实落地。

---

## 阶段 3：数据库迁移与仓储

- [x] 3.1 新增 `0025_pi_tool_support.sql`：按 §6 表格逐表放宽（含 `mcp_*` 三表），`managed_targets` 显式限定 `pi` 的 artifact 集合为 `provider|prompt|mcp|skill`，`prompt_profiles` 增加 `is_active_pi` 与部分唯一索引。
- [x] 3.2 每个锚点先用 `SELECT` 验证恰好命中一次；未命中立即失败（沿用 `instr(...) > 0` 守卫）。
- [x] 3.3 仓储层：`db/profiles.rs` 的 `is_active_pi` 映射、`db/profiles_tests.rs`、`db/tests.rs`、`db/skills.rs`、`db/mcp.rs` 的 Pi 行。
- [x] 3.4 迁移测试：v24 → v25 升级、旧行保留、同连接插入、重开、外键/索引、重复打开、canary（`pi` + `hook|agent` 在 `managed_targets` 被拒；`pi` + `provider|prompt|mcp|skill` 可插入；`pi` hook/agent 行在各自表被拒）。

**验证**：`cargo test db::`；`pnpm bindings:generate && pnpm bindings:check`。

**回滚点 R3**：已应用的迁移**不倒迁**；代码回滚后放宽的 CHECK 对旧数据无破坏。

---

## 阶段 4：服务、同步与 fail-closed

- [x] 4.1 `profiles/`：Pi Provider 与 Prompt 分支（发现/校验/渲染/导入预览/每工具生效提示词）。
- [x] 4.2 `skills/`：Pi 全局与项目分配、导入、状态聚合；项目 trust 未确认时禁止 Apply。
- [x] 4.3 `mcp/`：Pi 分支——导入（`mcpServers` 容器）、会话投影（stdio/HTTP，不写 `type`，停用→`disabled`）、适配器就绪门禁、遮蔽检测与 `exclusive` 模式接线；未知字段进 `extra` 原样保留，零求值，敏感 selector 全覆盖。
- [x] 4.4 `hooks/`、`agents/`：`ensure_*_supported` 守卫，对 `Tool::Pi` 返回 `PI_HOOKS_UNSUPPORTED` / `PI_AGENTS_UNSUPPORTED`，穷举 match 显式拒绝。
- [x] 4.5 `projects/`：Pi 只产生 MCP 与 Skills 的 assignment/status；不产生项目 Prompt/Hook/Agent 行；`.pi/mcp.json` 不受 trust 门禁，但 Apply 前给安全提示。
- [x] 4.6 `sync/`：`PI_PROMPT_OVERRIDE_DETECTED` 与 MCP 遮蔽类诊断硬阻断，其余 Pi 诊断码接线；`overview/` 计数与 allowed_root 复用统一映射。
- [x] 4.7 跨层 E2E：`src-tauri/tests/phase8_e2e.rs` 增加 Pi Prompt/Provider/MCP 的 Preview → Apply → 漂移 → Restore；Hook/Agent 请求与适配器缺失时的 MCP 请求均全链路拒绝。
- [x] 4.8 隔离 smoke 固化为回归 fixture：`PI_CODING_AGENT_DIR=<tmp>` + `HOME=<tmp>` 下 0.85.1 真实 loader 发现受管链接与 `AGENTS.md`；`AGENTS.override.md` 使写入无效；未信任项目忽略 `.pi/skills`；适配器 `getConfigDiscoveryPaths` 的优先级与项目覆盖全局（`research/pi-mcp-adapter.md` §7 脚本骨架）。

**验证**：`cargo test`；`pnpm test --run`。

**回滚点 R4**：先关闭 capability 与集合，再移除 service 分支。

---

## 阶段 5：前端与文档

- [x] 5.1 `pnpm bindings:generate`，`tool-metadata.ts` 增加 `pi`（label / icon / `profileRoute`，能力来自 bindings）。
- [x] 5.2 `ENABLED_TOOL_ORDER` 增加 `pi`（默认不勾选）；补设置页与 metadata 集合测试。
- [x] 5.3 页面收敛验证：Hooks/Agents 无 Pi 入口；MCP 页按 descriptor 能力呈现适配器缺失/未加载诊断与 `pi install npm:pi-mcp-adapter` 指引；Projects 出现 MCP 与 Skills；Providers/Prompts 导航按 `PROFILE_TOOLS` 出现。
- [x] 5.4 品牌资源：从 `research/assets/pi-badge.svg` **字节级复制** 到 `src/assets/brand/pi-icon.svg`，校验 SHA-256 == `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a`；同目录 README 按既有格式补条目（来源描述 + Press Kit URL + 资产 URL + 抓取日期 2026-09-14 + MIT 许可 + 哈希）。**不自绘、不 recolor、不优化、不用 CDN**。
- [x] 5.5 文档同步：`docs/maintainers/adding-tool-adapter.md` 第 9 节 Pi 行改为已核验结论（含迁移号、能力矩阵、诊断码），README 工具列表与能力表更新。
- [x] 5.6 文案一致性检查：UI 文案、`tool_capabilities()`、`research/verified-contract.md` 三者逐项比对。

**验证**：`pnpm check` + `git diff --check`。

**回滚点 R5**：UI 与共享集合关闭 Pi 即可让功能整体不可达。

---

## 阶段 6：质量门与评审

- [x] 6.1 全量质量门：`pnpm check`、`git diff --check`。
- [x] 6.2 事实核查：随机抽 3 条设计结论，对照 `research/verified-contract.md` 的官方 URL 复核。
- [ ] 6.3 邀请 `trellis-check` 做独立跨层检查（AC1–AC9 逐条）。
- [x] 6.4 spec 更新（阶段 3.3）：把 Pi 合同、诊断码与 fail-closed 矩阵写入 `.trellis/spec/backend/`（新增 `pi-adapter-guidelines.md` 并在 `index.md` 登记）。
- [ ] 6.5 汇报与提交：AC 逐条结论 + 未决问题，等待用户确认后再提交。

---

## 风险与停线条件

| 风险                                                 | 停线条件                                                                            |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Pi 版本升级改变 symlink 发现或候选文件表             | 隔离 smoke 失败 → 回规划关闭对应该能力                                              |
| 适配器升级改变 schema 或路径                         | 遮蔽/漂移 smoke 失败或最低版本探测触发 → 回规划重核 `mcpServers` 契约与最低支持版本 |
| 适配器与 EasyToAgents 互写导致受管条目持续漂移       | read-modify-write 后仍复现 → 停线，回规划重定 ownership                             |
| `models.json` 条目级合并在真实 0.85.1 下丢失内置模型 | Provider Apply 关闭（回到 PRD，不改成整文件覆写）                                   |
| `PI_CODING_AGENT_DIR` 语义无法安全映射               | Provider/Prompt/Skills/MCP descriptor 全部 unsupported，只保留工具入口状态          |
| 迁移锚点未命中                                       | 迁移失败并回滚，不手改历史 SQL                                                      |
| 原五工具任一回归                                     | 立即停线，先修回归再继续                                                            |

---

## 阶段 3–6 实施记录（2026-09-14，主会话内联实施）

> 环境事实：本机 Pi host 把 `read`/`bash`/`edit`/`write`/`grep`/`find`/`ls` 以非 `builtin`
> 来源注册，导致 `pi-subagents` 的 `getHostBuiltinToolNames()` 把这些工具从显式 `tools:`
> 白名单中全部过滤掉，`trellis-implement` / `trellis-check` / `trellis-research` 派发后
> 拿不到任何仓库工具（`scout` 探针返回 `effective tool allowlist: [contact_supervisor]`）。
> 经用户确认采取了「主会话内联实施 + 不使用子代理」的方式完成阶段 3–6；作为缓解，
> `.pi/agents/trellis-*.md` 的去 `tools:` 白名单改动已实测可恢复派发（每个文件内均留有注释）。

### 阶段 3（数据库迁移与仓储）

- `0025_pi_tool_support.sql` 采用「TEMP 表前置锚点校验 + `writable_schema` 原地 replace +
  后置新锚点校验 + 旧锚点不得残留 + `pragma_integrity_check`」结构；锚点按 **v24 实时 schema
  文本** 复核（0022 已改写 `managed_targets` 的 tool CHECK 尾部）。
- 前置校验额外断言 `agent_global_assignments` 等 Agents/Hooks 表**未被**放宽（防止上游误改后
  本迁移静默通过）；这也是首次运行 `stale = 0` 失败的原因：该表与 6 张 MCP/Skills 表共用
  同一段旧锚点文本，因此旧锚点残留检查只扫本迁移放行的 6 张表。
- 迁移测试：`db::tests::pi_tool_support_migration_opens_only_supported_artifacts`（升级 / 旧行保留 /
  同连接插入 / 重开 / 重复打开 / 索引 / 外键 / 完整性 / canary）与
  `db::tests::pi_tool_support_migration_rejects_a_missing_exact_anchor`（锚点缺失即中止且不推进版本）。
- 仓储层接入 `is_active_pi`：结构体字段、4 处 SELECT 列表、2 处 INSERT、`prompt_from_row`、
  3 处 `CASE` 生效位查询、`set_global_prompt_assignment` 的 Pi 分支、`deactivate_prompt_profiles`
  列映射，以及 `db/profiles_tests.rs` 的 Pi 生效/替换/停用用例。
- 全量迁移后版本号断言由 24 推进到 25（`src/db/tests.rs` 24 处 + `src/app/mod.rs` 2 处）。

### 阶段 4（服务、同步与 fail-closed）

- `db/profiles.rs` 的 3 处 `Tool::Pi => pending_implementation()` 与 `profiles/models.rs`、
  `profiles/service_prelude.rs`、`mcp/service.rs` 的占位全部替换为真实分支；占位函数
  `adapters::pi::pending_implementation()` 已删除（`adapters/pi/mod.rs` 内 `#[allow(dead_code)]`
  的链接自检 helper 同步启用）。
- `overview::tool_summary` 的 `Tool::Pi => None` 特例移除：`is_active_pi` 落地后统一走同一
  `CASE` 查询，避免「DB 已生效、总览仍显示无生效提示词」的不一致。
- MCP 遮蔽：`PreviewTargetRequest` 新增通用 `hard_block: Option<String>`（服务层证明「本次写入
  必然无效」时的硬阻断通道），`prepare_mcp_sync` 对 Pi 项目 scope 用「本项目实际受管的名称
  （项目自有 + 全局继承）」做只读同名检测。**这里对 design §4 做了收窄解释**：应用自身禁止同一
  MCP 同时做全局与项目分配，且项目 `<root>/.pi/mcp.json` 在适配器合并链中优先级最高，因此
  「全局条目被项目文件遮蔽」在项目目标路径上不可达；按可达语义实现为「项目受管名称被项目
  `.mcp.json` / 用户手写 `.pi/mcp.json` 同名覆盖 → 项目预览硬阻断」。
- Skills 自检：`prepare_skill_sync_in_connection` 对 Pi 调用
  `managed_children_symlink_diagnostic`，断链/逃逸经 `hard_block` 阻断。
- trust 诊断：`assess_drift` 新增 `trust_diagnostic()`，Pi 项目目标返回
  `PI_PROJECT_SKILLS_UNTRUSTED` / `PI_PROJECT_SKILLS_TRUST_UNKNOWN`，其它工具保持 Codex 文案。
- Hooks：`ensure_hooks_supported` 显式拒绝 Pi（`PI_HOOKS_UNSUPPORTED`），
  `hook_target_descriptor` 复用同一守卫（此前只拒 OpenCode，Pi 会落到泛化文案）。
- Agents：新增 `unsupported_agent_tool(tool, scope)`；Pi 在全局/项目分配入口返回
  `PI_AGENTS_UNSUPPORTED`，其它工具保持既有泛化文案。`validate_agent_tool_settings` 原本就在
  入口拒绝非 Claude/Codex，`unreachable!` 分支不会到达 Pi。
- 跨层 E2E：`tests/phase8_e2e.rs::pi_chain_covers_provider_prompt_mcp_drift_restore_and_fail_closed`
  覆盖 Provider 条目级合并（保留非受管 provider 与未知顶层键）、Prompt Apply、
  `AGENTS.override.md` 硬阻断、全局 MCP 投影（不写 `type`，未受管 `directTools` 保留）、
  项目同名遮蔽硬阻断、适配器缺失 fail closed、Hooks/Agents 稳定诊断码、
  项目 Skills 未受信任阻断，以及漂移 → Restore 逐字节回滚。
- 4.8 隔离 smoke：`src-tauri/tests/pi_adapter_smoke.rs` 在 `HOME`/`PI_CODING_AGENT_DIR` 隔离下
  直接 import 适配器库，回归「agent dir 被尊重 + 项目 `.pi/mcp.json` 覆盖 agent 文件」；
  未安装适配器或 `node` 时跳过并打印原因（**部分偏离**：`research/pi-mcp-adapter.md` §7 里
  真实 `pi` loader 发现受管链接与 `AGENTS.md` 的端到端 smoke 需要模型额度，未固化为自动
  fixture；该结论的实机证据保留在 `research/baseline-2026-09-14.md` §0.3 与
  `research/prompt-and-skills.md`，列为残余风险）。

### 阶段 5（前端与文档）

- `pnpm bindings:generate` 后 bindings 中 `Tool` 含 `pi`，`TOOL_CAPABILITIES` 的 Pi 行与
  `design.md` §2 矩阵逐项一致（provider/promptGlobal/mcp/skills = true；hooks/agents/
  projectAgents/agentToolSettings = false）。
- `tool-metadata.ts` / `ENABLED_TOOL_ORDER`（默认不勾选）/ onboarding `Choices` /
  官方登录文案映射已就位；MCP 与 Skills 页面按 descriptor 能力自动收敛（Hooks/Agents 页面
  数据源不含 Pi）。
- 新增 `global-target-status-ui.ts` 的 Pi MCP 诊断文案（含 `pi install npm:pi-mcp-adapter`
  指引）与 `global-target-status-ui.test.ts` 用例。
- 品牌资产字节级一致（SHA-256 `a5624bc3…0858a`）；README 条目修正为核验过的 Press Kit URL
  `https://pi.dev/press-kit` 并补 MIT 归属。
- 文档同步：`docs/maintainers/adding-tool-adapter.md` §9 的 Pi 行改为已核验结论（迁移号、
  能力矩阵、诊断码、共享文件禁写）；README 的工具列表、能力表与说明段补 Pi。

### 阶段 6（质量门与评审）

- `cargo test`（lib 411 passed / 0 failed，integration + E2E 全绿）、`cargo fmt --check`、
  `cargo clippy --all-targets -- -D warnings`、`pnpm typecheck`、`pnpm test --run`（38 files /
  331 tests）、`pnpm bindings:check`、`git diff --check` 全绿。
- `pnpm format:check` / `pnpm lint` 仅剩基线既存失败：未跟踪的 `.pi/`（Trellis/Pi 扩展产物，
  不在 tsconfig project service 内、Prettier 未忽略），与 Pi 接入无关；未顺手修复。
- 事实核查：随机抽取 3 条结论对照 `research/verified-contract.md` 的官方 URL 复核 ——
  (a) `getPiGlobalConfigPath()` 尊重 `PI_CODING_AGENT_DIR`（本阶段新增自动 smoke 直接验证）；
  (b) `models.json` 的 provider 条目级合并与内置模型合并语义（codec 单测 + E2E 保留未知字段）；
  (c) `mcpServers` 容器与项目 `.pi/mcp.json` 优先级（E2E 遮蔽用例 + 隔离 smoke）。
- 6.3 的独立 `trellis-check` 未能执行：本机子代理派发不可用（见本节开头），经用户确认改为
  主会话自查（上述质量门 + `pi-adapter-guidelines.md` 的 AC 逐条对照），因此本任务缺少一次
  独立跨层复核，列为残余风险。
