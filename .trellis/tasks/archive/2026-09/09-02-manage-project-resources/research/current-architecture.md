# 当前架构与安全约束研究

## 结论

当前项目登记只扫描目标级状态，项目详情只管理中央资源追加，尚未建立“项目原生资源项”的独立身份与生命周期。新功能应复用现有适配器、持久化预览、全局单写者、快照和回滚机制，但不能把 managed_items 当作原生资源表：该表要求关联中央资源，而项目原生资源在默认状态下并不属于中央库。

## 关键证据

| 主题 | 证据 | 设计含义 |
| --- | --- | --- |
| 登记只读 | src-tauri/src/projects/service.rs:82-114 | 登记可持久化项目和观察元数据，但不得改写项目文件。 |
| 项目扫描 | src-tauri/src/projects/service.rs:290-355 | 当前只把项目级 MCP/Skill 目标放入 ProjectDto；需要补入受支持的 Prompt，并增加逐项发现。 |
| 初始未纳管 | src-tauri/src/projects/service.rs:485-567 | “发现”不等于 ownership；原生项必须与 managed_items 分开分类。 |
| 项目资源 UI | src/features/projects/project-detail-page.tsx:269-383 | 当前页面按资源类型与平台展示中央追加；原生资源应作为独立分区加入同一组合。 |
| 平台路径 | src-tauri/src/adapters/claude/mod.rs:129-165；src-tauri/src/adapters/codex/mod.rs:112-155；src-tauri/src/adapters/cursor/mod.rs:83-122 | Claude/Codex/Cursor 支持项目 MCP/Skill；仅 Claude/Codex 支持项目 Prompt。 |
| 受管项模型 | src-tauri/src/db/migrations/0001_initial.sql:292-397 | managed_targets 可复用为空 baseline、无 item 的中性目标身份；managed_items 才表达中央资源 ownership，不适合保存未接管原生项。 |
| Ownership | src-tauri/src/adapters/mod.rs:827-884 | MCP 可按 selector 修改单项，Skill 可按子项名称管理，Prompt 是 WholeDocument。 |
| 原子写与回滚 | src-tauri/src/sync/apply.rs:1949-2253、2836-3151 | 禁用/恢复必须进入现有预览、快照、原子写、反向回滚和 rollback_failed 合同。 |
| Skill 完整树 | src-tauri/src/skills/library.rs:730-865、951-1040、1787-1837 | 普通 Skill 目录必须使用完整树 hash、复制校验和安全删除；外部链接只移动链接自身。 |
| 中央 Skill 隔离 | src-tauri/src/skills/library.rs:493-579 | 应复用隔离、hash 重验、目标占用冲突和恢复失败保护模式。 |
| MCP 安全 | .trellis/spec/backend/mcp-import-guidelines.md | DTO、预览、错误和日志不得泄露 MCP 凭据；原始值只可留在允许的私有存储和快照中。 |
| 项目移除 | .trellis/spec/backend/quality-guidelines.md:689-735 | 项目移除不得改写原生目标；若存在已禁用原生资源，应先恢复，避免登记移除后失去常规恢复入口。 |

## 资源级操作语义

### MCP

- 资源单位是容器内的单个 server 名称。
- 发现时解析私有配置，但前端只获得名称、传输类型、状态和脱敏诊断。
- 禁用时对整个目标文件创建私有快照，只删除目标 selector；未知兄弟条目、字段和 TOML 注释尽量保留。
- 恢复时从私有快照提取原始 selector 值并合并进当前文件；若同名位置已被占用则冲突，不能覆盖。

### Skill

- 资源单位是正式项目 Skill 根下的直属入口。
- 普通目录以完整树 hash 识别并保存 directory_tree 快照；符号链接保存链接文本与入口身份，不修改链接目标。
- 禁用后入口必须离开工具实际扫描根；恢复只允许写回缺失的原路径，目标占用时保留快照并报冲突。

### Prompt

- 资源单位是整个 CLAUDE.md 或 AGENTS.md 文件。
- 禁用保存 payload_file 快照后移除整文件。
- 恢复只允许目标路径为空时恢复原始字节与权限；不尝试合并 Markdown。

## 需要新增的能力

- 项目原生资源专用持久化记录，保存稳定逻辑身份、状态、观察 hash、禁用快照引用和 row version；中性 target identity 行不得被解释为 ownership。
- 逐项只读发现与安全分类，排除仍由 managed_items 证明 ownership 的中央托管项。
- 原生资源动作的持久化 PreviewPlan 与专用 Apply 入口。
- 项目详情页原生资源分区、状态、禁用/恢复动作和冲突反馈。
- 数据库迁移、绑定、Rust/React 测试与真实桌面检查。

## 风险

- MCP 快照可能包含凭据，不能进入普通 DTO、diff、错误、日志或 journal 明文。
- Skill 普通目录的递归复制和删除必须受现有限额、身份、硬链接和逃逸检查保护。
- SQLite 与文件系统没有跨资源事务；崩溃恢复必须依赖持久化 run、journal 和 snapshot，不能猜测成功。
- 已禁用资源的快照不能被普通快照删除流程移除；项目登记也不能在仍有禁用资源时直接移除。
- 保护必须接入 commands/overview.rs 调用的 sync::delete_snapshots 以及未来共享 cleanup guard，不能只保护新接口。
