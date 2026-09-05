# 接入新的工具 Adapter

EasyToAgents 以 capability 为先，不要求新工具复制 Claude 或 Codex 的全部能力。只有得到官方、可复现证据的资源，才能进入导入、Preview/Apply 和恢复链路；未知能力必须保持 `unsupported`，不得猜测私有路径或 schema。

## 1. 先建立证据与能力矩阵

开始改代码前，为每个 `artifact × scope × operation` 记录官方来源、验证日期和结论：

| Artifact       | Global  | Project | Import  | Apply   | 证据与诊断                      |
| -------------- | ------- | ------- | ------- | ------- | ------------------------------- |
| Provider       | Unknown | N/A     | Unknown | Unknown | 官方文件路径、schema、优先级    |
| Prompt / Rules | Unknown | Unknown | Unknown | Unknown | 用户级与项目级分别核验          |
| MCP            | Unknown | Unknown | Unknown | Unknown | 路径、容器、transport、敏感字段 |
| Skills         | Unknown | Unknown | Unknown | Unknown | 发现目录、嵌套规则、链接兼容性  |

状态只允许：

- `Supported`：官方合同明确，且本地 fixture 或实机 smoke 可以复现。
- `Unsupported`：官方明确不支持，或产品选择不纳入该能力。
- `Unknown`：证据不足；行为与 `Unsupported` 一样 fail closed，但保留后续调研入口。
- `ToolNotInstalled`：合同已知，但本机安装探针没有得到可信结果。

证据表至少包含官方 URL、页面标题、访问日期、稳定路径/格式、版本或渠道限制。第三方博客、论坛和逆向得到的私有存储不能单独授权写入。

Cursor 的当前矩阵是一个非对称示例：全局/项目 MCP 与 Skills 为 Supported；Provider、API Key、模型、全部 Prompt/Rules 为 Unsupported。不要因为 `Tool` 已存在就自动开放所有页面或数据库表。

## 2. 领域合同与 Adapter

1. 在 `src-tauri/src/domain/mod.rs` 为 `Tool` 添加稳定的小写序列化值和往返测试。值写入 SQLite 后不能随显示名称重命名。
2. 在 `src-tauri/src/adapters/<tool>/mod.rs` 实现 `ToolAdapter`。每个 descriptor 必须显式声明：
   - artifact、scope、project root 与目标路径；
   - `TargetFormat`、ownership selector、敏感 selector；
   - capability、policy、trust、prompt override 和 symlink policy。
3. Unsupported descriptor 不提供目标路径，并在任何 scan、path unwrap、Preview 持久化或 Apply 之前由服务入口拒绝。
4. 把 Adapter 注册到实际支持资源的 registry。共享集合位于 `src-tauri/src/adapters/mod.rs`：
   - `PROFILE_TOOLS` 只含 Provider/Prompt 工具；
   - `ASSIGNABLE_MCP_TOOLS`、`ASSIGNABLE_SKILL_TOOLS` 分别列出可分配工具。
5. 检索所有 `match Tool` 和二元分支；穷举分支必须明确处理新工具，不能用 `_` 把它误当成 Codex。

## 3. 安装探针与显式环境

探针只负责读取可信安装事实，不负责创建配置：

- macOS Desktop 优先校验生产 Bundle ID，再读取大小受限、类型受限的 `Info.plist` 版本。
- CLI 只能作为官方明确支持的补充证据；CLI 缺失不能否定已验证的桌面应用。
- 候选路径、命令、超时和环境全部通过显式输入注入，测试不得读取真实 HOME 或 PATH。
- 不可信 Bundle、异常版本、链接路径、超时和权限问题返回 `unsupported`/`unavailable`，不得降级为可写。

同步的 `allowed_root` 必须按 tool、artifact、scope 精确推导。全局配置根不能回退到整个 HOME；项目目标只能使用已 canonicalize 的登记项目根。缺失根、链接逃逸或类型冲突必须阻止 Apply。

## 4. 数据库迁移

只能追加前向迁移，不能改历史 SQL。逐表回答“这个 artifact 是否真的会保存新 Tool”：

- 仅放宽 Supported artifact 的 assignment、import preview 和 managed target。
- Provider/Prompt 不受支持时，其表继续拒绝该工具值，形成服务层之外的第二道边界。
- SQLite CHECK 需要 `writable_schema` 时，必须以表名和精确旧锚点限定替换；迁移前验证每个锚点恰好命中一次，未命中立即回滚。
- 测试从前一 schema version 升级，覆盖旧行保留、同连接插入、重开、外键、索引、重复打开和 unsupported canary。
- 回滚代码时不倒迁数据库；放宽的 CHECK 必须对旧数据无破坏。

## 5. 服务与同步链路

逐项检查 MCP、Skills、Profiles、Projects、Overview、Sync 与 Restore：

