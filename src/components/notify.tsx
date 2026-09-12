import {
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type PropsWithChildren,
} from "react";

import { cn } from "@/lib/utils";
import { toneClass } from "@/lib/tone-class";
import {
  NotifyContext,
  notifyDurationMs,
  type Notification,
  type NotifyMessage,
} from "@/components/notify-context";

export function NotifyProvider({ children }: PropsWithChildren) {
  const [notifications, setNotifications] = useState<Notification[]>([]);
  const timersRef = useRef(
    new Map<number, ReturnType<typeof globalThis.setTimeout>>(),
  );

  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const timeoutId of timers.values()) {
        globalThis.clearTimeout(timeoutId);
      }
      timers.clear();
    };
  }, []);

  const notify = useCallback((next: NotifyMessage) => {
    const id = Date.now() + Math.random();
    setNotifications((current) => [...current, { ...next, id }]);
    const timeoutId = globalThis.setTimeout(() => {
      timersRef.current.delete(id);
      setNotifications((current) => current.filter((item) => item.id !== id));
    }, notifyDurationMs);
    timersRef.current.set(id, timeoutId);
  }, []);

  const clear = useCallback(() => {
    for (const timeoutId of timersRef.current.values()) {
      globalThis.clearTimeout(timeoutId);
    }
    timersRef.current.clear();
    setNotifications([]);
  }, []);

  return (
    <NotifyContext.Provider value={{ notifications, notify, clear }}>
      {children}
      <NotifyViewport notifications={notifications} />
    </NotifyContext.Provider>
  );
}

interface NotifyProps {
  notification: NotifyMessage | null;
}

export function Notify({ notification }: NotifyProps) {
  if (!notification) return null;

  const failure = notification.kind === "error";

  return (
    <div
      role={failure ? "alert" : "status"}
      aria-atomic="true"
      className={cn(
        "rounded-dialog w-full border p-4 text-sm shadow-lg",
        toneClass(failure ? "destructive" : "success"),
      )}
    >
      {notification.message}
    </div>
  );
}

export function NotifyViewport({
  notifications,
}: {
  notifications?: readonly Notification[];
}) {
  const context = useContext(NotifyContext);
  const visible = notifications ?? context?.notifications ?? [];
  return (
    <div
      aria-label="通知"
      className="fixed top-14 right-4 z-[60] flex w-[min(calc(100vw-2rem),24rem)] flex-col gap-3"
    >
      {visible.map((notification) => (
        <Notify key={notification.id} notification={notification} />
      ))}
    </div>
  );
}
