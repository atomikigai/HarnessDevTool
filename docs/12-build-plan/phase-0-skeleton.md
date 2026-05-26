---
id: build-plan/phase-0-skeleton
title: F0 — Skeleton (server + shell + persistencia)
shard: 12-build-plan
tags: [phase, f0, skeleton, mvp]
summary: Server arranca, persiste, sirve la UI vacía, todo dockerizado.
related: [build-plan/overview, build-plan/tech-stack-locked, build-plan/repo-layout, build-plan/phase-1-sessions]
sources: []
---

# F0 — Skeleton

## Meta
Tener "los cables" andando: server Axum responde, frontend SvelteKit pinta, persistencia en disco, todo levantable con `just docker-up`. **Cero agentes aún**. Es la base sobre la que F1+ apilan.

## Entregables

### Backend
- [ ] `backend/Cargo.toml` workspace con miembros: `harness-server`, `harness-core`.
- [ ] `harness-server`:
  - [ ] Axum router con middleware `tower-http` (cors, trace, compression, timeout).
  - [ ] `GET /api/health` → `200 { "version": "0.0.1", "uptime_s": N }`.
  - [ ] `GET /api/events` SSE endpoint que emite tick cada 5s (dummy, validación de stream).
  - [ ] `GET /api/threads` → `[]` (lista vacía leída de disco).
  - [ ] `POST /api/threads` → crea thread, persiste `meta.json` + `events.jsonl` vacío, devuelve `{ id }`.
  - [ ] `X-Protocol-Version: 1.0` header en todas las responses.
- [ ] `harness-core`:
  - [ ] Estructuras `Thread`, `Item`, `Event` con `#[derive(TS, Serialize, Deserialize)]`.
  - [ ] Persistencia mínima: `~/.harness/profiles/default/threads/<uuid>/`.
  - [ ] Append-only writer para `events.jsonl`.
  - [ ] `Store::create_thread`, `Store::list_threads`, `Store::get_thread`.
- [ ] Setup `ts-rs`: `cargo test --features ts-export` exporta `.ts` a `backend/bindings/`.
- [ ] Tracing: salida JSON a stderr.

### Frontend
- [ ] `frontend/package.json` con SvelteKit 2.x + Svelte 5 + adapter-node + Tailwind + shadcn-svelte + lucide-svelte.
- [ ] `components.json` configurado (shadcn-svelte init).
- [ ] Añadir `Button`, `Card`, `Sonner` (toast) iniciales.
- [ ] `+layout.svelte`: shell con sidebar fija (entradas placeholder: Threads, Agents, Settings).
- [ ] `+layout.ts`: `export const ssr = false; export const csr = true;`.
- [ ] `+page.svelte` (dashboard): muestra `version` y `uptime_s` del backend.
- [ ] `lib/api/client.ts`: wrapper `fetch` + `subscribeSSE`.
- [ ] `lib/api/types/` poblado por `ts-rs` (gitignored, regenerable).
- [ ] Tema dark por defecto; toggle dark/light usando tokens shadcn.

### Infra
- [ ] `Justfile` con: `dev`, `dev-local`, `dev-backend`, `dev-frontend`, `build`, `gen-types`, `docker-build`, `docker-up`, `docker-down`, `test`, `fmt`, `lint`.
- [ ] `backend/Dockerfile` multi-stage (rust:1-alpine → distroless).
- [ ] `frontend/Dockerfile` multi-stage (node:alpine + pnpm → adapter-node runtime).
- [ ] `docker-compose.yml` (prod) con servicios `backend` (`:7777`) y `frontend` (`:8080`).
- [ ] `docker-compose.dev.yml` con volúmenes de código para hot-reload.
- [ ] `.env.example` documentando `HARNESS_HOME`, `BACKEND_PORT`, `FRONTEND_PORT`.

## Test de aceptación

1. `just docker-build && just docker-up` → ambos containers levantan sin warnings.
2. `curl -s http://localhost:7777/api/health | jq .version` → versión imprimida.
3. `curl -N http://localhost:7777/api/events` → llegan ticks JSON-encoded cada 5s.
4. Browser en `http://localhost:8080` muestra la shell con sidebar y dashboard pintando el health del backend.
5. `POST /api/threads` con `curl` → archivo `~/.harness/profiles/default/threads/<uuid>/meta.json` aparece en disco.
6. `just gen-types` regenera `frontend/src/lib/api/types/` sin diff inesperado.

## Lo que NO está en F0

- No hay sesiones `claude`/`codex` (F1).
- No hay tasks (F2).
- No hay MCP server expuesto (F2).
- No hay módulos verticales (F4).
- No hay tema dinámico complejo, ni atajos de teclado, ni virtual scroll.

## Riesgos / decisiones a confirmar antes de empezar
- ¿`distroless/static-debian12` aguanta el binario static-musl? Verificar con un hello-world Axum **antes** de invertir más.
- ¿`adapter-node` con Svelte 5 ya está estable? Confirmar versión.
- ¿Tracing: stderr vs file por defecto en container? Default stderr; archivos opcionales.

## Estimación rough
2–4 días para un dev solo, sin agentes ayudando. Con agentes orquestados, F0 es ideal para validar pipeline de delivery del equipo.
