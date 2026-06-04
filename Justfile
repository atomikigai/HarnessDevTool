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
    npm_global_install "@ast-grep/cli" "ast-grep"

    # — Herramientas cargo (se instalan si faltan, primera vez puede tardar) —
    echo ""
    echo "Herramientas cargo:"
    cargo_install_if_missing() {
      local crate="$1"
      local bin="${2:-$1}"
      if command -v "$bin" &>/dev/null; then
        ok "$bin ($(${bin} --version 2>/dev/null | head -1 || echo 'instalado'))"
      else
        echo -e "  ${YELLOW}→${NC} Instalando $crate..."
        if command -v cargo-binstall &>/dev/null; then
          cargo binstall -y --quiet "$crate" 2>&1 | tail -1
        else
          cargo install --locked "$crate" --quiet 2>&1 | tail -1
        fi
        ok "$bin instalado"
      fi
    }
    cargo_install_if_missing "cargo-binstall" "cargo-binstall"

    # Core eficiente para agentes.
    cargo_install_if_missing "ripgrep"       "rg"
    cargo_install_if_missing "fd-find"       "fd"
    cargo_install_if_missing "skim"          "sk"
    cargo_install_if_missing "difftastic"    "difft"
    cargo_install_if_missing "hyperfine"     "hyperfine"
    cargo_install_if_missing "tokei"         "tokei"
    cargo_install_if_missing "xh"            "xh"
    cargo_install_if_missing "jaq"           "jaq"
    cargo_install_if_missing "watchexec-cli" "watchexec"
    cargo_install_if_missing "websocat"      "websocat"
    cargo_install_if_missing "pdf_oxide_cli" "pdf-oxide"
    cargo_install_if_missing "sd"            "sd"
    cargo_install_if_missing "typos-cli"     "typos"

    # Rust backend y calidad.
    cargo_install_if_missing "cargo-nextest"     "cargo-nextest"
    cargo_install_if_missing "cargo-audit"       "cargo-audit"
    cargo_install_if_missing "cargo-deny"        "cargo-deny"
    cargo_install_if_missing "cargo-machete"     "cargo-machete"
    cargo_install_if_missing "cargo-bloat"       "cargo-bloat"
    cargo_install_if_missing "cargo-edit"        "cargo-add"
    cargo_install_if_missing "flamegraph"        "cargo-flamegraph"

    # — Herramientas Go / sistema (mejor effort, sin sudo interactivo) —
    echo ""
    echo "Herramientas Go / sistema:"
    tool_version() {
      case "$1" in
        dasel) "$1" version 2>/dev/null | head -1 ;;
        *) "$1" --version 2>/dev/null | head -1 || echo 'instalado' ;;
      esac
    }
    go_install_if_missing() {
      local module="$1"
      local bin="$2"
      if command -v "$bin" &>/dev/null; then
        ok "$bin ($(tool_version "$bin"))"
      elif command -v go &>/dev/null; then
        echo -e "  ${YELLOW}→${NC} Instalando $module..."
        if go install "$module" 2>&1 | tail -1; then
          ok "$bin instalado"
        else
          warn "$bin no se pudo instalar con go install"
        fi
      else
        warn "$bin no encontrado — instala Go o el paquete del sistema"
      fi
    }
    package_tool_if_missing() {
      local bin="$1"
      local pkg="${2:-$1}"
      if command -v "$bin" &>/dev/null; then
        ok "$bin ($(tool_version "$bin"))"
      elif command -v brew &>/dev/null; then
        echo -e "  ${YELLOW}→${NC} Instalando $pkg con brew..."
        if brew install "$pkg" >/dev/null; then
          ok "$bin instalado"
        else
          warn "$bin no se pudo instalar con brew"
        fi
      elif command -v apk &>/dev/null && [ "$(id -u)" -eq 0 ]; then
        echo -e "  ${YELLOW}→${NC} Instalando $pkg con apk..."
        if apk add --no-cache "$pkg" >/dev/null; then
          ok "$bin instalado"
        else
          warn "$bin no se pudo instalar con apk"
        fi
      elif command -v apt-get &>/dev/null && [ "$(id -u)" -eq 0 ]; then
        echo -e "  ${YELLOW}→${NC} Instalando $pkg con apt..."
        if apt-get update -qq >/dev/null && apt-get install -y "$pkg" >/dev/null; then
          ok "$bin instalado"
        else
          warn "$bin no se pudo instalar con apt"
        fi
      else
        warn "$bin no encontrado — instala '$pkg' con tu gestor del sistema"
      fi
    }
    package_tool_if_missing "fzf" "fzf"
    package_tool_if_missing "uv" "uv"
    package_tool_if_missing "duckdb" "duckdb"
    package_tool_if_missing "jq" "jq"
    package_tool_if_missing "lsof" "lsof"
    package_tool_if_missing "strace" "strace"
    package_tool_if_missing "socat" "socat"
    package_tool_if_missing "shellcheck" "shellcheck"
    package_tool_if_missing "hadolint" "hadolint"
    go_install_if_missing "github.com/tomwright/dasel/v2/cmd/dasel@latest" "dasel"
    go_install_if_missing "github.com/gitleaks/gitleaks/v8@latest" "gitleaks"
    go_install_if_missing "github.com/google/osv-scanner/cmd/osv-scanner@latest" "osv-scanner"
    go_install_if_missing "github.com/aquasecurity/trivy/cmd/trivy@latest" "trivy"

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
    if (cd frontend && pnpm exec playwright --version &>/dev/null); then
      ok "playwright $(cd frontend && pnpm exec playwright --version 2>/dev/null | head -1)"
    else
      warn "playwright no disponible — corre 'cd frontend && pnpm install'"
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

