/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  commands,
  type McpImportPreviewDto,
  type McpServerDto,
  type PreviewPlan,
  type Tool,
} from "@/bindings/commands";
import { centralListLayoutStorageKeys } from "@/components/use-persisted-central-list-layout";
import { McpPage } from "@/features/mcp/mcp-page";
import { toolMetadata } from "@/lib/tool-metadata";

vi.mock("@/bindings/commands", () => ({
  commands: {
    listMcpServers: vi.fn(),
    getMcpServer: vi.fn(),
    createMcpServer: vi.fn(),
    updateMcpServer: vi.fn(),
    setMcpEnabled: vi.fn(),
    deleteMcpServer: vi.fn(),
    setGlobalMcpAssignment: vi.fn(),
    setProjectMcpAssignment: vi.fn(),
    listMcpProjects: vi.fn(),
    listMcpProjectOptions: vi.fn(),
    listGlobalMcpTargetStatuses: vi.fn(),
    getAppSettings: vi.fn(),
    updateAppSettings: vi.fn(),
    previewMcpSync: vi.fn(),
    applyMcpPreview: vi.fn(),
    readoptMcpTarget: vi.fn(),
    discoverMcpImport: vi.fn(),
    confirmMcpImport: vi.fn(),
  },
}));

const server: McpServerDto = {
  id: "00000000-0000-4000-8000-000000000501",
  name: "fixture-mcp",
  transport: "stdio",
  command: "npx",
  args: ["-y", "fixture"],
  url: null,
  headerNames: [],
  envNames: ["MCP_TOKEN"],
  redactedExtra: { nested: { apiToken: "[REDACTED]" } },
  enabled: true,
  globalTools: [],
  rowVersion: 2,
};

const preview: PreviewPlan = {
  previewId: "00000000-0000-4000-8000-000000000599",
  scope: "global",
  projectId: null,
  dbVersion: 3,
  warningCodes: [],
  targets: [
    {
      targetId: "00000000-0000-4000-8000-000000000598",
      descriptor: {
        tool: "claude",
        artifactKind: "mcp",
        scope: "global",
        projectRoot: null,
        path: "/isolated/home/.claude.json",
        format: "json",
        managedSelectorRoots: ["mcpServers"],
        sensitiveSelectors: ["mcpServers/*/headers", "mcpServers/*/env"],
        capability: { state: "supported", diagnosticCode: null },
        policy: "allowed",
        trust: "not_required",
        promptOverride: "not_applicable",
        symlinkPolicy: "reject",
      },
      ownership: { kind: "selectors", paths: [["mcpServers", "fixture-mcp"]] },
      changeKind: "update",
      status: "in_sync",
      currentFullHash: "a".repeat(64),
      currentManagedHash: "b".repeat(64),
      desiredManagedHash: "c".repeat(64),
      targetRowVersion: 1,
      rowVersions: [],
      redactedDiff: {
        before: {},
        after: { mcpServers: { "fixture-mcp": { env: "[REDACTED]" } } },
      },
      warningCodes: [],
      baselineMismatchedItems: [],
      readoptAvailable: false,
      errorCode: null,
      git: null,
      excludeFromGit: false,
    },
  ],
};

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
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <McpPage />
    </QueryClientProvider>,
  );
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
    data: { applyMode: "preview_confirm" },
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

