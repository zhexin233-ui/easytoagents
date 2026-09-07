# OpenCode 接入设计

## 结构与交付边界

作为单一平台适配任务交付：四类受支持资源共用 Tool、环境解析、JSONC、数据库和同步恢复边界，拆成独立发布子任务会产生临时不一致能力。implement.md 按内部阶段推进并单独验证，最终一次整体验收。五类能力的产品范围以 prd.md 为准。

新增 `Tool::Opencode`（持久化字符串 `opencode`）及 OpenCodeAdapter。Provider/Prompt/MCP/Skills registry 加入 OpenCode；Hooks registry 不加入，domain/RPC/DB/UI 四层拒绝。Rust 生成 TypeScript binding；metadata 新增 OpenCode 图标、路由及能力。

## 目标解析与环境合同

新增显式 OpenCode 环境结构，由 AppState 注入 HOME、XDG 路径、已知 OPENCODE_CONFIG/CONFIG_DIR/CONFIG_CONTENT 与 disable 标记；服务禁止直接读取进程环境。

- 全局配置根：XDG_CONFIG_HOME/opencode，未设置时 HOME/.config/opencode。
- 同层有 opencode.jsonc 时选 JSONC；仅 json 时选 JSON；都不存在新建 json。两者同时存在时保留低优先级文件，只把实际选定文件作为受管目标。
- 项目 MCP 默认根目录 opencode.json(c)；同时核对 .opencode/opencode.json(c) 等实际覆盖来源。已存在更高优先级项目配置时，按已验证优先级选定唯一项目写入目标，不在多个文件之间盲目复制。
- 全局 Prompt 为 config 根的 AGENTS.md；Skills 为 config 根/skills 与项目 .opencode/skills。
- 自定义单文件/目录、inline、托管来源不能被忽略。对不在正式受管目标中的已知覆盖：只读比较相关选择器，冲突则标明受阻/覆盖；不能证实优先级时 fail closed，不猜路径写入。组织远端/其它进程环境无法穷尽，UI 不声称已验证完整运行时。
- allowed_root 全局限定 config 根，项目限定 canonical 登记根；自定义路径必须有显式合法来源和窄目录边界才可用。不能退回整个 HOME。Restore 从持久化身份重新推导并核验同一规则。
- Preview 绑定选定路径、同层候选存在状态及相关覆盖证据指纹。Apply 前重新解析，不能把旧预览移到新路径。

CLI 探针采用显式候选和有界 `--version`，隔离测试覆盖 wrapper、缺失、错误/超时。已验证版本 1.18.29；不宣称更早最低版本。Desktop 如无可信 bundle 身份证据保持 Unknown 诊断，不成为写入授权。实现阶段可补充官方 bundle 证据，不猜名称即 Installed。

## 文档表示与共享文件写入

增加明确 JSONC 格式与无损编辑路径：解析用于投影/哈希，写入用保留注释、未知键及非受管文本的语法树编辑；不能简单去注释后 serde_json 重写整个文件。错误语法、重复歧义键和类型冲突拒绝。新文件生成 JSON。

Provider 和 MCP 共用配置：ownership 精确到 provider 子字段与 mcp 名称；复用现有共享文件同步机制，按同一观测版本协调应用。顺序更新、同一计划和恢复必须有测试，禁止一个 artifact 用旧全文件内容覆盖另一个。snapshot 保留原始字节与现有私有 payload/journal；恢复仅按现有权限和冲突规则执行。

## Provider

全局表单采用现有渠道结构，新增 OpenCode 专属 SDK 信息（独立于 zcode_kind），支持官方 npm SDK 字符串及已验证常用协议；不自动执行或安装 SDK。

