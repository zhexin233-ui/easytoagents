# Skills 来源布局研究

## 1. 本机布局（只读观测）

观测根：`$HOME=/Users/zhexin`；仅枚举顶层，并对 `.system` 观察直接孩子。

| 来源根             | 顶层条目           | 类型/目标                                                          | 目标形态（至多两层）                                                                                                                                                                  |
| ------------------ | ------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `~/.claude/skills` | `.DS_Store`        | 普通文件                                                           | 不是技能入口                                                                                                                                                                          |
|                    | `skill-install`    | 符号链接 → `/Users/zhexin/.skills-manager/skills/skill-install`    | 目标是单技能目录（目标含 `SKILL.md`）                                                                                                                                                 |
|                    | `smart-search-cli` | 符号链接 → `/Users/zhexin/.skills-manager/skills/smart-search-cli` | 目标是单技能目录（目标含 `SKILL.md`）                                                                                                                                                 |
| `~/.codex/skills`  | `.DS_Store`        | 普通文件                                                           | 不是技能入口                                                                                                                                                                          |
|                    | `hatch-pet`        | 目录                                                               | 单技能目录（`SKILL.md` 在根）                                                                                                                                                         |
|                    | `skill-install`    | 符号链接 → `/Users/zhexin/.skills-manager/skills/skill-install`    | 目标是单技能目录（目标含 `SKILL.md`）                                                                                                                                                 |
|                    | `smart-search-cli` | 符号链接 → `/Users/zhexin/.skills-manager/skills/smart-search-cli` | 目标是单技能目录（目标含 `SKILL.md`）                                                                                                                                                 |
|                    | `.system`          | 目录                                                               | 集合目录；直接孩子为 `.codex-system-skills.marker`（文件）、`imagegen`、`openai-docs`、`plugin-creator`、`review-agent`、`skill-creator`、`skill-installer`（技能目录；本轮未读正文） |
| `~/.agents/skills` | （不存在）         | 缺失                                                               | 无可枚举条目                                                                                                                                                                          |

事实依据：本机 `find -H`/`readlink` 枚举；目标单技能的 `SKILL.md` 存在性只作结构核验，未读取正文或执行脚本。计数时 `.DS_Store`、marker 不应计为技能；`~/.claude/skills` 计 2 个普通技能入口，`~/.codex/skills` 计 3 个普通顶层技能入口（一个目录、两个链接）加 `.system` 集合中的 6 个入口，`.agents` 计 0。数字仅表示文件布局，未验证技能正文是否合法。

推断：对当前真实布局，“直接 child 枚举”足够发现两个普通根中的直属技能，但不足以发现 Codex `.system` 下的技能集合；稳妥规则应允许有限的集合目录展开，同时只接受目录自身或其直属孩子包含 `SKILL.md` 的技能入口。不能据此推断 Claude/Codex 官方是否都应导入 `.system`：它更像 Codex 内置系统集合，是否正式纳入用户可导入范围需产品选择。

## 2. 显式环境与 adapter 合同

- `ExplicitEnvironment` 字段及构造合同在 `src-tauri/src/adapters/mod.rs:203-231`：`new(home, claude_config_dir, codex_home, availability)`；缺省分别为 `home/.claude` 和 `home/.codex`，根会规范化。
- 访问器在 `src-tauri/src/adapters/mod.rs:299-309`：`claude_config_dir()`、`codex_home()`；`uses_default_claude_config_dir()` 在 `:341-343`。
- Claude adapter 的全局 Skills 目标是 `environment.claude_config_dir().join("skills")`，项目目标是 `project/.claude/skills`，格式为 `SymlinkDirectory`、选择器 `$children`：`src-tauri/src/adapters/claude/mod.rs:116-121`、`:145-150`。
- Codex adapter 的全局 Skills 目标明确是 `environment.home().join(".agents/skills")`，不是 `codex_home/.agents/skills`；项目目标是 `project/.agents/skills`，同样 `$children`/`SymlinkDirectory`：`src-tauri/src/adapters/codex/mod.rs:98-104`、`:133-139`。已有测试将此不随 `CODEX_HOME` 迁移作为合同：`src-tauri/src/adapters/mod.rs:1329-1338`。

