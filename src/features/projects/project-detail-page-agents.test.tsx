import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type AgentProjectOptionDto } from "@/bindings/commands";
import {
  project,
  renderPage,
  setupMocks,
} from "./project-detail-page.test-helpers";
import { makePreviewPlan } from "@/test/fixtures/preview-plan";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const options: AgentProjectOptionDto[] = [
  {
    agentId: "agent-inherited",
    name: "inherited-agent",
    enabled: true,
    state: "inherited",
    selectable: false,
    rowVersion: 2,
  },
  {
    agentId: "agent-project",
    name: "project-agent",
    enabled: true,
    state: "available",
    selectable: true,
    rowVersion: 4,
  },
];

describe("ProjectDetailPage Agents", () => {
  beforeEach(() => {
    setupMocks();
    vi.mocked(commands.listAgentProjectOptions).mockResolvedValue({
      status: "ok",
      data: options,
    });
    vi.mocked(commands.setProjectAgentAssignment).mockResolvedValue({
      status: "ok",
      data: {
        id: "agent-project",
        name: "project-agent",
        description: "项目 Agent",
        prompt: "检查项目。",
        enabled: true,
        globalAssignments: [],
        toolSettings: { claude: null, codex: null },
        rowVersion: 5,
      },
    });
    vi.mocked(commands.previewAgentSync).mockResolvedValue({
      status: "ok",
      data: makePreviewPlan({
        previewId: "agent-project-preview",
        scope: "project",
        projectId: project.id,
      }),
    });
    vi.mocked(commands.applyAgentPreview).mockResolvedValue({
      status: "ok",
      data: {
        runId: "agent-project-run",
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
  });

  it("显示项目 Agents、继承只读项并使用精确分配和预览 Apply 命令", async () => {
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Agents" }),
    );
    expect(
      await screen.findByRole("heading", { name: "Claude Agents 项目追加" }),
    ).toBeVisible();

    const inherited = await screen.findByRole("button", {
      name: "inherited-agent Agents 项目追加",
    });
    expect(inherited).toBeDisabled();
    expect(inherited).toHaveAttribute("aria-pressed", "true");
    const available = screen.getByRole("button", {
      name: "project-agent Agents 项目追加",
    });
    fireEvent.click(available);
    await waitFor(() =>
      expect(commands.setProjectAgentAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        agentId: "agent-project",
        assigned: true,
        agentRowVersion: 4,
        projectRowVersion: project.rowVersion,
      }),
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Claude Agents 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewAgentSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: project.id,
        excludeFromGit: false,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applyAgentPreview).toHaveBeenCalledWith({
        previewId: "agent-project-preview",
        tool: "claude",
        projectId: project.id,
      }),
    );
  });

  it("项目 Agents 工具切换不包含 ZCode", async () => {
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Agents" }),
    );
    const platformGroup = screen.getByRole("group", {
      name: "项目平台管理视图",
    });
    expect(
      within(platformGroup).queryByRole("button", {
        name: "管理 ZCode 项目资源",
      }),
    ).not.toBeInTheDocument();
  });
});
