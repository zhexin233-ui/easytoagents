# 新增提示词模块：全局与项目级提示词管理

## Goal

提示词模块统一管理**指令文档**（Markdown 提示词档案）：中央档案库 + 全局分配（Claude `~/.claude/CLAUDE.md`、Codex `$CODEX_HOME/AGENTS.md`，每工具一份生效）+ 项目级分配（显式分配，硬拷贝为项目根 `CLAUDE.md` / `AGENTS.md`，项目可随时自改）。交互模式参考 Skills（库 + 分配 + 预览同步）。

> 2026-08-30 用户澄清：提示词 = CLAUDE.md / AGENTS.md 指令文档，不是 `.claude/commands` 斜杠命令文件。此前基于命令文件库的规划作废重写。

## Background（已核实事实）

- **既有全局提示词功能**：`src-tauri/src/profiles/` + `prompt_profiles` 表（`0001_initial.sql:31`，内容存库，`is_active` 每工具唯一索引 `uq_prompt_profiles_one_active_per_tool`）+ 前端 `src/features/tool-profiles/prompt-panel.tsx`（provider 页内面板）。已具备：应用内新建/编辑/删除命名档案、切换生效、首次接管无损导入、整文档预览同步（`prepare_prompt_sync`）。**仅全局，无项目级。**
- **原生路径**：Claude 项目记忆 = 项目根 `CLAUDE.md`（另有父目录与全局 `~/.claude/CLAUDE.md` 分层）；Codex = 项目根 `AGENTS.md`（逐级目录查 `AGENTS.override.md` → `AGENTS.md`），全局 `~/.codex/AGENTS.md`（`AGENTS.override.md` 优先）。两工具**都原生支持项目级**。（archive/08-19-ai-config-desktop/research/{claude-memory,codex-agents-md}.md）
- **引擎现状**：`ArtifactKind::Prompt` 已有 `TargetFormat::Markdown` + `$document` 整文档选择器 + `ManagedOwnership::WholeDocument` 描述符（claude/codex 各一个全局目标）；apply 引擎对文件目标产生 `WriteFile` 变更（`sync/apply.rs`）——**项目级硬拷贝无需新增同步格式**，只需新增项目级描述符与分配投影。
- `src/features/prompts/` 为 `.gitkeep` 占位目录，是新模块的预留位置。
- Skills 模块的交互范式（中央列表/平台分配按钮/同步徽章/变更预览弹窗/项目详情资源页签）可复用为 UI 参考，但提示词对象是整文档而非文件库。

## Requirements

- **R1 中央档案库**：命名提示词档案（Markdown 正文），应用内新建/编辑/删除/重命名（沿用现有 profiles 能力），保留首次接管导入。
- **R2 全局分配**：每工具一份生效（现状行为保持）：Claude → `~/.claude/CLAUDE.md`，Codex → `$CODEX_HOME/AGENTS.md`；切换走既有预览同步链路。
- **R3 项目级分配（硬拷贝）**：在项目里显式为工具分配一份档案 → 应用时**复制**为项目根 `CLAUDE.md`（Claude）/ `AGENTS.md`（Codex）；此后文件归项目所有、可随时自行修改、不回写档案库；外部修改后同步预览呈现漂移，可重新应用覆盖（readopt 语义）。
- **R4 无全局继承**：全局生效档案不自动进入任何项目；项目分配完全显式。
- **R5 安全**：项目已有 `CLAUDE.md`/`AGENTS.md` 时，分配前必须在预览中明示将覆盖，并提供「先把现有内容导入为新档案」的路径；非受管内容与首次接管的无损语义沿用。
- **R6 UI（统一收纳）**：侧边栏新增「提示词」模块（工具页签 + 档案库新建/编辑/删除/全局生效 + 接管导入 + 同步状态）；provider 页（/claude、/codex）内的提示词面板迁除，只留渠道面板；项目详情页新增「提示词」资源页签（每工具选择档案 → 预览 → 应用 → 解除分配）。

## Acceptance Criteria

- [ ] **AC1 全局**：全局新建/切换生效档案 → `~/.claude/CLAUDE.md` / `$CODEX_HOME/AGENTS.md` 内容随之更新（预览确认或直接应用模式）；行为与现状等价。
- [ ] **AC2 项目分配**：为项目分配档案后，项目根出现 `CLAUDE.md`（Claude）/ `AGENTS.md`（Codex），内容与所选档案一致；文件为普通拷贝，可独立编辑。
- [ ] **AC3 漂移**：外部修改项目文件后，预览呈现「本地已修改」，可重新应用覆盖为档案内容；档案库与其他项目不受影响。
- [ ] **AC4 显式性**：全局生效不使任何项目出现新文件；未分配的项目无变更；解除分配后项目文件保留、仅停止纳管。
- [ ] **AC5 覆盖保护**：项目已存在目标文件时，分配预览明示将覆盖，并支持先导入现有内容为新档案。
- [ ] **AC6 收纳**：提示词管理入口唯一（侧边栏「提示词」+ 项目详情页签）；渠道页不再有提示词面板；总览与 onboarding 不回归。
- [ ] **AC7 质量**：`pnpm check` 全绿（含 bindings 一致性）。

## Out of Scope

- `.claude/commands` / `$CODEX_HOME/prompts` 斜杠命令文件管理（用户澄清不属于本模块）
- 项目文件反向 adopt 回写档案库（项目副本可自由漂移）
- 父目录链/子目录级记忆文件管理（只管项目根一个文件）
- `AGENTS.override.md` 的专门管理（作为原生层叠行为，文档中说明即可）

## Risks

- **R1（迁移）**：`managed_targets` 需重建以放开 CHECK（SQLite 不能改 CHECK），触及既有基线存储——按 12 步流程 + 事务 + 迁移断言，独立提交便于回滚（应用已有 database-backups 机制）。
- **R2（回归）**：全局同步路径与渠道页在改造中必须零回归——全局分支行为冻结、既有测试全保留。
- **R3（外部）**：Codex 对 `AGENTS.override.md`/fallback 文件名的层叠规则可能在工具侧演进，基线对账只管我们写入的那个文件。

## Key Decisions（已确认）

1. 【形态】提示词 = CLAUDE.md / AGENTS.md 指令文档（用户澄清，非 `.claude/commands` 命令文件）。
2. 【项目级硬拷贝】项目分配 = 复制为项目根文件，归项目所有、可随时自改、不回写档案库。
3. 【无全局继承】全局生效不自动进入项目；项目分配完全显式。
4. 【统一收纳】侧边栏新模块承载全部提示词管理（档案库 + 全局 + 同步）；provider 页内提示词面板迁除，避免双写。
5. 【后端不拆模块】prompt 逻辑留在 `profiles/`（与 provider 共享管线），项目级能力原位扩展；「新模块」落在前端与交互层。

## Out of Scope

- `.claude/commands` / `$CODEX_HOME/prompts` 斜杠命令文件管理（用户澄清不属于本模块）
- 项目文件反向 adopt 回写档案库（项目副本可自由漂移）
- 父目录链/子目录级记忆文件管理（只管项目根一个文件）
- `AGENTS.override.md` 的专门管理（作为原生层叠行为，状态提示沿用现有 `promptOverride`）
