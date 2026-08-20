import { createHashRouter, RouterProvider } from "react-router-dom";

import { AppShell } from "@/app/app-shell";
import { DashboardPage } from "@/features/dashboard/dashboard-page";
import { McpPage } from "@/features/mcp/mcp-page";
import { ToolProfilesPage } from "@/features/tool-profiles/tool-profiles-page";

const router = createHashRouter([
  {
    path: "/",
    element: <AppShell />,
    children: [
      {
        index: true,
        element: <DashboardPage />,
      },
      {
        path: "claude",
        element: <ToolProfilesPage tool="claude" />,
      },
      {
        path: "codex",
        element: <ToolProfilesPage tool="codex" />,
      },
      {
        path: "mcp",
        element: <McpPage />,
      },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
