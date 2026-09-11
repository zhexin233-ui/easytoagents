import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "@/bindings/commands";
import { ProjectsPage } from "@/features/projects/projects-page";
import { renderWithProviders } from "@/test/render";
import { makeProject } from "@/test/fixtures/dtos";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const project = makeProject({
  id: "00000000-0000-4000-8000-000000000701",
  displayName: "隔离项目",
  rootPath: "/isolated/projects/fixture",
  pathStatus: "valid",
  gitStatus: "repository",
  codexTrustStatus: "trusted",
  claudePolicyStatus: "allowed",
  targets: [
    {
      tool: "claude",
      artifactKind: "mcp",
      targetPath: "/isolated/projects/fixture/.mcp.json",
      capability: "supported",
      policy: "allowed",
      trust: "not_required",
      status: "external_owned_change",
      diagnosticCode: "EXTERNAL_OWNED_CHANGE",
    },
  ],
  nativeResources: {
    active: 0,
    disabled: 0,
    missing: 0,
    conflict: 0,
  },
  lastScannedAt: "2026-08-24T10:00:00Z",
  rowVersion: 3,
});

function renderPage() {
  return renderWithProviders(<ProjectsPage />);
}

describe("ProjectsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [project],
    });
    vi.mocked(commands.registerProject).mockResolvedValue({
      status: "ok",
      data: project,
    });
    vi.mocked(commands.rescanProject).mockResolvedValue({
      status: "ok",
      data: project,
    });
    vi.mocked(commands.removeProject).mockResolvedValue({
      status: "ok",
      data: {
        id: project.id,
        removed: true,
        nativeConfigurationLeftUnmanaged: true,
      },
    });
  });

  it("展示规范路径、Git/trust/policy 和非颜色唯一的目标状态", async () => {
    renderPage();
    expect(await screen.findByText("隔离项目")).toBeInTheDocument();
    expect(screen.getByText("/isolated/projects/fixture")).toBeInTheDocument();
    expect(screen.getByText("external_owned_change")).toHaveClass("sr-only");
    expect(screen.getByText(/原生资源：启用 0 · 已禁用 0/)).toBeVisible();
    expect(screen.getByRole("link", { name: "打开详情" })).toHaveAttribute(
      "href",
      "/projects/00000000-0000-4000-8000-000000000701",
    );
  });

  it("登记只发送显示名与选择路径，不隐式调用任何 Apply", async () => {
    renderPage();
    await screen.findByText("隔离项目");
    fireEvent.change(screen.getByLabelText("项目目录"), {
      target: { value: "/isolated/projects/new-project" },
    });
    fireEvent.change(screen.getByLabelText("显示名称"), {
      target: { value: "新项目" },
    });
    fireEvent.click(screen.getByRole("button", { name: "登记项目" }));

    await waitFor(() =>
      expect(commands.registerProject).toHaveBeenCalledWith({
        rootPath: "/isolated/projects/new-project",
        displayName: "新项目",
      }),
    );
    expect(
      screen.getByText(
        /尚未对项目原生配置执行任何写入。原生资源：启用 0 · 已禁用 0/,
      ),
    ).toBeInTheDocument();
  });

  it("项目卡展示原生资源计数，存在已禁用资源时阻止移除登记", async () => {
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [
        {
          ...project,
          nativeResources: {
            active: 2,
            disabled: 1,
            missing: 0,
            conflict: 0,
          },
        },
      ],
    });
    renderPage();
    expect(await screen.findByText("隔离项目")).toBeInTheDocument();
    expect(screen.getByText(/原生资源：启用 2 · 已禁用 1/)).toBeVisible();
    expect(screen.getByText(/移除登记前须先恢复已禁用资源/)).toBeVisible();
    expect(screen.getByRole("button", { name: "移除登记" })).toBeDisabled();
    expect(commands.removeProject).not.toHaveBeenCalled();
  });
});
