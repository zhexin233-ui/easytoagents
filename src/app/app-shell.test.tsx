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

describe("AppShell 外观模式切换", () => {
  beforeEach(() => {
    vi.mocked(commands.listProjects).mockReset();
    vi.mocked(commands.listProjects).mockResolvedValue({
      status: "ok",
      data: [],
    });
    localStorage.clear();
    document.documentElement.classList.remove("dark");
  });

  afterEach(cleanup);

  it("默认选中跟随系统，三态按钮均暴露 aria-pressed 与 title", () => {
    renderShell();

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

  it("点击暗色后立即挂 dark class 并持久化；切回亮色后移除", () => {
    renderShell();

    fireEvent.click(screen.getByRole("button", { name: "暗色模式" }));
    expect(screen.getByRole("button", { name: "暗色模式" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "亮色模式" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(localStorage.getItem(themeStorageKey)).toBe("dark");

    fireEvent.click(screen.getByRole("button", { name: "亮色模式" }));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(localStorage.getItem(themeStorageKey)).toBe("light");
    expect(
      screen.getByRole("button", { name: "跟随系统外观" }),
    ).toHaveAttribute("aria-pressed", "false");
  });
});
