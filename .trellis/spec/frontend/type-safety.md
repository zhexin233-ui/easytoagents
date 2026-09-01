# Type Safety

> Type safety patterns in this project.

---

## Overview

The frontend uses strict TypeScript with generated Specta bindings as the
Rust/Tauri contract. `strict`, `noUncheckedIndexedAccess`,
`exactOptionalPropertyTypes`, `isolatedModules`, and `noEmit` are enabled.
ESLint forbids explicit `any`, type assertions, and floating promises in
handwritten TypeScript.

---

## Type Organization

- Import command inputs, DTOs, enums, and result types from
  `src/bindings/commands.ts` with `import type` where possible.
- Define component props next to the component. Keep a feature-only union or
  helper type in its feature file until it is genuinely shared.
- Do not duplicate a Rust/Specta type in `src/lib` or a page. API modules should
  transform errors and create query options without weakening DTOs.
- Regenerate bindings with `pnpm bindings:generate` after changing Rust
  commands or DTOs, then run `pnpm bindings:check`.

---

## Validation

The frontend has no Zod/Yup-style validation layer. Runtime domain validation
is backend-owned and returns the generated `Result<T, AppError>` contract.
Frontend boundary code must:

- inspect the generated result discriminant in `unwrapResult`;
- narrow `unknown` errors with `instanceof` before reading fields;
- narrow optional detail values with `typeof`;
- render stable error codes/messages rather than asserting a payload shape.

---

## Common Patterns

- Use generic helpers only when they preserve the generated contract. The
  reference is `unwrapResult<T>(result: Result<T, AppError>): T`.
- Build query keys with `as const` so TanStack Query retains tuple identity.
- Use optional chaining, explicit guards, and methods such as `at(-1)` in code
  affected by `noUncheckedIndexedAccess`.
- Use discriminated unions and exhaustive `switch` statements for generated or
  local state values.

```ts
export function unwrapResult<T>(result: Result<T, AppError>): T {
  if (result.status === "error") {
    throw new ProfileRpcError(result.error);
  }
  return result.data;
}
```

---

## Forbidden Patterns

- No explicit `any` or handwritten type assertions. Generated bindings are the
  only current exception; `eslint.config.js` excludes `src/bindings/**` because
  the generator owns those casts.
- No plain `string` substitute for generated `Tool`, `ArtifactKind`, status,
  error-code, or input unions.
- No locally reconstructed RPC payload or DTO cast in a component.
- Capability-aware tool metadata must preserve the exact key-specific return type so a
  caller cannot require `profileRoute!` or another assertion for Cursor. Profile routes
  are only present for the compile-time `PROFILE_TOOLS` subset; MCP/Skill selectors use
  their own capability subsets.
- No unhandled promise. Await it, return it, or deliberately handle rejection.
- Do not edit generated bindings to silence a type error; fix the Rust contract,
  generator, or handwritten caller.
