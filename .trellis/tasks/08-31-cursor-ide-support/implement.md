# Cursor 产品扩展：执行计划

## 1. 合同与基础设施

- [x] 将 cursor 加入 Rust Tool 与稳定序列化测试。
- [x] 建立后端共享工具集合与前端 tool-metadata.ts，先替换会把 Cursor 错判为 Codex 的二元 label/icon 分支。
- [x] 增加 Cursor 品牌资源及来源说明。
- [x] 扩展 ToolAvailability、ExplicitEnvironment、DTO 与生成绑定所需导出。

验证：领域序列化单测、前端 metadata 单测、生成 bindings 后运行 pnpm bindings:check。

## 2. 安装探针与 Cursor Adapter

- [x] 实现生产 Cursor.app Bundle ID/version 探测和 agent --version 补充探测，所有输入保持显式、可 fixture 化。
- [x] 新增 Cursor Adapter：Provider/Prompt unsupported；MCP/Skills 全局/项目 descriptors 使用官方路径。
- [x] 为 descriptor path、capability、ownership、sensitive selector、allowed root 添加单测。

验证：tool_probe 与 Adapter 模块测试。

回滚点：若 Desktop plist 解析无法在现有安全边界内完成，停止实现并回到设计，不用仅 CLI 探测替代桌面安装事实。

## 3. 数据库 v10

- [x] 新增并注册 0010_cursor_tool_support.sql。
- [x] 只放宽 MCP/Skills assignments、MCP/Skills import previews 与 managed targets 的工具 CHECK。
- [x] 扩展 Rust Tool 解码器。
- [x] 添加 v9→v10 保留、约束 canary、同连接、重开、外键/索引测试。

验证：cargo test --manifest-path src-tauri/Cargo.toml db:: -- --nocapture。

回滚点：迁移未能精确命中或 schema 对象有任何丢失时，不继续服务层实现。

## 4. MCP 能力

- [x] 在 MCP service/import/registry/status 中加入 Cursor。
- [x] 实现 Cursor mcpServers stdio/HTTP render 与 import parser。
- [x] 覆盖 headers/env/auth 脱敏、未知字段保留、显式导入与不隐式 Apply。
- [x] 扩展全局/项目 preview/apply、stale、冲突和 restore 测试。

验证：MCP 模块测试与 Cursor fixture round-trip。

## 5. Skills 能力

- [x] 在 Skills service/import/registry/status 中加入 Cursor 专属同步目标与兼容导入来源。
- [x] 扩展 allowed root、assignment、baseline、冲突和 restore 测试。
- [x] 用隔离 Skill fixture 验证 Cursor 对受管 symlink 目录的实际发现；记录版本与结果。
- [x] 兼容验证成功；无需触发“固定为 Unsupported 并重新进入规划审批”的失败分支。

验证：Skills 模块测试、隔离目录测试和本机兼容 smoke。

## 6. Profiles、Projects、Overview 与 Sync

- [x] Profiles 对 Cursor Provider/Prompt 返回稳定 unsupported，不写数据库、不生成 preview。
- [x] Project service 加入 Cursor MCP/Skills 状态与分配，排除 Prompt assignment。
- [x] Overview/restore/sync registry 加入 Cursor，并验证 allowed root 与 snapshot 恢复。
- [x] 生成并核对 Rust→TypeScript bindings。

验证：相关 Rust 单元测试、phase8_e2e Cursor 场景、pnpm bindings:check。

## 7. 前端

- [x] MCP/Skills 全局分配按钮、目标状态、导入对话框加入 Cursor。
- [x] Project Detail 加入 Cursor 工具视图，仅渲染 MCP/Skills。
- [x] Dashboard 加入 Cursor 并明确 Provider/Prompt Unsupported。
- [x] AppShell/页面文案去除固定双工具假设；Onboarding 保持 Claude/Codex Profile 专用。
- [x] 更新 fixtures 和 Testing Library 测试，覆盖勾选/取消、禁用、错误和无隐式 Apply。

验证：pnpm typecheck、pnpm test --run、pnpm lint。

## 8. 维护者文档

- [x] 新增 docs/maintainers/adding-tool-adapter.md。
- [x] 记录 capability-first 流程、官方证据表、代码注册点、迁移、bindings、测试与回滚。
- [x] 用 Pi/ZCode 展示 Unknown→Research→Supported/Unsupported 的填写方式，不写未经核验的路径。
- [x] README 更新当前产品范围、首次使用与维护文档链接。

## 9. 完整质量门

- [x] pnpm format:check
- [x] pnpm lint
- [x] pnpm typecheck
- [x] pnpm test --run
- [x] pnpm bindings:check
- [x] pnpm rust:check
- [x] pnpm check
- [x] git diff --check
- [x] 复核 Claude/Codex fixtures、路径和 UI 无回归。
- [x] 复核官方 URL、Cursor capability 文案与维护者文档一致。
