# Bug Analysis：已分配的首次 Skills 状态被误报为非受管变更

## 1. Root Cause Category

- **Category**：D（Test Coverage Gap）+ C（Change Propagation Failure）。
- **Specific Cause**：上一轮首次状态修复只把 `desired.is_empty()` 写成保护条件，测试也锁定了“有分配就不能出现首次诊断”，没有建立“未分配首次目录”和“已分配首次待同步”两条互斥状态合同。后端返回通用可合并 drift，前端又没有足够证据解释它，最终在分配按钮与目标卡片之间形成跨层语义缺口。

## 2. Why Fixes Failed

1. 旧修复解决了空目录/仅元文件在无分配时的展示，却把 desired 统一归入真实 drift，范围不完整。
2. 旧测试覆盖了 desired，但只断言“不以 `SKILL_TARGET_INITIAL_` 开头”，没有断言用户此时应看到的可操作下一步；测试强化了错误假设。
3. 同步引擎的通用 `ExternalNonOwnedChange` 是正确安全证据，单改徽标或通用枚举都会把展示问题扩散到 MCP 和已有 baseline 场景。

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
| --- | --- | --- | --- |
| P0 | Test Coverage | Claude `.DS_Store` + Codex missing + desired + 双工具 Apply 的隔离全链路测试 | DONE |
| P0 | Documentation | 在 Skills 导入规范写出无分配首次、已分配待同步、真实 drift 三段矩阵 | DONE |
| P0 | Architecture | 后端用窄 diagnostic 传递证据，前端只按 status+code 组合展示，不改通用 drift/Apply | DONE |
| P1 | Code Review | 首次状态变化必须遍历 desired × baseline pair × managed items × scan/status | DONE |
| P1 | UI Test | 分配/取消分配刷新列表和状态、显示下一步且绝不隐式 Preview/Apply | DONE |

## 4. Systematic Expansion

- **Similar Issues**：Provider/MCP/Prompt 的“中央意图已改但未 Apply”也依赖明确反馈；本次不改其业务，但共享页面质量规范已要求 CRUD/assignment 不暗示原生写入。
- **Design Improvement**：保持通用 status 表达目标事实，使用受证据约束的 diagnostic 表达产品阶段；不要为工作流阶段新增一个会污染所有资源的通用枚举。
- **Process Improvement**：状态测试不能只断言“旧文案不出现”，还必须断言用户下一步、动作可用性及不会发生的写入。

## 5. Knowledge Capture

- [x] 更新 `.trellis/spec/backend/skill-import-guidelines.md`。
- [x] 扩展 Rust service 与前端页面测试。
- [x] 记录本机只读证据、根因和真实桌面验证边界。
- [x] 仓库不存在 `src/templates/markdown/spec/` 或其它 spec 模板镜像，无可同步模板。
