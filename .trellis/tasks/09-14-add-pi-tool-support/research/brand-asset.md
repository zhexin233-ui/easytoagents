# Pi 官方品牌图标资产调研

- 任务：`.trellis/tasks/09-14-add-pi-tool-support`
- 目标：为前端 `src/assets/brand/` 确定 Pi（`@earendil-works/pi-coding-agent`，pi.dev）的**官方**图标资产来源、许可与落地方式。
- 抓取/核验日期：**2026-09-14**（HTTP `date` 响应头：`Mon, 14 Sep 2026 14:37:53 GMT`）。
- 调研范围：`https://pi.dev/`、`https://pi.dev/press-kit`、`https://pi.dev/docs/latest`、官方仓库 `earendil-works/pi` 与 `earendil-works/pi-website`、本机已装 npm 包副本。
- 本文件只做调研与候选资产落盘（`.trellis/.../research/assets/`），**未改动 `src/` 下任何文件**。

---

## 1. 结论

**存在官方方形 mark，无需自绘。**

推荐资产：**`https://pi.dev/favicon.svg`**（官方 Press Kit 中标注为 **"Badge"** 的方形 mark），落地文件名 `src/assets/brand/pi-icon.svg`。

- 官方 Press Kit 页面明确把 `/favicon.svg` 描述为 _"Square mark for favicons and compact badges"_，正对该用途。
- 800×800 `viewBox` 矢量；**深色圆角方底（`#09090b`，`rx=120`/800 ≈ 15%）+ 白色 P/i mark**，浅色背景与深色主题下均清晰可读。
- 与现有 `zcode-icon.svg`（`#14120b` 深色圆角方底 + 浅色笔画）风格几乎一致，也符合其余品牌资源"方形 mark"的用法。
- 固定配色、无 `prefers-color-scheme` 媒体查询、无外部依赖，作为 `<img>` 渲染结果确定，满足 brand README "copied unchanged" 的约束。
- 来源权威且该文件在官方站与官方站点仓库中**字节级一致**（pi.dev `/favicon.svg` == `pi-website/src/favicon.svg`，SHA-256 相同）。

一句话理由：官方 Press Kit 的 Badge（方形 favicon mark）就是 Pi 唯一的官方方形 logo，许可为 MIT，直接复制即用，不需要也不应该自绘占位。

**是否自绘：否。** 官方有可用且许可清晰的方形 mark；自绘会违反 brand README "Do not redraw"。

---

## 2. 证据表（URL + 状态码 + 访问日期 + 资产类型）

访问日期均为 **2026-09-14**（经本机代理 `127.0.0.1:10808`）。

