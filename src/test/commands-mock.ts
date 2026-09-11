import { vi } from "vitest";
import type { AppError, Result } from "@/bindings/commands";

type CommandFunction = (...args: never[]) => unknown;

export function mockCommands<T extends Record<string, CommandFunction>>(
  actual: T,
) {
  // eslint-disable-next-line @typescript-eslint/unbound-method -- 仅用于安全读取命令对象的自有键。
  const hasOwn = Object.prototype.hasOwnProperty;
  return Object.fromEntries(
    Object.keys(actual)
      .filter((key) => hasOwn.call(actual, key))
      .map((key) => [key, vi.fn()]),
  );
}

export function okResult<T>(data: T): Result<T, AppError> {
  return { status: "ok", data };
}
export function errResult(error: AppError): Result<never, AppError> {
  return { status: "error", error };
}
