# 技术设计：Hook 脚本中央接管

## 1. 数据模型（迁移 0015_hooks_scripts.sql）

```sql
ALTER TABLE hooks ADD COLUMN script_name TEXT CHECK(script_name IS NULL OR (
    length(script_name) BETWEEN 1 AND 255 AND script_name = trim(script_name)
    AND instr(script_name, char(0)) = 0 AND instr(script_name, '/') = 0
));
ALTER TABLE hooks ADD COLUMN script_hash TEXT CHECK(script_hash IS NULL OR (
    length(script_hash) = 64 AND script_hash = lower(script_hash)
    AND script_hash NOT GLOB '*[^0-9a-f]*'
));
```

- `script_name` = 中央目录内文件名（已清洗的 basename，无路径分隔符）；
  绝对路径由服务层以 `central_hooks/<hook_id>/<script_name>` 推导，不落库。
- NULL = inline 命令（现状行为）；两者由服务层保证同置同空（ALTER 无法加
  表级 CHECK，代码层为唯一写入点）。
- `schema_version` 断言 14 → 15（db/mod.rs 9 处 + app/mod.rs 1 处）。

## 2. 中央目录（AppPaths）

- `central_hooks: data_root.join("hooks")`，加入 `private_directories()`（0700）。
- 脚本文件 `central_hooks/<hook_id>/<script_name>` 用 `create_private_file`
  写入（0600）。与 `central_skills` 同级、同权限模型。

## 3. 接管判定（hooks/import.rs）

1. `split_shell_words(command)`：极简 shell 分词（尊重单双引号；不支持 `$"`、
   反引号等，遇到即整条视为 inline——保守不猜测）。
2. 首 token 的 basename ∈ 解释器集合 {bash, sh, zsh, python, python3, perl,
   ruby, node} 才尝试接管。
3. 在其余 token 中按序找第一个「非 `-` 开头、展开 `~/` 与 `$HOME/` 后命中
   既有普通文件（symlink_metadata，拒绝链接/特殊文件）、大小 ≤ 512 KiB」的
   token 作为脚本。
4. 接管：`script_source_path` = 该绝对路径；discover 不读内容入 DTO。
   未命中 → inline；若首 token 是解释器则给 reason 提示（含 `${...}` 项目级
   变量路径的场景）。

## 4. 确认导入（confirm_hook_import + create_hook 共用通道）

`CreateHookInput` 增加 `script_source_path: Option<String>`（手动新增传 NULL）。
create 流程：

1. 生成 `hook_id`；若带脚本：再次校验文件（规则同 discover）→ 读 bytes →
   `script_hash = sha256(bytes)`；`script_name` = 清洗后的 basename
   （`[A-Za-z0-9._-]` 之外的字符替换为 `_`，空则 `hook.sh`，截断 100）。
2. 重写 command：原分词结果中脚本 token 替换为带引号的
   `central_hooks/<id>/<script_name>` 绝对路径，其余 token 原样拼回。
3. 先写中央文件（0600），后插 DB（带 id）；DB 失败（如名称冲突）→ 删除已写
   文件，返回错误。无脚本时与现状一致。
4. AlreadyManaged 去重：接管型比较 (event, matcher, script_hash, timeout)；
   inline 型比较完整命令（与现状一致）。script_hash 落库支持该判定。

## 5. 生命周期

- `delete_hook`：DB 删除成功后 best-effort 删除 `central_hooks/<id>/` 目录
  （NotFound 忽略）。FK RESTRICT 保证只有无任何分配（无原生引用）时可删除。
- `update_hook`：命令文本可编辑（用户自担）；`script_name/script_hash`
  不经 update 变更；本次不提供中央脚本内容编辑。
- 恢复：native 目标快照恢复后命令仍指向中央路径；分配中 hook 不可删除，
  故中央文件必然存在。

## 6. DTO 与 bindings

- `HookImportCandidateDto` += `script_adopted: bool`、
  `script_source_path: Option<String>`。
- `CreateHookInput` += `script_source_path: Option<String>`（`#[serde(default)]`
  语义由 Option 承担）。
- `HookDto` += `script_name: Option<String>`。
- `pnpm bindings:generate` 重新生成；文档注释避免空行（尾随空格教训）。

## 7. 前端

- 导入对话框：候选行展示「将复制脚本到受管目录（原路径 → 中央副本）」或
  「命令直存（无可接管脚本）」；确认时回传 `scriptSourcePath`。
- Hooks 页面中央列表：接管型显示 `受管脚本` 标识；`createInput` 传 `scriptSourcePath: null`。

## 8. 权衡

- **复制不移动**：原脚本位于不受管作用域，删除超出快照/恢复安全边界；
  功能目标（中央持有本体、原生引用中央）不受影响；与 Skills 先例一致。
- **命令重写为绝对路径落库**：与 skills `central_path` 绝对路径落库同先例；
  数据目录迁移是既有全局约束。
- **极简分词器而非引入 shlex 依赖**：不支持的反曲语法（反引号/`$"`）直接
  判 inline，宁可少接管不可错接管。
