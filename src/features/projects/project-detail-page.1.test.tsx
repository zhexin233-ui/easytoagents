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
  it("直接应用模式下无冲突项目预览跳过对话框立即 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    renderPage();
    // 预览自带 GIT_TRACKED 警告；警告不阻止直接应用，与对话框行为一致。
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 直接应用" }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: project.id,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(
      await screen.findByText(
        "项目原生配置已通过持久化预览应用并完成写后验证。",
      ),
    ).toBeVisible();
  });

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
    expect(await screen.findByText("○ 未纳管")).toBeVisible();
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
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
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
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(
      await screen.findByText(
        "项目原生配置已通过持久化预览应用并完成写后验证。",
      ),
    ).toBeVisible();
  });

  it("直接应用模式下冲突项目预览回退为人工确认且 Apply 禁用", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    const baseTarget = preview.targets[0];
    if (!baseTarget) throw new Error("预览 fixture 缺少目标");
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: {
        ...preview,
        targets: [
          {
            ...baseTarget,
            changeKind: "conflict",
            status: "external_owned_change",
          },
        ],
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 直接应用" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "应用这份预览" })).toBeDisabled();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });

  it("冲突项目预览展示不匹配条目并支持以当前内容重新接管", async () => {
    const baseTarget = preview.targets[0];
    if (!baseTarget) throw new Error("预览 fixture 缺少目标");
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: {
        ...preview,
        targets: [
          {
            ...baseTarget,
            changeKind: "conflict",
            status: "external_owned_change",
            warningCodes: ["MANAGED_ITEM_BASELINE_MISMATCH"],
            baselineMismatchedItems: ["项目 MCP"],
            readoptAvailable: true,
            errorCode: "CONFLICT",
          },
        ],
      },
    });
    vi.mocked(commands.readoptMcpTarget).mockResolvedValue({
      status: "ok",
      data: {
        targetPath: "/isolated/projects/detail/.mcp.json",
        updatedItemCount: 1,
        removedItemCount: 0,
      },
    });
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    expect(
      await screen.findByText("内容不一致的受管条目：项目 MCP"),
    ).toBeVisible();

    fireEvent.click(
      screen.getByRole("button", {
        name: "以当前内容重新接管 /isolated/projects/detail/.mcp.json",
      }),
    );
    await waitFor(() =>
      expect(commands.readoptMcpTarget).toHaveBeenCalledWith({
        tool: "claude",
        projectId: project.id,
      }),
    );
    expect(await screen.findByText(/请再次点击同步按钮完成写入/)).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
  });
});
