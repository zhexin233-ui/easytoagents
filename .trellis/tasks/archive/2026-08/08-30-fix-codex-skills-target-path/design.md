# Design: 修复 Codex Skills 目标路径

## 现状与根因

| 位置 | 现状 | 问题 |
| --- | --- | --- |
| `src-tauri/src/adapters/codex/mod.rs:101` | 全局 Skill target = `environment.home().join(".agents/skills")` | Codex 不读该目录 |
| `src-tauri/src/adapters/codex/mod.rs:136` | 项目 Skill target = `root.join(".agents/skills")` | Codex 项目级读 `.codex/skills` |
| `src-tauri/src/skills/import.rs:58-67` | `source_roots()`：`CodexAgents`=`~/.agents/skills`（主）、`CodexCompatibility`=`$CODEX_HOME/skills`（次） | 主次颠倒 |
| `src-tauri/src/skills/models.rs:151-152` | `SkillImportSourceKind::{CodexAgents, CodexCompatibility}`（serde: `codex_agents` / `codex_compatibility`） | 枚举名与新语义不符 |
| `src/features/skills/skill-import-dialog.tsx:34-35` | `codex_agents` 标注"正式同步目标" | 标签错误 |

依据：`~/.codex/skills/.system/` 下存在 Codex 自带技能（imagegen、skill-creator 等），证明 Codex 以 `$CODEX_HOME/skills` 为原生技能目录；Claude adapter 对比项正确（`~/.claude/skills`、`<root>/.claude/skills`）。

## 方案

1. **Target descriptors（核心修复）**
   - `adapters/codex/mod.rs` 全局：`environment.codex_home().join("skills")`
   - 项目：`root.join(".codex/skills")`
   - 跟随 `CODEX_HOME` 与 Codex 自身读取规则一致（原测试注释"不能随 CODEX_HOME 迁移"的旧决策作废，反向断言）。

2. **`SkillImportSourceKind` 语义修正**
   - `CodexCompatibility` → 重命名为 `CodexHome`（serde `codex_home`），指向 `$CODEX_HOME/skills`，语义=官方目录+正式同步目标。
   - `CodexAgents` 保留（serde `codex_agents`），指向 `~/.agents/skills`，语义=跨工具通用目录、仅导入来源。
   - `source_roots()` 顺序调整为 `codex_home` 在前（决定导入对话框来源展示顺序）。该枚举不入库（`db/` 无持久化），重命名仅影响 FFI 面与前端标签。
   - `.system` 内置排除逻辑遍历全部 Codex 来源根，调整后自动覆盖两个目录，无需改动（`import.rs:90-100`）。

3. **Bindings 重新生成**：`pnpm bindings:generate` 更新 `src/bindings/commands.ts` 的 `SkillImportSourceKind` 联合类型。

4. **前端标签**（`skill-import-dialog.tsx`）：
   - `codex_home`: "Codex 官方目录（正式同步目标）"
   - `codex_agents`: "Codex Agents 通用目录（仅导入来源）"

5. **测试更新**
   - `adapters/mod.rs`（~1335）：全局 Skill 路径断言改为 `custom_codex/skills`，注释反转为"跟随 CODEX_HOME，与 Codex 读取规则一致"。
   - `skills/service.rs`：1773（Apply 后链接位置）、1872（仅导入不建目录）、2477（descriptor 路径）改用 `environment.codex_home().join("skills")`。
   - `skills/import.rs`：`custom_roots_same_content_conflicts_invalid_links_and_private_paths`（970-1009，路径与来源顺序）、~1150 用例（`sources[0]`→`sources[1]` 或按新顺序改写断言）；875 的"不存在"断言保持。
   - 前端 mock：`skills-page.test.tsx`（kind 字符串与 `/isolated/home/.codex/skills` 路径、1151 过滤逻辑）、`project-detail-page.test.tsx:85`。

## 兼容性与数据影响

- `managed_targets.target_path` 为属性列，按 tool/kind/scope 识别目标；路径变化后由检测刷新，用户在 UI 内重新 Apply 即在新位置生成链接。
- 旧目录 `~/.agents/skills` 中既有链接不被触碰（R5）；因不再是受管目标，应用不会再更新它们。
- 无 DB schema 变更、无迁移脚本。

## Tradeoffs

- **不做双写**（同时维护 `.agents/skills` 与 `.codex/skills` 链接）：Codex 只读后者，双写制造"两处都是目标"的假象，违背"中央意图 → 明确落点"的产品模型。
- **跟随 CODEX_HOME 而非硬编码 `~/.codex`**：Codex 以 `$CODEX_HOME` 为根（内置 `.system` 技能即在其中），硬编码会在自定义安装下复刻同一个 bug。
- **保留 `.agents/skills` 导入来源**：零成本保住存量用户的可发现性，同时明确其"仅来源"身份。

## 回滚

单 commit 交付，`git revert` 即可整体回滚；无数据/ schema 副作用。
