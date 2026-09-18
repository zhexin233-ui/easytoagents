import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  commands,
  project,
  preview,
  mcpOptions,
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
  it("初始未纳管诊断呈现中性徽章与说明而非非受管变更警告", async () => {
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "ok",
      data: {
        ...project,
        targets: project.targets.map((target) =>
          target.tool === "codex" && target.artifactKind === "mcp"
            ? {
                ...target,
                status: "external_non_owned_change" as const,
                diagnosticCode: "PROJECT_TARGET_INITIAL_UNMANAGED",
              }
            : target,
        ),
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "展开工具配置状态" }),
    );
    expect(await screen.findByText("未纳管")).toBeVisible();
    expect(
      screen.getByText(
        "该目标由外部维护，本项目暂无需要写入的项目级配置；全局配置持续继承。",
      ),
    ).toBeVisible();
    expect(screen.queryByText(/非受管变更/)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/诊断：PROJECT_TARGET_INITIAL_UNMANAGED/),
    ).not.toBeInTheDocument();
  });

  it("直接应用模式下启用项目追加自动同步并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "codex"] },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "项目 MCP MCP 项目追加" }),
    );
    await waitFor(() =>
      expect(commands.setProjectMcpAssignment).toHaveBeenCalledWith({
        projectId: project.id,
        tool: "claude",
        mcpId: mcpOptions[1]?.mcpId,
        assigned: true,
        mcpRowVersion: mcpOptions[1]?.rowVersion,
        projectRowVersion: project.rowVersion,
      }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
    expect(
      await screen.findByText(
        "项目原生配置已通过持久化预览应用并完成写后验证。",
      ),
    ).toBeVisible();
  });
});
