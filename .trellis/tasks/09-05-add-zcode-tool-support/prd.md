# PRD: 新增 ZCode 工具全套支持（Provider/Prompt/MCP/Skills）

## Goal

为 EasyToAgents 新增 ZCode 作为可选启用的第四个原生工具（与 Cursor 同为设置里的可选项），能力范围是**全套**：Provider（接口配置）、Prompt（提示词）、MCP、Skills；只有官方确实没有对应配置面的能力才排除。

## 背景

`docs/maintainers/adding-tool-adapter.md` 第 9 节将 ZCode 列为"待调研候选"，要求在证据、能力矩阵和回滚边界审核通过后才新增 Tool 值。本机（macOS）安装有 ZCode，可作为证据来源完成该核验。

## 证据（本机核验，2026-09-05）

- 安装身份：`/Applications/ZCode.app`，Bundle ID `dev.zcode.app`，`CFBundleShortVersionString` = `3.11.2`（semver 可校验）。无 `zcode` CLI 可执行文件在 PATH 上（Resources 内有 `zcode-cli`，属应用内部二进制，不作为探针依据）。
- Provider（接口配置）：`~/.zcode/v2/config.json` 顶层 `provider` 对象，键为 provider id（如 `builtin:zai-start-plan`），值形如：
  `{ name, kind, options: { apiKey, baseURL, apiKeyRequired? }, enabled?, source, models: { <model>: {...} }, systemDisabledReason? }`。
  `apiKey` 是敏感字段。`models`/`source`/`systemDisabledReason` 为应用自管数据，写入时必须保留。
- Prompt（指令文件，官方 zcode-configuration-guide）：用户级 `~/.zcode/AGENTS.md`，项目级 `<repo>/AGENTS.md`（Markdown，整文档）。
- MCP（官方 zcode-configuration-guide）：用户级 `~/.zcode/cli/config.json` → 嵌套键 `mcp.servers`；项目级 `<repo>/.zcode/config.json` → 嵌套键 `mcp.servers`。JSON。（`~/.agents/mcp.json` 的 `mcpServers` 是兼容回退，MVP 不写。）
- Skills（官方 zcode-configuration-guide）：用户级 `~/.zcode/skills/`，项目级 `<repo>/.zcode/skills/`；目录 + SKILL.md，与 Claude/Codex/Cursor 同型。
- 同文件混有非受管内容：`~/.zcode/cli/config.json` 另有 `hooks`、插件状态；`.zcode/config.json` 另有 `hooks`。必须用受管选择器（`mcp/servers`）只接管 MCP 子树，其余字段原样保留。

## 能力矩阵（ZCode）

| Artifact | Global | Project | Import | Apply | 证据 |
| --- | --- | --- | --- | --- | --- |
| Provider | Supported | N/A | Supported | Supported | 本机 `~/.zcode/v2/config.json` 实测 schema |
| Prompt | Supported | Supported | Supported | Supported | 官方配置指南 AGENTS.md |
| MCP | Supported | Supported | Supported | Supported | 官方配置指南 + 本机文件 |
| Skills | Supported | Supported | Supported | Supported | 官方配置指南 + 本机目录布局 |

诊断码沿用既有命名风格：`ZCODE_INSTALLATION_PROBE_UNSUPPORTED` 等。

## Requirements

1. `Tool` 枚举新增 `Zcode => "zcode"`（稳定序列化值，入库不可再改名）。
2. ZCode 安装探针：desktop bundle 校验（Bundle ID `dev.zcode.app`、大小受限 Info.plist 读取、版本 semver 校验），模式与 Cursor 探针一致；候选路径显式注入。CLI 不作为探针证据。
3. ZCode Adapter 描述符矩阵：上表 4 类 artifact，全 Supported（未安装时 ToolNotInstalled）；敏感 selector 覆盖 `provider/*/options/apiKey`、`mcp/servers/*/headers|env|auth`；Skills `SymlinkDirectory`/`ManagedChildrenOnly`；Provider 仅 Global。
4. 注册：`PROFILE_TOOLS`、`ASSIGNABLE_MCP_TOOLS`、`ASSIGNABLE_SKILL_TOOLS` 加入 ZCode；`ToolAvailability` 结构体加 `zcode` 字段。
5. DB 前向迁移 `0013`：放宽 mcp/skill 分配与导入预览、`managed_targets`、`provider_profiles`、`prompt_project_assignments`、`profile_import_previews` 的 tool CHECK 增加 `zcode`（managed_targets 的 zcode 不加 artifact 限制，因为四类全支持）；`prompt_profiles` 增加 `is_active_zcode` 列 + 每工具至多一份生效的部分唯一索引；`skill_import_previews` 的 source_kind 增加 `zcode_home`/`zcode_agents`。
6. 服务层：provider 发现/校验/渲染（ZCode 分支）、prompt 全局与项目分配（is_active_zcode）、MCP/Skills 分配与导入（新增 import 来源）、项目登记目标链、overview 统计与恢复路径、sync apply 的 adapter 注册表。
7. 前端：重新生成 bindings；`tool-metadata.ts` 增加 zcode（label "ZCode"、图标、profileRoute `/zcode`、全能力 true）；设置页 ENABLED_TOOL_ORDER 加 zcode；skills 导入来源文案；品牌图标 `src/assets/brand/`。
8. 默认 enabled_tools 保持 `[claude, codex]`：ZCode 与 Cursor 一样是可选启用项，不默认开启。
9. README 工具列表与能力说明更新；`docs/maintainers/adding-tool-adapter.md` 第 9 节 ZCode 行更新为已核验状态。

## Acceptance Criteria

- [ ] `pnpm check`（format/lint/typecheck/test/bindings:check/rust:check）全绿。
- [ ] 迁移测试：从 0012 升级后旧行保留、新 schema 可插入 zcode 行、unsupported canary 仍被拒绝。
- [ ] 探针测试：成功/缺失/错误 Bundle ID/异常版本/链接路径矩阵。
- [ ] Adapter 测试：global/project 描述符矩阵、ownership、敏感 selector、allowed root。
- [ ] 前端 metadata 集合测试与设置页测试覆盖 zcode。
- [ ] 不开启 ZCode 时现有行为零变化（默认 enabled_tools 不含 zcode）。

## 非目标

- 不写 `~/.agents/mcp.json` 兼容回退路径。
- 不管理 `models`/`source`/`systemDisabledReason` 等 ZCode 应用自管字段（只保留，不受管）。
- 不支持 CLI 探针（无官方 PATH 安装合同）。
- 不为其他待调研工具（Pi 等）开放任何能力。
