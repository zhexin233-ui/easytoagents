import {
  fireEvent,
  screen,
  waitFor,
  within,
  act,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  claudeIconUrl,
  codexIconUrl,
  cursorIconUrl,
} from "./project-detail-page.test-helpers";
import {
  commands,
  createDeferred,
  project,
  preview,
  mcpOptions,
  hookOptions,
  skillPreview,
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
  it("默认选择 Claude MCP，可切换 Cursor 的 MCP/Skills", async () => {
    renderPage();

    const resourceGroup = await screen.findByRole("group", {
      name: "项目资源管理视图",
    });
    const mcpButton = within(resourceGroup).getByRole("button", {
      name: "管理项目 MCP",
    });
    const skillButton = within(resourceGroup).getByRole("button", {
      name: "管理项目 Skill",
    });
    expect(
      within(resourceGroup).queryByRole("button", {
        name: "管理项目提示词",
      }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/项目提示词/)).not.toBeInTheDocument();
    const platformGroup = screen.getByRole("group", {
      name: "项目平台管理视图",
    });
    const claudeButton = within(platformGroup).getByRole("button", {
      name: "管理 Claude 项目资源",
    });
    const codexButton = within(platformGroup).getByRole("button", {
      name: "管理 Codex 项目资源",
    });
    const cursorButton = within(platformGroup).getByRole("button", {
      name: "管理 Cursor 项目资源",
    });
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveAttribute("aria-pressed", "false");
    expect(claudeButton).toHaveAttribute("aria-pressed", "true");
    expect(claudeButton).toHaveAttribute("title", "管理 Claude 项目资源");
    expect(codexButton).toHaveAttribute("aria-pressed", "false");
    expect(codexButton).toHaveAttribute("title", "管理 Codex 项目资源");
    expect(cursorButton).toHaveAttribute("aria-pressed", "false");
    expect(cursorButton).toHaveAttribute("title", "管理 Cursor 项目资源");
    expect(claudeButton.querySelector("img")).toHaveAttribute(
      "src",
      claudeIconUrl,
    );
    expect(codexButton.querySelector("img")).toHaveAttribute(
      "src",
      codexIconUrl,
    );
    expect(cursorButton.querySelector("img")).toHaveAttribute(
      "src",
      cursorIconUrl,
    );
    expect(claudeButton.querySelector("img")).toHaveAttribute("alt", "");
    expect(claudeButton.querySelector("img")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(codexButton.querySelector("img")).toHaveAttribute("alt", "");
    expect(codexButton.querySelector("img")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(claudeButton.querySelector("svg")).toBeNull();
    expect(codexButton.querySelector("svg")).toBeNull();
    expect(claudeButton.firstElementChild).toHaveClass("opacity-100");
    expect(codexButton.firstElementChild).toHaveClass(
      "opacity-25",
      "grayscale",
    );
    expect(
      await screen.findByRole("heading", { name: "Claude MCP 项目追加" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: /Codex .*项目追加/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Skills" }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(1),
    );
    expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "claude",
    });
    expect(commands.listSkillProjectOptions).not.toHaveBeenCalled();

    fireEvent.click(skillButton);
    expect(mcpButton).toHaveAttribute("aria-pressed", "false");
    expect(skillButton).toHaveAttribute("aria-pressed", "true");
    expect(
      await screen.findByRole("heading", { name: "Claude Skill 项目追加" }),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "Skills" })).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "MCP" }),
    ).not.toBeInTheDocument();
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(1),
    );
    expect(commands.listSkillProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "claude",
    });

    fireEvent.click(codexButton);
    expect(claudeButton).toHaveAttribute("aria-pressed", "false");
    expect(codexButton).toHaveAttribute("aria-pressed", "true");
    expect(
      await screen.findByRole("heading", { name: "Codex Skill 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listSkillProjectOptions).toHaveBeenCalledTimes(2),
    );
    expect(commands.listSkillProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "codex",
    });

    fireEvent.click(mcpButton);
    expect(mcpButton).toHaveAttribute("aria-pressed", "true");
    expect(skillButton).toHaveAttribute("aria-pressed", "false");
    expect(
      await screen.findByRole("heading", { name: "Codex MCP 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(2),
    );
    expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "codex",
    });

    fireEvent.click(claudeButton);
    expect(claudeButton).toHaveAttribute("aria-pressed", "true");
    expect(codexButton).toHaveAttribute("aria-pressed", "false");
    expect(
      await screen.findByRole("heading", { name: "Claude MCP 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenCalledTimes(3),
    );
    expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
      projectId: project.id,
      tool: "claude",
    });

    fireEvent.click(cursorButton);
    expect(
      await screen.findByRole("heading", { name: "Cursor MCP 项目追加" }),
    ).toBeVisible();
    // 可继续切换到 Cursor 的项目 Skill。
    fireEvent.click(skillButton);
    expect(
      await screen.findByRole("heading", { name: "Cursor Skill 项目追加" }),
    ).toBeVisible();
    fireEvent.click(mcpButton);
    expect(
      await screen.findByRole("heading", { name: "Cursor MCP 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
        projectId: project.id,
        tool: "cursor",
      }),
    );
  });

  it.each([
    ["claude", "Claude"],
    ["codex", "Codex"],
    ["cursor", "Cursor"],
    ["zcode", "ZCode"],
  ] as const)(
    "从 %s Hooks 切换到 OpenCode 时隐藏 Hooks 并回落到 MCP",
    async (sourceTool, sourceLabel) => {
      vi.mocked(commands.getAppSettings).mockResolvedValue({
        status: "ok",
        data: {
          applyMode: "preview_confirm",
          enabledTools: [sourceTool, "opencode"],
        },
      });
      vi.mocked(commands.listMcpProjectOptions).mockResolvedValue({
        status: "ok",
        data: mcpOptions,
      });
      vi.mocked(commands.listHookProjectOptions).mockImplementation((input) =>
        input.tool === "opencode"
          ? Promise.resolve({
              status: "error",
              error: {
                code: "INVALID_INPUT",
                message: "OPENCODE_HOOKS_UNSUPPORTED",
                details: { diagnosticCode: "OPENCODE_HOOKS_UNSUPPORTED" },
                recoverable: false,
                action: null,
              },
            })
          : Promise.resolve({ status: "ok", data: hookOptions }),
      );
      renderPage();

      await screen.findByRole("heading", {
        name: `${sourceLabel} MCP 项目追加`,
      });
      fireEvent.click(screen.getByRole("button", { name: "管理项目 Hook" }));
      expect(
        await screen.findByRole("heading", {
          name: `${sourceLabel} Hook 项目追加`,
        }),
      ).toBeVisible();
      await waitFor(() =>
        expect(commands.listHookProjectOptions).toHaveBeenCalledWith({
          projectId: project.id,
          tool: sourceTool,
        }),
      );
      vi.mocked(commands.listHookProjectOptions).mockClear();

      fireEvent.click(
        screen.getByRole("button", { name: "管理 OpenCode 项目资源" }),
      );

      const resourceGroup = screen.getByRole("group", {
        name: "项目资源管理视图",
      });
      await waitFor(() =>
        expect(
          within(resourceGroup).queryByRole("button", {
            name: "管理项目 Hook",
          }),
        ).not.toBeInTheDocument(),
      );
      expect(
        within(resourceGroup).getByRole("button", {
          name: "管理项目 MCP",
        }),
      ).toHaveAttribute("aria-pressed", "true");
      expect(
        await screen.findByRole("heading", {
          name: "OpenCode MCP 项目追加",
        }),
      ).toBeVisible();
      await waitFor(() =>
        expect(commands.listMcpProjectOptions).toHaveBeenCalledWith({
          projectId: project.id,
          tool: "opencode",
        }),
      );
      expect(commands.listHookProjectOptions).not.toHaveBeenCalled();
      expect(
        screen.queryByText(/OPENCODE_HOOKS_UNSUPPORTED/),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByText("中央库暂无可追加项。"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole("heading", { name: "Hooks" }),
      ).not.toBeInTheDocument();
      const excludeCopy = screen.getByText(
        "新建文件写入 .git/info/exclude（不提交到仓库）",
      );
      const assignmentCard = excludeCopy.closest("article");
      expect(assignmentCard).not.toBeNull();
      if (!assignmentCard) {
        throw new Error(".git/info/exclude 说明不在资源追加卡片中");
      }
      expect(
        within(assignmentCard).getByRole("heading", {
          name: "MCP",
        }),
      ).toBeVisible();
    },
  );

  it("当前选中工具被关闭时在渲染期回落到第一个启用工具", async () => {
    const { client } = renderPage();
    await screen.findByRole("heading", { name: "Claude MCP 项目追加" });

    fireEvent.click(
      screen.getByRole("button", { name: "管理 Cursor 项目资源" }),
    );
    expect(
      await screen.findByRole("heading", { name: "Cursor MCP 项目追加" }),
    ).toBeVisible();

    // 确保初始设置请求已完成，再替换设置响应验证渲染期回落。
    await act(async () => {
      await client.refetchQueries({ queryKey: ["settings"] });
    });
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "preview_confirm", enabledTools: ["codex"] },
    });
    await act(async () => {
      await client.invalidateQueries({ queryKey: ["settings"] });
    });

    const platformGroup = screen.getByRole("group", {
      name: "项目平台管理视图",
    });
    await waitFor(() =>
      expect(
        within(platformGroup).queryByRole("button", {
          name: "管理 Cursor 项目资源",
        }),
      ).not.toBeInTheDocument(),
    );
    const codexButton = within(platformGroup).getByRole("button", {
      name: "管理 Codex 项目资源",
    });
    expect(codexButton).toHaveAttribute("aria-pressed", "true");
    expect(
      await screen.findByRole("heading", { name: "Codex MCP 项目追加" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(commands.listMcpProjectOptions).toHaveBeenLastCalledWith({
        projectId: project.id,
        tool: "codex",
      }),
    );
  });

  it("工具配置状态默认折叠且可展开收起", async () => {
    renderPage();

    const toggle = await screen.findByRole("button", {
      name: "展开工具配置状态",
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Claude · MCP")).not.toBeInTheDocument();

    fireEvent.click(toggle);
    const collapseToggle = screen.getByRole("button", {
      name: "收起工具配置状态",
    });
    expect(collapseToggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Claude · MCP")).toBeVisible();

    fireEvent.click(collapseToggle);
    expect(screen.queryByText("Claude · MCP")).not.toBeInTheDocument();
  });

  it("工具配置状态为 Hook 目标渲染 Hooks 标签而不是空文案", async () => {
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "ok",
      data: {
        ...project,
        targets: [
          ...project.targets,
          {
            tool: "claude",
            artifactKind: "hook",
            targetPath: "/isolated/projects/detail/.claude/settings.json",
            capability: "supported",
            policy: "allowed",
            trust: "not_required",
            status: "missing",
            diagnosticCode: null,
          },
        ],
      },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "展开工具配置状态" }),
    );
    expect(screen.getByText("Claude · Hooks")).toBeVisible();
    expect(screen.queryByText(/^Claude · $/)).not.toBeInTheDocument();
  });

  it("项目不可用时“返回项目列表”通过路由跳转而不是改写 location", async () => {
    vi.mocked(commands.getProject).mockResolvedValue({
      status: "error",
      error: {
        code: "NOT_FOUND",
        message: "项目不存在",
        details: {},
        recoverable: false,
        action: null,
      },
    });
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "返回项目列表" }),
    );
    expect(await screen.findByText("项目列表路由已渲染")).toBeVisible();
  });

  it("切换资源或平台会重置尚未提交的 Git exclude 选择", async () => {
    renderPage();
    await screen.findByRole("heading", { name: "Claude MCP 项目追加" });

    const mcpExclude = screen.getByRole("checkbox", {
      name: /新建文件写入 .git\/info\/exclude/,
    });
    fireEvent.click(mcpExclude);
    expect(mcpExclude).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    const skillExclude = screen.getByRole("checkbox", {
      name: /新建文件写入 .git\/info\/exclude/,
    });
    expect(skillExclude).not.toBeChecked();
    fireEvent.click(skillExclude);
    expect(skillExclude).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 MCP" }));
    expect(
      screen.getByRole("checkbox", {
        name: /新建文件写入 .git\/info\/exclude/,
      }),
    ).not.toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    expect(
      screen.getByRole("checkbox", {
        name: /新建文件写入 .git\/info\/exclude/,
      }),
    ).not.toBeChecked();

    fireEvent.click(
      screen.getByRole("button", { name: "管理 Codex 项目资源" }),
    );
    const codexSection = screen
      .getByRole("heading", { name: "Codex Skill 项目追加" })
      .closest("section");
    if (!codexSection) throw new Error("未找到 Codex Skill 项目管理区");
    const codexExclude = within(codexSection).getByRole("checkbox", {
      name: /新建文件写入 .git\/info\/exclude/,
    });
    expect(codexExclude).not.toBeChecked();
    fireEvent.click(codexExclude);
    expect(codexExclude).toBeChecked();

    fireEvent.click(
      screen.getByRole("button", { name: "管理 Claude 项目资源" }),
    );
    expect(
      screen.getByRole("checkbox", {
        name: /新建文件写入 .git\/info\/exclude/,
      }),
    ).not.toBeChecked();
  });

  it("切换组合会清理消息与预览 mutation，并忽略旧组合的迟到结果", async () => {
    const delayedPreview =
      createDeferred<Awaited<ReturnType<typeof commands.previewMcpSync>>>();
    vi.mocked(commands.previewMcpSync)
      .mockResolvedValueOnce({
        status: "ok",
        data: { ...preview, targets: [] },
      })
      .mockReturnValueOnce(delayedPreview.promise);
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    expect(
      await screen.findByText(
        "该项目只有全局继承 MCP，不需要创建项目配置文件。",
      ),
    ).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    expect(
      screen.queryByText("该项目只有全局继承 MCP，不需要创建项目配置文件。"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "管理项目 MCP" }));
    const claudePreviewButton = await screen.findByRole("button", {
      name: "Claude MCP 同步预览",
    });
    fireEvent.click(claudePreviewButton);
    await waitFor(() => expect(claudePreviewButton).toBeDisabled());

    fireEvent.click(
      screen.getByRole("button", { name: "管理 Codex 项目资源" }),
    );
    const codexPreviewButton = await screen.findByRole("button", {
      name: "Codex MCP 同步预览",
    });
    expect(codexPreviewButton).toBeEnabled();

    await act(async () => {
      delayedPreview.resolve({ status: "ok", data: preview });
      await delayedPreview.promise;
    });
    expect(
      screen.queryByRole("dialog", { name: "确认原生配置变更" }),
    ).not.toBeInTheDocument();
  });

  it("旧组合的迟到 Apply 不会关闭当前组合的新预览或写入旧消息", async () => {
    const delayedApply =
      createDeferred<Awaited<ReturnType<typeof commands.applyMcpPreview>>>();
    vi.mocked(commands.applyMcpPreview).mockReturnValueOnce(
      delayedApply.promise,
    );
    renderPage();

    fireEvent.click(
      await screen.findByRole("button", { name: "Claude MCP 同步预览" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledTimes(1),
    );
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    fireEvent.click(
      screen.getByRole("button", { name: "管理 Codex 项目资源" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Codex MCP 同步预览" }),
    );
    expect(
      await screen.findByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();

    await act(async () => {
      delayedApply.resolve({
        status: "ok",
        data: {
          runId: "late-claude-apply",
          status: "succeeded",
          appliedTargets: 1,
          snapshotCount: 1,
        },
      });
      await delayedApply.promise;
    });
    expect(
      screen.getByRole("dialog", { name: "确认原生配置变更" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("项目原生配置已通过持久化预览应用并完成写后验证。"),
    ).not.toBeInTheDocument();
  });

  it("平台切换后 MCP 与 Skill 预览和 Apply 都使用当前 Codex 目标", async () => {
    renderPage();
    fireEvent.click(
      await screen.findByRole("button", { name: "管理 Codex 项目资源" }),
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Codex MCP 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewMcpSync).toHaveBeenCalledWith({
        tool: "codex",
        projectId: project.id,
        excludeFromGit: false,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applyMcpPreview).toHaveBeenCalledWith({
        previewId: preview.previewId,
        tool: "codex",
        projectId: project.id,
      }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "确认原生配置变更" }),
      ).not.toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: "管理项目 Skill" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Codex Skills 同步预览" }),
    );
    await waitFor(() =>
      expect(commands.previewSkillSync).toHaveBeenCalledWith({
        tool: "codex",
        projectId: project.id,
        excludeFromGit: false,
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "应用这份预览" }),
    );
    await waitFor(() =>
      expect(commands.applySkillPreview).toHaveBeenCalledWith({
        previewId: skillPreview.previewId,
        tool: "codex",
        projectId: project.id,
      }),
    );
  });
});
