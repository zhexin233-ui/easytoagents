# 根因：项目级 Agents 视图不显示项目原生资源

调研日期：2026-09-13。所有行号以当前 `main`（eb45750）为准。

## 1. 现象

项目详情页切到 Agents 视图后，页面只显示"<工具> Agents 项目追加"分区；
`.claude/agents/*.md`、`.codex/agents/*.toml`、`.cursor/agents/*.md`、
`.opencode/agents/*.md` 里用户自行维护的子代理文件不会出现在"项目原生资源"分区，
而 MCP / Skill / Hooks 视图都会列出对应原生条目。

## 2. 根因链路

这是 `09-12-add-agents-management` 明确写入 PRD 的非目标（"项目原生资源登记对子代理
目录中非受管文件的只读扫描、禁用与恢复"），不是回归。Agent 在四处被显式排除：

| 层 | 位置 | 排除方式 |
| --- | --- | --- |
| 后端描述符过滤 | `src-tauri/src/projects/native_resources.rs:286-292` `supported_project_descriptors` | 只保留 `Mcp \| Skill \| Hook` |
| 后端观测分支 | 同文件 `observe_items`（约 L432-440） | `Prompt \| Provider \| Agent => Ok(None)`，注释写明"agents 的目录内非受管文件按 PRD 非目标处理" |
| 后端动作所有权 | 同文件 `native_ownership`（约 L985-1005） | `Agent` 与 `Prompt/Provider` 一起 fail closed |
| 后端解析 | 同文件 `parse_artifact` | 没有 `"agent"` 分支，`safe_summary` 对 Agent 返回 `{}` |
| 前端类型 | `src/lib/projects-api.ts:13-21` | `ProjectResourceKind = Exclude<ArtifactKind, "provider" \| "prompt" \| "agent">`，注释说明 Agent 不是整项目原生资源 |
| 前端页面 | `src/features/projects/detail/page.tsx:381-390` | `activeResourceView !== "agent"` 时才挂载 `ProjectNativeResources` |
| 数据库 | `src-tauri/src/db/migrations/0021_project_native_hook_entries.sql` | `project_native_resources.entry_type` CHECK 只允许 `mcp_entry / directory / symlink / hook_entry` |

## 3. 与 Hooks 补齐任务的差异（09-12-fix-project-native-hooks-empty）

Hooks 任务是同类缺口的先例（只读识别、动作 fail closed、迁移放宽 CHECK），本任务
可以完整复用其骨架，但 Agents 有两处本质差异：

### 3.1 描述符是目录，不是文件

`adapter.discover()` 为 Agent 产出的是**目录级** descriptor（`path = <root>/.claude/agents`，
`format = Markdown/Toml`，`managed_selectors` 为空）。设计文档明确写了"目录 descriptor 本身
不能直接 `scan_target`（会把目录当文件读）"，Agents 服务通过
`descriptor.for_agent_file(name, ext)`（`src-tauri/src/adapters/discovery.rs:294`）派生文件级
descriptor 后才做 scan。

因此原生观测不能像 MCP/Hook 那样对 descriptor 直接 `scan_target`，也不能像 Skill 那样依赖
`SymlinkDirectory` 格式的目录扫描；需要自行 `read_dir` 并按 `agent_file_extension(tool)`
（`discovery.rs:17`）过滤文件。

### 3.2 目标身份行与 Agents 同步的冲突（关键安全约束）

`project_native_resources.target_id` 外键指向 `managed_targets`，登记时由
`repository::insert_project_target_identity_in`（`src-tauri/src/db/native_resources.rs:331`）按
`(project_id, tool, artifact_kind, scope='project', target_path)` 找到或创建一行身份行。
对 MCP/Skill/Hook 而言，这行与中央同步用的目标行是**同一行**（同一路径）。

Agents 同步则按"一个受管文件 = 一行 managed_targets"建模
（`src-tauri/src/db/agents.rs:4`），`list_agent_managed_targets`（`db/agents.rs:626`）按
`(tool, artifact_kind='agent', scope, project_id)` 列出全部行，**不区分路径是目录还是文件、
也不区分 baseline 是否为空**。`prepare_agents_sync`（`src-tauri/src/agents/service_core.rs:605-735`）：

1. `existing_rows` 非空时不会走"空集不建运行"的早退；
2. 删除候选循环对每个"文件名不在 desired 中"的行调用 `file_stem_of(target_path)` →
   `for_agent_file(stem, ext)` → `scan_target`，文件可读即 `delete_target = true`。

