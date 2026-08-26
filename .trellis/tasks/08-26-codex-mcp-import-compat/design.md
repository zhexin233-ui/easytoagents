# Codex MCP 导入兼容性与凭据判定设计

## 1. 边界

改动集中在原生 MCP 转换、MCP 敏感值登记及导入诊断。沿用数据库、所有权、令牌确认、Preview/Apply、生成 DTO 与现有 UI。无 schema 迁移，不新增导入模式或隐式动作。

## 2. 原生类型规范化

`parse_native_item` 对两个工具使用相同协议矩阵：

| type | command/url | 结果 |
| --- | --- | --- |
| 缺省或 stdio | 仅 command | stdio |
| 缺省或 http/streamable_http | 仅 url | streamable_http |
| 已知协议但字段不对应、混合或缺失 | 任意 | Invalid，说明字段冲突 |
| SSE 或未知协议 | 任意 | Unsupported，指出 type 边界但不回显未知值 |
| 非字符串 type | 任意 | Invalid，指出类型错误 |

显式 type 只映射中央 transport，不放入 extra，不放宽保留键校验。enabled/disabled 检查优先，`env_http_headers` 保持不支持，其它可表达 extra 原样保留。

确认复用同一 parser。基线仍取原始观察投影，导入不改字节；renderer 省略冗余 type、补充 enabled 等差异在后续同步预览中展示，不能在导入时直接改 TOML。

## 3. 展示隐藏与凭据证据分离

### 安全 API 的用途区分

在 `SecretRedactor` 内区分两种登记意图：

- **凭据**：参与所有输出脱敏，并作为普通字段拦截证据。现有 `register_secret` 的语义不变，包括短秘密。
- **仅供展示隐藏的私有运行值**：继续保守脱敏，但其子串重叠不构成凭据证据。新增明确方法登记，不能从凭据集合移除或降级同值。

新增纯内容判定方法，判断已登记凭据和可识别秘密；不以 `redact_text` 返回值变化作为判据。保留嵌套 JSON 敏感键/值的识别，但重新序列化、空白规范化不能被当作命中。

不改变 Provider 显式密钥登记，不减少结构化 env/header 脱敏，不引入长度或熵阈值。RPC 仍只返回 env/header 名称，导入投影继续走完整 redactor。

### MCP 环境变量的保守分类

MCP 层复用一个登记 helper，应用顺序固定：

1. 敏感键名、可识别令牌形态/内嵌凭据、已知凭据同值，按凭据处理。
2. 只有明确支持的运行变量名且值符合对应路径/布尔/超时结构，才登记为仅展示隐藏的运行值。至少覆盖本次 `NODE_REPL_NODE_PATH`；其它常用路径、开关、超时规则须有固定规则和反例测试，不依据模糊包含 PATH 或数值短就豁免。
3. 用途不明、类型不匹配或不能证明是普通运行配置的 env 值仍为凭据。header 和可识别 extra 继续保守处理。

helper 统一 native discover 与 `register_configuration_secrets`，后者被 create/update、import confirm、sync preview 复用，避免先导入或生成预览后重新污染凭据集合。

扫描从当前 native 输入和已读取的中央 MCP records 重新建立两类证据，加上进程 redactor 已有的显式凭据，避免依赖操作顺序。旧数据库行不迁移，仅修复登记分类；其它资源的凭据 hydration 不在本任务扩展。

### 普通字段与跨条目保护

name、command、各项 args、url 使用凭据判定。可识别秘密仍由中央验证器拒绝，不放松 `ValidatedMcpConfiguration`。跨条目真实凭据重用仍拒绝；同值同时为运行值和凭据时，凭据优先，顺序无关。

不通过忽略所有 env、按条目隔离掉真实跨条目秘密，或跳过已有凭据检查修复。用途不明的自定义环境值可能继续阻止导入，并显示受阻字段；这是本次保守边界。

## 4. 安全诊断

`CandidateError` 可持有安全字符串或小型内部错误类型，统一转为既有 reason。通过固定字段名/规则构造：类型错误、缺失字段、混合 transport、不支持字段、中央校验错误、凭据命中位置。

`AppError::invalid_input` 已持有固定 field/reason，可严格筛选 INVALID_INPUT 且字段属于已知集合后复用；其它情况回退固定文案。禁止拼接 serde/TOML 原始错误、用户值或未知键名。命中只显示 command、args[index] 等位置，不显示值。

UI 不判断资格、不解析配置，现有 reason 展示直接承载细化文案。扩展现有测试，不增加多余 RPC code 或迁移。

## 5. 风险与回滚

- 最高风险为真实凭据错归运行值：固定规则、值形状校验、敏感键优先、未知项保守和同值不可降级必须同时验证。
- 登记入口策略可能不一致：覆盖 create/update/confirm/preview 后重扫及空 redactor 重扫，不能只测试首次扫描。
- 细化错误可能泄漏原始内容：仅用固定规则和安全字段名，审计非空 RPC/持久化预览/错误/同步记录载体。
- 没有持久化结构变更；已导入项仍用既有所有权，正常格式差异在后续预览展示。回退仅涉及本任务代码，不回滚用户配置、删除中央记录或重置受管基线。
- 若需要扩展 SSE/引用字段、降低未知 env 保护或改变 DTO 产品行为，返回规划重新审阅。
