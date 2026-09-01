import {
  act,
  cleanup,
  render,
  renderHook,
  screen,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Notify } from "@/components/notify";
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
