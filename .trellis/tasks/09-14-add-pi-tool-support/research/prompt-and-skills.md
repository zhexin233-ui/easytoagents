# Pi 接入调研（方向 B）：Prompt / 上下文文件 · Skills · 项目资源边界

- **调研方向**：B —— Prompt / 上下文文件 + Skills + 项目资源边界
- **调研日期**：2026-09-14
- **目标工具**：Pi（`@earendil-works/pi-coding-agent`），本机版本 **0.85.1**（`pi --version` 实测）
- **本机默认配置目录**：`~/.pi/agent`（官方 env `PI_CODING_AGENT_DIR` 可覆盖，默认 `~/.pi/agent`）
- **硬性取证手段**：`smart-search fetch/search` 官方文档站 `https://pi.dev/docs/latest/`；本机权威文档副本 `.../pi-coding-agent/docs/*.md`；本机 dist 源码；本机隔离 smoke。

## 证据强度声明

| 级别                    | 含义                                                    | 本文件中的来源                                                                                                        |
| ----------------------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| **A. 官方 URL 证据**    | 官方文档站明确写出                                      | `https://pi.dev/docs/latest/{skills,usage,settings,prompt-templates,packages,environment-variables,quickstart,index}` |
| **B. 本机文件证据**     | 已安装 0.85.1 的 `docs/*.md`、`dist/**/*.js` 与实机配置 | `~/.volta/.../pi-coding-agent/{docs,dist}`、`~/.pi/agent/`                                                            |
| **C. 实机可复现 smoke** | 隔离 HOME/agentDir 下用 0.85.1 真实代码路径跑出结果     | `/tmp/pi-*` 临时 fixture（本文件给出脚本）                                                                            |
| **D. 推测**             | 无官方/源码支撑                                         | 本文件明确标注，默认按 Unknown fail closed                                                                            |

> 官方文档与 0.85.1 本机文档**逐字一致**（skills/usage/prompt-templates/settings/packages/environment-variables 已比对）。dist 源码用于补齐文档未写明的精确顺序/边界；所有影响写盘的结论都由 B+C 双证据。

## 1. 证据总表（artifact × scope × operation）

| Artifact                        | Scope                       | Import                        | Apply                         | 依据（官方 URL + 访问日期 + 版本）                                                                                                                |
| ------------------------------- | --------------------------- | ----------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Prompt                          | Global                      | **Supported**                 | **Supported**                 | 全局上下文文件 `~/.pi/agent/AGENTS.md`；https://pi.dev/docs/latest/quickstart（2026-09-14，0.85.1）；`usage#context-files`                        |
| Prompt                          | Project                     | **Unsupported**               | **Unsupported**               | 产品政策：项目 Prompt/Rules 永远 Unsupported（`docs/maintainers/adding-tool-adapter.md` §1）；pi 虽支持项目 `AGENTS.md`，但**不因工具支持而开放** |
| Skills                          | Global                      | **Supported**                 | **Supported**                 | `~/.pi/agent/skills/`；https://pi.dev/docs/latest/skills#locations（2026-09-14，0.85.1）                                                          |
| Skills                          | Project                     | **Supported**（trust-gated）  | **Supported**（trust-gated）  | `.pi/skills/` 仅项目受信任后加载；https://pi.dev/docs/latest/skills#locations + https://pi.dev/docs/latest/settings#project-trust                 |
| Skills                          | Project（`.agents/skills`） | 识别为**非目标**              | 不写入                        | pi 也会发现 `<cwd→git root>/.agents/skills`，但属跨工具共享目录，EasyToAgents 只写工具自有 `.pi/skills`                                           |
| PromptTemplate                  | Global                      | **Unsupported（本次范围外）** | **Unsupported（本次范围外）** | pi 支持 `~/.pi/agent/prompts/*.md`，但不属于本次六类资源模型；见 §Q6                                                                              |
| PromptTemplate                  | Project                     | **Unsupported（本次范围外）** | **Unsupported（本次范围外）** | 同上；且其项目作用域与「项目 Prompt 永远 Unsupported」冲突，不能归入 Prompt                                                                       |
| Provider / MCP / Hooks / Agents | —                           | 不在本方向 B 范围             | —                             | 由其他方向调研                                                                                                                                    |

**状态语义**：`Supported` = 官方合同明确且本机 smoke 复现；`Unsupported` = 官方不支持**或产品选择不纳入**；`Unknown` = 证据不足，fail closed；`ToolNotInstalled` = 合同已知但探针不可信。

---

## 2. 逐条结论

### Q1. 全局 Prompt/上下文：加载语义、精确优先级、EasyToAgents 最安全的接管点

**结论（A+B+C）**：Pi 的全局指令载体是 **`~/.pi/agent/AGENTS.md`**（agent dir 由 `PI_CODING_AGENT_DIR` 覆盖，默认 `~/.pi/agent`）。EasyToAgents 应接管该文件，并**必须 fail closed / 诊断以下三类失效场景**。

**官方引用**：

- https://pi.dev/docs/latest/quickstart（访问 2026-09-14，0.85.1）：
  > Pi loads: `~/.pi/agent/AGENTS.md` for global instructions; `AGENTS.md` or `CLAUDE.md` from parent directories and the current directory. If a directory contains `AGENTS.override.md`, Pi loads it instead of `AGENTS.md` or `CLAUDE.md` from that directory.
- https://pi.dev/docs/latest/usage#context-files（访问 2026-09-14，0.85.1）：同上，并说明 `--no-context-files`/`-nc` 可整体禁用。

**精确优先级（B 源码 + C smoke 双重确认）**：