afterEach(cleanup);

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
    const assignedServer: McpServerDto = {
      ...server,
      globalTools: ["claude"],
    };
    const updatedServer: McpServerDto = {
      ...assignedServer,
      globalTools: ["claude", "codex"],
      rowVersion: assignedServer.rowVersion + 1,
    };
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
  });

  it("直接应用模式下分配切换自动同步并 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    const assignedServer: McpServerDto = {
      ...server,
      globalTools: ["claude"],
    };
    const updatedServer: McpServerDto = {
      ...assignedServer,
      globalTools: ["claude", "codex"],
      rowVersion: assignedServer.rowVersion + 1,
    };
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

  it("直接应用模式下启停已分配 MCP 自动同步其分配工具", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    const assignedServer: McpServerDto = {
      ...server,
      globalTools: ["claude"],
    };
    vi.mocked(commands.listMcpServers)
      .mockResolvedValueOnce({ status: "ok", data: [assignedServer] })
      .mockResolvedValue({
        status: "ok",
        data: [
          {
            ...assignedServer,
            enabled: false,
            rowVersion: assignedServer.rowVersion + 1,
          },
        ],
      });
    vi.mocked(commands.setMcpEnabled).mockResolvedValue({
      status: "ok",
      data: { ...assignedServer, enabled: false },
    });
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "停用" }));
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
  });

  it("MCP 非受管变更保留共享状态原义，不使用 Skills 首次目录文案", async () => {
    vi.mocked(commands.listGlobalMcpTargetStatuses).mockResolvedValueOnce({
      status: "ok",
      data: [
        {
          tool: "claude",
          projectId: null,
          targetPath: "/isolated/home/.claude.json",
          status: "external_non_owned_change",
          diagnosticCode: "EXTERNAL_NON_OWNED_CHANGE",
        },
      ],
    });
    renderPage();
    expect(await screen.findByText("△ 非受管变更")).toBeVisible();
    expect(screen.getByText("EXTERNAL_NON_OWNED_CHANGE")).toBeVisible();
    expect(screen.queryByText("○ 未纳入同步管理")).not.toBeInTheDocument();
    expect(screen.queryByText("○ 空目录，待配置")).not.toBeInTheDocument();
    expect(await globalButton("生成全局预览")).toBeEnabled();
    expect(commands.previewMcpSync).not.toHaveBeenCalled();
  });

  it("默认隐藏表单，新增与编辑可取消、关闭和 Escape 清理草稿并恢复焦点", async () => {
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [server],
    });
    renderPage();
    const edit = await screen.findByRole("button", { name: "编辑" });
    const trigger = screen.getByRole("button", { name: "新增 MCP" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("名称")).not.toBeInTheDocument();
    trigger.focus();
    fireEvent.click(trigger);

    let dialog = screen.getByRole("dialog", { name: "新增 MCP" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleDescription(/保存只更新中央 MCP/);
    const close = within(dialog).getByRole("button", { name: "关闭" });
    const submit = within(dialog).getByRole("button", { name: "保存中央意图" });
    expect(close).toHaveFocus();
    fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    expect(submit).toHaveFocus();
    fireEvent.keyDown(submit, { key: "Tab" });
    expect(close).toHaveFocus();
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "未保存草稿" },
    });
    fireEvent.change(within(dialog).getByLabelText("Env JSON"), {
      target: { value: '{"TOKEN":"draft-secret"}' },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    edit.focus();
    fireEvent.click(edit);
    dialog = screen.getByRole("dialog", { name: "编辑 MCP" });
    expect(within(dialog).getByLabelText("名称")).toHaveValue(server.name);
    expect(within(dialog).getByLabelText("Env JSON")).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(edit).toHaveFocus();

    trigger.focus();
    fireEvent.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新增 MCP" });
    expect(within(dialog).getByLabelText("名称")).toHaveValue("");
    expect(within(dialog).getByLabelText("Env JSON")).toHaveValue("{}");
    expect(within(dialog).getByLabelText("Env JSON")).toBeEnabled();
    expect(
      within(dialog).queryByText(/保持数据库中的/),
    ).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    expect(commands.createMcpServer).not.toHaveBeenCalled();
    expect(commands.updateMcpServer).not.toHaveBeenCalled();
  });

  it("校验与保存错误留在弹窗内，保留输入并在关闭后清理", async () => {
    vi.mocked(commands.createMcpServer).mockResolvedValue({
      status: "error",
      error: {
        code: "CONFLICT",
        message: "MCP 名称已存在",
        recoverable: true,
        action: "rescan",
      },
    });
    renderPage();
    const trigger = screen.getByRole("button", { name: "新增 MCP" });
    fireEvent.click(trigger);
    let dialog = screen.getByRole("dialog", { name: "新增 MCP" });
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "冲突草稿" },
    });
    fireEvent.change(within(dialog).getByLabelText("Command"), {
      target: { value: "npx" },
    });
    fireEvent.change(within(dialog).getByLabelText("Env JSON"), {
      target: { value: "invalid-json" },
    });
    fireEvent.submit(within(dialog).getByRole("form"));
    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "Env 不是合法 JSON。",
    );
    expect(commands.createMcpServer).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    fireEvent.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新增 MCP" });
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("Env JSON")).toHaveValue("{}");

    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "冲突草稿" },
    });
    fireEvent.change(within(dialog).getByLabelText("Command"), {
      target: { value: "npx" },
    });
    fireEvent.submit(within(dialog).getByRole("form"));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "CONFLICT：MCP 名称已存在",
    );
    expect(within(dialog).getByLabelText("名称")).toHaveValue("冲突草稿");
    expect(within(dialog).getByLabelText("Command")).toHaveValue("npx");
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新增 MCP" });
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("名称")).toHaveValue("");
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });

  it("保存及刷新期间阻止重复提交和关闭，完成后可安全打开新草稿", async () => {
    const pending =
      deferred<Awaited<ReturnType<typeof commands.createMcpServer>>>();
    const refresh =
      deferred<Awaited<ReturnType<typeof commands.listMcpServers>>>();
    vi.mocked(commands.createMcpServer).mockReturnValueOnce(pending.promise);
    renderPage();
    await screen.findByText(/中央库尚无 MCP/);
    vi.mocked(commands.listMcpServers).mockReturnValueOnce(refresh.promise);
    const trigger = screen.getByRole("button", { name: "新增 MCP" });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "新增 MCP" });
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: server.name },
    });
    fireEvent.change(within(dialog).getByLabelText("Command"), {
      target: { value: "npx" },
    });
    const form = within(dialog).getByRole("form");
    within(dialog).getByRole("button", { name: "保存中央意图" }).focus();
    act(() => {
      fireEvent.submit(form);
      fireEvent.submit(form);
      fireEvent.click(trigger);
      fireEvent.keyDown(dialog, { key: "Escape" });
    });
    expect(await within(dialog).findByRole("status")).toHaveTextContent(
      "正在保存",
    );
    expect(dialog).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Tab" });
    const firstField = within(dialog).getByLabelText("名称");
    const lastField = within(dialog).getByRole("checkbox", {
      name: /启用（停用后/,
    });
    expect(firstField).toHaveFocus();
    fireEvent.keyDown(firstField, { key: "Tab", shiftKey: true });
    expect(lastField).toHaveFocus();
    dialog.focus();
    fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
    expect(lastField).toHaveFocus();
    fireEvent.keyDown(lastField, { key: "Tab" });
    expect(firstField).toHaveFocus();
    for (const name of ["关闭", "取消", "正在保存…"]) {
      const button = within(dialog).getByRole("button", { name });
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    fireEvent.keyDown(dialog, { key: "Escape" });
    fireEvent.submit(form);
    expect(dialog).toBeVisible();
    expect(commands.createMcpServer).toHaveBeenCalledTimes(1);

    await act(async () => {
      pending.resolve({ status: "ok", data: server });
      await pending.promise;
    });
    await waitFor(() =>
      expect(commands.listMcpServers).toHaveBeenCalledTimes(2),
    );
    fireEvent.click(trigger);
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(dialog).toBeVisible();
    expect(within(dialog).getByLabelText("名称")).toHaveValue(server.name);
    await act(async () => {
      refresh.resolve({ status: "ok", data: [server] });
      await refresh.promise;
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(trigger).toHaveFocus();
    fireEvent.click(trigger);
    const nextDialog = screen.getByRole("dialog", { name: "新增 MCP" });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("");
    fireEvent.change(within(nextDialog).getByLabelText("名称"), {
      target: { value: "下一份草稿" },
    });
    expect(within(nextDialog).getByLabelText("名称")).toHaveValue("下一份草稿");
    expect(commands.createMcpServer).toHaveBeenCalledTimes(1);
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });

  it("使用遮罩字段创建结构化 stdio MCP，并只调用生成 command", async () => {
    vi.mocked(commands.createMcpServer).mockResolvedValue({
      status: "ok",
      data: server,
    });
    renderPage();
    await screen.findByText(/中央库尚无 MCP/);
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [server],
    });
    fireEvent.click(screen.getByRole("button", { name: "新增 MCP" }));
    const form = screen.getByRole("dialog", { name: "新增 MCP" });
    const envInput = within(form).getByLabelText("Env JSON");
    expect(envInput).toHaveAttribute("type", "password");
    fireEvent.change(within(form).getByLabelText("名称"), {
      target: { value: "fixture-mcp" },
    });
    fireEvent.change(within(form).getByLabelText("Command"), {
      target: { value: "npx" },
    });
    fireEvent.change(within(form).getByLabelText("Args（每行一项）"), {
      target: { value: "-y\nfixture" },
    });
    fireEvent.change(envInput, {
      target: { value: '{"MCP_TOKEN":"ui-secret"}' },
    });
    fireEvent.change(within(form).getByLabelText("扩展字段 JSON"), {
      target: { value: '{"startup_timeout_sec":10}' },
    });
    fireEvent.click(within(form).getByRole("button", { name: "保存中央意图" }));
    await waitFor(() =>
      expect(commands.createMcpServer).toHaveBeenCalledWith({
        name: "fixture-mcp",
        transport: "stdio",
        command: "npx",
        args: ["-y", "fixture"],
        url: null,
        headers: {},
        env: { MCP_TOKEN: "ui-secret" },
        extra: { startup_timeout_sec: 10 },
        enabled: true,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(
      await screen.findByRole("heading", { name: server.name }),
    ).toBeVisible();
    expect(commands.listMcpServers).toHaveBeenCalledTimes(2);
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });

  it("编辑时不回填敏感值并默认发送 keep", async () => {
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [server],
    });
    vi.mocked(commands.updateMcpServer).mockResolvedValue({
      status: "ok",
      data: server,
    });
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "编辑" }));
    expect(screen.getByRole("dialog", { name: "编辑 MCP" })).toBeVisible();
    const envInput = screen.getByLabelText("Env JSON");
    expect(envInput).toHaveValue("{}");
    expect(envInput).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "保存中央意图" }));
    await waitFor(() =>
      expect(commands.updateMcpServer).toHaveBeenCalledWith({
        id: server.id,
        name: server.name,
        transport: "stdio",
        command: server.command,
        args: server.args,
        url: null,
        headers: { action: "clear" },
        env: { action: "keep" },
        extra: { action: "keep" },
        enabled: server.enabled,
        rowVersion: server.rowVersion,
      }),
    );
    expect(screen.queryByText("ui-secret")).not.toBeInTheDocument();
  });

  it("不再呈现或查询项目追加入口", async () => {
    renderPage();
    expect(
      await screen.findByRole("heading", { name: "全局目标状态" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "项目追加选择器" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByLabelText("项目")).not.toBeInTheDocument();
    expect(commands.listMcpProjects).not.toHaveBeenCalled();
    expect(commands.listMcpProjectOptions).not.toHaveBeenCalled();
    expect(commands.setProjectMcpAssignment).not.toHaveBeenCalled();
  });

  it("打开脱敏持久化预览并用原 previewId Apply", async () => {
    renderPage();
    const statusSection = screen
      .getByRole("heading", { name: "全局目标状态" })
      .closest("section");
    const claudeCard = statusSection
      ? (await within(statusSection).findByText("Claude")).closest("article")
      : null;
    if (!claudeCard) throw new Error("未找到 Claude 状态卡");
    expect(within(claudeCard).getByText("○ 待初始化")).toHaveClass(
      "bg-amber-50",
    );
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

  it("直接应用模式下无冲突预览跳过对话框立即 Apply", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    renderPage();
    const previewButton = await globalButton("直接应用全局同步");
    fireEvent.click(previewButton);
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
    expect(await screen.findByText(/已应用 1 个 MCP 目标/)).toHaveAttribute(
      "role",
      "status",
    );
  });

  it("直接应用模式下预览与 Apply 失败都只使用失败通知", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    vi.mocked(commands.previewMcpSync).mockResolvedValueOnce({
      status: "error",
      error: {
        code: "DATABASE_ERROR",
        message: "MCP 预览暂不可用",
        recoverable: true,
      },
    });
    vi.mocked(commands.applyMcpPreview).mockResolvedValue({
      status: "error",
      error: {
        code: "ATOMIC_WRITE_FAILED",
        message: "MCP 应用失败",
        recoverable: true,
      },
    });
    renderPage();

    fireEvent.click(await globalButton("直接应用全局同步"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "DATABASE_ERROR：MCP 预览暂不可用",
    );
    expect(
      screen.getAllByText("DATABASE_ERROR：MCP 预览暂不可用"),
    ).toHaveLength(1);

    fireEvent.click(await globalButton("直接应用全局同步"));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "ATOMIC_WRITE_FAILED：MCP 应用失败",
      ),
    );
    expect(
      screen.getAllByText("ATOMIC_WRITE_FAILED：MCP 应用失败"),
    ).toHaveLength(1);
  });

  it("直接应用模式下冲突预览回退为人工确认且 Apply 禁用", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
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
    const previewButton = await globalButton("直接应用全局同步");
    fireEvent.click(previewButton);
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "应用这份预览" })).toBeDisabled();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });

  it("冲突预览展示不匹配条目并支持以当前内容重新接管", async () => {
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
    // 接管后自动重新生成预览。
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledTimes(2),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
  });

  it.each([
    [
      "CLAUDE_POLICY_UNKNOWN",
      "△ 策略状态待确认",
      "无法确认 Claude 管理策略是否允许该类自定义目标，当前已安全阻止预览。",
      "bg-amber-50",
    ],
    [
      "CLAUDE_POLICY_BLOCKED",
      "⛔ 策略阻止",
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
    expect(
      await screen.findByText(/暂无启用且已分配到该工具的中央 MCP/),
    ).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
  });

  it("直接应用模式下空目标预览使用通知提示无需写入", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" },
    });
    vi.mocked(commands.previewMcpSync).mockResolvedValue({
      status: "ok",
      data: { ...preview, targets: [] },
    });
    renderPage();

    fireEvent.click(await globalButton("直接应用全局同步"));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "暂无启用且已分配到该工具的中央 MCP",
    );
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
    expect(commands.applyMcpPreview).not.toHaveBeenCalled();
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
      expect(
        await screen.findByText(/原生配置未改写，请单独生成全局预览/),
      ).toBeVisible();
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
    const close = screen.getByRole("button", { name: "关闭 MCP 导入" });
    const rescan = screen.getByRole("button", { name: "重新检测" });
    close.focus();
    fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    expect(rescan).toHaveFocus();
    fireEvent.keyDown(rescan, { key: "Tab" });
    expect(close).toHaveFocus();
    fireEvent.keyDown(close, { key: "Escape" });
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
    const close = screen.getByRole("button", { name: "关闭 MCP 导入" });
    expect(close).toBeDisabled();
    fireEvent.click(close);
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
