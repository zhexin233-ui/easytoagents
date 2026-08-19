import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { DashboardPage } from "@/features/dashboard/dashboard-page";

vi.mock("@/bindings/commands", () => ({
  commands: {
    getAppInfo: vi.fn().mockResolvedValue({
      name: "EasyToAgents",
      version: "0.1.0",
    }),
  },
}));

describe("DashboardPage", () => {
  it("通过生成的命令客户端展示桌面后端信息", async () => {
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
      },
    });

    render(
      <QueryClientProvider client={queryClient}>
        <DashboardPage />
      </QueryClientProvider>,
    );

    expect(
      await screen.findByText("桌面后端已连接 · EasyToAgents 0.1.0"),
    ).toBeInTheDocument();
  });
});
