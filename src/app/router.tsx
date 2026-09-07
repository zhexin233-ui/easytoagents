import { createHashRouter, RouterProvider } from "react-router-dom";

import { AppShell } from "@/app/app-shell";
import { DashboardPage } from "@/features/dashboard/dashboard-page";
import { HooksPage } from "@/features/hooks/hooks-page";
import { McpPage } from "@/features/mcp/mcp-page";
import { PromptsPage } from "@/features/prompts/prompts-page";
import { ProjectDetailPage } from "@/features/projects/project-detail-page";
import { ProjectsPage } from "@/features/projects/projects-page";
import { SkillsPage } from "@/features/skills/skills-page";
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
        path: "cursor",
        element: <ToolProfilesPage tool="cursor" />,
      },
      {
        path: "zcode",
        element: <ToolProfilesPage tool="zcode" />,
      },
      {
        path: "opencode",
        element: <ToolProfilesPage tool="opencode" />,
      },
      {
        path: "mcp",
        element: <McpPage />,
      },
      {
        path: "hooks",
        element: <HooksPage />,
      },
      {
        path: "skills",
        element: <SkillsPage />,
      },
      {
        path: "prompts",
        element: <PromptsPage />,
      },
      {
        path: "projects",
        element: <ProjectsPage />,
      },
      {
        path: "projects/:projectId",
        element: <ProjectDetailPage />,
      },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