# Valida disponibilidad de herramientas del harness y agentes
tools-doctor:
    #!/usr/bin/env bash
    set -euo pipefail
    missing=0
    check_tool() {
      local tool="$1"
      if command -v "$tool" >/dev/null 2>&1; then
        printf "ok      %-18s %s\n" "$tool" "$(command -v "$tool")"
      else
        printf "missing %-18s\n" "$tool"
        missing=1
      fi
    }
    check_optional() {
      local tool="$1"
      if command -v "$tool" >/dev/null 2>&1; then
        printf "ok      %-18s %s\n" "$tool" "$(command -v "$tool")"
      else
        printf "optional %-18s\n" "$tool"
      fi
    }
    tools=(
      git cargo node pnpm
      rg fd difft hyperfine tokei xh jaq jq dasel watchexec websocat sd typos
      uv duckdb lsof strace socat
      cargo-nextest cargo-audit cargo-deny cargo-machete cargo-bloat cargo-add cargo-flamegraph
      gitleaks osv-scanner trivy shellcheck hadolint
      opensrc ast-grep pdf-oxide
    )
    for tool in "${tools[@]}"; do
      check_tool "$tool"
    done
    if command -v fzf >/dev/null 2>&1; then
      printf "ok      %-18s %s\n" "fzf/sk" "$(command -v fzf)"
    elif command -v sk >/dev/null 2>&1; then
      printf "ok      %-18s %s\n" "fzf/sk" "$(command -v sk)"
    else
      printf "missing %-18s\n" "fzf or sk"
      missing=1
    fi
    for tool in claude codex cursor-agent agy; do
      check_optional "$tool"
    done
    if [ -d frontend ] && (cd frontend && pnpm exec playwright --version >/dev/null 2>&1); then
      printf "ok      %-18s %s\n" "playwright" "$(cd frontend && pnpm exec playwright --version)"
    else
      printf "missing %-18s\n" "playwright"
      missing=1
    fi
    if [ "$missing" -eq 0 ]; then
      echo "Toolset completo."
    else
      echo "Faltan herramientas del set potente. Corre 'just setup' o instala las indicadas."
    fi
    exit "$missing"

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

# Run all tests (usa cargo-nextest si está disponible, fallback a cargo test)
test:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v cargo-nextest &>/dev/null; then
        (cd backend && cargo nextest run)
    else
        (cd backend && cargo test)
    fi
    cd frontend && pnpm check

# Install only the Chromium browser needed by frontend Playwright tests
playwright-install:
    cd frontend && pnpm exec playwright install chromium

# Audita vulnerabilidades en dependencias Rust
audit:
    cd backend && cargo audit

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
