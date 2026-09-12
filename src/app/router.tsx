import { lazy } from "react";
import { createHashRouter, RouterProvider } from "react-router-dom";

import { AppShell } from "@/app/app-shell";
import { TOOL_PROFILE_ROUTES } from "@/app/tool-profile-routes";

const DashboardPage = lazy(() =>
  import("@/features/dashboard/dashboard-page").then((module) => ({
    default: module.DashboardPage,
  })),
);
const HooksPage = lazy(() =>
  import("@/features/hooks/hooks-page").then((module) => ({
    default: module.HooksPage,
  })),
);
const McpPage = lazy(() =>
  import("@/features/mcp/mcp-page").then((module) => ({
    default: module.McpPage,
  })),
);
const PromptsPage = lazy(() =>
  import("@/features/prompts/prompts-page").then((module) => ({
    default: module.PromptsPage,
  })),
);
const ProjectDetailPage = lazy(() =>
  import("@/features/projects/detail/page").then((module) => ({
    default: module.ProjectDetailPage,
  })),
);
const ProjectsPage = lazy(() =>
  import("@/features/projects/projects-page").then((module) => ({
    default: module.ProjectsPage,
  })),
);
const SkillsPage = lazy(() =>
  import("@/features/skills/skills-page").then((module) => ({
    default: module.SkillsPage,
  })),
);

const router = createHashRouter([
  {
    path: "/",
    element: <AppShell />,
    children: [
      {
        index: true,
        element: <DashboardPage />,
      },
      ...TOOL_PROFILE_ROUTES.map(({ path, element }) => ({ path, element })),
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
