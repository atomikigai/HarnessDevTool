#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE=(docker compose -f "$ROOT_DIR/docker-compose.mcp.yml")
SERVICES=(crawl4ai excalidraw-mcp)
PRIMARY_CONTAINERS=(crawl4ai excalidraw-mcp)

if [[ "${HARNESS_MCP_PDF_OXIDE_BUILD:-0}" == "1" ]]; then
  echo "Building optional pdf-oxide stdio MCP image."
  "${COMPOSE[@]}" --profile stdio build pdf-oxide-mcp
fi

primary_containers_running() {
  local container
  for container in "${PRIMARY_CONTAINERS[@]}"; do
    if ! docker ps --filter "name=^/${container}$" --filter status=running -q | grep -q .; then
      return 1
    fi
  done
}

running_services="$("${COMPOSE[@]}" ps --services --status running 2>/dev/null || true)"

all_running=true
for service in "${SERVICES[@]}"; do
  if ! grep -Fxq "$service" <<<"$running_services"; then
    all_running=false
    break
  fi
done

if [[ "$all_running" == true ]]; then
  echo "MCP containers are already running; following logs without rebuild."
  exec "${COMPOSE[@]}" logs -f "${SERVICES[@]}"
fi

if primary_containers_running; then
  echo "Primary MCP containers are already running; following logs without starting harness duplicates."
  docker logs -f crawl4ai &
  docker logs -f excalidraw-mcp &
  wait
  exit $?
fi

echo "Starting MCP containers without rebuild. Run 'just mcp-build' when image changes need a rebuild."
exec "${COMPOSE[@]}" up