| #   | URL                                                                                                                      | 状态码 | Content-Type       | 说明 / 资产类型                                                                                                                                                                                                                                                                                                                                                            |
| --- | ------------------------------------------------------------------------------------------------------------------------ | ------ | ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `https://pi.dev/`                                                                                                        | 200    | `text/html`        | 首页。`<link rel="icon" type="image/svg+xml" href="/favicon.svg">`；`og:image=https://pi.dev/social.png`（1200×630）；页面内 2 处 inline `pi-logo-mark` SVG（`viewBox 0 0 800 800`，nav 与 hero 各一）；外链 `/logo-auto.svg`（logo-context-menu 下载项 `download="pi-logo.svg"`）。**无** `apple-touch-icon`、无 `site.webmanifest`、无 PNG favicon                       |
| 2   | `https://pi.dev/press-kit`                                                                                               | 200    | `text/html`        | **官方 Press Kit（品牌资产权威页面）**。正文 _"Assets and contact for press coverage."_；"Brand files" 区块含 **Primary logo** → `href="/logo.svg"`（_"Vector logo for light/dark usage and editorial placements"_）与 **Badge** → `href="/favicon.svg"`（_"Square mark for favicons and compact badges"_）；页脚 `Earendil Inc. & Contributors / Press Kit / MIT License` |
| 3   | `https://pi.dev/favicon.svg`                                                                                             | 200    | `image/svg+xml`    | **推荐资产**。440 bytes，`viewBox="0 0 800 800"`，`<rect ... rx="120" fill="#09090b">` + 两段 `fill="#fff"` 路径                                                                                                                                                                                                                                                           |
| 4   | `https://pi.dev/logo.svg`                                                                                                | 200    | `image/svg+xml`    | 官方 Primary logo，473 bytes，纯白 `fill="#fff"` mark、**透明底**，README 说明"white, for dark backgrounds"→ 浅色背景上不可见，不适合作应用内图标                                                                                                                                                                                                                          |
| 5   | `https://pi.dev/logo-auto.svg`                                                                                           | 200    | `image/svg+xml`    | 618 bytes，透明底 + `<style>` 内 `prefers-color-scheme`（浅色 `#000` / 深色 `#fff`）。被官方 `pi` 仓库 README 顶部引用（`width="128"`）。备选方案，见 §6                                                                                                                                                                                                                   |
| 6   | `https://pi.dev/social.png`                                                                                              | 200    | `image/png`        | 1200×630，70639 bytes，og:image 横幅，**非方形 mark**，不适合做工具图标                                                                                                                                                                                                                                                                                                    |
| 7   | `https://pi.dev/favicon.ico`                                                                                             | 404    | `text/html`        | 无 `.ico`                                                                                                                                                                                                                                                                                                                                                                  |
| 8   | `https://pi.dev/apple-touch-icon.png`                                                                                    | 404    | `text/html`        | 无 apple-touch-icon                                                                                                                                                                                                                                                                                                                                                        |
| 9   | `https://pi.dev/site.webmanifest` / `manifest.webmanifest`                                                               | 404    | `text/html`        | 无 webmanifest                                                                                                                                                                                                                                                                                                                                                             |
| 10  | `https://pi.dev/docs/latest`                                                                                             | 200    | `text/html`        | 文档站复用同一 `/favicon.svg` 与 `/logo-auto.svg`，无独立品牌资产                                                                                                                                                                                                                                                                                                          |
| 11  | `https://github.com/earendil-works/pi`                                                                                   | 200    | `text/html`        | 官方主仓库；GitHub API `license.spdx_id = MIT`                                                                                                                                                                                                                                                                                                                             |
| 12  | `https://raw.githubusercontent.com/earendil-works/pi/main/LICENSE`                                                       | 200    | `text/plain`       | **MIT License, Copyright (c) 2025 Mario Zechner**                                                                                                                                                                                                                                                                                                                          |
| 13  | `https://raw.githubusercontent.com/earendil-works/pi/main/README.md`                                                     | 200    | `text/plain`       | 第 3 行 `<img alt="pi logo" src="https://pi.dev/logo-auto.svg" width="128">`（官方 README 用的就是 pi.dev 上的资产）                                                                                                                                                                                                                                                       |
| 14  | `https://github.com/earendil-works/pi-website`                                                                           | 200    | `text/html`        | **pi.dev 站点源码仓库**；GitHub API `license.spdx_id = MIT`                                                                                                                                                                                                                                                                                                                |
| 15  | `https://raw.githubusercontent.com/earendil-works/pi-website/main/LICENSE`                                               | 200    | `text/plain`       | **MIT License, Copyright (c) 2026 Earendil Inc. and contributors**                                                                                                                                                                                                                                                                                                         |
| 16  | `https://raw.githubusercontent.com/earendil-works/pi-website/main/README.md`                                             | 200    | `text/plain`       | 明文定义：`logo.svg` = _"Pi logo (white, for dark backgrounds)"_；`favicon.svg` = _"Pi logo with dark background (for browser tabs)"_                                                                                                                                                                                                                                      |
| 17  | `https://raw.githubusercontent.com/earendil-works/pi-website/main/src/favicon.svg`                                       | 200    | `image/svg+xml`    | 与 `https://pi.dev/favicon.svg` **字节级一致**（同 SHA-256）                                                                                                                                                                                                                                                                                                               |
| 18  | `https://raw.githubusercontent.com/earendil-works/pi-website/main/src/logo.svg`                                          | 200    | `image/svg+xml`    | 与 `https://pi.dev/logo.svg` **字节级一致**（同 SHA-256）                                                                                                                                                                                                                                                                                                                  |
| 19  | `https://github.com/earendil-works/pi` 仓库 `git/trees/main?recursive=1`（API）                                          | 200    | `application/json` | 全树 1921 项，**仓库内不含任何 logo/icon SVG**；仅 `clankolas.png`（TUI mascot）、doc 截图、`LICENSE`。品牌资产由站点仓库 `pi-website` 承载                                                                                                                                                                                                                                |
| 20  | `https://pi.dev/press-kit/screenshots.zip`                                                                               | 200    | `application/zip`  | 372004 bytes，仅含 `tree-view.png`、`doom-extension.png` **两张截图**（不是品牌文件包）                                                                                                                                                                                                                                                                                    |
| 21  | `https://www.npmjs.com/package/@earendil-works/pi-coding-agent`                                                          | 403    | `text/html`        | npm 网页对脚本抓取返回 403（反爬），非权限/许可信号；改由本机已装包 `package.json` 核验                                                                                                                                                                                                                                                                                    |
| 22  | 本机包 `~/.volta/tools/image/packages/@earendil-works/pi-coding-agent/lib/node_modules/@earendil-works/pi-coding-agent/` | local  | —                  | `package.json`: `"license": "MIT"`, `"repository.directory": "packages/coding-agent"`；包内 `find *.svg/*.png/*.ico` 仅命中 `dist/modes/interactive/assets/clankolas.png` 与 docs 截图，**npm 包未随附官方方形 logo**                                                                                                                                                      |

