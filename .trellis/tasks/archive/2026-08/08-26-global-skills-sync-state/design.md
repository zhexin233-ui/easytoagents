# 技术设计

## 边界

问题是首次同步状态语义缺口和操作提示不足，当前没有证据表明导入损坏或原生同步引擎失效。保持中央复制、分配意图与原生写入分离，不自动同步，不放宽同名冲突或 stale 检查。

仅修改 Skills 状态服务、共享状态展示、Skills 页说明和对应测试。数据库结构、DTO、通用 `SyncStatus`、`assess_drift`、目标路径与 Apply 引擎不变。

## 后端诊断合同

在 `list_global_skill_target_statuses_with_policy_probe` 中沿用已有中央完整性、baseline、managed items、ownership、scan 与 assessment：

新增诊断 `SKILL_TARGET_INITIAL_SYNC_PENDING`，仅当以下证据全部成立：

1. desired 至少一项，所有中央记录经现有检查均为 Ready。
2. baseline full/managed hash **同时为空**。
3. existing managed items 为空。
4. assessment 可合并，且为下列安全组合之一：
   - `Missing` + `TargetScan::Missing`；
   - `ExternalNonOwnedChange` + 成功的 `ObservedDocument::SymlinkDirectory` 扫描。

由现有 drift/ownership 逻辑保障第二种组合没有同名受管投影。仅新增展示诊断，不更改原 `status`、`can_merge` 或预览/Apply 判定。

| 证据 | 页面语义 |
| --- | --- |
| 无分配、目标缺失 | 保持“待初始化” |
| 无分配、无 baseline、无 managed items、成功目录扫描 | 保持“空目录，待配置”或“未纳入同步管理” |
| 有分配、满足新增全部条件 | “已分配，待同步”，清楚说明尚未写入原生链接 |
| 只有一个 baseline hash / 已有 managed items | 不使用新诊断 |
| 真实同名冲突、受管漂移、中央副本问题、类型/权限/策略/能力问题 | 保持原诊断及阻断 |
| 完整 baseline 后发生外部非受管变化 | 保持 `EXTERNAL_NON_OWNED_CHANGE` |
| 成功 Apply 后 baseline/managed items 完整 | 正常显示 `in_sync`，不得仍显示首次待同步 |

`.DS_Store` 不视为合法技能、不删除、不从底层扫描/hash 中排除。它是应保留的外部目录条目。只有预览行、没有 baseline 的 target 记录仍可符合新增条件。

## 前端数据流

`status + diagnosticCode` → `globalTargetStatusPresentation` → `SyncStatusBadge`、说明、预览按钮。

- 共享 helper 只在新诊断与 `missing` / `external_non_owned_change` 精确匹配时返回“○ 已分配，待同步”。其它 status 与新 code 的错误组合不覆盖默认处理。
- 说明强调“尚未写入工具目录；点击预览全局同步并确认应用。现有非受管内容会保留。”不声称已经安装、目录已存在或已发现其它技能。
- 分配区增加短说明，分配成功反馈提示“分配已更新，只改变中央配置，需预览并确认同步”。取消分配也适用，不承诺原生链接立即消失。
- 保留全量 Skills 查询失效；新增 UI 回归证明分配后列表与状态刷新。没有任何自动 preview/Apply，保留精确 preview ID 消费与弹窗冲突阻断。
- 不修改通用同步预览中的原始警告/冲突。它们仍展示真实计划，并由现有共享弹窗决定是否允许 Apply。

## 本机处理

用户批准此方案后，先完成隔离测试和必要构建，再通过应用现有生产 Preview/Apply 流程，分别处理当前唯一已分配 Skill `smart-search-cli`：

- Claude：计划新增 `~/.claude/skills/smart-search-cli` 链接，保留 `.DS_Store`。
- Codex：计划创建缺失的 `~/.agents/skills` 及同名链接，保留 `~/.codex/skills/.system` 与兼容目录全部内容。
- 链接均指向数据库已验证的中央副本；实际值由后端解析，不把本机绝对路径或 UUID 写入产品代码。
- 应用自己的快照、journal、基线与 managed items 正常生成；禁止手工 `ln -s`、改 SQL baseline、删除元文件或绕过预览来让状态变绿。
- Apply 前若实际预览超出上述范围、出现同名冲突/其它异常，停止本机写入并报告；不得替用户覆盖。
- 通过应用状态、数据库相关元数据和链接/内容只读检查核对结果。不会调用模型或执行技能 CLI 来制造加载成功证据；新会话发现的限制须如实说明。

## 兼容与回滚

- 不增加迁移；回退本次产品代码不会改变数据库 schema。
- 本机写入只使用已有同步器的快照/恢复机制。失败按现有安全回滚处理，不手工删除未知目录或链接。
- 共享 helper 必须跑 MCP 回归，确保 Skills 专用诊断不会改变其它资源的初始、策略、信任或漂移展示。
- 后续已有完整 baseline 时新增/取消分配的全面待同步建模、本地技能运行依赖、其它历史 Skill 来源消失等不在本次范围。
