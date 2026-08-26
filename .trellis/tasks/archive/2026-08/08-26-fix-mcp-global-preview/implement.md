# MCP 原生全局导入执行计划

## 当前门禁

- 用户已在最终方案摘要后明确批准实施；按已审阅范围执行。
- 获得最终方案批准后才执行 `python3 .trellis/scripts/task.py start .trellis/tasks/08-26-fix-mcp-global-preview`。
- 适用项目协作约定：主代理负责方案取舍、产品代码修改与最终验证；子代理只做独立探索/核验，使用 default、fork_turns=none，不复用。实现前主代理加载 `trellis-before-dev`，质量检查加载 `trellis-check`。

## 顺序与检查点

### 1. 输入模型与原生转换

- [x] 阅读本任务 PRD、设计、上下文清单及实际待改代码；确认新输入/DTO 和错误状态唯一归属。
- [x] 在 MCP 模块实现单工具原生条目转换、逐项诊断、严格格式边界、同名精确等价判断与候选生成；复用现有校验、scan、hash 和 redactor。
- [x] 将停用、SSE、不可表达引用或非法项保持不可选，不为它们建立管理关系。
- [x] 扩展现有 Rust fixture 测试：Claude stdio/HTTP、Codex stdio/HTTP、缺省字段、unknown extra 保留、错误条目不影响合法项、敏感参数拒绝。禁止读取真实 HOME。

### 2. 持久化、确认与基线

- [x] 新增第 5 个前向迁移及注册，保存脱敏导入预览、候选身份和所需版本证据；测试升级和重复打开。
- [x] 实现 discover，只保存导入证据，不创建中央/assignment/managed rows。
- [x] 实现 confirm：重新扫描并验证源身份/hash/选择，Immediate 事务内检查活动写入与行版本，原子创建/复用、分配、接管所选 items 和消费令牌。
- [x] 实现首次和增量接管；新增项目不覆盖旧 baseline 漂移，不接管未选择项；同名中央冲突/项目互斥/任一步失败回滚整个批次。
- [x] 在现有 MCP service/DB 测试中覆盖：分批导入、同名跨工具复用、不同/大小写冲突、重复确认、空/重复/伪造选择、stale file/path/中央与管理版本、旧受管项缺失/修改、活动写入互斥、事务失败无部分状态。
- [x] 用隔离源文件字节比较证明 discover/confirm/preview 不改原生内容；用后续真实 preview/apply 证明已选项不误报冲突且未选中项/无关字段保留。

### 3. RPC 和生成绑定

- [x] commands/mcp 加入 discover/confirm 并注入与现有预览一致的环境和策略探针；lib.rs 完整注册 command 和 DTO。
- [x] 运行 `pnpm bindings:generate`，由生成器更新 commands.ts；运行 `pnpm bindings:check`。不要手工填补生成绑定。
- [x] 审计新增 DTO/持久化 preview JSON/错误载体脱敏；敏感值仅保存在允许的私有中央记录和 baseline。

### 4. MCP 页面导入流程

- [x] 每工具目标卡加入检测导入入口；空中央库、空全局预览和成功提示指向正确下一步。
- [x] 新增独立导入对话框：默认无选择、候选状态/复用说明、逐项原因、确认/取消、加载/空/失败；复用焦点 helper 和按钮。
- [x] 确认仅传 previewId/candidateIds，不重建配置；关闭/重开/换工具丢弃旧令牌与选择，防止延迟响应污染新状态。
- [x] 确认成功刷新 MCP query family，不隐式 Apply；保留既有同步对话框和空目标行为。
- [x] 扩展现有 mcp-page.test.tsx 的 commands mock 与用例：两工具参数、选择 payload、状态反馈、秘密不可见、不可选项、复用说明、刷新、关闭与焦点、过期/错误不丢已有列表、绝不调用 createMcpServer/applyMcpPreview。

### 5. 整体验证与收尾

- [x] 独立核验本任务全量 diff、数据流和风险边界；由主代理复核证据并修复发现，不只看最后一轮变化。
- [x] 加入真实载体秘密审计，先证明载体非空再断言无 fixture secret；覆盖导入预览表、RPC、错误、同步预览、sync_items/journal。
- [x] 执行定向检查及完整质量门，补齐规范中的可执行导入/增量接管合同；不运行会对真实源配置 Apply 的桌面流程。
- [x] 如进行浏览器/桌面交互验证，仅连接明确隔离的 fixture 环境；否则报告已完成的自动化验证范围，不将 jsdom 测试声称为真实桌面验证。
- [x] 使用 `trellis-update-spec` 评估并记录新合同；更新任务验证结果。
- [x] 已获得用户对 verification.md 中单次工作提交的授权；不推送远程，归档与 journal 另行收尾。

## 验证命令

按依赖顺序执行；定向失败先修复，再跑全量。

- `cargo test --manifest-path src-tauri/Cargo.toml mcp`
- `cargo test --manifest-path src-tauri/Cargo.toml db::tests`
- `pnpm bindings:generate`
- `pnpm bindings:check`
- `pnpm test --run src/features/mcp/mcp-page.test.tsx`
- `pnpm format:check`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm check`（包含完整前端测试、cargo fmt/clippy/test）

测试筛选名以现有/新增符号为准，不以命令成功但匹配 0 项作为通过证据。格式修复只针对本次修改文件。

## 风险与回滚检查

- 高风险点：增量 baseline union、same-name 私有值比较、旧 managed_items 版本校验、导入确认与 Apply/Restore 并发、原生停用项误删、脱敏前 DTO 构造。
- 先通过后端隔离集成测试再接入 UI；后端门禁不过不启用确认入口。
- 不改变既有中央 enabled=false 的同步删除语义；原生停用项只读展示，不用 renderer 小修掩盖差异。
- 新迁移必须作为有序前缀追加；回退实现不得删除生产迁移或静默降级用户数据库。
- 发现需要扩展停用/SSE/引用字段模型、自动重命名或跨工具不同配置共享时，返回规划更新范围并重新审阅，不能悄悄扩大本任务。

## 实施状态

代码与质量门已完成，实际检查结果、验收证据、验证边界和待确认提交清单见 `verification.md`。工作提交 `27729fc` 已按用户要求推送 `origin/main`；任务已归档为 `completed`，归档和会话记录使用独立收尾提交。
