# AI Agent Guidelines

This file provides guidance for AI coding agents (Claude Code, Cursor, GitHub Copilot, etc.) working on this codebase.

## Required Reading

Before making any changes, read and follow the project guidelines in [CONTRIBUTING.md](./CONTRIBUTING.md).

## Quick Reference

### Project Structure

- `app/` - Tauri desktop app (TypeScript + Rust)
- `server/` - Python backend server

### Before Committing

Run all checks to ensure code quality:

```bash
# TypeScript (from app/)
pnpm check

# Python (from server/)
uv run ruff check --fix && uv run ruff format && uv run ty check

# Rust (from app/)
pnpm cargo
```

### Key Principles

1. **Explicit types** - Prefer typed structures over raw dicts
2. **Exhaustive matching** - Use pattern matching for variants
3. **Verbose naming** - Names should read like documentation
4. **Forward compatibility** - Handle unknown messages/values gracefully
