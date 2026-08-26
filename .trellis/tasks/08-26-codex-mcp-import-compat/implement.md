# Codex MCP 导入修复执行计划

## 当前门禁

- 用户已在最终摘要后明确批准，现激活任务并按已审阅方案实施。
- 主代理负责方案、具体代码修改和最终验证；探索/独立核验按项目约定使用 default、fork_turns=none 的一次性子代理。
- 实施前加载 `trellis-before-dev`，完整阅读待改函数、规划三文档和上下文清单。不得对真实 HOME 确认导入或 Apply。

## 执行顺序

### 1. 固化失败案例

- [x] 在现有 MCP service fixture 添加 Codex 显式 stdio/http/streamable_http、无 type、字段冲突和错误类型用例，不经过会删除 type 的 helper。
- [x] 添加合成 node_repl 路径重叠及路径/开关/超时的同条目、跨条目重叠案例，验证旧逻辑在正确断言处失败。
- [x] 扩展现有 security 测试：运行值与凭据同值、短凭据、未知用途 env、JSON 文本规范化与真实嵌套秘密。

### 2. 协议转换与诊断

- [x] 修改 `src-tauri/src/mcp/import.rs` 判定矩阵，保留停用和不支持边界。
- [x] 区分类型错误、协议冲突、引用字段不支持与中央校验错误，reason 不含用户值。
- [x] 确认复用相同 parser，private equality 和 baseline 保持原始观察语义。

### 3. 分离敏感值用途并统一登记

- [x] 在 `src-tauri/src/security/mod.rs` 区分展示隐藏与凭据证据，保留显式秘密 API 和短值保护。
- [x] MCP 层集中定义运行变量保守分类；敏感名/形态优先、未知项仍为凭据、同值不可降级。
- [x] 同步 native discover 与中央配置登记入口，覆盖 create/update/confirm/preview。
- [x] 普通字段使用内容凭据判定，不因 JSON 规范化或普通运行值重叠而拒绝。
- [x] 扫描从 native 和中央 MCP records 重建证据，验证空 redactor 与已有 redactor 的一致性。

### 4. 全流程与界面回归

- [x] 在现有 MCP service 测试走 discover → confirm → rescan → preview → 隔离 Apply，验证选择、私有值/extra、停用与未选中项保护。
- [x] 显式和省略 type 的同名等价配置可复用，非法项不影响其它条目。
- [x] 审计非空 RPC、导入证据/预览、错误、同步记录/journal，无合成真实凭据泄漏。
- [x] 扩展 `src/features/mcp/mcp-page.test.tsx`：具体原因、正确选择、停用项不可选、无隐式 Apply；预计不改 DTO，仍检查生成绑定。

### 5. 整体质量门与规范

- [x] 子代理独立核验全量 diff、分类边界与测试缺口，主代理复核证据。
- [x] 主代理跑完整检查，记录命令、匹配数量和失败处理；不把 jsdom 当真实桌面验证。
- [x] 用 `trellis-break-loop` 回顾前次 fixture 删 type/遗漏 env 交叉关系的问题；通过 `trellis-update-spec` 更新导入与安全合同。
- [ ] 通过门禁后提出提交清单，获单次确认再提交，不擅自推送。

## 验证命令

- `cargo test --manifest-path src-tauri/Cargo.toml security::tests`
- `cargo test --manifest-path src-tauri/Cargo.toml mcp_import`
- `cargo test --manifest-path src-tauri/Cargo.toml mcp`
- `pnpm test --run src/features/mcp/mcp-page.test.tsx`
- `pnpm bindings:check`（仅 DTO 实际变化时先 `pnpm bindings:generate`）
- `pnpm check`

按依赖顺序执行，筛选命令必须命中目标测试。格式修复仅限本任务文件。真实配置只读安全结构，不用作测试输入或写入目标。

## 回滚与返回规划条件

普通运行值可导入与真实凭据不泄漏须同时通过。若不能在既定保守范围内满足，不扩大豁免或关闭验证，而是回到设计。发现需要迁移、新协议/引用字段、凭据可见或写真实配置时，重新提交规划摘要。