### 2.1 抓取到的原始 HTML 关键片段（证据留存）

- `pi.dev/` head：`<link rel="icon" type="image/svg+xml" href="/favicon.svg"/>`；`<meta property="og:image" content="https://pi.dev/social.png"/>`（宽 1200 高 630）。
- `pi.dev/press-kit` "Brand files" 原文（去标签）：
  - `Primary logo` / `SVG` / _"Vector logo for light/dark usage and editorial placements."_ → Download `/logo.svg`
  - `Badge` / `SVG` / _"Square mark for favicons and compact badges."_ → Download `/favicon.svg`
  - `Screenshots` … `Download all files as a ZIP bundle` → `/press-kit/screenshots.zip`
  - 页脚：`Earendil Inc. & Contributors` · `Press Kit` · `MIT License`
- `pi-website/README.md` 结构说明原文：
  - `logo.svg            Pi logo (white, for dark backgrounds)`
  - `favicon.svg         Pi logo with dark background (for browser tabs)`

---

## 3. 许可判定

| 项                             | 结论                                                                                                                                                                  | 证据                                                                                                                                        |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Pi 主项目                      | **MIT**                                                                                                                                                               | `earendil-works/pi` 仓库 `LICENSE`（_Copyright (c) 2025 Mario Zechner_）；GitHub API `spdx_id = MIT`；npm 包 `package.json.license = "MIT"` |
| pi.dev 站点 / 品牌资产所在仓库 | **MIT**                                                                                                                                                               | `earendil-works/pi-website` `LICENSE`（_Copyright (c) 2026 Earendil Inc. and contributors_）；GitHub API `spdx_id = MIT`                    |
| Press Kit 授权                 | 官方页面 _"Assets and contact for press coverage"_，页脚声明 **MIT License**，并为 Primary logo 与 Badge 提供显式下载链接                                             | 见证据 #2                                                                                                                                   |
| 资产归属                       | 该资产属 **Earendil Inc. / Pi 官方项目资产**，以 **MIT License** 随项目/站点源码分发；`favicon.svg` 与 `logo.svg` 同时存在于 pi.dev 与官方站点仓库 `src/`（字节一致） | 见证据 #17/#18                                                                                                                              |

**结论**：可安全 bundle 进 EasyToAgents。MIT 允许复制、再分发与随附使用，唯一义务是保留版权与许可声明（EasyToAgents 现有 brand README 已记录来源 URL，满足出处标注；如需更严格，可在 README 条目标注 `MIT License` 与版权方）。

**与现有 brand README 措辞规范的对齐**：现有条目按 `文件名: 来源描述, 来源 URL, retrieved <日期> and copied unchanged, SHA-256` 记录；官方资产且"copied unchanged"是默认要求。Pi 属于**官方资产 + MIT**，无需写"self-drawn placeholder"说明。

