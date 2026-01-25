# Contributing to Tambourine Voice Dictation

Thanks for your interest in contributing to Tambourine! This guide will help you get started.

## Development Setup

### Prerequisites

- **Rust** (use `rustup`)
- **Node.js 22+** and **pnpm**
- **Python 3.13+** and **uv**
- **Linux only**: `libwebkit2gtk-4.1-dev`, `build-essential`, `libxdo-dev`, `libssl-dev`, and other Tauri dependencies

### Server Setup

```bash
cd server
cp .env.example .env   # Add your API keys
uv sync                # Install dependencies
uv run python main.py  # Start the server (default: localhost:8765)
```

### App Setup

```bash
cd app
pnpm install   # Install dependencies
pnpm dev       # Start Tauri app in dev mode
```

## Code Quality

Pre-commit hooks automatically run linting, formatting, and type checking on commit. You can also run checks manually:

### TypeScript (app/)

```bash
pnpm lint       # Biome linting with auto-fix
pnpm typecheck  # TypeScript type checking
pnpm knip       # Detect unused code
pnpm check      # Run all checks (lint, typecheck, knip, test, cargo)
```

### Python (server/)

```bash
uv run ruff check --fix  # Linting with auto-fix
uv run ruff format       # Code formatting
uv run ty check          # Type checking
```

### Rust (app/src-tauri/)

```bash
cargo clippy --all-targets --all-features  # Linting
cargo fmt                                   # Formatting
```

Or use the pnpm wrapper from the app directory:

```bash
pnpm cargo:clippy
pnpm cargo:fmt
pnpm cargo        # Run all Rust checks
```

## Testing

```bash
# TypeScript
cd app && pnpm test

# Python
cd server && uv run pytest

# Rust
cd app && pnpm cargo:test
# or: cd app/src-tauri && cargo test
```

## Commit Conventions

Use descriptive commit messages with a type prefix:

- `Feat:` New features
- `Fix:` Bug fixes
- `Chore:` Maintenance, dependency updates
- `Docs:` Documentation changes
- `Refactor:` Code refactoring without behavior changes

Example: `Feat: add support for Azure Speech provider`

## Code Style & Philosophy

### Typing & Pattern Matching

- Prefer **explicit types** over raw dicts—make invalid states unrepresentable where practical
- Prefer **typed variants over string literals** when the set of valid values is known.
- Use **exhaustive pattern matching** (`match` in Python and Rust, `ts-pattern` in TypeScript) so the type checker can verify all cases are handled
- Structure types to enable exhaustive matching when handling variants

### Forward Compatibility

Client and server should evolve independently:

- **Unknown values**: Parse to an explicit `Unknown*` variant (never `None`), log at warn level, preserve raw data, gracefully ignore instead of raising exception

### Self-Documenting Code

- **Verbose naming**: Variable and function naming should read like documentation
- **Strategic comments**: Only for non-obvious logic or architectural decisions; avoid restating what code shows

## Pull Request Process

1. Fork the repository and create a feature branch from `main`
2. Make your changes and ensure all checks pass (`pnpm check` in app, CI will run server checks)
3. Write clear commit messages following the conventions above
4. Submit a pull request to `main` with a description of your changes

## Community Guidelines

Be respectful and constructive in all interactions. We're building this together and value contributions of all kinds—code, documentation, bug reports, and feature suggestions.

## Adding New Providers

STT and LLM providers are defined in `server/services/provider_registry.py`:

1. Add enum value to `STTProviderId` or `LLMProviderId` in `protocol/providers.py`
2. Import the pipecat service class in `provider_registry.py`
3. Add a provider config entry to `STT_PROVIDERS` or `LLM_PROVIDERS`
4. Add the environment variable to `.env.example`

See existing providers for credential mapper patterns.

## Questions?

Open an issue or join the discussion on the project's GitHub page.
