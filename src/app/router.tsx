import { createHashRouter, RouterProvider } from "react-router-dom";

import { DashboardPage } from "@/features/dashboard/dashboard-page";

const router = createHashRouter([
  {
    path: "/",
    element: <DashboardPage />,
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