**未找到**独立的 Pi trademark/brand-usage 政策页面（`pi.dev` 全站链接中只有 `/press-kit` 一个品牌相关入口）；因此以 MIT + Press Kit 声明为准，见 §6 风险。

---

## 4. 候选资产清单（含 SHA-256 与本地路径）

候选文件已落盘至 `.trellis/tasks/09-14-add-pi-tool-support/research/assets/`（原始字节，未做任何优化/重绘）。

| 本地文件                            | 类型 / 尺寸                                                                                         | 字节  | SHA-256                                                            | 原始来源 URL                                                                       | 官方定位                                                                         |
| ----------------------------------- | --------------------------------------------------------------------------------------------------- | ----- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `pi-badge.svg`                      | SVG，`viewBox 0 0 800 800`，深色圆角底 `#09090b` + 白 mark                                          | 440   | `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a` | `https://pi.dev/favicon.svg`                                                       | **Press Kit "Badge"** — Square mark for favicons and compact badges              |
| `pi-badge.pi-website-repo.svg`      | 同上，字节级相同                                                                                    | 440   | `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a` | `https://raw.githubusercontent.com/earendil-works/pi-website/main/src/favicon.svg` | 站点仓库中的规范源（与 pi.dev 一致）                                             |
| `pi-logo-white.svg`                 | SVG，`viewBox 0 0 800 800`，纯白 mark，透明底                                                       | 473   | `2d43cb4f6a239ac70416214ac05842be34175339f761001fab3e8d01459bec69` | `https://pi.dev/logo.svg`                                                          | Press Kit "Primary logo"（white, for dark backgrounds）                          |
| `pi-logo-white.pi-website-repo.svg` | 同上，字节级相同                                                                                    | 473   | `2d43cb4f6a239ac70416214ac05842be34175339f761001fab3e8d01459bec69` | `https://raw.githubusercontent.com/earendil-works/pi-website/main/src/logo.svg`    | 站点仓库中的规范源                                                               |
| `pi-logo-auto.svg`                  | SVG，`viewBox 0 0 800 800`，透明底；`<style>` 内浅色 `#000` / 深色 `#fff`（`prefers-color-scheme`） | 618   | `03d509c104b9570063fa268fd3235ed7e0e41dafd93124ca94cae3726f58f117` | `https://pi.dev/logo-auto.svg`                                                     | 站点/README 使用；不在 Press Kit "Brand files" 列表内，但是官方 README 顶部 logo |
| `pi-social-og.png`                  | PNG，1200×630                                                                                       | 70639 | `e87032cc9d95ffdbe94bfa047b59a267f1521fdbe35acb5f92f8ea77df1308ea` | `https://pi.dev/social.png`                                                        | og:image 横幅，非方形 mark（仅信息性留存）                                       |

**补充事实**：

- `pi.dev/favicon.svg` 与 `pi-website/src/favicon.svg` **字节完全相同**；`pi.dev/logo.svg` 与 `pi-website/src/logo.svg` 亦完全相同（SHA-256 各自一致），说明 pi.dev 直接以站点仓库为单一来源。
- `pi.dev/social.png`（1200×630, 70639 B）与站点仓库 `src/social.png`（1586×1162, 175682 B）**不同**，说明部署时可能做了裁剪；不影响本次方形 mark 结论。
- 官方**没有** `.ico`、`apple-touch-icon.png`、webmanifest、PNG favicon：方形 mark 只有 SVG 一种形态。

### 4.1 官方 Mark 几何（两个变体共享）

`favicon.svg`（Badge）与 `logo.svg`（Primary）是同一 mark 的两种呈现：P 形外轮廓（含内孔，`fill-rule="evenodd"`）+ 右下 i 点方块 `M517.36 400 H634.72 V634.72 H517.36 Z`。差别只在背景：

- `favicon.svg` = `rect 800×800 rx=120 fill=#09090b` + 白色 mark（**方形 mark，自带深色底**）。
- `logo.svg` = 只有白色 mark，透明底（**深色背景专用**）。
- `logo-auto.svg` = 只有透明 mark，默认黑、深色模式白（主题自适应，但依赖 `prefers-color-scheme`）。

---

