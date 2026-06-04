#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SESSION_NAME="${ZELLIJ_SESSION:-harness-dev}"
LAYOUT_FILE="$ROOT_DIR/scripts/zellij-dev.kdl"
COMPOSE=(docker compose -f "$ROOT_DIR/docker-compose.mcp.yml")
MCP_SERVICES=(crawl4ai excalidraw-mcp)
PRIMARY_MCP_CONTAINERS=(crawl4ai excalidraw-mcp)

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

source "$ROOT_DIR/scripts/dev-env.sh"
export_harness_dev_env

if zellij list-sessions --short --no-formatting 2>/dev/null | grep -Fxq "$SESSION_NAME"; then
  exec zellij attach "$SESSION_NAME"
fi

if ! mcp_services_running; then
  export CRAWL4AI_PORT="${CRAWL4AI_PORT:-$(find_random_free_port "$BACKEND_PORT" "$FRONTEND_PORT")}"
  export EXCALIDRAW_MCP_PORT="${EXCALIDRAW_MCP_PORT:-$(find_random_free_port "$BACKEND_PORT" "$FRONTEND_PORT" "$CRAWL4AI_PORT")}"
fi

zellij attach --create-background "$SESSION_NAME" options \
  --default-layout "$LAYOUT_FILE" \
  --default-cwd "$ROOT_DIR"

exec zellij attach "$SESSION_NAME"