事实：因此“额外导入来源”与“正式同步目标”不同。导入扫描若要兼容真实机器，可把 `$HOME/.claude/skills`、`$HOME/.codex/skills`（以及用户选择的其它根）作为来源；但同步/写入的正式目标由 adapter 合同决定：Claude 为解析后的 `claude_config_dir/skills`，Codex 为 `home/.agents/skills`。不能把 Codex `$CODEX_HOME/skills` 当成正式用户 Skills 目标。

## 3. 归档研究核验

- `official-config-paths.md:15-22` 记载 Codex 默认配置 `~/.codex/config.toml`、用户 Skills `$HOME/.agents/skills`，并称官方支持符号链接 Skill 目录。
- `official-config-paths.md:45-53` 记载 Claude 用户 Skills `~/.claude/skills`，支持指向其它位置的 Skill 目录符号链接并去重同一目标；`CLAUDE_CONFIG_DIR` 会改变解析后的配置根，但用户 MCP 的非默认根行为未明确，需 capability probe。
- 归档 Claude 原文 `claude-skills.md:113-119` 明确嵌套 `.claude/skills`、符号链接跟随、同一目标只加载一次；`:129-137` 说明启动目录到仓库根的项目发现、嵌套目录按访问触发、每个技能目录以 `SKILL.md` 为入口。
- `codex-skills.md` 本次 `rg` 未命中这些关键词（文件可能只有抓取/格式异常内容），故没有从该文件确认 `.system`、隐藏目录或嵌套规则；不能把缺失命中当成 Codex 不支持。

## 4. 最小来源枚举规则候选（供主代理取舍）

候选规则：对每个显式来源根只枚举直属孩子；忽略普通文件、隐藏元文件（例如 `.DS_Store`）；直属目录若自身有 `SKILL.md` 则作为单技能；否则仅对已知/显式允许的集合目录展开一层（如 `.system`），其孩子同样只接受自身含 `SKILL.md` 的目录。对符号链接先 `symlink_metadata` 识别，读取目标目录结构但不跟随任意递归；断链记为诊断并跳过。为避免循环，维护已访问目录的 canonical path；多个入口 canonical 到同一目录时聚合为一个技能并保留全部来源证据。

测试矩阵（范围仍需产品选择）：

| 维度      | 最小用例                                                                                                                                                              |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 根        | 默认 `$HOME/.claude/skills`、`$HOME/.codex/skills`；自定义 `claude_config_dir/skills`；自定义 `codex_home` 同时验证 Codex 全局仍是 `$HOME/.agents/skills`；显式额外根 |
| 形态      | 直属单技能、集合目录一层、普通文件、空目录、缺失 `SKILL.md`                                                                                                           |
| 链接      | 有效单技能链接、有效集合链接、断链、链接回祖先/循环、两个链接指向同一 canonical 目录                                                                                  |
| 名称/隐藏 | `.system`、`.DS_Store`、其它点目录；是否允许隐藏集合需产品定义                                                                                                        |
| 冲突      | Claude/Codex 同名但不同 canonical；同一 canonical 多来源；正式同步目标与额外来源重叠                                                                                  |

需要用户/主代理明确的范围：是否把 Codex `.system` 内置技能纳入可导入列表；是否扫描 `$HOME/.agents/skills`（当前缺失但官方路径）；是否允许任意一层集合目录还是只允许白名单集合；默认根之外是否接受自定义/额外根；同名不同来源展示/选择策略。上述均未替用户拍板。

## 5. 主代理收敛

- `.agents/skills` 已在当前修复范围内，无须再次询问。配置根来自 `ExplicitEnvironment`：Claude 使用解析后的 `claude_config_dir/skills`；Codex 来源为 `home/.agents/skills` 和解析后的 `codex_home/skills`（默认 `home/.codex/skills`），后者仅是兼容导入来源。
- 不扩大为任意路径的递归搜索，也不增加用户自定义搜索根配置。现有单目录选择功能继续保留。
- 用户已于本任务规划中明确“内置技能不纳入本次导入范围”。排除 `.system`，不展开或读取其中技能正文；允许显示内置集合被排除的说明。
