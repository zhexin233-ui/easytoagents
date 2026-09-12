##### 开始使用

##### 核心功能

##### 深度集成

##### 帮助

# 子智能体

子智能体是主 Agent 可以在独立上下文中启动的专用 Agent，完成后会把结果汇总回主对话。ZCode 内置了 **general-purpose（通用型）** 和 **Explore** 子智能体，现在你还可以在设置中创建自己的 **用户级子智能体**。

当主 Agent 判断任务需要独立上下文或并行调研时，会通过 Agent 工具启动子智能体。子智能体在自己的上下文中工作，并把结果汇总回主对话，帮助主 Agent 继续推进任务。

![主 Agent 并行委派代码审查、探索搜索、文档调研等子智能体，各自独立上下文并行执行](/content/docs/v2/screenshots/subagent-parallel-20260701.webp)

![主 Agent 并行委派代码审查、探索搜索、文档调研等子智能体，各自独立上下文并行执行](/content/docs/v2/screenshots/subagent-parallel-20260701.webp)

## 内置：通用型子智能体

**general-purpose** 是默认的通用型子智能体，拥有完整工具权限，适合在独立上下文中处理需要读取、修改、运行命令或综合推进的任务。主 Agent 可以在需要把一段工作拆出去并行推进时调用它。

适合交给 general-purpose 的任务包括：

如果任务只需要只读调研、证据收集或调用链梳理，优先使用下面的 **Explore**。

## 内置：Explore

**Explore** 是一个只读的文件搜索和代码库调研专家，适合处理大范围检索、调用链梳理、代码结构理解和证据收集这类探索任务。

Explore 子智能体只用于只读探索，不会创建、修改、移动或删除文件。它主要使用读取和搜索相关工具，例如读取文件、按文件名匹配、按正则搜索内容，以及在需要时读取已知 URL 或搜索外部信息。

适合交给 Explore 的任务包括：

你可以直接在提示词里要求先做探索，例如：

`请先用 Explore 调研这个模块的调用链，再总结修改入口和风险点。`
`请用只读方式搜索这个功能的实现位置，列出相关文件和证据。`

## 自定义子智能体（Beta）

**Beta** — 用户级自定义子智能体正在灰度上线，能力范围后续可能调整。

现在你可以在 **设置 -> 子智能体** 中创建自己的子智能体。自定义子智能体可以把一个可复用的角色——例如代码审查员、测试编写者、文档调研员——连同它的模型、工具权限和指令一起打包，在不同任务中重复使用。

![子智能体设置页：内置的 general-purpose 与 Explore 角色](/content/docs/v2/screenshots/subagent-list-20260701.webp)

![子智能体设置页：内置的 general-purpose 与 Explore 角色](/content/docs/v2/screenshots/subagent-list-20260701.webp)

在子智能体面板里，你可以：

`general-purpose`
`Explore`

### 新建子智能体

点击右上角的 **新建**，在表单中配置这个可复用角色：

![新建子智能体表单：名称、颜色、模型、描述、可用工具与系统提示词](/content/docs/v2/screenshots/subagent-create-20260701.webp)

![新建子智能体表单：名称、颜色、模型、描述、可用工具与系统提示词](/content/docs/v2/screenshots/subagent-create-20260701.webp)

| 字段 | 说明 |
| --- | --- |
| **名称** | 子智能体的标识，例如 `code-reviewer`。 |
| **颜色标记** | 在列表和会话中区分该子智能体身份的颜色，仅作标识，不表达状态。 |
| **模型** | 可选「继承默认」（跟随主 Agent 当前模型），或指定一个具体模型。 |
| **思考强度** | 为该子智能体单独设置推理档位，可选档位由所选模型决定。**仅在指定了具体模型时生效**：选择「继承默认」时子智能体完全跟随主 Agent 的推理配置，此项不参与。 |
| **描述** | 展示给主 Agent 的简短说明。主 Agent 依据它判断何时调用该子智能体——描述越准确，越容易在合适的任务中被自动选用。 |
| **可用工具** | 控制该子智能体能调用的工具范围。「默认所有权限」继承全部工具；「自定义可用工具」则逐个勾选（例如只给 `Read` / `Grep` / `Glob` 做只读审查，`Bash` / `Edit` / `Write` 等可写工具会有标记提示）。 |
| **系统提示词** | 描述这个子智能体的角色、边界和规则。 |

