import { listen } from "@tauri-apps/api/event";

/** 与 `commands/environment.rs` 的 `ENVIRONMENT_READY_EVENT` 保持一致。 */
export const ENVIRONMENT_READY_EVENT = "environment-ready";

/**
 * 订阅后端探测完成事件；返回取消订阅函数。
 * 非 Tauri 宿主（如 vitest 的 jsdom）没有事件桥，订阅失败时静默忽略。
 */
export function subscribeEnvironmentReady(
  handler: (succeeded: boolean) => void,
): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;
  listen<boolean>(ENVIRONMENT_READY_EVENT, (event) => handler(event.payload))
    .then((stop) => {
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    })
    .catch(() => {
      // 没有 Tauri IPC 时 listen 会拒绝；页面仍可通过手动刷新获得最新状态。
    });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