- 中央 CRUD 与工具分配是不同动作；分配不隐式 Apply。
- Import 是只读发现 → 持久化脱敏预览 → 用户显式选择 → 中央导入，不隐式接管原生目标。全局 Skill 只有在正式目标入口与 Ready 中央副本的名称和完整树哈希精确一致时，才可另行准备 takeover-aware Preview；首次接管即使开启 direct Apply 也必须再次确认。
- MCP renderer/parser 必须保留未知字段，只修改受管名称；`headers`、`env`、`auth` 和扩展凭据不能进入普通 DTO、日志或预览明文。
- Skills 继续使用中央不可变副本和逐名称受管链接。普通 Apply 对普通目录、外部链接、断链和逃逸保持冲突保护；显式首次接管只能通过持久化证据生成专用 mutation。外部链接只替换入口，普通目录必须先创建可恢复目录树快照。工具是否发现符号链接必须由实机 smoke 证明。
- Project service 只创建该工具支持的 assignment/status；不支持 Prompt 的工具不能产生项目 Prompt 行。
- Overview 可以展示 Unsupported，但不能把它描述为“未接管”。
- Restore 必须从 snapshot 的 tool/artifact/scope 重新推导同一窄 allowed root，并复用现有 journal、snapshot、写后校验与回滚。

## 6. 前端元数据与页面

Rust 的 `Tool` 是 TypeScript 联合类型的唯一来源。修改 Rust 后运行：

```bash
pnpm bindings:generate
pnpm bindings:check
```

不要手改 `src/bindings/commands.ts`。工具显示与能力集中在 `src/lib/tool-metadata.ts`：label、icon、profile route 和 Provider/Prompt/MCP/Skills capability 必须来自同一条 metadata。

检查以下界面：

- MCP/Skills：全局分配、导入、目标状态、直接应用与错误状态；
- Projects：工具切换、资源标签、项目 assignment、Preview/Apply；
- Dashboard：工具计数、Supported/Unsupported 文案和管理入口；
- Profiles/Prompts/AppShell：只为 `PROFILE_TOOLS` 提供 CRUD 与导航；
- Onboarding：只展示真正可导入的首次配置，不为 Unsupported 能力创建空卡片；
- Restore：工具标签、目标路径与诊断一致。

组件边界也要 fail closed。即使路由当前不可达，把不支持工具直接传给组件也不能触发 Provider/Prompt 查询或 mutation。

品牌资源放在 `src/assets/brand/`，并在同目录 README 记录来源、许可或自行绘制说明。

## 7. Fixtures 与测试

每个新工具至少覆盖：

- Tool 序列化、生成 bindings 与 metadata 集合；
- Desktop/CLI 探针的成功、缺失、错误 ID、异常版本、链接路径和超时；
- Adapter 的 global/project descriptor、unsupported capability、ownership、敏感 selector 和 allowed root；
- 数据库上一版本升级、精确锚点、旧数据、约束 canary、外键/索引与重开；
- MCP stdio/HTTP round-trip、未知字段、敏感值、Missing/InSync/漂移/解析失败/类型冲突/stale/恢复；
- Skills 全局/项目分配、导入来源、普通目录/外部链接/断链/逃逸、恢复和实机发现 smoke；
- 前端分配、导入、状态、项目视图、Unsupported、取消分配和无隐式 Apply；
- `src-tauri/tests/phase8_e2e.rs` 的跨层 Preview → Apply → 漂移 → Restore。

完整质量门：

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test --run
pnpm bindings:check
pnpm rust:check
pnpm check
git diff --check
```

## 8. 发布与回滚

合入前确认 capability 文案、官方证据和代码矩阵完全一致。某项能力在实现阶段失去证据时，回到规划并把该能力关闭；不要临时改成另一种未设计的写入模式。

代码回滚顺序：先从 UI 和共享集合关闭 capability，再移除 service/registry，最后移除 Adapter 分支。已应用的前向数据库迁移保留。任何原生写入失败都使用现有 snapshot/journal 恢复，不增加旁路清理脚本。

## 9. Pi 与 ZCode 示例

Pi 当前只作为待调研候选，不代表已知路径；ZCode 已于 2026-09-05 依据本机核验
与官方 zcode-configuration-guide 完成证据核验并正式接入（迁移 `0013`）：

| 工具  | Provider  | Prompt/Rules | MCP       | Skills    | 下一步                                                                                                                                                 |
| ----- | --------- | ------------ | --------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Pi    | Unknown   | Unknown      | Unknown   | Unknown   | 找到官方配置与安装文档，建立版本化 fixture                                                                                                             |
| ZCode | Supported | Supported    | Supported | Supported | 已接入：desktop bundle（`dev.zcode.app`）探针；`~/.zcode/v2/config.json` 的 provider 条目只接管 name/kind/options/enabled；MCP 为 `mcp.servers` 嵌套键 |

在官方证据、capability matrix 和回滚边界审核通过前，不为它们新增 Tool 值、猜测目标目录或复制 Cursor 的 Adapter。
