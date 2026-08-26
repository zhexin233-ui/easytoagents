# Codex MCP 导入修复验证

## 结果与边界

本次代码修复及完整质量门已完成，工作提交为 `bf94c45`，任务已归档。用户已明确授权提交并推送。原生 Codex/Claude 配置未被写入；未对用户真实配置执行导入确认或 Apply。

UI 由 jsdom/mock commands 验证，原生文件写入链路由隔离 Rust fixture 的真实 Preview/Apply 验证；未声称完成真实桌面 UI 验证或已更新用户安装的应用二进制。

## 先失败后通过

在修改产品逻辑前，先扩展既有显式类型/跨工具复用测试并添加运行路径全流程测试。`cargo test ... mcp_import` 得到 10 通过、3 失败：

- 显式 Codex type 导致可导入数量 0，而期望 2。
- 带显式 type 的同名规范配置无法获得 Reuse。
- 运行路径、开关、超时与参数重叠导致可导入数量 0，而期望 3。

修复后上述测试全部通过，保留了失败场景，没有通过删除 type 或移除环境值规避问题。

## 最终检查

| 检查 | 结果 |
| --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml mcp` | 27 通过 |
| `cargo test --manifest-path src-tauri/Cargo.toml security::tests` | 8 通过 |
| `pnpm test --run src/features/mcp/mcp-page.test.tsx` | 16 通过 |
| `pnpm check` | 通过 |
| 完整前端测试 | 8 文件、62 测试通过 |
| 完整 Rust 测试 | 168 单元测试及 3 个集成/绑定/IPC 测试通过 |
| Prettier、ESLint、TypeScript、cargo fmt、Clippy `-D warnings` | 全部通过 |
| 生成绑定 | `generated_bindings_are_current` 通过，无 DTO 变更 |
| `git diff --check` | 通过 |

## 验收映射

- **AC1**：修改既有 `mcp_import_selects_extends_and_syncs_without_touching_unselected_entries` 与跨工具复用用例，保留显式 Codex stdio/http；新增运行值流程覆盖 streamable_http 别名。
- **AC2/AC4**：`mcp_import_runtime_values_survive_crud_confirmation_preview_and_rescan` 覆盖 create/update → discover → confirm → preview → 同进程/空 redactor 重扫 → 再次确认 → 独立 preview/apply；普通 JSON 参数保持私有原值。
- **AC3**：`mcp_environment_classification_keeps_unknown_and_credential_values_protected` 及新增 security 测试覆盖普通值、错误形状、未知变量、短秘密、同值登记顺序和嵌套 JSON 凭据。
- **AC3/AC5**：`mcp_import_preserves_credential_protection_and_reports_safe_field_reasons` 验证中央记录凭据恢复、拒绝项/跨条目凭据、header、短秘密、固定字段提示以及实际非空 RPC/预览证据不泄漏合成凭据。
- **AC5**：扩展两个工具的既有 UI 用例，展示 args/env_http_headers 具体原因，invalid/unsupported/disabled 仍不可选，无隐式创建或 Apply。
- **AC6**：隔离 Apply 后运行 env/args/extra、停用项和无关字段保留；显式协议仅在后续预览/Apply 规范化。既有真实载体秘密审计及未选项保护继续通过。
- **AC7**：全量门禁通过，DTO/schema、事务、所有权与原生写入流程未改。

## 独立核验

两个一次性 default 子代理分别审阅凭据检测/登记和导入协议/诊断/测试链路，均未发现可行动回归；主代理已复核关键代码和实际测试结果。保留的限制是用途不明的 env、SSE、env_http_headers 与原生停用项仍不可自动放行。

## Bug Analysis：导入输入与脱敏用途混淆

### 1. 根因类别

- **D · 测试覆盖缺口**：成功 fixture 主动移除 type，无法代表实际来源结构。
- **E · 隐含假设**：默认所有 env 值都是凭据，默认展示转换发生变化就说明存在秘密。
- **B · 跨层合同**：展示隐藏、持久化私有值与 DTO 普通字段验证共用了不同用途的判据。

### 2. 先前实现为何遗漏

前一任务解决了原生导入入口、选择、事务和所有权，但覆盖主要来自 renderer 生成并简化的配置，未保留冗余显式协议，也未构造 env 与 command/args 的值关系。本次红灯测试直接证明这些遗漏，不是 MCP 服务启动故障。

### 3. 预防机制

| 优先级 | 机制 | 状态 |
| --- | --- | --- |
| P0 | 凭据证据与展示隐藏用途分离，同值凭据不可降级 | 已实现并测试 |
| P0 | native 与中央 CRUD/confirm/preview 共用环境值登记，扫描恢复中央证据 | 已实现并测试 |
| P1 | 保留真实来源显式字段与跨字段关系，先证明旧代码失败 | 已完成 |
| P1 | 固定字段诊断、非空实际载体秘密审计、全流程隔离测试 | 已完成 |

### 4. 同类问题排查

已追踪所有 MCP 登记入口和 AppState 空 redactor 生命周期。Provider 的显式凭据登记语义未改变，完整 Provider/同步测试通过。不扩大到其它资源的凭据 hydration 或未知 env 自动分类。

### 5. 知识沉淀

- 已更新 backend 的 MCP 导入、错误处理和质量规范及索引。
- 仓库无 `src/templates/markdown/spec/` 模板目录，不创建无关模板镜像。
- 规范随本任务修复一并列入待确认工作提交，归档和 journal 后续独立处理。

## 已批准提交

用户已批准一个工作提交：`fix: 修复 Codex MCP 导入协议与凭据误判`，并要求推送。

文件清单：

- `src-tauri/src/mcp/import.rs`
- `src-tauri/src/mcp/service.rs`
- `src-tauri/src/security/mod.rs`
- `src/features/mcp/mcp-page.test.tsx`
- `.trellis/spec/backend/index.md`
- `.trellis/spec/backend/mcp-import-guidelines.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/backend/error-handling.md`
- 本任务目录：`task.json`、`prd.md`、`design.md`、`implement.md`、`implement.jsonl`、`check.jsonl`、`research/root-causes.md`、`verification.md`。

未识别脏文件：无。按用户授权提交并推送；归档及 journal 使用后续独立提交，不混入本次工作提交。
