# Design: ZCode 工具全套支持

## 边界与总体形状

ZCode 按 Codex 的"全能力工具"形状接入，但文件格式为 JSON（Provider/MCP）+ Markdown（Prompt）+ SymlinkDirectory（Skills）。安装探针按 Cursor 的 desktop-bundle-only 模式。

## 目标路径与所有权合同

| Artifact × Scope | 目标路径 | 格式 | managed_selector_roots | 敏感 selector |
| --- | --- | --- | --- | --- |
| Provider × Global | `~/.zcode/v2/config.json` | Json | `provider` | `provider/*/options/apiKey` |
| Prompt × Global | `~/.zcode/AGENTS.md` | Markdown | `$document` | — |
| Prompt × Project | `<root>/AGENTS.md` | Markdown | `$document` | — |
| MCP × Global | `~/.zcode/cli/config.json` | Json | `mcp/servers` | `mcp/servers/*/headers`, `*/env`, `*/auth` |
| MCP × Project | `<root>/.zcode/config.json` | Json | `mcp/servers` | 同上 |
| Skill × Global | `~/.zcode/skills` | SymlinkDirectory | `$children` | — |
| Skill × Project | `<root>/.zcode/skills` | SymlinkDirectory | `$children` | — |

关键点：

- Provider/MCP 与其他资源**共享文件**（v2/config.json 还有 models 等；cli/config.json 还有 hooks/plugins），必须使用 Selectors ownership 的 JSON 合并渲染（复用 `render_document` 的 Selectors 分支），受管子树之外的键原样保留。
- Provider 的受管写入粒度是 `provider/<id>` 整项 + `provider/<id>/options/apiKey` 敏感；`models`、`source`、`systemDisabledReason` 不受管——但 `provider/<id>` 整项选择器会覆盖整项。为避免 clobber 应用自管字段，ZCode Provider 的**项目级投影只写 options 子选择器**：ownership selectors 为 `provider/<id>/name`、`provider/<id>/kind`、`provider/<id>/options`、`provider/<id>/enabled`，从而 models/source 保留。渲染时 desired projection 由服务层按此拆分。
- MCP 容器键为嵌套 `mcp.servers`（JSON path 两段），与 Cursor 的顶层 `mcpServers` 不同；服务层构造中央 server 投影时按工具区分容器路径。
- Prompt 无 override 文件合同：`prompt_override = NotApplicable`。

## 探针

`app/tool_probe.rs`：

- `ZCODE_BUNDLE_ID = "dev.zcode.app"`；`zcode_app_paths = [/Applications/ZCode.app, ~/Applications/ZCode.app]`。
- 复用 `read_cursor_bundle_version` 抽象为通用 `read_app_bundle_version(app_path, bundle_id)`（改名为 `read_desktop_bundle_version`，Cursor/ZCode 共用），校验 CFBundleIdentifier、大小上限、semver。
- `ReleaseToolProbeResult` 增加 `zcode: ToolProbeOutcome`；`ToolAvailability` 增加 `zcode` 字段；`with_zcode_installation_version`。
- 探针输入 `ReleaseToolProbeInput` 增加 `zcode_app_paths`（显式注入，测试不读真实 HOME/PATH）。

## DB 迁移 0013_zcode_tool_support.sql

1. 六张 mcp/skill 分配与导入预览表：`('claude','codex','cursor')` → 追加 `'zcode'`（writable_schema 原地文本替换，限定表名+旧锚点）。
2. `managed_targets`：`tool IN ('claude','codex','cursor') AND (tool != 'cursor' OR artifact_kind IN ('mcp','skill'))` → 追加 `'zcode'`，**不加** artifact 限制（四类全支持）。
3. `provider_profiles`：tool CHECK `('claude','codex')` → 追加 `'zcode'`（provider_profiles.tool 是档案的工具绑定，UNIQUE(tool,name) 与 per-tool active 索引自动覆盖 zcode）。
4. `prompt_profiles`：`ADD COLUMN is_active_zcode INTEGER NOT NULL DEFAULT 0 CHECK(...)` + 部分唯一索引 `uq_prompt_profiles_one_active_zcode`。
5. `prompt_project_assignments`、`profile_import_previews`：tool CHECK 追加 `'zcode'`。
6. `skill_import_previews`：source_kind CHECK 追加 `zcode_home`、`zcode_agents`。
7. `mcp_import_previews`：tool CHECK 追加（同 1 组）。

