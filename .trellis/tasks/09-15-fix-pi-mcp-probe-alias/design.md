# Design：修复 Pi MCP 探测与容器别名

## 1. 设计边界

本任务只修正 Pi adapter 的事实解释，不改变通用 MCP schema、数据库结构或其他工具行为。
所有 alias 兼容和规范化集中在 `PiAdapter`，服务层继续消费 canonical `mcpServers` 投影。

## 2. 适配器就绪状态聚合

`probe_mcp_adapter()` 继续分别计算 global/project scope，但聚合顺序改为表达实际加载边界：

1. 已就绪的全局适配器优先，项目未信任或项目 package 被过滤不能撤销它。
2. 全局没有可用适配器时，项目 scope 才决定当前项目上下文能否加载适配器。
3. 没有任何 Ready scope 时，继续按 Filtered → NotLoaded、已安装但版本过低 → VersionUnsupported、其余 → Missing 的 fail-closed 语义返回。
4. `exclusive` 模式继续忽略项目 scope。

版本冲突不做新的推断：保持现有“无法证明有效版本则 fail closed”原则，并用组合测试锁定结果。

## 3. Alias 读取与 canonical 投影

### 3.1 统一入口

复用 `pi::read_mcp_servers()` 作为唯一容器解释函数：

- `mcpServers` 存在：读取 canonical，忽略 alias；
- canonical 缺失且 `mcp-servers` 存在：读取 alias，并产生 `PI_MCP_CONTAINER_ALIAS_DETECTED`；
- 均不存在：返回空/缺失；
- 容器类型错误：沿用现有 parse/conflict 错误。

`PiAdapter` 在 `project_managed` 或等价 adapter 投影边界把 alias 临时投影成 canonical，随后通用
`scan_target`、Import、Preview、drift/status 和项目原生资源链继续只处理 `mcpServers`。

### 3.2 写入规范化

Pi JSON render 在 clone 原文档后执行条目级 read-modify-write：

- 以 §3.1 选中的容器作为当前值；
- 应用既有受管 selector；
- 写回 `mcpServers` 并删除 `mcp-servers`；
- 保留未知顶层字段、未受管 server 和未知 server 字段；
- canonical 与 alias 同时存在时不把 alias 独有条目合并进 canonical，严格匹配上游 `??` 语义。

## 4. Import、原生资源与 Restore

- Import 不增加第二套解析器，消费 adapter 规范化后的 canonical 投影。
- 项目原生资源的 observe/action/hash 与 drift/status 同样消费该投影。
- snapshot 中读取 MCP 条目时使用相同 alias 解释；最终 render 负责写回 canonical。
- 文件级 snapshot 仍保存原始字节，因此 Apply 失败时的原子回滚不受规范化影响。

## 5. 诊断

alias 命中时把 `PI_MCP_CONTAINER_ALIAS_DETECTED` 接入现有 preview/status 诊断通道。该诊断为提示，
不阻断 Import 或 Apply；完成一次成功 Apply 后文件已 canonical 化，诊断自然消失。

## 6. 文档同步

归档任务 PRD 保持历史记录；在当前任务 PRD、有效 spec 和必要的维护文档中统一说明：

- `settings.json`/`trust.json` 不受管且永不写入；
- 允许限定用途的安全只读探测；
- 静态探测不覆盖临时 CLI/扩展 trust 决策。

## 7. 回滚

无数据库迁移。出现回归时可整体回退 probe 聚合和 Pi adapter alias override；canonical-only 文件仍与旧版本兼容。
