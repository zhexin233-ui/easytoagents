<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

This project is managed by Trellis. The working knowledge you need lives under `.trellis/`:

- `.trellis/workflow.md` — development phases, when to create tasks, skill routing
- `.trellis/spec/` — package- and layer-scoped coding guidelines (read before writing code in a given layer)
- `.trellis/workspace/` — per-developer journals and session traces
- `.trellis/tasks/` — active and archived tasks (PRDs, research, jsonl context)

If a Trellis command is available on your platform (e.g. `/trellis:finish-work`, `/trellis:continue`), prefer it over manual steps. Not every platform exposes every command.

If you're using Codex or another agent-capable tool, additional project-scoped helpers may live in:
- `.agents/skills/` — reusable Trellis skills
- `.codex/agents/` — optional custom subagents

Managed by Trellis. Edits outside this block are preserved; edits inside may be overwritten by a future `trellis update`.

<!-- TRELLIS:END -->

# 本地开发环境注意事项

## GitHub 网络代理

- 当前 macOS 系统代理为 `127.0.0.1:10808`。浏览器会自动使用系统代理，但 Codex 启动的 shell 不一定继承 `HTTP_PROXY`、`HTTPS_PROXY` 或 `ALL_PROXY`，因此可能出现浏览器能打开 GitHub、`git push`/`curl` 直连却超时的情况。
- 遇到该现象时，先用 `scutil --proxy` 核对当前代理地址，并确认本地代理端口仍可访问；不要把超时误判成 GitHub 认证或仓库权限错误。
- 需要执行 GitHub 命令时，优先仅为单次命令注入当前代理，例如：`HTTP_PROXY=http://127.0.0.1:10808 HTTPS_PROXY=http://127.0.0.1:10808 ALL_PROXY=http://127.0.0.1:10808 git push origin main`。
- 未经用户明确授权，不写入永久 Git 代理配置，也不修改 macOS 系统代理设置。
