# MCP 原生导入格式核验

## 已确认事实

- 中央模型 `McpServerInput`/`ValidatedMcpConfiguration` 只有 `name、transport、command、args、url、headers、env、extra、enabled`；RPC DTO 只输出 `header_names/env_names/redacted_extra`，不会回传敏感值（`src-tauri/src/mcp/models.rs:35-45,88-99`；`src-tauri/src/mcp/service.rs:910-926`）。
- stdio 约束：必须 `command`，不能有 `url/headers`；HTTP 约束：必须无凭据绝对 HTTP(S) `url`，不能有 `command/args/env`（`models.rs:256-304`）。因此 Claude JSON 的 `type:"stdio"`→`Stdio`、`type:"http"`（以及现有 fixture 的 `type:"streamable_http"`）→`StreamableHttp` 时，字段映射分别为 `command/args/env` 与 `url/headers`（fixture：`src-tauri/tests/fixtures/phase2/claude-user-mcp.json:2-10`）。
- Codex TOML stdio 直接使用 `command/args/env`；HTTP 使用 `url/http_headers`。统一内部字段是 `headers`，渲染回 Codex 时才改名 `http_headers`（`src-tauri/src/mcp/service.rs:663-690`）。Codex 的 `enabled` 当前只在渲染时强制写 `true`，无论中央记录实际 enabled 值（同处 `:677,690`）。
- Claude 渲染写 `type="stdio"` 或 `type="http"`，不会保留原生 `type="streamable_http"`；Codex 不写 transport type（`service.rs:638-690`）。HTTP headers 仅在非空时写出；stdio args/env 同理。
- 原生额外字段进入 `extra` 后会合并回目标对象；`extra` 必须 JSON object，禁止与结构化键（含 `type/transport/command/args/url/headers/http_headers/env_http_headers/env/enabled/disabled`）大小写不敏感冲突，且 TOML 可表达、≤64KiB、嵌套≤8 层（`models.rs:26-31,426-470`）。因此 `startup_timeout`、`startup_timeout_sec`、`env_vars`、`bearer_token_env_var` 等可作为 extra 保留，前提是值满足 portable-extra；未知数组只能是标量数组。
- 敏感值在创建/更新时注册到 `SecretRedactor`，列表/DTO 对 extra 脱敏；headers/env 只返回名称（`service.rs:65-86,910-968`）。预览构建也先注册配置秘密（`service.rs:608-625` 附近的 `build_desired_projection`）。
- 同步只取已分配且 `record.enabled` 的条目；项目层另取全局已启用继承项（`service.rs:423-437`）。这意味着导入的 disabled 条目应保存在中央库但不进入 desired projection。
- Claude 目标敏感 selectors 是 `mcpServers/*/headers`、`.../env`；Codex 是 `mcp_servers/*/http_headers`、`.../env_http_headers`、`.../env`（`adapters/claude/mod.rs:106-114,137-145`; `adapters/codex/mod.rs:83-91,118-126`）。当前 Codex fixture 仅覆盖 `http_headers`，且含 bearer secret：`src-tauri/tests/fixtures/phase2/codex-config.toml:10-12`。

## 导入风险/语义变化（需产品决定）

- 读取原生条目若把 `disabled=true` 映射为 `enabled=false`，中央值可保留；但再次渲染 Codex 会写 `enabled=true`（明确丢失 disabled 语义）。Codex 原生 `enabled=false` 同样没有可写回路径，需修正 renderer 或接受导入后语义改变。
- Codex `startup_timeout_sec`、`startup_timeout`、`bearer_token_env_var`、`env_vars` 目前没有结构化字段；放进 `extra` 可跨工具保存，但 renderer 会原样写给 Claude，可能成为 Claude 不认识的扩展。若 `env_vars` 是对象/数组中嵌套对象，`validate_portable_extra` 可能拒绝；需逐字段诊断。
- Codex 的 `env_http_headers` 是环境变量引用语义，不等价于实际 `http_headers`；当前中央模型只有敏感 `headers`，直接折叠会丢失“引用而非值”。`env_http_headers` 已被列为敏感 selector，却没有 renderer 输出该键（`codex/mod.rs:88-90`; `service.rs:684-688`）。
- 当前无发现/解析原生条目的 MCP 导入函数；适配器只负责目标 discovery（`adapters/claude/mod.rs:20-121`; `adapters/codex/mod.rs:20-135`），通用 parse/render 是同步文档机制（`adapters/mod.rs:857-903,963-1018`）。新增导入需单独逐条转换和错误收集，不能假设已有 parser。

## 现有安全拒绝规则（应逐条诊断）

- 仅支持中央 `Stdio` 与 `StreamableHttp`；SSE/其它 native type 无对应 transport，应返回条目级 unsupported，而非整表失败（`models.rs:276-304`）。
- stdio args ≤256 项、单项≤16KiB；可识别 token/key 或疑似 `--token/--authorization/...` 参数直接拒绝，建议改 env/headers（`models.rs:315-347`）。
- headers 名仅 ASCII 字母数字 `-/_`，值无非法控制字符且≤64KiB；env key 必须合法环境变量名、值无 NUL 且≤64KiB（`models.rs:349-394`）。
- URL 拒绝非 HTTP(S)、无 host、用户名/密码、fragment；query 参数若可识别 secret 也拒绝（`models.rs:396-418`）。
- extra 非 object、null、控制字符键、保留键冲突、超 64KiB/8 层/不可表示数组或整数都拒绝（`models.rs:426-470`）。建议结果携带 `name/index/field/code`，继续处理其余条目。

## 名称比较与测试定位

- `ArtifactName::parse` 负责名称合法性；数据库/ownership 使用原始 `record.name` 做 map key/`BTreeSet`，未见大小写折叠或自动 rename（`models.rs:267`; `service.rs:696-710`）。因此跨 Claude/Codex 同名应视为可复用同一中央记录；同工具大小写差异目前是不同 key，冲突需条目级报告。是否允许 rename、大小写等价需决策。
- 最小测试可加在 `src-tauri/src/mcp/models.rs` 现有 `transport_fields_are_mutually_exclusive_and_required` 附近（`models.rs:532-565`）：导入字段映射/逐条拒绝、extra 保留与冲突、URL/args secret。渲染回写测试放 `src-tauri/src/mcp/service.rs` 的 `stdio_input/http_input` fixture helpers（`service.rs:1169-1200`），新增 Codex enabled=false、`env_http_headers`、未知扩展断言；原生样本复用 phase2 fixtures（上列路径），无需读取真实 HOME。

## 待决点

1. Codex `enabled` 是否扩展到中央/renderer 以保真，还是导入时明确警告并启用。
2. `env_http_headers`、`bearer_token_env_var` 是否设计成结构化引用字段；直接放 `extra` 会丢失跨工具语义。
3. `type:"streamable_http"`、SSE 是否兼容映射；当前 renderer Claude 只输出 `http`。
4. 同名跨工具是否共享中央记录，以及大小写冲突/rename 规则。
