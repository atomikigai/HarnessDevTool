set shell := ["bash", "-cu"]
set dotenv-load := true

# Default: list all recipes
default:
    @just --list

# List recipes
list:
    @just --list

# Run the local dev workspace in Zellij
dev:
    ./scripts/dev-zellij.sh

# Run backend + frontend locally in parallel (no Zellij)
dev-raw:
    #!/usr/bin/env bash
    set -euo pipefail
    cleanup() {
      trap - EXIT INT TERM
      docker compose -f docker-compose.mcp.yml down
      jobs -pr | xargs -r kill
    }
    trap cleanup EXIT INT TERM
    docker compose -f docker-compose.mcp.yml up -d
    (cd backend && cargo run -p harness-server) &
    (cd frontend && pnpm dev) &
    wait

# Alias
dev-local: dev-raw

# Run only backend (local)
dev-backend:
    cd backend && exec cargo run -p harness-server

# Run only frontend (local)
dev-frontend:
    cd frontend && exec pnpm dev

# Run optional MCP support services in foreground
dev-mcp:
    ./scripts/dev-mcp.sh

# Build release artifacts (rust + svelte)
build:
    cd backend && cargo build --release
    cd frontend && pnpm build

# Generate TypeScript types via ts-rs and copy into frontend
gen-types:
    cd backend && cargo test --features ts-export --workspace
    mkdir -p frontend/src/lib/api/types
    cp -r backend/bindings/* frontend/src/lib/api/types/

# Run all tests
test:
    cd backend && cargo test
    cd frontend && pnpm test

# Format both stacks
fmt:
    cd backend && cargo fmt
    cd frontend && pnpm format

# Lint both stacks
lint:
    cd backend && cargo clippy --workspace -- -D warnings
    cd frontend && pnpm lint

# Build production images
docker-build:
    docker compose -f docker-compose.yml build

# Bring up production stack (detached)
docker-up:
    docker compose -f docker-compose.yml up -d

# Tear down production stack
docker-down:
    docker compose -f docker-compose.yml down

# Bring up dev stack with hot-reload (foreground)
docker-dev:
    docker compose -f docker-compose.dev.yml up

# Start optional MCP support services without rebuilding
mcp-up:
    docker compose -f docker-compose.mcp.yml up -d

# Rebuild and start optional MCP support services when images change
mcp-build:
    docker compose -f docker-compose.mcp.yml up -d --build

# Stop optional MCP support services
mcp-down:
    docker compose -f docker-compose.mcp.yml down

# Follow optional MCP support service logs
mcp-logs:
    docker compose -f docker-compose.mcp.yml logs -f
