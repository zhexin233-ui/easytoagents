# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

This directory records the conventions used by the Rust/Tauri backend under
`src-tauri/`. The guides describe the current codebase, including the absence
of a general-purpose logging pipeline.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | Current |
| [Database Guidelines](./database-guidelines.md) | ORM patterns, queries, migrations | Current |
| [Error Handling](./error-handling.md) | Error types, handling strategies | Current |
| [Quality Guidelines](./quality-guidelines.md) | Explicit discovery, preview and integration safety | Current |
| [Native MCP Import](./mcp-import-guidelines.md) | Explicit selection, private equality, atomic adoption and preview lifecycle | Current |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | Current |

---

## How to Fill These Guidelines

For each guideline file:

1. Document your project's **actual conventions** (not ideals)
2. Include **code examples** from your codebase
3. List **forbidden patterns** and why
4. Add **common mistakes** your team has made

The goal is to help AI assistants and new team members understand how YOUR project works.

---

**Language**: All documentation should be written in **English**.
