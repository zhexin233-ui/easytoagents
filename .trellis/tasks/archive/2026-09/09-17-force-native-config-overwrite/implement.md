# 执行计划：原生配置变更直接强制覆盖

按序执行；每一步先修改/新增失败测试，再完成实现。后端 Preview 合同稳定后再生成 bindings 和接入前端，避免中间协议漂移。

## 本次执行记录（2026-09-17）

核心链路已落地并通过全量门禁：受管漂移按 ownership、完整 baseline 和硬错误分类；中央 mutation 与项目原生资源均按精确 scope 串行执行持久化 Preview → Apply；被动扫描通过 `ExternalChangePlan` 提供脱敏证据和双向动作。Provider、Prompt、MCP、Skill、Hook、Agent 均已接入同一计划 claim、row version/hash 校验和资源专用采纳执行器；可唯一且无损映射时更新中央实体/managed item/baseline，不可映射时 fail closed 并进入应用内匹配/导入入口，不能静默猜测或以 baseline-only readopt 冒充采纳。

当前实现边界已在 backend/frontend 规范中记录：原生采纳统一复用 `ExternalChangePlan` 执行器和事务证据，不恢复独立 readopt 产品入口。资源专用执行器仍负责最终唯一性、无损映射、敏感字段和竞态复核。

## 本地复现补丁（2026-09-18）

使用真实 OpenCode 配置复现到：原生 `provider.ccgo`/`model=opencode/deepseek-v4-flash-free`，中央档案的稳定 ID 为 `easytoagents_*`，且 Provider 受管目标的 full/managed baseline 均为空。旧路径把首次可解析的 Provider 误分类为 `ExternalOwnedChange + CONFLICT`，同时稳定 ID 不一致又不能安全原生采纳，最终两条动作都被阻断。

已收口为：Provider 档案首次无 baseline 时，仅在 scan 为 `Observed`、Provider ownership 合法且中央 desired projection 非空的情况下生成可覆盖的 `ExternalNonOwnedChange`；中央覆盖仍复用持久化 Preview/Apply 并保留未管理字段。稳定 ID 不一致仍返回 `MATCH_OR_IMPORT_REQUIRED`，必须走应用内匹配/导入。无中央 Provider 意图且无 baseline 的被动状态扫描直接跳过未受管目标。回归规则已写入 `.trellis/spec/backend/quality-guidelines.md`。

## 步骤 1：后端可覆盖漂移合同

- [x] 1.1 为漂移评估建立“可覆盖受管漂移 / 硬阻断”清晰分类，保留现有状态诊断但不再把合法 `ExternalOwnedChange` 一律转换为 conflict。
- [x] 1.2 重构 MCP、Hooks、Skills managed-item baseline mismatch，使 Preview 同时保留 mismatch 名单与完整 `ObservedTarget` hashes。
- [x] 1.3 让 Provider、Prompt、MCP、Skills、Hooks、Agents 对已观察且 ownership 合法的受管漂移生成可 Apply update/delete 计划；parse/permission/policy/trust/type/path 错误继续阻断。
- [x] 1.4 保留 Apply 的 descriptor、ownership、row-version、hash 与一次性 claim 校验；确认旧 conflict Preview 不能被新版本直接消费。
- [x] 1.5 增加后端回归：selector 非受管内容保留、whole-document 覆盖、条目漂移覆盖、Preview 后并发变化零写入、重复 claim 单成功。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml sync::
cargo test --manifest-path src-tauri/Cargo.toml profiles::
cargo test --manifest-path src-tauri/Cargo.toml mcp::
cargo test --manifest-path src-tauri/Cargo.toml hooks::
cargo test --manifest-path src-tauri/Cargo.toml skills::
cargo test --manifest-path src-tauri/Cargo.toml agents::
```

回滚点：仅回滚新的漂移分类和 observation 携带方式，不修改 snapshot/journal/apply 内核。

## 步骤 2：后端受影响 scope 协议

- [x] 2.1 定义生成绑定的 `SyncScopeDto`（artifact kind、tool、projectId）和统一 mutation result/等价字段，建立稳定去重排序规则。
- [x] 2.2 为 Provider/Prompt mutation 计算变更前后 active/global tool scope 并集；覆盖 active 编辑、创建首个 active、切换、active 删除和 Pi 多 Provider 投影。
- [x] 2.3 为 MCP/Skill/Hook/Agent 增加按资源反查全部全局与显式项目 assignments 的 repository/service 能力；中央编辑/启停返回完整 scopes，不再只依赖列表 DTO 的全局字段。
- [x] 2.4 让全局/项目 assignment mutation 返回精确 scope；未分配 create/import 返回空 scope；被引用约束拒绝的删除不返回 scope。
- [x] 2.5 增加后端触发矩阵测试：global-only、project-only、混合 assignments，变更前后并集，active Provider/Prompt 删除，Pi，稳定去重排序及全局继承不制造项目 scope。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml profiles::
cargo test --manifest-path src-tauri/Cargo.toml mcp::
cargo test --manifest-path src-tauri/Cargo.toml skills::
cargo test --manifest-path src-tauri/Cargo.toml hooks::
cargo test --manifest-path src-tauri/Cargo.toml agents::
```

