# Codex MCP 导入异常：证据与核验边界

## 本机只读结构核验

2026-08-26 用 Python TOML 解析器在内存中读取用户提供的 Codex 全局配置位置，仅输出字段名、类型、数量和安全的协议/开关值。未记录完整命令、路径值、参数值、环境变量值或密钥。

| 条目 | 已观察结构 | 对应代码结论 |
| --- | --- | --- |
| memory、fast-context、另一个 stdio 项 | 显式 `type=stdio`，有 command/args | Codex 的显式类型进入 Unsupported |
| 另一个 HTTP 项 | 显式 `type=http`，有 url | 同一类型分支拒绝 |
| node_repl | 无 type，有 command/args、启动超时和运行时 env | command 包含 `env.NODE_REPL_NODE_PATH` 的普通绝对路径，被登记值替换规则命中 |
| computer-use | `enabled=false` | Disabled 是现有契约要求，不是本次误判 |

仅计算值的包含关系，安全输出为：来源 `node_repl.env.NODE_REPL_NODE_PATH`、类别 `absolute_path`、目标 `node_repl.command`、关系 `substring`。未调用真实配置的导入确认或 Apply。

## 根因一：显式 type 的工具限定

- `src-tauri/src/mcp/import.rs:408-484`：`parse_native_item` 先取 enabled/disabled，再解析 type、command/url；显式 stdio/http 的成功分支限定 `Tool::Claude`。
- `src-tauri/src/mcp/import.rs:448-453`：未匹配的 `Some(type)` 返回笼统 Unsupported，包括本可无损转换的 Codex 显式类型。
- `src-tauri/src/mcp/service.rs:627-693`：renderer 写中央 transport 的规范形式，Codex 不写 type；导入映射到 transport，后续可通过独立同步预览规范化。
- `src-tauri/src/mcp/service.rs:2077-2084`：现有成功用例 helper 主动删除 type，遗漏真实输入形式。

## 根因二：展示脱敏被当作凭据判定

- `src-tauri/src/mcp/import.rs:61-105,500-510`：扫描先登记全部 native env/header 值，然后对 name/command/url/args 运行 `redact_text`，文本变化即 Invalid。
- `src-tauri/src/security/mod.rs:55-95`：登记值不限长度；替换为子串匹配，`redact_text` 还会解析并重新序列化 JSON。因此文本变化不等同于发现凭据。
- `src-tauri/src/security/mod.rs:106-202`：结构化 env/header 隐藏、敏感键名、令牌形态和内嵌凭据识别可复用，不能删除。
- `src-tauri/src/mcp/service.rs:961-992`：create/update 与预览也登记全部 env/header；只改 native 扫描会被其它入口重新污染。
- `src-tauri/src/mcp/import.rs:290-292`：主代理确认 confirm 完成后调用 `register_configuration_secrets`。子代理最初只检查到 280 行，把此处列为待核验，不能沿用“confirm 不登记”的推测。
- `src-tauri/src/mcp/service.rs:605-615`：构建同步 desired projection 同样登记中央配置。
- `src-tauri/src/app/mod.rs:127-163`：AppState 从空 redactor 启动，不从数据库恢复登记值；扫描须建立所需 native/中央 MCP 凭据证据，不能依赖操作顺序。
- `src-tauri/src/mcp/service.rs:908-927`：中央 DTO 直接返回 name/command/args/url，仅返回 env/header 名称和脱敏 extra，不能简单取消普通字段凭据验证。

## 前端与错误路径

- `src/features/mcp/mcp-import-dialog.tsx:20-27,125-150`：标签来自 status，reason 已逐项展示；细化原因不需要新 DTO。
- `src-tauri/src/mcp/models.rs:192-230`、`src/bindings/commands.ts:522-525`：reason 已是可空字符串。
- `src-tauri/src/error.rs:166-173`：`invalid_input` 的 field/reason 来自固定文本，可安全映射；不应拼接任意原生值或底层错误字符串。

## 测试扩展位置

- `src-tauri/src/mcp/service.rs:2111` 起：隔离来源、选择、confirm、preview 和真实 fixture Apply。
- `src-tauri/src/mcp/service.rs:2378` 起：逐项不支持、非法与敏感内容；缺少 Codex 显式类型和正常 env 重叠。
- `src-tauri/src/security/mod.rs:548-562`：短值、JSON 形态与 Unicode 秘密保护，不能用长度阈值绕过。
- `src/features/mcp/mcp-page.test.tsx:698-926`：两工具选择、错误、过期、重扫、关闭、在途请求及无隐式 Apply。

## 基础文档与基线验证

主代理已完整阅读前一导入任务 `.trellis/tasks/archive/2026-08/08-26-fix-mcp-global-preview/` 下的 `prd.md`、`design.md`、`implement.md`，沿用不改原文件、显式选择、不可丢字段与凭据保护边界。

- `cargo test --manifest-path src-tauri/Cargo.toml mcp_import`：12 项通过，0 失败。
- `pnpm test --run src/features/mcp/mcp-page.test.tsx`：1 文件、16 项通过。

以上为规划阶段、未修改产品代码时的隔离测试基线，不单独证明新问题已修复。根因来自真实安全结构与代码分支核对；实施后的新增回归及完整质量门结果见 `../verification.md`，未进行真实桌面验证。
