#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SESSION_NAME="${ZELLIJ_SESSION:-harness-dev}"
LAYOUT_FILE="$ROOT_DIR/scripts/zellij-dev.kdl"
COMPOSE=(docker compose -f "$ROOT_DIR/docker-compose.mcp.yml")
MCP_SERVICES=(crawl4ai excalidraw-mcp)
PRIMARY_MCP_CONTAINERS=(crawl4ai excalidraw-mcp)

find_free_port() {
  python3 - "$1" <<'PY'
import socket
import sys

port = int(sys.argv[1])
while True:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        try:
            sock.bind(("127.0.0.1", port))
        except OSError:
            port += 1
            continue
        print(port)
        break
PY
}

export_default_port() {
  local name="$1"
  local default="$2"

  if [[ -n "${!name:-}" ]]; then
    export "$name"
    return
  fi

  local selected
  selected="$(find_free_port "$default")"
  export "$name=$selected"

  if [[ "$selected" != "$default" ]]; then
    echo "$name default port $default is busy; using $selected for this Zellij session." >&2
  fi
}

mcp_services_running() {
  local container
  local primary_running=true
  for container in "${PRIMARY_MCP_CONTAINERS[@]}"; do
    if ! docker ps --filter "name=^/${container}$" --filter status=running -q | grep -q .; then
      primary_running=false
      break
    fi
  done

  if [[ "$primary_running" == true ]]; then
    return 0
  fi

  local running_services
  running_services="$("${COMPOSE[@]}" ps --services --status running 2>/dev/null || true)"

  local service
  for service in "${MCP_SERVICES[@]}"; do
    if ! grep -Fxq "$service" <<<"$running_services"; then
      return 1
    fi
  done
}

if ! command -v zellij >/dev/null 2>&1; then
  echo "zellij is not installed. Install it first, then run: just dev" >&2
  exit 127
fi

export_default_port BACKEND_PORT 7778
export_default_port FRONTEND_PORT 8081
export HARNESS_BIND="${HARNESS_BIND:-127.0.0.1:${BACKEND_PORT}}"
export HARNESS_CORS_ORIGIN="${HARNESS_CORS_ORIGIN:-http://localhost:${FRONTEND_PORT}}"

if zellij list-sessions --short --no-formatting 2>/dev/null | grep -Fxq "$SESSION_NAME"; then
  exec zellij attach "$SESSION_NAME"
fi

if ! mcp_services_running; then
  export_default_port CRAWL4AI_PORT 11235
  export_default_port EXCALIDRAW_MCP_PORT 3001
fi

zellij attach --create-background "$SESSION_NAME" options \
  --default-layout "$LAYOUT_FILE" \
  --default-cwd "$ROOT_DIR"

exec zellij attach "$SESSION_NAME"