1. **同目录候选文件顺序**（`dist/core/resource-loader.js:32-53` `loadContextFileFromDir`；★ 官方未文档化此完整候选表）：
   `AGENTS.override.md` → `AGENTS.md` → `AGENTS.MD` → `CLAUDE.md` → `CLAUDE.MD`，**取第一个存在的普通文件**，其余全部忽略。
2. **全局文件来自 agent dir**：`loadContextFileFromDir(resolvedAgentDir)`，即 `~/.pi/agent/AGENTS.md`（`resource-loader.js:87-90`）。因此 `~/.pi/agent/AGENTS.override.md` 会**替换**我们的 `AGENTS.md`。
3. **拼接顺序**（`loadProjectContextFiles`，`resource-loader.js:82-110`）：
   **全局 agent dir → 最远祖先目录 → … → cwd**（`ancestorContextFiles.unshift`，故 cwd 最后）。不是「cwd 优先」，而是按目录从外到内顺序拼接，系统提示里以多个 `<project_instructions path="…">` 节点呈现（`dist/core/system-prompt.js:21-33`，与 `system-prompt.js:103-115`）。
4. **不限定 git 仓库**：无 git root 时向上查到文件系统根，可能命中 `~/AGENTS.md`、`/Users/AGENTS.md` 等祖先文件（`resource-loader.js:93-105`）。
5. **`SYSTEM.md` / `APPEND_SYSTEM.md`**（B 源码 `resource-loader.js:806-830`；A 官方 `usage#system-prompt-files`）：
   - `<agent_dir>/SYSTEM.md`（全局）或 `<project>/.pi/SYSTEM.md`（受信任项目）**替换系统提示**；项目优先，项目受信任且存在时不再用全局。
   - `<agent_dir>/APPEND_SYSTEM.md` 或 `<project>/.pi/APPEND_SYSTEM.md` **追加**到系统提示。
   - **关键**：C smoke 证明，即使 `SYSTEM.md` 替换了系统提示，`AGENTS.md` 仍作为 `<project_context>` 被追加，**不会使我们的写入失效**（见 §Q7 case5）。

**C smoke（0.85.1 真实代码路径）关键结果**：

| case | 文件状态                              | 实际加载                                                                    |
| ---- | ------------------------------------- | --------------------------------------------------------------------------- |
| 1    | 仅 `AGENTS.md`                        | `AGENTS.md` ✅                                                              |
| 2    | `AGENTS.md` + `AGENTS.override.md`    | **仅 `AGENTS.override.md`**（我们的 AGENTS.md 被静默忽略）                  |
| 3    | 仅 `CLAUDE.md`                        | `CLAUDE.md` ✅（回退生效）                                                  |
| 4    | 我们的 `AGENTS.md` + 用户 `CLAUDE.md` | 仅 `AGENTS.md`（**用户的 CLAUDE.md 被我们遮蔽**）                           |
| 5    | + `SYSTEM.md` + `APPEND_SYSTEM.md`    | 系统提示被 `SYSTEM.md` 替换、`APPEND_SYSTEM.md` 追加，`AGENTS.md` 仍加载 ✅ |

**回答任务三问**：

- **EasyToAgents 接管哪个文件最安全？** → `~/.pi/agent/AGENTS.md`（`WholeDocument`）。它不会替换 pi 内置系统提示（区别于危险的 `SYSTEM.md`），且即使存在 `SYSTEM.md`/`APPEND_SYSTEM.md` 仍生效。**不要**接管 `SYSTEM.md`（会清空 pi 内置能力与工具说明）、**不要**接管 `APPEND_SYSTEM.md`（语义是追加而非「全局指令」）、**不要**写 `~/.agents/skills` 等共享目录。
- **override/system 文件是否会静默覆盖/使写入无效？**
  - `~/.pi/agent/AGENTS.override.md` 存在 → **我们的 AGENTS.md 静默无效**，必须诊断。
  - `~/.pi/agent/CLAUDE.md`/`CLAUDE.MD` 存在而我们要写 `AGENTS.md` → 我们**反向遮蔽用户文件**，导入也会漏读，需诊断。
  - `~/.pi/agent/AGENTS.MD` 同理会被我们的 `AGENTS.md` 遮蔽。
  - `SYSTEM.md` / `APPEND_SYSTEM.md` → **不使写入无效**，无需 fail closed（但可在诊断中提示语义变化）。
- **是否需要 fail closed？** → **需要**。至少：
  - `AGENTS.override.md` 存在 ⇒ 视为 `PromptOverrideState::Present`，Apply 前告警/拒绝（建议升级为硬阻断，因为写入必然无效）。
  - 目标是用户自有 `CLAUDE*.md`/`AGENTS.MD` 的唯一指令源 ⇒ 输出 takeover/遮蔽风险诊断。
  - `--no-context-files`（`-nc`）为运行时 CLI 开关，静态不可探测 ⇒ 标 `Unknown` 并在文档明示无法保证。

---

### Q2. 官方是否提供项目级 Prompt/Rules？

**官方（A）**：是。Pi 支持项目 `AGENTS.md`/`CLAUDE.md`（含祖先目录向上查找）与受信任项目 `.pi/SYSTEM.md`/`.pi/APPEND_SYSTEM.md`（`usage#context-files`、`usage#system-prompt-files`，访问 2026-09-14，0.85.1）。

