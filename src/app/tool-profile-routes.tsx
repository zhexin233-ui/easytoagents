// 路由配置模块：导出供 router 与切换测试共用的常量，非组件文件，
// fast-refresh 不适用。
/* eslint-disable react-refresh/only-export-components */
import { lazy, type ReactElement } from "react";

const ToolProfilesPage = lazy(() =>
  import("@/features/tool-profiles/tool-profiles-page").then((module) => ({
    default: module.ToolProfilesPage,
  })),
);

// 各工具页签渲染同一组件类型且位于同一 Outlet 位置；key 让切换工具时整页
// 重挂载：导入预览、表单、错误提示与同步预览对话框都是页面局部 state，
// 复用实例会把上一个工具的状态带到新页签。导出供路由与切换测试共用同一份配置。
export interface ToolProfileRoute {
  path: string;
  element: ReactElement;
}

export const TOOL_PROFILE_ROUTES: ReadonlyArray<ToolProfileRoute> = [
  { path: "claude", element: <ToolProfilesPage key="claude" tool="claude" /> },
  { path: "codex", element: <ToolProfilesPage key="codex" tool="codex" /> },
  { path: "cursor", element: <ToolProfilesPage key="cursor" tool="cursor" /> },
  { path: "zcode", element: <ToolProfilesPage key="zcode" tool="zcode" /> },
  {
    path: "opencode",
    element: <ToolProfilesPage key="opencode" tool="opencode" />,
  },
];
