#!/usr/bin/env bash
set -euo pipefail

# Test local TURN + Tambourine server setup for non-Docker macOS flow.
# Usage:
#   ./test-local.sh                     # uses TURN_SHARED_SECRET from server/.env
#   ./test-local.sh <TURN_SHARED_SECRET> # pass secret explicitly
#
# Optional tuning:
#   TURNUTILS_ALLOW_FORBIDDEN_IP=1  - treat 403 Forbidden IP as non-fatal
#   TURNUTILS_STRICT=1 (default)    - fail on all turnutils_uclient errors
#                                     set to 0 to allow Forbidden IP fallback
script_directory_path="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root_path="$(cd "${script_directory_path}/.." && pwd)"
server_directory_path="${repository_root_path}/server"
server_environment_file_path="${server_directory_path}/.env"

server_http_port="${PORT:-8765}"
turn_server_url="${TURN_SERVER_URL:-turn:127.0.0.1:3478}"
turn_credential_ttl="${TURN_CREDENTIAL_TTL:-3600}"
turn_shared_secret="${TURN_SHARED_SECRET:-}"

print_usage() {
  cat <<'USAGE'
Usage:
  ./test-local.sh [TURN_SHARED_SECRET]

Description:
  Validate local Tambourine + coturn startup health, ICE config response, and
  TURN credential behavior.

Positional:
  TURN_SHARED_SECRET    Optional. Secret used by turnutils_uclient.
                       Falls back to server/.env when omitted.

Environment:
  TURNUTILS_ALLOW_FORBIDDEN_IP=1  Treat 403 (Forbidden IP) as non-fatal.
  TURNUTILS_STRICT=0               Allow Forbidden IP without failing the test.
  TURN_SHARED_SECRET                Used when secret argument is not supplied.
USAGE
}

require_command() {
  local command_name="$1"

  if ! command -v "${command_name}" >/dev/null 2>&1; then
    echo "Missing required command: ${command_name}" >&2
    return 1
  fi
}

load_server_environment() {
  if [ -f "${server_environment_file_path}" ]; then
    set -a
    # shellcheck disable=SC1090
    source "${server_environment_file_path}"
    set +a
  fi

  if [ -n "${PORT:-}" ]; then
    server_http_port="${PORT}"
  fi

  if [ -n "${TURN_SERVER_URL:-}" ]; then
    turn_server_url="${TURN_SERVER_URL}"
  fi

  if [ -n "${TURN_CREDENTIAL_TTL:-}" ]; then
    turn_credential_ttl="${TURN_CREDENTIAL_TTL}"
  fi

  if [ -n "${TURN_SHARED_SECRET:-}" ]; then
    turn_shared_secret="${TURN_SHARED_SECRET}"
  fi
}

parse_secret_argument() {
  if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    print_usage
    exit 0
  fi

  if [ "$#" -gt 1 ]; then
    echo "Usage: ${0} [TURN_SHARED_SECRET]" >&2
    exit 1
  fi

  if [ "$#" -eq 1 ] && [ -n "$1" ]; then
    turn_shared_secret="$1"
  fi

  if [ -n "${turn_shared_secret}" ]; then
    export TURN_SHARED_SECRET="${turn_shared_secret}"
  fi
}

parse_turn_endpoint() {
  local endpoint_without_scheme="$1"
  local host_and_port="${endpoint_without_scheme#*:}"

  local host="${host_and_port%:*}"
  local port="${host_and_port#*:}"

  if [ "${host_and_port}" = "${port}" ]; then
    host="${host_and_port}"
    port="3478"
  fi

  export turn_host="${host}"
  export turn_port="${port}"
}

check_server_health() {
  local base_url="http://127.0.0.1:${server_http_port}"
  if curl -fsS "${base_url}/health" >/dev/null; then
    echo "[PASS] Tambourine server health endpoint is reachable: ${base_url}/health"
    return 0
  fi

  echo "[FAIL] Tambourine server health endpoint is not reachable: ${base_url}/health" >&2
  return 1
}

check_turn_listeners() {
  local udp_line
  local tcp_line
  local udp_ok=false
  local tcp_ok=false

  if lsof -nP -iUDP:"${turn_port}" -a -c "turnserver" >/dev/null 2>&1; then
    udp_ok=true
  fi

  if lsof -nP -iTCP:"${turn_port}" -a -c "turnserver" >/dev/null 2>&1; then
    tcp_ok=true
  fi

  if [ "${udp_ok}" = true ] || [ "${tcp_ok}" = true ]; then
    echo "[PASS] TURN server is listening on ${turn_host}:${turn_port}"
    return 0
  fi

  echo "[FAIL] TURN server not listening on ${turn_host}:${turn_port}" >&2
  return 1
}