**产品结论（必须保持）**：**项目作用域 Prompt/Rules 永远 Unsupported**。依据 `docs/maintainers/adding-tool-adapter.md` §1：「For Prompt/Rules, Import and Apply are evaluated only for the supported global scope. A project-scope result is always `Unsupported`…」；「Registered project files such as `CLAUDE.md`, `AGENTS.md` … remain user-owned; project discovery and synchronization do not observe, disable, restore, or rewrite them.」

因此：Projects 服务不得为 Pi 产生任何项目 Prompt descriptor/assignment/native-resource 行；项目 `AGENTS.md`、`.pi/SYSTEM.md`、`.pi/APPEND_SYSTEM.md` 属用户自有，EasyToAgents 不观察、不写入、不恢复。**不因为 pi 原生支持项目 AGENTS.md 而提议开放。**

---

### Q3. Skills 发现：完整目录集、确定顺序、去重/覆盖、frontmatter 约束

**官方目录集（A，https://pi.dev/docs/latest/skills#locations，访问 2026-09-14，0.85.1）**：

- Global：`~/.pi/agent/skills/`、`~/.agents/skills/`
- Project（**仅项目受信任后**）：`.pi/skills/`、`.agents/skills/`（cwd 向上到 git root，非仓库则到文件系统根）
- Packages：`skills/` 目录或 `package.json` 的 `pi.skills`
- Settings：`skills` 数组中的文件或目录
- CLI：`--skill <path>`（可重复；`--no-skills` 下仍生效）

**明确否定**：`~/.pi/skills/` **不是** pi 的 skill 位置（仅 `~/.pi/agent/skills/`）。任务中的疑点已排除。

**发现规则（A+B）**：

- `~/.pi/agent/skills/` 与 `.pi/skills/`：**根目录直属 `.md`**（含 `SKILL.md`）在具备合法 frontmatter + 非空 `description` 时作为单个 skill；目录含 `SKILL.md` 视为 skill root 并**不再递归深入**；否则递归子目录找 `SKILL.md`（`dist/core/skills.js:120-215`）。
- `~/.agents/skills/` 与项目 `.agents/skills/`：**根 `.md` 被忽略**，但分组子目录里的 `.md` 声明 frontmatter 时被发现（`skills.js:180-190`，mode=`agents`）。
- 无合法 frontmatter 的普通 Markdown 静默忽略。
- `.gitignore`/`.ignore`/`.fdignore` 在 skill 目录内生效（`skills.js:26-70`）。

**确定顺序与覆盖规则（B 源码，★ 官方文档未给出完整顺序）**：

`dist/core/package-manager.js:51-64` `resourcePrecedenceRank`（lower = higher precedence，`toResolvedPaths` 升序排序，`loadSkills` **保留第一个**同名 skill）：

| rank | 来源                                                               |
| ---- | ------------------------------------------------------------------ |
| 0    | project + settings 显式条目（`source: "local", scope: "project"`） |
| 1    | project + 自动发现（`.pi/skills`、`.agents/skills`）               |
| 2    | user + settings 显式条目                                           |
| 3    | user + 自动发现（`~/.pi/agent/skills`、`~/.agents/skills`）        |
| 4    | package 资源                                                       |

即：**project 显式 > project 自动 > user 显式 > user 自动 > package**。同一位置内的目录遍历用 `readdirSync` 顺序（非承诺稳定）。
同名冲突：**保留第一个，其余记 `collision` 诊断**（`skills.js:318-345`；A 官方 skills.md：「Name collisions … keep the first skill found」）。
符号链接去重：`canonicalizePath` 的 `realPathSet`，完全指向同一 realpath 的重复项**静默跳过**（`skills.js:322-329`）。

**C smoke（0.85.1 真实 loader，trust=true）实测顺序**：

```
1. collide      (project / .pi/skills,        scope=project, src=auto)  ← 冲突 winner
2. proj-dup     (project / .pi/skills)
3. agents-proj  (project / .agents/skills)
4. ext-extra    (user settings path,           src=local)
5. global-dup   (user / ~/.pi/agent/skills,    src=auto)
COLLISION collide: winner=…/proj/.pi/skills/collide/SKILL.md  loser=…/agent/skills/collide/SKILL.md
```

→ **项目同名 skill 会赢过全局同名 skill**，EasyToAgents 的中央副本 + 链接策略必须考虑这一点（项目分配与全局分配同名时，pi 实际用项目版）。

**SKILL.md frontmatter 官方要求（A+B）**：

| 字段                                                                            | 官方要求                                                                      | 0.85.1 实现（源码/smoke）                                                            |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `name`                                                                          | **必填**，≤64 字符，小写 `a-z0-9-`，无首尾/连续连字符；**不要求与父目录同名** | 缺失时**回退为父目录名**并仍加载；非法字符仅 **warning，仍加载**                     |
| `description`                                                                   | **必填**，≤1024 字符                                                          | **缺失/空白 ⇒ 不加载**（warning）                                                    |
| `license`/`compatibility`/`metadata`/`allowed-tools`/`disable-model-invocation` | 可选                                                                          | 未知字段忽略；`disable-model-invocation: true` 从系统提示隐藏，仅 `/skill:name` 可用 |

C smoke 验证：`name` 缺失（回退目录名）→ 加载；`Bad--Name`（非法）→ warning 仍加载；`name` 与目录名不符 → 加载；缺 `description` → 不加载；根目录 `notes.md` 无 frontmatter → 静默忽略。

**对 EasyToAgents 的含义**：中央 skill 名与链接目录名应严格 `^[a-z0-9][a-z0-9-]{0,63}$`，并校验 SKILL.md frontmatter `name` 与中央 name 一致；不一致会导致 Pi 使用 frontmatter 名 → 与中央名不一致（观测/漂移/冲突判断需以 frontmatter name 为准或直接拒绝）。

