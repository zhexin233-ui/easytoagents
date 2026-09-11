import { createContext } from "react";

export interface NotifyMessage {
  kind: "success" | "error";
  message: string;
}

export interface Notification extends NotifyMessage {
  id: number;
}

export interface NotifyContextValue {
  notifications: Notification[];
  notify: (next: NotifyMessage) => void;
  clear: () => void;
}

export const NotifyContext = createContext<NotifyContextValue | null>(null);

export const notifyDurationMs = 3_000;