## 5. 明确回答：是否存在官方方形 mark？

**存在，且是官方 Press Kit 一等资产。** `https://pi.dev/favicon.svg`（Press Kit 名称 **"Badge"**），800×800 矢量、深色圆角方底 + 白色 mark，官方描述即 _"Square mark for favicons and compact badges"_。

对比其它候选：

| 候选                       | 分辨率        | 矢量 | 背景透明                    | 浅色背景可读                  | 深色主题可读                                                                     | 作 `*-icon.svg` 的评价                           |
| -------------------------- | ------------- | ---- | --------------------------- | ----------------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------ |
| **`favicon.svg`（Badge）** | 800×800 矢量  | ✅   | ❌（自带 `#09090b` 圆角底） | ✅ 白 mark 对比强             | ✅ 白 mark；深底可能与本应用深色 UI 融合，但同 `zcode-icon.svg` 先例             | **推荐**                                         |
| `logo.svg`（Primary，白）  | 800×800 矢量  | ✅   | ✅                          | ❌ 白 mark 在浅色背景上不可见 | ✅                                                                               | 不可单独作图标（官方标注 dark backgrounds only） |
| `logo-auto.svg`            | 800×800 矢量  | ✅   | ✅                          | ✅（默认黑）                  | ✅（媒体查询白），但**依赖 WebView 对 `<img>` 内 `prefers-color-scheme` 的支持** | 可行备选，见 §6                                  |
| `social.png`（og:image）   | 1200×630 位图 | ❌   | —                           | —                             | —                                                                                | 不适合（非方形、横幅、位图）                     |

**推荐 `favicon.svg` 的决定性理由**：

1. 官方 Press Kit 对其的定位就是"方形 mark"，用途完全匹配 @1 的 icon 场景。
2. 固定配色、无媒体查询、无外部字体/样式依赖 → 在所有渲染路径（Vite `<img>`、Tauri WebView）下结果**确定**，符合 brand README "copied unchanged / do not recolor"。
3. 自带深色底的构图与现有 `zcode-icon.svg` 同款式，UI 一致性最好；`claude-icon-square.svg` 也与"带底方形"同类。

---

## 6. 风险与降级方案

| 风险                                                | 说明                                                                                                                        | 降级方案                                                                                                                                                                                 |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **无独立 trademark/brand 政策页面**                 | pi.dev 仅有 `/press-kit`；未找到明确的商标使用条款。资产可用依据是 MIT 许可 + Press Kit 显式下载。                          | 在 `src/assets/brand/README.md` 条目中额外注明 `MIT License (Earendil Inc. / Mario Zechner)` 与来源 URL，保留出处与许可；不作为官方背书宣传。（当前无阻塞）                              |
| **深色 UI 下深色底 mark 融合**                      | Badge 底色 `#09090b` 在深色主题下与背景接近，仅白 mark 突出；但 `zcode-icon.svg` 已是同一模式并在同 UI 中在用。             | 若实测观感差：在渲染处加容器/描边，或改用 `logo-auto.svg`（透明、主题自适应）。**不建议**重绘或 recolor 官方文件。                                                                       |
| **`logo-auto.svg` 的主题自适应在 `<img>` 中不生效** | `prefers-color-scheme` 位于 SVG 内部 `<style>`；部分 WebView 对 img 内媒体查询支持不稳定，可能恒为黑色 → 深色主题下不可见。 | 保持首选 `favicon.svg`（固定配色，无此风险）。若确需透明底，必须先在 Tauri WebView 实测两主题，否则不用 `logo-auto.svg`。                                                                |
| **上游未来替换/改版 favicon**                       | 站点资产可能更新，导致与已 bundle 文件漂移。                                                                                | 已记录 2026-09-14 的 SHA-256；README 标注抓取日期。更新时按 brand README 流程重新抓取、比对哈希并改 README，而非运行时 CDN 引用。                                                        |
| **许可若被质疑不明确**                              | 极端情况下若要求"仅记录官方 URL 不落盘"。                                                                                   | 最小降级：不 bundle 文件，仅在 README 记录 `https://pi.dev/favicon.svg` + MIT + 抓取日期；或以同一几何**自绘占位**并注明（参考 `zcode-icon.svg` 先例）——但此降级**不必要**，MIT 已足够。 |