回滚点：scope 解析属于中央 mutation 返回协议，不进入原生写内核；协议与生成 bindings 必须一起回滚。

## 步骤 3：被动扫描与外部变化计划

- [x] 3.1 统一六类资源的现场状态扫描，使 Provider/Prompt/MCP/Skill/Hook/Agent 全局与项目目标都能返回当前 drift，而不是部分模块只读 `last_status`。
- [x] 3.2 定义并持久化 `ExternalChangePlan`（或等价 Preview 扩展），绑定 observation hashes、descriptor/ownership、资源/assignment/item row versions、脱敏 diff 与双向动作能力/阻断原因。
- [x] 3.3 实现计划消费的 stale/path/type/row-version 复核；中央覆盖复用普通 persisted Preview/Apply，用户点击动作后不再二次确认。
- [x] 3.4 保留通用 baseline helper 供内部使用，但删除把 readopt 暴露成“采纳”的产品语义和 RPC/UI 依赖。
- [x] 3.5 增加扫描/计划测试：六类状态、只读扫描、plan 后磁盘/DB 变化零写入、脱敏序列化、中央覆盖成功后 in-sync。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml sync::
cargo test --manifest-path src-tauri/Cargo.toml profiles::
cargo test --manifest-path src-tauri/Cargo.toml mcp::
cargo test --manifest-path src-tauri/Cargo.toml skills::
cargo test --manifest-path src-tauri/Cargo.toml hooks::
cargo test --manifest-path src-tauri/Cargo.toml agents::
```

回滚点：外部变化计划只连接现有扫描与写入内核；若回滚，删除新计划/API，不改历史 Preview、snapshot 或 baseline 数据。

## 步骤 4：六类资源采纳原生更改

- [x] 4.1 Provider 复用 `adopt_provider_native` 并接入统一计划；保留 provider id/唯一候选、auth 和 secret 校验。
- [x] 4.2 Prompt 新增唯一 active profile 的正文采纳；Cursor 剥离 frontmatter，override/fallback、空/非法正文 fail closed；不可映射时进入应用内匹配/导入。
- [x] 4.3 MCP 按 managed item identity 解析并更新中央配置；重命名/新增/unknown transport/unsupported 字段返回应用内匹配/导入能力。
- [x] 4.4 Skill 对已配对入口安全 inspect/copy 完整 source tree 到私有 staging，hash 复核后更新中央 content/frontmatter/baseline；name/path/type/竞态异常阻断。
- [x] 4.5 Hook 按 managed item/assignment 采纳可表示字段与脚本；匿名歧义、复杂 shell、事件/assignment 变化进入应用内匹配。
- [x] 4.6 Agent 按 managed path/resource identity 更新中央字段和 tool settings；重命名、同 stem、dropped fields 进入应用内匹配。
- [x] 4.7 各资源服务已覆盖直接采纳、stale row/hash、不可无损映射、敏感字段脱敏、中央实体+item+baseline 一致提交，Skill/Hook 覆盖 staging 清理；共享 `commands::external_changes` 已统一 claim、资源执行器调用、sync item 收敛与 run finalize，服务层与共享链路校验均通过。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml profiles::
cargo test --manifest-path src-tauri/Cargo.toml mcp::
cargo test --manifest-path src-tauri/Cargo.toml skills::
cargo test --manifest-path src-tauri/Cargo.toml hooks::
cargo test --manifest-path src-tauri/Cargo.toml agents::
```

回滚点：各资源采纳执行器以统一计划为边界，可逐资源回滚；不得回滚成 baseline-only readopt 冒充采纳。

## 步骤 5：Skill 接管与项目原生资源直接执行

- [x] 5.1 扩展/复用 Skill takeover Preview，使用户选中的普通目录或外部链接继续生成完整 takeover evidence 与现有 `Mutation::TakeoverSymlink`。
- [x] 5.2 覆盖普通目录 `directory_tree` 快照与恢复、外部链接目标保护、未知兄弟保护、隔离/安装任一步骤失败的回滚测试。
- [x] 5.3 删除项目原生资源 confirmation warning，保留专用 evidence、rowVersion、hash 和恢复占用保护。
- [x] 5.4 增加项目资源禁用/恢复成功、Preview 后变化、未知占用、身份/类型变化测试。

验证：

```bash
cargo test --manifest-path src-tauri/Cargo.toml skills::import_tests
cargo test --manifest-path src-tauri/Cargo.toml sync::apply::tests
cargo test --manifest-path src-tauri/Cargo.toml projects::native_resources
```

回滚点：特殊流程仍通过现有持久化 Preview/Apply 命令；可单独恢复 UI 阻断而无需更改事务内核。

## 步骤 6：设置、RPC 与生成绑定收敛

