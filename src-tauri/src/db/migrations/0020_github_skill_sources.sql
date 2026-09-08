-- Skill 来源除本地绝对路径外，允许服务端规范化后的公开 GitHub 目录 URL。
-- URL 的严格解析、逐段解码与主机限制由 GitHub 下载边界执行；数据库保留
-- 基本形状约束，拒绝查询、片段、凭据形状与相对路径片段。

PRAGMA writable_schema = ON;

UPDATE sqlite_master
SET sql = replace(
    sql,
    'source_path TEXT NOT NULL CHECK(
        source_path LIKE ''/%'' AND source_path != ''/'' AND instr(source_path, ''//'') = 0
        AND source_path NOT LIKE ''%/../%'' AND source_path NOT LIKE ''%/./%''
        AND source_path NOT LIKE ''%/..'' AND source_path NOT LIKE ''%/.''
        AND substr(source_path, -1) != ''/''
    )',
    'source_path TEXT NOT NULL CHECK(
        (
            source_path LIKE ''/%'' AND source_path != ''/'' AND instr(source_path, ''//'') = 0
            AND source_path NOT LIKE ''%/../%'' AND source_path NOT LIKE ''%/./%''
            AND source_path NOT LIKE ''%/..'' AND source_path NOT LIKE ''%/.''
            AND substr(source_path, -1) != ''/''
        ) OR (
            source_path LIKE ''https://github.com/%/%/tree/%/%''
            AND instr(substr(source_path, 20), ''//'') = 0
            AND source_path NOT LIKE ''%?%'' AND source_path NOT LIKE ''%#%''
            AND source_path NOT LIKE ''%/../%'' AND source_path NOT LIKE ''%/./%''
            AND source_path NOT LIKE ''%/..'' AND source_path NOT LIKE ''%/.''
            AND substr(source_path, -1) != ''/''
        )
    )'
)
WHERE type = 'table' AND name = 'skills';

PRAGMA writable_schema = OFF;
