# 修复 Cursor 全局/项目级提示词不可配置问题

## Goal

为 Cursor 开放提示词（Prompt/Rules）能力：用户可以在 EasyToAgents 中为 Cursor 创建提示词档案、
将其设为 Cursor 全局生效提示词，或在项目内分配项目级提示词，由应用以官方文件合同写入并纳管。
修复「Cursor 配置不了全局提示词和项目级提示词」的问题。

## 背景（官方证据，2026-09-06 核验）

当前 Cursor 的 Prompt 在 adapter 中被显式标记为 `CURSOR_PROMPT_UNSUPPORTED`（global/project 均无目标路径），
`db/profiles.rs`、`profiles/service.rs` 与前端 `tool-metadata.ts` 全链路 fail closed。这是
capability-first 设计下的历史状态：当时缺乏官方文件合同证据。现已核验官方来源：

1. `https://cursor.com/help/customization/rules`（"Where are rules stored?" 小节，访问日期 2026-09-06）：
   - **Project rules** 存储于项目内 `.cursor/rules/`，git 版本管理；
   - **User rule files** 存储于 `~/.cursor/rules`（Windows: `%USERPROFILE%\.cursor\rules`），
     仅保存在本机、不随账号同步 —— 即官方明确的全局规则文件目录。
2. `https://cursor.com/docs/rules`（访问日期 2026-09-06）：
   - 项目规则必须使用 `.mdc` 扩展名；纯 `.md` 因没有 frontmatter 会被规则系统忽略；
   - frontmatter 字段 `description` / `globs` / `alwaysApply`；`alwaysApply: true` 表示
     每次会话都包含（与本应用「提示词档案 = 常驻指令」的语义一致）。

## Requirements

### R1 Cursor 提示词档案成为一等能力

- `Tool::Cursor` 加入 Provider/Prompt 工具集合（`PROFILE_TOOLS`）；Provider 能力保持 Unsupported 不变。
- 提示词档案 CRUD 与其他工具一致；每工具至多一份全局生效档案（`is_active_cursor` 启用位）。

### R2 官方文件合同

- 全局目标：`~/.cursor/rules/easytoagents.mdc`。
- 项目目标：`<登记项目根>/.cursor/rules/easytoagents.mdc`。
- 写入格式：`.mdc` = 固定 frontmatter（`alwaysApply: true`）+ 档案正文；观测/导入时剥离
  frontmatter，档案库中只保存纯正文（与其他工具共享的正文语义一致）。
- 规则目录内其他用户自建 `.mdc` 文件不属于受管范围（单文件整文档接管，不是目录接管）。

### R3 全链路一致

- 全局启用/停用、项目分配/解除、导入（发现 → 预览 → 确认）、Preview/Apply、漂移检测、
  恢复（Restore）与快照链路对 Cursor 提示词可用，语义与 Claude/Codex/ZCode 完全一致。
- 数据库前向迁移遵循既有 writable_schema 原地修订模式；不放宽 provider 相关 CHECK。
- 前端按 metadata 驱动自动生效：提示词页出现 Cursor 页签、项目详情页出现 Cursor 提示词区、
  Dashboard 显示 Cursor 生效提示词名；`/cursor` 工具页展示状态与提示词入口（无 Provider 面板）。

### R4 Fail-closed 边界保持

- Cursor Provider / API Key / 模型仍全程拒绝（`CURSOR_PROVIDER_UNSUPPORTED`）。
- 未核验的路径不得猜测；`.cursor/rules` 目录由 Apply 按需创建，仅创建必要父目录。

## Acceptance Criteria

- [x] AC1 Cursor 适配器 discover 返回 supported 的 Prompt descriptor（global + project），
      路径分别为 `~/.cursor/rules/easytoagents.mdc` 与 `<root>/.cursor/rules/easytoagents.mdc`；
      Provider descriptor 保持 unsupported。
- [x] AC2 迁移 `0017_cursor_prompt_support.sql` 从 schema v16 升级后：`prompt_profiles` 具备
      `is_active_cursor` 列与部分唯一索引（每工具至多一份生效）；`managed_targets` 允许
      cursor×prompt；`prompt_project_assignments`、`profile_import_previews` 接受 `'cursor'`；
      provider_profiles 的 CHECK 保持拒绝 `'cursor'`；canary 测试通过。
- [x] AC3 `TargetFormat::CursorMdc`：渲染输出 `---\nalwaysApply: true\n---\n\n<body>`；
      观测/导入剥离 frontmatter 得到纯正文；无 frontmatter 的文件正文按原文处理。
- [x] AC4 通过服务层可为 Cursor 完成：全局启用（写全局文件）、项目分配（写项目文件）、
      导入既有 `.mdc`（剥离 frontmatter 入库）、解除分配与停用（文件保留、停止纳管）。
- [x] AC5 Dashboard 的 Cursor 卡片显示生效提示词名；提示词页有 Cursor 页签；项目详情页
      Cursor 工具下出现提示词分配区。
- [x] AC6 全量质量门通过：`pnpm format:check`、`pnpm lint`、`pnpm typecheck`、
      `pnpm test --run`、`pnpm bindings:check`、`pnpm rust:check`、`pnpm check`、
      `git diff --check`。
- [x] AC7 `docs/maintainers/adding-tool-adapter.md` 能力矩阵更新 Cursor Prompt 为 Supported
      并记录官方证据 URL 与核验日期。

## Constraints

- 简体中文注释/commit；不修改历史迁移 SQL；不做逆向路径写入。
- 受管文件名固定为 `easytoagents.mdc`（与其他工具「单文件整文档接管」模式对齐）。
