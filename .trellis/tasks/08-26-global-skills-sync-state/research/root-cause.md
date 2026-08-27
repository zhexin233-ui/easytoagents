# 本机证据与根因

核验日期：2026-08-26。所有本机操作为只读；未调用导入、分配、同步预览或 Apply，也未执行技能正文/脚本。

## 当前电脑

| 路径/记录 | 事实 |
| --- | --- |
| `/Users/zhexin/.claude/skills` | 仅一个普通文件 `.DS_Store`，无技能目录或链接 |
| `/Users/zhexin/.agents/skills` | 不存在 |
| `/Users/zhexin/.codex/skills` | `.DS_Store` 与 `.system`；内置集合不属于本次导入或修复范围 |
| `/Users/zhexin/.skills-manager/skills/smart-search-cli` | 源目录存在 |
| `/Users/zhexin/Library/Application Support/com.easytoagents.desktop/skills/3cd463f9-ac96-47e3-8166-3d07d4eaeb18` | 中央副本存在，三个文件与源逐文件 SHA-256 相同 |

上述三个原生 Skills 根均无 `smart-search-cli`，无直属损坏链接。中央副本与源的文件集合相同：`SKILL.md`、`agents/openai.yaml`、`references/cli-contract.md`；没有读取或执行其中的指令。

私有数据库：`/Users/zhexin/Library/Application Support/com.easytoagents.desktop/easytoagents.sqlite3`。使用 SQLite URI `mode=ro` 加 `PRAGMA query_only=ON`，先读 schema，再查最小元数据列，不打印正文、凭证或预览 payload。

- `skill_global_assignments` 恰好两行，分别为 Claude/Codex，均指向上面的 smart-search UUID。
- `managed_items WHERE resource_kind='skill'` 为 0。
- `managed_targets` 只有 Claude/Codex 各自的 `mcp`、`prompt`、`provider`，共六行；没有 `artifact_kind='skill'`。
- 因此没有可证明的 Skills baseline、managed item 或同步目标；其它资源的历史 Apply 不能证明 Skills 已经同步。
- `skill_import_previews` 的 consumed 记录只证明中央复制导入完成，不证明原生同步。

主代理独立复核了目录直属条目、全部全局 Skill 分配、Skill managed item 计数及 target 的 tool/artifact/scope；结论与探索结果一致。

## 确认的原因

1. **导入与分配成功，但未执行 Skills 同步。** `src/features/skills/skills-page.tsx:103` 的 `globalAssignmentMutation` 只调用 `setGlobalSkillAssignment`；真正写入由 `previewSkillSync` 与 `applySkillPreview` 两步完成（同文件 `:122`、`:689`）。原始导入说明已写明仅复制，但分配按钮旁没有直接提醒。
2. **首次诊断遗漏了已分配场景。** `src-tauri/src/skills/service.rs:405` 的初始诊断要求 `desired.is_empty()`（`:412`）。一旦分配，空目录或只有 `.DS_Store` 的未初始化目录都会退回通用诊断。
3. **通用漂移算法按既有合同工作。** `src-tauri/src/sync/mod.rs:412` 对无 baseline、空 managed projection 的既有目录返回 `ExternalNonOwnedChange`、`can_merge=true`。此处是可合并状态，不是覆盖授权或阻断错误。
4. **Codex 路径正确。** 官方文档将用户 Skills 根定义为 `$HOME/.agents/skills`；`~/.codex/skills` 在本项目仅是兼容导入来源。当前“待初始化”反映真实目录缺失，不是路径配置错误。
5. **前端没有足够证据自行判断首次同步。** `src/lib/global-target-status-ui.ts:17` 只接收 status/diagnosticCode；只有两个现存初始诊断会覆盖通用徽标。修复应由后端补充窄诊断，不能根据“已分配”直接把所有漂移改为待同步。

## 代码与测试落点

| 位置 | 作用与约束 |
| --- | --- |
| `src-tauri/src/skills/service.rs:362` | `list_global_skill_target_statuses_with_policy_probe`，已有中央完整性、baseline、ownership、scan 和 drift 证据 |
| `src-tauri/src/skills/service.rs:1184` | 现有首次状态矩阵包含 `.DS_Store`，并明确断言 desired 非空不产生初始诊断；需要精确更新这条旧合同 |
| `src-tauri/src/skills/service.rs:1552` | 缺失目标预览/Apply/快照/权限测试，可扩展两工具覆盖 |
| `src-tauri/src/skills/service.rs:1647` | 同名目录、外部/断裂链接冲突保护，必须保持 |
| `src-tauri/src/skills/service.rs:1878` | 预览后外部变化使旧预览失效，必须保持 |
| `src/lib/global-target-status-ui.ts:22` | 窄诊断映射；MCP 与 Skills 共用，必须同时匹配 status 与 code |
| `src/features/skills/skills-page.tsx:103` | 全局分配成功目前只失效查询，可增加中央意图/需预览应用的说明 |
| `src/features/skills/skills-page.tsx:363` | “全局已分配”按钮；应有附近的流程说明，不声称已安装 |
| `src/features/skills/skills-page.test.tsx:1103` | 首次状态展示测试的自然扩展位置 |

## 资料

- [OpenAI 官方 Skills 文档](https://learn.chatgpt.com/docs/build-skills#where-to-save-skills)：已通过官方工具搜索并完整获取，确认 `$HOME/.agents/skills` 与目录链接支持；新技能通常自动发现，未出现时可重启。
- `.trellis/tasks/archive/2026-08/08-19-ai-config-desktop/design.md`：中央意图与原生目标分离、Preview/Apply、安全所有权不变量。
- `.trellis/tasks/archive/2026-08/08-26-skills-global-import/design.md`：导入不分配/接管、Codex 兼容来源、初始状态原合同。
- `.trellis/tasks/archive/2026-08/08-25-fix-global-target-initial-status/design.md`：共享 status+diagnostic 展示模型，避免改通用状态或吞掉真实错误。

## 验证边界

目前没有运行真实同步预览/Apply、浏览器或桌面验收。代码路径表明当前两目标应可预览，但不能以静态阅读替代本机 Apply 成功。实施阶段先用隔离测试复现 `.DS_Store` 与缺失目录，再按用户批准范围处理本机唯一 Skill。

## 独立规划核验与主代理裁定

独立核验确认中央完整性、半 baseline、策略/权限/类型错误均在原 assessment 路径得到保护；提出 Missing+desired 和已有目录+desired 的测试缺口，均已列入实施矩阵。

核验将“当前实现不符合新增 guard”列为阻断，这是本任务明确要修改的已知缺陷，而不是设计不相容。现有 `service.rs:1260` 附近的断言是 `!diagnostic_code.starts_with("SKILL_TARGET_INITIAL_")`，不是要求分配后继续返回 `SKILL_TARGET_INITIAL_UNMANAGED`。主代理已亲自读过原文，实施步骤 2 明确更新该过宽断言；不据此改动通用漂移算法或放弃 Missing 场景。
