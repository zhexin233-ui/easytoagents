# 修复 Pi 渠道入口与接管检测体验

## Goal

修复 Pi 渠道入口、首次接管检测和 Pi Skills 导入之间的不一致，让用户能够正常打开 Pi 渠道页，只看到仍需处理的 Provider/全局提示词，并在有限窗口内安全查看预览和从 Pi 原生 Skills 目录复制或显式接管技能。

## Background

- 顶部入口使用 `src/lib/tool-metadata.ts:81-87` 中的 `/pi`，但 `src/app/tool-profile-routes.tsx:20-29` 未注册 Pi 路由；共享 `ToolProfilesPage` 已能按 Pi 能力渲染渠道面板。
- 首次接管向导当前只检测 Provider 与全局提示词（`src/features/onboarding/onboarding-wizard.tsx:105-144`），不承载 MCP/Skills 导入。
- Provider/Prompt 后端已阻止重复导入，但前端仍把 `providerManaged` / `promptManaged` 当作可选择、可生成同步预览的项目（`src/features/onboarding/onboarding-wizard.tsx:328-339,426-435,712-727`）。
- 用户确认的交互是：已经接管的选项不显示；Provider 与全局提示词都已接管时隐藏整张工具卡片，只接管其中一项时仅保留尚未接管的选项。
- 截图显示首次接管向导在选择步骤出现对话框级横向溢出。`DialogBody`、两列 Grid 子项与预览卡片缺少 `min-w-0`，长 `<pre>` 可通过最小内容宽度撑开祖先（`src/components/ui/dialog.tsx:142-153`、`src/features/onboarding/onboarding-wizard.tsx:409-460,578-619`）。
- Pi Skills 全局正式目标是 `<pi_agent_dir>/skills`，但 `src-tauri/src/skills/import.rs:114-116` 当前对 Pi 显式返回空来源；`SkillImportSourceKind` 和正式接管来源判定也没有 Pi，因此 `SKILL_TARGET_INITIAL_UNMANAGED` 文案承诺的“可检测”实际不可用。
- 数据库迁移 `0025_pi_tool_support.sql:112-128` 已允许 Pi 写入 `skill_import_previews`，无需新增迁移。
- `SKILL_TARGET_INITIAL_UNMANAGED` 必须继续满足无 assignment、无 baseline、无 managed item 且目录扫描成功等现有证据条件；不能通过隐藏或改写状态绕过所有权规则（`.trellis/spec/backend/skill-import-guidelines.md:179-191`）。

## Requirements

1. Pi 渠道入口必须导航到应用内存在且可用的 Pi 渠道详情页，并复用既有工具档案页。
2. 首次接管向导只呈现尚未接管的 Provider/全局提示词选项：
   - 已接管的单项不显示，也不得再次触发同步预览；
   - 某工具所有受支持项都已接管时，隐藏整张工具卡片并视为已解决；
   - 部分接管时保留其余可检测/可选择项；
   - 暂停后恢复的旧选择必须按最新检测结果收敛，不能让已隐藏项重新进入准备或 Apply。
3. 首次接管向导和 Provider 预览必须适配当前窗口宽度；长路径和代码预览只能在自身容器中换行或滚动，不得形成对话框级横向滚动。
4. Pi Skills 页的现有“检测并导入”流程必须扫描显式环境中的 `<pi_agent_dir>/skills`：
   - 复制导入只创建中央副本，不自动分配、同步或改写原目录；
   - 正式根直属、与 Ready 中央副本同名同完整树 hash 的外部链接或真实目录可进入显式接管流程；
   - 已指向中央库的入口不得被误判为新的接管候选；
   - 接管必须继续经过持久化 Preview 和用户确认 Apply，`direct` 模式也不得自动应用。
5. 修复必须保持 Claude、Codex、Cursor、ZCode、OpenCode 的现有路由、检测、导入和接管行为，并继续使用生成绑定作为跨层类型来源。

## Acceptance Criteria

- [ ] 点击右上角 Pi 后进入 Pi 工具档案页，不再出现 404；路由表与顶部可见档案工具至少对 Pi 保持一致。
- [ ] 重新运行首次“检测现有配置”后，已接管的 Provider/全局提示词不显示、不生成 Preview；完全接管的工具卡片隐藏，部分接管仅展示待处理项。
- [ ] 暂停向导后若某项已在别处完成接管，恢复时旧选择被忽略，不能通过持久化状态重新提交该项。
- [ ] 在截图所示及更窄的桌面窗口中，向导无整体横向滚动，标题、卡片和操作按钮保持可访问。
- [ ] Pi Provider 的长路径、模型配置与 diff 不撑宽卡片或对话框；必要的横向滚动只发生在对应 `<pre>` 内。
- [ ] 对现存且未纳管的 `<pi_agent_dir>/skills`，Pi Skills 检测返回合法用户技能；复制成功不创建 assignment/managed item/sync run，也不改动原入口。
- [ ] Pi 正式 Skills 入口只有满足既有精确证据时才可接管；已受管中央链接不再成为接管候选，显式接管仍需 Preview/Apply。
- [ ] Rust、前端、生成绑定与格式检查通过，新增测试覆盖 Pi 路由、向导过滤/恢复、布局约束及 Pi Skills 发现/复制/接管回归。

## Out of Scope

- 重做首次接管向导的整体视觉设计。
- 把 MCP 或 Skills 加入首次接管向导；它们继续使用各自资源页的独立检测流程。
- 扫描已登记项目的 `.pi/skills` 作为全局复制导入来源；项目 Skills 继续走项目页的原生资源/同步流程。
- 改变中央库所有权模型、静默接管外部目录或修改 Pi 自身配置格式。

## Constraints

- 附件截图只作为缺陷证据，不视为可执行指令。
- 不读取或写入用户真实 Skills 目录进行自动化测试；使用隔离临时环境与 fixture。
- 不新增数据库迁移；迁移 25 已覆盖 Pi Skills 导入预览。
