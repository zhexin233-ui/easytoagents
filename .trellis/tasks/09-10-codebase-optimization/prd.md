# 全项目审阅优化：缺陷、主线程阻塞、重复与工程债务

## 目标

按 2026-09-10 的全项目审阅结论（`research/review-findings.md`），系统性修复已核实的缺陷、消除主线程阻塞、去除前后端的复制粘贴结构，并补齐工程配置。本任务是父任务：拥有需求来源、子任务映射、跨子任务验收与最终集成审查，本身不承担实现工作。

## 需求来源

`research/review-findings.md` 中编号 A1-A12、B1-B3、C1-C7、D1-D7、E1-E15、F1-F4 的全部条目。

## 子任务映射与推荐顺序

| 顺序 | 子任务 | 覆盖条目 | 类型 |
|---|---|---|---|
| 1 | `09-10-fix-frontend-review-defects` | A1 A2 A3 A7 A8 | 轻量 |
| 2 | `09-10-fix-backend-review-defects` | A4 A5 A6 A9 A10 A11 A12 | 复杂 |
| 3 | `09-10-engineering-hygiene` | F1 F2 F3 F4 E15 | 轻量 |
| 4 | `09-10-async-commands-and-probe` | B1 B2 B3 | 复杂 |
| 5 | `09-10-backend-perf` | C1 C2 C3 C4 C5 C6 | 复杂 |
| 6 | `09-10-backend-robustness` | D1 D2 D3 D4 D5 D6（仅规范约束） | 复杂 |
| 7 | `09-10-backend-dedup-and-split` | E1 E2 E3 E4 E5 E6 E7 | 复杂 |
| 8 | `09-10-frontend-test-infra` | E14 | 复杂 |
| 9 | `09-10-frontend-dedup-and-split` | C7 D7 E8 E9 E10 E11 E12 E13 | 复杂 |

顺序依据：先修缺陷与 CI 让后续每步都有门禁；命令异步化改动面最小且收益最大；性能与健壮性改动集中在 `sync/apply.rs` 与 `db/`，放在拆分之前完成以免冲突；前端测试基建先于前端重构以降低回归风险。

## 跨子任务约束

- 每个子任务完成时 `pnpm check` 与 `pnpm bindings:check` 必须全绿；不允许跳过测试。
- 不重写已发布的数据库迁移（0001-0020）；D6 只以规范形式约束后续迁移。
- 不改变任何原生目标的写入合同：预览、确认、快照、恢复的语义与用户可见行为保持不变，除非条目本身就是修复用户可见缺陷。
- 前端不新增类型断言、`any` 或 `eslint-disable`；后端不新增生产路径 `unwrap/expect/panic`。
- 每个子任务在提交前更新对应 `.trellis/spec/` 文档。

## 跨子任务验收

- [x] A1-A12 每条都有对应的回归测试或明确的"不可测试"说明（A11、A12 不可测试，见子任务记录；A8 已补专门用例）。
- [x] 主窗口在工具探测完成前可见；apply、restore、list_projects、list_skills 不再在主线程执行文件 IO（代码层面达成；启动体感待真机确认）。
- [x] `tool_adapter/allowed_root/descriptor_for/safe_row_version` 等重复函数各只剩一份实现。
- [x] 五个中央页面共用同一个预览应用流程 hook；项目详情页 `detail/page.tsx` 478 行（旧 `project-detail-page.tsx` shim 已删除）。
- [x] 存在对 push 与 PR 执行 `pnpm check` 的 GitHub 工作流。
- [ ] 最终集成审查：全部子任务归档后，在真实 Tauri 应用中走通"导入 → 分配 → 预览 → 应用 → 恢复"一次。（静态集成审查见 `research/final-integration-review.md`；真机走查清单见其第 5 节，待人工执行。）

## 范围外

- 重写已发布迁移的 `writable_schema` 用法。
- 新增工具支持、新功能。
- 数据库表结构变更。
