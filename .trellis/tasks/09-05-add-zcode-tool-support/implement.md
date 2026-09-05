# Implement: ZCode 工具全套支持

## 执行清单（有序）

### 后端核心

1. [x] `src-tauri/src/domain/mod.rs`：`Tool` 加 `Zcode => "zcode"`；补往返测试。
2. [x] `src-tauri/src/adapters/zcode/mod.rs`：新建 `ZcodeAdapter`（矩阵见 design.md）+ 测试。
3. [x] `src-tauri/src/adapters/mod.rs`：`pub mod zcode`；`PROFILE_TOOLS`、`ASSIGNABLE_MCP_TOOLS`、`ASSIGNABLE_SKILL_TOOLS` 加 ZCode；`ToolAvailability` 加字段并更新所有构造点；`ExplicitEnvironment` 加 `zcode_installation_version` + match 臂。
4. [x] `src-tauri/src/app/tool_probe.rs`：`ZCODE_BUNDLE_ID`、`zcode_app_paths`、`ReleaseToolProbeResult.zcode`、probe 函数；把 cursor bundle 读取抽象为共用函数；测试矩阵。
5. [x] `cargo test` 编译通过后再继续（fast feedback）。

### DB 迁移

6. [x] `src-tauri/src/db/migrations/0013_zcode_tool_support.sql`。
7. [x] `src-tauri/src/db/migrations.rs`（或注册处）登记迁移；升级测试（旧行保留/canary/重开）。

### 服务层

8. [x] `db/profiles.rs`：is_active_zcode 读写、`Tool::Zcode` 分支。
9. [x] `db/mcp.rs` / `db/skills.rs` / `db/mcp_imports.rs` / `db/skill_imports.rs`：tool 与 source_kind 解析。
10. [x] `mcp/service.rs`、`mcp/import.rs`：注册表、allowed root、容器键、投影。
11. [x] `skills/service.rs`、`skills/import.rs`：注册表、`ZcodeHome`/`ZcodeAgents` 来源（`skills/models.rs` 枚举）。
12. [x] `profiles/service.rs`：ZCode provider 发现/校验/渲染；prompt import 目标。
13. [x] `projects/service.rs`、`projects/native_resources.rs`：目标链、原生资源、恢复。
14. [x] `overview/mod.rs`：tool 解析、active provider、恢复 allowed root。
15. [x] `sync/apply.rs`：`adapter_for` 注册。

### 前端

16. [x] `pnpm bindings:generate`；确认 `Tool` 含 `"zcode"`。
17. [x] `src/lib/tool-metadata.ts` + 图标 + `tool-metadata.test.ts`。
18. [x] `settings-dialog.tsx`、`skill-import-dialog.tsx` 及相关测试。
19. [x] 各 feature 测试文件工具数组补齐。

### 文档与收尾

20. [x] `README.md`、`docs/maintainers/adding-tool-adapter.md` 第 9 节。
21. [x] 全量 `pnpm check`；`git diff --check`。

## 验证命令

```bash
cargo test --manifest-path src-tauri/Cargo.toml   # 后端快速反馈
pnpm check                                         # 完整质量门
git diff --check
```

## 回滚点

- 迁移 SQL 独立文件，可整体摘除（已应用则保留）。
- 每个服务文件独立可回退；`Tool::Zcode` 枚举值一旦入库不回滚。
