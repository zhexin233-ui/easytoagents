# 新增 Cursor 产品扩展与工具接入文档

## Goal

把 Cursor 加入 EasyToAgents，作为用户可以按资源选择的第三种工具。Cursor MVP 只管理用户级/项目级 MCP 与 Skills；同时提供一份维护者文档，使后续接入 Pi、ZCode 等工具时能够按统一的 capability、Adapter、迁移、界面和验证流程实施。

产品价值：用户可以在现有中央 MCP/Skills 库中勾选 Cursor，并继续获得预览、冲突保护、原子写入、快照与恢复，而无需手工维护 Cursor 的 JSON 和 Skill 目录。

## Background

- EasyToAgents 当前正式支持 Claude Code 与 Codex，工具枚举、数据库 CHECK、Adapter registry 和前端选择器均存在二元假设。
- 用户已确认这是产品运行时扩展，不是仓库自身 `.cursor/` 开发配置调整。
- 用户已确认 Cursor 的 Provider、API Key、模型、用户级 Prompt 和项目级 Rules/Prompt 均不纳入支持。
- Cursor 官方公开的 MCP 目标是 `~/.cursor/mcp.json` 与 `<project>/.cursor/mcp.json`，顶层容器为 `mcpServers`。
- Cursor 官方公开的 Skills 目标包括 `~/.cursor/skills/` 与 `<project>/.cursor/skills/`；本任务使用 Cursor 专属目录，避免与 Codex 的目标产生多工具所有权冲突。
- Cursor 官方 macOS 安装为 `.dmg` 应用，生产 Bundle ID 为 `com.todesktop.230313mzl4w4u92`；Cursor CLI 是独立的 `agent` 命令。
- 官方证据与命令记录在 `research/cursor-official-config.md`，仓库影响面记录在 `research/repository-impact.md`。

## Requirements

### R1. Cursor 工具与能力边界

- 新增稳定工具值 `cursor`，贯穿 Rust 领域类型、生成的 TypeScript 合同、数据库解码、状态 DTO 与 UI 元数据。
- Cursor capability matrix 固定为：
  - MCP：用户级、项目级均支持。
  - Skills：用户级、项目级均支持。
  - Provider/API Key/模型：不支持。
  - Prompt/Rules：用户级、项目级均不支持。
- 未支持能力必须返回稳定 `unsupported` capability/错误码并 fail closed，不允许猜测 Cursor 私有存储或生成可 Apply 的预览。

### R2. 安装发现

- 在 macOS 13+ 只读探测 Cursor 生产应用 Bundle ID，并读取受约束的版本信息。
- 可把官方 `agent --version` 作为 Cursor CLI 的补充探测，但不得把缺少 CLI 等同于未安装桌面应用。
- 探测失败、版本格式不可信或路径异常时返回 `unavailable`/`unsupported`，不得执行配置写入。

### R3. MCP 管理

- 用户级目标：`$HOME/.cursor/mcp.json`。
- 项目级目标：`<project>/.cursor/mcp.json`。
- 只拥有中央库已分配名称对应的 `mcpServers.<name>` 条目；未知条目与未知字段保持不变。
- 支持现有中央模型中的 `stdio` 与 `streamable_http`，按 Cursor 官方 JSON 字段渲染 `command`、`args`、`env`、`url`、`headers` 等内容。
- `headers`、`env`、`auth` 及扩展字段中的凭据继续脱敏，不能进入普通 DTO、日志或预览明文。
- 支持只读发现并显式导入已有 Cursor 用户级 MCP；导入不能隐式 Apply。

### R4. Skills 管理

- 用户级同步目标：`$HOME/.cursor/skills/<name>`。
- 项目级同步目标：`<project>/.cursor/skills/<name>`。
- 沿用中央 Skill 副本与逐名称受管符号链接模型；普通目录、外部链接、断链和链接逃逸保持冲突保护。
- 支持只读发现并显式导入 Cursor 用户级 Skills；`.agents/skills` 只可作为官方兼容的导入来源，不作为 Cursor 受管同步目标。
- Cursor 官方未明确承诺符号链接发现；实现完成时必须进行本机兼容验证。若无法证明可用，Cursor Skills 必须保持 `unsupported`，不得悄悄切换到未设计的 Copy 模式。

### R5. 可选分配与界面

- Cursor 出现在 MCP 与 Skills 的全局平台分配按钮、项目工具视图、导入入口、目标状态和总览信息中。
- 用户不勾选 Cursor 时，仅保留中央资源，不创建或修改任何 Cursor 目标。
- 勾选 Cursor 只更新中央分配意图；原生写入仍必须经过持久化 Preview/Apply。
- 首次引导仍只处理 Claude/Codex 的 Provider 与全局 Prompt，不增加无可操作内容的 Cursor Provider/Prompt 卡片。
- Claude/Codex 的工具档案导航保持不变；Cursor 不提供 Provider/Prompt CRUD 页面。Cursor 的支持范围通过总览、MCP/Skills 页面和清晰的 capability 文案呈现。
- 工具标签、图标、可分配能力和路由信息集中维护，删除散落的二元 label/icon 分支。

