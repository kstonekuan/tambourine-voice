# Contributing to Tambourine Voice Dictation

Thanks for your interest in contributing to Tambourine! This guide will help you get started.

If you find Tambourine useful, consider giving the repo a star — it helps others discover the project and keeps contributors motivated. You can also join the [Discord server](https://discord.gg/dUyuXWVJ2a) to connect with the community.

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

### Install Git Hooks

```bash
# Install pre-commit (one-time)
uv tool install pre-commit

# Enable git hooks for this repository
pre-commit install
```

## Code Quality

After installing hooks, pre-commit will automatically run linting, formatting, and type checking on commit. You can also run checks manually:

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
- Prefer **shared internal functions over factory patterns** when extracting common logic from hooks or functions—keep each export explicitly defined for better IDE navigation and readability

### Forward Compatibility

Client and server should evolve independently:

- **Unknown values**: Parse to an explicit `Unknown*` variant (never `None`), log at warn level, preserve raw data, gracefully ignore instead of raising exception

### Self-Documenting Code

- **Verbose naming**: Variable and function naming should read like documentation
- **Strategic comments**: Only for non-obvious logic or architectural decisions; avoid restating what code shows

### Test Writing Standards

- Prioritize **business behavior** and user-visible outcomes over implementation details.
- Test our own domain logic (state transitions, message parsing, defaults, fallbacks), not third-party library internals.
- Prefer real typed inputs/outputs and avoid mocking behavior by default.
- Use mocks only when truly necessary, primarily at unstable boundaries (network, filesystem, time, OS integrations) for determinism.
- Keep tests resilient to refactors: assert on externally meaningful behavior, not private call sequences.

### Error Handling (Rust)

Scope: this section applies only to Rust code in `app/src-tauri`.

- Model expected or handled outcomes as typed variants (enums), not through the error channel.
- Reserve the error channel for unexpected or unrecoverable failures.
- For internal unexpected-failure paths, use `anyhow::Result`.
- Add `.context(...)` or `.with_context(...)` at fallible boundaries to preserve failure context.
- For Tauri command and external interface boundaries, choose the boundary error shape intentionally on a case-by-case basis.
- Keep boundary behavior stable unless a change is intentional and justified in the PR.

## Pull Request Process

1. Fork the repository and create a feature branch from `main`
2. Make your changes and ensure all checks pass (`pnpm check` in app, CI will run server checks)
    * Pre-commit hooks run automatically and will auto-fix most formatting issues and check for linting / type issues
    * If a hook fails, you can review the output, stage the auto-fixed files, and commit again
3. Write clear commit messages following the conventions above
4. Submit a pull request to `main` with a description of your changes

## Community Guidelines

Be respectful and constructive in all interactions. We're building this together and value contributions of all kinds—code, documentation, bug reports, and feature suggestions.

## Adding New Providers

STT and LLM providers are defined in `server/services/provider_registry.py`:

1. Add enum value to `STTProviderId` or `LLMProviderId` in `server/protocol/providers.py`
2. Import the pipecat service class in `server/services/provider_registry.py`
3. Add a provider config entry to `STT_PROVIDERS` or `LLM_PROVIDERS`
4. Add the environment variable to `.env.example`

See existing providers for credential mapper patterns.

## Examples

The `examples/` directory contains pre-built prompt configurations for different domains. Community contributions are welcome.

### Creating a New Example

1. Customize your prompts in **Settings > LLM Formatting Prompt**
2. Go to **Settings > Data Management** and click Export
3. Copy the 3 `.md` files to a new directory in `examples/` (use lowercase with hyphens, e.g., `medical-transcription`)
4. Test by importing and performing dictation

## Questions?

Open an issue, start a [GitHub Discussion](https://github.com/kstonekuan/tambourine-voice/discussions), or join the [Discord server](https://discord.gg/dUyuXWVJ2a).
