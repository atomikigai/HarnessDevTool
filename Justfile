set shell := ["bash", "-cu"]
set dotenv-load := true

# Default: list all recipes
default:
    @just --list

# List recipes
list:
    @just --list

# Run backend + frontend locally in parallel
dev:
    just dev-raw

# Run backend + frontend locally in parallel (no Zellij)
dev-raw:
    #!/usr/bin/env bash
    set -euo pipefail
    source ./scripts/dev-env.sh
    export_harness_dev_env
    cleanup() {
      trap - EXIT INT TERM
      jobs -pr | xargs -r kill 2>/dev/null || true
    }
    trap cleanup EXIT INT TERM
    ./scripts/dev-mcp.sh &
    (cd backend && cargo run -p harness-server) &
    (cd frontend && pnpm dev) &
    wait

# Alias
dev-local: dev-raw

# Run only backend (local)
dev-backend:
    source ./scripts/dev-env.sh; \
    export_harness_dev_env; \
    cd backend && exec cargo run -p harness-server

# Run only frontend (local)
dev-frontend:
    source ./scripts/dev-env.sh; \
    export_harness_dev_env; \
    cd frontend && exec pnpm dev

# Run optional MCP support services in foreground
dev-mcp:
    ./scripts/dev-mcp.sh

# Bootstrap: instala herramientas necesarias y valida el entorno
setup:
    #!/usr/bin/env bash
    set -euo pipefail

    GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
    ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
    warn() { echo -e "  ${YELLOW}!${NC} $1"; }
    fail() { echo -e "  ${RED}✗${NC} $1"; FAILED=1; }

    FAILED=0
    echo ""
    echo "=== harness setup ==="
    echo ""

    # — Herramientas requeridas del sistema —
    echo "Sistema:"
    for tool in git cargo node pnpm; do
      if command -v "$tool" &>/dev/null; then
        ok "$tool $(${tool} --version 2>&1 | head -1)"
      else
        fail "$tool no encontrado — instálalo manualmente"
      fi
    done

    # — Herramientas npm globales —
    echo ""
    echo "Herramientas npm globales:"
    npm_global_install() {
      local pkg="$1"
      local bin="${2:-$1}"
      if command -v "$bin" &>/dev/null; then
        ok "$bin ($(${bin} --version 2>/dev/null | head -1 || echo 'instalado'))"
      else
        echo -e "  ${YELLOW}→${NC} Instalando $pkg..."
        npm install -g "$pkg" 2>&1 | tail -1
        ok "$bin instalado"
      fi
    }
    npm_global_install "opensrc" "opensrc"

    # — CLIs de agentes (opcionales, el harness funciona sin ellos) —
    echo ""
    echo "CLIs de agentes (opcionales):"
    for tool in claude codex; do
      if command -v "$tool" &>/dev/null; then
        ok "$tool"
      else
        warn "$tool no encontrado — necesario para spawnar agentes"
      fi
    done

    # — Dependencias frontend —
    echo ""
    echo "Frontend:"
    if [ -d "frontend/node_modules" ]; then
      ok "node_modules presente"
    else
      echo -e "  ${YELLOW}→${NC} Instalando dependencias frontend..."
      (cd frontend && pnpm install --frozen-lockfile)
      ok "pnpm install completado"
    fi

    # — Validación backend —
    echo ""
    echo "Backend:"
    echo -e "  ${YELLOW}→${NC} cargo check..."
    if (cd backend && cargo check --workspace -q 2>&1); then
      ok "cargo check OK"
    else
      fail "cargo check falló — revisa errores de compilación"
    fi

    echo ""
    if [ "$FAILED" -eq 0 ]; then
      echo -e "${GREEN}Setup completo. Corre 'just dev' para iniciar.${NC}"
    else
      echo -e "${RED}Setup incompleto — resuelve los errores marcados con ✗.${NC}"
      exit 1
    fi
    echo ""

# Build release artifacts (rust + svelte)
build:
    cd backend && cargo build --release
    cd frontend && pnpm build

# Generate TypeScript types via ts-rs and copy into frontend
gen-types:
    cd backend && cargo test --features ts-export --workspace
    mkdir -p frontend/src/lib/api/types
    cp -r backend/bindings/* frontend/src/lib/api/types/
    mkdir -p frontend/src/lib/api/crates
    for dir in backend/crates/*/bindings; do \
      [ -d "$dir" ] || continue; \
      crate="${dir#backend/crates/}"; \
      crate="${crate%/bindings}"; \
      mkdir -p "frontend/src/lib/api/crates/$crate"; \
      cp -r "$dir" "frontend/src/lib/api/crates/$crate/"; \
    done

# Run all tests
test:
    cd backend && cargo test
    cd frontend && pnpm check

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
    #!/usr/bin/env bash
    set -euo pipefail
    source ./scripts/dev-env.sh
    export_harness_dev_env
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
