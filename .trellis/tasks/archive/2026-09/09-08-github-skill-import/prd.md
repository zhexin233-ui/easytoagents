# GitHub 目录链接导入 Skill

## 目标
在 Skills 页面输入 GitHub 上单个 Skill 的根目录链接，自动下载该目录的完整内容并导入中央库，省去手动下载和选目录。

## 背景与证据
- 当前本地入口位于 `src/features/skills/skills-page.tsx:271`，调用目录导入弹窗。
- 单目录服务 `src-tauri/src/skills/service.rs:65` 已提供校验、复制和入库流程。
- `src-tauri/src/skills/library.rs:31` 定义目录大小与文件数限制，`:216` 和 `:266` 实施名称冲突与排他落盘。

## 需求
- R1：新增「从 GitHub 导入」入口，输入链接后点击导入，显示下载中及成功/失败状态，保留已有导入方式。
- R2：支持公开 GitHub 仓库的单个 Skill 目录 `https://github.com/{owner}/{repo}/tree/{ref}/{path}`，下载目录内 SKILL.md 和全部支持的资源及子目录。
- R3：技能名称沿用 SKILL.md 元数据解析；同名冲突不覆盖、不自动改名。无合法 SKILL.md 或内容超限时拒绝导入。
- R4：只创建中央副本，不执行技能脚本，不自动分配、同步或接管工具目录。
- R5：失败给出可理解的反馈；正常失败清理本次临时下载，不显示半成品。提交成功但列表刷新失败须明确说明已导入，避免重复提交。

## 验收条件
- A1 / R1-R3：以下链接均能完成导入，资源文件完整：
  - https://github.com/anthropics/skills/tree/main/skills/pdf
  - https://github.com/vercel-labs/skills/tree/main/skills/find-skills
- A2 / R1、R5：下载期间禁止双提交，成功刷新 Skills 列表；失败可修改链接重试。
- A3 / R2、R3、R5：无效链接、不存在的 ref/目录、网络失败或限流、非法内容和名称冲突有明确反馈，无可见半成品。
- A4 / R4：导入不更改工具目录、分配或同步记录。
- A5 / R2：固定一次解析的提交版本下载，避免分支更新造成文件混合。

## 本次范围外
私有仓库登录、整个技能合集的扫描批量导入、自动更新已导入技能、其他托管平台。
