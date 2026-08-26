# MCP 全局导入格式核验

## 已证实

- `parse_native_item` 对 Codex 的带 `type` 条目一律不支持：只有 Claude 分支接受 `Some("stdio")`、`Some("http"|"streamable_http")`，而 Codex 仅接受 `native_type == None`（`src-tauri/src/mcp/import.rs:428-444`）。若 Codex 原生文件出现显式 `type: "stdio"` 或 `type: "http"`，会显示 `Unsupported`，不会误转协议；是否属于目标工具实际格式需由适配器格式契约确认。
- `parse_native_item` 先移除 `enabled`/`disabled`，停用项在 `src-tauri/src/mcp/import.rs:411-417` 直接返回 `Disabled`。启用项最终固定 `enabled: true`（`src-tauri/src/mcp/import.rs:456-475`）；`disabled:false` 不会被保留到 extra。当前 `native_mcp_item` 对 Codex 固定写 `enabled:true`，Claude 不写 enabled（`src-tauri/src/mcp/service.rs:634-690`）。这是控制字段归一化，未发现启用/删除停用项的路径。
- HTTP header 字段按工具映射：Claude 读取 `headers`，Codex 读取 `http_headers`（`src-tauri/src/mcp/import.rs:463-470`）；另一名称会留在 extra，但 `RESERVED_EXTRA_KEYS` 禁止它（`src-tauri/src/mcp/models.rs:15-29`），因此混用字段被拒绝而不是静默转换。`env_http_headers` 在解析早期以 `Unsupported` 拒绝（`src-tauri/src/mcp/import.rs:420-424`）。
- 普通字段泄漏防护覆盖名称、command、args/url：名称/command 在解析时检查可识别 secret（`src-tauri/src/mcp/import.rs:446-455`），discover 后再次用 redactor 检查 command/url/args（`src-tauri/src/mcp/import.rs:95-105`）；URL query 也由模型校验拒绝可识别 secret（`src-tauri/src/mcp/models.rs:451-475`）。拒绝分支保持 `redacted_projection = null`（`src-tauri/src/mcp/import.rs:70-107`），未见原始普通字段回 RPC。
- 未知 portable extra 的确会保留：解析后剩余 object 作为 `extra`（`src-tauri/src/mcp/import.rs:456-475`），native 输出从 extra 克隆后插入规范字段（`src-tauri/src/mcp/service.rs:627-633`）。但与保留原则相容的前提是该 key 不在 reserved 列表且通过 `validate_extra`。
- 中央库复用条件是大小写名称完全相同且 `configuration_from_record(record)? == configuration`；仅大小写不同或配置不同均 `NameConflict`（`src-tauri/src/mcp/import.rs:127-139`）。原生同名大小写冲突也会拒绝（`src-tauri/src/mcp/import.rs:114-124`）。数据库层 MCP name 本身为 `COLLATE NOCASE UNIQUE`（`src-tauri/src/db/migrations/0001_initial.sql:69-77`）。

## 风险/存疑

- `CandidateEvidence` 将原始名称写入私有 preview `context_json`（`src-tauri/src/mcp/import.rs:22-27,146-160`）。discover RPC DTO 中名称已 redacted，且 parse 会拒绝可识别 secret 名称；因此当前测试范围内没有外泄，但如果检测器漏识别名称秘密，私有 preview 会保存原文。需确认“私有中央库”是否允许 preview evidence 保存原始名称。
- Codex 显式 `type` 的兼容性没有仓库内事实证明；当前 Codex adapter 的 selector 只说明 `mcp_servers/*/env_http_headers`（`src-tauri/src/adapters/codex/mod.rs:89-124`），未找到其 schema 对 `type` 的明确声明。建议由主代理核对实际 Codex 格式后决定是否扩大匹配，不能仅凭此处推断为 bug。
- `native_mcp_item` 对 Claude HTTP 总是输出 `type:"http"`，即便输入为 `type:"streamable_http"`（`src-tauri/src/mcp/service.rs:650-655`）。解析将二者合并为同一 `StreamableHttp` DTO（`src-tauri/src/mcp/import.rs:435-437`），这是有意协议归一化；若上游把两种 type 语义区分，则存在转换语义风险，但仓库当前模型只有一个 HTTP transport，未能证实。

## 未发现

- 未发现一个非法条目阻断其它条目的逻辑：每个条目错误均 push candidate 后 continue（`src-tauri/src/mcp/import.rs:80-93`）。
- 未发现 discover/confirm 写入原生 MCP 文件；本范围代码只 `scan_target` 读取，并将变更交给后续 sync（`src-tauri/src/mcp/import.rs:326-397`）。
