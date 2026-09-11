import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type McpImportPreviewDto } from "@/bindings/commands";
import { centralListLayoutStorageKeys } from "@/components/use-persisted-central-list-layout";
import { McpPage } from "@/features/mcp/mcp-page";
import { renderWithProviders } from "@/test/render";
import { makeMcpServer, makeMcpPreview } from "@/test/fixtures";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const server = makeMcpServer();

const preview = makeMcpPreview();

const newCandidateId = "00000000-0000-4000-8000-000000000602";
const reusedCandidateId = "00000000-0000-4000-8000-000000000603";
const nativeImport: McpImportPreviewDto = {
  previewId: "00000000-0000-4000-8000-000000000601",
  tool: "claude",
  targetPath: "/isolated/home/.claude.json",
  message: null,
  candidates: [
    {
      candidateId: newCandidateId,
      name: "native-new",
      transport: "stdio",
      status: "importable",
      action: "create",
      reason: null,
      redactedProjection: { command: "npx", env: "[REDACTED]" },
    },
    {
      candidateId: reusedCandidateId,
      name: "native-reuse",
      transport: "streamable_http",
      status: "importable",
      action: "reuse",
      reason: null,
      redactedProjection: { url: "https://example.test/mcp" },
    },
    {
      candidateId: "00000000-0000-4000-8000-000000000604",
      name: "native-disabled",
      transport: null,
      status: "disabled",
      action: null,
      reason: "停用项不会自动启用。",
      redactedProjection: null,
    },
    {
      candidateId: "00000000-0000-4000-8000-000000000605",
      name: "native-conflict",
      transport: "stdio",
      status: "name_conflict",
      action: null,
      reason: "中央库存在不同配置的同名项。",
      redactedProjection: null,
    },
    {
      candidateId: "00000000-0000-4000-8000-000000000606",
      name: "native-invalid",
      transport: null,
      status: "invalid",
      action: null,
      reason: "args 必须是字符串数组，不能为 null 或其它类型。",
      redactedProjection: null,
    },
    {
      candidateId: "00000000-0000-4000-8000-000000000607",
      name: "native-unsupported",
      transport: null,
      status: "unsupported",
      action: null,
      reason: "env_http_headers 环境变量引用暂不能保真导入，原配置保持不变。",
      redactedProjection: null,
    },
  ],
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function renderPage() {
  return renderWithProviders(<McpPage />);
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(commands.listMcpServers).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.listMcpProjects).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.listMcpProjectOptions).mockResolvedValue({
    status: "ok",
    data: [],
  });
  vi.mocked(commands.listGlobalMcpTargetStatuses).mockResolvedValue({
    status: "ok",
    data: [
      {
        tool: "claude",
        projectId: null,
        targetPath: "/isolated/home/.claude.json",
        status: "missing",
        diagnosticCode: null,
      },
      {
        tool: "codex",
        projectId: null,
        targetPath: "/isolated/home/.codex/config.toml",
        status: "missing",
        diagnosticCode: null,
      },
      {
        tool: "cursor",
        projectId: null,
        targetPath: "/isolated/home/.cursor/mcp.json",
        status: "missing",
        diagnosticCode: null,
      },
    ],
  });
  vi.mocked(commands.getAppSettings).mockResolvedValue({
    status: "ok",
    data: {
      applyMode: "preview_confirm",
      enabledTools: ["claude", "codex", "cursor"],
    },
  });
  vi.mocked(commands.previewMcpSync).mockResolvedValue({
    status: "ok",
    data: preview,
  });
  vi.mocked(commands.applyMcpPreview).mockResolvedValue({
    status: "ok",
    data: {
      runId: preview.previewId,
      status: "succeeded",
      appliedTargets: 1,
      snapshotCount: 1,
    },
  });
  vi.mocked(commands.discoverMcpImport).mockImplementation((tool) =>
    Promise.resolve({
      status: "ok",
      data: {
        ...nativeImport,
        tool,
        targetPath:
          tool === "claude"
            ? "/isolated/home/.claude.json"
            : tool === "codex"
              ? "/isolated/home/.codex/config.toml"
              : "/isolated/home/.cursor/mcp.json",
      },
    }),
  );
  vi.mocked(commands.confirmMcpImport).mockResolvedValue({
    status: "ok",
    data: { tool: "claude", createdCount: 1, reusedCount: 0, assignedCount: 1 },
  });
});

