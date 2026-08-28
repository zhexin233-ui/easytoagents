# Journal - yang (Part 1)

> AI development session journal
> Started: 2026-08-19

---



## Session 1: 完成 AI 配置桌面端 Phase 0-8

**Date**: 2026-08-25
**Task**: 完成 AI 配置桌面端 Phase 0-8
**Branch**: `main`

### Summary

完成 Claude/Codex 配置管理桌面端 Phase 0-8，实现隔离预览应用恢复、Provider/Prompt、MCP、Skills、项目总览、release 探针与端到端质量门；用户接受未执行的人工 smoke 和签名公证风险，Phase 8 已关闭并归档。

### Git Commits

| Hash | Message |
|------|---------|
| `28302af` | (see git log) |
| `769632e` | (see git log) |
| `7f1ccab` | (see git log) |
| `f6145a6` | (see git log) |
| `f236a8f` | (see git log) |
| `40bc3f2` | (see git log) |
| `4061ca6` | (see git log) |
| `6ccf879` | (see git log) |
| `a26e9b7` | (see git log) |
| `3a80174` | (see git log) |

### Status

[OK] **Completed**


## Session 2: 完成 Trellis 规范与配置接管改进

**Date**: 2026-08-25
**Task**: 完成 Trellis 规范与配置接管改进
**Branch**: `main`

### Summary

补全并归档 Trellis 开发规范任务；提交 Trellis 多平台代理配置，以及 macOS Volta 探针、Codex OAuth 导入、全局目标状态和 onboarding 交互改进。

### Git Commits

| Hash | Message |
|------|---------|
| `1e938ba` | (see git log) |
| `539b89f` | (see git log) |
| `46de613` | (see git log) |
| `d675bd9` | (see git log) |

### Status

[OK] **Completed**


## Session 3: 修复 MCP 与 Skills 初始化状态

**Date**: 2026-08-25
**Task**: 修复 MCP 与 Skills 初始化状态
**Branch**: `main`

### Summary

修复 Claude 企业策略源缺失被误判为 Unknown；统一 MCP 与 Skills 的待初始化、策略待确认和策略阻止展示，补齐前后端回归测试与规范。

### Git Commits

| Hash | Message |
|------|---------|
| `452dc08` | (see git log) |

### Status

[OK] **Completed**


## Session 4: 修复 MCP 全局预览并补齐原生导入

**Date**: 2026-08-26
**Task**: 修复 MCP 全局预览并补齐原生导入
**Branch**: `main`

### Summary

补齐 Claude/Codex 原生全局 MCP 的显式扫描、勾选导入与来源分配，保留独立 Preview/Apply；实现、验证、提交、推送和任务归档已完成。

### Main Changes

- 新增严格转换、私有配置等价比较、分批基线接管、过期与事务回滚保护以及逐项安全诊断。
- 新增导入对话框、生命周期回归和第 5 次前向迁移，记录跨层规范。

### Git Commits

| Hash | Message |
|------|---------|
| `27729fc` | (see git log) |

### Testing

- [OK] pnpm check 通过：50 个前端测试、163 个 Rust 单元测试、3 个集成测试及格式、lint、类型、clippy 检查。
- [OK] pnpm build 通过；所有原生写入测试仅使用隔离 fixture，未对用户真实配置执行导入确认或 Apply。

### Status

[OK] **Completed**


## Session 5: 新增与编辑表单弹窗化

**Date**: 2026-08-26
**Task**: 新增与编辑表单弹窗化
**Branch**: `main`

### Summary

MCP、渠道档案和全局提示词改为按钮触发弹窗，完成自动化与隔离浏览器验证。

### Main Changes

- 复用 FormDialog，移除平铺表单并调整 MCP 全宽列表。
- 保存失败保留输入，关闭清理草稿和错误，保存与刷新期间防重复操作。
- 修复提交按钮禁用后的焦点逃逸，保留 Tab/Shift+Tab/Escape 与焦点恢复。

### Git Commits

| Hash | Message |
|------|---------|
| `cb8e022` | (see git log) |

### Testing

- [OK] pnpm test --run：8 个文件、62 项测试通过。
- [OK] pnpm lint、pnpm typecheck、pnpm format:check、pnpm build 与 git diff --check 通过。
- [OK] 隔离浏览器验证三类弹窗、390×700 窄视口、滚动和 pending 键盘行为，控制台无警告或错误。

### Status

[OK] **Completed**

### Next Steps

- 按用户授权推送 main 到 origin，完成后核对远程与本地一致。


## Session 6: 修复 Codex MCP 导入兼容性与凭据误判

**Date**: 2026-08-26
**Task**: 修复 Codex MCP 导入兼容性与凭据误判
**Branch**: `main`

### Summary

支持等价显式 MCP 类型，区分展示隐藏与凭据判定，统一环境变量登记并细化安全诊断；完整质量门通过，原生配置未写入。

### Main Changes

- 保留 Codex stdio/http/streamable_http 显式类型并支持规范化复用。
- 普通运行路径、开关与超时不再因文本重叠误判；真实凭据与未知环境值继续保守保护。

### Git Commits

| Hash | Message |
|------|---------|
| `bf94c45` | (see git log) |

### Testing

