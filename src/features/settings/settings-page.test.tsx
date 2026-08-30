/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type AppSettingsDto } from "@/bindings/commands";
import { SettingsPage } from "@/features/settings/settings-page";

vi.mock("@/bindings/commands", () => ({
  commands: {
    getAppSettings: vi.fn(),
    updateAppSettings: vi.fn(),
  },
}));

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <SettingsPage />
    </QueryClientProvider>,
  );
}

async function applyModeCheckbox() {
  return screen.findByRole("checkbox", {
    name: "直接应用（跳过预览确认对话框）",
  });
}

afterEach(cleanup);

describe("SettingsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("默认渲染为预览确认模式，勾选后保存 direct 并回读", async () => {
    vi.mocked(commands.getAppSettings)
      .mockResolvedValueOnce({
        status: "ok",
        data: { applyMode: "preview_confirm" } satisfies AppSettingsDto,
      })
      .mockResolvedValueOnce({
        status: "ok",
        data: { applyMode: "direct" } satisfies AppSettingsDto,
      });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" } satisfies AppSettingsDto,
    });
    renderPage();

    const checkbox = await applyModeCheckbox();
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        applyMode: "direct",
      }),
    );
    await waitFor(() => expect(checkbox).toBeChecked());
  });

  it("已开启直接应用时勾选框呈选中态，取消勾选保存 preview_confirm", async () => {
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "direct" } satisfies AppSettingsDto,
    });
    vi.mocked(commands.updateAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "preview_confirm" } satisfies AppSettingsDto,
    });
    renderPage();

    const checkbox = await applyModeCheckbox();
    expect(checkbox).toBeChecked();
    fireEvent.click(checkbox);
    await waitFor(() =>
      expect(commands.updateAppSettings).toHaveBeenCalledWith({
        applyMode: "preview_confirm",
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
    renderPage();

    expect(await screen.findByRole("alert")).toBeVisible();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });
});
