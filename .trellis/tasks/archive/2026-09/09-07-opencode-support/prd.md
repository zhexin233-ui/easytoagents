# 新增 OpenCode 支持

## 目标与用户价值

用户在 EasyToAgents 中管理 OpenCode 官方支持且能够验证的现有资源能力，沿用中央编辑、显式导入、分配、预览、应用和恢复流程。用户已选择完整能力接入，不限定为 MCP/Skills；本任务仍处于规划，需最终方案审核后实施。

## 背景与证据

- 当前 Tool 仅 Claude/Codex/Cursor/ZCode：`src-tauri/src/domain/mod.rs:47`。能力集合在 `src-tauri/src/adapters/mod.rs:27` 和 `src/lib/tool-metadata.ts:21`。
- 新工具合同见 `docs/maintainers/adding-tool-adapter.md`，按资源开放能力，提示词仅全局，未知能力关闭。
- 当前官方文档及本机 OpenCode 1.18.29 隔离验证已完成，权威汇总见 `research/verified-contract.md`。Provider、MCP 配置解析及全局/项目 Skill 链接发现通过；非产品端到端测试。

## 范围与需求

| 编号 | 资源/能力 | 首版行为 |
| --- | --- | --- |
| R1 | 工具入口 | OpenCode 可在工具设置、总览、首次检测及受支持资源页面中选择；展示真实安装/不可用/受阻状态 |
| R2 | Provider（全局） | 管理 API Key 渠道、端点、SDK 协议及默认模型；导入可明确识别且能无损表示的原生配置；保留非受管 provider、模型高级配置和其他设置 |
| R3 | 提示词（全局） | 导入/分配/同步 OpenCode 全局 AGENTS.md，沿用每工具一份生效提示词 |
| R4 | MCP（全局/项目） | local 与 remote 配置的中央分配、导入及原生状态管理、预览/应用/恢复；保留环境变量、headers、可表示扩展；停用或不可表示条目明确诊断 |
| R5 | Skills（全局/项目） | 全局来源导入、中央分配及受管链接同步，沿用首次接管确认与恢复；不合规 frontmatter 阻止 OpenCode 分配并给出原因 |
| R6 | 原生兼容性 | 支持 JSON/JSONC，保留非受管字段及注释；识别多来源覆盖，不能把仅文件同步成功描述为所有运行时配置均已生效 |
| R7 | 安全与回归 | 凭据脱敏、持久化预览失效检测、窄路径边界、失败回滚和恢复延续现有合同，原四工具不退化 |
| R8 | Hooks 状态 | OpenCode Hooks 通过插件回调实现，不兼容现有 command-only 模型；明确显示此能力暂不支持，阻止分配/导入/写入 |

## 不在范围内

- 新增通用 Plugins/Agents/Commands 管理器或生成命令到插件的桥接运行时。
- 项目级提示词/规则管理；不观察或改写项目 AGENTS.md/CLAUDE.md。
- Provider OAuth 登录、token 刷新、auth.json 或 mcp-auth.json 的写入/迁移；凭据引用不自动展开读取任意文件。
- 代替 OpenCode 连接模型/MCP、启动会话，或承诺所有 SDK 特有参数都可在表单中编辑。
- 修改组织远端配置、MDM 管理配置或其它工具兼容来源。

## 验收标准

- AC1（R1/R8）：工具开关与页面能力一致；未安装可诊断；Hooks 即使直接调用 RPC 也被拒绝。
- AC2（R2/R3）：新增/编辑/导入并启用 API Key 渠道或全局提示词，预览精确目标后应用；OpenCode 隔离环境能解析配置，非受管设置保留，普通 DTO/日志无密钥。
- AC3（R4）：local/remote 的导入 → 分配 → Preview → Apply → 漂移 → Restore 完整验证；未选/未知/停用原生条目不会被隐式纳管或删除。
- AC4（R5）：全局与项目受管 Skill 链接被实际 CLI 发现；原始来源不变；同名外部目录冲突、首次接管确认、恢复及非法 name/description 均有验证。
- AC5（R6/R7）：JSONC 注释/非受管语义保留；共享文件 Provider/MCP 连续或组合更新不互相覆盖；预览后文件、配置路径或覆盖来源变化导致旧预览失效。
- AC6（R6）：已知更高优先级配置冲突显示来源及诊断；不修改外部覆盖配置；提示重启 OpenCode 生效，文件状态与运行时覆盖区别明确。
- AC7（R7）：从 v18 前向升级，原数据、索引、外键及重开正常；项目 Prompt 和 OpenCode Hook 写入持续被拒绝；绑定生成、全套项目检查通过。

## 风险与验证边界

已验证版本为 CLI 1.18.29；Desktop 独立探针、JSONC 细粒度写入、配置覆盖与恢复是实施时的重点验收项。Desktop bundle 证据不足时显示探针不可确认，不猜 ID；不能把 CLI 缺失等同于 Desktop 未安装。若实现证据否定上述范围，应返回规划，不静默削减能力。
