# 实施计划

## 0. 开发前门禁

- [ ] 加载 trellis-before-dev 与 Phase 2.1 细节。
- [ ] 阅读 backend/frontend index、quality、database、error、component、hook、state、type-safety、MCP import、Skill import 和本任务 research。
- [ ] 确认工作树只包含本任务规划产物，分支为 codex/manage-project-resources。

## 1. 数据库与领域模型

- [ ] 新增 v12 migration：project_native_resources、唯一约束、状态/entry type 检查、row version 与 snapshot 引用保护。
- [ ] 在 db/mod.rs 注册迁移，不修改历史 migration。
- [ ] 增加 repository CRUD、CAS、按 project/target/state 查询与 referenced snapshot / project removal guard。
- [ ] 明确 target identity upsert 只创建空 baseline、无 managed item 的中性目标行，并测试它不构成 ownership、不放宽普通 Apply。
- [ ] 把 snapshot 引用保护接入现有 sync::delete_snapshots、项目移除和所有 retention/cleanup 入口；禁止只在新 repository API 中保护。
- [ ] 增加旧数据库升级、重复打开、约束、CAS、显式/批量快照删除和项目移除保护测试。
- 回滚点：该阶段不读取或写入项目原生目标；迁移失败由现有预迁移备份与事务处理。

## 2. 只读逐项发现

- [ ] 新增 projects/native_resources 领域模块和安全 DTO。
- [ ] 复用 adapter descriptor/scan_target，按 MCP selector、Skill 直属入口、Prompt 整文档提取逐项观察。
- [ ] 对照 managed_items 区分中央托管、中央漂移与项目原生；不创建 ownership。
- [ ] 登记、get、rescan 返回 native summary；详情专用命令返回脱敏逐项列表。
- [ ] 补入 Claude/Codex Prompt，保持 Cursor Prompt unsupported。
- [ ] 覆盖空/缺失/非法目标、三平台矩阵、managed collision、missing/conflict 对账、登记零原生写测试。
- 回滚点：此阶段仍为纯发现，若写路径未准备好可独立保留。

## 3. 持久化预览与 Apply 意图

- [ ] 新增 ProjectNativeResourceAction 枚举、preview/apply 命令和 Specta 注册。
- [ ] Preview 只接受 resource ID、row version、action；服务端保存 descriptor、target/item hash、target/resource row version、snapshot/action 私有证据。
- [ ] Preview 与 Apply 都穷尽校验 active+disable、disabled+restore 状态矩阵；missing/conflict/中央托管项拒绝。
- [ ] 接入全局 writer、claim_preview、journal、权限审计和 stable error codes。
- [ ] direct 偏好不自动 Apply。
- [ ] 覆盖 forged/stale/consumed preview、非法动作状态、并发 writer、rollback_failed、项目软删除、目标身份变化测试。

## 4. MCP 禁用与恢复

- [ ] 复用 JSON/TOML selector renderer 和敏感 selector redaction。
- [ ] 禁用：whole-target snapshot → 删除单 selector → 原子写 → 写后验证 → 状态提交。
- [ ] 恢复：从私有 snapshot 解析原条目 → 检查同名为空 → 当前目标回滚快照 → selector 合并 → 验证 → 状态提交。
- [ ] 覆盖 Claude/Codex/Cursor、未知兄弟语义值、TOML 目标外注释、同名占用、secret carrier audit、父文件缺失恢复、活动 writer 和回滚失败。

## 5. Skill 禁用与恢复

- [ ] 复用完整树 digest/copy/verify/remove、symlink no-follow identity 和限额。
- [ ] 为有精确原生资源证据的目录/链接增加收窄 mutation，不能放宽普通 Apply。
- [ ] 禁用普通目录与外部 symlink；不触碰链接目标。
- [ ] 恢复时使用同父临时入口 + 排他 rename，目标占用则保留 snapshot。
- [ ] 覆盖三平台、普通文件 bytes/mode、目录树、内部安全链接文本、外部链接文本、断链、硬链接/逃逸/特殊文件、父目录缺失恢复、活动 writer、路径竞态和崩溃回滚。

## 6. Prompt 禁用与恢复

- [ ] 仅支持 Claude CLAUDE.md 与 Codex AGENTS.md exact descriptor。
- [ ] payload_file snapshot 后移除整文件；恢复要求目标为空并还原字节/mode。
- [ ] 覆盖 exact bytes/mode、Cursor unsupported、父文件缺失恢复、外部重建冲突、权限变化、活动 writer 和 rollback_failed。

## 7. 前端与绑定

- [ ] 生成 commands.ts，新增 projects API query keys/mutations。
- [ ] 在 ProjectDetailPage 的当前平台/资源组合中加入“项目原生资源”分区，保留“中央追加”分区。
- [ ] 展示 active/disabled/missing/conflict、安全来源和诊断；实现禁用/恢复按钮。
- [ ] 复用 ChangePreviewDialog 与焦点/模态锁；direct 下仍只打开预览。
- [ ] 成功后联合失效 project、native resource、MCP、Skill、Prompt 和 recovery 查询。
- [ ] active writer / rollback_failed 显示全局阻断；disabled 在资源缺失时仍提供恢复，conflict 仅在目标键占用时出现。
- [ ] 项目卡/登记反馈展示原生资源计数；存在 disabled 时移除项目给出阻断提示。
- [ ] 扩展 projects-page.test.tsx、project-detail-page.test.tsx 和预览对话测试，覆盖加载、空、错误、晚到响应、双提交、焦点与无敏感内容。

## 8. 全量验证

- [ ] pnpm bindings:generate
- [ ] pnpm bindings:check
- [ ] 对修改过的 Rust 模块运行定向 cargo test。
- [ ] 对修改过的前端测试运行 pnpm test --run 的定向文件。
- [ ] pnpm rust:check
- [ ] pnpm check
- [ ] 使用隔离临时 HOME/CODEX_HOME/CLAUDE_CONFIG_DIR fixture 完成跨平台矩阵，不操作真实用户目录。
- [ ] 对 commands/overview.rs 的 delete_snapshots 执行集成测试，证明禁用资源引用快照不可删除、恢复完成后可按历史快照策略处理。
- [ ] 若条件允许，pnpm tauri dev 做真实桌面 smoke；未执行时在交付中明确说明。

## 9. 复核与规范

- [ ] 运行 trellis-check，逐条复核 CRITICAL/WARNING 的实际数据来源与安全边界。
- [ ] 更新 backend/frontend spec，记录项目原生资源来源、Preview/Apply、快照引用和 UI 状态合同。
- [ ] 检查 PRD AC1-AC9 映射，确认无自动接管、无 silent Apply、无 secret 泄露。
- [ ] 通过质量门禁后提交中文 commit，并执行 trellis-finish-work。
