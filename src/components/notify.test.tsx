import {
  act,
  cleanup,
  render,
  renderHook,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Notify, NotifyProvider } from "@/components/notify";
import { notifyDurationMs, useNotify } from "@/components/use-notify";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Notify", () => {
  it("成功使用非打断状态播报，失败保留 alert 语义", () => {
    const { rerender } = render(
      <Notify notification={{ kind: "success", message: "同步成功" }} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("同步成功");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    rerender(<Notify notification={{ kind: "error", message: "同步失败" }} />);
    expect(screen.getByRole("alert")).toHaveTextContent("同步失败");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

describe("useNotify", () => {
  it("NotifyProvider 按队列保留连续通知，并分别自动消失", () => {
    vi.useFakeTimers();

    function Trigger() {
      const { notify } = useNotify();
      return (
        <>
          <button
            type="button"
            onClick={() => notify({ kind: "success", message: "第一条" })}
          >
            第一条
          </button>
          <button
            type="button"
            onClick={() => notify({ kind: "error", message: "第二条" })}
          >
            第二条
          </button>
        </>
      );
    }

    render(
      <NotifyProvider>
        <Trigger />
      </NotifyProvider>,
    );
    act(() => screen.getByRole("button", { name: "第一条" }).click());
    act(() => screen.getByRole("button", { name: "第二条" }).click());

    expect(screen.getByRole("status")).toHaveTextContent("第一条");
    expect(screen.getByRole("alert")).toHaveTextContent("第二条");

    act(() => {
      vi.advanceTimersByTime(notifyDurationMs);
    });
    const viewport = screen.getByLabelText("通知");
    expect(within(viewport).queryByText("第一条")).not.toBeInTheDocument();
    expect(within(viewport).queryByText("第二条")).not.toBeInTheDocument();
  });

  it("通知展示三秒后自动消失", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useNotify());

    act(() => {
      result.current.notify({ kind: "success", message: "同步成功" });
    });
    expect(result.current.notification).toEqual({
      kind: "success",
      message: "同步成功",
    });

    act(() => {
      vi.advanceTimersByTime(notifyDurationMs - 1);
    });
    expect(result.current.notification).not.toBeNull();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current.notification).toBeNull();
  });

  it("失败通知同样在三秒后自动消失", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useNotify());

    act(() => {
      result.current.notify({ kind: "error", message: "同步失败" });
    });
    expect(result.current.notification).toEqual({
      kind: "error",
      message: "同步失败",
    });

    act(() => {
      vi.advanceTimersByTime(notifyDurationMs);
    });
    expect(result.current.notification).toBeNull();
  });

  it("新通知替换当前通知并重新开始三秒计时", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useNotify());

    act(() => {
      result.current.notify({ kind: "success", message: "第一次" });
    });
    act(() => {
      vi.advanceTimersByTime(2_000);
    });
    act(() => {
      result.current.notify({ kind: "error", message: "第二次" });
    });
    expect(result.current.notification).toEqual({
      kind: "error",
      message: "第二次",
    });

    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(result.current.notification?.message).toBe("第二次");
    act(() => {
      vi.advanceTimersByTime(2_000);
    });
    expect(result.current.notification).toBeNull();
  });

  it("卸载时清理仍在等待的计时器", () => {
    vi.useFakeTimers();
    const { result, unmount } = renderHook(() => useNotify());

    act(() => {
      result.current.notify({ kind: "success", message: "等待消失" });
    });
    expect(vi.getTimerCount()).toBe(1);

    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
