# 技术设计

## 问题边界

问题源于两层语义叠加：发布探针把“官方企业策略源不存在”与“策略源不可验证”都压缩成无证据，Adapter 再把无证据映射为 Unknown；前端又只根据 `SyncStatus::PolicyBlocked` 渲染红色阻止，无法利用诊断码区分 Unknown 与 Blocked。

本次不扩展跨层 DTO。后端继续返回现有 `SyncStatus` 与 `diagnosticCode`，前端在全局目标卡片边界根据两者生成展示模型。

## 数据流

```text
macOS 官方策略路径
  -> release tool probe（可信读取与策略证据）
  -> ClaudeAdapter descriptor.policy
  -> MCP/Skills 全局状态 DTO（status + diagnosticCode）
  -> global-target-status-ui 展示模型
  -> SyncStatusBadge + 说明 + 预览按钮
```

## 后端判定矩阵

| 官方策略输入 | 结果 | 理由 |
| --- | --- | --- |
| 主文件不存在，drop-in 目录不存在或为空 | Allowed | 没有启用企业限制的官方来源 |
| 合法对象但字段缺失 | Allowed | `strictPluginOnlyCustomization` 未启用 |
| 显式 `false` | Allowed | 官方值明确允许 |
| 显式 `true` 或包含 surface 的数组 | 按现有规则 Blocked/Allowed | 保持逐 surface 合同 |
| 非法 JSON/字段类型、不可读、非规范或链接路径 | Unknown | 无法信任证据 |
| `policyHelper` 或非空 drop-in 目录 | Unknown | 动态/多来源有效值尚不能安全合并 |

`read_managed_settings` 需要区分“可信确认缺失”和“读取不安全/解析失败”，不能继续用同一个 `None` 表达两者。缺失来源生成绑定 Claude 版本与配置根、`source_path=None` 的 Allowed 证据；现有版本/配置根/来源变化失效规则保持不变。

## 前端展示模型

`src/lib/global-target-status-ui.ts` 作为 MCP 与 Skills 的唯一全局状态展示映射：

| 状态/诊断码 | 标签与色调 | 操作 |
| --- | --- | --- |
| `missing` | 黄色“待初始化” | 可预览 |
| `policy_blocked + CLAUDE_POLICY_UNKNOWN` | 黄色“策略状态待确认” | 禁用 |
| `policy_blocked + CLAUDE_POLICY_BLOCKED` | 红色“策略阻止” | 禁用 |
| 其它失败/信任状态 | 保持现有语义 | 保持现有规则 |

`SyncStatusBadge` 增加窄范围的显式色调覆盖能力，使诊断感知的展示不复制 Tailwind 样式；默认调用者行为不变。状态说明和标签由共享 helper 生成，两页不得各自解析诊断码。

## 兼容性与风险

- DTO、数据库与生成绑定不变，无迁移。
- 行为变化仅发生在“可信确认官方策略源未配置”时：由 Unknown 改为 Allowed。
- 最大风险是把不在已支持官方路径中的企业管理机制视为未配置。对已知动态 `policyHelper`、drop-in、多源、权限与路径异常继续 Unknown，控制风险边界。
- 回滚可分别撤销发布探针缺失语义与前端展示映射，不影响持久化数据。

## 影响文件

- `src-tauri/src/app/tool_probe.rs`
- `src-tauri/src/adapters/mod.rs`
- `src-tauri/src/mcp/service.rs` 与/或其测试模块
- `src-tauri/src/skills/service.rs` 与/或其测试模块
- `src/components/sync-status-badge.tsx`
- `src/lib/global-target-status-ui.ts`
- `src/features/mcp/mcp-page.tsx`、`mcp-page.test.tsx`
- `src/features/skills/skills-page.tsx`、`skills-page.test.tsx`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/frontend/quality-guidelines.md`（仅在形成可复用合同后更新）