如果原生登记为 `<root>/.claude/agents` 创建目录级身份行，`file_stem_of` 得到 `"agents"`，
派生路径为 `<root>/.claude/agents/agents.md`；只要用户恰好有一个名为 `agents.md` 的原生
文件，下一次 Agents 同步就会把它列为删除目标（且 baseline 绑定在目录行上）。

若改为按文件创建身份行（与 Agents 同步共用行），问题更严重：每个原生文件都会出现在
`existing_rows` 中、文件名不在 desired 中、文件可读 → **同步会删除用户所有原生 agent
文件**。因此身份行必须是目录级，且 Agents 同步必须显式忽略"路径不是本目录内合法文件"
的行。`agent_file_descriptor(directory_descriptor, target_path, tool)`
（`src-tauri/src/agents/service_native.rs:79`）已经实现了这个校验（派生路径必须与
`target_path` 完全一致），可直接复用作为过滤器。

其他读取 `list_agent_managed_targets` 的入口：

- `list_global_agent_target_statuses`（`service_core.rs:260`）只查 `Scope::Global`，原生
  登记只产生项目作用域行，不受影响；
- `readopt_agent_target`（`service_core.rs:460-470`）按精确 `target_path` 匹配并再经
  `agent_file_descriptor` 校验，目录行无法通过；
- 项目详情 `project.targets`（`src-tauri/src/projects/service.rs:631-633`）对 Agent 直接
  `return Ok(None)`，目录行不会出现在"工具配置状态"卡片中。

## 4. 可复用的既有能力

| 能力 | 位置 | 备注 |
| --- | --- | --- |
| 目录级 descriptor 发现 | `adapter.discover()` + `supported_project_descriptors` | 只需放开 `ArtifactKind::Agent`；ZCode 项目级 `path: None` 且 capability Unsupported，会被现有 `path.is_some()` / `observe_descriptor` 门禁自然排除 |
| 文件扩展名 | `adapters::agent_file_extension(tool)` | Codex `toml`，其余 `md` |
| 文件级路径校验 | `agents::service_native::agent_file_descriptor` | 目前 `pub(super)`，需提升可见性或在 `agents/mod.rs` 重导出 |
| Markdown / TOML 解析 | `agents::service_native::parse_markdown_agent_file(text, tool)` / `parse_codex_agent_file(text)` | `pub(super)`，返回 `ParsedAgentFile { name, description, prompt, dropped_fields, retained }`；解析失败返回稳定诊断码常量（`AGENT_FRONTMATTER_INVALID` 等） |
| 文件大小上限 | `agents/import.rs:27` `MAX_AGENT_FILE_BYTES = 512 KiB` | 私有常量，导入时超限标记 `AGENT_FILE_TOO_LARGE` |
| 内容哈希 | `sync::hash_bytes` / `sync::hash_json` | 64 位小写十六进制，满足 `observed_item_hash` CHECK |
| 凭据判定 | `security::contains_detectable_secret(field, text)` | Hook 命令脱敏已用 |
| 展示索引 | `native_resources.rs` `HookDisplayIndex` | 仅在一次 list 调用的内存中，不落库；可泛化为通用索引供 Agent 复用 |
| 中央所有权 | `db::agents::list_agent_managed_targets(tool, Scope::Project, Some(project_id))` | 返回 `target_path` + `baseline_full_hash/managed_hash`；0012 迁移注释明确"空 baseline 不构成 ownership" |
| 迁移先例 | `0021_project_native_hook_entries.sql` + `db/tests.rs:2405` | 12 步表重建放宽 CHECK 的规范化先例 |
| 前端只读行 | `src/features/projects/detail/native-resources.tsx` `HookSummary` | Hook 条目隐藏操作按钮、渲染 safeSummary 的模式 |

## 5. 结论

- 本任务属于"补齐同构能力"而非修 bug：需要迁移 + 后端观测 + 前端展示三层改动，且
  必须附带 Agents 同步的隔离守卫与对应测试。
- 禁用 / 恢复不纳入本任务：Agent 文件是整文件目标，技术上可行，但需要在
  `sync::NativeResourceEntryType` 与 apply/restore 链路新增整文件快照与恢复分支，
  改动面在最敏感的写入层，应另立任务。