---

### Q4. 符号链接兼容性

**结论：Supported（A 部分 + B 源码 + C smoke + 实机）**。pi **跟随**符号链接的 skill 目录与符号链接的 `SKILL.md`。

- **官方文档**：未明确写「是否跟随 symlink」（A 缺口，标 Unknown 级别的措辞）。
- **本机源码证据（B，0.85.1）**：
  - `dist/core/skills.js:156-166`：目录项 `entry.isSymbolicLink()` 时用 `statSync` 解析，指向文件且为 `SKILL.md` 则加载。
  - `dist/core/skills.js:186-201`：符号链接指向**目录**时 `statSync` 判定为目录并递归进入。
  - `dist/core/package-manager.js:203-260`（`collectSkillEntries`）同样对 symlink 做 `statSync` 解析。
  - `skills.js:322-329`：用 `canonicalizePath`（realpath）去重，避免同一目标经多条链接重复加载。
  - 断链：`statSync` 抛错 ⇒ 静默 `continue`，无诊断（`skills.js:163-166, 196-200`）。
- **C smoke（0.85.1 真实 `loadSkills`）**：
  - `agent/skills/symlinked-skill -> /tmp/.../central/symlinked-skill` ⇒ **加载成功**，`filePath` 为链接路径。
  - `agent/skills/file-symlink-skill/SKILL.md -> .../REAL.md` ⇒ **加载成功**。
  - 断链目录 ⇒ 静默跳过，无诊断。
- **实机证据（C，本机真实配置）**：`~/.pi/agent/skills/smart-search-cli` 当前即为 EasyToAgents 写入的符号链接：
  `smart-search-cli -> /Users/zhexin/Library/Application Support/com.easytoagents.desktop/skills/smart-search-cli`（`ls -la ~/.pi/agent/skills/`，2026-09-14），且该 skill 在本机 pi 会话的 system prompt `<available_skills>` 中可见并可被加载。**这直接证明现有 EasyToAgents 的 Skill 同步链路对 Pi 生效。**

**对 EasyToAgents 的含义**：`SymlinkPolicy::ManagedChildrenOnly` 对 Pi Skills 是成立的（与 Claude/Codex/Cursor/ZCode 一致）。仍建议在 descriptor 与 status 中保留「symlink 是否被发现」的实机 smoke 结果，避免未来 pi 版本改动导致静默失效。

---

### Q5. 项目 trust 对 Skills 的影响

**结论（A+B+C）**：项目 `.pi/skills` / `.agents/skills` **只有在项目受信任后才会被发现**；未信任时静默忽略（无错误），并会因 `.pi/skills` 的存在触发 trust 流程。

**官方引用**：

- https://pi.dev/docs/latest/skills#locations（访问 2026-09-14，0.85.1）：
  > Project (only after the project is trusted): `.pi/skills/`, `.agents/skills/`…
- https://pi.dev/docs/latest/settings#project-trust（访问 2026-09-14，0.85.1）：
  > Non-interactive modes (`-p`, `--mode json`, `--mode rpc`) do not show a trust prompt. Without an applicable saved trust decision, they use `defaultProjectTrust` … `ask` (default) and `never` ignore those project resources, while `always` trusts them.
  > Set it to `"ask"`, `"always"`, or `"never"` in `~/.pi/agent/settings.json`…

**`defaultProjectTrust` 取值语义（A）**：

| 值            | 交互式       | 非交互式（`-p`/`--mode json`/`--mode rpc`） |
| ------------- | ------------ | ------------------------------------------- |
| `ask`（默认） | 弹出信任询问 | 无询问 ⇒ **忽略**项目资源                   |
| `always`      | 自动信任     | 信任                                        |
| `never`       | 不信任       | 忽略                                        |

补充（A）：信任决定持久化在 `~/.pi/agent/trust.json`，**最近的祖先目录条目生效**；`--approve/-a`、`--no-approve/-na` 可按次覆盖；`/trust` 写 trust.json 但不热重载。`pi config`/package 命令走同一 trust 流（`pi update` 不询问）。

**trust-requiring 判定（B，`dist/core/trust-manager.js:8-16,145-168`）**：`.pi/` 下存在 `settings.json | extensions | skills | prompts | themes | SYSTEM.md | APPEND_SYSTEM.md` 任一，或 cwd→祖先存在 `.agents/skills`，即触发信任要求。**写入 `.pi/skills` 本身就会让 pi 认为该项目「需要信任」。**

**C smoke 实测（production `DefaultResourceLoader.reload`，0.85.1）**：

| 项目 trust                  | 项目 `.pi/skills` / `.agents/skills`              |
| --------------------------- | ------------------------------------------------- |
| `resolveProjectTrust=false` | **完全不加载**（仅全局 + user settings 路径生效） |
| `resolveProjectTrust=true`  | 加载，且 rank 高于 user（见 Q3）                  |

**对 EasyToAgents 的含义**：

- 项目 Skills 写入后可能**静默无效**（非交互 `ask`/`never` 或无 trust 记录），状态诊断必须读取 `~/.pi/agent/settings.json` 的 `defaultProjectTrust` 与 `~/.pi/agent/trust.json` 的最近祖先决定，映射到 `TargetTrustState::{Trusted,Untrusted,Unknown,NotRequired}`。
- 写入项目 `.pi/skills` 会改变 pi 的信任判定（触发询问）。这是产品行为副作用，需在 Preview/Apply 文案与诊断中明示（可复用 Codex 的 project-trust 诊断模式）。

---

