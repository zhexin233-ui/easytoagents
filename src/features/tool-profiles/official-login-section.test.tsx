import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type OfficialLoginPhase } from "@/bindings/commands";
import { OfficialLoginSection } from "@/features/tool-profiles/official-login-section";
import { makeOfficialLoginStatus } from "@/test/fixtures/dtos";
import { renderWithProviders } from "@/test/render";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

const confirmSpy = vi.fn<(message?: string) => boolean>();

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("confirm", confirmSpy);
  confirmSpy.mockReturnValue(true);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("OfficialLoginSection", () => {
  it.each<{
    phase: OfficialLoginPhase;
    text: string;
    destructive: boolean;
  }>([
    { phase: "succeeded", text: "登录流程已完成。", destructive: false },
    { phase: "failed", text: "登录流程失败。", destructive: true },
    { phase: "cancelled", text: "登录已取消。", destructive: false },
    {
      phase: "timed_out",
      text: "登录等待超时，已终止子进程。",
      destructive: true,
    },
  ])("终态 $phase 显示对应文案与诊断", async ({ phase, text, destructive }) => {
    vi.mocked(commands.getOfficialLoginStatus).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "codex",
        phase,
        loggedIn: phase === "succeeded",
        authMethod: phase === "succeeded" ? "chatgpt" : null,
        diagnostic: "Starting local login server",
        manualCommand: "codex login",
      }),
    });
    renderWithProviders(<OfficialLoginSection tool="codex" />);
    const phaseText = await screen.findByText(text);
    expect(phaseText).toBeVisible();
    if (destructive) {
      expect(phaseText).toHaveClass("text-destructive");
    } else {
      expect(phaseText).not.toHaveClass("text-destructive");
    }
    expect(screen.getByText("Starting local login server")).toBeVisible();
    expect(
      screen.getByText(
        phase === "succeeded"
          ? "已登录官方账号（chatgpt）"
          : "当前未登录官方账号。",
      ),
    ).toBeVisible();
    // 终态不轮询，登录按钮可再次使用。
    expect(screen.getByRole("button", { name: "登录官方账号" })).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "取消登录" }),
    ).not.toBeInTheDocument();
    expect(commands.getOfficialLoginStatus).toHaveBeenCalledTimes(1);
  });

  it("运行中展示登录地址供手动访问，并只在运行中轮询", async () => {
    vi.mocked(commands.getOfficialLoginStatus).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "claude",
        phase: "running",
        loggedIn: null,
        loginUrl: "https://claude.ai/oauth/authorize?state=fixture",
      }),
    });
    renderWithProviders(<OfficialLoginSection tool="claude" />);
    expect(
      await screen.findByText("正在等待浏览器完成官方账号授权…"),
    ).toBeVisible();
    expect(
      screen.getByText("https://claude.ai/oauth/authorize?state=fixture"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "取消登录" })).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "登录官方账号" }),
    ).not.toBeInTheDocument();
  });

  it("已登录时再次登录先确认；Codex 提示会清除现有凭据，拒绝后不调用命令", async () => {
    vi.mocked(commands.getOfficialLoginStatus).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "codex",
        loggedIn: true,
        authMethod: "chatgpt",
        manualCommand: "codex login",
      }),
    });
    vi.mocked(commands.startOfficialLogin).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "codex",
        phase: "running",
        loggedIn: null,
        manualCommand: "codex login",
      }),
    });
    renderWithProviders(<OfficialLoginSection tool="codex" />);
    expect(await screen.findByText("已登录官方账号（chatgpt）")).toBeVisible();
    expect(
      screen.getByText(/开始登录时 Codex 会先清除当前的登录凭据/),
    ).toBeVisible();

    confirmSpy.mockReturnValueOnce(false);
    fireEvent.click(screen.getByRole("button", { name: "登录官方账号" }));
    expect(confirmSpy).toHaveBeenCalledWith(
      expect.stringContaining("立即清除现有凭据"),
    );
    expect(commands.startOfficialLogin).not.toHaveBeenCalled();

    confirmSpy.mockReturnValueOnce(true);
    fireEvent.click(screen.getByRole("button", { name: "登录官方账号" }));
    await waitFor(() =>
      expect(commands.startOfficialLogin).toHaveBeenCalledWith("codex"),
    );
    expect(
      await screen.findByText("正在等待浏览器完成官方账号授权…"),
    ).toBeVisible();
  });

  it("未登录时直接启动登录，不弹确认", async () => {
    vi.mocked(commands.getOfficialLoginStatus).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({ tool: "claude", loggedIn: false }),
    });
    vi.mocked(commands.startOfficialLogin).mockResolvedValue({
      status: "ok",
      data: makeOfficialLoginStatus({
        tool: "claude",
        phase: "running",
        loggedIn: null,
      }),
    });
    renderWithProviders(<OfficialLoginSection tool="claude" />);
    // 状态加载完成前登录按钮保持禁用；等待探测结果后再点击。
    expect(await screen.findByText("当前未登录官方账号。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "登录官方账号" }));
    await waitFor(() =>
      expect(commands.startOfficialLogin).toHaveBeenCalledWith("claude"),
    );
    expect(confirmSpy).not.toHaveBeenCalled();
  });

  it("状态查询失败时显示结构化错误并保留刷新入口", async () => {
    vi.mocked(commands.getOfficialLoginStatus).mockResolvedValue({
      status: "error",
      error: {
        code: "ENVIRONMENT_PROBING",
        message: "工具环境仍在检测中，请稍后重试",
        recoverable: true,
        action: "rescan",
      },
    });
    renderWithProviders(<OfficialLoginSection tool="claude" />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "ENVIRONMENT_PROBING：工具环境仍在检测中，请稍后重试",
    );
    expect(screen.getByRole("button", { name: "登录官方账号" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新状态" })).toBeEnabled();
  });
});
