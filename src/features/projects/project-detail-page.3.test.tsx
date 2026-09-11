import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectDto } from "@/bindings/commands";
import {
  commands,
  project,
  preview,
  mcpOptions,
  skillOptions,
  skillPreview,
  nativePreview,
  nativeResource,
  renderPage,
  setupMocks,
} from "./project-detail-page.test-helpers";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

describe("ProjectDetailPage", () => {
  beforeEach(setupMocks);
  it("MCP 继承保持只读，项目追加刷新三组查询并用持久化预览 Apply", async () => {
    vi.mocked(commands.getProject)
      .mockResolvedValueOnce({ status: "ok", data: project })
      .mockResolvedValue({
        status: "ok",
        data: { ...project, rowVersion: project.rowVersion + 1 },
      });
    const { client } = renderPage();
    const invalidateQueries = vi.spyOn(client, "invalidateQueries");
    const inherited = await screen.findByRole("button", {
      name: "全局 MCP MCP 项目追加",
    });
    const available = screen.getByRole("button", {
      name: "项目 MCP MCP 项目追加",
    });
    expect(inherited).toBeDisabled();
    expect(inherited).toHaveAttribute("aria-pressed", "true");
    expect(inherited).toHaveTextContent("禁用");
    expect(available).toBeEnabled();
    expect(available).toHaveAttribute("aria-pressed", "false");
    expect(available).toHaveTextContent("启用");
    expect(screen.getByText("全局继承")).toBeVisible();
    expect(screen.getByText("只读")).toBeVisible();
    expect(screen.getByText("可追加")).toBeVisible();

    fireEvent.click(available);
    await waitFor(() =>
      expect(commands.setProjectMcpAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        mcpId: mcpOptions[1]?.mcpId,
        assigned: true,
        mcpRowVersion: 4,
        projectRowVersion: 7,
      }),
    );
    await waitFor(() => expect(commands.getProject).toHaveBeenCalledTimes(2));
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["projects"],
    });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["mcp"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["skills"] });
    await waitFor(() =>
      expect(
        client.getQueryData<ProjectDto>(["projects", "detail", project.id])
          ?.rowVersion,
      ).toBe(8),
    );
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(2),
    );

    vi.mocked(commands.setProjectMcpAssignment).mockClear();
    fireEvent.click(
      screen.getByRole("button", { name: "项目 MCP MCP 项目追加" }),
    );
    await waitFor(() =>
      expect(commands.setProjectMcpAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        mcpId: mcpOptions[1]?.mcpId,
        assigned: true,
        mcpRowVersion: 4,
        projectRowVersion: 8,
      }),
    );
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();

    fireEvent.click(
      screen.getByRole("button", { name: "Claude MCP 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: project.id,
        excludeFromGit: false,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
  });

  it("Skill 继承保持只读，项目追加支持 Git exclude、预览与显式 Apply", async () => {
    vi.mocked(commands.getProject)
      .mockResolvedValueOnce({ status: "ok", data: project })
      .mockResolvedValue({
        status: "ok",
        data: { ...project, rowVersion: project.rowVersion + 1 },
      });
    const { client } = renderPage();
    const invalidateQueries = vi.spyOn(client, "invalidateQueries");
    fireEvent.click(
      await screen.findByRole("button", { name: "管理项目 Skill" }),
    );

    const inherited = await screen.findByRole("button", {
      name: "全局 Skill Skill 项目追加",
    });
    const available = screen.getByRole("button", {
      name: "项目 Skill Skill 项目追加",
    });
    expect(inherited).toBeDisabled();
    expect(inherited).toHaveAttribute("aria-pressed", "true");
    expect(inherited).toHaveTextContent("禁用");
    expect(available).toBeEnabled();
    expect(available).toHaveAttribute("aria-pressed", "false");
    expect(available).toHaveTextContent("启用");
    expect(screen.getByText("全局继承")).toBeVisible();
    expect(screen.getByText("只读")).toBeVisible();
    expect(screen.getByText("可追加")).toBeVisible();

    fireEvent.click(available);
    await waitFor(() =>
      expect(commands.setProjectSkillAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        skillId: skillOptions[1]?.skillId,
        assigned: true,
        skillRowVersion: 6,
        projectRowVersion: 7,
      }),
    );
    await waitFor(() => expect(commands.getProject).toHaveBeenCalledTimes(2));
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["projects"],
    });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["mcp"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["skills"] });
    await waitFor(() =>
      expect(
        client.getQueryData<ProjectDto>(["projects", "detail", project.id])
          ?.rowVersion,
      ).toBe(8),
    );
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(2),
    );

    vi.mocked(commands.setProjectSkillAssignment).mockClear();
    fireEvent.click(
      screen.getByRole("button", { name: "项目 Skill Skill 项目追加" }),
    );
    await waitFor(() =>
      expect(commands.setProjectSkillAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        skillId: skillOptions[1]?.skillId,
        assigned: true,
        skillRowVersion: 6,
        projectRowVersion: 8,
      }),
    );
    expect(commands.applySkillPreview).not.toHaveBeenCalled();

    const claudeSection = screen
      .getByRole("heading", { name: "Claude Skill 项目追加" })
      .closest("section");
    if (!claudeSection) throw new Error("未找到 Claude 项目管理列");
    fireEvent.click(
      within(claudeSection).getByRole("checkbox", {
        name: /若目标是应用新建且未跟踪/,
      }),
    );
    fireEvent.click(
      within(claudeSection).getByRole("button", {
        name: "Claude Skills 同步预览",
      }),
    );
    await waitFor(() =>
      expect(commands.previewSkillSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: project.id,
        excludeFromGit: true,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledWith({
        previewId: skillPreview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
  });

  it("已追加与异常状态以 tag 展示且按钮显示禁用", async () => {
    vi.mocked(commands.listMcpProjectOptions).mockResolvedValue({
      status: "ok",
      data: [
        {
          mcpId: "00000000-0000-4000-8000-000000000722",
          name: "已追加 MCP",
          enabled: false,
          state: "selected",
          selectable: true,
          rowVersion: 9,
        },
      ],
    });
    vi.mocked(commands.listSkillProjectOptions).mockResolvedValue({
      status: "ok",
      data: [
        {
          skillId: "00000000-0000-4000-8000-000000000723",
          name: "异常 Skill",
          status: "invalid",
          state: "selected",
          selectable: true,
          rowVersion: 10,
        },
      ],
    });
    renderPage();

    const mcpButton = await screen.findByRole("button", {
      name: "已追加 MCP MCP 项目追加",
    });
    expect(mcpButton).toBeEnabled();
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(mcpButton).toHaveTextContent("禁用");
    expect(screen.getByText("项目追加")).toBeVisible();
    expect(screen.getByText("已停用")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    const skillButton = await screen.findByRole("button", {
      name: "异常 Skill Skill 项目追加",
    });
    expect(skillButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveTextContent("禁用");
    expect(screen.getByText("项目追加")).toBeVisible();
    expect(screen.getByText("invalid")).toBeVisible();
  });

  it("MCP 与 Skill 空目标预览只解释无需写入且不开放 Apply", async () => {
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, targets: [] },
    });
    vi.mocked(commands.previewSkillSync).mockResolvedValue({
      status: "ok",
      data: { ...skillPreview, targets: [] },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    expect(
      await screen.findByText(
        "该项目只有全局继承 MCP，不需要创建项目配置文件。",
      ),
    ).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude Skills 同步预览" }),
    );
    expect(
      await screen.findByText(
        "该项目只有全局继承 Skills，不需要创建项目链接目录。",
      ),
    ).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
    expect(commands.applySkillPreview).not.toHaveBeenCalled();
  });

  it("在 MCP 与 Skill 视图中都保留项目目标阻断", async () => {
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "ok",
      data: { ...project, codexTrustStatus: "untrusted" },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "管理 Codex 项目资源" }),
    );
    expect(
      await screen.findByRole("button", { name: "Codex MCP 同步预览" }),
    ).toBeDisabled();
    expect(screen.getByText(/Codex 项目尚未受信任/)).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    expect(
      await screen.findByRole("button", { name: "Codex Skills 同步预览" }),
    ).toBeDisabled();
    expect(screen.getByText(/Codex 项目尚未受信任/)).toBeVisible();
    expect(commands.previewMcpSync).not.toHaveBeenCalled();
    expect(commands.previewSkillSync).not.toHaveBeenCalled();
  });

  it("空的项目原生资源分区位于中央追加之上，且不展示敏感配置", async () => {
    renderPage();
    expect(
      await screen.findByRole("heading", { name: "项目原生资源" }),
    ).toBeVisible();
    expect(await screen.findByText("当前组合没有项目原生资源")).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "Claude MCP 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listProjectNativeResources).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        artifactKind: "mcp",
      }),
    );
  });

  it("直接应用模式下禁用原生资源仍只打开预览，确认后才 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    vi.mocked(commands.listProjectNativeResources).mockResolvedValue({
      status: "ok",
      data: [nativeResource],
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "临时禁用 native-stdio" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(
      screen.getByText("PROJECT_NATIVE_RESOURCE_REQUIRES_CONFIRMATION"),
    ).toBeVisible();
    expect(commands.applyProjectNativeResourcePreview).not.toHaveBeenCalled();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyProjectNativeResourcePreview).toHaveBeenCalledWith({
        previewId: nativePreview.previewId,
      }),
    );
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
    expect(screen.queryByText("sk-native-secret")).not.toBeInTheDocument();
  });

  it("rollback_failed 时全局阻断原生资源操作", async () => {
    vi.mocked(commands.getInterruptedRun).mockResolvedValue({
      status: "ok",
      data: {
        runId: "00000000-0000-4000-8000-000000000750",
        status: "rollback_failed",
        journalAvailable: true,
        targets: [],
      },
    });
    vi.mocked(commands.listProjectNativeResources).mockResolvedValue({
      status: "ok",
      data: [nativeResource],
    });
    renderPage();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "存在未完成的写入或回滚失败",
    );
    expect(
      await screen.findByRole("button", { name: "临时禁用 native-stdio" }),
    ).toBeDisabled();
  });

  it("已禁用原生资源提供恢复入口且不自动 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    vi.mocked(commands.listProjectNativeResources).mockResolvedValue({
      status: "ok",
      data: [
        {
          ...nativeResource,
          state: "disabled",
          canDisable: false,
          canRestore: true,
          disabledAt: "2026-09-03T03:00:00Z",
        },
      ],
    });
    renderPage();
    expect(await screen.findByText("项目原生 · 已禁用")).toBeVisible();
    fireEvent.click(
      await screen.findByRole("button", { name: "恢复 native-stdio" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(commands.applyProjectNativeResourcePreview).not.toHaveBeenCalled();
  });
});
