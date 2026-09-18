import { fireEvent, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type AppSettingsDto } from "@/bindings/commands";
import { SettingsDialog } from "@/features/settings/settings-dialog";
import type { ThemePreference } from "@/components/use-theme";
import { renderWithProviders } from "@/test/render";

vi.mock("@/bindings/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/bindings/commands")>();
  const { mockCommands } = await import("@/test/commands-mock");
  return { ...actual, commands: mockCommands(actual.commands) };
});

function renderDialog(
  props: Partial<Parameters<typeof SettingsDialog>[0]> = {},
) {
  return renderWithProviders(
    <SettingsDialog
      open
      onClose={() => {}}
      themePreference="system"
      onThemePreferenceChange={() => {}}
      {...props}
    />,
  );
}

function ThemeStateHarness() {
  const [preference, setPreference] = useState<ThemePreference>("system");
  return (
    <SettingsDialog
      open
      onClose={() => {}}
      themePreference={preference}
      onThemePreferenceChange={setPreference}
    />
  );
}

function renderThemedDialog() {
  return renderWithProviders(<ThemeStateHarness />);
}

describe("SettingsDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(commands.getEnvironmentState).mockResolvedValue({
      status: "ok",
      data: {
        probing: false,
        tools: [
          {
            tool: "claude",
            availability: "installed",
            installationVersion: "2.1.217",
            installationProbeDiagnostic: null,
          },
          {
            tool: "codex",
            availability: "unavailable",
            installationVersion: null,
            installationProbeDiagnostic: null,
          },
        ],
      },
    });
    vi.mocked(commands.refreshEnvironment).mockResolvedValue({
      status: "ok",
      data: { probing: false, tools: [] },
    });
  });

  it("工具检测区块展示检测结果，点击重新检测调用刷新命令", async () => {
    renderDialog();
    const list = await screen.findByRole("list", { name: "工具检测结果" });
    expect(list).toHaveTextContent("Claude：已检测到 2.1.217");
    expect(list).toHaveTextContent("Codex：未检测到");
    fireEvent.click(screen.getByRole("button", { name: "重新检测工具" }));
    await waitFor(() =>
      expect(commands.refreshEnvironment).toHaveBeenCalledTimes(1),
    );
    expect(await screen.findByText("已重新检测工具环境。")).toHaveAttribute(
      "role",
      "status",
    );
  });

  it("open 为 false 时不渲染对话框，也不请求设置", () => {
    renderDialog({ open: false });

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(commands.getAppSettings).not.toHaveBeenCalled();
  });

  it("不再展示应用方式或直接应用开关", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "codex"] } satisfies AppSettingsDto,
    });
    renderDialog();

    expect(
      await screen.findByRole("heading", { name: "启用的工具" }),
    ).toBeVisible();
    expect(screen.queryByText("应用方式")).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/直接应用/)).not.toBeInTheDocument();
  });

  it("设置读取失败时展示错误且不渲染勾选框", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "error",
      error: {
        code: "DATABASE_ERROR",
        message: "本地数据库操作失败",
        recoverable: false,
      },
    });
    renderDialog();

    expect(await screen.findByRole("alert")).toBeVisible();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });

  it("点击完成按钮触发 onClose", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "codex"] } satisfies AppSettingsDto,
    });
    const onClose = vi.fn();
    renderDialog({ onClose });

    fireEvent.click(await screen.findByRole("button", { name: "完成" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("启用的工具区块默认勾选 Claude 与 Codex，Cursor 未勾选", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "codex"] } satisfies AppSettingsDto,
    });
    renderDialog();

    expect(
      await screen.findByRole("heading", { name: "启用的工具" }),
    ).toBeVisible();
    expect(
      await screen.findByRole("checkbox", { name: "Claude" }),
    ).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Codex" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Cursor" })).not.toBeChecked();
  });

  it("勾选 Cursor 后只提交按固定顺序的启用工具", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "codex"] } satisfies AppSettingsDto,
    });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        enabledTools: ["claude", "codex", "cursor"],
      } satisfies AppSettingsDto,
    });
    renderDialog();

    fireEvent.click(await screen.findByRole("checkbox", { name: "Cursor" }));
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        enabledTools: ["claude", "codex", "cursor"],
      }),
    );
  });

  it("取消 Codex 时提交剩余启用工具", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        enabledTools: ["claude", "codex", "cursor"],
      } satisfies AppSettingsDto,
    });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "cursor"] } satisfies AppSettingsDto,
    });
    renderDialog();

    fireEvent.click(await screen.findByRole("checkbox", { name: "Codex" }));
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        enabledTools: ["claude", "cursor"],
      }),
    );
  });
});

describe("SettingsDialog 外观模式切换", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { enabledTools: ["claude", "codex"] } satisfies AppSettingsDto,
    });
  });

  it("默认选中跟随系统，三态按钮均暴露 aria-pressed 与 title", async () => {
    renderDialog();

    await screen.findByRole("heading", { name: "启用的工具" });
    expect(screen.getByRole("button", { name: "亮色模式" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByRole("button", { name: "暗色模式" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    const systemButton = screen.getByRole("button", {
      name: "跟随系统外观",
    });
    expect(systemButton).toHaveAttribute("aria-pressed", "true");
    expect(systemButton).toHaveAttribute("title", "跟随系统外观");
  });

  it("点击暗色后回调 dark 且按钮呈选中态", async () => {
    const onThemePreferenceChange = vi.fn();
    renderDialog({ onThemePreferenceChange });

    await screen.findByRole("heading", { name: "启用的工具" });
    fireEvent.click(screen.getByRole("button", { name: "暗色模式" }));
    expect(onThemePreferenceChange).toHaveBeenCalledWith("dark");
  });

  it("父组件状态更新后选中态随之切换", async () => {
    renderThemedDialog();

    await screen.findByRole("heading", { name: "启用的工具" });
    fireEvent.click(screen.getByRole("button", { name: "暗色模式" }));
    expect(screen.getByRole("button", { name: "暗色模式" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(
      screen.getByRole("button", { name: "跟随系统外观" }),
    ).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(screen.getByRole("button", { name: "亮色模式" }));
    expect(screen.getByRole("button", { name: "亮色模式" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "暗色模式" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });
});
