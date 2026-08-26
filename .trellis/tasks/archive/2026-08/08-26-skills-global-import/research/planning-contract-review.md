# 规划合同核对

核对范围：`prd.md` R1–R8/AC1–AC9，对照 `design.md` §3、§5、§6、`implement.md` §3–5，并抽查现有状态服务、DTO、状态展示和 MCP 导入对话框。未执行导入、Apply 或任何真实目录写入。

## 结论

未发现阻塞级合同矛盾。覆盖矩阵如下：

| 合同           | 对应设计/实施证据                    | 结论                                                                                                                                                                                                                                |
| -------------- | ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1/R2、AC1/2/8 | design §2.1/§6；implement §1/§4      | 已覆盖：Claude 解析根、`.agents`、解析后的 `CODEX_HOME`/默认 `.codex` 同时列出；中央空列表和正式目标缺失均不阻止检测。                                                                                                              |
| R3、AC4        | design §2.2–2.3、§4；implement §1/§2 | 已覆盖：断链、权限、无效内容、来源链身份和资源上限均有独立诊断/拒绝边界。                                                                                                                                                           |
| R4、AC3/7      | design §1、§3–4；implement §2/§4     | 已覆盖：只复制勾选项；不修改来源、不 assignment、不 managed ownership、不 Apply。`already_imported` 只表示精确名称+完整哈希且中央记录有效，且明确“只读展示、不新增副本或 assignment”（design §3）；复制/识别不等于受管或 assigned。 |
| R5、AC5        | design §5；implement §3              | 已覆盖：首次提示要求双空基线、无 managed items、无 desired assignment 且成功 symlink scan；半基线、managed items、中央损坏、策略/权限/类型阻断保留原状态。空目录/仅元文件不声称发现技能。                                           |
| R6、AC6        | design §3/§6；implement §2/§4        | 已覆盖：精确重复、大小写/内容冲突、无默认勾选、失败后重扫、列表刷新和状态展示。                                                                                                                                                     |
| R7、AC7        | design §4.2–4.3、§6；implement §2/§4 | 已覆盖：preview token、来源/中央版本重验、批量事务、重复令牌、失败补偿、提交不确定性和防重提交。                                                                                                                                    |
| R8、AC9        | design §1/§2.2、§6；implement §1/§4  | 已覆盖：`.system` 在打开正文前排除，别名也排除，不扫描插件缓存；仅内置时无可选候选。                                                                                                                                                |

## 关键核验点

- 现有状态服务确实先检查受管 desired 记录的损坏（`src-tauri/src/skills/service.rs:373-386`），再按 baseline/managed items 扫描并 `assess_drift`（`src-tauri/src/skills/service.rs:388-412`）。这支持“基线损坏、MCP/通用真实漂移、策略错误不得被首次提示遮盖”的设计前提；新增 Skills 诊断必须继续保持该优先级。
- 现有 `SkillTargetStatusDto` 仅有 `status` 与可选 `diagnostic_code`（`src-tauri/src/skills/models.rs:139-145`），design §5 已明确只在 Skills 状态服务附加诊断，不扩展通用枚举；`globalTargetStatusPresentation` 当前仅按 status 映射（`src/lib/global-target-status-ui.ts:17-62`），因此 implement §3/§4 的“status 与诊断同时匹配才覆盖文案、MCP 映射不变”是必要且一致的约束。
- MCP 对话框现有模式可直接核验 requestId/query 隔离、`useDialogFocus`、确认令牌提交和 pending 锁：`src/features/mcp/mcp-import-dialog.tsx:16-44`、`160-180`。其描述明确原生配置不变且后续 Apply 独立（`...mcp-import-dialog.tsx:73-78`）。Skills 设计要求独立 DTO/不复用 MCP 业务逻辑，同时复用焦点/错误/模态基础设施，和 frontend quality 规范一致。

## 非阻塞最小修订建议

1. **[非阻塞，文档可核验性]** `implement.md` §4 已列“requestId 隔离、焦点、晚到响应”，但未逐项写出打开/重扫必须生成新 requestId、关闭/换工具清空选择；这些约束在 `design.md §6` 已完整存在。可在实施检查清单同一条补上这两个词，避免测试只验证 query key 而漏掉选择状态清理。
2. **[非阻塞，测试证据]** `implement.md` §5 的命令覆盖前端 Skills/MCP 与 Rust `skills::`，但没有单独点名 `global-target-status-ui` 的“诊断不匹配不覆盖”测试；design §5 已规定该行为。建议在共享 helper/MCP 回归条目中显式加入 `SKILL_TARGET_INITIAL_*` 与普通/MCP status 不匹配矩阵。

以上两项不改变产品合同，也不构成实现阻塞。
