# OpenCode resources research (2026-09-07)

范围：官方 OpenCode 文档/官方仓库规范；网页证据通过 `smart-search` 获取。`opencode` 本机只读探针：`/Users/zhexin/.volta/bin/opencode`，版本 `1.18.29`。

## MCP

Context7 `/anomalyco/opencode` 返回的官方仓库规范摘录（原文保存在 `context7-docs.json`，其来源为 `https://github.com/anomalyco/opencode/blob/dev/specs/v2/config.md`）：

```jsonc
"mcp": {
  "timeout": { "startup": 30000, "request": 300000 },
  "servers": {
    "github": {
      "type": "local",
      "command": ["npx", "-y", "@github/github-mcp-server"],
      "environment": { "GITHUB_TOKEN": "{env:GITHUB_TOKEN}" },
      "disabled": false,
      "timeout": { "startup": 60000 }
    },
    "docs": {
      "type": "remote",
      "url": "https://docs.example.com/mcp",
      "headers": { "Authorization": "Bearer {env:DOCS_TOKEN}" },
      "oauth": {
        "client_id": "{env:MCP_CLIENT_ID}",
        "client_secret": "{env:MCP_CLIENT_SECRET}",
        "scope": "read write",
        "callback_port": 19876,
        "redirect_uri": "http://127.0.0.1:19876/mcp/oauth/callback"
      },
      "disabled": false,
      "timeout": { "request": 600000 }
    }
  }
}
```

事实：OpenCode 官方规范同时声明 local `command` 为数组、`environment`、`disabled`、startup/request timeout；remote `url`、`headers`、OAuth client_id/client_secret/scope/callback_port/redirect_uri。现有模型 `src-tauri/src/domain/mod.rs:417-428` 为 `McpTransport::{Stdio,StreamableHttp}`、`command: Option<String>`、`args: Vec<String>`、`url`、`headers`、`env`、`extra`、`enabled`。因此 local command 数组、timeout、OAuth 专用字段需落入现有 `extra` 或另建结构；现有 HTTP transport 可表达 remote URL/headers，但未显式 OAuth。

## Skills

官方页面 `https://opencode.ai/docs/skills`，原始抓取见 `skills.json`。关键原文：

> “Agent skills let OpenCode discover reusable instructions from your repo or home directory.”

搜索路径原文逐字为：

> `.opencode/skills/<name>/SKILL.md`  
> `~/.config/opencode/skills/<name>/SKILL.md`  
> `.claude/skills/<name>/SKILL.md`  
> `~/.claude/skills/<name>/SKILL.md`  
> `.agents/skills/<name>/SKILL.md`  
> `~/.agents/skills/<name>/SKILL.md`

项目路径会从当前工作目录向上遍历至 git worktree；全局加载 `~/.config/opencode/skills/*/SKILL.md`、`~/.claude/skills/*/SKILL.md`、`~/.agents/skills/*/SKILL.md`。每个 `SKILL.md` 必须 YAML frontmatter，仅识别 `name`, `description`, `license`, `compatibility`, `metadata`；name 正则 `^[a-z0-9]+(-[a-z0-9]+)*$`；description 长度 1-1024。权限在 `opencode.json` 的 `permission.skill` 以 allow/deny/ask 和通配符控制。

与现有模型：`Skill` 字段见 `src-tauri/src/domain/mod.rs:431-439`；`SkillStatus`/`TargetType` 见 `:273-292`。现有 skills 服务已有 symlink 实现（`src-tauri/src/skills/service.rs:1051-1209`；目标类型输出 `targetType: "symlink"`），测试明确项目/global 路径和 symlink（如 `:2892-3004`）。OpenCode 可沿用中央目录到 `.opencode/skills` 及 `~/.config/opencode/skills` 的目录级/技能级 symlink 语义；官方文档没有要求必须 symlink，symlink 是当前产品同步机制的实现事实。

## Hooks / plugins

官方页面 `https://opencode.ai/docs/plugins`，原始抓取见 `plugins.json`。关键原文：

> “Plugins allow you to extend OpenCode by hooking into various events and customizing behavior.”

加载位置：项目 `.opencode/plugins/`、全局 `~/.config/opencode/plugins/`；npm 包写入 config，启动时由 Bun 自动安装并缓存于 `~/.cache/opencode/node_modules/`。加载顺序原文为 global config、project config、global plugin directory、project plugin directory；插件为导出函数的 JavaScript/TypeScript module，函数返回 hooks object。

官方事件集合（原文完整）：`command.executed`; `file.edited`, `file.watcher.updated`; `installation.updated`; `lsp.client.diagnostics`, `lsp.updated`; `message.part.removed`, `message.part.updated`, `message.removed`, `message.updated`; `permission.asked`, `permission.replied`; `server.connected`; `session.created`, `session.compacted`, `session.deleted`, `session.diff`, `session.error`, `session.idle`, `session.status`, `session.updated`; `todo.updated`; `shell.env`; `tool.execute.after`, `tool.execute.before`; `tui.prompt.append`, `tui.command.execute`, `tui.toast.show`。另有 `experimental.session.compacting` hook，可修改压缩上下文/提示词。

事实：OpenCode 没有被官方文档证明存在独立 hooks JSON 配置文件；Hooks 是插件 API（本地 JS/TS 或 npm 插件）返回的 `Hooks` 对象能力。Context7 官方仓库摘录（`context7-docs.json`，来源 `packages/plugin/src/index.ts`）列出 `Hooks { dispose?, event?: (input:{event: Event})=>Promise<void>, config?: (input: Config)=>Promise<void>, tool?: ... }`。

与现有统一模型：`HookEvent` 及其 13 个 canonical 事件见 `src-tauri/src/domain/mod.rs:108-180`；当前实现明确仅支持 command 型，process/prompt 导入被拒绝（`src-tauri/src/hooks/import.rs:243-281`，原文错误分别为“process 型 hook 暂不支持导入（仅支持 command 型）。”、“prompt 型 hook 暂不支持导入（仅支持 command 型）。”）。OpenCode 的插件事件是 JS 回调，不是统一 command hook 条目；不存在可验证的直接语义兼容，不能把事件名猜测映射成 `HookEvent`。若要覆盖 OpenCode 官方 Hooks 全能力，产品模型必须能表示插件模块/事件回调或将其作为 plugin 资源；现有 `ArtifactKind` 仅有 Hook（`src-tauri/src/domain/mod.rs:89-103`），当前 command-only 合同不足。

## 未覆盖/存疑

1. `smart-search fetch https://opencode.ai/docs/mcp` 返回 network_error，未获得当前 docs 页面正文；MCP 关键字段来自 Context7 官方仓库 `specs/v2/config.md` 摘录，需主代理按需要直接抽查该官方 URL。
2. Context7 摘录给出 `skills` 配置数组（local paths/remote URLs）和 `plugin` 配置数组（npm/local/file URL/tuple），但当前官方 docs fetch 的 Skills 页面只证明标准发现目录，未证明这些 config schema 在当前稳定版本 1.18.29 的最终行为；应视为官方仓库规范证据、版本行为仍需核验。
3. 未读取用户配置或运行会话；未安装/修改任何工具或产品文件。
