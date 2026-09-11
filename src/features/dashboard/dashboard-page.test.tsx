import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type AppSettingsDto,
  type DashboardSummaryDto,
} from "@/bindings/commands";
import { DashboardPage } from "@/features/dashboard/dashboard-page";
import { renderWithProviders } from "@/test/render";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

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
    {
      tool: "cursor",
      activeProviderName: null,
      activePromptName: null,
      globalMcpCount: 4,
      globalSkillCount: 5,
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
  return renderWithProviders(<DashboardPage />);
}

describe("DashboardPage", () => {
  beforeEach(() => {
    vi.mocked(commands.getDashboardSummary).mockReset();
    vi.mocked(commands.listSnapshots).mockReset();
    vi.mocked(commands.getAppSettings).mockReset();
    vi.mocked(commands.getEnvironmentState).mockReset();
    vi.mocked(commands.getEnvironmentState).mockResolvedValue({
      status: "ok",
      data: { probing: false, tools: [] },
    });
    vi.mocked(commands.refreshEnvironment).mockReset();
    vi.mocked(commands.refreshEnvironment).mockResolvedValue({
      status: "ok",
      data: { probing: false, tools: [] },
    });
  });

  it("探测进行中时展示等待态并禁用重新检测", async () => {
    mockEnabledTools(["claude", "codex"]);
    vi.mocked(commands.getDashboardSummary).mockResolvedValue({
      status: "ok",
      data: summary,
    });
    vi.mocked(commands.getEnvironmentState).mockResolvedValue({
      status: "ok",
      data: { probing: true, tools: [] },
    });
    renderDashboard();
    const waiting = await screen.findByText(/正在检测本机工具安装状态/);
    expect(waiting).toHaveAttribute("role", "status");
    expect(
      screen.getByRole("button", { name: "正在检测工具…" }),
    ).toBeDisabled();
    expect(commands.refreshEnvironment).not.toHaveBeenCalled();
  });

  it("重新检测工具后刷新总览查询", async () => {
    mockEnabledTools(["claude", "codex"]);
    vi.mocked(commands.getDashboardSummary).mockResolvedValue({
      status: "ok",
      data: summary,
    });
    renderDashboard();
    await screen.findByText("Claude 主渠道");
    const summaryCalls = vi.mocked(commands.getDashboardSummary).mock.calls
      .length;
    fireEvent.click(
      await screen.findByRole("button", { name: "重新检测工具" }),
    );
    await waitFor(() =>
      expect(commands.refreshEnvironment).toHaveBeenCalledTimes(1),
    );
    await waitFor(() =>
      expect(
        vi.mocked(commands.getDashboardSummary).mock.calls.length,
      ).toBeGreaterThan(summaryCalls),
    );
  });

  function mockEnabledTools(enabledTools: AppSettingsDto["enabledTools"]) {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "preview_confirm", enabledTools },
    });
  }

  it("展示全部工具、Cursor 不支持能力、项目、冲突、快照与最近同步聚合", async () => {
    mockEnabledTools(["claude", "codex", "cursor"]);
    vi.mocked(commands.getDashboardSummary).mockResolvedValue({
      status: "ok",
      data: summary,
    });
    renderDashboard();

    expect(await screen.findByText("Claude 主渠道")).toBeInTheDocument();
    expect(screen.getByText("Codex 主渠道")).toBeInTheDocument();
    const cursorHeading = screen.getByRole("heading", { name: "Cursor" });
    const cursorCard = cursorHeading.closest("article");
    if (!cursorCard) throw new Error("未找到 Cursor 总览卡片");
    // Cursor Provider 仍不支持；提示词已按官方规则文件合同开放。
    expect(within(cursorCard).getAllByText("不支持")).toHaveLength(1);
    expect(within(cursorCard).getByText("未接管")).toBeInTheDocument();
    expect(
      within(cursorCard).getByRole("link", { name: "管理" }),
    ).toHaveAttribute("href", "/cursor");
    expect(screen.getByText("最近同步")).toBeInTheDocument();
    expect(screen.getByText("apply · global")).toBeInTheDocument();
    expect(screen.getByText("待处理冲突")).toBeInTheDocument();
  });

  it("被关闭的工具不再渲染总览卡片", async () => {
    mockEnabledTools(["claude", "codex"]);
    vi.mocked(commands.getDashboardSummary).mockResolvedValue({
      status: "ok",
      data: summary,
    });
    renderDashboard();

    expect(await screen.findByText("Claude 主渠道")).toBeInTheDocument();
    expect(screen.getByText("Codex 主渠道")).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Cursor" }),
    ).not.toBeInTheDocument();
  });

  it("空状态只提供首次检测这一项下一步", async () => {
    mockEnabledTools(["claude", "codex"]);
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
