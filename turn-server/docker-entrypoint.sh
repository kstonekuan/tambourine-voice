#!/usr/bin/env sh
set -eu

if [ -z "${TURN_SHARED_SECRET:-}" ]; then
  echo "TURN_SHARED_SECRET is required but was not set." >&2
  exit 1
fi

if [ -n "${TURN_EXTERNAL_IP:-}" ]; then
  exec turnserver \
    -c /etc/turnserver.conf \
    -n \
    --static-auth-secret="${TURN_SHARED_SECRET}" \
    --external-ip="${TURN_EXTERNAL_IP}"
fi

exec turnserver \
  -c /etc/turnserver.conf \
  -n \
  --static-auth-secret="${TURN_SHARED_SECRET}"
