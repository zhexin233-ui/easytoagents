# Frontend Development Guidelines

> Best practices for frontend development in this project.

---

## Overview

This directory records the conventions used by the React/Tauri frontend under
`src/`. Feature pages consume generated command bindings through typed query
helpers and keep native writes behind persisted preview flows.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | Current |
| [Component Guidelines](./component-guidelines.md) | Component patterns, props, composition | Current |
| [Hook Guidelines](./hook-guidelines.md) | Custom hooks, data fetching patterns | Current |
| [State Management](./state-management.md) | Local state, global state, server state | Current |
| [Quality Guidelines](./quality-guidelines.md) | Preview dialogs, direct-apply exceptions, project-native resources | Current |
| [Native MCP Import](../backend/mcp-import-guidelines.md) | Generated import DTOs, selection, query lifecycle and no implicit Apply | Current |
| [全局 Skills 导入](../backend/skill-import-guidelines.md) | 生成 DTO、检测选择、请求隔离、复制不接管与首次状态展示 | Current |
| [Type Safety](./type-safety.md) | Type patterns, validation | Current |

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
