<p align="center">
  <img src="app/src-tauri/icons/icon.png" alt="Tambourine" width="128" height="128">
</p>

# Tambourine

Customizable AI powered voice dictation tool. Open-source alternative to [Wispr Flow](https://wisprflow.ai) and [Superwhisper](https://superwhisper.com). Speak and your words are typed wherever your cursor is.

## Why?

**You speak faster than you type.** Typing averages 40-50 wpm, but speaking averages 130-160 wpm. Tambourine lets you write at the speed you think, capturing ideas before they slip away.

**It's easier on your body.** Hours of typing can lead to wrist strain, eye fatigue, and poor posture. Voice dictation gives your hands a break and lets you work from any position—standing, walking, or leaning back in your chair.

**AI handles the cleanup.** Unlike raw transcription, Tambourine uses AI to remove filler words ("um", "uh", "like"), fix grammar, and format your text properly. You speak naturally; the output reads like polished writing.

**It works everywhere.** Voice dictation types directly at your cursor—emails, messages, documents, code, terminal. No copy-pasting or app switching required.

**Why not native dictation?** Built-in dictation is not personalized but Tambourine can be customized to your speaking and writing style, and with a personal dictionary for uncommon terms.

**Why not proprietary tools?** Unlike Wispr Flow or Superwhisper, this project gives you full control:

- **Swap AI providers** — Choose your STT (Cartesia, Deepgram, AssemblyAI, and more) and LLM (Cerebras, OpenAI, Anthropic, and more)
- **Customize processing** — Modify prompts, add custom processors, or chain multiple LLMs
- **Extensible** — Built on [Pipecat](https://github.com/pipecat-ai/pipecat)'s modular pipeline framework

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
- **LLM Text Cleanup** - Removes filler words, fixes grammar using configurable LLM
- **Customizable Prompts** - Edit cleanup rules, enable advanced features, add personal dictionary
- **Automatic Typing** - Pastes cleaned text at cursor position
- **Recording Overlay** - Visual indicator in bottom-right corner during dictation
- **System Tray Integration** - Click to show/hide, right-click menu
- **Transcription History** - View and copy previous dictations
- **Paste Last Transcription** - Re-type previous dictation with `Ctrl+Alt+.`
- **Customizable Hotkeys** - Configure shortcuts to your preference
- **Device Selection** - Choose your preferred microphone
- **Sound Feedback** - Audio cues for recording start/stop
- **Auto-Mute Audio** - Automatically mute system audio while dictating (Windows/macOS)
- **In-App Provider Selection** - Switch STT and LLM providers without restarting

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
│  - LLM cleanup (Cerebras, OpenAI, Anthropic, and more)      │
│  - FastAPI endpoints for config and provider switching      │
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

## Quick Start

### 1. Get API Keys

Sign up and get API keys for the providers you want to use. Some providers with generous free tiers as of this writing:
- Cartesia: https://cartesia.ai (STT)
- Cerebras: https://cloud.cerebras.ai (LLM)
- Gemini: https://aistudio.google.com (LLM)
- Groq: https://console.groq.com (STT/LLM)

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

## App Commands

```bash
cd app

# Development
pnpm dev           # Start Tauri app in dev mode
pnpm dev:vite      # Start Vite dev server only
pnpm lint          # Lint and format code (Biome)
pnpm typecheck     # Run TypeScript type checking
pnpm knip          # Check for unused exports/dependencies
pnpm cargo         # Run Clippy and format Rust code
pnpm check         # Run all checks (lint + typecheck + knip + cargo)

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
- **LLM Cleanup Prompt** - Three customizable sections:
  - Core Cleanup Rules - Filler word removal, grammar, punctuation commands
  - Advanced Features - Backtrack corrections ("scratch that"), list formatting
  - Personal Dictionary - Custom words

## Technology Stack

- **App**: Tauri, Rust, React, TypeScript, Tailwind CSS, Mantine
- **Server**: Python, Pipecat, FastAPI

## Acknowledgments

Built with [Tauri](https://tauri.app/) for the cross-platform desktop app and [Pipecat](https://github.com/pipecat-ai/pipecat) for the modular voice AI pipeline.

## License

[AGPL-3.0](LICENSE)