check_ice_servers_response() {
  local register_response
  local client_uuid
  local ice_response
  local expected_turn_url
  local turn_url="$1"

  register_response="$(curl -fsS -X POST "http://127.0.0.1:${server_http_port}/api/client/register")"

  if command -v jq >/dev/null 2>&1; then
    client_uuid="$(printf '%s\n' "${register_response}" | jq -r '.uuid')"
  else
    client_uuid="$(printf '%s\n' "${register_response}" | sed -n 's/.*"uuid"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
  fi

  if [ -z "${client_uuid}" ] || [ "${client_uuid}" = "null" ]; then
    echo "[FAIL] Failed to register client for /api/ice-servers test" >&2
    return 1
  fi

  ice_response="$(curl -fsS -H "X-Client-UUID: ${client_uuid}" "http://127.0.0.1:${server_http_port}/api/ice-servers")"

  if ! printf '%s\n' "${ice_response}" | grep -q 'stun:stun.l.google.com:19302'; then
    echo "[FAIL] ICE response missing default STUN entry" >&2
    return 1
  fi

  expected_turn_url="\"urls\":\"${turn_url}\""
  if ! printf '%s\n' "${ice_response}" | grep -q "${expected_turn_url}"; then
    echo "[FAIL] ICE response missing TURN URL: ${turn_url}" >&2
    return 1
  fi

  echo "[PASS] ICE endpoint returns STUN and TURN entry for ${turn_url}"
  return 0
}

check_turnutils_credentials() {
  if [ -z "${TURN_SHARED_SECRET:-}" ]; then
    echo "[SKIP] TURN_SHARED_SECRET not provided; skipping turnutils_uclient auth test."
    return 0
  fi

  if ! command -v turnutils_uclient >/dev/null 2>&1; then
    echo "[SKIP] turnutils_uclient not installed; skipping TURN auth handshake test"
    return 0
  fi

  if ! command -v openssl >/dev/null 2>&1; then
    echo "[FAIL] openssl is required for TURN credential test" >&2
    return 1
  fi

  local unix_timestamp="$(( $(date +%s) + turn_credential_ttl ))"
  local shared_secret="$1"
  local password
  local strict_turnutils_check="${TURNUTILS_STRICT:-1}"
  local allow_forbidden_ip="${TURNUTILS_ALLOW_FORBIDDEN_IP:-0}"
  local temp_output
  local turnutils_status
  local turnutils_log

  password="$(printf '%s' "${unix_timestamp}" | openssl dgst -sha1 -hmac "${shared_secret}" -binary | base64)"

  temp_output="$(mktemp)"
  if turnutils_uclient -n 1 -y -u "${unix_timestamp}" -w "${password}" \
      -p "${turn_port}" "${turn_host}" >"${temp_output}" 2>&1; then
    rm -f "${temp_output}"
    echo "[PASS] turnutils_uclient authentication test succeeded for ${turn_host}"
    return 0
  fi

  turnutils_status=$?
  turnutils_log="$(cat "${temp_output}")"
  rm -f "${temp_output}"

  if [ "${allow_forbidden_ip}" = "1" ] || [ "${allow_forbidden_ip}" = "true" ]; then
    if printf '%s\n' "${turnutils_log}" | grep -q "Forbidden IP"; then
      if [ "${strict_turnutils_check}" = "1" ]; then
        echo "[FAIL] turnutils_uclient authentication test failed for ${turn_host}:${turn_port}" >&2
        printf '%s\n' "${turnutils_log}" >&2
        return 1
      fi

      echo "[PASS] turnutils_uclient failed with expected 403 (Forbidden IP) for this policy; treating as non-fatal."
      echo "[INFO] This is usually expected when no-multicast-peers is enabled."
      return 0
    fi
  fi

  echo "[FAIL] turnutils_uclient authentication test failed for ${turn_host}:${turn_port} (exit code ${turnutils_status})" >&2
  printf '%s\n' "${turnutils_log}" >&2
  echo "[INFO] Running verbose auth check for diagnosis" >&2
  if turnutils_uclient -v -n 1 -y -u "${unix_timestamp}" -w "${password}" \
      -p "${turn_port}" "${turn_host}" > "${temp_output}" 2>&1; then
    rm -f "${temp_output}"
    echo "[PASS] turnutils_uclient authentication test succeeded in verbose mode for ${turn_host}:${turn_port}"
    return 0
  fi

  turnutils_log="$(cat "${temp_output}")"
  rm -f "${temp_output}"

  if [ "${allow_forbidden_ip}" = "1" ] || [ "${allow_forbidden_ip}" = "true" ]; then
    if printf '%s\n' "${turnutils_log}" | grep -q "Forbidden IP"; then
      if [ "${strict_turnutils_check}" = "1" ]; then
        echo "[FAIL] turnutils_uclient authentication test failed for ${turn_host}:${turn_port} (verbose run)." >&2
        printf '%s\n' "${turnutils_log}" >&2
        return 1
      fi

      echo "[PASS] turnutils_uclient verbose run failed with expected 403 (Forbidden IP); treating as non-fatal."
      echo "[INFO] This is usually expected when no-multicast-peers is enabled."
      return 0
    fi
  fi

  echo "[FAIL] turnutils_uclient authentication test failed for ${turn_host}:${turn_port} (verbose run)." >&2
  printf '%s\n' "${turnutils_log}" >&2
  return 1
}

main() {
  require_command curl || exit 1
  require_command lsof || exit 1

  load_server_environment
  parse_secret_argument "$@"
  parse_turn_endpoint "${turn_server_url}"

  check_server_health
  check_turn_listeners
  check_ice_servers_response "${turn_server_url}"
  check_turnutils_credentials "${TURN_SHARED_SECRET:-}"
}

main "$@"
