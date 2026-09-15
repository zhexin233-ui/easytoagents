# 技术设计

## 1. 边界与总体方案

本任务保持现有页面与写入架构，不新建并行流程：Pi 渠道复用 `ToolProfilesPage`；首次接管仍只编排 Provider/Prompt；Pi Skills 复用 `discover_skill_import`、`confirm_skill_import`、`prepare_skill_takeover` 与 `ChangePreviewDialog`。

四个修复点可以在同一任务中独立验证，但共享工具元数据、向导和 Skills 能力合同，采用一个实现任务完成，避免父子任务引入额外集成门槛。

## 2. Pi 渠道路由

- 在共享 `TOOL_PROFILE_ROUTES` 中增加 `path: "pi"`，元素使用带 `key="pi"` 的 `ToolProfilesPage tool="pi"`。
- 不新增专用 Pi 页面；Provider/Prompt/MCP/Skills 能力继续来自生成的 `TOOL_CAPABILITIES`。
- 扩展真实路由清单测试，避免只测试元数据而再次漏注册。

## 3. 首次接管的已接管项过滤

后端 discovery 继续返回原生可导入证据；前端同时查询中央档案状态。向导在 render、选择恢复和 prepare 三处统一派生“当前可处理项”：

- Provider 可处理：有 discovery provider，且没有 active central Provider。
- Prompt 可处理：有 discovery prompt，且该工具没有 active global Prompt。
- 已接管项不渲染 checkbox，不参与 `canPrepare`，也不调用 `previewProviderSync` / `previewPromptSync`。
- 一张工具卡片没有任何可处理或需显式跳过的项时从选择网格隐藏，并作为已解决，不阻塞下一步。
- 只接管一项时，另一项仍按 discovery/capability 状态显示；未安装、未发现配置等现有显式跳过语义保持不变。

`localStorage` 仍保存用户选择，但恢复值必须与当前派生可处理项相交。旧选择仅是偏好，不是权限或发现证据；最新 discovery/central 状态不可选时强制视为 false。该行为改变现行“可重新预览已导入 active profile”的 onboarding 约定，完成实现后同步更新前端质量规范。

## 4. 向导横向溢出

- 共享 `DialogBody` 增加 `min-w-0`，确保它作为 `DialogContent` 的 flex 子项可以收缩；保持既有纵向滚动所有权。
- Onboarding 两列 Grid 的 `fieldset`、预览 `article` 和目标容器增加 `min-w-0`。
- 长 JSON/diff `<pre>` 保留局部 `overflow-auto`，必要时增加 `max-w-full`；路径继续使用 `break-all`。
- 不给调用方新增自定义 `max-w-*`，不改变共享 Dialog 的 size 合同，不移除 `html/body overflow:hidden`。

布局测试以稳定的类/结构合同为主，并在真实浏览器或桌面 smoke 中验证 `dialog.scrollWidth <= dialog.clientWidth`；jsdom 不承担布局计算证明。

## 5. Pi Skills 导入与接管

- 为 `SkillImportSourceKind` 增加 Pi 全局正式来源（序列化名采用与现有命名一致的 `pi_agent_global`）。
- `source_roots` 对 Pi 返回 `environment.pi_agent_dir()/skills`；环境不可映射、能力不支持或策略阻断时继续 fail closed。
- 将该来源加入正式 managed source 判定，使其仅在 descriptor 的全局正式目标精确等于来源根时具备接管资格。
- 项目 `.pi/skills` 不加入此无 project identity 的全局 discovery 命令。
- 复用现有来源链 no-follow 身份、完整树 hash、中央状态指纹、环境指纹、预算和 stale preview 重验。
- 复制成功不创建 assignment、managed target/item、sync run，也不修改原始目录。
- 同名同 hash 且位于正式根的外部链接/真实目录只进入 takeover 分组；准备后仍由持久化 Preview 显式 Apply。
- 已直接指向中央库且能对应已知 Ready 中央记录的入口不附加 takeover 资格，也不得把中央目录复制回自身。
- 不修改 `SKILL_TARGET_INITIAL_UNMANAGED` 的证据条件。

迁移 25 已放宽 `skill_import_previews.tool`，不新增迁移或持久化字段。新 source enum 通过 Specta 重新生成 TypeScript union，禁止手改生成绑定。

## 6. 兼容、风险与回滚

- 来源枚举新增成员可能触发 Rust/TypeScript 穷尽匹配；通过 bindings check 与全量 typecheck 发现遗漏。
- 共享 `DialogBody` 的 `min-w-0` 会影响全部弹窗，但属于允许 flex 子项收缩的非视觉语义修复；回归检查表单、预览和导入弹窗。
- Onboarding 过滤改变已接管项的重新同步入口；工具档案页仍保留正常手动同步入口，因此不会失去同步能力。
- 回滚可按路由、向导过滤、布局、Pi Skills 四组差异逐组撤销；不涉及数据迁移或不可逆数据变更。

