---
id: app-server/web-deployment
title: harness-server — deployment con Docker
shard: 04-app-server
tags: [deployment, docker, compose]
summary: Dos containers (frontend + backend) en docker-compose, bind-mounts de claude/codex del host.
related: [build-plan/tech-stack-locked, build-plan/phase-0-skeleton, cross-cutting/profiles]
sources: []
---

# Deployment

## Topología

```
                        host (tu máquina)
┌──────────────────────────────────────────────────────────────────────┐
│ Browser → http://localhost:8080 ── frontend container                │
│ Browser → http://localhost:7777 ── backend container (CORS allowed)  │
│                                                                      │
│  ┌─────────────────────────┐    ┌──────────────────────────────────┐ │
│  │ frontend (node:alpine)  │    │ backend (distroless)             │ │
│  │ SvelteKit (adapter-node)│    │ harness-server :7777             │ │
│  │ exposed :8080           │    │ + spawns claude/codex            │ │
│  └─────────────────────────┘    └─────┬────────────────────────────┘ │
│                                       │                              │
│  bind-mounts (host → backend container):                             │
│    /usr/local/bin/claude → /usr/local/bin/claude:ro                  │
│    /usr/local/bin/codex  → /usr/local/bin/codex:ro                   │
│    ~/.harness → /data                                                │
│    (auth de claude/codex queda dentro de /data/profiles/<p>/cli-state/) │
└──────────────────────────────────────────────────────────────────────┘
```

## Archivos

```
HarnessDevTool/
├── docker-compose.yml             # prod
├── docker-compose.dev.yml         # dev override (mounts de código para hot reload)
├── .env.example
├── backend/Dockerfile
└── frontend/Dockerfile
```

## docker-compose.yml (sketch)

```yaml
services:
  backend:
    build: ./backend
    ports:
      - "7777:7777"
    volumes:
      - ${HOME}/.harness:/data
      - /usr/local/bin/claude:/usr/local/bin/claude:ro
      - /usr/local/bin/codex:/usr/local/bin/codex:ro
    environment:
      - RUST_LOG=harness_server=info,tower_http=debug
      - HARNESS_HOME=/data
      - HARNESS_LISTEN=0.0.0.0:7777
    restart: unless-stopped

  frontend:
    build: ./frontend
    ports:
      - "8080:3000"
    environment:
      - PUBLIC_API_BASE_URL=http://localhost:7777
    depends_on:
      - backend
    restart: unless-stopped
```

## Backend Dockerfile (sketch)

```dockerfile
# build stage
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static
WORKDIR /app
COPY backend/ .
RUN cargo build --release --bin harness-server --target x86_64-unknown-linux-musl

# runtime
FROM gcr.io/distroless/static-debian12
COPY --from=build /app/target/x86_64-unknown-linux-musl/release/harness-server /usr/local/bin/harness-server
VOLUME ["/data"]
EXPOSE 7777
ENV HARNESS_HOME=/data HARNESS_LISTEN=0.0.0.0:7777
ENTRYPOINT ["/usr/local/bin/harness-server"]
CMD ["serve"]
```

## Frontend Dockerfile (sketch)

```dockerfile
# build
FROM node:alpine AS build
RUN corepack enable && corepack prepare pnpm@latest --activate
WORKDIR /app
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm build

# runtime
FROM node:alpine
RUN corepack enable && corepack prepare pnpm@latest --activate
WORKDIR /app
COPY --from=build /app/build ./build
COPY --from=build /app/package.json ./
COPY --from=build /app/node_modules ./node_modules
EXPOSE 3000
ENV NODE_ENV=production HOST=0.0.0.0 PORT=3000
CMD ["node", "build"]
```

## Profile switching y bind-mounts

Cuando el usuario cambia profile (`harness profile use <name>`):
- El symlink `~/.harness/active_profile` se actualiza.
- Dentro del container, `/data/active_profile` (symlink) cambia → los procesos leen del nuevo profile.
- **No requiere restart de docker-compose** porque `~/.harness/` está bind-mounted entero; los profiles son subdirs.
- Para auth (`cli-state/`): un symlink interno `/root/.claude` → `/data/profiles/<active>/cli-state/.claude/` se actualiza al cambiar profile (lo hace el backend al recibir la operación).

## Modo dev (sin Docker)

```bash
just dev-backend &      # cargo watch corre harness-server en host
just dev-frontend       # vite dev en :5173, proxy a :7777
```

`vite.config.ts` define el proxy:
```ts
server: {
  proxy: {
    "/api": { target: "http://localhost:7777", changeOrigin: true },
    "/sse": { target: "http://localhost:7777", changeOrigin: true, ws: false },
  }
}
```

## Producción remota (futuro)

Si en algún momento despliegas el harness en una VM:
- Mismo `docker-compose.yml`, expón puertos detrás de un reverse proxy (nginx, Caddy).
- SSE requiere `proxy_buffering off` y `X-Accel-Buffering: no`.
- HTTPS recomendado; el frontend funciona igual.
- Acceso desde tu LAN o vía VPN.

Pero esto **no es el objetivo v1**. Single-user local self-host es el target.

## Anti-patrones

| Mal | Bien |
|---|---|
| Backend en host + frontend en Docker (asimétrico) | Ambos en Docker o ambos en host |
| Bind-mountear `~/.claude` directo al container | Aislar por profile en `cli-state/` |
| Exponer `:7777` al internet | Solo localhost o LAN privada |
| Sin volumes para `/data` | Persistencia perdida al `docker compose down -v` |