- [OK] pnpm check：62 前端测试、168 Rust 单元测试及 3 项绑定/IPC/集成测试通过。
- [OK] 先复现三项旧逻辑失败，再验证修复；两轮独立只读核验无可行动发现。

### Status

[OK] **Completed**

### Next Steps

- 已获用户推送授权；真实桌面应用尚未重新打包，未对真实原生配置执行确认导入或 Apply。


## Session 7: 全局 Skills 检测导入修复

**Date**: 2026-08-26
**Task**: 全局 Skills 检测导入修复
**Branch**: `main`

### Summary

完成 skills-global-import：新增 Claude/Codex 全局用户技能检测与勾选复制导入，排除内置及跨工具别名，修正首次状态提示；保持原安装和同步管理关系不变。已完成独立复核、全量检查、隔离浏览器验收和任务归档；用户已授权提交并推送。

### Main Changes

- 新增来源证据、批量确认事务、v6 迁移和生成 RPC 绑定；确认中复核内置排除与来源身份，保守处理失败清理和不确定提交。
- 新增 Skills 导入弹窗、查询隔离和错误重扫；首次已有目录显示未纳入同步管理；补充跨层规范与验收截图。

### Git Commits

| Hash | Message |
|------|---------|
| `0f28c561d6e59b55f17b9a673e846e0c06e3f7c5` | (see git log) |

### Testing

- [OK] pnpm check 通过：前端 96 项，Rust 186 项单元测试及 3 项集成测试；pnpm build 通过。
- [OK] 隔离浏览器验证选择、提交锁、刷新、来源失效重扫和局部不可用；未对真实用户目录执行导入或 Apply。
- [OK] 归档任务上下文各 14 项校验通过；git diff --check 通过。

### Status

[OK] **Completed**


## Session 8: 修复全局 Skills 首次分配与同步状态

**Date**: 2026-08-27
**Task**: 修复全局 Skills 首次分配与同步状态
**Branch**: `main`

### Summary

查明导入与分配未执行原生同步的根因；新增已分配待同步诊断和操作提示，补齐冲突与漂移回归；全量检查通过，并通过真实 Debug 应用为 Claude/Codex 同步 smart-search-cli，核验链接、baseline 与 managed items 均已同步。

### Git Commits

| Hash | Message |
|------|---------|
| `4f4f67c` | (see git log) |

### Status

[OK] **Completed**


## Session 9: 中央列表三列布局与官方平台图标

**Date**: 2026-08-27
**Task**: 中央列表三列布局与官方平台图标
**Branch**: `main`

### Summary

为 MCP 与 Skills 中央列表增加单列/三列切换和紧凑卡片底栏，使用可追溯的 Claude/Codex 官方资源展示全局分配状态，并完成完整检查与桌面应用视觉验收。

### Git Commits

| Hash | Message |
|------|---------|
| `bf7bac9` | (see git log) |

### Status

[OK] **Completed**


## Session 10: 持久化中央列表布局选择

**Date**: 2026-08-28
**Task**: 持久化中央列表布局选择
**Branch**: `main`

### Summary

为 MCP 与 Skills 中央列表增加彼此独立的布局偏好持久化，补齐异常回退与重新挂载测试，并同步前端规范。

### Git Commits

| Hash | Message |
|------|---------|
| `bbdbdfb` | (see git log) |

### Status

[OK] **Completed**


## Session 11: 集中管理项目 MCP 与 Skill

**Date**: 2026-08-28
**Task**: 集中管理项目 MCP 与 Skill
**Branch**: `main`

### Summary

移除 MCP 与 Skills 中央页的项目追加入口，在项目详情中增加 MCP/Skill 切换并保留双工具分配、预览和 Apply 流程；补齐回归测试与前端规范。

### Git Commits

| Hash | Message |
|------|---------|
| `19bfe91` | (see git log) |

### Status

[OK] **Completed**


## Session 12: 升级 Trellis 至 0.6.15

**Date**: 2026-08-28
**Task**: 升级 Trellis 至 0.6.15
**Branch**: `main`

### Summary

升级 Trellis 模板与运行脚本，收紧任务路径和 Hook 上下文边界，补充 DeepSeek Harness/Kimi 支持，并强化实现与检查范围纪律。

### Git Commits

| Hash | Message |
|------|---------|
| `c1b6298` | (see git log) |

### Status

[OK] **Completed**


## Session 13: 项目详情平台图标切换

**Date**: 2026-08-28
**Task**: 项目详情平台图标切换
**Branch**: `main`

### Summary

为项目详情增加 Claude/Codex 品牌图标切换，与 MCP/Skill 构成双轴单选；仅挂载当前组合，并隔离视图切换后的迟到 mutation UI 回写；补齐回归测试与前端规范。

### Git Commits

| Hash | Message |
|------|---------|
| `2349a9d` | (see git log) |

### Status

[OK] **Completed**


## Session 14: 美化 GitHub 项目展示

**Date**: 2026-08-28
**Task**: 美化 GitHub 项目展示
**Branch**: `main`

### Summary

新增中文 README 与 1280×640 项目横幅，完善并回读确认 GitHub 英文简介和 Topics；完整质量门与链接检查通过。

### Git Commits

| Hash | Message |
|------|---------|
| `83d82c0` | (see git log) |

### Status

[OK] **Completed**
