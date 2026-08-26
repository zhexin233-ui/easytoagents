# 验证记录

日期：2026-08-26。状态：实现、独立复核、主代理全量检查和隔离浏览器验收完成；工作提交为 `0f28c56`；用户已明确授权提交并推送。

## 最终命令

| 命令                                                                            | 实际结果                                                                                          |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `cargo test --manifest-path src-tauri/Cargo.toml skills::import -- --nocapture` | 主代理修正内置排除后，14 项导入测试通过                                                           |
| `pnpm check`                                                                    | 主代理最终执行退出码 0；Prettier、ESLint、TypeScript、Rust fmt/clippy 全通过                      |
| `pnpm check` 内的前端测试                                                       | 8 个文件，96 项通过                                                                               |
| `pnpm check` 内的 Rust 测试                                                     | 186 项单元测试和 3 项集成测试通过；集成含 bindings、Tauri app_info IPC smoke、隔离 full-chain E2E |
| `pnpm bindings:generate`                                                        | 实现阶段由后端代理生成，未手写绑定；最终全量运行中的 `generated_bindings_are_current` 再次通过    |
| Skills/MCP 定向前端测试                                                         | 独立复核 2 个文件、60 项通过；已被主代理全量覆盖                                                  |

补充检查：`pnpm build` 退出码 0；新增规范和更新的任务文档通过 scoped Prettier；`task.py validate skills-global-import` 的 implement/check 各 14 个真实条目通过，单文件均小于 32768 字节；`git diff --check` 通过。最终 dirty 清单全部属于本任务，未发现外来变更。

## 验收映射

| 验收                             | 可观察证据                                                                                                                                                        |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC1 空中央库也可检测             | page 测试；浏览器空列表 → Codex 候选 → 单项确认 → 中央列表出现记录                                                                                                |
| AC2 仅兼容目录可用               | `compatibility_links_are_readonly_deduplicated_and_selected_without_adoption`；浏览器 `.agents/skills` 缺失但 `.codex/skills` 候选可选                            |
| AC3 仅选择项且原安装不变         | 同一 Rust 测试核对选择子集、源文件内容、inode/权限、入口链接文本；未选择项不导入；浏览器确认后只一项                                                              |
| AC4 有证据的目录链接与错误反馈   | import/library 覆盖相对/绝对入口、断链/循环、祖先/内部链接/硬链接/特殊文件、非法技能、不可读来源与限额；不执行 fixture 脚本                                       |
| AC5 首次状态与真实漂移           | service 的初始状态矩阵及原有漂移/策略测试；Skills/MCP page 回归；浏览器显示“未纳入同步管理”，复制后仍不声称接管                                                   |
| AC6 选择、重复、冲突、重扫和交互 | 60 项 Skills/MCP 定向测试；浏览器默认无选择、已导入/冲突/无效禁选、提交锁、失败需重扫、选择重置、Escape/焦点恢复                                                  |
| AC7 不自动管理与整批确认         | 导入测试比较 assignment/managed/sync/snapshot 表计数；来源/中央 stale、重用令牌、writer、两个独立连接竞争、第二项故障和不确定提交；浏览器中央两工具均“全局未分配” |
| AC8 自定义根与重复来源           | `custom_roots_same_content_conflicts_invalid_links_and_private_paths`、兼容链接/同内容归并和跨工具中央复用；沿用显式环境路径合同                                  |
| AC9 内置排除                     | 两工具内置集合/真实目标别名矩阵；确认前、copy、SQL 阶段内置身份变化整批拒绝；仅内置无候选/无令牌                                                                  |

## 浏览器可视与交互验收

使用 Browser 插件连接本地隔离 Vite 页面，加载真实 `App`、样式、Skills 页面和对话框。唯一替换是 `@/bindings/commands` 的内存 fixture；未实现的方法立即报错，未建立 Tauri IPC，未连接实际配置或数据库。

临时 fixture：`/var/folders/x5/s776s1nd1wsbcxdskf5ftb400000gn/T/easytoagents-skills-ui-5JQtxv/commands.js`。只监听 `127.0.0.1:4187`，验收后已关闭测试标签页并停止服务。截图使用默认 1280×720 视口，没有调整用户浏览器设置。

实际完成：

- Skills 页面非空、无 Vite 错误覆盖层；Claude 首次已有目录说明、Codex 待初始化和两个检测入口正确。
- Codex 同时展示正式目标和兼容来源，明确 `.system` 排除；默认无勾选，已导入/冲突/非法候选禁选。
- 勾选一项后确认可用，第二项仍未选。提交中关闭/取消/重扫/所有候选及确认按钮禁用，焦点在弹窗容器。
- 成功后弹窗关闭、中央列表仅一项、两工具均未分配、成功说明原安装不变，焦点回到 Codex 检测按钮；无水平溢出。
- Claude 的来源变更 fixture 显示错误，禁止再次确认旧令牌；重扫后错误消失且全部选项复位；Escape 关闭并恢复触发按钮焦点。
- 局部不可用来源保留明确错误，另一个来源的有效候选仍可选；长内容在弹窗正文滚动，标题和底部操作可见。
- 浏览器 console warn/error 列表为空。自动化读取单个 checkbox 的 locator 在重扫后两次超时；随后用新 DOM 快照及只读 DOM 证实已正确渲染和复位，实际勾选/确认继续成功，未发现页面错误。

截图已目视检查：

- [来源局部不可用但兼容来源仍可选择](./research/ui-partial-source.png)
- [来源变化后的错误与重新检测入口](./research/ui-stale.png)

## 独立复核

见 [实现复核记录](./research/implementation-review.md)。主代理核对并修复了跨工具内置排除，以及确认期间内置集合变化的检查遗漏；未成立的解析疑点与枚举预算也记录了代码依据。

## 验证边界与发布注意

- 没有向用户真实 `~/.claude/skills`、`~/.agents/skills`、`~/.codex/skills` 或自定义根调用确认导入/Apply，也没有修改真实应用数据库。
- 浏览器 fixture 验收不等于真实 Tauri 桌面安装验证；新导入 RPC 的业务验证为 Rust service fixture + 生成绑定 + 前端 mock 契约，不声称已做真实宿主全链路导入。
- 不确定提交由故障注入模拟，未制造实际磁盘故障；Linux 专用 rename 分支未在 macOS 上执行。
- 128 MiB 是累计读取/复制预算；限额测试使用受限 Cell 与候选上限，没有制造实际 128 MiB 内容扫描。
- SQLite 与文件系统之间崩溃可能留下私有无记录副本；不自动清扫，也不触碰原安装。提交结果无法判定时保留目录并报错。
- 数据库迁移为 v6。旧二进制不应继续打开新库；回退须使用兼容版本或迁移前私有备份，不自动降级或删表。
