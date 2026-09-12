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
  it("打开脱敏持久化预览并用原 previewId Apply", async () => {
    renderPage();
    const statusSection = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const claudeCard = statusSection
      ? (await within(statusSection).findByText("Claude")).closest("article")
      : null;
    if (!claudeCard) throw new Error("未找到 Claude 状态卡");
    expect(within(claudeCard).getByText("待初始化")).toHaveClass("bg-amber-50");
    expect(
      within(claudeCard).getByText(
        "尚未写入受管目标；生成预览会在确认后初始化。",
      ),
    ).toBeVisible();
    const previewButton = within(claudeCard).getByRole("button", {
      name: "生成全局预览",
    });
    expect(previewButton).toBeEnabled();
    fireEvent.click(previewButton);
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(screen.getAllByText(/\[REDACTED\]/).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "claude",
        projectId: null,
      }),
    );
  });
  it("直接应用模式下全局目标状态卡隐藏手动同步按钮", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    renderPage();
    expect(await globalButton("检测并导入已有 MCP")).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "直接应用全局同步" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "生成全局预览" }),
    ).not.toBeInTheDocument();
    expect(commands.previewMcpSync).not.toHaveBeenCalled();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });
  it("直接应用模式下自动同步的预览与 Apply 失败通知按队列堆叠", async () => {
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
    vi.mocked(commands.setMcpEnabled).mockResolvedValue({
      status: "ok",
      data: { ...assignedServer, enabled: false },
    });
    vi.mocked(commands.deleteMcpServer).mockResolvedValue({
      status: "ok",
      data: { id: assignedServer.id, deleted: true },
    });
    vi.mocked(commands.previewMcpSync)
      .mockResolvedValueOnce({
        status: "error",
        error: {
          code: "DATABASE_ERROR",
          message: "MCP 预览暂不可用",
          recoverable: true,
        },
      })
      .mockResolvedValue({ status: "ok", data: preview });
    vi.mocked(commands.applyMcpPreview).mockResolvedValue({
      status: "error",
      error: {
        code: "ATOMIC_WRITE_FAILED",
        message: "MCP 应用失败",
        recoverable: true,
      },
    });
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "停用" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "DATABASE_ERROR：MCP 预览暂不可用",
    );
    expect(
      screen.getAllByText("DATABASE_ERROR：MCP 预览暂不可用"),
    ).toHaveLength(1);

    fireEvent.click(await screen.findByRole("button", { name: "删除" }));
    await waitFor(() =>
      expect(
        screen.getByText("ATOMIC_WRITE_FAILED：MCP 应用失败"),
      ).toHaveAttribute("role", "alert"),
    );
    expect(
      screen.getAllByText("ATOMIC_WRITE_FAILED：MCP 应用失败"),
    ).toHaveLength(1);
  });
  it("直接应用模式下编辑已分配 MCP 保存后自动同步并 Apply", async () => {
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
    vi.mocked(commands.updateMcpServer).mockResolvedValue({
      status: "ok",
      data: assignedServer,
    });
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "编辑" }));
    fireEvent.click(screen.getByRole("button", { name: "保存中央意图" }));
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
  it("直接应用模式下保存触发同步遇冲突预览回退为人工确认且 Apply 禁用", async () => {
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
    vi.mocked(commands.updateMcpServer).mockResolvedValue({
      status: "ok",
      data: assignedServer,
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

    fireEvent.click(await screen.findByRole("button", { name: "编辑" }));
    fireEvent.click(screen.getByRole("button", { name: "保存中央意图" }));
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "应用这份预览" })).toBeDisabled();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });
  it("冲突预览展示不匹配条目并支持以当前内容重新接管", async () => {
    const refresh =
      deferred<Awaited<ReturnType<typeof commands.listMcpServers>>>();
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
            baselineMismatchedItems: ["node_repl"],
            readoptAvailable: true,
            errorCode: "CONFLICT",
          },
        ],
      },
    });
    vi.mocked(commands.readoptMcpTarget).mockResolvedValue({
      status: "ok",
      data: {
        targetPath: "/isolated/home/.claude.json",
        updatedItemCount: 1,
        removedItemCount: 0,
      },
    });
    renderPage();
    fireEvent.click(await globalButton("生成全局预览"));
    expect(
      await screen.findByText("内容不一致的受管条目：node_repl"),
    ).toBeVisible();
    vi.mocked(commands.listMcpServers).mockReturnValueOnce(refresh.promise);

    fireEvent.click(
      screen.getByRole("button", {
        name: "以当前内容重新接管 /isolated/home/.claude.json",
      }),
    );
    await waitFor(() =>
      expect(commands.readoptMcpTarget).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
      }),
    );
    expect(commands.previewMcpSync).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByText(
        "已以当前内容重新接管（刷新 1 个、清理 0 个条目基线）；正在重新生成预览。",
      ),
    ).not.toBeInTheDocument();
    await act(async () => {
      refresh.resolve({ status: "ok", data: [] });
      await refresh.promise;
    });
    // 接管后自动重新生成预览。
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledTimes(2),
    );
    const status = await screen.findByText(
      "已以当前内容重新接管（刷新 1 个、清理 0 个条目基线）；正在重新生成预览。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(
        "已以当前内容重新接管（刷新 1 个、清理 0 个条目基线）；正在重新生成预览。",
      ),
    ).toHaveLength(1);
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
  });
  it("重新接管后的预览失败与成功通知按队列呈现且不重复", async () => {
    const baseTarget = preview.targets[0];
    if (!baseTarget) throw new Error("预览 fixture 缺少目标");
    vi.mocked(commands.previewMcpSync)
      .mockResolvedValueOnce({
        status: "ok",
        data: {
          ...preview,
          targets: [
            {
              ...baseTarget,
              changeKind: "conflict",
              status: "external_owned_change",
              readoptAvailable: true,
              errorCode: "CONFLICT",
            },
          ],
        },
      })
      .mockResolvedValueOnce({
        status: "error",
        error: {
          code: "DATABASE_ERROR",
          message: "重新生成预览失败",
          recoverable: true,
        },
      });
    vi.mocked(commands.readoptMcpTarget).mockResolvedValue({
      status: "ok",
      data: {
        targetPath: "/isolated/home/.claude.json",
        updatedItemCount: 2,
        removedItemCount: 1,
      },
    });
    renderPage();

    fireEvent.click(await globalButton("生成全局预览"));
    fireEvent.click(
      await screen.findByRole("button", {
        name: "以当前内容重新接管 /isolated/home/.claude.json",
      }),
    );

    const alert = await screen.findByText("DATABASE_ERROR：重新生成预览失败");
    expect(alert).toHaveAttribute("role", "alert");
    expect(alert).toHaveAttribute("aria-atomic", "true");
    expect(
      screen.getAllByText("DATABASE_ERROR：重新生成预览失败"),
    ).toHaveLength(1);
    const readoptStatus = screen.getByText(
      "已以当前内容重新接管（刷新 2 个、清理 1 个条目基线）；正在重新生成预览。",
    );
    expect(readoptStatus).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(
        "已以当前内容重新接管（刷新 2 个、清理 1 个条目基线）；正在重新生成预览。",
      ),
    ).toHaveLength(1);
  });
  it.each([
    [
      "CLAUDE_POLICY_UNKNOWN",
      "策略状态待确认",
      "无法确认 Claude 管理策略，预览已阻止。",
      "bg-amber-50",
    ],
    [
      "CLAUDE_POLICY_BLOCKED",
      "策略阻止",
      "Claude 管理策略禁止该类自定义目标。",
      "bg-red-50",
    ],
  ] as const)(
    "区分全局策略诊断 %s 的提示、色调和阻断操作",
    async (diagnosticCode, label, description, toneClass) => {
      vi.mocked(commands.listGlobalMcpTargetStatuses).mockResolvedValue({
        status: "ok",
        data: [
          {
            tool: "claude",
            projectId: null,
            targetPath: "/isolated/home/.claude.json",
            status: "policy_blocked",
            diagnosticCode,
          },
        ],
      });
      renderPage();
      const statusSection = screen
        .getByRole("heading", { name: "全局目标状态" })
        .closest("section");
      const claudeCard = statusSection
        ? (await within(statusSection).findByText("Claude")).closest("article")
        : null;
      if (!claudeCard) throw new Error("未找到 Claude 状态卡");

      expect(within(claudeCard).getByText(label)).toHaveClass(toneClass);
      expect(within(claudeCard).getByText(description)).toBeVisible();
      expect(within(claudeCard).getByText(diagnosticCode)).toBeVisible();
      const button = within(claudeCard).getByRole("button", {
        name: "生成全局预览",
      });
      expect(button).toBeDisabled();
      fireEvent.click(button);
      expect(commands.previewMcpSync).not.toHaveBeenCalled();
      const importButton = within(claudeCard).getByRole("button", {
        name: "检测并导入已有 MCP",
      });
      expect(importButton).toBeDisabled();
      fireEvent.click(importButton);
      expect(commands.discoverMcpImport).not.toHaveBeenCalled();
    },
  );

  it("空目标预览只提示无需写入，不展示可 Apply 的对话框", async () => {
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, targets: [] },
    });
    renderPage();
    const statusSection = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const claudeCard = statusSection
      ? (await within(statusSection).findByText("Claude")).closest("article")
      : null;
    if (!claudeCard) throw new Error("未找到 Claude 状态卡");
    fireEvent.click(
      within(claudeCard).getByRole("button", { name: "生成全局预览" }),
    );
    const status =
      await screen.findByText(/暂无启用且已分配到该工具的中央 MCP/);
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(/暂无启用且已分配到该工具的中央 MCP/),
    ).toHaveLength(1);
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });
});
