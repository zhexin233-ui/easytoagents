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
  it("直接应用模式下启停已分配 MCP 自动同步其分配工具", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct", enabledTools: ["claude", "codex"] },
    });
    const assignedServer = makeMcpServer({
      ...server,
      globalTools: ["claude"],
    });
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
    const status = await screen.findByText(/已应用 1 个 MCP 目标/);
    expect(status).toHaveAttribute("role", "status");
    expect(screen.getAllByText(/已应用 1 个 MCP 目标/)).toHaveLength(1);
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
    const submit = within(dialog).getByRole("button", { name: "保存中央意图" });
    const nameInput = within(dialog).getByLabelText("名称");
    expect(nameInput).toHaveFocus();
    fireEvent.keyDown(nameInput, { key: "Tab", shiftKey: true });
    expect(submit).toHaveFocus();
    fireEvent.keyDown(submit, { key: "Tab" });
    expect(nameInput).toHaveFocus();
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
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
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
    await screen.findByText("尚无 MCP");
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
    for (const name of ["取消", "正在保存…"]) {
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
    expect(
      screen.queryByText(
        "中央 MCP 已保存；原生配置尚未修改。请生成预览后再 Apply。",
      ),
    ).not.toBeInTheDocument();
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
    expect(
      await screen.findByText(
        "中央 MCP 已保存；原生配置尚未修改。请生成预览后再 Apply。",
      ),
    ).toHaveAttribute("role", "status");
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
    await screen.findByText("尚无 MCP");
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
    const status = await screen.findByText(
      "中央 MCP 已保存；原生配置尚未修改。请生成预览后再 Apply。",
    );
    expect(status).toHaveAttribute("role", "status");
    expect(
      screen.getAllByText(
        "中央 MCP 已保存；原生配置尚未修改。请生成预览后再 Apply。",
      ),
    ).toHaveLength(1);
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
});
