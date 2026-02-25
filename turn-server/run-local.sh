#!/usr/bin/env bash
set -euo pipefail

# Starts both services for local development:
# - coturn TURN server from turn-server/turnserver.conf
# - Tambourine server (Pipecat-based) from server/main.py
script_directory_path="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root_path="$(cd "${script_directory_path}/.." && pwd)"
server_directory_path="${repository_root_path}/server"
server_environment_file_path="${server_directory_path}/.env"

ensure_required_command_exists() {
  local required_command_name="$1"
  local install_instructions="$2"

  if ! command -v "${required_command_name}" >/dev/null 2>&1; then
    echo "Missing command: ${required_command_name}"
    echo "${install_instructions}"
    exit 1
  fi
}

load_existing_server_environment_file() {
  if [ ! -f "${server_environment_file_path}" ]; then
    echo "Missing ${server_environment_file_path}."
    echo "Create it first (for example: cp server/.env.example server/.env) and set your provider keys."
    exit 1
  fi

  set -a
  # shellcheck disable=SC1090
  source "${server_environment_file_path}"
  set +a
}

validate_turn_environment_configuration() {
  if [ -z "${TURN_SHARED_SECRET:-}" ]; then
    if ! command -v openssl >/dev/null 2>&1; then
      echo "TURN_SHARED_SECRET is missing and OpenSSL is not installed."
      echo "Install OpenSSL or set TURN_SHARED_SECRET explicitly in server/.env."
      exit 1
    fi

    generated_turn_secret="$(openssl rand -hex 32)"
    echo "TURN_SHARED_SECRET was missing. Generated a new value for local runtime use."
    echo "Generated TURN_SHARED_SECRET: ${generated_turn_secret}"
    export TURN_SHARED_SECRET="$generated_turn_secret"
  else
    echo "Using TURN_SHARED_SECRET from server/.env"
  fi

  export TURN_SERVER_URL="${TURN_SERVER_URL:-turn:127.0.0.1:3478}"
  export TURN_CREDENTIAL_TTL="${TURN_CREDENTIAL_TTL:-3600}"

  export TURN_SHARED_SECRET
}

cleanup_turn_server_process() {
  if [ -n "${turn_server_process_id:-}" ] && kill -0 "${turn_server_process_id}" >/dev/null 2>&1; then
    kill "${turn_server_process_id}" >/dev/null 2>&1 || true
  fi
}

main() {
  ensure_required_command_exists "turnserver" "Install with: brew install coturn"
  ensure_required_command_exists "uv" "Install with: brew install uv"

  load_existing_server_environment_file
  validate_turn_environment_configuration
  echo "Active TURN_SHARED_SECRET: ${TURN_SHARED_SECRET}"
  echo "Starting local stack: coturn TURN + Tambourine (Pipecat) server"

  turnserver \
    -c "${script_directory_path}/turnserver.conf" \
    -n \
    --static-auth-secret="${TURN_SHARED_SECRET}" &
  turn_server_process_id=$!
  trap cleanup_turn_server_process EXIT INT TERM

  echo "TURN server started at ${TURN_SERVER_URL}"
  echo "Starting Tambourine server from ${server_directory_path}"

  cd "${server_directory_path}"
  uv sync
  uv run python main.py
}

main "$@"
