-- Hook 脚本中央接管：导入的 hook 把脚本本体复制到中央目录
-- `central_hooks/<hook_id>/<script_name>`，命令重写为引用中央绝对路径；
-- 原生配置在同步后直接引用中央副本（与 Skills 中央副本模式对齐）。
--
-- script_name/script_hash 均为 NULL 表示 inline 命令（无接管脚本）；
-- 两者同置同空的约束由服务层保证（hooks::create 通道是唯一写入点），
-- ALTER TABLE 无法追加跨列 CHECK。script_hash 为脚本内容 SHA-256，
-- 用于 AlreadyManaged 去重与生命周期管理。

ALTER TABLE hooks ADD COLUMN script_name TEXT CHECK(script_name IS NULL OR (
    length(script_name) BETWEEN 1 AND 255 AND script_name = trim(script_name)
    AND instr(script_name, char(0)) = 0 AND instr(script_name, '/') = 0
));

ALTER TABLE hooks ADD COLUMN script_hash TEXT CHECK(script_hash IS NULL OR (
    length(script_hash) = 64 AND script_hash = lower(script_hash)
    AND script_hash NOT GLOB '*[^0-9a-f]*'
));
