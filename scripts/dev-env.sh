#!/usr/bin/env bash

find_random_free_port() {
  python3 - "$@" <<'PY'
import random
import socket
import sys

blocked = {int(p) for p in sys.argv[1:] if p}

def can_bind(host, port):
    family = socket.AF_INET6 if ":" in host else socket.AF_INET
    with socket.socket(family, socket.SOCK_STREAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            sock.bind((host, port))
        except OSError:
            return False
    return True

ports = list(range(42000, 61000))
random.shuffle(ports)

for port in ports:
    if port in blocked:
        continue
    if can_bind("127.0.0.1", port) and can_bind("::1", port):
        print(port)
        raise SystemExit(0)

raise SystemExit("no free port found in 42000-60999")
PY
}

export_harness_dev_env() {
  if [[ -z "${BACKEND_PORT:-}" ]]; then
    BACKEND_PORT="$(find_random_free_port)"
    export BACKEND_PORT
  fi

  if [[ -z "${FRONTEND_PORT:-}" ]]; then
    FRONTEND_PORT="$(find_random_free_port "$BACKEND_PORT")"
    export FRONTEND_PORT
  fi

  export HARNESS_BIND="${HARNESS_BIND:-127.0.0.1:${BACKEND_PORT}}"
  export HARNESS_CORS_ORIGIN="${HARNESS_CORS_ORIGIN:-http://localhost:${FRONTEND_PORT}}"
  export PUBLIC_HARNESS_API_TOKEN="${PUBLIC_HARNESS_API_TOKEN:-${HARNESS_API_TOKEN:-}}"

  echo "Harness dev ports: backend http://${HARNESS_BIND}, frontend http://localhost:${FRONTEND_PORT}" >&2
}
