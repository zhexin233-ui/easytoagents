/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type ProjectDto } from "@/bindings/commands";
import { ProjectsPage } from "@/features/projects/projects-page";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@/bindings/commands", () => ({
  commands: {
    listProjects: vi.fn(),
    registerProject: vi.fn(),
    rescanProject: vi.fn(),
    removeProject: vi.fn(),
  },
}));

const project: ProjectDto = {
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
  lastScannedAt: "2026-08-24T10:00:00Z",
  rowVersion: 3,
};

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <ProjectsPage />
      </QueryClientProvider>
    </MemoryRouter>,
  );
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

  afterEach(cleanup);

  it("展示规范路径、Git/trust/policy 和非颜色唯一的目标状态", async () => {
    renderPage();
    expect(await screen.findByText("隔离项目")).toBeInTheDocument();
    expect(screen.getByText("/isolated/projects/fixture")).toBeInTheDocument();
    expect(screen.getByText("external_owned_change")).toHaveClass("sr-only");
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
      screen.getByText(/尚未对项目原生配置执行任何写入/),
    ).toBeInTheDocument();
  });
});
