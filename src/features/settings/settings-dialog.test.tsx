/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type AppSettingsDto } from "@/bindings/commands";
import { SettingsDialog } from "@/features/settings/settings-dialog";
import type { ThemePreference } from "@/components/use-theme";

vi.mock("@/bindings/commands", () => ({
  commands: {
    getAppSettings: vi.fn(),
    updateAppSettings: vi.fn(),
  },
}));

function renderDialog(
  props: Partial<Parameters<typeof SettingsDialog>[0]> = {},
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <SettingsDialog
        open
        onClose={() => {}}
        themePreference="system"
        onThemePreferenceChange={() => {}}
        {...props}
      />
    </QueryClientProvider>,
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
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ThemeStateHarness />
    </QueryClientProvider>,
  );
}

async function applyModeCheckbox() {
  return screen.findByRole("checkbox", {
    name: "直接应用（跳过预览确认对话框）",
  });
}

afterEach(cleanup);

describe("SettingsDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("open 为 false 时不渲染对话框，也不请求设置", () => {
    renderDialog({ open: false });

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(commands.getAppSettings).not.toHaveBeenCalled();
  });

  it("默认渲染为预览确认模式，勾选后保存 direct 并回读", async () => {
    vi.mocked(commands.getAppSettings)
      .mockResolvedValueOnce({
        status: "ok",
        data: {
          applyMode: "preview_confirm",
          enabledTools: ["claude", "codex"],
        } satisfies AppSettingsDto,
      })
      .mockResolvedValueOnce({
        status: "ok",
        data: {
          applyMode: "direct",
          enabledTools: ["claude", "codex"],
        } satisfies AppSettingsDto,
      });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "direct",
        enabledTools: ["claude", "codex"],
      } satisfies AppSettingsDto,
    });
    renderDialog();

    const checkbox = await applyModeCheckbox();
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        applyMode: "direct",
        enabledTools: ["claude", "codex"],
      }),
    );
    await waitFor(() => expect(checkbox).toBeChecked());
  });

  it("已开启直接应用时勾选框呈选中态，取消勾选保存 preview_confirm", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "direct",
        enabledTools: ["claude", "codex"],
      } satisfies AppSettingsDto,
    });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      } satisfies AppSettingsDto,
    });
    renderDialog();

    const checkbox = await applyModeCheckbox();
    expect(checkbox).toBeChecked();
    fireEvent.click(checkbox);
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      }),
    );
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

  it("点击关闭按钮触发 onClose", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      } satisfies AppSettingsDto,
    });
    const onClose = vi.fn();
    renderDialog({ onClose });

    fireEvent.click(await screen.findByRole("button", { name: "关闭" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("启用的工具区块默认勾选 Claude 与 Codex，Cursor 未勾选", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      } satisfies AppSettingsDto,
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

  it("勾选 Cursor 后整包提交 applyMode 与按固定顺序的启用工具", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      } satisfies AppSettingsDto,
    });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex", "cursor"],
      } satisfies AppSettingsDto,
    });
    renderDialog();

    fireEvent.click(await screen.findByRole("checkbox", { name: "Cursor" }));
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex", "cursor"],
      }),
    );
  });

  it("取消 Codex 时保持 applyMode 并提交剩余启用工具", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "direct",
        enabledTools: ["claude", "codex", "cursor"],
      } satisfies AppSettingsDto,
    });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: {
        applyMode: "direct",
        enabledTools: ["claude", "cursor"],
      } satisfies AppSettingsDto,
    });
    renderDialog();

    fireEvent.click(await screen.findByRole("checkbox", { name: "Codex" }));
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        applyMode: "direct",
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
      data: {
        applyMode: "preview_confirm",
        enabledTools: ["claude", "codex"],
      } satisfies AppSettingsDto,
    });
  });

  it("默认选中跟随系统，三态按钮均暴露 aria-pressed 与 title", async () => {
    renderDialog();

    await applyModeCheckbox();
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

    await applyModeCheckbox();
    fireEvent.click(screen.getByRole("button", { name: "暗色模式" }));
    expect(onThemePreferenceChange).toHaveBeenCalledWith("dark");
  });

  it("父组件状态更新后选中态随之切换", async () => {
    renderThemedDialog();

    await applyModeCheckbox();
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