### Q6. Prompt Templates 与现有资源模型的关系

**事实（A）**：https://pi.dev/docs/latest/prompt-templates（访问 2026-09-14，0.85.1）：

> Pi loads prompt templates from: Global `~/.pi/agent/prompts/*.md`; Project `.pi/prompts/*.md` (only after the project is trusted); Packages …; Settings `prompts` array …; CLI `--prompt-template`. 文件名即命令名（`review.md` → `/review`），用 `/name` 显式调用，`description`/`argument-hint` 可选。

**与现有资源模型的对应：不对应任何一类。** 建议如下：

> **建议：本次不纳入 Prompt Templates；不要将其视为 `Prompt` 资源。** 若将来接入，应作为**独立的新 artifact（例如 `PromptTemplate`）**，用「中央不可变副本 + 逐名称受管符号链接/文件」建模（与 Skills/Agents 同构），而不是复用单文件 `Prompt`。

理由：

1. **语义不同**：`Prompt` 是**始终注入系统提示**的全局指令文档（`$document`，如 Claude `CLAUDE.md`、Codex `AGENTS.md`）；Prompt Template 是**用户显式 `/name` 触发的片段展开**，默认不进入上下文。混为 `Prompt` 会破坏 Prompt 的单一受管文档语义。
2. **作用域冲突**：模板原生支持项目 `.pi/prompts`，而产品合同要求项目 Prompt **永远 Unsupported**。把模板归入 `Prompt` 会强制开放项目 Prompt 或产生自相矛盾的能力矩阵。作为独立 artifact 才能单独声明「项目作用域」。
3. **文件形态不同**：`Prompt` 是单文件整文接管；模板是多文件、以文件名为主键、带 `description`/`argument-hint` frontmatter，需要逐名称 ownership（更像 Skills）。
4. **触发不同**：模板依赖 `/name` 命令，不参与 skill 的渐进披露，也不接受 `name` frontmatter（命令名来自文件名）。

因此本次 Pi 能力矩阵中：Prompt Templates = **Unsupported（产品范围外）**，不新增 descriptor；保持 fail closed。

---

### Q7. 隔离验证建议（可复现 smoke）

**已验证可跑（0.85.1，2026-09-14）**：

**(1) `PI_CODING_AGENT_DIR` 隔离生效（CLI 级）**

```bash
BASE=$(mktemp -d /tmp/pi-cli.XXXXXX); AGENT="$BASE/agent"; mkdir -p "$AGENT"
printf '{"theme":"dark"}' > "$AGENT/settings.json"
PI_CODING_AGENT_DIR="$AGENT" pi list        # 实测输出 "No packages installed."，未读取真实 ~/.pi/agent（真实环境有 9 个 package）
PI_CODING_AGENT_DIR="$AGENT" pi --version   # 0.85.1
```

说明：`pi` 无「列出 skills」子命令，`pi list` 只能证明 agent dir 解析被隔离；skills 发现需走代码路径 smoke（下）。

**(2) 生产路径 skills 发现 + trust + 顺序（推荐，最强）**

```bash
BASE=$(mktemp -d /tmp/pi-prod.XXXXXX); AGENT="$BASE/agent"; PROJ="$BASE/proj"
mkdir -p "$AGENT/skills/global-dup" "$PROJ/.pi/skills/proj-dup" "$PROJ/.agents/skills/agents-proj"
printf -- '---\nname: global-dup\ndescription: GLOBAL\n---\ng\n' > "$AGENT/skills/global-dup/SKILL.md"
printf -- '---\nname: proj-dup\ndescription: PROJ\n---\np\n' > "$PROJ/.pi/skills/proj-dup/SKILL.md"
printf -- '---\nname: agents-proj\ndescription: AGENTS\n---\na\n' > "$PROJ/.agents/skills/agents-proj/SKILL.md"
cat > "$BASE/prod.mjs" <<'EOF'
import { DefaultResourceLoader } from "/Users/zhexin/.volta/tools/image/packages/@earendil-works/pi-coding-agent/lib/node_modules/@earendil-works/pi-coding-agent/dist/index.js";
const [agentDir, projRoot] = process.argv.slice(2);
for (const trust of [false, true]) {
  const l = new DefaultResourceLoader({ cwd: projRoot, agentDir });
  await l.reload({ resolveProjectTrust: async () => trust });
  console.log(`\n== trust=${trust} ==`);
  for (const s of l.getSkills().skills) console.log(`  ${s.name} | ${s.sourceInfo?.scope}/${s.sourceInfo?.source} | ${s.filePath}`);
}
EOF
node "$BASE/prod.mjs" "$AGENT" "$PROJ"
```

实测：`trust=false` 只出现全局 skill；`trust=true` 出现项目 `.pi/skills` 与 `.agents/skills`，且项目同名 skill 赢得 collision。

**(3) symlink 跟随 + AGENTS 拼接/override（生产路径）**

