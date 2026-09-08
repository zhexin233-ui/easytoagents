# 仓库探索结果

- 前端目录导入：src/features/skills/skill-directory-import-dialog.tsx:15-35、:97-169；Skills 入口 skills-page.tsx:271-279。
- RPC：src-tauri/src/commands/skills.rs:27-35；输入 models.rs:22-26。
- 单项入库：src-tauri/src/skills/service.rs:65-91；批量事务参考 import.rs:581-724。
- 目录预算 library.rs:31-36；prepare :176-257；排他 rename :266-295；cleanup :351-367。
- Cargo.toml:16-32 没有 HTTP/归档依赖，已有 url；Git 子进程在 git/mod.rs:285-334，仅本地检查用途。
- error.rs:11-46、:92-110、:129-158 使用固定错误与详情白名单。
- 测试入口 package.json:8-19。HTTP 测试使用本地模拟服务与 tempfile，不依赖外网。
- 主代理抽查了 service.rs:65-91、Cargo.toml 和 package.json，结论一致。
