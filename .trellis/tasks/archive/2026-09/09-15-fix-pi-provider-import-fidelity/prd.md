# 修复 Pi 渠道导入：多 provider 检测与 models 元数据保留

## Goal

Pi 的「检测已有配置」（渠道导入）必须忠实反映 `<pi_agent_dir>/models.json`：

1. **不再漏 provider**：`providers` 下的每个条目都是独立候选，检测要全部列出（当前只取
   `settings.json` 的 `defaultProvider`，其余条目静默忽略；无 `defaultProvider` 且多条目时
   整份文件直接放弃）。
2. **不再丢字段**：导入产生的中央渠道档案必须保留 provider 条目的 `api`（API 格式）、
   `name`、`models[]`（含逐模型 `contextWindow`/`reasoning`/`cost`/`thinkingLevelMap`/
   `input`/`maxTokens`/`name` 等）、`headers`、`modelOverrides` 及其他未知字段。当前
   `models` 被排除在档案之外，Apply 时 `providers/<id>/models` 整个数组被
   `[{id: 默认模型}]` 覆盖，逐模型元数据全部丢失。
3. **可见**：渠道档案界面只读展示 Pi 的 API 格式与模型摘要，避免「看不见就以为丢了」。

## Requirements

### R1 多 provider 候选检测

- Pi 的 `ProviderCodec::discover` 必须返回 `providers` 下**全部**条目的候选，顺序与文件一致。
- 没有任何 `defaultProvider`、或 `defaultProvider` 指向不存在的条目时，仍然返回全部候选；
  只用于给候选标注「默认渠道」，不得再作为放弃检测的理由。
- 无法解析的条目（非对象、provider id 非法）不阻断其他候选：标记为不可导入并给出稳定原因码。
- 单条目 provider 的既有行为（名称回退、`$ENV` 原样保留、明文 `apiKey` 诊断）不变。

### R2 批量多选导入

- 「检测已有配置」→ 候选列表（可导入 / 已纳入管理 / 配置无效），用户勾选并确认，一次性
  原子地创建多个中央渠道档案。
- 每个候选可编辑导入名称，默认 `entry.name` → provider id → `已导入渠道`。
- 已存在同 `provider_id` 中央档案的候选标记为 `already_managed` 且不可勾选；确认接口对
  该情况必须拒绝（不能重复建档）。
- 光标 provider 之外的 codec（Claude/Codex/ZCode/OpenCode）行为不变：仍最多一个候选，
  仍只在「该工具尚无中央渠道档案」时可确认。
- 首次接管引导（onboarding wizard）在该工具的渠道开关打开时导入全部可导入候选。

### R3 models / 未知字段保真

- 导入档案例携带原生 provider 条目的完整内容（`api`、`models`、`headers`、
  `modelOverrides`、`name` 及未知键），并作为目标基线的一部分落库。
- Apply 渲染时 `models` **按模型 id 合并**：原数组中已存在的模型条目逐字节保留；仅当默认
  模型 id 不在数组中时才追加 `{ "id": <默认模型> }`。绝不把整个数组替换成单个裸 `{id}`，
  也绝不写 `models: []`。
- 档案的编辑路径（改名称/API 地址/API Key/默认模型）必须保留原 `api` 与 `models` 内容。
- 同一 `models.json` 的受管基线必须只包含「当前有中央渠道档案的 provider」，即分次导入
  第二个 provider 时基线取并集，不能覆盖第一个，也不能把未导入的 provider 纳入受管。

### R4 只读展示

- `ProviderProfileDto` 增加只读的 Pi 摘要：API 格式与模型摘要（`id` + 可选 `name`）。
- 展示数据不得包含 `apiKey`、`headers` 等凭据内容。

### R5 脱敏与安全边界

- 新增的候选 DTO、预览 JSON、错误信息、日志不得出现 `apiKey`/`headers` 明文；候选身份
  （provider id）来自服务端持久化证据，前端只提交不透明候选 id。
- 只读探测边界不变：仍不写 `settings.json`/`trust.json`，仍不执行 `pi`，仍不展开
  `$ENV`/`!command`。

## Acceptance Criteria

- [ ] AC1 复现用例：`models.json` 含 `cc` + `gemini` 两条，`settings.json.defaultProvider = cc`
      → 检测返回 2 个可导入候选（旧行为 1 个）。
- [ ] AC2 无 `defaultProvider` + 2 条 provider → 仍返回 2 个候选（旧行为返回空）。
- [ ] AC3 全选导入 → 生成 2 个 Pi 渠道档案；Preview 后 Apply →
      `cc.models` 的 `contextWindow`/`reasoning`/`cost`/`thinkingLevelMap` 逐字段保留，
      `api` 保留，`gemini` 条目逐字节不变。
- [ ] AC4 只导入 `cc` → `gemini` 仍出现在下次检测中且可导入；导入 `gemini` 后受管基线
      为两者并集，Apply 不删除、不改写 `cc`。
- [ ] AC5 已导入的 provider 再次检测时为 `already_managed`；直接对已导入候选确认返回冲突，
      不产生第二份档案。
- [ ] AC6 provider 条目缺 `apiKey` 或 provider id 非法 → 该候选不可导入并给出原因码，
      同文件其他候选仍可导入。
- [ ] AC7 渠道档案界面显示 API 格式与模型摘要，且不出现 API Key / headers 内容。
- [ ] AC8 Claude/Codex/ZCode/OpenCode 的检测结果、确认守卫与既有测试全部不变。
- [ ] AC9 `pnpm check`（format/lint/typecheck/vitest/rust:check/bindings:check）全部通过。

## Out of Scope

- 完整的 API 格式下拉与模型列表编辑器（本期只做只读展示）。
- Claude/Codex/ZCode/OpenCode 的 provider 语义、导入守卫调整。
- `headers`/`modelOverrides` 的图形化编辑。
- Pi provider 的手工新增/编辑表单新增 `api` 字段（超出本期范围，另行评估）。

## Notes

- 复现证据（本任务诊断阶段实测，临时测试已删除）：档案
  `config_json = {"authKind":"api_key","providerId":"cc","extraProviderFields":{"api":"openai-completions"}}`，
  Apply 后 `cc.models` 从 `[{id, contextWindow, reasoning}]` 变为 `[{id}]`；`api` 与另一个
  provider 未被触碰。
- 相关规范：`.trellis/spec/backend/pi-adapter-guidelines.md`、
  `.trellis/spec/backend/mcp-import-guidelines.md`、
  `.trellis/spec/backend/skill-import-guidelines.md`、
  `.trellis/spec/backend/database-guidelines.md`。
