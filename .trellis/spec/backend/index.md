# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

This directory records the conventions used by the Rust/Tauri backend under
`src-tauri/`. The guides describe the current codebase, including its
structured tracing pipeline and private rolling log files.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | Current |
| [Database Guidelines](./database-guidelines.md) | ORM patterns, queries, migrations | Current |
| [Error Handling](./error-handling.md) | Error types, handling strategies | Current |
| [Quality Guidelines](./quality-guidelines.md) | Explicit discovery, preview, native-resource disable/restore, and integration safety | Current |
| [Native MCP Import](./mcp-import-guidelines.md) | Explicit selection, format compatibility, credential classification and atomic adoption | Current |
| [全局 Skills 导入](./skill-import-guidelines.md) | 显式来源、入口链接、内置排除、批量复制与首次状态合同 | Current |
| [OpenCode Adapter](./opencode-adapter-guidelines.md) | Explicit JSON/JSONC boundaries, Provider/MCP/Skills contracts, and unsupported Hooks | Current |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | Current |
| [Release Workflow](./release-workflow-guidelines.md) | Manual ARM64 DMG release contracts, validation, permissions, and retry safety | Current |

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
