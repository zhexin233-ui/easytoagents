# 修复 Codex Skills 目标路径指向 .agents/skills 的配置错误

## Goal

EasyToAgents 当前把 Codex 的 Skills 同步目标指向 `~/.agents/skills`（全局）与 `<project>/.agents/skills`（项目级），但 Codex 实际读取的是 `$CODEX_HOME/skills`（默认 `~/.codex/skills`）与 `<project>/.codex/skills`。结果是：应用显示"已同步"，但 Codex 看不到这些 Skills（用户机器上 `~/.agents/skills/skill-install`、`~/.agents/skills/smart-search-cli` 即为已被错误落点的受管链接）。本任务修正 Codex 的 Skills 目标路径与相关检测来源、标签语义。

## Requirements

- R1 Codex 全局 Skills 同步目标改为 `$CODEX_HOME/skills`（默认 `~/.codex/skills`；用户自定义 `CODEX_HOME` 时跟随，与 Codex 自身行为一致）。
- R2 Codex 项目级 Skills 同步目标改为 `<project>/.codex/skills`。
- R3 Skills 导入检测来源调整：`~/.codex/skills` 为主来源；`~/.agents/skills` 保留为次级来源（仅导入，不再作为同步目标），保证已有用户在该目录的技能仍可被发现并导入。
- R4 前端来源标签与新的语义一致：`~/.codex/skills` 展示为"正式同步目标"，`~/.agents/skills` 不再被描述为同步目标。
- R5 不自动删除或改写 `~/.agents/skills` 中已存在的旧链接（遵循应用"不碰非受管内容"的安全模型）；旧链接由用户手动清理，需在交付说明中列出受影响路径。

## Constraints

- 不改变 Claude 的任何路径行为。
- 不改变中央库位置（`Application Support/com.easytoagents.desktop/skills/`）。
- FFI bindings（`src/bindings/commands.ts`）为生成物，必须通过 `bindings:check` 重新生成，不允许手改漂移。
- 跨端枚举 `SkillImportSourceKind` 的重命名需同步 Rust serde 序列化名与前端类型。

## Acceptance Criteria

- [ ] AC1 Codex 全局 Skill target descriptor 路径为 `$CODEX_HOME/skills`，自定义 CODEX_HOME 时跟随（有测试断言）。
- [ ] AC2 Codex 项目级 Skill target descriptor 路径为 `<project>/.codex/skills`（有测试断言）。
- [ ] AC3 对全局 Codex Skills 执行 Apply 后，symlink 出现在 `$CODEX_HOME/skills/<name>` 并指向中央库（service 层测试断言）。
- [ ] AC4 导入检测仍能同时发现 `~/.agents/skills` 与 `$CODEX_HOME/skills` 中的候选技能，且来源种类/标签语义正确（import 层测试断言）。
- [ ] AC5 前端导入对话框来源标签更新，`codex_agents` 不再标注为"正式同步目标"。
- [ ] AC6 `pnpm bindings:check` 通过，Rust 枚举与 TS 类型一致。
- [ ] AC7 `pnpm check`（format/lint/typecheck/test/rust）全部通过。

## Notes

- 现网影响：已通过 EasyToAgents 同步到 `~/.agents/skills` 的 Skills 在 Codex 中不可见；修复后需在应用内对 Codex 全局 Skills 重新 Apply，旧目录残留链接手动删除。
- 本任务为缺陷修复，不引入自动迁移功能；如未来需要"一键迁移旧目录"，另立任务。
