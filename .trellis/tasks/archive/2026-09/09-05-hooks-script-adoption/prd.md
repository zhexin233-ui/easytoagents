# 导入 Hook 时接管脚本到中央目录并让原生引用中央副本

## Goal

「检测并导入已有 Hooks」选中导入后，不能只在中央库存命令文本（仍指向原脚本路径）；
必须把脚本本体收进受管的中央目录，分配启用并同步后，各工具原生配置中的命令
**直接引用中央副本**。与 Skills 的「中央不可变副本 + 原生引用」模式对齐，
消除对原脚本路径的运行时依赖。

## Requirements

- **R1 中央脚本目录**：`AppPaths` 新增 `central_hooks()`（`data_root/hooks`，0700）；
  每个 hook 的脚本存于 `central_hooks/<hook_id>/<script_name>`（0600）。
- **R2 导入接管**：discover 阶段解析命令中的脚本路径并校验可接管性；
  confirm 阶段复制脚本到中央目录，中央 `command` 重写为引用中央绝对路径
  （如 `bash "<central>/block-rm.sh"`），原命令的其余部分（解释器、参数）保持不变。
- **R3 接管规则（fail-closed）**：仅当命令首 token 是已知解释器（bash/sh/zsh/
  python/python3/perl/ruby/node）且命令中存在可解析为既有普通文件的 token
  （支持 `~/` 与 `$HOME/` 前缀展开）时接管；`${CLAUDE_PROJECT_DIR}` 等项目级
  变量路径与相对路径不接管（import 仅面向全局目标，这类路径无法安全解析），
  命令原样保存（inline 模式）并给出提示；拒绝符号链接/特殊文件，脚本大小上限 512 KiB。
- **R4 数据模型**：迁移 0015 为 `hooks` 增加 `script_name`/`script_hash`
  （NULL = inline 命令；两者同置同空；hash 为脚本内容 SHA-256，用于去重与生命周期）。
- **R5 去重**：接管型候选与中央已有 hook 按（event、matcher、script_hash、timeout）
  判定 AlreadyManaged；inline 型仍按完整命令比较。
- **R6 生命周期**：删除未分配的 hook 时清理 `central_hooks/<hook_id>/`；
  分配中的 hook 由外键 RESTRICT 阻止删除，保证原生引用永远有效。
  原脚本文件**复制不移动**（保留原文件），与 Skills「复制不接管」一致；
  原生配置改指中央副本由常规同步完成。
- **R7 前端**：导入候选展示「将复制脚本到受管目录 / 命令直存」；中央列表对
  接管型 hook 展示脚本标识；确认导入时回传 `scriptSourcePath` 由服务端二次校验。

## 非目标

- 手动「新增 Hook」表单的脚本接管（后续可复用同一 `script_source_path` 通道）。
- 删除原脚本文件、或对原 hooks 目录做任何写操作。
- ZCode `process` 型 / Cursor `prompt` 型 hook 的接管。

## Acceptance Criteria

- [ ] 导入接管型 hook 后：`central_hooks/<id>/` 存在且 0600；`hooks.command`
      引用中央绝对路径；四工具 Apply 后原生命令引用中央副本；原脚本文件未被动过。
- [ ] inline 命令（无可接管脚本）行为与现状一致。
- [ ] 删除未分配 hook 后中央目录被清理；分配中删除被 RESTRICT 拒绝。
- [ ] 迁移 0015 升级测试（v14→15、旧行保留、CHECK 金丝雀）通过；schema_version 断言 15。
- [ ] `pnpm check` 全绿；bindings 重新生成且 check 通过。