---

## 7. 实施步骤草稿（**本次不执行**，供实施阶段照做）

> 约束：`src/` 下任何写入都属于后续实施阶段（design.md §9 / implement.md 5.4）。本节仅为草稿。

### 7.1 落盘

1. 复制候选文件到目标（**字节级不变**，不要经编辑器保存/优化）：

   ```bash
   cp .trellis/tasks/09-14-add-pi-tool-support/research/assets/pi-badge.svg \
      src/assets/brand/pi-icon.svg
   ```

2. 校验哈希等于 `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a`：

   ```bash
   shasum -a 256 src/assets/brand/pi-icon.svg
   ```

   若哈希不一致，说明被工具改写过，必须回到原始来源 `https://pi.dev/favicon.svg` 重新下载。

### 7.2 `src/assets/brand/README.md` 追加条目（照既有格式）

在 `opencode-icon.svg` 条目之后、`zcode-icon.svg` 之前（或按现有顺序追加）插入：

```markdown
- `pi-icon.svg`: official Pi badge mark (square mark for favicons and
  compact badges), `https://pi.dev/press-kit`, asset
  `https://pi.dev/favicon.svg` (canonical source
  `https://github.com/earendil-works/pi-website/blob/main/src/favicon.svg`),
  retrieved 2026-09-14 and copied unchanged. Pi and its website are
  distributed under the MIT License.
  SHA-256: `a5624bc3b8cac94de75f6f13701eca2ad3ef67bbeba286c4af3f398806f0858a`.
```

> 措辞说明：现有条目用 "official <项目> <资产类型>, <URL>, retrieved <日期> and copied unchanged. SHA-256: `...`"。上面沿用该句式，并补 Press Kit 页面 URL（说明"官方对外提供该资产"）与 MIT 许可归属，符合 README "记录来源/许可" 的要求。

### 7.3 `src/lib/tool-metadata.ts` 引用

`src/lib/tool-metadata.ts` 现有 import 风格：

```ts
import claudeIconUrl from "@/assets/brand/claude-icon-square.svg";
import codexIconUrl from "@/assets/brand/codex-icon-light.png";
import cursorIconUrl from "@/assets/brand/cursor-icon.svg";
import zcodeIconUrl from "@/assets/brand/zcode-icon.svg";
import opencodeIconUrl from "@/assets/brand/opencode-icon.svg";
```

新增一行（按字母/现有排布插入即可）：

```ts
import piIconUrl from "@/assets/brand/pi-icon.svg";
```

并在 `TOOL_METADATA`（或等价映射）中为 `pi` 增加条目，`icon: piIconUrl`，`label: "Pi"`。**能力值必须来自 `@/bindings/commands` 的 `TOOL_CAPABILITIES`，不要手写。**

### 7.4 验收点（实施阶段自查）

- [ ] `src/assets/brand/pi-icon.svg` SHA-256 == `a5624bc3…`（未被优化器改写）。
- [ ] README 条目含：来源描述 + Press Kit URL + 资产 URL + 抓取日期 `2026-09-14` + SHA-256 + MIT 许可。
- [ ] `tool-metadata.ts` 中 `pi.icon === piIconUrl`，`label === "Pi"`。
- [ ] UI 在浅色与深色主题下均能辨识 Pi 图标（`size-4` / `size-5`, `object-contain`）。
- [ ] 未引入任何运行时 CDN 引用（仍为本地 `import`）。

---

## 8. 遗留问题 / 后续确认

1. **无独立商标政策页面** —— 已按 MIT + Press Kit 处理；若法务要求更严格，需向 `rfc@earendil.com`（Press Kit 页面给出的联系方式）确认商标使用。
2. **深色主题观感** —— 实施阶段应在真实 Tauri 窗口中按浅/深两主题截图确认 `pi-icon.svg` 可辨识度；若不可接受，优先容器描边，其次评估 `logo-auto.svg`（需先验证 WebView 媒体查询）。
3. **上游资产漂移** —— 已固化 2026-09-14 的 SHA-256；后续若 Pi 换 logo，需按 brand README 流程重新抓取并更新条目。
