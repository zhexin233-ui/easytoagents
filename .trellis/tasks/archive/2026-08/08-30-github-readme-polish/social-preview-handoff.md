# GitHub Social Preview 人工交接

## 当前状态

- 预览素材为仓库中的 `docs/assets/github-hero.png`，尺寸 1280×640，符合本任务设计。
- 2026-08-30 通过 GitHub REST 请求读取 `repos/zhexin233-ui/easytoagents/social-preview` 返回 HTTP 404，无法据此确认或更新 Social Preview。
- 已登录的 GitHub 页面可进入上传界面，但 Chrome 扩展未获 `file://` 访问权限，文件选择被拒绝，因此本次没有上传或覆盖线上预览图。
- 在无法确认现有自定义预览图的情况下，不应自动覆盖。

## 人工操作步骤

1. 打开仓库 `zhexin233-ui/easytoagents`，进入 **Settings → General → Social preview**。
2. 先确认页面当前是否已有自定义预览图；若存在未知图片，停止并向仓库维护者确认是否替换。
3. 点击 **Edit** 或 **Upload an image**，选择本地仓库中的 `docs/assets/github-hero.png`。
4. 保存后重新打开该设置区域，确认显示的是 1280×640 的蓝金 Hero。
5. 等待 GitHub 分享卡片缓存刷新后，再用仓库 URL 检查外部分享预览。
