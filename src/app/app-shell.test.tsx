/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AppShell } from "@/app/app-shell";
import { commands } from "@/bindings/commands";
import { themeStorageKey } from "@/components/use-theme";

vi.mock("@/bindings/commands", () => ({
  commands: {
    listProjects: vi.fn(),
    getAppSettings: vi.fn(),
    updateAppSettings: vi.fn(),
  },
}));

function renderShell() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <AppShell />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("AppShell 侧边栏设置入口", () => {
  beforeEach(() => {
    vi.mocked(commands.listProjects).mockReset();
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [],
    });
    vi.mocked(commands.getAppSettings).mockReset();
    vi.mocked(commands.getAppSettings).mockResolvedValue({
      status: "ok",
      data: { applyMode: "preview_confirm" },
    });
    localStorage.clear();
    document.documentElement.classList.remove("dark");
  });

  afterEach(cleanup);

  it("设置不再是一级导航链接，而是左下角的图标按钮", () => {
    renderShell();

    expect(
      screen.queryByRole("link", { name: "设置" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute(
      "aria-haspopup",
      "dialog",
    );
  });

  it("点击后打开设置对话框，关闭后消失", async () => {
    renderShell();

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    expect(await screen.findByRole("dialog", { name: "设置" })).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("顶栏不再渲染外观切换，外观切换位于设置弹窗内", async () => {
    renderShell();

    expect(
      screen.queryByRole("group", { name: "外观模式" }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const group = await screen.findByRole("group", { name: "外观模式" });
    expect(group).toBeVisible();
    expect(
      screen.getByRole("button", { name: "跟随系统外观" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

  it("在设置弹窗中切换外观会立即挂 dark class 并持久化", async () => {
    renderShell();

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "暗色模式" }));
    expect(screen.getByRole("button", { name: "暗色模式" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(localStorage.getItem(themeStorageKey)).toBe("dark");

    fireEvent.click(screen.getByRole("button", { name: "亮色模式" }));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(localStorage.getItem(themeStorageKey)).toBe("light");
  });
});
