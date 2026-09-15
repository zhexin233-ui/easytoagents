# 修复 Pi MCP 探测与容器别名

## 目标与用户价值

修复 Pi MCP 在“全局适配器已就绪但项目未受信任”时被错误关闭，以及合法
`mcp-servers` 容器在导入、观测和恢复链路中被漏读的问题。修复后，EasyToAgents
展示的能力状态和实际 `pi-mcp-adapter` 加载行为一致，并且不会因容器规范化遮蔽用户配置。

## 已确认事实

- Pi 官方文档说明：项目 trust 解析前已加载 user/global extensions；拒绝 trust 只跳过受保护的项目资源和项目 package。
- `pi-mcp-adapter` 上游读取合同为 `raw.mcpServers ?? raw["mcp-servers"]`，两者并存时 canonical `mcpServers` 优先。
- 当前 `probe_mcp_adapter()` 在任一 scope 为 `Filtered` 时整体返回 `NotLoaded`，会覆盖已经就绪的全局适配器：`src-tauri/src/adapters/pi/probe.rs:120-133`。
- 当前生产 Import/观测链使用固定 `mcpServers` selector；已有 `read_mcp_servers()` 只用于测试和遮蔽检测：`src-tauri/src/mcp/import.rs:393-415`、`src-tauri/src/adapters/pi/mod.rs:611-644`。
- 原任务 PRD 把 `settings.json`/`trust.json` 写成“绝不读写”，但官方合同和最终 spec 均要求受限只读探测 package/trust 状态。

## 范围与需求

### R1：适配器就绪状态

- 全局 `pi-mcp-adapter` 已声明、已安装且版本满足要求时，项目 scope 未受信任或被过滤不得把整体能力降为 `NotLoaded`。
- 全局不就绪时，仍按项目 package 的 trust、过滤、安装和版本事实 fail closed。
- 不降低现有 Missing、NotLoaded、VersionUnsupported、exclusive mode 的保守性。

### R2：MCP 容器别名

- Pi MCP 所有生产读取/观测入口接受 `mcpServers` 与 `mcp-servers`。
- 两个容器同时存在时只采用 canonical `mcpServers`，与上游 `??` 语义一致。
- Apply 后只保留 canonical `mcpServers`；删除旧 alias，但保留其它未知顶层字段和非受管 server。
- Import、Preview、Apply、漂移检测、项目原生资源状态/操作和 Restore 使用同一别名解释，不能各自实现不同规则。
- 识别 alias 时保留稳定诊断 `PI_MCP_CONTAINER_ALIAS_DETECTED`。

### R3：测试与兼容

- 补齐 `global ready + project untrusted/filtered` 的回归测试。
- 补齐 alias-only、canonical+alias 的 Import、观测、Apply、漂移与 Restore 测试。
- 原五工具和 Pi 的 canonical-only 配置行为保持不变。

### R4：文档口径

- 将 Pi 规范中的 `settings.json`/`trust.json` 边界统一为“不接管、不写入；仅允许为 Provider 选择、package 就绪和项目 trust 进行受限只读探测”。
- 明确静态文件探测不代表单次 Pi 会话的 `--approve`/`--no-approve` 或扩展临时 trust 决策。

## 验收标准

- AC1：全局适配器 Ready、项目 package 未受信任或被过滤时，Pi 全局/项目 MCP descriptor 仍可使用全局适配器，且路径不被清空。
- AC2：只有项目 package 可用时，未受信任仍返回 `PI_MCP_ADAPTER_NOT_LOADED`；受信任且版本满足时返回 Ready。
- AC3：alias-only Pi MCP 文件能被 Import 和状态/漂移链完整观测，不再被当作空配置。
- AC4：canonical 与 alias 并存时只读取 canonical；Apply/Restore 后只写 canonical，同时保留未知顶层字段和非受管条目。
- AC5：项目原生资源的 observe、disable/restore 与 snapshot 恢复对 alias-only 输入有效。
- AC6：`PI_MCP_CONTAINER_ALIAS_DETECTED` 在用户可见的预览或状态诊断中可观察。
- AC7：相关 Rust 单测、Pi E2E、`pnpm bindings:check`、`pnpm check` 与 `git diff --check` 全部通过。
- AC8：PRD、`.trellis/spec/backend/pi-adapter-guidelines.md` 和维护文档对 `settings.json`/`trust.json` 的只读边界一致。

## 不在范围内

- Provider、Prompt、Skills、Hooks、Agents 的功能扩展或重构。
- 修改 `pi-mcp-adapter` 上游行为或支持新的 MCP schema。
- 写入 `settings.json`、`trust.json`、共享 MCP 文件或凭据文件。
- 推断某次外部 Pi 进程的临时 CLI/扩展 trust 决策。

## 风险与约束

- 别名规范化必须发生在 Pi adapter 边界，避免把 Pi 特例扩散到其他工具。
- snapshot/restore 必须保存原文件字节并沿用现有原子写入，不能因规范化削弱回滚能力。
- 能力探针仍是静态近似；无法证明运行时扩展注册时必须继续 fail closed。
