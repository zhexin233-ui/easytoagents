# Skills 全局初始状态：跨层合同核验

## 已确认事实

- 当前 Skills 状态 DTO 只有 `tool`、`projectId`、`targetPath`、`status`、`diagnosticCode`，没有 target id、baseline hash、ownership、是否曾纳入管理等字段：`src-tauri/src/skills/models.rs:139-146`，前端绑定同构于 `src/bindings/commands.ts:574`。
- `list_global_skill_target_statuses_with_policy_probe` 为每个工具读取中央已分配 Skills，按 `find_skill_target_baseline(...).unwrap_or(ManagedTargetBaseline{target_id:"", full_hash:None, managed_hash:None})` 构造空 baseline；若有中央 Skill 检查失败，直接返回 `ExternalOwnedChange` 并透传其诊断码：`src-tauri/src/skills/service.rs:361-397`。
- 状态计算使用 `build_skill_ownership`、既有 managed items 和 `verify_managed_item_baselines` 后调用通用 `assess_drift`，但 DTO 丢弃了 baseline/ownership/`can_merge`/完整诊断列表：`src-tauri/src/skills/service.rs:398-415`。
- 通用判定明确把 `(None,None)` 且 managed projection 为空归为 `ExternalNonOwnedChange`，即使这是首次扫描；同一分支也覆盖空目录（只要目录可观察且 projection 为空）：`src-tauri/src/sync/mod.rs:401-414`。有 managed baseline 时，managed hash 相同而 full hash 不同才是 `ExternalNonOwnedChange`；owned hash 不同则 `ExternalOwnedChange`。
- 因此目前无法仅凭 Skills `SkillTargetStatusDto` 安全区分“首次未管理但已有原生内容”和“已管理后非受管变化”。空目录也没有独立状态：由扫描结果与空 projection 共同落入首次 `external_non_owned_change`，除非目标缺失而返回 `missing`。
- `ManagedTargetBaseline` 本身只保留 `target_id`、row version、full/managed hash，没有显式 ownership 或“adopted/ever-managed”标志：`src-tauri/src/sync/mod.rs:272-300`。ownership 在 preview/apply request 中存在，但 status DTO 不返回。

## UI/MCP 可复用合同

- Skills 和 MCP 已共用 `globalTargetStatusPresentation(status, diagnosticCode)`：`src/features/skills/skills-page.tsx:408-416`、`src/features/mcp/mcp-page.tsx:396-404`、`src/lib/global-target-status-ui.ts:1-57`。该函数只解释 `missing`、策略、失败、未受信任；`external_non_owned_change` 当前回退为 `description:null`，徽章仍显示通用“非受管变更”（`src/components/sync-status-badge.tsx:6`）。
- 不宜全局重命名 `external_non_owned_change`：MCP 已使用该语义表示“受管 projection 未变但目标含额外字段”，改名会掩盖真实漂移。最小安全方向是 Skills 专用诊断扩展（例如 DTO 增加明确 `managementState`/`diagnosticCode`，或只在 Skills service 产生 `initial_external_content`），让 UI presentation 接收可选 artifact-specific context；保留通用 `SyncStatus` 作为机器状态。
- MCP 导入对话框的可复用生命周期：打开时生成 `requestId: crypto.randomUUID()`，查询 key 为 `mcp-import/tool/requestId`，`staleTime: Infinity`、`gcTime: 0`、关闭后不复用在途请求，且禁用 focus/reconnect refetch：`src/features/mcp/mcp-page.tsx:430-449`、`src/lib/mcp-api.ts:13-24`。Skills 若新增“检测并导入已有 Skill”对话框，应采用同样 request-scoped query 合同，避免初次扫描结果粘连。
- 对话框可直接复用 `useDialogFocus(true,onClose)`：保存触发元素、打开聚焦首控件、Escape 关闭、Tab 环绕、卸载恢复原焦点：`src/components/use-dialog-focus.ts:12-69`；MCP 对话框接入位置：`src/features/mcp/mcp-import-dialog.tsx:34-66`。
- Skills 现有全局状态卡只有状态查询、预览按钮，没有检测/导入入口；实现最小新合同至少需要：独立发现 query（request id）、候选项 DTO（稳定 id/path/name/status）、确认 mutation 输入与结果、关闭/成功后 invalidate `skillKeys.all` 或 `skillKeys.globalStatuses()`。这些是 UI 生命周期需求，具体发现扫描安全复制不在本核验范围。

## 可扩展单测锚点与状态矩阵

已有锚点：通用 drift 分支在 `src-tauri/src/sync/mod.rs:1432-1479`；Skills 初始 missing/策略 unknown/blocked 在 `src-tauri/src/skills/service.rs:1159-1193`；中央 Skill 内容损坏导致 owned change 在 `src-tauri/src/skills/service.rs:1490-1525`；成功 Apply 后 `InSync` 在 `src-tauri/src/skills/service.rs:1630-1668`。前端状态/焦点测试可参照 `src/features/mcp/mcp-page.test.tsx:928-957` 与 `src/components/snapshot-restore-dialog.test.tsx:80-115`。

| 场景                         | baseline                               | 目标/managed projection   | 当前状态                                                     | 规划应验证                      |
| ---------------------------- | -------------------------------------- | ------------------------- | ------------------------------------------------------------ | ------------------------------- |
| 初始无目录                   | `(None,None)`                          | missing scan              | `missing`                                                    | 保持“待初始化”                  |
| 初始空目录                   | `(None,None)`                          | observed + empty          | `external_non_owned_change`                                  | 应改为可辨认“首次未管理/可导入” |
| 初始已有外部 Skill           | `(None,None)`                          | observed + non-empty      | `external_non_owned_change`                                  | 不得称用户修改；候选可导入      |
| 已部分导入/已有 managed item | 有 baseline，managed hash 相符         | full hash 多外部项        | `external_non_owned_change`                                  | 明确“已管理后非受管变化”        |
| 真实受管漂移                 | baseline managed hash 与 observed 不同 | owned item 改动/缺失      | `external_owned_change`                                      | 阻止或要求处理，保留诊断        |
| 中央 Skill 内容失败          | 任意                                   | `inspect_record != Ready` | `external_owned_change` + `CENTRAL_SKILL_CONTENT_CHANGED` 等 | 失败原因不能被初始状态覆盖      |
| 目录解析/权限失败            | 任意                                   | scan error                | `parse_error`/`permission_denied`/`failed`                   | 对话框失败可重试，焦点恢复      |

## 待决策

1. “首次未管理”判定是否以 `managed_targets` 行存在为准，还是需要持久化显式 adoption/import marker；仅凭两个 hash 无法表达空目录已被初始化但尚未写入的历史。
2. DTO 是否增加 `managementState`（如 `unmanaged_initial`、`managed_in_sync`、`managed_external_non_owned_change`）及候选计数/可导入能力；建议不改变通用 `SyncStatus` 名称。
3. 空目录应继续显示可预览同步的 `missing` 语义，还是单独诊断为“首次空目标”；两者对“检测并导入”按钮启用条件不同。
4. 首次发现的外部目录是否允许直接 adopt baseline，还是只能逐候选导入中央库后再由普通 Skills sync 建立受管基线。