### R6. 数据与升级兼容

- 新增前向迁移，只放宽实际需要保存 Cursor 的 MCP、Skills、import preview 与 managed target 表；Provider/Prompt 表继续拒绝 Cursor 数据。
- 迁移必须保留已有 Claude/Codex 行、索引、外键、触发器和 schema version 幂等性。
- 现有数据库升级前备份、事务回滚与重复打开行为保持不变。

### R7. 同步安全与恢复

- Cursor 复用现有 descriptor、ownership、baseline、stale preview、原子替换、journal、snapshot 和 restore 合同。
- 用户级与项目级 allowed root 必须显式解析，不能回退到过宽目录。
- Cursor 未安装、目标格式错误、受管内容漂移或目标类型冲突时阻止 Apply，并返回稳定诊断。

### R8. 维护者文档

- 新增 `docs/maintainers/adding-tool-adapter.md`，并从 README 开发/贡献部分链接。
- 文档覆盖官方资料核验、capability matrix、Tool/Adapter/探针、数据库迁移、MCP/Skills/Profiles/Projects/Overview/Restore、生成 bindings、前端元数据、fixtures、测试和回滚点。
- Pi、ZCode 只作为“待官方核验”的接入示例，不在本任务中实现，也不猜测配置路径。

## Acceptance Criteria

- [x] **AC1**：Cursor 作为稳定 `cursor` 工具值出现在后端合同与生成绑定中；旧数据库可无损升级，只有 Cursor MCP/Skills 相关表接受该工具值。
- [x] **AC2**：应用能区分 Cursor 生产桌面应用与可选 CLI 探测结果；缺少 `agent` CLI 不会把已安装桌面应用误报为未安装。
- [x] **AC3**：MCP 中央资源可勾选 Cursor 全局目标，预览并应用到 `~/.cursor/mcp.json`；未勾选时不创建或修改该文件。
- [x] **AC4**：项目 MCP 可分配到 Cursor 并安全应用到 `<project>/.cursor/mcp.json`；外部条目、未知字段和敏感值保护与 Claude/Codex 一致。
- [x] **AC5**：Skills 中央资源可分配到 Cursor 用户级/项目级专属目录，且符号链接兼容性已得到本机验证；无法验证时能力保持显式不支持。
- [x] **AC6**：已有 Cursor 用户级 MCP 与 Skills 可以只读检测、显式选择并导入中央库，导入操作不隐式写回原生目标。
- [x] **AC7**：Cursor 出现在 MCP/Skills 的全局与项目分配、目标状态、导入和总览界面；工具文案与图标不再依赖 Claude/Codex 二元判断。
- [x] **AC8**：Cursor Provider/API Key/模型及所有 Prompt/Rules 不提供创建、导入、分配、Preview 或 Apply；直接调用相关命令也返回稳定 unsupported 错误。
- [x] **AC9**：Cursor 同步覆盖 Missing、InSync、外部非受管变化、外部受管变化、解析失败、目标类型冲突、过期预览、快照恢复等关键状态。
- [x] **AC10**：维护者文档能够指导新增另一工具，并对 Pi/ZCode 的未知项明确要求先核验官方资料。
- [x] **AC11**：Rust/前端/迁移/生成绑定测试与 `pnpm check`、`git diff --check` 全部通过，Claude/Codex 既有行为无回归。

## Out of Scope

- Cursor Provider、API Key、模型配置。
- Cursor 用户级 Prompt、项目级 Rules/Prompt，以及 `AGENTS.md`/`.cursor/rules` 写入。
- Cursor 私有设置存储、Cursor Settings UI 自动化、团队策略与 MCP Marketplace 安装。
- Cursor Nightly 或其他预发布渠道的正式支持。
- Pi、ZCode 或其他新工具的实际 Adapter 实现。
- 新增 Skills Copy 模式或跨平台安装包。

## Technical Notes

- 关键官方来源：
  - [Cursor Quickstart](https://cursor.com/docs/get-started/quickstart)
  - [Cursor Deployment Patterns](https://cursor.com/docs/enterprise/deployment-patterns)
  - [Cursor MCP](https://cursor.com/docs/mcp)
  - [Cursor Agent Skills](https://cursor.com/docs/skills)
  - [Cursor BYOK](https://cursor.com/help/account/bring-your-own-api-key)
- 本任务保持为一个实现任务：Cursor 的 Tool/迁移/Adapter/UI/文档共享同一 capability 合同，拆成子任务会增加跨任务合同漂移风险。
