# 验证记录

日期：2026-08-27。

## 代码与自动化检查

- Trellis 实现代理完成后运行 Rust Skills 针对性测试、Skills/MCP 前端测试、lint、typecheck、bindings、clippy、build 与 `pnpm check`，全部通过。
- 独立 Trellis 检查代理补强半 baseline、managed item 漂移、中央损坏、同名普通目录/外部或断裂链接、策略/权限/目标类型及完整 baseline 后非受管变化断言；重新运行全量检查通过。
- 主会话亲自运行 `pnpm check`：Prettier、ESLint、TypeScript、Vitest 8 文件 107 项、Cargo fmt/clippy、Rust 187 项、bindings、command smoke、phase8 E2E 和 doc tests 全部通过。
- 规范/复盘更新后再次运行 `pnpm format:check` 与 `git diff --check` 通过；最终提交前再运行完整 `pnpm check`。

## 真实桌面验证

为避免连接旧的裸 `tauri dev` 进程，先运行：

```bash
pnpm tauri build --debug --bundles app
```

构建成功，Computer Use 连接当前 Debug `.app`，不是 `/Applications` 中的正式包。

### 应用前

- 中央列表中 `smart-search-cli` 为 Ready，Claude/Codex 均“全局已分配”。
- Claude 卡片：`external_non_owned_change` + `SKILL_TARGET_INITIAL_SYNC_PENDING`，展示“已分配，待同步”。
- Codex 卡片：`missing` + 同一专用诊断，展示“已分配，待同步”。

### Claude

- 真实预览目标：`/Users/zhexin/.claude/skills`，change 为 Update，仅在 managed projection 中新增 `smart-search-cli` symlink，目标为已验证中央 UUID 目录。
- 预览保留 `EXTERNAL_NON_OWNED_CHANGE` warning，并明确非受管内容会保留。
- Apply 成功：应用显示“已应用 1 个 Skills 目标，并创建 1 份快照”，Claude 卡片变为 `in_sync`。

### Codex

- 真实预览目标：`/Users/zhexin/.agents/skills`，change 为 Add，before 为 null，after 仅包含同一个 `smart-search-cli` symlink。
- Apply 成功：应用显示“已应用 1 个 Skills 目标，并创建 3 份快照”，Codex 卡片变为 `in_sync`。

## 应用后只读核验

- `~/.claude/skills` 直属条目为 `.DS_Store` 与 `smart-search-cli`；元文件保留，链接解析到中央副本。
- `~/.agents/skills` 由应用创建为 `0700`，直属条目只有 `smart-search-cli`，链接解析到同一中央副本。
- `~/.codex/skills` 仍只有 `.DS_Store` 与 `.system`；兼容来源和内置集合未修改。
- SQLite 只读查询确认两个 global/skill managed target 均 `last_status='in_sync'`，full/managed baseline hash 非空；各有一个 external key 为 `smart-search-cli` 且 item hash 非空的 managed item。
- 最近两次 global Apply 均为 `succeeded`，完成时间分别为 `2026-08-27T02:40:18.270Z`（Claude）与 `2026-08-27T02:40:38.898Z`（Codex）。

## 验证边界

- 未执行 `smart-search-cli` 的技能脚本或网络搜索；本任务验证的是导入/分配/同步状态、物理链接与管理证据。
- 已运行的 Codex/Claude 会话可能需要新会话或重启后才重新发现新增 Skill；不把当前会话元数据缓存当作同步失败。
