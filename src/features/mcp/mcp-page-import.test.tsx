import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type McpImportPreviewDto,
  type Tool,
} from "@/bindings/commands";
import { McpPage } from "@/features/mcp/mcp-page";
import { renderWithProviders } from "@/test/render";
import { makeMcpServer, makeMcpPreview } from "@/test/fixtures";
import { toolMetadata } from "@/lib/tool-metadata";

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

async function globalButton(name: string, tool: Tool = "claude") {
  const section = screen
    .getByRole("heading", { name: "全局目标状态" })
    .closest("section");
  if (!section) throw new Error("未找到全局目标状态");
  const card = (
    await within(section).findByText(toolMetadata(tool).label)
  ).closest("article");
  if (!card) throw new Error("未找到工具状态卡");
  return within(card).getByRole("button", { name });
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
  it("直接应用模式下删除已分配 MCP 自动同步清理并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    const assignedServer = makeMcpServer({
      ...server,
      globalTools: ["claude"],
    });
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [assignedServer],
    });
    vi.mocked(commands.deleteMcpServer).mockResolvedValue({
      status: "ok",
      data: { id: assignedServer.id, deleted: true },
    });
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "删除" }));
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: null,
      }),
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText(/已应用 1 个 MCP 目标/)).toBeVisible();
  });
  it("直接应用模式下自动同步空目标仅提示无需写入", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    const assignedServer = makeMcpServer({
      ...server,
      globalTools: ["claude"],
    });
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [assignedServer],
    });
    vi.mocked(commands.deleteMcpServer).mockResolvedValue({
      status: "ok",
      data: { id: assignedServer.id, deleted: true },
    });
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, targets: [] },
    });
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "删除" }));

    const status =
      await screen.findByText(/暂无启用且已分配到该工具的中央 MCP/);
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });
  it("直接应用模式下导入成功后自动同步导入工具", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    vi.mocked(commands.confirmMcpImport).mockResolvedValue({
      status: "ok",
      data: {
        tool: "claude",
        createdCount: 1,
        reusedCount: 0,
        assignedCount: 1,
      },
    });
    renderPage();
    fireEvent.click(await globalButton("检测并导入已有 MCP"));
    const dialog = await screen.findByRole("dialog", {
      name: "导入 Claude 全局 MCP",
    });
    const checkbox = await within(dialog).findByRole("checkbox", {
      name: "导入 native-new",
    });
    fireEvent.click(checkbox);
    fireEvent.click(
      within(dialog).getByRole("button", { name: "确认导入所选项（1）" }),
    );
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: null,
      }),
    );
    expect(await screen.findByText(/已应用 1 个 MCP 目标/)).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
  });
  it.each(["claude", "codex", "cursor"] as const)(
    "按 %s 扫描且只导入明确勾选项，成功后独立生成同步预览",
    async (tool) => {
      vi.mocked(commands.confirmMcpImport).mockResolvedValue({
        status: "ok",
        data: { tool, createdCount: 1, reusedCount: 0, assignedCount: 1 },
      });
      renderPage();
      const button = await globalButton("检测并导入已有 MCP", tool);
      expect(commands.discoverMcpImport).not.toHaveBeenCalled();
      fireEvent.click(button);
      const dialog = await screen.findByRole("dialog", {
        name: `导入 ${toolMetadata(tool).label} 全局 MCP`,
      });
      const checkbox = await within(dialog).findByRole("checkbox", {
        name: "导入 native-new",
      });
      expect(commands.discoverMcpImport).toHaveBeenCalledWith(tool);
      for (const item of within(dialog).getAllByRole("checkbox")) {
        expect(item).not.toBeChecked();
      }
      expect(
        within(dialog).getByRole("checkbox", { name: "导入 native-disabled" }),
      ).toBeDisabled();
      expect(
        within(dialog).getByRole("checkbox", { name: "导入 native-conflict" }),
      ).toBeDisabled();
      for (const name of ["native-invalid", "native-unsupported"]) {
        expect(
          within(dialog).getByRole("checkbox", { name: `导入 ${name}` }),
        ).toBeDisabled();
      }
      expect(
        within(dialog).getByText(
          "args 必须是字符串数组，不能为 null 或其它类型。",
        ),
      ).toBeVisible();
      expect(
        within(dialog).getByText(
          "env_http_headers 环境变量引用暂不能保真导入，原配置保持不变。",
        ),
      ).toBeVisible();
      expect(within(dialog).getByText(/复用相同配置的中央记录/)).toBeVisible();
      expect(within(dialog).getByText(/\[REDACTED\]/)).toBeVisible();
      expect(
        within(dialog).getByRole("button", { name: "确认导入所选项（0）" }),
      ).toBeDisabled();
      fireEvent.click(checkbox);
      const readsBeforeImport = vi.mocked(commands.listMcpServers).mock.calls
        .length;
      fireEvent.click(
        within(dialog).getByRole("button", { name: "确认导入所选项（1）" }),
      );
      await waitFor(() =>
        expect(commands.confirmMcpImport).toHaveBeenCalledWith({
          previewId: nativeImport.previewId,
          candidateIds: [newCandidateId],
        }),
      );
      const status =
        await screen.findByText(/原生配置未改写，请单独生成全局预览/);
      expect(status).toHaveAttribute("role", "status");
      expect(
        screen.getAllByText(/原生配置未改写，请单独生成全局预览/),
      ).toHaveLength(1);
      await waitFor(() =>
        expect(commands.listMcpServers).toHaveBeenCalledTimes(
          readsBeforeImport + 1,
        ),
      );
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(commands.createMcpServer).not.toHaveBeenCalled();
      expect(commands.previewMcpSync).not.toHaveBeenCalled();
      expect(commands.applyMcpPreview).not.toHaveBeenCalled();
      expect(commands.discoverMcpImport).toHaveBeenCalledTimes(1);
      fireEvent.click(await globalButton("生成全局预览", tool));
      expect(
        await screen.findByRole("dialog", { name: "确认原生配置变更" }),
      ).toBeVisible();
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool,
        projectId: null,
        excludeFromGit: false,
      });
    },
  );

  it("过期确认必须重新检测，并清空旧选择和旧 token", async () => {
    vi.mocked(commands.confirmMcpImport).mockResolvedValueOnce({
      status: "error",
      error: {
        code: "STALE_PREVIEW",
        message: "配置已变化",
        recoverable: true,
      },
    });
    renderPage();
    fireEvent.click(await globalButton("检测并导入已有 MCP"));
    fireEvent.click(
      await screen.findByRole("checkbox", { name: "导入 native-new" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "确认导入所选项（1）" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("STALE_PREVIEW");
    expect(
      screen.getByRole("button", { name: "确认导入所选项（1）" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "导入 native-new" }),
    ).toBeDisabled();
    const nextPreview = {
      ...nativeImport,
      previewId: "00000000-0000-4000-8000-000000000606",
    };
    vi.mocked(commands.discoverMcpImport).mockResolvedValueOnce({
      status: "ok",
      data: nextPreview,
    });
    fireEvent.click(screen.getByRole("button", { name: "重新检测" }));
    const checkbox = await screen.findByRole("checkbox", {
      name: "导入 native-reuse",
    });
    expect(checkbox).not.toBeChecked();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.click(checkbox);
    fireEvent.click(
      screen.getByRole("button", { name: "确认导入所选项（1）" }),
    );
    await waitFor(() =>
      expect(commands.confirmMcpImport).toHaveBeenLastCalledWith({
        previewId: nextPreview.previewId,
        candidateIds: [reusedCandidateId],
      }),
    );
    await screen.findByText(/原生配置未改写/);
  });
  it("扫描失败不伪装为空配置，显式重试后展示缺失说明", async () => {
    vi.mocked(commands.discoverMcpImport).mockResolvedValueOnce({
      status: "error",
      error: {
        code: "PARSE_ERROR",
        message: "原生配置无法解析",
        recoverable: true,
      },
    });
    renderPage();
    fireEvent.click(await globalButton("检测并导入已有 MCP"));
    expect(await screen.findByRole("alert")).toHaveTextContent("PARSE_ERROR");
    expect(
      screen.getByRole("button", { name: "确认导入所选项（0）" }),
    ).toBeDisabled();
    vi.mocked(commands.discoverMcpImport).mockResolvedValueOnce({
      status: "ok",
      data: {
        ...nativeImport,
        previewId: null,
        candidates: [],
        message: "未发现原生全局配置。",
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "重新检测" }));
    expect(await screen.findByText("未发现原生全局配置。")).toBeVisible();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "确认导入所选项（0）" }),
    ).toBeDisabled();
    expect(commands.confirmMcpImport).not.toHaveBeenCalled();
  });
  it("关闭在途扫描后重新打开不会复用旧响应，键盘焦点留在对话框并恢复入口", async () => {
    const pending =
      deferred<Awaited<ReturnType<typeof commands.discoverMcpImport>>>();
    vi.mocked(commands.discoverMcpImport).mockReturnValueOnce(pending.promise);
    renderPage();
    const trigger = await globalButton("检测并导入已有 MCP");
    trigger.focus();
    fireEvent.click(trigger);
    expect(await screen.findByText("正在检测已有全局 MCP…")).toBeVisible();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(trigger).toHaveFocus();
    fireEvent.click(trigger);
    await screen.findByRole("checkbox", { name: "导入 native-new" });
    await act(async () => {
      pending.resolve({
        status: "ok",
        data: {
          ...nativeImport,
          previewId: null,
          candidates: [],
          message: "旧响应",
        },
      });
      await pending.promise;
    });
    expect(screen.queryByText("旧响应")).not.toBeInTheDocument();
    expect(commands.discoverMcpImport).toHaveBeenCalledTimes(2);
    const dialog = screen.getByRole("dialog");
    dialog.focus();
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(
      screen.getByRole("checkbox", { name: "导入 native-new" }),
    ).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(trigger).toHaveFocus();
    expect(commands.confirmMcpImport).not.toHaveBeenCalled();
  });
  it("确认在途时阻止关闭和重复提交", async () => {
    const pending =
      deferred<Awaited<ReturnType<typeof commands.confirmMcpImport>>>();
    vi.mocked(commands.confirmMcpImport).mockReturnValueOnce(pending.promise);
    renderPage();
    fireEvent.click(await globalButton("检测并导入已有 MCP"));
    fireEvent.click(
      await screen.findByRole("checkbox", { name: "导入 native-new" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "确认导入所选项（1）" }),
    );
    expect(
      await screen.findByRole("button", { name: "正在导入…" }),
    ).toBeDisabled();
    const rescan = screen.getByRole("button", { name: "重新检测" });
    expect(rescan).toBeDisabled();
    expect(screen.getByRole("button", { name: "正在导入…" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "正在导入…" }));
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeVisible();
    expect(commands.confirmMcpImport).toHaveBeenCalledTimes(1);
    await act(async () => {
      pending.resolve({
        status: "ok",
        data: {
          tool: "claude",
          createdCount: 1,
          reusedCount: 0,
          assignedCount: 1,
        },
      });
      await pending.promise;
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });
});
