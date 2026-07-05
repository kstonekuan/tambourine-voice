# Native macOS Deployment

Run Tambourine as a native macOS app with the Python server managed as a background service.

## Prerequisites

| Tool   | Install                                              |
| ------ | ---------------------------------------------------- |
| Rust   | [rustup.rs](https://rustup.rs)                      |
| Node.js| [nodejs.org](https://nodejs.org) (v18+)             |
| pnpm   | `npm install -g pnpm`                               |
| uv     | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |

## 1. Clone and Configure

```bash
git clone https://github.com/kstonekuan/tambourine-voice.git
cd tambourine-voice

# Set up server environment
cd server
cp .env.example .env
# Edit .env and add your API keys (at least one STT + one LLM provider)

# Install Python dependencies
uv sync
cd ..
```

## 2. Build the App

```bash
cd app
pnpm install
pnpm tauri build
```

The built `.app` bundle will be at:

```
app/src-tauri/target/release/bundle/macos/Tambourine.app
```

## 3. Install the App

Copy the built app to your Applications folder:

```bash
cp -r app/src-tauri/target/release/bundle/macos/Tambourine.app /Applications/
```

On first launch, macOS may show a security prompt. Go to **System Settings > Privacy & Security** and click **Open Anyway** if needed.

You will also need to grant **Accessibility** permission to Tambourine (System Settings > Privacy & Security > Accessibility) for it to type text at your cursor.

## 4. Set Up the Background Server

The server needs to be running for the app to work. You can either run it manually or install it as a launchd service that starts automatically on login.

### Option A: Automatic (Recommended)

```bash
# Install as a login service — starts automatically on login
scripts/macos-server.sh install
```

This generates a launchd plist, loads it, and the server starts immediately. It will also auto-start on future logins.

### Option B: Manual

```bash
# Start the server in the background
scripts/macos-server.sh start

# Check status
scripts/macos-server.sh status
```

### Server Management Commands

```bash
scripts/macos-server.sh start      # Start the server
scripts/macos-server.sh stop       # Stop the server
scripts/macos-server.sh restart    # Restart the server
scripts/macos-server.sh status     # Check if running + health check
scripts/macos-server.sh log        # Show recent logs
scripts/macos-server.sh install    # Install as launchd service
scripts/macos-server.sh uninstall  # Remove launchd service
```

### Proxy Configuration

If you need to route server traffic through a proxy, set environment variables before running install or start:

```bash
export HTTP_PROXY="http://proxy:port"
export HTTPS_PROXY="http://proxy:port"
scripts/macos-server.sh start
```

For the launchd service, you can add proxy variables to the `EnvironmentVariables` section of the generated plist at `~/Library/LaunchAgents/com.tambourine.voice-server.plist`.

## 5. Daily Usage

1. Open **Tambourine** from `/Applications` (or Spotlight).
2. The server runs in the background (if installed as a service).
3. Use the hotkeys to dictate:
   - **Toggle**: `Ctrl+Alt+Space`
   - **Hold**: `` Ctrl+Alt+` ``
4. Text appears at your cursor.

## Troubleshooting

### Server won't start

```bash
# Check logs
scripts/macos-server.sh log

# Verify uv is available
which uv

# Test the server directly
cd server && uv run python main.py
```

### App can't connect to server

1. Check the server is running: `scripts/macos-server.sh status`
2. Verify the health endpoint: `curl http://127.0.0.1:8765/health`
3. Make sure no other process is using port 8765: `lsof -i :8765`

### Text not being typed

Grant Accessibility permission to Tambourine:
**System Settings > Privacy & Security > Accessibility** > Add and enable **Tambourine**.

### Updating

```bash
cd tambourine-voice
git pull

# Rebuild the server dependencies
cd server && uv sync && cd ..

# Rebuild the app
cd app && pnpm install && pnpm tauri build

# Reinstall the app
cp -r app/src-tauri/target/release/bundle/macos/Tambourine.app /Applications/

# Restart the server
scripts/macos-server.sh restart
```

### Uninstalling

```bash
# Remove the launchd service
scripts/macos-server.sh uninstall

# Remove the app
rm -rf /Applications/Tambourine.app

# (Optional) Remove logs
rm -rf ~/Library/Logs/Tambourine
```
