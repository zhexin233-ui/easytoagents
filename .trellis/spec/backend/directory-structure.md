# Directory Structure

> How backend code is organized in this project.

---

## Overview

The backend is a single Rust crate under `src-tauri/`. Tauri commands are thin
IPC adapters, domain modules own validation and orchestration, `db/` owns
SQLite persistence, and tool-specific native configuration behavior lives
behind adapters and focused service modules.

---

## Directory Layout

```
src-tauri/
├── build.rs                     # Tauri build integration
├── examples/
│   └── export-bindings.rs       # Specta TypeScript binding generator
├── src/
│   ├── adapters/                # Claude/Codex target discovery and rendering
│   ├── app/                     # App paths, environment and shared state
│   ├── commands/                # Tauri/Specta IPC boundary
│   ├── db/
│   │   ├── migrations/         # Ordered embedded SQLite migrations
│   │   ├── mcp.rs
│   │   ├── profiles.rs
│   │   ├── projects.rs
│   │   └── skills.rs
│   ├── domain/                  # Shared domain types and validation
│   ├── error.rs                 # Stable cross-layer error contract
│   ├── git/                     # Read-only Git inspection
│   ├── mcp/                     # MCP central intent and synchronization
│   ├── profiles/                # Provider/prompt profile services
│   ├── projects/                # Project registration and scanning
│   ├── security/                # Redaction and path safety
│   ├── skills/                  # Skill import and synchronization
│   ├── sync/                    # Preview, apply, snapshot and recovery engine
│   ├── lib.rs                   # Modules, command registration and app setup
│   └── main.rs                  # Desktop entry point
└── tests/                       # Crate-level integration tests
```

---

## Module Organization

- Keep `src/commands/*.rs` thin: annotate commands with `#[tauri::command]`
  and `#[specta::specta]`, acquire the required `AppState` lock, and delegate
  to a domain/service module.
- Put SQLite statements and row decoding in `src/db/<domain>.rs`. Keep
  cross-record validation and native synchronization orchestration in the
  corresponding top-level domain module.
- Add native-format discovery, scanning and rendering behind `adapters/`; do
  not embed Claude/Codex file parsing in commands or UI-facing DTOs.
- Keep shared security behavior in `security/`, stable RPC failures in
  `error.rs`, and durable external writes/recovery in `sync/`.
- Co-locate focused unit tests in `#[cfg(test)] mod tests`. Use `src-tauri/tests/`
  when a test crosses command registration, generated bindings, SQLite, or
  several service modules.

---

## Naming Conventions

- Rust modules, files and functions use `snake_case`; structs, enums and DTOs
  use `UpperCamelCase`.
- Command modules and database modules use the domain noun, such as
  `commands/projects.rs` and `db/projects.rs`.
- SQLite migrations use a zero-padded sequence and descriptive suffix, for
  example `0004_project_registration.sql`, and must also be registered in the
  ordered `MIGRATIONS` array.
- RPC input/output types end in `Input`, `Dto`, `Plan`, or `Result` where those
  roles apply. Generated TypeScript names come from these Rust types.

---

## Examples

- `src-tauri/src/commands/projects.rs` demonstrates a thin typed command layer:

  ```rust
  #[tauri::command]
  #[specta::specta]
  pub fn get_project(
      state: State<'_, AppState>,
      id: String,
  ) -> Result<ProjectDto, AppError> {
      let database = state.database().lock().map_err(|_| state_lock_error())?;
      projects::get_project(&database, state.environment()?, &id)
  }
  ```

- `src-tauri/src/db/mod.rs` owns connection setup and the ordered embedded
  migration list. Entity-specific persistence is split into `db/mcp.rs`,
  `db/profiles.rs`, `db/projects.rs`, and `db/skills.rs`.
- `src-tauri/src/sync/` is the reference for a cross-cutting module with pure
  preview computation, durable apply, snapshots, journals, and recovery tests.
- `src-tauri/tests/command_smoke.rs`, `bindings.rs`, and `phase8_e2e.rs`
  demonstrate command registration, generated-binding, and end-to-end tests.

## Forbidden Patterns

- Do not put SQL, native-file parsing, or durable writes directly in a Tauri
  command.
- Do not introduce an untyped catch-all `utils` module. Keep helpers with the
  domain or boundary whose invariants they enforce.
- Do not hand-edit `src/bindings/commands.ts`; update Rust command/DTO types and
  run `pnpm bindings:generate`.
