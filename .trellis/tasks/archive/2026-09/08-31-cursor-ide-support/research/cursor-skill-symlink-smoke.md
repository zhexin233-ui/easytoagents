# Cursor Skills 符号链接兼容性验证

验证日期：2026-08-31。

## 环境

- Cursor Desktop：3.17.21
- Bundle ID：`com.todesktop.230313mzl4w4u92`
- 独立 `agent` CLI：未安装；本次使用 Cursor Desktop 的本地 Agents UI 验证
- 平台：macOS

## 隔离结构

在 `/tmp/easytoagents-cursor-skills-smoke.XePHrK/` 创建：

```text
source/cursor-symlink-smoke/SKILL.md
project/.cursor/skills/cursor-symlink-smoke
  -> /tmp/easytoagents-cursor-skills-smoke.XePHrK/source/cursor-symlink-smoke
```

`SKILL.md` frontmatter 的 name 为 `cursor-symlink-smoke`，description 为“EasyToAgents Cursor 符号链接发现兼容性隔离验证。”。

## 步骤与结果

1. 在 Cursor Desktop 的 Agents 界面打开隔离 `project` 工作区。
2. 在输入框键入 `/` 打开本工作区可用 Skills/commands 列表。
3. 列表明确显示：

```text
cursor-symlink-smoke EasyToAgents Cursor 符号链接发现兼容性隔离验证。
```

4. 测试没有发送 Agent 请求，完成后清空输入框。

结论：Cursor 3.17.21 能发现 `.cursor/skills/<name>` 下指向工作区外 Skill 目录的符号链接。当前 `ManagedChildrenOnly` 模型可以保持 Supported；未来若 Cursor 版本回归，应关闭 Cursor Skills capability，而不是切换到未设计的 Copy 模式。
