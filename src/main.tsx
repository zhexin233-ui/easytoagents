import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/app/app";
import { applyThemeFromStorage } from "@/components/use-theme";
import "@/styles.css";

// 渲染前同步挂 dark class：首帧即按持久化偏好/系统外观渲染，避免主题闪变
// （Tauri CSP 不允许 index.html 内联脚本）。
applyThemeFromStorage();

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("找不到应用挂载节点");
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