describe("McpPage", () => {
  it("中央列表默认单列并可切换为响应式三列", async () => {
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [server],
    });
    renderPage();

    const section = screen
      .getByRole("heading", { name: "中央列表" })
      .closest("section");
    if (!section) throw new Error("未找到 MCP 中央列表");
    const card = await within(section).findByRole("heading", {
      name: server.name,
    });
    const article = card.closest("article");
    if (!article) throw new Error("未找到 MCP 中央卡片");
    const list = article.parentElement;
    if (!list) throw new Error("未找到 MCP 中央列表容器");
    const body = article.querySelector<HTMLElement>(
      '[data-slot="central-list-card-body"]',
    );
    const footer = article.querySelector<HTMLElement>(
      '[data-slot="central-list-card-actions"]',
    );
    if (!body || !footer) throw new Error("未找到 MCP 卡片主体或操作栏");
    const listButton = within(section).getByRole("button", {
      name: "单列显示",
    });
    const gridButton = within(section).getByRole("button", {
      name: "三列网格显示",
    });

    expect(listButton).toHaveAttribute("aria-pressed", "true");
    expect(gridButton).toHaveAttribute("aria-pressed", "false");
    expect(list).toHaveClass("space-y-3");
    expect(list).not.toHaveClass("grid");
    expect(article).toHaveAttribute("data-layout", "list");
    expect(within(article).getByText("入口")).toBeVisible();
    expect(within(article).getByText("敏感字段")).toBeVisible();
    expect(article.querySelector("pre")).toHaveTextContent("[REDACTED]");
    expect(
      within(footer).queryByRole("button", { name: "编辑" }),
    ).not.toBeInTheDocument();

    fireEvent.click(gridButton);
    expect(listButton).toHaveAttribute("aria-pressed", "false");
    expect(gridButton).toHaveAttribute("aria-pressed", "true");
    expect(list).toHaveClass(
      "grid",
      "auto-rows-fr",
      "items-stretch",
      "md:grid-cols-2",
      "lg:grid-cols-3",
    );
    expect(list).not.toHaveClass("space-y-3");
    expect(article).toHaveAttribute("data-layout", "grid");
    expect(article).toHaveClass(
      "flex",
      "h-full",
      "flex-col",
      "overflow-hidden",
    );
    expect(body).toHaveClass("flex", "flex-1", "flex-col", "p-4");
    expect(footer).toHaveClass("mt-auto", "border-t", "px-4", "py-3");
    expect(footer).toHaveAccessibleName(`${server.name} 操作`);
    expect(within(article).queryByText("入口")).not.toBeInTheDocument();
    expect(within(article).queryByText("敏感字段")).not.toBeInTheDocument();
    expect(article.querySelector("pre")).not.toBeInTheDocument();
    expect(within(body).getByText("入口摘要")).toBeVisible();
    expect(body).toHaveTextContent("扩展信息已脱敏");
    for (const name of ["编辑", "停用", "删除"]) {
      const button = within(footer).getByRole("button", { name });
      expect(button).toBeVisible();
      expect(button).toHaveAttribute("title", name);
      expect(button).toHaveClass("size-8", "p-0");
      expect(button.querySelector("svg")).toHaveAttribute(
        "aria-hidden",
        "true",
      );
    }
    expect(
      within(footer).getByRole("button", { name: "停用" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      within(footer).getByLabelText(`${server.name} 全局平台分配`),
    ).toHaveClass("ml-auto");
  });
  it("独立保存布局并在重新挂载后恢复，非法值回退为单列", () => {
    localStorage.setItem(centralListLayoutStorageKeys.skills, "grid");

    const firstRender = renderPage();
    const listButton = screen.getByRole("button", { name: "单列显示" });
    const gridButton = screen.getByRole("button", {
      name: "三列网格显示",
    });
    expect(listButton).toHaveAttribute("aria-pressed", "true");
    expect(gridButton).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(gridButton);
    expect(localStorage.getItem(centralListLayoutStorageKeys.mcp)).toBe("grid");
    expect(localStorage.getItem(centralListLayoutStorageKeys.skills)).toBe(
      "grid",
    );
    firstRender.unmount();

    const secondRender = renderPage();
    expect(
      screen.getByRole("button", { name: "三列网格显示" }),
    ).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "单列显示" }));
    expect(localStorage.getItem(centralListLayoutStorageKeys.mcp)).toBe("list");
    expect(localStorage.getItem(centralListLayoutStorageKeys.skills)).toBe(
      "grid",
    );
    secondRender.unmount();

    const thirdRender = renderPage();
    expect(screen.getByRole("button", { name: "单列显示" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    thirdRender.unmount();

    localStorage.setItem(centralListLayoutStorageKeys.mcp, "invalid-layout");
    renderPage();
    expect(screen.getByRole("button", { name: "单列显示" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
  it("存储读写失败时保持页面可用", () => {
    const getItemSpy = vi
      .spyOn(Storage.prototype, "getItem")
      .mockImplementation(() => {
        throw new Error("storage read blocked");
      });

    renderPage();
    expect(screen.getByRole("button", { name: "单列显示" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    getItemSpy.mockRestore();

    const setItemSpy = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(() => {
        throw new Error("storage write blocked");
      });
    fireEvent.click(screen.getByRole("button", { name: "三列网格显示" }));
    expect(
      screen.getByRole("button", { name: "三列网格显示" }),
    ).toHaveAttribute("aria-pressed", "true");
    setItemSpy.mockRestore();
  });
  it("平台图标暴露分配状态并保留原全局分配 payload", async () => {
    const assignedServer = makeMcpServer({
      ...server,
      globalTools: ["claude"],
    });
    const updatedServer = makeMcpServer({
      ...assignedServer,
      globalTools: ["claude", "codex"],
      rowVersion: assignedServer.rowVersion + 1,
    });
    const assignment =
      deferred<Awaited<ReturnType<typeof commands.setGlobalMcpAssignment>>>();
    vi.mocked(commands.listMcpServers)
      .mockResolvedValueOnce({ status: "ok", data: [assignedServer] })
      .mockResolvedValue({ status: "ok", data: [updatedServer] });
    vi.mocked(commands.setGlobalMcpAssignment).mockReturnValue(
      assignment.promise,
    );
    renderPage();

    const claudeButton = await screen.findByRole("button", {
      name: "Claude 全局已分配",
    });
    const codexButton = screen.getByRole("button", {
      name: "Codex 全局未分配",
    });
    const cursorButton = screen.getByRole("button", {
      name: "Cursor 全局未分配",
    });
    expect(claudeButton).toHaveAttribute("aria-pressed", "true");
    expect(claudeButton).toHaveAttribute("title", "Claude 全局已分配");
    expect(codexButton).toHaveAttribute("aria-pressed", "false");
    expect(codexButton).toHaveAttribute("title", "Codex 全局未分配");
    expect(cursorButton).toHaveAttribute("aria-pressed", "false");
    expect(cursorButton).toHaveAttribute("title", "Cursor 全局未分配");
    const claudeIcon = claudeButton.querySelector("img");
    const codexIcon = codexButton.querySelector("img");
    const cursorIcon = cursorButton.querySelector("img");
    expect(claudeIcon?.getAttribute("src")).toMatch(
      /^(data:image\/svg\+xml|.*claude-icon-square\.svg$)/,
    );
    expect(codexIcon?.getAttribute("src")).toMatch(
      /^(data:image\/png|.*codex-icon-light\.png$)/,
    );
    expect(cursorIcon?.getAttribute("src")).toMatch(
      /^(data:image\/svg\+xml|.*cursor-icon\.svg$)/,
    );
    expect(claudeButton.querySelector("svg")).toBeNull();
    expect(codexButton.querySelector("svg")).toBeNull();
    expect(claudeButton.firstElementChild).toHaveClass("opacity-100");
    expect(codexButton.firstElementChild).toHaveClass(
      "opacity-25",
      "grayscale",
    );
    expect(screen.queryByText("Claude 全局已分配")).not.toBeInTheDocument();

    fireEvent.click(codexButton);
    await waitFor(() =>
      expect(commands.setGlobalMcpAssignment).toHaveBeenCalledWith({
        tool: "codex",
        mcpId: server.id,
        assigned: true,
        rowVersion: server.rowVersion,
      }),
    );
    expect(claudeButton).toBeDisabled();
    expect(codexButton).toBeDisabled();

    await act(async () => {
      assignment.resolve({ status: "ok", data: updatedServer });
      await assignment.promise;
    });

    const updatedCodexButton = await screen.findByRole("button", {
      name: "Codex 全局已分配",
    });
    expect(updatedCodexButton).toHaveAttribute("aria-pressed", "true");
    expect(updatedCodexButton.firstElementChild).toHaveClass("opacity-100");
    await waitFor(() => {
      expect(commands.listMcpServers).toHaveBeenCalledTimes(2);
      expect(commands.listGlobalMcpTargetStatuses).toHaveBeenCalledTimes(2);
    });
    expect(commands.previewMcpSync).not.toHaveBeenCalled();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });
  it("被关闭的工具从平台图标列与状态卡中消失", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      },
    });
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [server],
    });
    renderPage();

    expect(
      await screen.findByRole("button", { name: "Claude 全局未分配" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Cursor 全局未分配" }),
    ).not.toBeInTheDocument();
    const statusSection = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    if (!statusSection) throw new Error("未找到全局目标状态");
    expect(await within(statusSection).findByText("Claude")).toBeVisible();
    expect(within(statusSection).queryByText("Cursor")).not.toBeInTheDocument();
  });
  it("删除图标按钮保留版本化删除 payload", async () => {
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [server],
    });
    vi.mocked(commands.deleteMcpServer).mockResolvedValue({
      status: "ok",
      data: { id: server.id, deleted: true },
    });
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "删除" }));

    await waitFor(() =>
      expect(commands.deleteMcpServer).toHaveBeenCalledWith({
        id: server.id,
        rowVersion: server.rowVersion,
      }),
    );
    const status = await screen.findByText(
      "中央 MCP 已删除；仍需预览并 Apply 才会安全清理旧受管条目。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(
        "中央 MCP 已删除；仍需预览并 Apply 才会安全清理旧受管条目。",
      ),
    ).toHaveLength(1);
  });
  it("删除失败只显示一次错误通知", async () => {
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [server],
    });
    vi.mocked(commands.deleteMcpServer).mockResolvedValue({
      status: "error",
      error: {
        code: "CONFLICT",
        message: "MCP 已变化",
        recoverable: true,
      },
    });
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "删除" }));

    const alert = await screen.findByText("CONFLICT：MCP 已变化");
    expect(alert).toHaveAttribute("role", "alert");
    expect(alert).toHaveAttribute("aria-atomic", "true");
    expect(screen.getAllByText("CONFLICT：MCP 已变化")).toHaveLength(1);
  });
  it("直接应用模式下分配切换自动同步并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    const assignedServer = makeMcpServer({
      ...server,
      globalTools: ["claude"],
    });
    const updatedServer = makeMcpServer({
      ...assignedServer,
      globalTools: ["claude", "codex"],
      rowVersion: assignedServer.rowVersion + 1,
    });
    vi.mocked(commands.listMcpServers)
      .mockResolvedValueOnce({ status: "ok", data: [assignedServer] })
      .mockResolvedValue({ status: "ok", data: [updatedServer] });
    vi.mocked(commands.setGlobalMcpAssignment).mockResolvedValue({
      status: "ok",
      data: updatedServer,
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "Codex 全局未分配" }),
    );
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "codex",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "codex",
        projectId: null,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText(/已应用 1 个 MCP 目标/)).toBeVisible();
  });
});
