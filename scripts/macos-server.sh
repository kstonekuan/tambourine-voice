#!/usr/bin/env bash
#
# macos-server.sh — Manage the Tambourine Python server as a macOS launchd service.
#
# Usage: scripts/macos-server.sh {start|stop|restart|status|log|install|uninstall}
#
# Environment variables (optional):
#   HTTP_PROXY, HTTPS_PROXY, ALL_PROXY — forwarded to the server process
#   TAMBOURINE_HOST — server bind address  (default: 127.0.0.1)
#   TAMBOURINE_PORT — server bind port     (default: 8765)

set -euo pipefail

# ── Paths ────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SERVER_DIR="$PROJECT_DIR/server"

SERVICE_LABEL="com.tambourine.voice-server"
PLIST_PATH="$HOME/Library/LaunchAgents/$SERVICE_LABEL.plist"
TEMPLATE_PATH="$SCRIPT_DIR/macos-server.plist.template"

LOG_DIR="$HOME/Library/Logs/Tambourine"
LOG_OUT="$LOG_DIR/server-stdout.log"
LOG_ERR="$LOG_DIR/server-stderr.log"
PID_FILE="$LOG_DIR/server.pid"

HOST="${TAMBOURINE_HOST:-127.0.0.1}"
PORT="${TAMBOURINE_PORT:-8765}"

# ── Detect uv ────────────────────────────────────────────────────────────────
find_uv() {
  if command -v uv &>/dev/null; then
    command -v uv
    return
  fi
  # Common Homebrew / cargo / pipx locations
  for candidate in \
    "$HOME/.cargo/bin/uv" \
    "$HOME/.local/bin/uv" \
    "/opt/homebrew/bin/uv" \
    "/usr/local/bin/uv"; do
    if [[ -x "$candidate" ]]; then
      echo "$candidate"
      return
    fi
  done
  echo "Error: uv not found. Install it: https://docs.astral.sh/uv/getting-started/installation/" >&2
  exit 1
}

UV_PATH="$(find_uv)"

# ── Helpers ──────────────────────────────────────────────────────────────────
ensure_log_dir() {
  mkdir -p "$LOG_DIR"
}

is_running() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid=$(<"$PID_FILE")
    if kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
    rm -f "$PID_FILE"
  fi
  return 1
}

get_pid() {
  if [[ -f "$PID_FILE" ]]; then
    cat "$PID_FILE"
  fi
}

# ── Commands ─────────────────────────────────────────────────────────────────
cmd_start() {
  ensure_log_dir
  if is_running; then
    echo "Server already running (PID $(get_pid))."
    return 0
  fi

  echo "Starting Tambourine server on $HOST:$PORT ..."
  cd "$SERVER_DIR"

  # Build env passthrough for proxy variables
  env_args=()
  for var in HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy; do
    if [[ -n "${!var:-}" ]]; then
      env_args+=("$var=${!var}")
    fi
  done

  if [[ ${#env_args[@]} -gt 0 ]]; then
    env "${env_args[@]}" "$UV_PATH" run python main.py --host "$HOST" --port "$PORT" \
      >>"$LOG_OUT" 2>>"$LOG_ERR" &
  else
    "$UV_PATH" run python main.py --host "$HOST" --port "$PORT" \
      >>"$LOG_OUT" 2>>"$LOG_ERR" &
  fi

  local pid=$!
  echo "$pid" > "$PID_FILE"
  echo "Server started (PID $pid). Logs: $LOG_DIR/"
}

cmd_stop() {
  if ! is_running; then
    echo "Server is not running."
    return 0
  fi

  local pid
  pid="$(get_pid)"
  echo "Stopping server (PID $pid) ..."
  kill "$pid" 2>/dev/null || true

  # Wait up to 5 seconds for graceful shutdown
  for _ in {1..10}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      break
    fi
    sleep 0.5
  done

  # Force kill if still alive
  if kill -0 "$pid" 2>/dev/null; then
    echo "Force-killing PID $pid ..."
    kill -9 "$pid" 2>/dev/null || true
  fi

  rm -f "$PID_FILE"
  echo "Server stopped."
}

cmd_restart() {
  cmd_stop
  cmd_start
}

cmd_status() {
  if is_running; then
    echo "Server is running (PID $(get_pid))."
    # Quick health check
    if command -v curl &>/dev/null; then
      local code
      code=$(curl -s -o /dev/null -w '%{http_code}' "http://$HOST:$PORT/health" 2>/dev/null || echo "000")
      if [[ "$code" == "200" ]]; then
        echo "Health check: OK"
      else
        echo "Health check: FAILED (HTTP $code) — server may still be starting."
      fi
    fi
  else
    echo "Server is not running."
    return 1
  fi
}

cmd_log() {
  if [[ ! -d "$LOG_DIR" ]]; then
    echo "No log directory found at $LOG_DIR"
    return 1
  fi
  echo "=== stdout (last 30 lines) ==="
  tail -30 "$LOG_OUT" 2>/dev/null || echo "(empty)"
  echo ""
  echo "=== stderr (last 30 lines) ==="
  tail -30 "$LOG_ERR" 2>/dev/null || echo "(empty)"
}

cmd_install() {
  ensure_log_dir

  if [[ ! -f "$TEMPLATE_PATH" ]]; then
    echo "Error: plist template not found at $TEMPLATE_PATH"
    exit 1
  fi

  if launchctl list "$SERVICE_LABEL" &>/dev/null; then
    echo "Service already loaded. Unloading first ..."
    launchctl unload "$PLIST_PATH" 2>/dev/null || true
  fi

  echo "Generating plist at $PLIST_PATH ..."
  sed \
    -e "s|{{INSTALL_DIR}}|$PROJECT_DIR|g" \
    -e "s|{{UV_PATH}}|$UV_PATH|g" \
    -e "s|{{LOG_PATH}}|$LOG_DIR|g" \
    "$TEMPLATE_PATH" > "$PLIST_PATH"

  echo "Loading launchd service ..."
  launchctl load "$PLIST_PATH"

  echo "Installed and loaded $SERVICE_LABEL."
  echo "The server will start automatically on login."
  echo "Logs: $LOG_DIR/"
}

cmd_uninstall() {
  if launchctl list "$SERVICE_LABEL" &>/dev/null; then
    echo "Unloading $SERVICE_LABEL ..."
    launchctl unload "$PLIST_PATH" 2>/dev/null || true
  else
    echo "Service is not loaded."
  fi

  if [[ -f "$PLIST_PATH" ]]; then
    echo "Removing $PLIST_PATH ..."
    rm -f "$PLIST_PATH"
  fi

  echo "Uninstalled. Logs at $LOG_DIR/ were preserved."
}

# ── Main ─────────────────────────────────────────────────────────────────────
usage() {
  echo "Usage: $0 {start|stop|restart|status|log|install|uninstall}"
  echo ""
  echo "Commands:"
  echo "  start      Start the server in the background"
  echo "  stop       Stop the server"
  echo "  restart    Restart the server"
  echo "  status     Check if the server is running"
  echo "  log        Show recent server logs"
  echo "  install    Install as a launchd service (auto-start on login)"
  echo "  uninstall  Remove the launchd service"
}

case "${1:-}" in
  start)     cmd_start ;;
  stop)      cmd_stop ;;
  restart)   cmd_restart ;;
  status)    cmd_status ;;
  log)       cmd_log ;;
  install)   cmd_install ;;
  uninstall) cmd_uninstall ;;
  -h|--help) usage ;;
  *)
    usage >&2
    exit 1
    ;;
esac
