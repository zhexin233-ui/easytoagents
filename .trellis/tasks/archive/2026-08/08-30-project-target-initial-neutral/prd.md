# 项目目标初始未纳管状态中性化

## Goal

当项目受管目标文件存在、但 EasyToAgents 从未写入（无基线）、文件中不含任何受管条目、且本项目没有需要写入的项目级配置时，项目详情页不再以琥珀色警告「△ 非受管变更 / 诊断：EXTERNAL_NON_OWNED_CHANGE」呈现，而是以中性信息态呈现（徽章「未纳管」+ 说明文案），消除"仅有全局继承配置的项目"页面上扰人且易误解的警告。

用户价值：picSlicer 这类只继承全局 MCP 的项目，其详情页不再显示看似出错实际无害的警告状态。

## Background（已确认事实）

- 判定链路：`src-tauri/src/projects/service.rs:484-515` `assess_managed_target` 在计算 ownership 时把全局继承的启用 MCP 一并计入，因此项目即使没有项目级 MCP，其 `config.toml` 仍会被巡检。
- 漂移分支：`src-tauri/src/sync/mod.rs:418-422`，基线 `full_hash`/`managed_hash` 均为空 + 观测到的 managed projection 为空 → `ExternalNonOwnedChange` + `EXTERNAL_NON_OWNED_CHANGE`，`can_merge = true`。
- 项目未追加任何 MCP 时，应用不会创建项目配置文件（预览提示见 `src/features/projects/project-detail-page.tsx:460`「该项目只有全局继承 MCP，不需要创建项目配置文件。」），因此该分支下基线永远为空，状态长期挂警告。
- 实例：`/Users/zhexin/github/picSlicer/.codex/config.toml` 仅含 Codex CLI 自身配置（`project_doc_fallback_filenames`、`shell_environment_policy`），无任何 `[mcp_servers.*]` 条目。
- 前端呈现：`src/features/projects/project-detail-page.tsx:218` 直接 `<SyncStatusBadge status={target.status} />`（无 label/tone 覆盖），`:223-224` 原样输出「诊断：EXTERNAL_NON_OWNED_CHANGE」。
- `SyncStatusBadge` 仅有 blocked/warning/success 三种 tone（`src/components/sync-status-badge.tsx:36`），`external_non_owned_change` 被归入 warning（`:52-55`）。
- 既有先例：Skills 全局目标用后端诊断码 `SKILL_TARGET_INITIAL_EMPTY` / `SKILL_TARGET_INITIAL_UNMANAGED` / `SKILL_TARGET_INITIAL_SYNC_PENDING`（`src-tauri/src/skills/service.rs:415-447`，门条件同样是"基线双空 + 无既有条目 + can_merge"）+ 前端 `globalTargetStatusPresentation`（`src/lib/global-target-status-ui.ts:22-53`）映射为友好文案；状态枚举值保持不变。
- `assess_managed_target` 同一路径同时服务 MCP 与 Skills 两类项目目标；`project_targets` 过滤出这两类（`src-tauri/src/projects/service.rs:338-349`）。
- 总览/仪表盘不渲染项目目标状态徽章（`src/features/dashboard/dashboard-page.tsx` 只展示同步运行状态），本任务不影响总览页。

## Requirements

- R1 后端：当 managed assessment 满足「状态为 `ExternalNonOwnedChange` 且 `can_merge = true` 且基线双哈希为空且无既有受管条目」时，项目目标状态的诊断码改用新的中性码 `PROJECT_TARGET_INITIAL_UNMANAGED`（命名对齐 `SKILL_TARGET_INITIAL_UNMANAGED`）。同步状态枚举值保持 `external_non_owned_change` 不变。MCP 与 Skills 两类项目目标统一生效。
- R2 前端：`SyncStatusBadge` 新增第 4 种中性 tone（muted，灰调，复用 `OptionTag` muted 配色风格，见 `src/features/projects/project-detail-page.tsx:804-827`）。项目详情页目标卡片对该新诊断码映射为：中性徽章「未纳管」+ 一行说明（如「该文件由外部维护；本项目暂无需要写入的项目级配置，全局配置持续继承。」），且不再输出原始「诊断：…」行；其他诊断码的呈现维持现状。
- R3 测试：后端为该分支补诊断码断言（projects/service 或 sync 层测试）；前端更新 `project-detail-page.test.tsx`，覆盖"目标文件存在但未纳管"时渲染中性徽章与说明、不出现「非受管变更」与原始诊断码。

## Acceptance Criteria

- [ ] Given 项目仅有全局继承 MCP、项目 `.codex/config.toml` 存在且无任何受管条目：项目详情页该目标卡片显示中性徽章「未纳管」与说明文案，不显示「△ 非受管变更」，不显示原始诊断码文本。
- [ ] Skills 项目目标命中同条件（目录存在、无基线、无受管条目）时同样中性呈现。
- [ ] 基线已建立后的真实外部改写（`sync/mod.rs:413-417` 第一分支，managed 哈希一致、full 哈希变化）仍显示「△ 非受管变更」警告，行为不变。
- [ ] 该中性状态下预览与追加流程不受影响：追加项目级 MCP 后照常合并写入既有文件并转为「已同步」。
- [ ] 全局目标（MCP/Skills）的状态展示维持现状，不受本任务影响。
- [ ] `pnpm test`（前端）与 `cargo test`（src-tauri）相关范围全部通过。

## Out of Scope

- 不新增 `SyncStatus` 枚举变体，不改 DB `last_status` 取值、不再生成交互绑定（`src/bindings/commands.ts` 的 `SyncStatus` 类型不变）。
- 不改变全局目标的任何展示（全局 config.toml 从未同步时的现状保持原样）。
- 不提供"主动接管外部文件"的新动作（`readopt_mcp_target` 需已有基线，维持现状）。
- 总览/仪表盘无改动。

## Key Decisions

- D1 采用「诊断码 + 前端呈现映射」方案（对齐 Skills 先例），而非新增 `SyncStatus` 变体：后者需改动 6 个 Rust 文件的穷尽匹配、bindings 再生成、badge 穷尽 Record 与 e2e，且会牵动 Skills 既有 INITIAL 流程（`global-target-status-ui.ts:22-53` 依赖 `status === "external_non_owned_change"`），收益不成比例。
- D2 中性文案定为「未纳管」，tone 为 muted 灰调（区别于 warning 琥珀、blocked 红、success 绿）。
- D3 `can_merge` 维持 `true`：该状态下追加项目级 MCP 仍可安全合并写入。

## Risks / Deferred

- 若未来仍希望引入独立状态枚举值（语义更精确），需一并迁移 Skills 的 `SKILL_TARGET_INITIAL_*` 门条件与 `globalTargetStatusPresentation` 的状态匹配，作为后续独立任务。
- `PROJECT_TARGET_INITIAL_UNMANAGED` 仅是展示层语义；DB `diagnostic_code`/`last_status` 存储值随之变化属于既有字段用法，无迁移。

## Open Questions

（无阻塞项；中性文案「未纳管」与说明措辞在最终规划摘要中供确认。）
