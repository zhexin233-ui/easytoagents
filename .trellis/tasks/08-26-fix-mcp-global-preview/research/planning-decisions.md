# 主代理核验与设计取舍

## 已确认输入

用户已批准补齐“扫描已有原生全局 MCP → 选择确认导入 → 生成预览”。截至最终方案摘要前，仍处于 planning，无产品代码修改。

## 对研究建议的校正

1. Provider `adopt_baseline` 只支持目标空 baseline（`src-tauri/src/db/profiles.rs:755-818`），不能原样复用到可分批选择的 MCP：确认第一批后必须允许新扫描导入剩余项。设计要求验证旧受管投影/item hash 无漂移后原子扩展 ownership；绝不无条件重建旧 baseline。
2. 仅 `create_mcp_server` 加 assignment 不能完成明确接管；还需绑定真实原生 key/item hash 和 managed target。主代理已核验 `prepare_mcp_sync` 与 `verify_managed_item_baselines`（`src-tauri/src/mcp/service.rs:465-504,715-741`）。
3. full file hash 足以检测源文件内容变化；额外的候选 ID/item hash 用于防止客户端伪造或选择未展示项，不是允许 full hash 变化后继续导入的替代条件。
4. Codex renderer 写 enabled=true 本身符合现有“只有启用记录进入 desired”的合同（`src-tauri/src/mcp/service.rs:423-437,627-693`）。将原生 enabled=false 导入为已受管的中央 enabled=false，会在下次同步计划删除；仅修改 renderer 不足以保持停用项。因此本次展示并跳过原生停用项。
5. `env_http_headers` 被中央 extra 明确保留且无结构化表达（`src-tauri/src/mcp/models.rs:26-31,426-470`），不能降级成普通 header 值。本次明确不可选，避免扩大中央模型；其它可移植扩展沿用现有校验。
6. `profile_import_previews` migration 的 artifact CHECK 只有 provider/prompt，不直接复用该表或修改历史迁移；增加 MCP 专用表，不改造整个导入框架。
7. 后端初始研究中的 `PreviewPlan.items` 是笔误，实际同步 DTO 字段为 `targets`。

## 本机额外只读结构核验

- Claude：3 个 stdio，0 个显式停用；Codex：5 个 stdio、1 个 HTTP，其中 1 个显式停用；均未发现 env_http_headers 字段。
- 两工具有 3 个精确同名项。仅在本次统计脚本的基础字段规范化比较中，这 3 项内容一致；这不是对尚未实现的生产转换器的测试，不据此保证所有项都通过正式校验。
- 没有输出任何服务名、参数值、headers/env 或其他秘密内容，也没有写原生文件。

## 提交审阅的边界

支持已启用且可保真转换的 stdio/HTTP，精确同名同配置时明确复用，不同或大小写冲突不覆盖；停用和不兼容项只读展示。最终边界以用户对最终摘要的批准为准，不能把最初“可以增加导入”当作已批准这些实现细节。