```bash
BASE=$(mktemp -d /tmp/pi-sym.XXXXXX); AGENT="$BASE/agent"; CEN="$BASE/central"; PROJ="$BASE/proj"
mkdir -p "$AGENT/skills" "$CEN/sym-skill" "$PROJ/sub"
printf -- '---\nname: sym-skill\ndescription: sym\n---\nb\n' > "$CEN/sym-skill/SKILL.md"
ln -s "$CEN/sym-skill" "$AGENT/skills/sym-skill"
printf 'GLOBAL-AGENTS\n' > "$AGENT/AGENTS.md"
printf 'ROOT-ANCESTOR\n' > "$PROJ/AGENTS.md"
printf 'SUB-CWD\n' > "$PROJ/sub/AGENTS.md"
cat > "$BASE/sym.mjs" <<'EOF'
import { DefaultResourceLoader } from "/Users/zhexin/.volta/tools/image/packages/@earendil-works/pi-coding-agent/lib/node_modules/@earendil-works/pi-coding-agent/dist/index.js";
const [agentDir, projRoot] = process.argv.slice(2);
const l = new DefaultResourceLoader({ cwd: projRoot + "/sub", agentDir });
await l.reload({ resolveProjectTrust: async () => true });
for (const s of l.getSkills().skills) console.log("SKILL", s.name, s.filePath);
for (const f of l.getAgentsFiles().agentsFiles) console.log("CTX", f.path, "::", f.content.trim());
EOF
node "$BASE/sym.mjs" "$AGENT" "$PROJ"
```

实测：symlink skill 被加载；上下文顺序为 `agent/AGENTS.md → proj/AGENTS.md → proj/sub/AGENTS.md`（全局→最远祖先→cwd）。把 `agent/AGENTS.md` 换成 `AGENTS.override.md` 后只加载 override。

**未能验证 / 缺口**：

- **CLI 端完整 skills 列表**：pi 无非交互「列出已加载 skills」命令；用真实模型跑 `-p` 才能看系统提示，需要凭据与网络。本次用官方导出的 `DefaultResourceLoader` 走同一生产代码路径替代，可复现且无需模型。
- **未来版本回归**：symlink 跟随、候选文件表、precedence rank 均来自 0.85.1 源码，官方文档未全部承诺；升级 pi 后应重跑 (2)(3) 作为回归 fixture。

---

## 3. 危险场景（会导致我们写错 / 静默无效）

| #   | 场景                                                                       | 后果                                                                   | 需要的防护                                                                                          |
| --- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| D1  | `~/.pi/agent/AGENTS.override.md` 存在                                      | 我们的 `~/.pi/agent/AGENTS.md` **静默无效**                            | descriptor `prompt_override=Present`；Apply 前告警并建议硬阻断；诊断 `PI_PROMPT_OVERRIDE_PRESENT`   |
| D2  | 用户仅用 `~/.pi/agent/CLAUDE.md`（或 `CLAUDE.MD`）                         | Import 漏读用户实际全局指令；我们写 `AGENTS.md` 后**反向遮蔽**用户文件 | 检测回退文件；诊断 `PI_PROMPT_CLAUDE_FALLBACK_PRESENT`（导入不完整 + 遮蔽风险）                     |
| D3  | 用户有 `~/.pi/agent/AGENTS.MD`                                             | 我们写 `AGENTS.md` 后遮蔽其大写变体                                    | 同上，纳入候选检测                                                                                  |
| D4  | `defaultProjectTrust=never` 或非交互 `ask`，或 trust.json 无祖先记录       | 项目 `.pi/skills` **静默不加载**                                       | `TargetTrustState` + 诊断 `PI_PROJECT_SKILLS_UNTRUSTED` / `PI_PROJECT_SKILLS_TRUST_UNKNOWN`         |
| D5  | 用户 `settings.json` 的 `skills` 数组含自定义目录（如 `~/.claude/skills`） | 额外 skill 源参与同名优先级（rank 2/0），可能压过我们的链接            | 读取 settings `skills` 列表；非空时诊断 `PI_SKILLS_SETTINGS_EXTRA_PATHS`                            |
| D6  | 用户 `settings.json` 用 `!`/`-` 排除我们的中央链接名                       | 我们的 skill 被禁用                                                    | 同上；诊断提示排除模式命中                                                                          |
| D7  | 同名 skill 同时存在于项目与全局                                            | pi 使用**项目版**（rank 0/1 > 2/3），与 EasyToAgents 预期可能不符      | 状态按 scope 聚合；诊断 `PI_SKILL_NAME_COLLISION`                                                   |
| D8  | 断链或链接指向目录逃逸                                                     | 断链被 pi **静默忽略**（无诊断）                                       | Apply 前后自检链接有效性；诊断 `PI_SKILL_SYMLINK_BROKEN` / `PI_SKILL_SYMLINK_ESCAPE`                |
| D9  | 目录名与 SKILL.md frontmatter `name` 不一致                                | pi 以 frontmatter name 加载，中央名与实际名不符                        | 中央导入/写入时校验；诊断 `PI_SKILL_FRONTMATTER_NAME_MISMATCH`                                      |
| D10 | 缺 `description` 的 SKILL.md                                               | pi **不加载**且仅 warning                                              | Apply 前校验；诊断 `PI_SKILL_DESCRIPTION_MISSING`                                                   |
| D11 | 运行 pi 时带 `--no-skills` / `-ns` 或 `--no-context-files` / `-nc`         | 全部/部分资源被禁，静态不可探测                                        | 标 `Unknown`，写入文档「无法保证」；不做假 Supported 承诺                                           |
| D12 | 使用 `PI_CODING_AGENT_DIR` 指向非默认目录                                  | 我们写到 `~/.pi/agent` 而 pi 读别处                                    | 必须用同一显式 `pi_agent_dir`（默认 `~/.pi/agent`），不可硬编码 HOME 拼接                           |
| D13 | 项目 `.agents/skills` 存在                                                 | 触发 trust；且与 `.pi/skills` 有独立优先级                             | EasyToAgents 只写 `.pi/skills`；检测 `.agents/skills` 并诊断 `PI_PROJECT_SKILLS_SHARED_DIR_PRESENT` |

---

## 4. 落地建议（descriptor / ownership / symlink / 诊断码）

