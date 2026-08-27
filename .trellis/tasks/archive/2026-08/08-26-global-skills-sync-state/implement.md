# 实施计划

## 进入实施前

- [x] 用户同意创建任务与规划。
- [x] 主代理阅读基础设计、相关旧设计和 Skills 导入规范。
- [x] 三路只读探索及本机关键证据独立复核完成，记录于 `research/root-cause.md`。
- [x] 用户在最终规划摘要之后明确批准实施；该批准包括执行本机两个目标的单 Skill 同步。
- [x] 已运行 `task.py start` 后再编辑产品代码。

上下文清单使用后端索引，避免大于 32 KiB 的 `quality-guidelines.md` 在注入时被截断；实施/检查须直接读取该规范的显式扫描、Skills 中央库、持久化 Apply 三节，不能把截断输出视为完整规范。

## 顺序

1. [x] 在 `src-tauri/src/skills/service.rs` 现有测试模块扩展首次同步矩阵，复现 Claude 仅 `.DS_Store`、Codex 缺失目标、两边已有分配且无 baseline 的情况。
2. [x] 添加 `SKILL_TARGET_INITIAL_SYNC_PENDING` 诊断的窄证据条件；保留无分配场景原合同，更新旧测试中“所有 desired 都不能产生初始诊断”的过宽断言。
3. [x] 在 `src/lib/global-target-status-ui.ts` 增加 status+code 精确映射；在 Skills 页加分配说明和成功反馈，不变更同步动作或其它资源行为。
4. [x] 在现有 `skills-page.test.tsx` 扩展待同步显示、分配后查询刷新、无隐式 preview/Apply、错误组合不覆盖和点击预览的流程测试。
5. [x] 扩展隔离后端端到端测试：两工具预览并 Apply 成功、目标链接正确、`.DS_Store` 和不同名外部条目保持、成功后 InSync；同名目录/未知或断裂链接、中央变更、半 baseline、现有 managed items、完整 baseline 后非受管漂移保持保护。
6. [x] 运行针对性检查，再全量检查；独立核验规范、跨层映射及真实冲突保护。
7. [x] 复查本机状态并使用当前 Debug 应用生产 UI 做单 Skill预览/Apply；Claude/Codex 两边预览均只包含 `smart-search-cli`，分别 Apply 成功。
8. [x] 更新 `.trellis/spec/backend/skill-import-guidelines.md` 中首次状态矩阵与回归要求并完成复盘；未获得提交确认前不提交。

## 验证命令

```bash
cargo test --manifest-path src-tauri/Cargo.toml skills::service::tests
pnpm test --run src/features/skills/skills-page.test.tsx src/features/mcp/mcp-page.test.tsx
pnpm bindings:check
pnpm check
pnpm build
```

优先使用既有测试和 fixture，不新增测试专用生产接口，不把真实用户 home 当测试根。若 `pnpm check` 因新任务文档格式失败，只格式化本任务文件，不格式化无关文件。

## 风险与停止点

- 后端 guard 不能吞掉半 baseline、同名碰撞、受管变更和策略错误；错误状态必须优先于首次待同步展示。
- 仅生成过 preview 的 target 行不是受管 baseline；测试覆盖有无 target 行两种情况。
- 不改变 preview 的原始 warning/conflict 或引擎的可合并条件；如出现必须改引擎的证据，返回规划。
- 本机预览超范围、stale 或冲突时停止；不绕过应用、不改库、不删除用户内容。
- 若真实桌面无法验证，明确报告缺口，不能用 mock 浏览器或静态链接检查替代完整桌面验收。