`code-reviewer`
`Read`
`Grep`
`Glob`
`Bash`
`Edit`
`Write`

保存后，ZCode 会把子智能体写入 `~/.zcode/agents/<name>.md` 这个 Markdown 文件，ZCode Agent 运行时会在下次运行时加载它。启用后，你可以让 Agent 自动选用该子智能体，也可以在聊天输入框中用 `@` 引用它。

`~/.zcode/agents/<name>.md`
`@`

### 定义文件字段参考

子智能体定义文件是带 frontmatter 的 Markdown，正文即系统提示词。除了表单里能配置的字段，手工编辑时还支持以下字段（camelCase，大小写敏感）：

| 字段 | 说明 |
| --- | --- |
| `name` / `description` | 必填，缺失时该定义文件会被忽略并产生诊断。 |
| `model` | 指定具体模型；写 `inherit` 或不填表示跟随主 Agent 当前模型。 |
| `thoughtLevel` | 思考强度档位（如 `high`）。**仅在同时配置了具体 `model` 时生效**，档位必须是该模型支持的值。注意字段名不是 `reasoningEffort`——不认识的字段会被静默忽略，不报错。 |
| `color` | 颜色标记（预设色）。 |
| `tools` / `disallowedTools` | 可用 / 禁用工具列表，边界规则见下文[工具与 MCP](#tools-and-mcp)。 |
| `maxTurns` | 单次调用的最大轮数（正整数）。 |
| `injectAgentsMd` | 是否注入 AGENTS.md，默认注入，见下文[项目指令注入](#agents-md)。 |
| `mcpServers` | 声明该子智能体依赖的 MCP 服务名列表（精确匹配）；声明的服务未连接时，调用会直接失败。 |

`name`
`description`
`model`
`inherit`
`thoughtLevel`
`high`
`model`
`reasoningEffort`
`color`
`tools`
`disallowedTools`
`maxTurns`
`injectAgentsMd`
`mcpServers`

**生效时机**：修改定义文件、或在设置页调整子智能体的模型 / 思考强度后，需要**新建会话**才会生效，已启动的会话不热更新。例外是切换主会话模型——未指定 `model` 的子智能体在后续调用中会立即跟随新模型。

`model`

### 工具与 MCP

子智能体能用哪些工具，取决于「可用工具」（即 `tools` 字段）的配置方式：

`tools`
`tools`
`*`
`Read`
`Grep`
`Glob`
`Bash`
`Edit`
`Write`
`WebFetch`
`WebSearch`
`TodoWrite`
`WebSearch`
`WebFetch`
`tools`
`mcp__<服务名>__<工具名>`
`mcp__server__*`

另外两条边界：子智能体只能看到主会话**启动时**已连接的 MCP 服务，会话中途新连接的服务对子智能体不可见（新建会话即可）；子智能体内不能再派发子智能体。

### 项目指令（AGENTS.md）注入

自 v3.7.1 起，子智能体默认注入用户级 `~/.zcode/AGENTS.md` 与工作区 AGENTS.md，与主 Agent 保持一致；在 frontmatter 中设置 `injectAgentsMd: false` 可关闭。内置的 Explore 默认不注入。v3.7.1 之前的版本，子智能体不注入 AGENTS.md。

`~/.zcode/AGENTS.md`
`injectAgentsMd: false`

### 范围与限制

`~/.zcode/agents/`
`general-purpose`
`Explore`

## 前台与后台执行

子智能体有两种执行方式：

出于安全考虑，后台运行的 `Explore` 子智能体 **只有只读工具**（读取文件、按文件名查找、按内容搜索），不能修改任何东西。

`Explore`

除了子智能体，终端命令同样可以后台执行，详见 [智能体开发环境工具](/cn/docs/ADE-tools)。

## 下一步

#### ZCode Agent

了解 ZCode 自研 Agent 的交互方式和模型适配。

#### Plugin

把技能、命令、子智能体和 MCP 服务器打包成一个扩展。
