import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type AgentDto } from "@/bindings/commands";
import { AgentsPage } from "@/features/agents/agents-page";
import { makePreviewPlan, makeTarget } from "@/test/fixtures/preview-plan";
import { renderWithProviders } from "@/test/render";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const agent: AgentDto = {
  id: "agent-1",
  name: "reviewer",
  description: "检查变更并提出风险。",
  prompt: "审阅当前变更。",
  enabled: true,
  globalAssignments: [],
  toolSettings: { claude: null, codex: null },
  rowVersion: 3,
};

function renderAgents() {
  return renderWithProviders(<AgentsPage />);
}

describe("AgentsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex", "cursor", "zcode", "opencode"],
      },
    });
    vi.mocked(commands.listAgents).mockResolvedValue({
      status: "ok",
      data: [agent],
    });
    vi.mocked(commands.listGlobalAgentTargetStatuses).mockResolvedValue({
      status: "ok",
      data: [
        {
          tool: "claude",
          directoryPath: "/isolated/home/.claude/agents",
          aggregateStatus: "missing",
          diagnosticCode: null,
          files: [
            {
              targetPath: "/isolated/home/.claude/agents/reviewer.md",
              status: "missing",
              diagnosticCode: null,
            },
          ],
        },
        {
          tool: "codex",
          directoryPath: "/isolated/home/.codex/agents",
          aggregateStatus: "in_sync",
          diagnosticCode: null,
          files: [],
        },
        {
          tool: "cursor",
          directoryPath: "/isolated/home/.cursor/agents",
          aggregateStatus: "missing",
          diagnosticCode: null,
          files: [],
        },
        {
          tool: "zcode",
          directoryPath: "/isolated/home/.zcode/agents",
          aggregateStatus: "missing",
          diagnosticCode: null,
          files: [],
        },
        {
          tool: "opencode",
          directoryPath: "/isolated/home/.config/opencode/agents",
          aggregateStatus: "missing",
          diagnosticCode: null,
          files: [],
        },
      ],
    });
    vi.mocked(commands.setGlobalAgentAssignment).mockResolvedValue({
      status: "ok",
      data: { ...agent, globalAssignments: ["claude"], rowVersion: 4 },
    });
    vi.mocked(commands.previewAgentSync).mockResolvedValue({
      status: "ok",
      data: makePreviewPlan({
        previewId: "agent-preview-1",
      }),
    });
    vi.mocked(commands.applyAgentPreview).mockResolvedValue({
      status: "ok",
      data: {
        runId: "agent-run-1",
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
    vi.mocked(commands.discoverAgentImport).mockResolvedValue({
      status: "ok",
      data: {
        tool: "claude",
        directoryPath: "/isolated/home/.claude/agents",
        candidates: [
          {
            candidateId: "candidate-1",
            sourcePath: "/isolated/home/.claude/agents/writer.md",
            name: "writer",
            description: "撰写说明",
            prompt: "请撰写说明。",
            droppedFields: ["model", "tools"],
            retainedFields: ["color"],
            toolSettings: { color: "cyan" },
            importable: true,
            diagnosticCode: null,
            reason: null,
          },
        ],
        message: null,
      },
    });
    vi.mocked(commands.confirmAgentImport).mockResolvedValue({
      status: "ok",
      data: { tool: "claude", createdCount: 1 },
    });
  });

  it("名称不符合交集规则时阻止保存", async () => {
    renderAgents();
    fireEvent.click(await screen.findByRole("button", { name: "新增 Agent" }));
    const dialog = screen.getByRole("dialog", { name: "新增 Agent" });
    const name = within(dialog).getByLabelText("名称");
    fireEvent.change(name, { target: { value: "Bad_Name" } });
    fireEvent.blur(name);
    expect(
      within(dialog).getByText(
        "名称只能使用小写字母、数字和连字符，长度为 1–64 个字符。",
      ),
    ).toBeVisible();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "保存中央意图" }),
    );
    expect(commands.createAgent).not.toHaveBeenCalled();
  });

  it("工具特有设置校验失败时阻止保存", async () => {
    renderAgents();
    fireEvent.click(await screen.findByRole("button", { name: "新增 Agent" }));
    const dialog = screen.getByRole("dialog", { name: "新增 Agent" });
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "valid-agent" },
    });
    fireEvent.change(within(dialog).getByLabelText("描述"), {
      target: { value: "描述" },
    });
    fireEvent.change(within(dialog).getByLabelText("系统提示正文"), {
      target: { value: "正文" },
    });
    fireEvent.click(within(dialog).getByText("工具特有设置"));
    fireEvent.change(
      within(dialog).getByRole("combobox", { name: "Claude 模型别名" }),
      { target: { value: "custom" } },
    );
    fireEvent.change(within(dialog).getByLabelText("Claude 自定义模型"), {
      target: { value: "invalid model" },
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "保存中央意图" }),
    );

    expect(
      within(dialog).getByText(
        "模型名必须非空、不能包含空白或 NUL，且不超过 128 字节。",
      ),
    ).toBeVisible();
    expect(commands.createAgent).not.toHaveBeenCalled();
  });

  it("编辑工具特有设置时发送规范化的覆盖层负载", async () => {
    vi.mocked(commands.updateAgent).mockResolvedValue({
      status: "ok",
      data: { ...agent, rowVersion: 4 },
    });
    vi.mocked(commands.setAgentToolSettings).mockResolvedValue({
      status: "ok",
      data: {
        ...agent,
        rowVersion: 5,
        toolSettings: {
          claude: {
            model: "claude-opus-5",
            color: "cyan",
            tools: ["Read", "Bash"],
          },
          codex: null,
        },
      },
    });

    renderAgents();
    fireEvent.click(
      await screen.findByRole("button", { name: `编辑 ${agent.name}` }),
    );
    const dialog = screen.getByRole("dialog", { name: "编辑 Agent" });
    fireEvent.click(within(dialog).getByText("工具特有设置"));
    fireEvent.change(
      within(dialog).getByRole("combobox", { name: "Claude 模型别名" }),
      {
        target: { value: "custom" },
      },
    );
    fireEvent.change(within(dialog).getByLabelText("Claude 自定义模型"), {
      target: { value: "claude-opus-5" },
    });
    fireEvent.change(
      within(dialog).getByRole("combobox", { name: "Claude Agent 颜色" }),
      {
        target: { value: "cyan" },
      },
    );
    fireEvent.change(within(dialog).getByLabelText("Claude Agent 工具列表"), {
      target: { value: "Read, Read, Bash" },
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "保存中央意图" }),
    );

    await waitFor(() =>
      expect(commands.setAgentToolSettings).toHaveBeenCalledWith({
        agentId: agent.id,
        tool: "claude",
        settings: {
          model: "claude-opus-5",
          color: "cyan",
          tools: ["Read", "Bash"],
        },
        rowVersion: 4,
      }),
    );
  });

  it("全局分配发送精确 Agent 与行版本", async () => {
    renderAgents();
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude 全局未分配" }),
    );
    await waitFor(() =>
      expect(commands.setGlobalAgentAssignment).toHaveBeenCalledWith({
        tool: "claude",
        agentId: agent.id,
        assigned: true,
        rowVersion: agent.rowVersion,
      }),
    );
  });

  it("列表徽标显示有工具特有设置的工具数量", async () => {
    vi.mocked(commands.listAgents).mockResolvedValue({
      status: "ok",
      data: [
        {
          ...agent,
          toolSettings: {
            claude: { color: "cyan" },
            codex: null,
          },
        },
      ],
    });

    renderAgents();

    expect(await screen.findByText("1 个工具有特有设置")).toBeVisible();
  });

  it("全局状态可展开文件明细，预览后只通过显式 Apply 写入", async () => {
    renderAgents();
    fireEvent.click(
      await screen.findByRole("button", { name: "展开 Claude 文件状态" }),
    );
    expect(
      await screen.findByText("/isolated/home/.claude/agents/reviewer.md"),
    ).toBeVisible();

    fireEvent.click(
      screen.getByRole("button", { name: "Claude Agents 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewAgentSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "应用这份预览" }));
    await waitFor(() =>
      expect(commands.applyAgentPreview).toHaveBeenCalledWith({
        previewId: "agent-preview-1",
        tool: "claude",
        projectId: null,
      }),
    );
  });

  it("冲突预览按文件重新接管后再次生成预览", async () => {
    const targetPath = "/isolated/home/.claude/agents/reviewer.md";
    const conflictedPlan = makePreviewPlan({
      targets: [
        makeTarget({
          descriptor: {
            ...makeTarget().descriptor,
            artifactKind: "agent",
            path: targetPath,
          },
          changeKind: "conflict",
          status: "external_owned_change",
          readoptAvailable: true,
          errorCode: "CONFLICT",
        }),
      ],
    });
    vi.mocked(commands.previewAgentSync)
      .mockResolvedValueOnce({ status: "ok", data: conflictedPlan })
      .mockResolvedValueOnce({
        status: "ok",
        data: makePreviewPlan({ previewId: "agent-preview-after-readopt" }),
      });
    vi.mocked(commands.readoptAgentTarget).mockResolvedValue({
      status: "ok",
      data: { targetPath },
    });

    renderAgents();
    fireEvent.click(
      await screen.findByRole("button", { name: "Claude Agents 同步预览" }),
    );
    expect(
      await screen.findByRole("button", {
        name: `以当前内容重新接管 ${targetPath}`,
      }),
    ).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", {
        name: `以当前内容重新接管 ${targetPath}`,
      }),
    );
    await waitFor(() =>
      expect(commands.readoptAgentTarget).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
        targetPath,
      }),
    );
    await waitFor(() =>
      expect(commands.previewAgentSync).toHaveBeenCalledTimes(2),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
  });

  it("直接应用模式下已有冲突仍可重新打开预览", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "direct",
        enabledTools: ["claude", "codex", "cursor", "zcode", "opencode"],
      },
    });
    vi.mocked(commands.listGlobalAgentTargetStatuses).mockResolvedValue({
      status: "ok",
      data: [
        {
          tool: "claude",
          directoryPath: "/isolated/home/.claude/agents",
          aggregateStatus: "external_owned_change",
          diagnosticCode: "EXTERNAL_OWNED_CHANGE",
          files: [
            {
              targetPath: "/isolated/home/.claude/agents/reviewer.md",
              status: "external_owned_change",
              diagnosticCode: "EXTERNAL_OWNED_CHANGE",
            },
          ],
        },
      ],
    });
    vi.mocked(commands.previewAgentSync).mockResolvedValue({
      status: "ok",
      data: makePreviewPlan({
        targets: [
          makeTarget({
            changeKind: "conflict",
            status: "external_owned_change",
            errorCode: "CONFLICT",
            readoptAvailable: true,
          }),
        ],
      }),
    });

    renderAgents();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "处理 Claude Agents 同步冲突",
      }),
    );
    await waitFor(() =>
      expect(commands.previewAgentSync).toHaveBeenCalledWith({
        tool: "claude",
        projectId: null,
        excludeFromGit: false,
      }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeVisible();
  });

  it("导入卡片展示将丢弃的字段并发送显式选择", async () => {
    renderAgents();
    fireEvent.click(
      (
        await screen.findAllByRole("button", {
          name: "检测并导入 Claude 全局 Agents",
        })
      )[0]!,
    );
    const dialog = await screen.findByRole("dialog", {
      name: "导入 Claude 全局 Agents",
    });
    await within(dialog).findByText("writer");
    expect(
      within(dialog).getByText("将丢弃工具特有字段：model、tools"),
    ).toBeVisible();
    expect(within(dialog).getByText("将保留工具特有字段：color")).toBeVisible();
    fireEvent.click(
      within(dialog).getByRole("checkbox", { name: "导入 writer" }),
    );
    fireEvent.click(
      within(dialog).getByRole("button", { name: "确认导入所选项（1）" }),
    );
    await waitFor(() =>
      expect(commands.confirmAgentImport).toHaveBeenCalledWith({
        tool: "claude",
        agents: [
          {
            definition: {
              name: "writer",
              description: "撰写说明",
              prompt: "请撰写说明。",
              enabled: true,
            },
            toolSettings: { color: "cyan" },
          },
        ],
      }),
    );
  });
});
