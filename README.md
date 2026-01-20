<p align="center">
  <img src="app/src-tauri/icons/icon.png" alt="Tambourine" width="128" height="128">
</p>

# Tambourine

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-AGPL--3.0-blue" alt="License: AGPL-3.0"></a>
  <a href="https://discord.gg/dUyuXWVJ2a"><img src="https://img.shields.io/badge/Discord-Join-5865F2?logo=discord&logoColor=white" alt="Discord"></a>
  <a href="https://deepwiki.com/kstonekuan/tambourine-voice"><img src="https://deepwiki.com/badge.svg" alt="DeepWiki"></a>
</p>

Your personal voice interface for any app. Speak naturally and your words appear wherever your cursor is, powered by customizable AI voice dictation.

Open-source alternative to [Wispr Flow](https://wisprflow.ai), [Superwhisper](https://superwhisper.com), and [Willow](https://willowvoice.com).

> ⚠️ **Build in Progress**
> This project is under active development. Core features work well, but expect breaking changes to the code, architecture, and configuration as the project evolves.

> 🚀 **Hosted Service Coming Soon!**
> [Join the waitlist](https://docs.google.com/forms/d/e/1FAIpQLSf6JfLheFlBU-jAVMzgA7CKCcFb39fGZOaizKjxSBMCwSrVZg/viewform) to use Tambourine without running the server yourself.

<p align="center">
  <img src="assets/home.png" alt="Home" width="600">
</p>

<p align="center">
  <img src="assets/settings.png" alt="Settings" width="600">
</p>

<p align="center">
  <img src="assets/windows_notepad.gif" alt="Dictating into Windows Notepad" width="600">
</p>

## Why?

**Your voice, any app.** Tambourine gives you a universal voice-to-text interface that works everywhere: emails, messages, documents, code editors, terminals. Press a hotkey, speak, and your words are typed at your cursor. No copy-pasting, no app switching, no limitations.

**Speak at the speed of thought.** Typing averages 40-50 wpm, but speaking averages 130-160 wpm. Capture ideas before they slip away, and give your hands a break from the keyboard.

**AI that understands you.** Unlike raw transcription, Tambourine uses AI to format your speech into clean text—removing filler words, adding punctuation, and applying your personal dictionary for technical terms and proper nouns.

**Why not native dictation?** Built-in dictation is not personalized but Tambourine can be customized to your speaking and writing style, and with a personal dictionary for uncommon terms.

**Why not proprietary tools?** Unlike Wispr Flow or Superwhisper, this project gives you full control and transparency.

**Fully customizable.** This is your voice interface, built your way:
- **Choose your AI providers** — Pick your STT (Cartesia, Deepgram, AssemblyAI) and LLM (Cerebras, OpenAI, Anthropic), run fully local with Whisper and Ollama, or use any of [Pipecat's supported services](https://docs.pipecat.ai/server/services/supported-services)
- **Customize the formatting** — Modify prompts, add custom rules, build your personal dictionary
- **Extend freely** — Built on [Pipecat](https://github.com/pipecat-ai/pipecat)'s modular pipeline, fully open-source

## Platform Support

| Platform | Compatibility |
| -------- | ------------- |
| Windows  | ✅             |
| macOS    | ✅             |
| Linux    | ⚠️             |
| Android  | ❌             |
| iOS      | ❌             |

## Features

- **Dual-Mode Recording**
  - Hold-to-record: `` Ctrl+Alt+` `` - Hold to record, release to stop
  - Toggle mode: `Ctrl+Alt+Space` - Press to start, press again to stop
- **Real-time Speech-to-Text** - Fast transcription with configurable STT providers
- **LLM Text Formatting** - Removes filler words, adds punctuation using configurable LLM
- **Customizable Prompts** - Edit formatting rules, enable advanced features, add personal dictionary
- **In-App Provider Selection** - Switch STT and LLM providers without restarting
- **Automatic Typing** - Input text directly at focused position
- **Recording Overlay** - Floating visual indicator
- **Transcription History** - View and copy previous dictations
- **Paste Last Transcription** - Re-type previous dictation with `Ctrl+Alt+.`
- **Auto-Mute Audio** - Automatically mute system audio while dictating (Windows/macOS)
- **Misc.** - System tray integration, microphone selection, sound feedback, configure hotkeys

## Planned Features

- **Context-Aware Formatting** - Automatically detect which application is focused and tailor formatting accordingly. Email clients get proper salutations and sign-offs, messaging apps get casual formatting, code editors get syntax-aware output with proper casing and punctuation.
- **Voice-Driven Text Modification** - Highlight existing text and describe how to modify it. Select a paragraph and say "make this more formal" or "fix the grammar" to transform text in place.
- **Voice Shortcuts** - Create custom triggers that expand to full formatted text. Say "insert meeting link" to paste your scheduling URL, or "sign off" for your email signature.
- **Auto-Learning Dictionary** - Automatically learn new words, names, and terminology from your usage patterns rather than requiring manual dictionary entries.
- **Observability and Evaluation** - Integrate tooling from Pipecat and other voice agent frameworks to track transcription quality, latency metrics, and formatting accuracy. Use insights to continuously optimize your personal dictation workflow.
- **Hosted Service** - Optional cloud-hosted backend so you can use Tambourine without running the Python server locally.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Tauri App (app/)                       │
│  - Global hotkeys (Ctrl+Alt+Space, Ctrl+Alt+`)              │
│  - Rust backend for keyboard and audio controls             │
│  - React frontend with SmallWebRTC client                   │
│  - System tray with show/hide toggle                        │
└─────────────────────────────┬───────────────────────────────┘
                              │
                          API :8765
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  Python Server (server/)                    │
│  - Pipecat SmallWebRTC for audio streaming                  │
│  - STT providers (Cartesia, Deepgram, Groq, and more)       │
│  - LLM formatting (Cerebras, OpenAI, Anthropic, and more)   │
│  - Runtime config via WebRTC data channel (RTVI protocol)   │
│  - Returns cleaned text to app                              │
└─────────────────────────────────────────────────────────────┘
```

## Prerequisites

- Rust
- Node.js
- pnpm
- Python
- uv (Python package manager)

### Linux Dependencies

```bash
sudo apt-get install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libgtk-3-dev
```

## Permissions

### Microphone Access

When you first use Tambourine, your operating system will prompt you to grant microphone access. Accept this permission to enable voice dictation.

### macOS Accessibility Permissions

On macOS, Tambourine needs accessibility permissions to type text at your cursor position.

- **Running the built app**: Grant accessibility access to "Tambourine"
- **Running in development**: Grant accessibility access to the application you run the code from:
  - If running from VS Code: Add "Visual Studio Code"
  - If running from Terminal: Add "Terminal" (or your terminal app like iTerm2)

## Quick Start

### 1. Get API Keys

Sign up and get API keys for the providers you want to use. Some providers with generous free tiers as of this writing:
- Cartesia: https://cartesia.ai (STT)
- Cerebras: https://cloud.cerebras.ai (LLM)
- Gemini: https://aistudio.google.com (LLM)
- Groq: https://console.groq.com (STT/LLM)

**Or run fully local:**
```bash
ollama run llama3.2
```
Then in your `.env` file, set `OLLAMA_BASE_URL=http://localhost:11434` for local LLM and `WHISPER_ENABLED=true` for local STT.

### 2. Set Up the Server

```bash
cd server

# Copy environment template and add your API keys
cp .env.example .env

# Install dependencies
uv sync

# Start the server
uv run python main.py
```

### 3. Set Up the App

```bash
cd app

# Install dependencies
pnpm install

# Start development mode
pnpm dev
```

### 4. Usage

1. Start the server first (`uv run python main.py`)
2. Start the app (`pnpm dev`)
3. Use either shortcut:
   - **Toggle**: Press `Ctrl+Alt+Space` to start, press again to stop
   - **Hold**: Hold `` Ctrl+Alt+` `` while speaking, release to stop
4. Your cleaned text is typed at your cursor

## Server Commands

```bash
cd server

# Start server (default: localhost:8765)
uv run python main.py

# Start with custom host/port
uv run python main.py --host 0.0.0.0 --port 9000

# Enable verbose logging
uv run python main.py --verbose
```

## Docker Deployment

Run the server in Docker instead of installing Python dependencies locally.

```bash
cd server

# Copy environment template and add your API keys
cp .env.example .env

# Build and start the container
docker compose up --build -d

# View logs
docker compose logs -f

# Stop the container
docker compose down

# Update to latest code
docker compose down && docker compose up --build -d
```

The `.env` file is read at runtime (not baked into the image), so your API keys stay secure.

## App Commands

```bash
cd app

# Development
pnpm check         # Run all checks (lint + typecheck + knip + test + cargo)
pnpm dev           # Start Tauri app in dev mode

# Production Build
pnpm build         # Build for current platform
```

## Configuration

### Server Configuration (.env)

Copy `.env.example` to `.env` and add API keys for at least one STT and one LLM provider. See the example file for all supported providers including Deepgram, Cartesia, OpenAI, Anthropic, Cerebras, Groq, AWS, and more. Additional [Pipecat-supported providers](https://docs.pipecat.ai/server/services/supported-services) can be added easily.

### App Configuration

The app connects to `localhost:8765` by default via WebRTC. Settings are persisted locally and include:

- **Providers** - Select active STT and LLM providers from available options
- **Audio** - Microphone selection, sound feedback, auto-mute during recording
- **Hotkeys** - Customize toggle and hold-to-record shortcuts
- **LLM Formatting Prompt** - Three customizable sections:
  - Core Formatting Rules - Filler word removal, punctuation, capitalization
  - Advanced Features - Backtrack corrections ("scratch that"), list formatting
  - Personal Dictionary - Custom words

## Tech Stack

- **Desktop App:** Rust, Tauri
- **Frontend:** TypeScript, React, Vite
- **UI:** Mantine, Tailwind CSS
- **State Management:** Zustand, Tanstack Query, XState
- **Backend:** Python, FastAPI
- **Voice Pipeline:** Pipecat
- **Communications:** WebRTC
- **Validation:** Zod, Pydantic
- **Code Quality:** Biome, Ruff, Ty, Clippy

## Acknowledgments

Built with [Tauri](https://tauri.app/) for the cross-platform desktop app and [Pipecat](https://github.com/pipecat-ai/pipecat) for the modular voice AI pipeline.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

[AGPL-3.0](LICENSE)
