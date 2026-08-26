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
import { McpPage } from "@/features/mcp/mcp-page";

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
    previewMcpSync: vi.fn(),
    applyMcpPreview: vi.fn(),
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
    await within(section).findByText(tool === "claude" ? "Claude" : "Codex")
  ).closest("article");
  if (!card) throw new Error("未找到工具状态卡");
  return within(card).getByRole("button", { name });
}

beforeEach(() => {
  vi.clearAllMocks();
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
    ],
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
            : "/isolated/home/.codex/config.toml",
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
  it("使用遮罩字段创建结构化 stdio MCP，并只调用生成 command", async () => {
    vi.mocked(commands.createMcpServer).mockResolvedValue({
      status: "ok",
      data: server,
    });
    renderPage();
    const form = screen
      .getByRole("heading", { name: "新增 MCP" })
      .closest("section");
    if (!form) throw new Error("未找到 MCP 表单");
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
    const envInput = screen.getByLabelText("Env JSON");
    expect(envInput).toHaveValue("{}");
    expect(envInput).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "保存中央意图" }));
    await waitFor(() =>
      expect(commands.updateMcpServer).toHaveBeenCalledWith(
        expect.objectContaining({
          id: server.id,
          headers: { action: "clear" },
          env: { action: "keep" },
          extra: { action: "keep" },
          rowVersion: server.rowVersion,
        }),
      ),
    );
    expect(screen.queryByText("ui-secret")).not.toBeInTheDocument();
  });

  it("把全局继承项显示为只读且不可重复选择", async () => {
    vi.mocked(commands.listMcpServers).mockResolvedValue({
      status: "ok",
      data: [{ ...server, globalTools: ["claude"] }],
    });
    vi.mocked(commands.listMcpProjects).mockResolvedValue({
      status: "ok",
      data: [
        {
          id: "00000000-0000-4000-8000-000000000510",
          displayName: "隔离项目",
          rootPath: "/isolated/project",
          codexTrustStatus: "trusted",
          rowVersion: 4,
        },
      ],
    });
    vi.mocked(commands.listMcpProjectOptions).mockResolvedValue({
      status: "ok",
      data: [
        {
          mcpId: server.id,
          name: server.name,
          enabled: true,
          state: "inherited",
          selectable: false,
          rowVersion: server.rowVersion,
        },
      ],
    });
    renderPage();
    await screen.findByRole("option", { name: "隔离项目" });
    fireEvent.change(await screen.findByLabelText("项目"), {
      target: { value: "00000000-0000-4000-8000-000000000510" },
    });
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledWith({
        projectId: "00000000-0000-4000-8000-000000000510",
        tool: "claude",
      }),
    );
    const button = await screen.findByRole("button", { name: "只读继承" });
    expect(button).toBeDisabled();
    expect(
      screen.getByText("全局继承（项目不可禁用或重复选择）"),
    ).toBeVisible();
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

  it.each(["claude", "codex"] as const)(
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
        name: `导入 ${tool === "claude" ? "Claude" : "Codex"} 全局 MCP`,
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