### 4.1 路径与环境

- 新增 `environment.pi_agent_dir()`，默认 `~/.pi/agent`，允许 `PI_CODING_AGENT_DIR` 覆盖（A：https://pi.dev/docs/latest/environment-variables> ，访问 2026-09-14，0.85.1：“`PI_CODING_AGENT_DIR` — Override the config directory; default is `~/.pi/agent`”）。**禁止**从进程读取真实 HOME 之外的隐式路径（沿用现有 adapter 的显式注入约定）。
- 探针：CLI `pi --version`（PATH），失败可回退到 `ToolNotInstalled`（`TargetCapability::tool_not_installed()`）。

### 4.2 Descriptor 建议

| artifact | scope   | path                        | format             | managed_selectors | symlink_policy        | 其它                                                                                                              |
| -------- | ------- | --------------------------- | ------------------ | ----------------- | --------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Prompt   | Global  | `<pi_agent_dir>/AGENTS.md`  | `Markdown`         | `["$document"]`   | `Reject`              | `prompt_override = discover_pi_prompt_override(<pi_agent_dir>/AGENTS.override.md)`；`allowed_root = pi_agent_dir` |
| Prompt   | Project | ——                          | ——                 | ——                | ——                    | **不生成 descriptor**（永远 Unsupported，服务入口前拒绝）                                                         |
| Skill    | Global  | `<pi_agent_dir>/skills`     | `SymlinkDirectory` | `["$children"]`   | `ManagedChildrenOnly` | 只写受管名称的逐名称链接                                                                                          |
| Skill    | Project | `<project_root>/.pi/skills` | `SymlinkDirectory` | `["$children"]`   | `ManagedChildrenOnly` | `allowed_root = project_root`；`trust` 由 trust.json + `defaultProjectTrust` 推导                                 |

- **不要**为 `~/.agents/skills` 或 `<project>/.agents/skills` 生成可写目标（跨工具共享，非 EasyToAgents 所有）。可作为只读冲突检测来源。
- **不要**为 `~/.pi/skills` 生成目标（不是官方位置）。
- **不要**为 `<project>/.pi/AGENTS.md`、`.pi/SYSTEM.md`、`.pi/APPEND_SYSTEM.md` 生成任何 descriptor（项目 Prompt 永远 Unsupported）。
- 全局 Prompt 的 `managed_selector_roots` 用 `$document`（整文接管），与 Claude `CLAUDE.md`、Codex `AGENTS.md` 一致。
- `policy`：Pi 无 Claude 式 `strictPluginOnlyCustomization` 等价物 → `PolicyState::Allowed`（本机未发现封锁机制；若将来出现以官方为准）。

### 4.3 Ownership / 敏感 selector

- Prompt：`$document`（整体受管单文件）；无敏感子结构（非 JSON）。接管语义与现有 `Prompt` 一致。
- Skills：`$children`（目录级），逐名称 ownership；`SymlinkPolicy::ManagedChildrenOnly`——只替换/删除 EasyToAgents 自己创建的链接，外部链接、普通目录、断链、逃逸都走现有冲突保护。
- 符号链接策略与 Claude/Codex/Cursor/ZCode 保持一致，证据见 §Q4。

### 4.4 诊断码建议（沿用 SCREAMING_SNAKE + 工具前缀约定）

| 诊断码                                 | 语义                                                            |
| -------------------------------------- | --------------------------------------------------------------- |
| `PI_INSTALLATION_PROBE_UNSUPPORTED`    | 探针不可信（对齐其他工具命名）                                  |
| `PI_PROJECT_PROMPT_UNSUPPORTED`        | 项目 Prompt 永远 Unsupported（产品政策）                        |
| `PI_PROMPT_OVERRIDE_PRESENT`           | `<pi_agent_dir>/AGENTS.override.md` 存在，我们的 AGENTS.md 无效 |
| `PI_PROMPT_OVERRIDE_UNKNOWN`           | override 状态无法判定                                           |
| `PI_PROMPT_CLAUDE_FALLBACK_PRESENT`    | 用户依赖 `CLAUDE.md`/`CLAUDE.MD` 回退：导入不完整 + 将被遮蔽    |
| `PI_PROMPT_AGENTS_UPPERCASE_PRESENT`   | 用户有 `AGENTS.MD`，会被我们的 `AGENTS.md` 遮蔽                 |
| `PI_PROJECT_SKILLS_UNTRUSTED`          | 项目未受信任，`.pi/skills` 不加载                               |
| `PI_PROJECT_SKILLS_TRUST_UNKNOWN`      | trust 状态不可判定（fail closed）                               |
| `PI_PROJECT_SKILLS_SHARED_DIR_PRESENT` | 项目 `.agents/skills` 存在（共享目录 + 触发 trust）             |
| `PI_SKILLS_SETTINGS_EXTRA_PATHS`       | `settings.json` 的 `skills` 非空，存在额外源/排除模式           |
| `PI_SKILL_NAME_COLLISION`              | 同名 skill 跨 scope/来源冲突（pi 保留第一个）                   |
| `PI_SKILL_SYMLINK_BROKEN`              | 受管链接断链（pi 静默忽略）                                     |
| `PI_SKILL_SYMLINK_ESCAPE`              | 链接指向 allowed_root 之外                                      |
| `PI_SKILL_FRONTMATTER_NAME_MISMATCH`   | 目录名/中央名与 SKILL.md `name` 不一致                          |
| `PI_SKILL_DESCRIPTION_MISSING`         | SKILL.md 缺 `description`，pi 不加载                            |

