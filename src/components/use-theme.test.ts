import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  applyThemeFromStorage,
  themeStorageKey,
  useTheme,
} from "@/components/use-theme";

interface FakeMediaQueryList {
  matches: boolean;
  addEventListener: (
    type: string,
    listener: (event: { matches: boolean }) => void,
  ) => void;
  removeEventListener: (
    type: string,
    listener: (event: { matches: boolean }) => void,
  ) => void;
}

function stubMatchMedia(initialMatches: boolean) {
  const listeners = new Set<(event: { matches: boolean }) => void>();
  const mediaQueryList: FakeMediaQueryList = {
    matches: initialMatches,
    addEventListener: (_type, listener) => {
      listeners.add(listener);
    },
    removeEventListener: (_type, listener) => {
      listeners.delete(listener);
    },
  };
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => mediaQueryList),
  );
  return {
    listenerCount: () => listeners.size,
    emit(nextMatches: boolean) {
      mediaQueryList.matches = nextMatches;
      for (const listener of listeners) {
        listener({ matches: nextMatches });
      }
    },
  };
}

function resetDocumentTheme() {
  document.documentElement.classList.remove("dark");
}

describe("useTheme", () => {
  beforeEach(() => {
    localStorage.clear();
    resetDocumentTheme();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    resetDocumentTheme();
  });

  it("初始化读取持久化偏好，并在 documentElement 上应用 dark class", () => {
    localStorage.setItem(themeStorageKey, "dark");

    const { result } = renderHook(() => useTheme());

    expect(result.current.preference).toBe("dark");
    expect(result.current.resolvedTheme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("存储缺失或值非法时回退跟随系统（jsdom 无 matchMedia 时按亮色解析）", () => {
    localStorage.setItem(themeStorageKey, "blue");

    const { result } = renderHook(() => useTheme());

    expect(result.current.preference).toBe("system");
    expect(result.current.resolvedTheme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("setPreference 立即切换 class 并持久化到 localStorage", () => {
    localStorage.setItem(themeStorageKey, "dark");
    const { result } = renderHook(() => useTheme());
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    act(() => {
      result.current.setPreference("light");
    });

    expect(result.current.preference).toBe("light");
    expect(result.current.resolvedTheme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(localStorage.getItem(themeStorageKey)).toBe("light");
  });

  it("system 模式订阅 matchMedia change，系统外观变化实时跟随", () => {
    const media = stubMatchMedia(false);

    const { result } = renderHook(() => useTheme());
    expect(result.current.preference).toBe("system");
    expect(media.listenerCount()).toBe(1);

    act(() => {
      media.emit(true);
    });

    expect(result.current.resolvedTheme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    act(() => {
      media.emit(false);
    });

    expect(result.current.resolvedTheme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("离开 system 模式时移除 matchMedia 监听", () => {
    const media = stubMatchMedia(false);
    const { result } = renderHook(() => useTheme());
    expect(media.listenerCount()).toBe(1);

    act(() => {
      result.current.setPreference("dark");
    });

    expect(media.listenerCount()).toBe(0);
    expect(result.current.resolvedTheme).toBe("dark");
  });

  it("卸载时移除 matchMedia 监听", () => {
    const media = stubMatchMedia(false);

    const { unmount } = renderHook(() => useTheme());
    expect(media.listenerCount()).toBe(1);

    unmount();

    expect(media.listenerCount()).toBe(0);
  });

  it("applyThemeFromStorage 在渲染前按持久化偏好挂 dark class", () => {
    localStorage.setItem(themeStorageKey, "dark");
    applyThemeFromStorage();
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    localStorage.setItem(themeStorageKey, "light");
    applyThemeFromStorage();
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    localStorage.setItem(themeStorageKey, "invalid");
    applyThemeFromStorage();
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});
