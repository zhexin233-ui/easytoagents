/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type DashboardSummaryDto } from "@/bindings/commands";
import { DashboardPage } from "@/features/dashboard/dashboard-page";

vi.mock("@/bindings/commands", () => ({
  commands: {
    getDashboardSummary: vi.fn(),
    listSnapshots: vi.fn(),
  },
}));

const summary: DashboardSummaryDto = {
  tools: [
    {
      tool: "claude",
      activeProviderName: "Claude 主渠道",
      activePromptName: "Claude 提示词",
      globalMcpCount: 2,
      globalSkillCount: 3,
    },
    {
      tool: "codex",
      activeProviderName: "Codex 主渠道",
      activePromptName: null,
      globalMcpCount: 1,
      globalSkillCount: 0,
    },
  ],
  projectCount: 4,
  conflictCount: 2,
  snapshotCount: 5,
  recentSyncRuns: [
    {
      id: "run-1",
      kind: "apply",
      status: "succeeded",
      scope: "global",
      projectId: null,
      startedAt: "2026-08-24T10:00:00Z",
      finishedAt: "2026-08-24T10:00:01Z",
      errorCode: null,
    },
  ],
  interruptedRun: null,
  needsOnboarding: false,
};

function renderDashboard() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <DashboardPage />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("DashboardPage", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.mocked(commands.getDashboardSummary).mockReset();
    vi.mocked(commands.listSnapshots).mockReset();
  });

  it("展示双工具、项目、冲突、快照与最近同步聚合", async () => {
    vi.mocked(commands.getDashboardSummary).mockResolvedValue({
      status: "ok",
      data: summary,
    });
    renderDashboard();

    expect(await screen.findByText("Claude 主渠道")).toBeInTheDocument();
    expect(screen.getByText("Codex 主渠道")).toBeInTheDocument();
    expect(screen.getByText("最近同步")).toBeInTheDocument();
    expect(screen.getByText("apply · global")).toBeInTheDocument();
    expect(screen.getByText("待处理冲突")).toBeInTheDocument();
  });

  it("空状态只提供首次检测这一项下一步", async () => {
    vi.mocked(commands.getDashboardSummary).mockResolvedValue({
      status: "ok",
      data: {
        ...summary,
        tools: summary.tools.map((tool) => ({
          ...tool,
          activeProviderName: null,
          activePromptName: null,
          globalMcpCount: 0,
          globalSkillCount: 0,
        })),
        projectCount: 0,
        conflictCount: 0,
        snapshotCount: 0,
        recentSyncRuns: [],
        needsOnboarding: true,
      },
    });
    renderDashboard();

    expect(await screen.findByText("尚未接管任何配置")).toBeInTheDocument();
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "开始首次检测" })).toBeEnabled();
  });
});
