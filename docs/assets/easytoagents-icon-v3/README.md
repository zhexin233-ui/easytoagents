# EasyToAgents 科技感图标 v3

电光蓝与冰青色的几何切面组成 E，三段粗线通过左侧骨架汇聚，表达多个 Agent 的统一配置入口。深石墨蓝底板、45° 切角与宽负空间保证小尺寸辨识度。

已直接替换 `src-tauri/icons/` 中的全部现有图标：PNG、ICNS、ICO 和 SVG。Tauri 配置继续引用原路径，重新构建应用后使用新版图标。

## 文件与尺寸

- `source.png`：内置 image_gen 生成的原始图像。
- `icon-1024.png`：标准主图，1024 × 1024，透明外边距约 100px，底板最长边为 824px。
- `icon-16.png`、`icon-64.png`：小尺寸检查图。
- `../../../src-tauri/icons/icon.png`：应用图标主图，与本目录标准主图一致。
- `../../../src-tauri/icons/icon.svg`：内嵌同版 PNG 的兼容容器，并非可编辑的矢量路径。

使用 macOS CoreGraphics 校准透明留白和尺寸，再使用 `pnpm tauri icon` 生成打包所需的多分辨率资源。仅复制项目现有资源类型，不引入移动端资源。前一版设计保留在 `../easytoagents-icon-v2/`。

## 验证结果

- 检查 16、32、64px 的实际显示效果。
- 检查应用所用 PNG 的尺寸与透明通道，确认 SVG 内嵌图像与 PNG 主图一致。
- ICNS 成功解包，ICO 包含 6 个尺寸，Tauri 图标配置引用的文件均存在。
- `pnpm tauri build --bundles app` 构建成功，产物为 0.1.1 版本的 Apple Silicon 应用；包内 ICNS 与新版图标一致。
- `pnpm check` 全部通过：前端 309 项测试，Rust 单元测试 340 项通过、2 项忽略，以及 5 项集成测试通过。
- `pnpm tauri build --bundles app,dmg` 构建成功；只读挂载 DMG 核验新版图标，安装到 `/Applications/EasyToAgents.app` 后确认图标与可执行文件均与构建产物一致。

## 安装包更新注意事项

`pnpm tauri build --bundles app` 只更新 `.app`，不会更新此前生成的 DMG。图标变更后需要安装包时，使用 `pnpm tauri build --bundles app,dmg`，并比较安装包内应用、构建目录应用与源图标的 ICNS 哈希。已安装应用仍显示旧图标时，先核对其 `Contents/Resources/icon.icns`，确认安装的是新版产物，再排查系统图标缓存。

## 生成提示词

```text
Use case: logo-brand
Asset type: actual production macOS app icon for EasyToAgents, a developer utility unifying multiple AI coding agent configurations.
Primary request: Create a new markedly futuristic, technical, precision-engineered app icon. The previous rounded soft ivory E looked too friendly and lacked technology; use an entirely new angular visual language.
Design: a single distinctive geometric E / convergence monogram built from THREE broad interlocking forward-directed blades, joined by a compact angular spine. Consistent precise 45-degree bevel cuts, faceted planar surfaces, strong negative-space channels, striking simple silhouette. The shape suggests three agent streams routed into one engineered core while remaining recognizably E. The upper and lower bars project right, middle bar is shorter. Integrated cyan-to-electric-blue material lighting; light icy cyan on the upper-left facets, saturated electric blue on lower-right facets. Subtle dimensional bevels only, no excessive extrusion. A sharp precision machined emblem, sophisticated developer-tool brand, calm and powerful, NOT a gaming badge.
Background: nearly black graphite/navy macOS continuous-corner rounded square, very subtle cool lighting and a faint restrained inner rim, impeccable premium finish. NO texture or noise.
Composition: square canvas, one centered standalone app icon. Tile exactly 80.5% of canvas width and height, centered with 9.75% transparent padding on every side, matching macOS Dock icon scale. Emblem about 52% of canvas width, balanced and optically centered. Generous negative space. True alpha transparency outside tile, no black or white matte and no checkerboard.
Small-size requirements: must work at 16,32,64 px. Thick bold planes, at least 5% canvas width between major strokes, no hairlines or fine circuitry. Identity must be conveyed by silhouette without depending on glow or shading.
Avoid: rounded capsule strokes, ivory or mint-green palette, bubble shapes, cute style, stock robot heads, brains, sparkles, busy circuit traces, satellite dots, extra rings, text or wordmark, presentation sheet, mockup, excessive neon haze, watermark. No decorative elements. Render the single icon at 1024x1024.
```
