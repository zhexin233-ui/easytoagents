# EasyToAgents App 图标设计 v2

三条路径汇聚为圆润的 E，表达多个 AI 工具的配置统一管理。深墨蓝底色搭配暖白与薄荷青，使用粗线条和宽负空间，照顾小尺寸辨识度。

- `source.png`：内置 image_gen 生成的原始设计，1254 × 1254，带透明通道。
- `icon-1024.png`：1024 × 1024 标准主图。
- `icon-{16,32,64,128,256,512,1024}.png`：常用尺寸，使用 macOS sips 缩放导出。
- `EasyToAgents.iconset/`：标准 macOS 多分辨率资源，包含 Retina 图标。
- `EasyToAgents.icns`：使用 macOS iconutil 打包的应用图标。

图标底板约占画布 80%，外围保留透明边距，沿用项目已有的 Dock 尺寸约定。本目录为独立设计交付，尚未替换 src-tauri/icons 中的当前图标。

## 生成提示词

```text
Use case: logo-brand
Asset type: production-ready macOS app icon for EasyToAgents, a local-first developer app unifying configuration for multiple AI coding agents.
Primary request: Design a refined, distinctive new logo that expresses several tools coming together into one easy workspace. A single bold abstract E-shaped convergence symbol, designed for excellent legibility at 16, 32 and 64 pixels.
Subject: One compact sculptural geometric E monogram constructed from three broad, rounded horizontal pathways merging into a shared curved left spine. The top and bottom paths are warm ivory; a single mint-teal middle path integrates naturally into the same silhouette. The right ends have confident softly rounded cuts. Make the silhouette beautifully balanced, simple, original and effortlessly readable, with ample open negative space between the three arms. The E should feel like an intelligent junction, not a standard typeface glyph, not circuit board wiring.
Style: exceptionally polished contemporary macOS utility app icon; nearly flat vector-like geometry, restrained soft material depth on the tile only, precise smooth edges, optical balance. No intricate 3D.
Composition: exactly one icon, square 1024x1024 canvas. A centered dark ink-navy rounded-square tile with macOS continuous rounded corners, approximately 824x824 pixels (80.5% of the canvas), from x100 to924 and y100 to924. Outside the tile must be real alpha transparency, no white matte, no checkerboard. Keep all shadows very subtle within the padding. Central monogram occupies approximately 55% of full canvas width and height, with comfortable internal breathing room.
Palette: deep ink navy tile, warm ivory primary glyph, fresh mint-teal middle accent. Strong contrast even when very small.
Constraints: output actual standalone icon artwork, not a presentation sheet, not a mockup. No captions, no product name, no wordmark, no tiny details, no outlines, no stars/sparkles, no robot face, no brain, no orbit, no extra nodes or disconnected decoration. Preserve clear chunky forms and wide gaps. True transparent exterior. Render at 1024x1024.
```
