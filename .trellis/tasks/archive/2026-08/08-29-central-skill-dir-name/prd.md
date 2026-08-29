# 中央副本目录改用 Skill 名称命名

## Goal

Skills 导入中央库后，`~/Library/Application Support/com.easytoagents.desktop/skills/` 下的
副本目录名使用 SKILL.md frontmatter 的 `name`（如 `skill-install`），而不是随机 UUID
（如 `957e5564-be34-40c6-b345-cb73005685a8`），使"中央副本"路径可读、可溯源。

## Requirements

- 新导入（单目录导入与批量导入确认）产生的中央副本目录名为 `central_skills/<frontmatter.name>`；
  数据库 `skills.id` 仍为 UUID，仅目录名解耦。
- `name` 已由 `validate_skill_name` 约束（小写字母/数字/单连字符、≤64 字节、非保留名 `synced`），
  可安全作为单段目录名，无需额外转义。
- 同名中央目录已存在时，prepare 阶段即报冲突并清理 staging，不留下半成品；
  finalize 仍用 `RENAME_EXCL`/`RENAME_NOREPLACE` 保证原子不覆盖。
- 既有 UUID 目录的记录在应用启动时一次性迁移：校验（真实目录、直属于 central 根、内容 hash 与记录一致）后
  原子重命名为 `<name>`，同事务更新 `skills.central_path`，并同步改写受管 symlink 与
  `managed_items.last_applied_item_hash`，避免同步目标出现悬空链接。
- 迁移是尽力而为 + 可恢复的：旧目录缺失但新目录已验证存在时补完 DB 更新（崩溃恢复）；
  任何一步不满足则该记录保持旧路径继续可用（inspect 兼容 id 命名或 name 命名两种布局），不得中断启动。

## Acceptance Criteria

- [x] 导入 `skill-install` 后中央目录为 `.../skills/skill-install`，UI"中央副本"显示可读路径。
- [x] 同名冲突：中央已存在同名目录/同名记录时，单目录导入失败且 staging 与中央均无残留。
- [x] 批量导入确认路径（discovery→confirm）在名称化目录下正常工作，既有测试全绿。
- [x] 启动迁移：legacy UUID 记录被重命名并更新 DB 与受管 symlink；二次启动幂等。
- [x] `cargo fmt`、`cargo test`（src-tauri）全部通过（191 项）；前端不受影响（无改动、bindings 无变化）。
