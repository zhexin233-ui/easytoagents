# 中央 Skill 内容变更后同步更改 — 技术设计

## Architecture and Boundaries

本操作只更新中央 Skill 记录，不写原生工具目录。

```
SkillsPage「同步更改」
  → 确认对话框（是 / 取消）
  → adopt_skill_content({ id, rowVersion })
  → inspect/digest 当前中央目录（library）
  → CAS UPDATE skills.content_hash / frontmatter_json / status='ready'
  → SkillDto（Ready）
  → invalidate skillKeys.all
```

| Layer | Owns | Must not |
| --- | --- | --- |
| `skills/library.rs` | 安全读取中央树、解析 `SKILL.md`、计算 tree hash | 写数据库、跟随不安全链接、返回正文到普通列表 |
| `skills/service.rs` | 采纳编排、名称不变约束、writer 拒绝、返回 DTO | 改 `central_path` / `name`、写 managed_targets、Preview/Apply |
| `db/skills.rs` | CAS 更新 hash/frontmatter/status，依赖现有 row_version bump 触发器 | 新迁移（现有列足够） |
| `commands/skills.rs` | 取 database 锁并委托 | 读进程环境、手写 payload |
| `SkillsPage` | 仅对该诊断显示按钮、确认对话框、mutation、notify | `window.confirm`、`invoke`、打开 `ChangePreviewDialog` |

## Contracts

### Command

```rust
adopt_skill_content(database, paths, input: &VersionedSkillInput) -> Result<SkillDto, AppError>
```

复用 `VersionedSkillInput { id, row_version }`。不要新 DTO 字段。返回现有 `SkillDto`，普通列表仍不包含 `SKILL.md` 正文。

### Adoption read path

不要调用 `preview_skill_content`（它在 hash 漂移时拒绝）。在 `library.rs` 增加采纳专用读取：沿用 `inspect_central_skill` 的路径/类型/canonical/digest 边界，但在 hash 不匹配时仍解析 `SKILL.md`。

成功前提：

1. 记录存在且 `row_version` 匹配，否则 `CONFLICT` / `rowVersion`。
2. 无 `applying` / `restoring` / `rollback_failed` `sync_runs`，否则 `WRITE_IN_PROGRESS`（与 skill import confirm 相同查询）。
3. 目录仍是中央根直属真实目录；类型/路径/缺失诊断对应稳定冲突，不写库。
4. digest 与 `SKILL.md` 解析通过；`frontmatter.name` 必须等于 `record.name`（含大小写/NOCASE 身份，拒绝改名）。
5. 若 digest.hash 已等于 `record.content_hash` 且解析合法：不强制改行，返回当前 Ready DTO（幂等）。
6. 否则 `UPDATE skills SET content_hash, frontmatter_json, status='ready' WHERE id AND row_version`；变更数不是 1 则冲突。触发器在 `NEW.row_version = OLD.row_version` 时自动 bump。

### Frontend

- 按钮放在现有 `skillActions` 中，仅 `skill.diagnosticCode === "CENTRAL_SKILL_CONTENT_CHANGED"`。
- 确认框使用 `useDialogFocus`，标题「同步更改」，说明只更新应用记录、不改写工具目录链接；按钮「是」「取消」。
- 点「是」前不调用 command。pending 时禁止关闭与重复提交。
- 成功：关对话框、`invalidateQueries(skillKeys.all)`、success notify。失败：error notify；过期版本依赖列表刷新，不在客户端重放旧 `rowVersion`。

## Compatibility

- 无 schema 迁移。
- 不改变 `inspect_central_skill` 对列表/预览/同步的现有阻断语义。
- 不接线 MCP `readoptMcpTarget`，不设置 Skills `readoptAvailable`。
- `applyMode=direct` 不适用：本命令不是原生 Apply。

## Trade-offs

- 确认框不用 `window.confirm`：无法稳定提供「是」，也不符合本页已有对话框焦点合同。
- 不刷新 managed item 基线：目标投影只含链接路径，刷新没有可观察收益，且会扩大写入面。
- 拒绝改名：改名涉及目录 rename、NOCASE 唯一与 `external_key`，超出本次「采纳内容」范围。

## Rollback

命令只更新一行中央记录。失败时不改磁盘。若错误地写入了错误 hash，用户仍会看到 `CENTRAL_SKILL_CONTENT_CHANGED` 或可通过再次采纳纠正。不提供单独的撤销命令。