> 提示：现有 `WARNING_CODEX_PROMPT_OVERRIDE` 是 Codex 专用常量（`sync/mod.rs:51`）。Pi 需要一个等价的 Pi 前缀码；不要复用 Codex 名字，以免能力矩阵文案混淆。

### 4.5 服务与同步要点

- Prompt 全局 Import：读 `<pi_agent_dir>/AGENTS.md`（`$document`）；若 `AGENTS.override.md` 存在则 Import 结果不代表实际生效内容，必须带诊断；不要自动读取 override 作为受管内容（它是用户自有冲突文件）。
- Skills Import：扫 `<pi_agent_dir>/skills`；符号链接、子目录、frontmatter 兼容性按 §Q3/Q4 处理；跳过非受管/共享目录。
- Preview/Apply/Restore：沿用通用 snapshot/journal；项目 Skills 需带 trust 门禁，未信任时禁止 Apply 或强制警告（对齐 Codex 的 `untrusted` 模式）。
- 状态聚合：目录 descriptor 不可写入，逐受管名称聚合 `SyncStatus`，取最严重状态；目录内未受管文件不进入 `managed_targets`（沿用 `adding-tool-adapter.md` §11.7）。

---

## 5. 复现命令与 fixture 摘要（2026-09-14，0.85.1）

| smoke                 | 命令要点                                                              | 实测结论                                                                         |
| --------------------- | --------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| agent dir 隔离        | `PI_CODING_AGENT_DIR=$tmp pi list`                                    | 只读临时 agent dir（返回 “No packages installed”），不读真实 `~/.pi/agent`       |
| 生产路径 skills/trust | `DefaultResourceLoader({cwd,agentDir}).reload({resolveProjectTrust})` | untrusted 忽略项目 skills；trusted 加载且项目优先                                |
| symlink 跟随          | `ln -s` skill 目录 / `SKILL.md`                                       | 均被发现并加载；断链静默忽略                                                     |
| AGENTS 优先级         | 全局 + 祖先 + cwd 的 AGENTS.md                                        | 顺序：全局→最远祖先→cwd；`AGENTS.override.md` 替换同目录候选                     |
| SYSTEM/APPEND         | `SYSTEM.md` + `APPEND_SYSTEM.md`                                      | SYSTEM 替换系统提示、APPEND 追加，但 `AGENTS.md` 仍作为 `<project_context>` 加载 |
| frontmatter           | 缺 name / 缺 desc / 非法 name / 名称不匹配                            | 缺 name 回退目录名；缺 desc 不加载；非法 name 仅 warning 仍加载；名称不匹配允许  |

**证据文件路径（本机，0.85.1）**：
`~/.volta/tools/image/packages/@earendil-works/pi-coding-agent/lib/node_modules/@earendil-works/pi-coding-agent/`

- `docs/skills.md`、`docs/usage.md`、`docs/settings.md`、`docs/prompt-templates.md`、`docs/packages.md`、`docs/environment-variables.md`、`docs/quickstart.md`、`docs/index.md`
- `dist/core/resource-loader.js`、`dist/core/skills.js`、`dist/core/package-manager.js`、`dist/core/trust-manager.js`、`dist/core/system-prompt.js`

**实机配置**：`~/.pi/agent/`（`settings.json`、`trust.json`、`skills/smart-search-cli -> ~/Library/Application Support/com.easytoagents.desktop/skills/smart-search-cli`）。

**官方 URL 清单（访问日期均为 2026-09-14，版本 0.85.1）**：

- https://pi.dev/docs/latest/index>
- https://pi.dev/docs/latest/quickstart>
- https://pi.dev/docs/latest/usage> （`#context-files`、`#system-prompt-files`、`#project-trust`）
- https://pi.dev/docs/latest/skills> （`#locations`、`#frontmatter`、`#validation`）
- https://pi.dev/docs/latest/prompt-templates>
- https://pi.dev/docs/latest/settings> （`#project-trust`、`#resources`）
- https://pi.dev/docs/latest/packages> （`#package-structure`、`#scope-and-deduplication`）
- https://pi.dev/docs/latest/environment-variables> （`PI_CODING_AGENT_DIR`）
- 关联标准：https://agentskills.io/specification> （pi 声明的 Agent Skills 标准）

---

## 6. 缺口与后续动作

1. **官方未文档化的项（一律按 Unknown，需 fixture 兜底）**：
   - 同目录候选文件完整表（`AGENTS.MD`/`CLAUDE.MD` 等大写变体）——仅源码可见。
   - skills precedence rank 数值——仅源码注释与实现。
   - symlink 跟随——官方文档未明说，靠源码 + 实机 smoke 证明。
2. **运行时可被 CLI 开关禁用**：`--no-skills/-ns`、`--no-context-files/-nc`、`--no-prompt-templates/-np`。静态无法探测，应标 `Unknown` 并在 UI 明示。
3. **升级回归 fixture**：把 §Q7 的 (2)(3) 脚本固化为 tests/fixture，pi 升级后重跑；一旦 symlink 或候选文件表变化，能力矩阵回到调研。
4. **待产品决策**：全局 Prompt 接管 `~/.pi/agent/AGENTS.md` 的「用户既有内容」采用与 Claude/Codex 相同的 `$document` 接管（含快照/恢复），无需新增模式；但 `AGENTS.override.md`/`CLAUDE.md` 遮蔽风险需产品确认告警级别（建议硬阻断 Apply）。
5. **Prompt Templates**：本次明确不纳入；若未来纳入，新建独立 artifact 并单独核验全局/项目作用域与文件级 ownership。
