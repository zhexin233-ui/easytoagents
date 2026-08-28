<p align="center">
  <img src="docs/assets/github-hero.png" alt="EasyToAgents — Claude · Codex · MCP · Skills 桌面开发者工具" width="100%" />
</p>

<h1 align="center">EasyToAgents</h1>

<p align="center"><strong>把 Claude、Codex、MCP、Skills 与项目配置收拢到一个可预览、可恢复的本地工作台。</strong></p>

<p align="center"><em>A local-first macOS desktop app for managing Claude, Codex, MCP, Skills, and project synchronization.</em></p>

<p align="center">
  <img src="https://img.shields.io/badge/macOS-13%2B-000000?logo=apple&logoColor=white" alt="macOS 13+" />
  <img src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111827" alt="React 19" />
  <img src="https://img.shields.io/badge/TypeScript-6-3178C6?logo=typescript&logoColor=white" alt="TypeScript 6" />
  <img src="https://img.shields.io/badge/Rust-1.77%2B-000000?logo=rust&logoColor=white" alt="Rust 1.77+" />
</p>

EasyToAgents 面向同时使用 Claude 与 Codex 的开发者。它将分散在工具全局目录和项目目录中的配置整理为中央意图，并把“编辑配置”拆成检测、选择、预览与应用几个明确步骤，降低直接修改原生文件带来的不确定性。

## 核心能力

| 能力                   | 说明                                                                                                                |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------- |
| **Providers / 提示词** | 检测并导入 Claude、Codex 的 Provider 与全局提示词；中央档案的增删改不会直接写入原生配置，切换后需先预览再确认应用。 |
| **MCP**                | 在中央库维护 MCP，按工具或项目分配；保存与删除先更新中央状态，原生配置变更需单独预览并应用。                        |
| **Skills**             | 将 Skill 来源复制到中央目录，再通过指向中央副本的符号链接同步到受管目标；应用前展示计划，应用后保留快照。           |
| **Projects**           | 登记和只读扫描本地项目，在项目维度管理 MCP 与 Skills；移除登记不会删除或改写已有原生配置。                          |

总览页同时呈现中央意图、原生目标状态、同步历史与恢复点，便于判断“希望的配置”和“磁盘上的实际配置”是否一致。

## 安全同步模型

1. **只读检测**：首次接管先扫描 Claude、Codex 与项目中的现有状态，不立即写入。
2. **选择性接管**：只把明确选择的 Provider、提示词、MCP 或 Skill 纳入中央管理。
3. **变更预览**：生成将要创建、更新或移除的目标计划，由用户确认影响范围。
4. **确认应用**：仅在 Apply 后写入原生目标，并保留不属于 EasyToAgents 管理的内容。
5. **快照恢复**：应用产生恢复点，可从同步历史回到先前状态。

## 快速开始

当前支持 **macOS 13+**，项目尚未提供公开 Release 或预编译安装包，需要从源码运行。

准备以下开发环境：

- Node.js 与 pnpm 10（仓库当前声明 `pnpm@10.13.1`）
- Rust 1.77.2 或更高版本
- Tauri 2 在 macOS 上所需的系统开发环境

```bash
git clone https://github.com/zhexin233-ui/easytoagents.git
cd easytoagents
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` 会启动 Vite 前端与 Tauri 桌面窗口；如只需调试前端页面，可运行 `pnpm dev`。

## 常用命令

| 命令                  | 用途                                |
| --------------------- | ----------------------------------- |
| `pnpm dev`            | 启动 Vite 开发服务器                |
| `pnpm tauri dev`      | 以开发模式启动桌面应用              |
| `pnpm build`          | 执行 TypeScript 检查并构建前端      |
| `pnpm test --run`     | 单次运行 Vitest 测试                |
| `pnpm lint`           | 运行 ESLint                         |
| `pnpm typecheck`      | 运行 TypeScript 类型检查            |
| `pnpm bindings:check` | 检查 Rust → TypeScript 绑定是否最新 |
| `pnpm rust:check`     | 检查格式、Clippy 与 Rust 测试       |
| `pnpm check`          | 运行项目完整质量检查                |

## 技术栈

- **桌面端**：Tauri 2、Rust、SQLite（rusqlite）
- **界面层**：React 19、TypeScript 6、React Router 7
- **状态与数据**：TanStack Query 5
- **构建与样式**：Vite 8、Tailwind CSS 4
- **测试与质量**：Vitest、Testing Library、ESLint、Prettier、Clippy

## 当前范围

- 仅支持 macOS 13+；其他桌面平台尚未纳入当前支持范围。
- 当前以源码开发和本地构建为主，没有公开 Release、下载页或独立官网。
- README 仅描述仓库中已经实现且可验证的配置管理流程，不代表所有第三方工具配置都已被覆盖。

## 参与贡献

欢迎通过 [Issues](https://github.com/zhexin233-ui/easytoagents/issues) 报告问题或讨论改进，也欢迎提交 [Pull Requests](https://github.com/zhexin233-ui/easytoagents/pulls)。开始修改前，请先说明变更范围，并确保 `pnpm check` 通过。
