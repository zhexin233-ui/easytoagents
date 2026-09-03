# 中央 Skill 内容变更后同步更改 — 实施计划

## 1. Backend contract

- [ ] 在 `skills/models.rs` 不新增输入类型；命令复用 `VersionedSkillInput`。
- [ ] 在 `skills/library.rs` 增加采纳读取：digest + 解析 `SKILL.md`，hash 漂移时也要能读正文；类型/路径/缺失保持与 `inspect_central_skill` 相同诊断。
- [ ] 在 `db/skills.rs` 增加 CAS 更新 `content_hash` / `frontmatter_json` / `status='ready'`，不手写 `row_version + 1`。
- [ ] 在 `skills/service.rs` 实现 `adopt_skill_content`：校验版本、拒绝活动 writer、强制 `frontmatter.name == record.name`、幂等 Ready、返回 `skill_dto`。
- [ ] 注册 `commands/skills.rs` 与 `lib.rs`；`pnpm bindings:generate`。

## 2. Frontend

- [ ] `SkillsPage`：仅 `CENTRAL_SKILL_CONTENT_CHANGED` 显示「同步更改」；打开确认对话框，不预加载正文。
- [ ] 对话框：「是」调用 `commands.adoptSkillContent({ id, rowVersion })`；取消/Escape/关闭不发 RPC 并恢复焦点；pending 锁定。
- [ ] 成功 invalidate `skillKeys.all` 并发 success notify；失败 error notify。禁止调用 preview/apply。

## 3. Tests

- [ ] Rust：改中央文件后采纳 → Ready、hash/description 更新、目录字节与 symlink 不变；已 Apply 目标不再报 `CENTRAL_SKILL_CONTENT_CHANGED`；改名/坏 SKILL.md/类型变化/过期版本/活动 writer 拒绝且不写库；hash 已一致时幂等。
- [ ] 前端：按钮显隐；打开对话框无 RPC；取消/Escape 无 RPC 且焦点恢复；点「是」payload 精确；成功后诊断消失且未调用 `previewSkillSync`/`applySkillPreview`；失败通知。
- [ ] `pnpm bindings:check`。

## 4. Spec

- [ ] 更新 `.trellis/spec/backend/quality-guidelines.md` 与 `.trellis/spec/backend/skill-import-guidelines.md`：中央内容漂移可通过显式采纳恢复 Ready，仍阻断未采纳的预览/同步/删除。
- [ ] 更新 `.trellis/spec/frontend/quality-guidelines.md`：Skills 中央卡片确认对话框与生成命令合同。

## 验证命令

```bash
pnpm bindings:generate
pnpm bindings:check
pnpm test -- --run src/features/skills/skills-page.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml skills::
pnpm check
git diff --check
```

## Risky files / rollback

- `src-tauri/src/skills/library.rs`：不要放宽普通 `preview_skill_content` / `inspect_central_skill` 的列表语义。
- `src-tauri/src/skills/service.rs`：不要在采纳路径调用 `preview_skill_sync` 或改 managed items。
- 若采纳读取无法安全复用 digest，保持独立 helper，禁止让内容预览 RPC 返回漂移正文。

## Follow-up before `task.py start`

- 规划摘要已展示，等待用户明确批准后再 `task.py start`。
- 不改 `09-02-manage-project-resources`。
