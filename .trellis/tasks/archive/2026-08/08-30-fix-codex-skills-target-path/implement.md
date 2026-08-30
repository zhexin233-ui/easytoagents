# Implement: 修复 Codex Skills 目标路径

前置：阅读 `.trellis/spec/backend/index.md` 与 `.trellis/spec/frontend/index.md` 及其列出的指南。

## 执行清单（按序）

1. [ ] `src-tauri/src/adapters/codex/mod.rs`
   - 全局 Skill target：`environment.home().join(".agents/skills")` → `environment.codex_home().join("skills")`
   - 项目 Skill target：`root.join(".agents/skills")` → `root.join(".codex/skills")`
2. [ ] `src-tauri/src/skills/models.rs`：`CodexCompatibility` → `CodexHome`（serde 自动为 `codex_home`）
3. [ ] `src-tauri/src/skills/import.rs`：`source_roots()` Codex 分支改为 `[(CodexHome, codex_home/skills), (CodexAgents, home/.agents/skills)]`
4. [ ] Rust 测试更新
   - `adapters/mod.rs` ~1335：断言 `custom_codex.join("skills")`，注释改为跟随 CODEX_HOME
   - `skills/service.rs` 1773 / 1872 / 2477：`.agents/skills` → `codex_home().join("skills")`
   - `skills/import.rs` 970-1009 / ~1150：来源顺序与路径断言
5. [ ] `pnpm bindings:generate`，确认 `src/bindings/commands.ts` 中 `SkillImportSourceKind` 变为 `"claude_global" | "codex_home" | "codex_agents"`
6. [ ] `src/features/skills/skill-import-dialog.tsx`：标签 map 改为 `codex_home` / `codex_agents` 新文案
7. [ ] 前端测试：`skills-page.test.tsx`、`project-detail-page.test.tsx` 中 kind 字符串与 `.agents/skills` 路径 mock 更新
8. [ ] 全量校验（见下）

## 验证命令

```bash
pnpm bindings:check
pnpm check   # format:check + lint + typecheck + vitest + cargo fmt/clippy/test
```

按 AC 逐条核对：AC1/AC2（adapters 测试）、AC3（service 测试）、AC4（import 测试）、AC5（前端标签/测试）、AC6（bindings:check）、AC7（pnpm check）。

## 回滚点

- 每步独立可 revert；最终单 commit 交付，整体 `git revert` 即回滚。

## 边界（本任务不做）

- 不迁移/删除 `~/.agents/skills` 既有链接（交付说明中列出手动清理路径）。
- 不改 Claude 路径、不改中央库、不做"一键迁移"功能。
