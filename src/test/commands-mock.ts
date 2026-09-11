import { vi } from "vitest";
import type { AppError, Result } from "@/bindings/commands";

type CommandFunction = (...args: never[]) => unknown;

export function mockCommands<T extends Record<string, CommandFunction>>(
  actual: T,
) {
  // 只替换命令对象的自有键，避免把原型链上的成员也 mock 掉。
  return Object.fromEntries(
    Object.keys(actual)
      .filter((key) => Object.hasOwn(actual, key))
      .map((key) => [key, vi.fn()]),
  );
}

export function okResult<T>(data: T): Result<T, AppError> {
  return { status: "ok", data };
}
export function errResult(error: AppError): Result<never, AppError> {
  return { status: "error", error };
}