- [x] 6.1 删除 Rust `ApplyMode`、settings DTO/input 字段和读取/写入分支；保留旧数据库键且不新增 migration。
- [x] 6.2 删除旧 `readoptAvailable`、readopt 产品 RPC/命令注册与 Provider adopt-native 弹窗专用 DTO；将可复用 Provider 逻辑接入统一外部变化执行器。
- [x] 6.3 删除项目原生资源“需要确认”warning 常量与 Preview 文案协议。
- [x] 6.4 将 `SyncScopeDto`、ExternalChangePlan、六类采纳结果与 mutation result 导出到 Specta，运行 `pnpm bindings:generate` 并执行一致性测试。

验证：

```bash
pnpm bindings:generate
pnpm bindings:check
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

回滚点：后端与生成 bindings 必须同一检查点回滚，禁止留下前后端半套 DTO。

## 步骤 7：前端统一 Preview → Apply 生命周期

- [x] 7.1 将共享同步 hook/服务改为可等待的 scope 执行器：稳定去重排序，逐 scope Preview → Apply，空计划 no-op，STALE_PREVIEW 最多重建一次。
- [x] 7.2 让 Provider/Prompt mutation 消费后端 scopes；覆盖编辑当前 Provider、Pi 任一参与档案变化、Prompt 当前选择/编辑/删除。
- [x] 7.3 让 MCP/Skill/Hook/Agent 中央 CRUD、启停与 assignment mutation 消费后端 scopes；覆盖 project-only assignments，消除 Hooks 添加/事件切换/移除非对称。
- [x] 7.4 项目原生资源点击禁用/恢复后直接消费精确 Preview；Skill 接管选择提交后直接消费 prepared Preview。
- [x] 7.5 对多 scope 操作等待全部尝试结束，区分成功/部分失败/完全失败；失败项提供应用内重试并保持查询状态可刷新。
- [x] 7.6 在六类全局/项目状态卡接入外部变化计划，显示非模态“采纳原生更改 / 以中央配置覆盖”；不可直接采纳时进入应用内匹配/导入。
- [x] 7.7 页面进入、窗口恢复 focus、environment-ready 与显式重扫时刷新当前可见资源状态；节流去重且不增加后台轮询/watcher。
- [x] 7.8 删除 `ChangePreviewDialog`、页面级旧 `openPreview`/readopt 接线、设置页 apply mode 开关及失效确认/手工处理文案。
- [x] 7.9 更新前端测试：六类有效 mutation、project-only scope、外部双向动作、匹配/导入、扫描触发、精确 previewId、无二次确认、no-op、stale、部分失败和查询刷新。

验证：

```bash
pnpm test --run src/features/tool-profiles src/features/prompts src/features/mcp src/features/skills src/features/hooks src/features/agents src/features/projects src/features/settings
pnpm typecheck
pnpm lint
pnpm format:check
```

回滚点：共享 hook、特殊流程页面和 settings/bindings 作为一个前端检查点回滚。

## 步骤 8：规范更新与全量质量门禁

- [x] 8.1 用 `trellis-update-spec` 更新 backend quality 中的 drift、managed-item、受影响 scope、ExternalChangePlan/采纳、Skill takeover 和项目资源合同。
- [x] 8.2 更新 frontend quality 中的统一直接执行、六类触发矩阵、按使用时机扫描、双向状态动作、错误/部分失败反馈与 stale 重试；删除旧 direct/preview-confirm/readopt 规范。
- [x] 8.3 检索 `preview_confirm`、`directApply`、`ChangePreviewDialog`、`readoptAvailable`、确认文案与旧 warning，确认无失效 RPC/UI/绑定/规范残留；仅保留服务内部 baseline helper，不将其作为产品采纳能力。
- [x] 8.4 执行完整质量门禁并检查 diff；针对 scope、双向映射、敏感字段、ownership、外部 symlink、恢复占用和快照可恢复性做独立复核。

验证：

```bash
pnpm check
git diff --check
rg -n 'preview_confirm|directApply|ChangePreviewDialog|readoptAvailable|确认原生配置变更|PROJECT_NATIVE_RESOURCE_REQUIRES_CONFIRMATION' src src-tauri .trellis/spec
```

## 完成门槛

- 六类资源的所有有效意图变更以及两个特殊流程均不再出现二次确认、延迟到下次切换或要求手工处理文件夹。
- 后端返回的受影响 scopes 覆盖全局与显式项目分配，前端不再基于不完整 DTO 猜测范围。
- 按使用时机扫描能发现六类外部变化，并提供真正更新中央实体的采纳动作或应用内匹配/导入；不得退化成 baseline-only readopt。
- 所有写入仍由持久化 Preview/Apply、快照、journal 和回滚内核完成。
- 可覆盖漂移与硬阻断测试矩阵完整，生成 bindings、前端、Rust 和全量 `pnpm check` 全部通过。
- 规范与实现同步，最终 diff 不包含无调用的旧模式、旧 RPC 或旧文案。