回滚：代码回退不倒迁数据库；放宽后的 CHECK 对旧数据无破坏。

## 服务层改动点（按文件）

- `domain/mod.rs`：`Zcode => "zcode"` + 往返测试。
- `adapters/zcode/mod.rs`：`ZcodeAdapter` + 描述符矩阵测试。
- `adapters/mod.rs`：mod 声明、`PROFILE_TOOLS`/`ASSIGNABLE_*`、`ToolAvailability`、`ExplicitEnvironment::with_zcode_installation_version`、`tool_availability`/`installation_version` match 臂。
- `app/tool_probe.rs`：如上。
- `db/profiles.rs`：prompt 全局/项目分配的 `Tool::Zcode` 分支（is_active_zcode）；provider 查询按 tool 通用化。
- `db/mcp.rs`、`db/skills.rs`、`db/mcp_imports.rs`、`db/skill_imports.rs`：tool/source_kind 解析 match 增加 `"zcode"`。
- `mcp/service.rs`：adapter 注册表、全局 allowed root（`~/.zcode/cli/config.json` 的父目录 → `~/.zcode/cli`；项目 → `<root>/.zcode`）、容器键 `mcp/servers`、投影构造按工具。
- `skills/service.rs`、`skills/import.rs`：adapter 注册表、import 来源 `ZcodeHome`（`~/.zcode/skills`）、`ZcodeAgents`（`~/.agents/skills`，通用目录仅导入）。
- `profiles/service.rs`：`descriptor_for`/`tool_adapter` 注册；provider 发现 `discover_zcode_provider`（读 `provider` map：name/kind/options{apiKey,baseURL}/enabled）；校验与渲染分支；prompt import 目标路径 `~/.zcode/AGENTS.md`。
- `projects/service.rs`、`projects/native_resources.rs`：目标链与原生资源发现/恢复加 ZCode。
- `overview/mod.rs`：tool 解析、active provider 名（ZCode 读 provider map 的 enabled 项或按 is_active）、快照恢复 allowed root（ZCode 四类全支持，按 artifact 推导）。
- `sync/apply.rs`：`adapter_for` 注册 `ZCODE: ZcodeAdapter`。

## 前端

- `pnpm bindings:generate` 重生成（`Tool` 联合类型自动加 `"zcode"`）。
- `src/lib/tool-metadata.ts`：zcode 条目（label "ZCode"、icon `zcode-icon.svg`、profileRoute `/zcode`、capabilities 全 true）；`PROFILE_TOOLS`/`MCP_TOOLS`/`SKILL_TOOLS` 加 `"zcode"`；`DEFAULT_ENABLED_TOOLS` 不变。
- `src/assets/brand/zcode-icon.svg`：自行绘制（简洁字母 Z 标），`src/assets/brand/README.md` 记录。
- `src/features/settings/settings-dialog.tsx`：`ENABLED_TOOL_ORDER` 加 `"zcode"`。
- `src/features/skills/skill-import-dialog.tsx`：`zcode_home`/`zcode_agents` 来源文案。
- 各页面测试文件中的工具数组补 zcode。

## 兼容与回滚

- 默认 enabled_tools 不含 zcode：老用户升级后 UI 无新工具出现，直到主动在设置中启用。
- 迁移只放宽 CHECK、新增列（默认 0），对旧数据无破坏；unsupported canary（如 `"windsurf"`）仍被拒绝。
- 回滚顺序：UI/共享集合摘除 → service/registry → adapter/枚举；迁移保留。
