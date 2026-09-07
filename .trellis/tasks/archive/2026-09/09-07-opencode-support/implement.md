# OpenCode 实施计划

状态：规划已获批准，进入实施。

## 顺序与检查点

1. [x] 读取 implement.jsonl 规范、PRD、设计及 verified-contract；quality-guidelines.md 超过原生注入上限（后端约 78KB、前端约 43KB），执行/检查子代理必须分段补读原文，不把注入截断视为规范完整；保存实施前 git 状态。确认 v18 仍为最新迁移，若已有并发迁移调整编号。
2. [x] 基础：Tool/metadata 类型、显式环境、候选路径/覆盖解析、安装探针、OpenCodeAdapter 与 Unsupported Hooks。检查序列化、路径边界、JSON/JSONC 多候选、环境/策略诊断。
3. [x] JSONC：引入风险可控的结构保留解析编辑支持（严格 JSONC 子集、重复键拒绝）；先验证注释/尾逗号/非受管内容保留，再接入通用 scan/render/restore，不退化成丢注释写入。
4. [x] 数据：v19 前向迁移、provider/prompt/MCP/skills 解码、提示词全局标记；v18→新版本、重开、索引/外键、unsupported canary 测试通过。
5. [x] Provider/Prompt：DTO 的 OpenCode SDK 字段、输入校验、秘密存储、原生导入、精确投影/ownership、模型选择和同步恢复。配置引用不展开，账户凭据文件不接管。
6. [x] MCP：稳定版 local/remote 编解码、environment、extra/OAuth 敏感映射、导入和项目原生禁用/恢复；所有不兼容 extra fail closed。添加 round-trip、未选字段保留与脱敏测试。
7. [x] Skills：来源目录、分配、目标 frontmatter 校验、首次接管与恢复；正式与兼容来源区分；用产品生成链接完成隔离 CLI smoke。
8. [x] 集成 UI：工具设置/导航/onboarding、四类资源、projects、overview、history/restore；Hooks 不可调用。品牌来源说明与 README/adapter 文档更新。
9. [x] 跨层完整验收：共享 Provider/MCP JSONC 文件组合与顺序 Apply、失败回滚、漂移/冲突/stale/Restore；全局/项目 Skills；项目 Prompt 与 OpenCode Hooks 零写入。
10. [x] 全量质量门、审查与规范同步，记录 CLI/Desktop 验证边界；随后按 Trellis 提交和收尾流程。

## 实施记录

- `pnpm check`：Prettier、ESLint、TypeScript、Vitest（16 个文件、241 项）、Rust
  clippy 与 Rust/集成测试（273 项库测试 + 绑定、命令、Hooks、Phase8 集成）全部通过。
- `git diff --check` 通过；Specta 绑定由生成检查确认是最新版本。
- CLI smoke 依据隔离 HOME/XDG/项目 fixture 完成，证据保存在
  `research/smoke-paths.json`、`smoke-config.json`、`smoke-precedence-*.json`、
  `smoke-skill.json`；未读取真实凭据、未启动模型或 MCP 服务。
- 桌面真实 Tauri/浏览器手工验证未执行；UI 合同由 Vitest 覆盖，Hooks 通过
  `OPENCODE_HOOKS_UNSUPPORTED` 保持不可调用。

## 验证命令

开发中按模块跑 Rust/Vitest 精准测试。Rust 类型更改后 `pnpm bindings:generate`，禁止手改生成绑定。

最终运行 `pnpm check`（先确认 package.json 聚合内容）；未覆盖的检查补跑 `pnpm format:check`、`pnpm lint`、`pnpm typecheck`、`pnpm test --run`、`pnpm bindings:check`、`pnpm rust:check`。通过后运行 `git diff --check`，不无理由重复整套检查。

实机 smoke：用临时 HOME 与全部 XDG 路径、合成凭据、纯净项目运行当前 CLI 的 `--pure debug config` 与 `--pure debug skill`，断言产品输出解析及两作用域 Skill 可见。不得读取真实用户凭据，不启动模型或 MCP 服务。记录版本/命令/断言；安装探针单独覆盖无 CLI 情况。

UI 测试覆盖精确 RPC 参数、分配不隐式 Apply、首次接管始终确认、Hooks Unsupported、晚到响应和密钥不展示。桌面/浏览器的实际验证按可用环境执行并区分 fixture 与真实 Tauri。

## 高风险文件与回退点

- `src-tauri/src/sync/` JSONC 及共享目标写入：每次修改先保住原四工具回归，禁止新的旁路写文件。
- `src-tauri/src/db/` 迁移只前向；约束锚点未精确命中立刻回滚事务。
- `src-tauri/src/profiles/`、`mcp/`：敏感引用和 OAuth 扩展不进入普通 DTO。
- `src-tauri/src/projects/`：OpenCode MCP/Skills 注册不连带创建 Prompt/Hook 状态。
- 若官方合同或 smoke 否定方案，应回规划更新 PRD/设计并复审，不静默缩减已审核范围。

## 规划收敛检查

- [x] 用户已确认全部可验证现有资源能力范围。
- [x] 当前官方证据与 v2 草案已区分，配置/链接已隔离验证。
- [x] 目标、范围、排除项和可观察验收明确。
- [x] PRD 已按最终结构重写，无阻塞产品问题。
- [x] design.md / implement.md / 两个上下文清单已准备。
- [x] 用户在最新规划摘要之后明确批准实施。