- 持久化稳定 provider ID；新渠道使用应用自有 ID，导入沿用来源 ID，不能从显示名动态变化。
- 写入 provider[id].name/npm/options.baseURL/options.apiKey、当前所需 model 元数据，以及顶层 model=`providerId/modelId`。精确子选择器保留其它 models、limit、headers、options、small_model、enabled/disabled_providers 等未编辑内容。导入记录保留可表示的必要 SDK/模型信息，不能丢失后默默变成另一种协议。
- 原生导入由顶层 model 明确定位 provider/model；无明确选择或所需配置不能无损表示时给出不可导入原因。不挑第一个 provider 或模型。
- 官方允许 options.apiKey；沿用私有中央凭据与脱敏 DTO。环境/文件引用作为引用保留并按敏感内容处理，不展开任意文件。仅 auth.json 中有 OAuth/API 登录而配置不够完整时不猜渠道，提示由 OpenCode 管理登录或在渠道页显式填写。
- 不写 auth.json，不接管账户 OAuth。已有高级 provider/options 和凭据在扫描、展示与恢复中统一脱敏。
- known enabled/disabled_providers 或更高优先级 model 使目标失效时显示受阻，不擅自修改这些用户设置。

## Prompt

全局 AGENTS.md 沿用 Markdown 全文所有权与现有导入/激活流程。明确全局文件可能覆盖 Claude fallback 的提示；不把 fallback 作为本工具原生受管目标。项目 Prompt 不生成 descriptor、assignment 或历史记录。

## MCP

以 research/verified-contract.md 当前稳定 schema 为准：

- 中央 stdio command+args ↔ local command 字符串数组；env ↔ environment。
- 中央 streamable_http ↔ remote url；headers 原样受密钥规则保护。
- `enabled` 按原生识别；disabled 原生条目沿用项目不可导入合同，中央停用从 desired 集移除，不能误纳管导致删除。
- `cwd`、数字 timeout、oauth false/object 等用经过类型验证的 extra 表示；OAuth clientSecret 必须走私有敏感字段存储/脱敏，不能塞入普通扩展 DTO。若现有 central validator 不接受，则扩展安全结构而非绕过检测。
- 未知扩展保留在原生文件，不支持语义标为不可导入；不同工具导出的 extra 必须经过目标 schema 校验，避免把其它平台的 type/env/auth 混入 OpenCode。
- 原生禁用/恢复使用 enabled=false（适用于继承覆盖），不能误用 v2 disabled。
- 不调用 `mcp auth`、不读取/写入 mcp-auth.json、不建立 MCP 连接。

## Skills

沿用全局来源显式导入、中央完整树哈希、不可变副本、逐名称符号链接和首次接管流程。正式入口仅 OpenCode 两个 skills 路径；.agents/.claude 可作为显式兼容导入来源并去重，不作为 OpenCode 写入目标。内置排除延续现有规则。

增加目标级 frontmatter 验证：name 符合官方 slug、与目录一致、description 长度有效。其它工具已有 Skill 不因新平台验证全局失效，仅 OpenCode 分配/同步拒绝不兼容项。

本机隔离 debug skill 已证明两作用域链接可发现；端到端应用后重复 smoke，确保产品实际输出而非手工 fixture 被发现。

## 数据与界面

追加 v19 迁移，不改历史 SQL。只放宽支持 artifact 的 Tool CHECK；全局 Prompt active flag/映射按当前数据库结构扩展，保持公共 Prompt DTO 的 global_tools。保留 OpenCode Hooks 和项目 Prompt 的否定约束。逐条 database tool decoder 穷举更新。

检查 tool settings、导航、onboarding、overview、profiles、prompts、MCP、Skills、projects、sync history/restore。工具默认启用集合沿用当前产品策略，不自动改变用户设置。Hooks 不出现可分配入口，明确插件型回调尚未接入。

## 验证、发布及回滚

覆盖 PRD AC1–AC7，具体命令见 implement.md。CLI smoke 不代表真实服务连通或 Desktop UI 测试。回滚先关闭 OpenCode capability 与入口，再撤回服务；前向数据库迁移保留；用户文件只通过快照恢复，不通过卸载或清理脚本删除。
