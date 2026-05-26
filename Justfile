set shell := ["bash", "-cu"]
set dotenv-load := true

# Default: list all recipes
default:
    @just --list

# List recipes
list:
    @just --list

# Run backend + frontend locally in parallel (no docker)
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT INT TERM
    (cd backend && cargo run -p harness-server) &
    (cd frontend && pnpm dev) &
    wait

# Alias
dev-local: dev

# Run only backend (local)
dev-backend:
    cd backend && cargo run -p harness-server

# Run only frontend (local)
dev-frontend:
    cd frontend && pnpm dev

# Build release artifacts (rust + svelte)
build:
    cd backend && cargo build --release
    cd frontend && pnpm build

# Generate TypeScript types via ts-rs and copy into frontend
gen-types:
    cd backend && cargo test --features ts-export
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
