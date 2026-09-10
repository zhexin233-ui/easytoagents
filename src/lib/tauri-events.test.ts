import { afterEach, describe, expect, it, vi } from "vitest";

const tauriEvent = vi.hoisted(() => ({
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: tauriEvent.listen,
}));

import {
  ENVIRONMENT_READY_EVENT,
  subscribeEnvironmentReady,
} from "@/lib/tauri-events";

describe("subscribeEnvironmentReady", () => {
  afterEach(() => {
    tauriEvent.listen.mockReset();
  });

  it("把事件 payload 转交给处理函数，并在取消订阅时停止监听", async () => {
    const stop = vi.fn();
    const captured: Array<(event: { payload: boolean }) => void> = [];
    tauriEvent.listen.mockImplementation(
      (name: string, handler: (event: { payload: boolean }) => void) => {
        expect(name).toBe(ENVIRONMENT_READY_EVENT);
        captured.push(handler);
        return Promise.resolve(stop);
      },
    );
    const handler = vi.fn();
    const unsubscribe = subscribeEnvironmentReady(handler);
    await Promise.resolve();
    const listener = captured[0];
    if (!listener) throw new Error("listen 未注册处理函数");
    listener({ payload: true });
    expect(handler).toHaveBeenCalledWith(true);
    unsubscribe();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("在取消订阅之后才完成注册时立即停止监听", async () => {
    const stop = vi.fn();
    const resolvers: Array<(value: () => void) => void> = [];
    tauriEvent.listen.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          resolvers.push(resolve);
        }),
    );
    const unsubscribe = subscribeEnvironmentReady(() => {});
    unsubscribe();
    const resolveListen = resolvers[0];
    if (!resolveListen) throw new Error("listen 未被调用");
    resolveListen(stop);
    await Promise.resolve();
    await Promise.resolve();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("没有 Tauri 事件桥时静默忽略注册失败", async () => {
    tauriEvent.listen.mockRejectedValue(new Error("no ipc"));
    const unsubscribe = subscribeEnvironmentReady(() => {});
    await Promise.resolve();
    await Promise.resolve();
    expect(() => unsubscribe()).not.toThrow();
  });
});
